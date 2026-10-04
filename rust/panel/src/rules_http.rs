//! Authenticated draft read/save/preview only. No runtime, capture or compiler owner.
//!
//! A service keeps one state. Source rules are borrowed during serialization;
//! only the draft snapshot and one effective preview are owned per request.
use crate::http::{self, Method};
use crate::policy::{
    Diagnostic, EffectivePolicy, LocalRule, MAX_LOCAL_RULES, MAX_SUBSCRIPTION_EDITS, Policy,
    PreparedSubscription, Rule, RuleKind, SubscriptionEdit, Target, validate_policy,
};
use crate::policy_store::{SaveOutcome, Snapshot, Store, StoreError};
use crate::subscription::{PolicySummary, Subscription, parse_clash_yaml, summarize_policy};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de, ser::SerializeSeq};
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Component, Path, PathBuf};

pub const MAX_SUBSCRIPTION_BYTES: usize = 2 << 20;
pub const MAX_RESPONSE_BYTES: usize = 8 << 20;
const JSON_TYPE: &str = "application/json; charset=utf-8";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RulesError;
impl fmt::Display for RulesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("local rule feature unavailable")
    }
}
impl std::error::Error for RulesError {}

pub struct RulesState {
    store: Store,
    subscription: PreparedSubscription,
    nodes: Vec<crate::native::Node>,
    diagnostics: Vec<Diagnostic>,
    summary: PolicySummary,
    data_dir: PathBuf,
}
impl fmt::Debug for RulesState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RulesState (private)")
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ApplyInput {
    revision: String,
    generation: u64,
    #[serde(default)]
    acknowledged_revision: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ApplyResponse {
    status: crate::runtime_http::WireStatus,
    draft_revision: String,
    #[serde(rename = "configSHA256")]
    config_sha256: String,
    applied: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ApplyUncertain {
    error: ApiError<'static>,
    status: crate::runtime_http::WireStatus,
    draft_revision: String,
    #[serde(rename = "configSHA256")]
    config_sha256: String,
    applied: bool,
}
impl RulesState {
    /// Reads only subscription.yaml and the independent draft. Missing source
    /// means no subscription; corrupt/unsafe existing source fails closed.
    /// Store::open handles its own private directory/draft. No save or Apply.
    pub fn open(data_dir: impl AsRef<Path>) -> Result<Self, RulesError> {
        let subscription = read_subscription(data_dir.as_ref())?;
        let summary = summarize_policy(&subscription);
        let prepared = PreparedSubscription::new(subscription.rules).map_err(|_| RulesError)?;
        let store = Store::open(data_dir.as_ref()).map_err(|_| RulesError)?;
        let data_dir = data_dir.as_ref().canonicalize().map_err(|_| RulesError)?;
        Ok(Self {
            store,
            subscription: prepared,
            nodes: subscription.nodes,
            diagnostics: subscription.diagnostics,
            summary,
            data_dir,
        })
    }
    fn preview(&self, policy: &Policy) -> Result<EffectivePolicy, RulesError> {
        let mut preview = self.subscription.merge(policy).map_err(|_| RulesError)?;
        preview.diagnostics.extend(
            self.summary
                .omitted_rules
                .iter()
                .map(|omission| Diagnostic {
                    scope: "subscription".into(),
                    index: omission.index,
                    code: omission.code.clone(),
                    message: omission.message.clone(),
                }),
        );
        Ok(preview)
    }
    pub(crate) fn respond(
        &mut self,
        writer: &mut impl Write,
        path: &str,
        method: Method,
        body: &[u8],
        runtime: Option<&mut crate::runtime_http::RuntimeHttp>,
    ) -> io::Result<()> {
        let head = method == Method::Head;
        match (path, method) {
            ("/api/proxy/nodes", Method::Get | Method::Head) => {
                let response = NodesResponse {
                    nodes: PublicNodes(&self.nodes),
                    diagnostics: &self.diagnostics,
                    selected_node_id: "",
                    revision: &self.summary.revision,
                    policy_summary: &self.summary,
                };
                write_json(writer, 200, "OK", &response, head)
            }
            ("/api/proxy/local-rules", Method::Get | Method::Head) => {
                self.readback(writer, head, runtime)
            }
            ("/api/proxy/local-rules/apply", Method::Post) => {
                let Some(runtime) = runtime else {
                    return runtime_unavailable(writer, false);
                };
                self.apply_saved(writer, body, runtime)
            }
            ("/api/proxy/local-rules" | "/api/proxy/local-rules/preview", Method::Post) => {
                let policy = match decode_policy(body) {
                    Ok(policy) => policy,
                    Err(InputError::Json) => {
                        return error(
                            writer,
                            400,
                            "Bad Request",
                            "invalid_json",
                            "JSON fields or types do not match the request contract.",
                            false,
                        );
                    }
                    Err(InputError::Policy) => {
                        return error(
                            writer,
                            422,
                            "Unprocessable Entity",
                            "local_rules_invalid",
                            "Local rule fields, matchers or counts are invalid.",
                            false,
                        );
                    }
                };
                if path.ends_with("/preview") {
                    return match self.preview(&policy) {
                        Ok(preview) => write_json(writer, 200, "OK", &preview, false),
                        Err(_) => unavailable(writer, false),
                    };
                }
                // Refuse an unrepresentable candidate before mutating storage.
                // The effective preview is dropped before Store::save fsync.
                let candidate = Snapshot {
                    revision: crate::policy::policy_revision(&policy)
                        .map_err(|_| io::Error::other("validated policy unavailable"))?,
                    policy,
                };
                if !self.readback_fits(&candidate) {
                    return response_too_large(writer, false);
                }
                let saved = self.store.save(&candidate.policy);
                drop(candidate);
                match saved {
                    Ok(outcome) if outcome.durability_error.is_some() => {
                        write_save_uncertain(writer, &outcome)
                    }
                    Ok(_) => self.readback(writer, false, runtime),
                    Err(failure) => write_save_failed(writer, failure, &self.store.snapshot()),
                }
            }
            _ => method_not_allowed(
                writer,
                head,
                if path == "/api/proxy/local-rules" {
                    "GET, HEAD, POST"
                } else if path == "/api/proxy/nodes" {
                    "GET, HEAD"
                } else {
                    "POST"
                },
            ),
        }
    }
    fn runtime_evidence(
        &self,
        runtime: Option<&mut crate::runtime_http::RuntimeHttp>,
    ) -> (Applied, u64) {
        use sha2::{Digest, Sha256};
        let Some(runtime) = runtime else {
            return (Applied::unknown(), 0);
        };
        let Ok(status) = runtime.rule_status() else {
            return (Applied::unknown(), 0);
        };
        let Ok(Some(config)) = runtime.rule_config() else {
            return (Applied::unknown(), status.generation);
        };
        let generation = config.identity.generation;
        if status.generation != generation
            || !status.ready
            || !status.running_matches_accepted
            || !status.desired
            || status.resource_suspended
            || status.durability_uncertain
            || status.needs_recovery
        {
            return (Applied::unknown(), generation);
        }
        let Some(manifest) = crate::rule_apply::read_manifest(&self.data_dir) else {
            return (Applied::unknown(), generation);
        };
        if manifest.generation != generation
            || manifest.native_sha256 != config.identity.sha256
            || manifest.native_sha256 != format!("{:x}", Sha256::digest(config.bytes()))
        {
            return (Applied::unknown(), generation);
        }
        (
            Applied {
                state: "known",
                revision: Some(manifest.revision),
                generation: Some(generation),
            },
            generation,
        )
    }
    fn apply_saved(
        &mut self,
        writer: &mut impl Write,
        body: &[u8],
        runtime: &mut crate::runtime_http::RuntimeHttp,
    ) -> io::Result<()> {
        use sha2::{Digest, Sha256};
        if body.len() > 64 << 10
            || body
                .iter()
                .find(|byte| !byte.is_ascii_whitespace())
                .is_none_or(|byte| *byte != b'{')
        {
            return error(
                writer,
                400,
                "Bad Request",
                "invalid_json",
                "Apply fields or types do not match the contract.",
                false,
            );
        }
        let input: ApplyInput = match serde_json::from_slice(body) {
            Ok(value) => value,
            Err(_) => {
                return error(
                    writer,
                    400,
                    "Bad Request",
                    "invalid_json",
                    "Apply fields or types do not match the contract.",
                    false,
                );
            }
        };
        let draft = self.store.snapshot();
        if input.revision != draft.revision {
            return error(
                writer,
                409,
                "Conflict",
                "local_rules_revision_changed",
                "Local draft changed; read it again.",
                false,
            );
        }
        if !input.acknowledged_revision.is_empty()
            && input.acknowledged_revision != self.summary.revision
        {
            return error(
                writer,
                409,
                "Conflict",
                "policy_revision_changed",
                "Subscription policy changed; review omissions again.",
                false,
            );
        }
        if self.summary.omitted > 0 && input.acknowledged_revision.is_empty() {
            return error(
                writer,
                409,
                "Conflict",
                "policy_acknowledgment_required",
                "Review and acknowledge omitted subscription rules.",
                false,
            );
        }
        let before = match runtime.rule_status() {
            Ok(value) => value,
            Err(failed) => return runtime.write_failure(writer, failed),
        };
        if before.generation != input.generation {
            return error(
                writer,
                409,
                "Conflict",
                "generation_conflict",
                "Runtime configuration changed; read it again.",
                false,
            );
        }
        if !before.desired
            || !before.ready
            || !before.running_matches_accepted
            || before.resource_suspended
        {
            return error(
                writer,
                409,
                "Conflict",
                "runtime_not_ready",
                "Start and verify the owned runtime before applying rules.",
                false,
            );
        }
        if before.durability_uncertain || before.needs_recovery {
            return error(
                writer,
                409,
                "Conflict",
                "runtime_recovery_pending",
                "Recover the runtime before applying new rules.",
                false,
            );
        }
        let accepted = match runtime.rule_config() {
            Ok(Some(value)) => value,
            Ok(None) => {
                return error(
                    writer,
                    409,
                    "Conflict",
                    "not_configured",
                    "Service is not configured.",
                    false,
                );
            }
            Err(failed) => return runtime.write_failure(writer, failed),
        };
        if accepted.identity.generation != input.generation {
            return error(
                writer,
                409,
                "Conflict",
                "generation_conflict",
                "Runtime configuration changed; read it again.",
                false,
            );
        }
        let refs = match crate::rule_apply::read_verified_refs(&self.data_dir) {
            Ok(value) => value,
            Err(_) => {
                return error(
                    writer,
                    409,
                    "Conflict",
                    "rules_unavailable",
                    "Pinned rule sets are unavailable or changed.",
                    false,
                );
            }
        };
        let effective = match self.subscription.merge(&draft.policy) {
            Ok(value) => value,
            Err(_) => {
                return error(
                    writer,
                    422,
                    "Unprocessable Entity",
                    "local_rules_invalid",
                    "Local policy cannot be merged.",
                    false,
                );
            }
        };
        let output = match crate::rule_apply::compile_preserving(
            accepted.bytes(),
            &self.nodes,
            effective.rules,
            self.diagnostics.clone(),
            refs,
            input.acknowledged_revision == self.summary.revision,
        ) {
            Ok(value) => value,
            Err(_) => {
                return error(
                    writer,
                    409,
                    "Conflict",
                    "proxy_configuration_invalid",
                    "Accepted proxy settings or selected node cannot be preserved.",
                    false,
                );
            }
        };
        // Source provenance is not needed during checker/readiness/storage. Do
        // not retain a second effective preview graph across the operation.
        drop(effective.provenance);
        drop(effective.diagnostics);
        drop(accepted);
        let status = match runtime.configure_rules(input.generation, &output.config) {
            Ok(value) => value,
            Err(failed) => return runtime.write_failure(writer, failed),
        };
        let readback = match runtime.rule_config() {
            Ok(Some(value)) => value,
            _ => {
                return write_json(
                    writer,
                    500,
                    "Internal Server Error",
                    &ApplyUncertain {
                        error: ApiError {
                            code: "local_rules_readback_failed",
                            message: "Runtime accepted state is unconfirmed; read its status.",
                        },
                        status: status.into(),
                        draft_revision: draft.revision,
                        config_sha256: output.sha256,
                        applied: false,
                    },
                    false,
                );
            }
        };
        if readback.identity.generation != status.generation
            || readback.bytes() != output.config
            || format!("{:x}", Sha256::digest(readback.bytes())) != output.sha256
            || status.durability_uncertain
            || status.needs_recovery
            || !status.desired
            || !status.ready
            || !status.running_matches_accepted
            || status.resource_suspended
        {
            return write_json(
                writer,
                500,
                "Internal Server Error",
                &ApplyUncertain {
                    error: ApiError {
                        code: "local_rules_readback_failed",
                        message: "Runtime accepted state is unconfirmed; read its status.",
                    },
                    status: status.into(),
                    draft_revision: draft.revision,
                    config_sha256: output.sha256,
                    applied: false,
                },
                false,
            );
        }
        drop(readback);
        let manifest = crate::rule_apply::AppliedManifest {
            revision: draft.revision.clone(),
            native_sha256: output.sha256.clone(),
            generation: status.generation,
        };
        if crate::rule_apply::write_manifest(&self.data_dir, &manifest).is_err() {
            return write_json(
                writer,
                500,
                "Internal Server Error",
                &ApplyUncertain {
                    error: ApiError {
                        code: "storage_failed",
                        message: "Runtime changed, but applied-rule evidence could not be durably saved.",
                    },
                    status: status.into(),
                    draft_revision: draft.revision,
                    config_sha256: output.sha256,
                    applied: false,
                },
                false,
            );
        }
        write_json(
            writer,
            200,
            "OK",
            &ApplyResponse {
                status: status.into(),
                draft_revision: draft.revision,
                config_sha256: output.sha256,
                applied: true,
            },
            false,
        )
    }
    fn response<'a>(
        &'a self,
        draft: &'a Snapshot,
        preview: &'a EffectivePolicy,
        applied: Applied,
        runtime_generation: u64,
    ) -> Readback<'a> {
        Readback {
            draft: Draft(draft),
            subscription_revision: &self.summary.revision,
            subscription_rules: SubscriptionRules {
                rules: self.subscription.rules(),
                fingerprints: self.subscription.fingerprints(),
            },
            preview,
            applied,
            runtime_generation,
            policy_summary: &self.summary,
        }
    }
    fn readback_fits(&self, draft: &Snapshot) -> bool {
        self.preview(&draft.policy).is_ok_and(|preview| {
            json_length(&self.response(draft, &preview, Applied::unknown(), 0)).is_ok()
        })
    }
    fn readback(
        &self,
        writer: &mut impl Write,
        head: bool,
        runtime: Option<&mut crate::runtime_http::RuntimeHttp>,
    ) -> io::Result<()> {
        // Fresh accepted Store snapshot; never a stale pre-save response.
        let draft = self.store.snapshot();
        let preview = match self.preview(&draft.policy) {
            Ok(preview) => preview,
            Err(_) => return unavailable(writer, head),
        };
        let (applied, generation) = self.runtime_evidence(runtime);
        write_json(
            writer,
            200,
            "OK",
            &self.response(&draft, &preview, applied, generation),
            head,
        )
    }
}

fn read_subscription(data_dir: &Path) -> Result<Subscription, RulesError> {
    if data_dir.as_os_str().is_empty() {
        return Err(RulesError);
    }
    let absolute = if data_dir.is_absolute() {
        data_dir.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|_| RulesError)?
            .join(data_dir)
    };
    let normalized: PathBuf = absolute
        .components()
        .filter(|part| *part != Component::CurDir)
        .collect();
    let directory = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(normalized)
    {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Subscription::default()),
        Err(_) => return Err(RulesError),
    };
    // SAFETY: live directory fd, fixed terminated name, and ownership moves to File.
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            c"subscription.yaml".as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return if io::Error::last_os_error().kind() == io::ErrorKind::NotFound {
            Ok(Subscription::default())
        } else {
            Err(RulesError)
        };
    }
    let mut file = unsafe { File::from_raw_fd(fd) };
    let metadata = file.metadata().map_err(|_| RulesError)?;
    if !metadata.is_file() || metadata.len() > MAX_SUBSCRIPTION_BYTES as u64 {
        return Err(RulesError);
    }
    // Metadata is bounded above; reserve only the actual source plus one
    // growth sentinel. Never initialize 2 MiB for a tiny source file.
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(metadata.len() as usize + 1)
        .map_err(|_| RulesError)?;
    let mut chunk = [0_u8; 32 << 10];
    while bytes.len() <= MAX_SUBSCRIPTION_BYTES {
        let remaining = (MAX_SUBSCRIPTION_BYTES + 1 - bytes.len()).min(chunk.len());
        let count = match file.read(&mut chunk[..remaining]) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(RulesError),
        };
        let length = bytes.len() + count;
        if length > bytes.capacity() {
            let capacity = length
                .max(bytes.capacity().saturating_mul(2))
                .min(MAX_SUBSCRIPTION_BYTES + 1);
            bytes
                .try_reserve_exact(capacity - bytes.len())
                .map_err(|_| RulesError)?;
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    if bytes.len() > MAX_SUBSCRIPTION_BYTES {
        return Err(RulesError);
    }
    parse_clash_yaml(&bytes).map_err(|_| RulesError)
}

// Typed decoding rejects duplicate and unknown fields at every known object.
// No JSON Value cache or second whole-body object graph is constructed.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    #[serde(deserialize_with = "object")]
    policy: StrictPolicy,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StrictPolicy {
    #[serde(deserialize_with = "locals")]
    rules: Vec<StrictLocal>,
    #[serde(deserialize_with = "edits")]
    subscription_edits: Vec<StrictEdit>,
}
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct StrictLocal {
    id: String,
    enabled: bool,
    label: String,
    note: String,
    #[serde(deserialize_with = "object")]
    rule: StrictRule,
}
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
struct StrictRule {
    kind: RuleKind,
    value: String,
    target: Target,
    no_resolve: bool,
    index: i64,
}
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
struct StrictEdit {
    id: String,
    source_fingerprint: String,
    disabled: bool,
    #[serde(deserialize_with = "replacement")]
    replacement: Option<StrictRule>,
    label: String,
    note: String,
}
fn replacement<'de, D: Deserializer<'de>>(d: D) -> Result<Option<StrictRule>, D::Error> {
    object::<D, StrictRule>(d).map(Some)
}
// serde's struct decoder also accepts positional sequences. HTTP contracts
// require objects at every struct position, so route them through a map-only
// visitor before the generated exact-field/duplicate-field implementation.
fn object<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<T, D::Error> {
    struct ObjectVisitor<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> de::Visitor<'de> for ObjectVisitor<T> {
        type Value = T;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a JSON object")
        }
        fn visit_map<A: de::MapAccess<'de>>(self, map: A) -> Result<T, A::Error> {
            T::deserialize(de::value::MapAccessDeserializer::new(map))
        }
    }
    d.deserialize_map(ObjectVisitor::<T>(std::marker::PhantomData))
}
struct Object<T>(T);
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Object<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        object(d).map(Self)
    }
}
fn bounded_array<'de, D: Deserializer<'de>, T: Deserialize<'de>, const LIMIT: usize>(
    d: D,
) -> Result<Vec<T>, D::Error> {
    struct Array<T, const N: usize>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const N: usize> de::Visitor<'de> for Array<T, N> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a bounded policy array")
        }
        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut out = Vec::new();
            while out.len() < N {
                match seq.next_element::<Object<T>>()? {
                    Some(Object(value)) => out.push(value),
                    None => return Ok(out),
                }
            }
            if seq.next_element::<de::IgnoredAny>()?.is_some() {
                return Err(de::Error::custom("policy collection limit"));
            }
            Ok(out)
        }
    }
    d.deserialize_seq(Array::<T, LIMIT>(std::marker::PhantomData))
}
fn locals<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<StrictLocal>, D::Error> {
    bounded_array::<D, StrictLocal, MAX_LOCAL_RULES>(d)
}
fn edits<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<StrictEdit>, D::Error> {
    bounded_array::<D, StrictEdit, MAX_SUBSCRIPTION_EDITS>(d)
}
impl From<StrictRule> for Rule {
    fn from(rule: StrictRule) -> Self {
        Self {
            kind: rule.kind,
            value: rule.value,
            target: rule.target,
            no_resolve: rule.no_resolve,
            index: rule.index,
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
enum InputError {
    Json,
    Policy,
}
fn input_bounds(body: &[u8]) -> Result<(), InputError> {
    if body.len() > crate::http::MAX_RULES_BODY_BYTES {
        return Err(InputError::Json);
    }
    let (mut depth, mut string, mut escaped, mut scalar, mut token) =
        (0_usize, false, false, 0_usize, 0_usize);
    for &byte in body {
        if string {
            scalar += 1;
            // Bounded scalar/token scan before typed String allocation. The
            // semantic field limits below remain stricter than this hard cap.
            if scalar > 8192 {
                return Err(InputError::Json);
            }
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                string = false;
            }
        } else {
            if matches!(byte, b'"' | b'{' | b'[' | b'}' | b']' | b',' | b':')
                || byte.is_ascii_whitespace()
            {
                token = 0;
            } else {
                token += 1;
                if token > 64 {
                    return Err(InputError::Json);
                }
            }
            match byte {
                b'"' => {
                    string = true;
                    scalar = 0;
                }
                b'{' | b'[' => {
                    depth += 1;
                    if depth > 32 {
                        return Err(InputError::Json);
                    }
                }
                b'}' | b']' => depth = depth.checked_sub(1).ok_or(InputError::Json)?,
                _ => (),
            }
        }
    }
    if string || depth != 0 {
        return Err(InputError::Json);
    }
    Ok(())
}
fn decode_policy(body: &[u8]) -> Result<Policy, InputError> {
    input_bounds(body)?;
    let mut decoder = serde_json::Deserializer::from_slice(body);
    let input: Input = object(&mut decoder).map_err(|_| InputError::Json)?;
    decoder.end().map_err(|_| InputError::Json)?;
    let policy = Policy {
        rules: input
            .policy
            .rules
            .into_iter()
            .map(|local| LocalRule {
                id: local.id,
                enabled: local.enabled,
                label: local.label,
                note: local.note,
                rule: local.rule.into(),
            })
            .collect(),
        subscription_edits: input
            .policy
            .subscription_edits
            .into_iter()
            .map(|edit| SubscriptionEdit {
                id: edit.id,
                source_fingerprint: edit.source_fingerprint,
                disabled: edit.disabled,
                replacement: edit.replacement.map(Into::into),
                label: edit.label,
                note: edit.note,
            })
            .collect(),
    };
    validate_policy(&policy).map_err(|_| InputError::Policy)?;
    Ok(policy)
}

struct Draft<'a>(&'a Snapshot);
impl Serialize for Draft<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Fields<'a> {
            policy: &'a Policy,
            revision: &'a str,
        }
        Fields {
            policy: &self.0.policy,
            revision: &self.0.revision,
        }
        .serialize(serializer)
    }
}
struct SubscriptionRules<'a> {
    rules: &'a [Rule],
    fingerprints: &'a [String],
}
impl Serialize for SubscriptionRules<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Source<'a> {
            fingerprint: &'a str,
            rule: &'a Rule,
        }
        let mut sequence = serializer.serialize_seq(Some(self.rules.len()))?;
        for (rule, fingerprint) in self.rules.iter().zip(self.fingerprints) {
            sequence.serialize_element(&Source { fingerprint, rule })?;
        }
        sequence.end()
    }
}
struct PublicNodes<'a>(&'a [crate::native::Node]);
impl Serialize for PublicNodes<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Same public projection as Subscription::public_nodes(), borrowing the
        // parsed source instead of cloning its private nodes or public strings.
        #[derive(Serialize)]
        struct Public<'a> {
            id: &'a str,
            label: &'a str,
            server: &'a str,
            port: u16,
            protocol: &'a str,
            transport: &'a str,
            reality: bool,
            vision: bool,
            utls: bool,
            udp: bool,
        }
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for node in self.0 {
            sequence.serialize_element(&Public {
                id: &node.id,
                label: &node.name,
                server: &node.server,
                port: node.port,
                protocol: "vless",
                transport: "tcp",
                reality: true,
                vision: true,
                utls: true,
                udp: node.udp,
            })?;
        }
        sequence.end()
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NodesResponse<'a> {
    nodes: PublicNodes<'a>,
    diagnostics: &'a [Diagnostic],
    selected_node_id: &'a str,
    revision: &'a str,
    policy_summary: &'a PolicySummary,
}
#[derive(Serialize)]
struct Applied {
    state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    revision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation: Option<u64>,
}
impl Applied {
    fn unknown() -> Self {
        Self {
            state: "unknown",
            revision: None,
            generation: None,
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Readback<'a> {
    draft: Draft<'a>,
    subscription_revision: &'a str,
    subscription_rules: SubscriptionRules<'a>,
    preview: &'a EffectivePolicy,
    applied: Applied,
    runtime_generation: u64,
    policy_summary: &'a PolicySummary,
}
#[derive(Serialize)]
struct ApiError<'a> {
    code: &'a str,
    message: &'a str,
}
#[derive(Serialize)]
struct ErrorResponse<'a> {
    error: ApiError<'a>,
}
#[derive(Serialize)]
struct SaveErrorResponse<'a> {
    error: ApiError<'a>,
    draft: Draft<'a>,
    committed: bool,
}
fn write_save_failed(
    writer: &mut impl Write,
    failure: StoreError,
    draft: &Snapshot,
) -> io::Result<()> {
    let (status, reason, code, message) = match failure {
        StoreError::InsufficientSpace | StoreError::Measurement => (
            409,
            "Conflict",
            "storage_insufficient",
            "Storage space is insufficient or unavailable; draft save was not committed.",
        ),
        _ => (
            500,
            "Internal Server Error",
            "storage_failed",
            "Draft save was not committed; read the draft again.",
        ),
    };
    write_json(
        writer,
        status,
        reason,
        &SaveErrorResponse {
            error: ApiError { code, message },
            draft: Draft(draft),
            committed: false,
        },
        false,
    )
}
fn write_save_uncertain(writer: &mut impl Write, outcome: &SaveOutcome) -> io::Result<()> {
    write_json(
        writer,
        500,
        "Internal Server Error",
        &SaveErrorResponse {
            error: ApiError {
                code: "storage_failed",
                message: "Draft was committed, but storage durability is unconfirmed; read the draft again.",
            },
            draft: Draft(&outcome.snapshot),
            committed: outcome.committed,
        },
        false,
    )
}
struct Counter {
    length: usize,
}
impl Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.length = self
            .length
            .checked_add(bytes.len())
            .filter(|length| *length <= MAX_RESPONSE_BYTES)
            .ok_or_else(|| io::Error::other("response limit"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn json_length(value: &impl Serialize) -> Result<usize, serde_json::Error> {
    let mut counter = Counter { length: 0 };
    serde_json::to_writer(&mut counter, value)?;
    // Every response includes precisely one trailing newline.
    counter.write_all(b"\n").map_err(serde_json::Error::io)?;
    Ok(counter.length)
}
fn write_json(
    writer: &mut impl Write,
    status: u16,
    reason: &str,
    value: &impl Serialize,
    head: bool,
) -> io::Result<()> {
    let length = match json_length(value) {
        Ok(length) => length,
        Err(_) => return response_too_large(writer, head),
    };
    // Fixed-size transport buffer under the original DeadlineWriter budget.
    // Both headers and body use it, and counting finished before any output.
    let mut buffered = io::BufWriter::with_capacity(8 << 10, writer);
    http::write_headers(&mut buffered, status, reason, JSON_TYPE, length as u64)?;
    if !head {
        serde_json::to_writer(&mut buffered, value).map_err(io::Error::other)?;
        buffered.write_all(b"\n")?;
    }
    buffered.flush()?;
    Ok(())
}
pub(crate) fn is_rules_path(path: &str) -> bool {
    matches!(
        path,
        "/api/proxy/local-rules"
            | "/api/proxy/local-rules/preview"
            | "/api/proxy/local-rules/apply"
            | "/api/proxy/select"
            | "/api/proxy/nodes"
    )
}
pub(crate) fn unavailable(writer: &mut impl Write, head: bool) -> io::Result<()> {
    error(
        writer,
        503,
        "Service Unavailable",
        "local_rules_unavailable",
        "Local rule storage or subscription is unavailable.",
        head,
    )
}
pub(crate) fn runtime_unavailable(writer: &mut impl Write, head: bool) -> io::Result<()> {
    error(
        writer,
        503,
        "Service Unavailable",
        "runtime_unavailable",
        "Runtime management is unavailable.",
        head,
    )
}
pub(crate) fn method_not_allowed(
    writer: &mut impl Write,
    head: bool,
    allow: &str,
) -> io::Result<()> {
    let body = b"{\"error\":{\"code\":\"method_not_allowed\",\"message\":\"Method is not allowed for this endpoint.\"}}\n";
    http::write_response_extra(
        writer,
        405,
        "Method Not Allowed",
        body,
        head,
        JSON_TYPE,
        &[("Allow", allow)],
    )
}
fn response_too_large(writer: &mut impl Write, head: bool) -> io::Result<()> {
    let body = b"{\"error\":{\"code\":\"response_too_large\",\"message\":\"Rule response exceeds the supported limit.\"}}\n";
    http::write_response_extra(
        writer,
        503,
        "Service Unavailable",
        body,
        head,
        JSON_TYPE,
        &[],
    )
}
fn error(
    writer: &mut impl Write,
    status: u16,
    reason: &str,
    code: &'static str,
    message: &'static str,
    head: bool,
) -> io::Result<()> {
    write_json(
        writer,
        status,
        reason,
        &ErrorResponse {
            error: ApiError { code, message },
        },
        head,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn response_count_and_stream_are_identical_and_head_has_no_body() {
        let policy = Policy::default();
        let value = &policy;
        let mut get = Vec::new();
        write_json(&mut get, 200, "OK", value, false).unwrap();
        let mut head = Vec::new();
        write_json(&mut head, 200, "OK", value, true).unwrap();
        let end = get.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        assert_eq!(head, get[..end]);
        assert_eq!(get.len() - end, json_length(value).unwrap());
    }
    #[test]
    fn response_over_bound_refuses_before_headers_not_truncated_json() {
        let value = "x".repeat(MAX_RESPONSE_BYTES);
        let mut bytes = Vec::new();
        write_json(&mut bytes, 200, "OK", &value, false).unwrap();
        assert!(bytes.starts_with(b"HTTP/1.1 503 "));
        assert!(bytes.len() < 1024);
        assert!(
            String::from_utf8(bytes)
                .unwrap()
                .contains("response_too_large")
        );
    }
    #[test]
    fn strict_input_bounds_fields_arrays_types_and_depth() {
        let good = br#"{"policy":{"rules":[],"subscriptionEdits":[]}}"#;
        assert_eq!(decode_policy(good).unwrap(), Policy::default());
        for body in [br#"{"policy":{"rules":null,"subscriptionEdits":[]}}"#.as_slice(), br#"{"policy":{"rules":[],"subscriptionEdits":[],"extra":1}}"#, br#"{"policy":{"rules":[],"subscriptionEdits":[]},"policy":{"rules":[],"subscriptionEdits":[]}}"#] { assert_eq!(decode_policy(body), Err(InputError::Json)); }
        let deep = format!("{}0{}", "[".repeat(33), "]".repeat(33));
        assert_eq!(decode_policy(deep.as_bytes()), Err(InputError::Json));
        let scalar = format!(
            "{{\"policy\":{{\"rules\":[],\"subscriptionEdits\":[]}},\"{}\":0}}",
            "x".repeat(8193)
        );
        assert_eq!(decode_policy(scalar.as_bytes()), Err(InputError::Json));
    }
    #[test]
    fn committed_uncertainty_response_contains_new_snapshot_not_old_applied_status() {
        let snapshot = Snapshot {
            policy: Policy::default(),
            revision: "synthetic-committed-revision".into(),
        };
        let outcome = SaveOutcome {
            snapshot,
            committed: true,
            durability_error: Some(StoreError::Storage),
        };
        let mut bytes = Vec::new();
        write_save_uncertain(&mut bytes, &outcome).unwrap();
        let end = bytes.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        let body: serde_json::Value = serde_json::from_slice(&bytes[end..]).unwrap();
        assert!(bytes.starts_with(b"HTTP/1.1 500 "));
        assert_eq!(body["error"]["code"], "storage_failed");
        assert_eq!(body["committed"], true);
        assert_eq!(body["draft"]["revision"], outcome.snapshot.revision);
        assert!(body.get("applied").is_none());
    }
    #[test]
    fn low_space_and_measurement_are_fixed_conflicts_with_retained_draft() {
        let snapshot = Snapshot {
            policy: Policy::default(),
            revision: "synthetic-retained-revision".into(),
        };
        for failure in [StoreError::InsufficientSpace, StoreError::Measurement] {
            let mut bytes = Vec::new();
            write_save_failed(&mut bytes, failure, &snapshot).unwrap();
            let end = bytes.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
            let body: serde_json::Value = serde_json::from_slice(&bytes[end..]).unwrap();
            assert!(bytes.starts_with(b"HTTP/1.1 409 "));
            assert_eq!(body["error"]["code"], "storage_insufficient");
            assert_eq!(body["committed"], false);
            assert_eq!(body["draft"]["revision"], snapshot.revision);
            assert!(body.get("applied").is_none());
        }
    }
}

//! Pure, owned local policy overlays. This module performs no I/O or compilation.
//!
//! Enabled local rules precede the subscription in exact draft order. A preview
//! `effective_index` is not a native route index: compiler preludes, resolve
//! actions, and fallback rules are outside this user policy.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::io::{self, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub const MAX_RULES: usize = 8192;
pub const MAX_LOCAL_RULES: usize = 512;
pub const MAX_SUBSCRIPTION_EDITS: usize = 1024;
pub const MAX_POLICY_ID_BYTES: usize = 64;
pub const MAX_RULE_LABEL_RUNES: usize = 64;
pub const MAX_RULE_NOTE_RUNES: usize = 256;
pub const MAX_RULE_VALUE_BYTES: usize = 253;

/// Unknown strings remain representable so validation reports the original
/// unsupported-matcher diagnostic, instead of changing its meaning at decoding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum RuleKind {
    Domain,
    DomainSuffix,
    DomainKeyword,
    IpCidr,
    RuleSet,
    Match,
    Unknown(String),
}

impl Default for RuleKind {
    fn default() -> Self {
        Self::Unknown(String::new())
    }
}

impl From<&str> for RuleKind {
    fn from(value: &str) -> Self {
        Self::from(value.to_owned())
    }
}

impl From<String> for RuleKind {
    fn from(value: String) -> Self {
        match value.as_str() {
            "domain" => Self::Domain,
            "domain-suffix" => Self::DomainSuffix,
            "domain-keyword" => Self::DomainKeyword,
            "ip-cidr" => Self::IpCidr,
            "rule-set" => Self::RuleSet,
            "match" => Self::Match,
            _ => Self::Unknown(value),
        }
    }
}

impl From<RuleKind> for String {
    fn from(value: RuleKind) -> Self {
        match value {
            RuleKind::Domain => "domain".into(),
            RuleKind::DomainSuffix => "domain-suffix".into(),
            RuleKind::DomainKeyword => "domain-keyword".into(),
            RuleKind::IpCidr => "ip-cidr".into(),
            RuleKind::RuleSet => "rule-set".into(),
            RuleKind::Match => "match".into(),
            RuleKind::Unknown(value) => value,
        }
    }
}

/// The action is `block`, never `reject`. Unknown strings have no aliases.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum Target {
    Direct,
    Proxy,
    Block,
    Unknown(String),
}

impl Default for Target {
    fn default() -> Self {
        Self::Unknown(String::new())
    }
}

impl From<&str> for Target {
    fn from(value: &str) -> Self {
        Self::from(value.to_owned())
    }
}

impl From<String> for Target {
    fn from(value: String) -> Self {
        match value.as_str() {
            "direct" => Self::Direct,
            "proxy" => Self::Proxy,
            "block" => Self::Block,
            _ => Self::Unknown(value),
        }
    }
}

impl From<Target> for String {
    fn from(value: Target) -> Self {
        match value {
            Target::Direct => "direct".into(),
            Target::Proxy => "proxy".into(),
            Target::Block => "block".into(),
            Target::Unknown(value) => value,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Rule {
    pub kind: RuleKind,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub value: String,
    pub target: Target,
    #[serde(skip_serializing_if = "is_false")]
    pub no_resolve: bool,
    pub index: i64,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LocalRule {
    pub id: String,
    pub enabled: bool,
    pub label: String,
    pub note: String,
    pub rule: Rule,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct SubscriptionEdit {
    pub id: String,
    pub source_fingerprint: String,
    pub disabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement: Option<Rule>,
    pub label: String,
    pub note: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Policy {
    pub rules: Vec<LocalRule>,
    pub subscription_edits: Vec<SubscriptionEdit>,
}

/// Fixed, safe public messages. Rejected rule values are never copied here.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Diagnostic {
    pub scope: String,
    pub index: i64,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EffectiveRuleIdentity {
    /// Index into `EffectivePolicy.rules`, not into compiled native rules.
    pub effective_index: i64,
    pub layer: String,
    pub stable_id: String,
    pub label: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub source_fingerprint: String,
    pub source_index: i64,
    pub source_ordinal: i64,
    pub kind: RuleKind,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub value: String,
    pub target: Target,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EffectivePolicy {
    pub rules: Vec<Rule>,
    pub provenance: Vec<EffectiveRuleIdentity>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Owns the same validation diagnostics that an invalid Go merge returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyError {
    pub diagnostics: Vec<Diagnostic>,
}

impl fmt::Display for PolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid local proxy policy")
    }
}

impl std::error::Error for PolicyError {}

fn diagnostic(scope: &str, index: i64, code: &str, message: &str) -> Diagnostic {
    Diagnostic {
        scope: scope.into(),
        index,
        code: code.into(),
        message: message.into(),
    }
}

fn valid_policy_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_POLICY_ID_BYTES
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
}

fn valid_policy_text(text: &str, limit: usize, multiline: bool) -> bool {
    text.len() <= limit * 4
        && text.chars().count() <= limit
        && text
            .chars()
            .all(|c| !c.is_control() || multiline && matches!(c, '\n' | '\t'))
}

// Exact private subscription.go validDomain, including single-label domains,
// numeric domains, no trailing dot, and ASCII-only labels.
fn valid_domain(domain: &str) -> bool {
    !domain.is_empty()
        && domain.len() <= 253
        && !domain.ends_with('.')
        && domain.split('.').all(|part| {
            !part.is_empty()
                && part.len() <= 63
                && !part.starts_with('-')
                && !part.ends_with('-')
                && part.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
}

// netip.ParsePrefix rejects zones, mapped IPv6, signed/zero-padded prefix
// lengths, and zero-padded IPv4. Prefixes may retain host bits until hashing.
fn parse_prefix(value: &str) -> Option<(IpAddr, u8)> {
    let (address, bits) = value.rsplit_once('/')?;
    if bits.is_empty()
        || !bits.bytes().all(|c| c.is_ascii_digit())
        || bits.len() > 1 && !(b'1'..=b'9').contains(&bits.as_bytes()[0])
    {
        return None;
    }
    let ip: IpAddr = address.parse().ok()?;
    let bits: u8 = bits.parse().ok()?;
    match ip {
        IpAddr::V4(_) if bits <= 32 => Some((ip, bits)),
        IpAddr::V6(address) if bits <= 128 && address.to_ipv4_mapped().is_none() => {
            Some((ip, bits))
        }
        _ => None,
    }
}

// Exact private validateRule, before the additional local policy constraints.
fn valid_rule(rule: &Rule) -> bool {
    if !matches!(rule.target, Target::Direct | Target::Proxy | Target::Block) {
        return false;
    }
    match rule.kind {
        RuleKind::Domain | RuleKind::DomainSuffix => valid_domain(&rule.value),
        RuleKind::DomainKeyword => {
            !rule.value.is_empty()
                && rule.value.len() <= 253
                && !rule.value.bytes().any(|c| matches!(c, b'\r' | b'\n' | 0))
        }
        RuleKind::IpCidr => parse_prefix(&rule.value).is_some(),
        RuleKind::RuleSet => matches!(rule.value.as_str(), "cn-domain" | "cn-ip" | "proxy-domain"),
        RuleKind::Match => rule.value.is_empty(),
        RuleKind::Unknown(_) => false,
    }
}

fn rule_problem(rule: &Rule) -> Option<(&'static str, &'static str)> {
    if matches!(rule.kind, RuleKind::Unknown(_)) {
        return Some((
            "unsupported-matcher",
            "rule matcher is not supported for forwarded clients",
        ));
    }
    if rule.kind == RuleKind::RuleSet
        && !matches!(rule.value.as_str(), "cn-domain" | "cn-ip" | "proxy-domain")
    {
        return Some((
            "unsupported-rule-set",
            "rule set is not a controlled local set",
        ));
    }
    if !valid_rule(rule) {
        return Some(("invalid-rule", "rule matcher value or action is invalid"));
    }
    if rule.value.len() > MAX_RULE_VALUE_BYTES
        || rule.no_resolve
            && rule.kind != RuleKind::IpCidr
            && !(rule.kind == RuleKind::RuleSet && rule.value == "cn-ip")
    {
        return Some((
            "invalid-rule",
            "rule value or option exceeds supported limits",
        ));
    }
    if rule.kind == RuleKind::DomainKeyword
        && !rule.value.bytes().all(|c| (0x21..=0x7e).contains(&c))
    {
        return Some((
            "invalid-rule",
            "rule keyword requires printable ASCII without spaces",
        ));
    }
    None
}

fn valid_source_fingerprint(reference: &str) -> bool {
    let bytes = reference.as_bytes();
    if !(66..=69).contains(&bytes.len())
        || bytes[64] != b':'
        || !bytes[..64]
            .iter()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(c))
        || !bytes[65..].iter().all(u8::is_ascii_digit)
    {
        return false;
    }
    let suffix = &reference[65..];
    let Ok(occurrence) = suffix.parse::<usize>() else {
        return false;
    };
    (1..=MAX_RULES).contains(&occurrence) && occurrence.to_string() == suffix
}

fn metadata_problems(
    problems: &mut Vec<Diagnostic>,
    ids: &mut HashSet<String>,
    scope: &str,
    index: i64,
    id: &str,
    label: &str,
    note: &str,
) {
    if !valid_policy_id(id) || ids.contains(id) {
        problems.push(diagnostic(
            scope,
            index,
            "invalid-id",
            "rule identity must be unique bounded ASCII",
        ));
    }
    ids.insert(id.into());
    if !valid_policy_text(label, MAX_RULE_LABEL_RUNES, false)
        || !valid_policy_text(note, MAX_RULE_NOTE_RUNES, true)
    {
        problems.push(diagnostic(
            scope,
            index,
            "invalid-metadata",
            "rule label or note exceeds supported limits",
        ));
    }
}

fn policy_problems(policy: &Policy) -> Vec<Diagnostic> {
    let mut problems = Vec::new();
    if policy.rules.len() > MAX_LOCAL_RULES
        || policy.subscription_edits.len() > MAX_SUBSCRIPTION_EDITS
    {
        problems.push(diagnostic(
            "policy",
            -1,
            "policy-limit",
            "local rule or subscription edit limit exceeded",
        ));
        return problems;
    }
    let mut ids = HashSet::with_capacity(policy.rules.len() + policy.subscription_edits.len());
    for (i, local) in policy.rules.iter().enumerate() {
        metadata_problems(
            &mut problems,
            &mut ids,
            "local-rule",
            i as i64,
            &local.id,
            &local.label,
            &local.note,
        );
        if let Some((code, message)) = rule_problem(&local.rule) {
            problems.push(diagnostic("local-rule", i as i64, code, message));
        }
    }
    let mut refs = HashSet::with_capacity(policy.subscription_edits.len());
    for (i, edit) in policy.subscription_edits.iter().enumerate() {
        metadata_problems(
            &mut problems,
            &mut ids,
            "subscription-edit",
            i as i64,
            &edit.id,
            &edit.label,
            &edit.note,
        );
        if !valid_source_fingerprint(&edit.source_fingerprint)
            || refs.contains(&edit.source_fingerprint)
        {
            problems.push(diagnostic(
                "subscription-edit",
                i as i64,
                "invalid-reference",
                "subscription reference must be exact and unique",
            ));
        }
        refs.insert(&edit.source_fingerprint);
        if edit.disabled == edit.replacement.is_some() {
            problems.push(diagnostic(
                "subscription-edit",
                i as i64,
                "invalid-edit",
                "subscription edit must disable or replace one rule",
            ));
        }
        if let Some(rule) = &edit.replacement
            && let Some((code, message)) = rule_problem(rule)
        {
            problems.push(diagnostic("subscription-edit", i as i64, code, message));
        }
    }
    problems
}

/// Validates disabled draft rules and inactive orphan edits as well.
pub fn validate_policy(policy: &Policy) -> Result<(), PolicyError> {
    let diagnostics = policy_problems(policy);
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(PolicyError { diagnostics })
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SemanticRule<'a> {
    kind: &'a RuleKind,
    value: Cow<'a, str>,
    target: &'a Target,
    no_resolve: bool,
}

fn normalized_rule(rule: &Rule) -> SemanticRule<'_> {
    let value = match rule.kind {
        RuleKind::Domain | RuleKind::DomainSuffix | RuleKind::DomainKeyword => {
            Cow::Owned(rule.value.to_ascii_lowercase())
        }
        RuleKind::IpCidr => {
            let (address, bits) =
                parse_prefix(&rule.value).expect("only validated rules are normalized");
            let masked = match address {
                IpAddr::V4(ip) => {
                    let mask = if bits == 0 {
                        0
                    } else {
                        u32::MAX << (32 - bits)
                    };
                    IpAddr::V4(Ipv4Addr::from(u32::from(ip) & mask))
                }
                IpAddr::V6(ip) => {
                    let mask = if bits == 0 {
                        0
                    } else {
                        u128::MAX << (128 - bits)
                    };
                    IpAddr::V6(Ipv6Addr::from(u128::from(ip) & mask))
                }
            };
            Cow::Owned(format!("{masked}/{bits}"))
        }
        _ => Cow::Borrowed(rule.value.as_str()),
    };
    SemanticRule {
        kind: &rule.kind,
        value,
        target: &rule.target,
        no_resolve: rule.no_resolve,
    }
}

// Stream typed serialization directly into SHA256; no JSON Value cache or
// whole-policy serialized buffer. Match encoding/json's default HTML escaping.
struct GoJsonFormatter;

impl serde_json::ser::Formatter for GoJsonFormatter {
    fn write_string_fragment<W: ?Sized + Write>(
        &mut self,
        writer: &mut W,
        fragment: &str,
    ) -> io::Result<()> {
        let mut start = 0;
        for (offset, c) in fragment.char_indices() {
            let escape = match c {
                '<' => b"\\u003c".as_slice(),
                '>' => b"\\u003e".as_slice(),
                '&' => b"\\u0026".as_slice(),
                '\u{2028}' => b"\\u2028".as_slice(),
                '\u{2029}' => b"\\u2029".as_slice(),
                _ => continue,
            };
            writer.write_all(&fragment.as_bytes()[start..offset])?;
            writer.write_all(escape)?;
            start = offset + c.len_utf8();
        }
        writer.write_all(&fragment.as_bytes()[start..])
    }
}

struct HashWriter(Sha256);

impl Write for HashWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn hash_policy_value(value: &impl Serialize) -> [u8; 32] {
    let mut writer = HashWriter(Sha256::new());
    let mut serializer = serde_json::Serializer::with_formatter(&mut writer, GoJsonFormatter);
    value
        .serialize(&mut serializer)
        .expect("concrete policy types and hash writer cannot fail");
    writer.0.finalize().into()
}

fn append_digest_hex(out: &mut String, digest: &[u8; 32]) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in digest {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
}

/// Hashes ordered, typed policy content without `Rule.index`. Metadata stays
/// exact; only validated rule semantics are normalized. Not a schema version.
pub fn policy_revision(policy: &Policy) -> Result<String, PolicyError> {
    validate_policy(policy)?;
    #[derive(Serialize)]
    struct LocalIdentity<'a> {
        id: &'a str,
        enabled: bool,
        label: &'a str,
        note: &'a str,
        rule: SemanticRule<'a>,
    }
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct EditIdentity<'a> {
        id: &'a str,
        source_fingerprint: &'a str,
        disabled: bool,
        label: &'a str,
        note: &'a str,
        replacement: Option<SemanticRule<'a>>,
    }
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Identity<'a> {
        rules: Vec<LocalIdentity<'a>>,
        subscription_edits: Vec<EditIdentity<'a>>,
    }
    let identity = Identity {
        rules: policy
            .rules
            .iter()
            .map(|rule| LocalIdentity {
                id: &rule.id,
                enabled: rule.enabled,
                label: &rule.label,
                note: &rule.note,
                rule: normalized_rule(&rule.rule),
            })
            .collect(),
        subscription_edits: policy
            .subscription_edits
            .iter()
            .map(|edit| EditIdentity {
                id: &edit.id,
                source_fingerprint: &edit.source_fingerprint,
                disabled: edit.disabled,
                label: &edit.label,
                note: &edit.note,
                replacement: edit.replacement.as_ref().map(normalized_rule),
            })
            .collect(),
    };
    let mut revision = String::with_capacity(64);
    append_digest_hex(&mut revision, &hash_policy_value(&identity));
    Ok(revision)
}

fn subscription_problems(subscription: &[Rule]) -> Vec<Diagnostic> {
    if subscription.len() > MAX_RULES {
        return vec![diagnostic(
            "subscription",
            -1,
            "subscription-limit",
            "subscription rule limit exceeded",
        )];
    }
    subscription
        .iter()
        .enumerate()
        .filter_map(|(i, rule)| {
            rule_problem(rule)
                .map(|(code, message)| diagnostic("subscription", i as i64, code, message))
        })
        .collect()
}

fn fingerprints_validated(subscription: &[Rule]) -> Vec<String> {
    use std::fmt::Write as _;

    // Duplicate counters need the digest bytes, not an owned hex string per key.
    let mut occurrences = HashMap::with_capacity(subscription.len());
    subscription
        .iter()
        .map(|rule| {
            let digest = hash_policy_value(&normalized_rule(rule));
            let occurrence = occurrences.entry(digest).or_insert(0usize);
            *occurrence += 1;
            let mut reference = String::with_capacity(69);
            append_digest_hex(&mut reference, &digest);
            write!(reference, ":{occurrence}").expect("writing to String cannot fail");
            reference
        })
        .collect()
}

/// Semantic SHA256 plus a one-based duplicate occurrence, in source order.
/// Source indexes are excluded and the caller's exact rules are not changed.
pub fn subscription_fingerprints(subscription: &[Rule]) -> Result<Vec<String>, PolicyError> {
    let diagnostics = subscription_problems(subscription);
    if !diagnostics.is_empty() {
        return Err(PolicyError { diagnostics });
    }
    Ok(fingerprints_validated(subscription))
}

/// Prepends enabled locals, then retains exact subscription order and content
/// unless explicitly disabled/replaced. Missing references remain inactive.
/// Rules after terminal MATCH remain visible with unreachable diagnostics.
pub fn merge_effective_policy(
    subscription: &[Rule],
    policy: &Policy,
) -> Result<EffectivePolicy, PolicyError> {
    validate_policy(policy)?;
    let diagnostics = subscription_problems(subscription);
    if !diagnostics.is_empty() {
        return Err(PolicyError { diagnostics });
    }
    let refs = fingerprints_validated(subscription);
    let edits: HashMap<_, _> = policy
        .subscription_edits
        .iter()
        .enumerate()
        .map(|(i, edit)| (edit.source_fingerprint.as_str(), i))
        .collect();
    let mut used = vec![false; policy.subscription_edits.len()];
    let enabled_locals = policy.rules.iter().filter(|local| local.enabled).count();
    let disabled_sources = refs
        .iter()
        .filter(|reference| {
            edits
                .get(reference.as_str())
                .is_some_and(|&i| policy.subscription_edits[i].disabled)
        })
        .count();
    let eligible_rules = enabled_locals + subscription.len() - disabled_sources;
    let mut out = EffectivePolicy {
        rules: Vec::with_capacity(eligible_rules),
        provenance: Vec::with_capacity(eligible_rules),
        // Draft/edit diagnostics are bounded here. Unreachable rules grow this
        // only when a terminal MATCH actually requires those diagnostics.
        diagnostics: Vec::with_capacity(
            policy.rules.len() - enabled_locals + policy.subscription_edits.len(),
        ),
    };
    let mut terminal = false;
    fn append_rule(
        out: &mut EffectivePolicy,
        terminal: &mut bool,
        rule: Rule,
        mut identity: EffectiveRuleIdentity,
    ) {
        identity.effective_index = out.rules.len() as i64;
        identity.kind = rule.kind.clone();
        identity.value = rule.value.clone();
        identity.target = rule.target.clone();
        if *terminal {
            out.diagnostics.push(diagnostic(
                "effective-rule",
                identity.effective_index,
                "unreachable-rule",
                "rule follows terminal MATCH and is unreachable",
            ));
        }
        if rule.kind == RuleKind::Match {
            *terminal = true;
        }
        out.rules.push(rule);
        out.provenance.push(identity);
    }
    for (i, local) in policy.rules.iter().enumerate() {
        if !local.enabled {
            out.diagnostics.push(diagnostic(
                "local-rule",
                i as i64,
                "disabled-rule",
                "disabled local rule is not compiled",
            ));
            continue;
        }
        append_rule(
            &mut out,
            &mut terminal,
            local.rule.clone(),
            EffectiveRuleIdentity {
                layer: "local".into(),
                stable_id: local.id.clone(),
                label: local.label.clone(),
                source_index: -1,
                source_ordinal: i as i64,
                ..EffectiveRuleIdentity::default()
            },
        );
    }
    for (i, original) in subscription.iter().enumerate() {
        let edit = edits.get(refs[i].as_str()).map(|&edit_index| {
            used[edit_index] = true;
            (edit_index, &policy.subscription_edits[edit_index])
        });
        if let Some((edit_index, edit)) = edit
            && edit.disabled
        {
            out.diagnostics.push(diagnostic(
                "subscription-edit",
                edit_index as i64,
                "disabled-rule",
                "explicitly disabled subscription rule is not compiled",
            ));
            continue;
        }
        // Disabled sources need no owned rule or provenance strings. A replaced
        // source needs only its replacement, never an intermediate source clone.
        let (mut rule, stable_id, label) = match edit {
            Some((_, edit)) => (
                edit.replacement
                    .as_ref()
                    .expect("edit was validated")
                    .clone(),
                edit.id.clone(),
                edit.label.clone(),
            ),
            None => (original.clone(), refs[i].clone(), String::new()),
        };
        rule.index = original.index;
        let identity = EffectiveRuleIdentity {
            layer: "subscription".into(),
            stable_id,
            label,
            source_fingerprint: refs[i].clone(),
            source_index: original.index,
            source_ordinal: i as i64,
            ..EffectiveRuleIdentity::default()
        };
        append_rule(&mut out, &mut terminal, rule, identity);
    }
    for (i, matched) in used.iter().enumerate() {
        if !matched {
            out.diagnostics.push(diagnostic(
                "subscription-edit",
                i as i64,
                "orphaned-edit",
                "subscription edit reference is absent and remains inactive",
            ));
        }
    }
    Ok(out)
}

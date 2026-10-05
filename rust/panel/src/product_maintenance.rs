//! Private, scoped configuration downloads and memory-only import previews.
//!
//! The authenticated root owns HTTP, authoritative generation observations,
//! native draft creation/cleanup and RuntimeManager verification. This module
//! never applies files, starts a service, owns a daemon, or accepts file paths.
//! Root must clear previews on authentication reset. Public projections exclude
//! runtime credentials and opaque TOML. A running frpc process proves neither
//! tunnel availability nor negotiated TLS.
use crate::product_io::{Backend, timestamp};
use crate::readiness_tun::{Budget, TunError};
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const MAX_BACKUP_BYTES: usize = 2 << 20;
pub const MAX_DOCUMENT_BYTES: usize = 128 << 10;
pub const MAX_RUNTIME_BYTES: usize = 512 << 10;
pub const MAX_PREVIEWS: usize = 4;
pub const MAX_PREVIEW_CONTENT_BYTES: usize = 4 << 20;
pub const PREVIEW_LIFETIME: Duration = Duration::from_secs(600);
pub const NATIVE_SCOPES: [&str; 6] = ["network", "wireless", "dhcp", "firewall", "system", "dropbear"];
const EXTRA_SCOPES: [&str; 4] = ["runtime.frpc", "runtime.sing-box", "device.annotations", "device.history"];
const MAX_JSON_NODES: usize = 65536;
const MAX_TOML_BYTES: usize = 1 << 20;
const MAX_STATEMENTS: usize = 8192;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApiError {
    pub status: u16,
    pub code: &'static str,
    pub message: &'static str,
}
impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for ApiError {}
fn failure(status: u16, code: &'static str, message: &'static str) -> ApiError {
    ApiError { status, code, message }
}
fn invalid_backup() -> ApiError {
    failure(400, "invalid_backup", "Provide one bounded UTF-8 JSON backup with exact, unique fields.")
}
fn invalid_frpc() -> ApiError {
    failure(422, "invalid_frpc", "The TOML shape cannot be safely edited in the form; use the native configuration editor.")
}
fn invalid_input() -> ApiError {
    failure(400, "invalid_field", "FRPC editor fields or credential intent are invalid.")
}
fn check(budget: &Budget<'_>) -> Result<(), ApiError> {
    budget.check().map_err(|e| match e {
        TunError::Cancelled => failure(409, "operation_cancelled", "Configuration operation was cancelled."),
        _ => failure(504, "operation_timeout", "Configuration operation exceeded its deadline."),
    })
}
// Bound encoding before allocating download/history output. JSON escaping can
// expand private content to six times its original byte length.
struct CappedJson { bytes: Vec<u8>, limit: usize }
impl std::io::Write for CappedJson {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.bytes.len().saturating_add(bytes.len()) > self.limit {
            return Err(std::io::Error::other("bounded JSON output"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}
fn encode_bounded(value: &Value,limit: usize,error: ApiError) -> Result<Vec<u8>,ApiError> {
    let mut output = CappedJson { bytes:Vec::new(),limit };
    serde_json::to_writer(&mut output,value).map_err(|_| error)?;
    Ok(output.bytes)
}
fn sha256(content: &[u8]) -> String {
    format!("{:x}", Sha256::digest(content))
}
fn native(scope: &str) -> bool { NATIVE_SCOPES.contains(&scope) }
fn supported(scope: &str) -> bool { native(scope) || EXTRA_SCOPES.contains(&scope) }
fn document_limit(scope: &str) -> usize {
    if native(scope) { MAX_DOCUMENT_BYTES } else { MAX_RUNTIME_BYTES }
}
fn scopes_valid(scopes: &[String]) -> Result<(), ApiError> {
    if scopes.is_empty() || scopes.len() > 10 || scopes.iter().any(|s| !supported(s)) {
        return Err(failure(400, "invalid_scope", "Select supported configuration scopes, not paths or file restoration instructions."));
    }
    let mut seen = BTreeSet::new();
    if scopes.iter().any(|s| !seen.insert(s)) {
        return Err(failure(400, "duplicate_scope", "Backup scopes must be unique."));
    }
    Ok(())
}

// Deserialize the original bytes, not a browser-reserialized object. Ordinary
// serde_json::Value silently accepts duplicate keys and is not an import parser.
struct JsonSeed<'a, 'b> {
    depth: usize,
    nodes: &'a Cell<usize>,
    budget: Option<&'a Budget<'b>>,
}
impl<'de> DeserializeSeed<'de> for JsonSeed<'_, '_> {
    type Value = Value;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        if self.depth > 16 || self.nodes.get() >= MAX_JSON_NODES {
            return Err(de::Error::custom("bounded JSON limit"));
        }
        self.nodes.set(self.nodes.get() + 1);
        if self.budget.is_some_and(|b| b.check().is_err()) {
            return Err(de::Error::custom("bounded JSON budget"));
        }
        d.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for JsonSeed<'_, '_> {
    type Value = Value;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str("bounded unique JSON") }
    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Value, E> { Ok(Value::Bool(v)) }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Value, E> { Ok(v.into()) }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Value, E> { Ok(v.into()) }
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
        serde_json::Number::from_f64(v).map(Value::Number).ok_or_else(|| E::custom("finite JSON number"))
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Value, E> { Ok(Value::String(v.to_owned())) }
    fn visit_string<E: de::Error>(self, v: String) -> Result<Value, E> { Ok(Value::String(v)) }
    fn visit_unit<E: de::Error>(self) -> Result<Value, E> { Ok(Value::Null) }
    fn visit_none<E: de::Error>(self) -> Result<Value, E> { Ok(Value::Null) }
    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(v) = a.next_element_seed(JsonSeed { depth: self.depth + 1, nodes: self.nodes, budget: self.budget })? {
            values.push(v);
        }
        Ok(Value::Array(values))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Value, A::Error> {
        let mut values = Map::new();
        while let Some(k) = a.next_key::<String>()? {
            if values.contains_key(&k) { return Err(de::Error::custom("duplicate JSON field")); }
            let v = a.next_value_seed(JsonSeed { depth: self.depth + 1, nodes: self.nodes, budget: self.budget })?;
            values.insert(k, v);
        }
        Ok(Value::Object(values))
    }
}
fn strict_json(raw: &[u8], budget: Option<&Budget<'_>>) -> Result<Value, ApiError> {
    if raw.len() > MAX_BACKUP_BYTES {
        return Err(failure(413, "backup_too_large", "Backup exceeds the 2 MiB JSON limit."));
    }
    if std::str::from_utf8(raw).is_err() { return Err(invalid_backup()); }
    if let Some(b) = budget { check(b)?; }
    let nodes = Cell::new(0);
    let mut decoder = serde_json::Deserializer::from_slice(raw);
    let result = JsonSeed { depth: 0, nodes: &nodes, budget }.deserialize(&mut decoder);
    if let Some(b) = budget { check(b)?; }
    let result = result.map_err(|_| invalid_backup())?;
    decoder.end().map_err(|_| invalid_backup())?;
    Ok(result)
}
fn exact_fields(value: &Value, required: &[&str], optional: &[&str]) -> Result<(), ApiError> {
    let object = value.as_object().ok_or_else(invalid_backup)?;
    if required.iter().any(|k| !object.contains_key(*k)) || object.iter().any(|(k,v)| v.is_null() || (!required.contains(&k.as_str()) && !optional.contains(&k.as_str()))) {
        return Err(invalid_backup());
    }
    Ok(())
}
fn bounded_text(value: &Value, key: &str, max: usize) -> Result<String, ApiError> {
    let text = value.get(key).and_then(Value::as_str).ok_or_else(invalid_backup)?;
    if text.is_empty() || text.len() > max || text.chars().any(char::is_control) {
        return Err(failure(400, "invalid_manifest", "Backup must identify the router model and build."));
    }
    Ok(text.to_owned())
}
fn generation(value: &Value) -> Result<u64, ApiError> {
    value.as_u64().filter(|v| *v > 0).ok_or_else(|| failure(400, "invalid_manifest", "Configuration must identify a positive accepted generation."))
}
// RFC3339 with real date/time ranges, optional fraction and numeric UTC offset.
fn valid_timestamp(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() < 20 || !s.is_ascii() || b.get(4) != Some(&b'-') || b.get(7) != Some(&b'-') || b.get(10) != Some(&b'T') || b.get(13) != Some(&b':') || b.get(16) != Some(&b':') { return false; }
    let number = |start: usize, end: usize| -> Option<u32> { s.get(start..end)?.parse().ok() };
    let Some(year) = number(0,4) else { return false; };
    let Some(month) = number(5,7) else { return false; };
    let Some(day) = number(8,10) else { return false; };
    let Some(hour) = number(11,13) else { return false; };
    let Some(minute) = number(14,16) else { return false; };
    let Some(second) = number(17,19) else { return false; };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month { 1|3|5|7|8|10|12 => 31, 4|6|9|11 => 30, 2 => if leap {29} else {28}, _ => return false };
    if year == 0 || day == 0 || day > days || hour > 23 || minute > 59 || second > 59 { return false; }
    let mut p = 19;
    if b.get(p) == Some(&b'.') {
        p += 1;
        let start = p;
        while b.get(p).is_some_and(u8::is_ascii_digit) { p += 1; }
        if p == start || p - start > 9 { return false; }
    }
    if s.get(p..) == Some("Z") { return true; }
    b.len() == p + 6 && matches!(b[p], b'+'|b'-') && b[p+3] == b':' && number(p+1,p+3).is_some_and(|n| n <= 23) && number(p+4,p+6).is_some_and(|n| n <= 59)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    module: String,
    content: String,
    digest: String,
    #[serde(default)]
    generation: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Envelope {
    model: String,
    build: String,
    created_at: String,
    generation: u64,
    scopes: Vec<String>,
    documents: Vec<Document>,
}
fn decode(raw: &[u8], budget: &Budget<'_>) -> Result<Envelope, ApiError> {
    let value = strict_json(raw, Some(budget))?;
    exact_fields(&value, &["model","build","createdAt","generation","scopes","documents"], &[])?;
    bounded_text(&value, "model", 128)?;
    bounded_text(&value, "build", 256)?;
    let list = value["documents"].as_array().ok_or_else(invalid_backup)?;
    if list.is_empty() || list.len() > 10 { return Err(invalid_backup()); }
    for document in list { exact_fields(document, &["module","content","digest"], &["generation"])?; }
    let envelope: Envelope = serde_json::from_value(value).map_err(|_| invalid_backup())?;
    generation(&json!(envelope.generation))?;
    if !valid_timestamp(&envelope.created_at) { return Err(failure(400, "invalid_manifest", "Backup creation time must be a valid RFC3339 timestamp.")); }
    scopes_valid(&envelope.scopes)?;
    if envelope.documents.len() != envelope.scopes.len() { return Err(failure(400, "missing_component", "Every selected scope must contain exactly one document.")); }
    let mut modules = BTreeSet::new();
    for document in &envelope.documents {
        check(budget)?;
        if !modules.insert(&document.module) { return Err(failure(400, "duplicate_module", "Backup documents must use unique module names.")); }
        if !envelope.scopes.contains(&document.module) { return Err(failure(400, "invalid_scope", "Document is outside the selected configuration scopes.")); }
        if document.content.len() > document_limit(&document.module) { return Err(failure(413, "document_too_large", "A selected configuration exceeds its document limit.")); }
        if document.digest != sha256(document.content.as_bytes()) { return Err(failure(400, "digest_mismatch", "Document digest does not match its exact content.")); }
        if document.module.starts_with("runtime.") && document.generation == 0 { return Err(failure(400, "invalid_manifest", "Runtime settings must identify their accepted generation.")); }
    }
    Ok(envelope)
}
fn metadata_valid(metadata: &Value) -> Result<(String,String), ApiError> {
    let model = bounded_text(metadata,"model",128).map_err(|_| failure(503,"metadata_unavailable","Cannot identify the current router model and build."))?;
    let build = bounded_text(metadata,"build",256).map_err(|_| failure(503,"metadata_unavailable","Cannot identify the current router model and build."))?;
    Ok((model,build))
}
fn pending(value: &Value) -> Result<(), ApiError> {
    if value.get("pendingCommit").is_some_and(|v| !v.is_null()) {
        return Err(failure(409,"confirmation_pending","Confirm or restore the provisional configuration before backup or import."));
    }
    Ok(())
}
fn current_documents(value: &Value) -> Result<BTreeMap<String,String>, ApiError> {
    let Some(list) = value.get("documents").and_then(Value::as_array) else { return Ok(BTreeMap::new()); };
    if list.len() > 10 { return Err(failure(503,"snapshot_unavailable","Current configuration snapshot is invalid.")); }
    let mut docs = BTreeMap::new();
    for item in list {
        let module = item["module"].as_str().ok_or_else(invalid_backup)?;
        let content = item["content"].as_str().ok_or_else(invalid_backup)?;
        if !supported(module) || content.len() > document_limit(module) || docs.insert(module.to_owned(),content.to_owned()).is_some() {
            return Err(failure(503,"snapshot_unavailable","Current configuration snapshot is invalid."));
        }
    }
    Ok(docs)
}
fn native_values(documents: &BTreeMap<String,String>) -> Value {
    Value::Array(documents.iter().filter(|(m,_)| native(m)).map(|(m,c)| json!({"module":m,"content":c})).collect())
}
fn issue(code: &'static str, message: &'static str) -> Value { json!({"code":code,"message":message}) }
struct StoredPreview {
    envelope: Envelope,
    current: BTreeMap<String,String>,
    generation: u64,
    model_mismatch: bool,
    deadline: Instant,
    bytes: usize,
}
/// Private content remains RAM-only. No archive, file import, or automatic flash history.
pub struct Maintenance {
    previews: RefCell<BTreeMap<String, StoredPreview>>,
}
impl fmt::Debug for Maintenance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str("Maintenance([private])") }
}
/// Private candidate consumed only by Configuration::stage_for_import. Root must
/// prevalidate the entire selected bundle and remove only IDs created by this
/// call on any failure, including cancellation. Cleanup failure must report
/// exact retainedDraftIds rather than claiming they were discarded.
pub struct ImportCandidate {
    pub module: String,
    pub content: String,
}
impl fmt::Debug for ImportCandidate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str("ImportCandidate([private])") }
}
impl Maintenance {
    pub fn open(data_dir: &Path) -> Result<Self, ApiError> {
        if data_dir.as_os_str().is_empty() { return Err(failure(400,"invalid_storage","A private application data directory is required.")); }
        Ok(Self { previews: RefCell::new(BTreeMap::new()) })
    }
    pub fn clear(&self) { self.previews.borrow_mut().clear(); }
    pub fn backup(&self, scopes: &[String], metadata: Value, native_documents: Value,
        runtime: &[(String,Vec<u8>)], annotations: Option<&[u8]>, io: &mut impl Backend,
        budget: &Budget<'_>) -> Result<Vec<u8>, ApiError> {
        check(budget)?;
        scopes_valid(scopes)?;
        pending(&native_documents)?;
        let (model,build) = metadata_valid(&metadata)?;
        let accepted_generation = generation(&native_documents["generation"])?;
        if metadata.get("generation").is_some_and(|v|v.as_u64()!=Some(accepted_generation)) {
            return Err(failure(409,"generation_conflict","Configuration changed while obtaining the backup snapshot."));
        }
        let native_docs = current_documents(&native_documents)?;
        if NATIVE_SCOPES.iter().any(|m| !native_docs.contains_key(*m)) {
            return Err(failure(503,"snapshot_unavailable","Accepted native snapshot must contain all six configuration modules."));
        }
        let mut runtime_docs = BTreeMap::new();
        for (module,bytes) in runtime {
            let scope = match module.as_str() { "frpc" => "runtime.frpc", "sing-box" => "runtime.sing-box", m => m };
            if !["runtime.frpc","runtime.sing-box"].contains(&scope) || runtime_docs.insert(scope,bytes).is_some() {
                return Err(failure(400,"invalid_scope","Runtime snapshot must use unique fixed service names."));
            }
        }
        let mut documents = Vec::new();
        for scope in scopes {
            check(budget)?;
            let bytes = if native(scope) { native_docs[scope].as_bytes().to_vec() }
                else if scope.starts_with("runtime.") {
                    (*runtime_docs.get(scope.as_str()).ok_or_else(|| failure(503,"runtime_unavailable","Selected accepted runtime settings are unavailable."))?).clone()
                } else if scope == "device.annotations" {
                    annotations.ok_or_else(|| failure(503,"snapshot_unavailable","Selected device annotations are unavailable."))?.to_vec()
                } else {
                    // Frozen device activity history is RAM-only. Root supplies
                    // the existing private snapshot, not an invented flash file.
                    let history = metadata.get("deviceHistory").filter(|v| !v.is_null())
                        .ok_or_else(|| failure(503,"snapshot_unavailable","Selected device activity history is unavailable."))?;
                    encode_bounded(history,MAX_RUNTIME_BYTES,failure(413,"document_too_large","Selected device activity history exceeds its document limit."))?
                };
            if bytes.len() > document_limit(scope) { return Err(failure(413,"document_too_large","Selected configuration exceeds its document limit.")); }
            let content = std::str::from_utf8(&bytes).map_err(|_| invalid_backup())?;
            let mut document = json!({"module":scope,"content":content,"digest":sha256(&bytes)});
            if scope.starts_with("runtime.") {
                // Root supplies the actual per-service accepted generation. Do
                // not relabel a native generation as runtime ownership evidence.
                let runtime_generation = metadata.get("runtimeGenerations").and_then(|v| v.get(scope))
                    .or_else(|| metadata.get("runtimeGenerations").and_then(|v| v.get(scope.trim_start_matches("runtime."))));
                document["generation"] = json!(generation(runtime_generation.ok_or_else(|| failure(503,"runtime_unavailable","Selected runtime snapshot must identify its accepted generation."))?)?);
            }
            documents.push(document);
        }
        check(budget)?;
        let envelope = json!({"model":model,"build":build,"createdAt":timestamp(io.now_unix()),"generation":accepted_generation,"scopes":scopes,"documents":documents});
        let bytes = encode_bounded(&envelope,MAX_BACKUP_BYTES,failure(413,"backup_too_large","Selected settings exceed the 2 MiB JSON backup limit; export smaller groups."))?;
        decode(&bytes,budget)?;
        check(budget)?;
        Ok(bytes)
    }
    fn prune(&self) { self.previews.borrow_mut().retain(|_,p| Instant::now() < p.deadline); }
    /// `metadata.documents` is the private accepted snapshot from root. Unknown
    /// current bytes are reported uncompared, never an invented empty baseline.
    pub fn preview(&self, raw: &[u8], metadata: Value, generation: u64, budget: &Budget<'_>) -> Result<Value,ApiError> {
        check(budget)?;
        let envelope = decode(raw,budget)?;
        pending(&metadata)?;
        let (model,build) = metadata_valid(&metadata)?;
        if generation == 0 { return Err(failure(409,"generation_conflict","Preview requires the current accepted configuration generation.")); }
        if metadata.get("generation").is_some_and(|v| v.as_u64() != Some(generation)) { return Err(failure(409,"generation_conflict","Preview snapshot generation changed.")); }
        let mut current = current_documents(&metadata)?;
        for (scope,key) in [("device.annotations","deviceAnnotations"),("device.history","deviceHistory")] {
            if !current.contains_key(scope) {
                if let Some(value) = metadata.get(key).filter(|v| !v.is_null()) {
                    let bytes = encode_bounded(value,MAX_RUNTIME_BYTES,failure(413,"document_too_large","Current baseline exceeds its document limit."))?;
                    let content = String::from_utf8(bytes).map_err(|_| invalid_backup())?;
                    current.insert(scope.to_owned(),content);
                }
            }
        }
        let candidates = Value::Array(envelope.documents.iter().filter(|d| native(&d.module)).map(|d| json!({"module":d.module,"content":d.content})).collect());
        let validations = crate::product_configuration::preview_documents(&native_values(&current),&candidates)
            .map_err(|e| failure(e.status,e.code,e.message))?;
        let validations = validations.as_array().ok_or_else(|| failure(500,"maintenance_failed","Native preview returned an invalid result."))?;
        let mut summary = json!({"added":0,"modified":0,"deleted":0,"unchanged":0,"uncompared":0});
        let mut changes = Vec::new();
        let mismatch = envelope.model != model;
        let mut warnings = vec![issue("private_configuration","Backup files contain private configuration. Keep downloaded files private.")];
        if mismatch { warnings.push(issue("model_mismatch","Backup model differs from this router. Explicitly acknowledge compatibility before staging.")); }
        if envelope.build != build { warnings.push(issue("build_mismatch","Backup build differs from this router. Unknown native vendor data is preserved.")); }
        for document in &envelope.documents {
            check(budget)?;
            let before = current.get(&document.module);
            let kind = match before {
                None => "uncompared",
                Some(text) if text == &document.content => "unchanged",
                Some(text) if text.is_empty() => "added",
                Some(_) if document.content.is_empty() => "deleted",
                Some(_) => "modified",
            };
            let count = summary[kind].as_u64().unwrap_or(0);
            summary[kind] = json!(count+1);
            let mut errors = json!([]);
            let mut dependencies = json!([]);
            let mut risks = json!([]);
            let mut diff = String::new();
            let mut valid = false;
            if native(&document.module) {
                if let Some(v) = validations.iter().find(|v| v["module"].as_str() == Some(document.module.as_str())) {
                    valid = v["valid"].as_bool() == Some(true);
                    if before.is_some() { diff = v["diff"].as_str().unwrap_or("").to_owned(); }
                    errors = v.get("errors").cloned().unwrap_or_else(|| json!([]));
                    dependencies = v.get("dependencies").cloned().unwrap_or_else(|| json!([]));
                    risks = v.get("risks").cloned().unwrap_or_else(|| json!([]));
                }
            } else {
                errors = json!([issue("separate_restore_required","This scope is preview-only. Restore using its dedicated generation-checked configuration workflow.")]);
            }
            // Runtime, annotations and history bytes never enter the public diff.
            changes.push(json!({"module":document.module,"kind":kind,"beforeBytes":before.map_or(0,|s| s.len()),"afterBytes":document.content.len(),
                "beforeDigest":before.map_or_else(String::new,|s| sha256(s.as_bytes())),"afterDigest":document.digest,"diff":diff,
                "stageable":native(&document.module) && valid && kind != "unchanged" && kind != "uncompared","valid":valid,
                "errors":errors,"dependencies":dependencies,"risks":risks}));
        }
        let bytes = envelope.documents.iter().map(|d| d.content.len()).sum::<usize>() + current.iter().filter(|(m,_)| native(m)).map(|(_,c)|c.len()).sum::<usize>();
        self.prune();
        let mut previews = self.previews.borrow_mut();
        if previews.len() >= MAX_PREVIEWS || previews.values().map(|p|p.bytes).sum::<usize>().saturating_add(bytes) > MAX_PREVIEW_CONTENT_BYTES {
            return Err(failure(409,"preview_limit","Private preview limit reached; discard old previews or wait for expiry."));
        }
        let mut random = [0u8;16];
        getrandom::fill(&mut random).map_err(|_| failure(503,"preview_unavailable","Cannot allocate a private import preview."))?;
        let id = random.iter().map(|b|format!("{b:02x}")).collect::<String>();
        if previews.contains_key(&id) { return Err(failure(503,"preview_unavailable","Cannot allocate a private import preview.")); }
        check(budget)?;
        let now = Instant::now();
        let unix = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| failure(503,"preview_unavailable","Current time is unavailable."))?.as_secs();
        let output = json!({"id":id,"generation":generation,"sourceModel":envelope.model,"currentModel":model,"modelMismatch":mismatch,
            "expiresAt":timestamp(unix.saturating_add(PREVIEW_LIFETIME.as_secs())),"summary":summary,"changes":changes,"warnings":warnings});
        let current = current.into_iter().filter(|(module,_)| native(module)).collect();
        previews.insert(id,StoredPreview { envelope,current,generation,model_mismatch:mismatch,deadline:now+PREVIEW_LIFETIME,bytes });
        Ok(output)
    }
    /// Discard touches only the identified RAM preview, never drafts/backups.
    pub fn discard(&self,id: &str) -> bool { self.previews.borrow_mut().remove(id).is_some() }
    /// Root calls this with a freshly observed accepted generation, then stages
    /// the ordered native bundle using its single Configuration owner. Preview
    /// remains available until root explicitly discards after successful staging.
    pub fn candidates(&self,preview_id: &str,generation: u64,modules: &[String],ack_model: bool,budget: &Budget<'_>) -> Result<Vec<ImportCandidate>,ApiError> {
        check(budget)?;
        self.prune();
        let previews = self.previews.borrow();
        let saved = previews.get(preview_id).ok_or_else(|| failure(404,"preview_not_found","Import preview expired or was discarded; preview the backup again."))?;
        if generation != saved.generation { return Err(failure(409,"generation_conflict","Configuration changed after preview; preview the backup again.")); }
        if saved.model_mismatch && !ack_model { return Err(failure(409,"model_mismatch","Explicitly acknowledge the router model mismatch before staging.")); }
        let chosen: BTreeSet<&str> = modules.iter().map(String::as_str).collect();
        if chosen.is_empty() || chosen.len() != modules.len() || chosen.len() > 6 || chosen.iter().any(|m| !native(m)) {
            return Err(failure(400,"invalid_selection","Select unique changed native groups; other scopes use dedicated restoration workflows."));
        }
        let mut selected = Vec::new();
        for module in NATIVE_SCOPES {
            if !chosen.contains(module) { continue; }
            let document = saved.envelope.documents.iter().find(|d| d.module == module).ok_or_else(|| failure(400,"invalid_selection","A selected configuration group is missing from this backup."))?;
            let before = saved.current.get(module).ok_or_else(|| failure(409,"snapshot_unavailable","The selected native group was not compared against accepted configuration."))?;
            if before == &document.content { return Err(failure(422,"no_changes","Only changed native configuration groups may be staged.")); }
            selected.push(ImportCandidate { module:module.to_owned(),content:document.content.clone() });
        }
        let values = Value::Array(selected.iter().map(|d|json!({"module":d.module,"content":d.content})).collect());
        let assessments = crate::product_configuration::preview_documents(&native_values(&saved.current),&values).map_err(|e|failure(e.status,e.code,e.message))?;
        let assessments = assessments.as_array().ok_or_else(|| failure(500,"maintenance_failed","Native preview returned an invalid result."))?;
        if assessments.len() != selected.len() || assessments.iter().any(|v|v["valid"].as_bool() != Some(true)) {
            return Err(failure(422,"invalid_candidate","A selected native document has validation errors."));
        }
        if assessments.iter().any(|v|v["dependencies"].as_array().is_some_and(|a|!a.is_empty())) {
            return Err(failure(422,"invalid_reference","Selected groups omit a required native dependency."));
        }
        check(budget)?;
        if Instant::now() >= saved.deadline {
            return Err(failure(404,"preview_not_found","Import preview expired; preview the backup again."));
        }
        Ok(selected)
    }
}

// FRPC form edits retain opaque TOML source spans. Root commits the returned
// private text only through the existing fixed RuntimeManager native checker.
struct Statement { start: usize, end: usize, value_start: usize, value_end: usize, path: Vec<String>, mapping: Option<usize> }
struct Table { end: usize, path: Vec<String>, mapping: Option<usize> }
struct Mapping { start: usize, end: usize }
struct FrpcDocument { statements: Vec<Statement>, tables: Vec<Table>, mappings: Vec<Mapping>, input: Value, has_token: bool }
fn string_end(raw: &str, start: usize) -> Result<usize, ApiError> {
    let b = raw.as_bytes();
    let quote = *b.get(start).ok_or_else(invalid_frpc)?;
    if !matches!(quote, b'\'' | b'"') { return Err(invalid_frpc()); }
    let mut p = start + 1;
    while p < b.len() {
        if b[p] == quote { return Ok(p + 1); }
        if b[p] == b'\\' && quote == b'"' { p += 2; continue; }
        if matches!(b[p], b'\n' | b'\r') { return Err(invalid_frpc()); }
        p += 1;
    }
    Err(invalid_frpc())
}
fn toml_string(text: &str) -> Result<String, ApiError> {
    let text = text.trim();
    if text.starts_with("\"\"\"") || text.starts_with("'''") || string_end(text, 0)? != text.len() { return Err(invalid_frpc()); }
    if text.starts_with('\'') {
        let value = &text[1..text.len() - 1];
        if value.chars().any(|c| c.is_control() && c != '\t') { return Err(invalid_frpc()); }
        return Ok(value.to_owned());
    }
    let mut output = String::new();
    let mut chars = text[1..text.len() - 1].chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            if c.is_control() && c != '\t' { return Err(invalid_frpc()); }
            output.push(c); continue;
        }
        match chars.next().ok_or_else(invalid_frpc)? {
            'b' => output.push('\u{0008}'), 't' => output.push('\t'), 'n' => output.push('\n'),
            'f' => output.push('\u{000c}'), 'r' => output.push('\r'), '"' => output.push('"'), '\\' => output.push('\\'),
            kind @ ('u' | 'U') => {
                let count = if kind == 'u' { 4 } else { 8 };
                let mut hex = String::with_capacity(count);
                for _ in 0..count { hex.push(chars.next().ok_or_else(invalid_frpc)?); }
                if !hex.bytes().all(|b| b.is_ascii_hexdigit()) { return Err(invalid_frpc()); }
                output.push(char::from_u32(u32::from_str_radix(&hex, 16).map_err(|_| invalid_frpc())?).ok_or_else(invalid_frpc)?);
            }
            _ => return Err(invalid_frpc()),
        }
    }
    Ok(output)
}
fn path_parts(text: &str) -> Result<Vec<String>, ApiError> {
    let b = text.as_bytes(); let mut p = 0; let mut parts = Vec::new();
    loop {
        while b.get(p).is_some_and(|c| matches!(*c, b' ' | b'\t')) { p += 1; }
        let start = p;
        if b.get(p).is_some_and(|c| matches!(*c, b'"' | b'\'')) {
            p = string_end(text, p)?; parts.push(toml_string(&text[start..p])?);
        } else {
            while b.get(p).is_some_and(|c| c.is_ascii_alphanumeric() || matches!(*c, b'_' | b'-')) { p += 1; }
            if p == start { return Err(invalid_frpc()); }
            parts.push(text[start..p].to_owned());
        }
        if parts.len() > 16 { return Err(invalid_frpc()); }
        while b.get(p).is_some_and(|c| matches!(*c, b' ' | b'\t')) { p += 1; }
        if p == b.len() { return Ok(parts); }
        if b[p] != b'.' { return Err(invalid_frpc()); } p += 1;
    }
}
fn toml_scalar(text: &str) -> Result<Value, ApiError> {
    let text = text.trim();
    if text.starts_with('"') || text.starts_with('\'') { return toml_string(text).map(Value::String); }
    if text == "true" { return Ok(Value::Bool(true)); } if text == "false" { return Ok(Value::Bool(false)); }
    if text.starts_with('[') {
        let b = text.as_bytes(); let mut p = 1; let mut items = Vec::new(); let mut expect_value = true;
        loop {
            while b.get(p).is_some_and(u8::is_ascii_whitespace) { p += 1; }
            if b.get(p) == Some(&b'#') { p = text[p..].find('\n').map_or(b.len(), |n| p + n + 1); continue; }
            if b.get(p) == Some(&b']') && p + 1 == b.len() { return Ok(Value::Array(items)); }
            if !expect_value || !b.get(p).is_some_and(|c| matches!(*c, b'"' | b'\'')) { return Err(invalid_frpc()); }
            let end = string_end(text, p)?; items.push(Value::String(toml_string(&text[p..end])?));
            if items.len() > 64 { return Err(invalid_frpc()); } p = end;
            while b.get(p).is_some_and(u8::is_ascii_whitespace) { p += 1; }
            if b.get(p) == Some(&b',') { p += 1; expect_value = true; }
            else if matches!(b.get(p), Some(b']') | Some(b'#')) { expect_value = false; }
            else { return Err(invalid_frpc()); }
        }
    }
    let clean = text.replace('_', "");
    if clean.is_empty() || text.starts_with('_') || text.ends_with('_') || text.contains("__") { return Err(invalid_frpc()); }
    let (negative, unsigned) = if let Some(s) = clean.strip_prefix('-') { (true, s) } else { (false, clean.strip_prefix('+').unwrap_or(&clean)) };
    let (base, digits) = if let Some(s) = unsigned.strip_prefix("0x") { (16, s) } else if let Some(s) = unsigned.strip_prefix("0o") { (8, s) } else if let Some(s) = unsigned.strip_prefix("0b") { (2, s) } else { (10, unsigned) };
    if digits.is_empty() || !digits.chars().all(|c| c.is_digit(base)) || (base == 10 && digits.len() > 1 && digits.starts_with('0')) { return Err(invalid_frpc()); }
    let integer = i64::from_str_radix(digits, base).map_err(|_| invalid_frpc())?;
    Ok(json!(if negative { -integer } else { integer }))
}
fn scan_frpc(raw: &str) -> Result<FrpcDocument, ApiError> {
    if raw.len() > MAX_TOML_BYTES || raw.contains('\0') { return Err(invalid_frpc()); }
    let b = raw.as_bytes(); let mut p = 0; let mut statements = Vec::new();
    let mut tables = vec![Table { end: b.len(), path: Vec::new(), mapping: None }];
    let mut mappings: Vec<Mapping> = Vec::new();
    let mut seen: BTreeSet<(String, Vec<String>)> = BTreeSet::new();
    let mut seen_tables: BTreeSet<(String, Vec<String>)> = BTreeSet::new();
    let mut namespace = "root".to_owned();
    let mut opaque_arrays = BTreeMap::<Vec<String>, usize>::new();
    let mut opaque_context: Option<(Vec<String>, usize)> = None;
    while p < b.len() {
        if statements.len() + tables.len() > MAX_STATEMENTS { return Err(invalid_frpc()); }
        let start = p;
        while b.get(p).is_some_and(|c| matches!(*c, b' ' | b'\t' | b'\r')) { p += 1; }
        if p == b.len() { break; }
        if matches!(b[p], b'#' | b'\n') { p = raw[p..].find('\n').map_or(b.len(), |n| p + n + 1); continue; }
        if b[p] == b'[' {
            let array = b.get(p + 1) == Some(&b'['); let path_start = p + if array { 2 } else { 1 }; let mut q = path_start;
            while q < b.len() {
                if matches!(b[q], b'"' | b'\'') { q = string_end(raw, q)?; continue; }
                if b[q] == b']' { break; }
                if matches!(b[q], b'\n' | b'\r' | b'#') { return Err(invalid_frpc()); } q += 1;
            }
            if q == b.len() || (array && b.get(q + 1) != Some(&b']')) { return Err(invalid_frpc()); }
            let path = path_parts(raw[path_start..q].trim())?; q += if array { 2 } else { 1 };
            while b.get(q).is_some_and(|c| matches!(*c, b' ' | b'\t' | b'\r')) { q += 1; }
            if b.get(q) == Some(&b'#') { q = raw[q..].find('\n').map_or(b.len(), |n| q + n); }
            if q < b.len() && b[q] != b'\n' { return Err(invalid_frpc()); } if q < b.len() { q += 1; }
            let old_mapping = tables.last().and_then(|t| t.mapping); let mut mapping = None;
            if path.first().is_some_and(|s| s == "proxies") {
                if path.len() == 1 && array {
                    if mappings.len() >= 64 { return Err(invalid_frpc()); }
                    mapping = Some(mappings.len()); mappings.push(Mapping { start, end: b.len() });
                } else if path.len() > 1 && !array && old_mapping.is_some() { mapping = old_mapping; }
                else { return Err(invalid_frpc()); }
                namespace = format!("proxy:{}", mapping.unwrap_or(0)); opaque_context = None;
            } else {
                if array && path.first().is_some_and(|s| s == "auth" || s == "transport") { return Err(invalid_frpc()); }
                if array {
                    let index = opaque_arrays.entry(path.clone()).or_default(); *index += 1;
                    namespace = format!("opaque:{}:{index}", serde_json::to_string(&path).map_err(|_| invalid_frpc())?);
                    opaque_context = Some((path.clone(), *index));
                } else if let Some((parent, index)) = &opaque_context {
                    if path.starts_with(parent) { namespace = format!("opaque:{}:{index}", serde_json::to_string(parent).map_err(|_| invalid_frpc())?); }
                    else { namespace = "root".to_owned(); opaque_context = None; }
                } else { namespace = "root".to_owned(); }
            }
            if !array && !seen_tables.insert((namespace.clone(), path.clone())) { return Err(invalid_frpc()); }
            if let Some(last) = tables.last_mut() { last.end = start; }
            if let Some(old) = old_mapping { if Some(old) != mapping { mappings[old].end = start; } }
            tables.push(Table { end: b.len(), path, mapping }); p = q; continue;
        }
        let key_start = p;
        while p < b.len() {
            if matches!(b[p], b'"' | b'\'') { p = string_end(raw, p)?; continue; }
            if b[p] == b'=' { break; } if matches!(b[p], b'\n' | b'#' | b'\r') { return Err(invalid_frpc()); } p += 1;
        }
        if p == b.len() { return Err(invalid_frpc()); }
        let table = tables.last().ok_or_else(invalid_frpc)?; let mut path = table.path.clone(); path.extend(path_parts(raw[key_start..p].trim())?); let mapping = table.mapping;
        // Prefix collisions make assignment identity ambiguous. Check at most
        // sixteen ancestors plus the next ordered key, not a quadratic scan.
        if (1..=path.len()).any(|n| seen.contains(&(namespace.clone(),path[..n].to_vec()))) {
            return Err(invalid_frpc());
        }
        if seen.range((namespace.clone(),path.clone())..).next()
            .is_some_and(|(ns,key)| ns == &namespace && key.starts_with(&path)) {
            return Err(invalid_frpc());
        }
        seen.insert((namespace.clone(), path.clone())); p += 1;
        while b.get(p).is_some_and(|c| matches!(*c, b' ' | b'\t')) { p += 1; }
        let value_start = p; let mut value_end = p; let mut quote = None; let mut triple = false; let mut stack = Vec::new();
        while p < b.len() {
            let c = b[p];
            if let Some(q) = quote {
                if c == b'\\' && q == b'"' { p += 2; continue; }
                if c == q && (!triple || b.get(p..p + 3).is_some_and(|v| v == [q, q, q])) { p += if triple { 3 } else { 1 }; quote = None; value_end = p; continue; }
                if !triple && matches!(c, b'\n' | b'\r') { return Err(invalid_frpc()); }
            } else if matches!(c, b'"' | b'\'') {
                quote = Some(c); triple = b.get(p..p + 3).is_some_and(|v| v == [c, c, c]); p += if triple { 3 } else { 1 }; continue;
            } else if matches!(c, b'[' | b'{') { stack.push(c); if stack.len() > 16 { return Err(invalid_frpc()); } }
            else if matches!(c, b']' | b'}') { if stack.pop() != Some(if c == b']' { b'[' } else { b'{' }) { return Err(invalid_frpc()); } }
            else if c == b'#' {
                if stack.is_empty() { value_end = p; p = raw[p..].find('\n').map_or(b.len(), |n| p + n + 1); break; }
                p = raw[p..].find('\n').map_or(b.len(), |n| p + n); continue;
            } else if c == b'\n' && stack.is_empty() { value_end = p; p += 1; break; }
            p += 1; value_end = p;
        }
        if quote.is_some() || !stack.is_empty() || p > b.len() { return Err(invalid_frpc()); }
        while value_end > value_start && b[value_end - 1].is_ascii_whitespace() { value_end -= 1; }
        if value_end == value_start { return Err(invalid_frpc()); }
        statements.push(Statement { start, end: p, value_start, value_end, path, mapping });
    }
    let fields = |mapping: Option<usize>| -> Result<BTreeMap<String, Value>, ApiError> {
        let mut values = BTreeMap::new();
        for statement in &statements {
            if statement.mapping != mapping { continue; }
            let parts = &statement.path[if mapping.is_some() { 1 } else { 0 }..];
            if parts.iter().any(|s| s.contains('.')) { continue; } let name = parts.join(".");
            if ["auth", "transport", "proxies", "auth.oidc", "transport.tls"].contains(&name.as_str()) { return Err(invalid_frpc()); }
            let modeled = if mapping.is_some() { ["name", "type", "localIP", "localPort", "remotePort", "customDomains"].contains(&name.as_str()) }
                else { ["serverAddr", "serverPort", "transport.protocol", "transport.tls.enable", "auth.method", "auth.token"].contains(&name.as_str()) };
            if modeled { values.insert(name, toml_scalar(&raw[statement.value_start..statement.value_end])?); }
        }
        Ok(values)
    };
    let typed = |values: &BTreeMap<String, Value>, name: &str, fallback: Value| -> Result<Value, ApiError> {
        let value = values.get(name).cloned().unwrap_or(fallback.clone());
        if (fallback.is_string() && !value.is_string()) || (fallback.is_boolean() && !value.is_boolean()) || (fallback.is_number() && !value.is_i64()) || (fallback.is_array() && !value.is_array()) { return Err(invalid_frpc()); } Ok(value)
    };
    let root = fields(None)?; let transport = typed(&root, "transport.protocol", json!("tcp"))?;
    if !["tcp", "quic"].contains(&transport.as_str().ok_or_else(invalid_frpc)?) || typed(&root, "auth.method", json!("token"))? != json!("token") { return Err(invalid_frpc()); }
    let token = typed(&root, "auth.token", json!(""))?; let mut proxies = Vec::new();
    for index in 0..mappings.len() {
        let values = fields(Some(index))?; let kind = typed(&values, "type", json!("tcp"))?;
        if !["tcp", "udp", "http", "https"].contains(&kind.as_str().ok_or_else(invalid_frpc)?) { return Err(invalid_frpc()); }
        let mut proxy = json!({"sourceId":format!("accepted-{index}"),"name":typed(&values,"name",json!(""))?,"type":kind,
            "localAddress":typed(&values,"localIP",json!("127.0.0.1"))?,"localPort":typed(&values,"localPort",json!(0))?});
        if kind == json!("tcp") || kind == json!("udp") { proxy["remotePort"] = typed(&values, "remotePort", json!(0))?; }
        else { let domains = typed(&values, "customDomains", json!([]))?; if domains.as_array().is_none_or(|a| a.iter().any(|v| !v.is_string())) { return Err(invalid_frpc()); } proxy["domains"] = domains; }
        proxies.push(proxy);
    }
    let input = json!({"serverAddress":typed(&root,"serverAddr",json!(""))?,"serverPort":typed(&root,"serverPort",json!(7000))?,"transport":transport,
        "tls":typed(&root,"transport.tls.enable",json!(true))?,"proxies":proxies});
    Ok(FrpcDocument { statements, tables, mappings, input, has_token: token.as_str().is_some_and(|s| !s.is_empty()) })
}
/// Public field projection excludes saved tokens, source, opaque extensions,
/// and any process-derived tunnel/TLS claims. Unsupported shapes are read-only.
pub fn frpc_projection(raw: &str) -> Result<Value, ApiError> {
    if raw.len() > MAX_TOML_BYTES { return Err(failure(413,"document_too_large","FRPC configuration exceeds its editor limit.")); }
    match scan_frpc(raw) {
        Ok(document) => Ok(json!({"supported":true,"input":document.input,"hasToken":document.has_token})),
        Err(_) => Ok(json!({"supported":false,"reason":"Use the native configuration editor for this TOML shape."})),
    }
}

fn valid_address(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 253 && !s.chars().any(|c| c.is_whitespace() || c.is_control() || matches!(c, '/' | '\\' | '"' | '\'' | '?' | '#' | '@'))
}
fn valid_domain(s: &str) -> bool {
    let host = s.strip_prefix("*.").unwrap_or(s);
    !host.is_empty() && s.len() <= 253 && host.split('.').all(|label| !label.is_empty() && label.len() <= 63 && !label.starts_with('-') && !label.ends_with('-') && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
}
fn valid_port(v: &Value) -> bool { v.as_u64().is_some_and(|p| (1..=65535).contains(&p)) }
fn name_valid(v: &Value) -> bool { v.as_str().is_some_and(|s| !s.trim().is_empty() && s.chars().count() <= 64) }
fn frpc_fields(value: &Value, required: &[&str], optional: &[&str]) -> Result<(), ApiError> { exact_fields(value, required, optional).map_err(|_| invalid_input()) }
fn validate_frpc_changes(original: &Value, input: &Value) -> Result<(), ApiError> {
    frpc_fields(input, &["serverAddress", "serverPort", "transport", "tls", "proxies"], &[])?;
    if !input["serverAddress"].is_string() || !input["serverPort"].is_i64() || !input["tls"].is_boolean() || !["tcp", "quic"].contains(&input["transport"].as_str().ok_or_else(invalid_input)?) { return Err(invalid_input()); }
    if original["serverAddress"] != input["serverAddress"] && !valid_address(input["serverAddress"].as_str().unwrap_or("")) { return Err(invalid_input()); }
    if original["serverPort"] != input["serverPort"] && !valid_port(&input["serverPort"]) { return Err(invalid_input()); }
    let proxies = input["proxies"].as_array().ok_or_else(invalid_input)?;
    if proxies.len() > 64 { return Err(invalid_input()); }
    let originals = original["proxies"].as_array().ok_or_else(invalid_input)?;
    let mut ids = BTreeSet::new(); let mut names = BTreeSet::new();
    for proxy in proxies {
        frpc_fields(proxy, &["name", "type", "localAddress", "localPort"], &["remotePort", "domains", "sourceId"])?;
        let kind = proxy["type"].as_str().ok_or_else(invalid_input)?;
        if !["tcp", "udp", "http", "https"].contains(&kind) || !proxy["name"].is_string() || !proxy["localAddress"].is_string() || !proxy["localPort"].is_i64() || !names.insert(proxy["name"].as_str().ok_or_else(invalid_input)?) { return Err(invalid_input()); }
        let previous = if let Some(id) = proxy.get("sourceId") {
            let id = id.as_str().ok_or_else(invalid_input)?;
            if !ids.insert(id) { return Err(invalid_input()); }
            Some(originals.iter().find(|v| v["sourceId"].as_str() == Some(id)).ok_or_else(invalid_input)?)
        } else { None };
        let changed = |key: &str| previous.is_none_or(|v| v[key] != proxy[key]);
        if changed("name") && !name_valid(&proxy["name"]) || changed("localAddress") && !valid_address(proxy["localAddress"].as_str().unwrap_or("")) || changed("localPort") && !valid_port(&proxy["localPort"]) { return Err(invalid_input()); }
        if kind == "tcp" || kind == "udp" {
            if proxy.get("domains").is_some() || !proxy["remotePort"].is_i64() || changed("remotePort") && !valid_port(&proxy["remotePort"]) { return Err(invalid_input()); }
        } else {
            if proxy.get("remotePort").is_some() { return Err(invalid_input()); }
            let domains = proxy["domains"].as_array().ok_or_else(invalid_input)?;
            if domains.iter().any(|v| !v.is_string()) || changed("domains") && (domains.is_empty() || domains.len() > 64 || domains.iter().any(|v| !valid_domain(v.as_str().unwrap_or("")))) { return Err(invalid_input()); }
        }
    }
    Ok(())
}
fn encoded(value: &Value) -> Result<String, ApiError> {
    match value {
        Value::String(s) => {
            let mut result = String::from("\"");
            for c in s.chars() {
                match c {
                    '"' => result.push_str("\\\""), '\\' => result.push_str("\\\\"), '\n' => result.push_str("\\n"), '\r' => result.push_str("\\r"), '\t' => result.push_str("\\t"), '\u{0008}' => result.push_str("\\b"), '\u{000c}' => result.push_str("\\f"),
                    c if c.is_control() || c == '\u{2028}' || c == '\u{2029}' => result.push_str(&format!("\\u{:04X}", c as u32)),
                    c => result.push(c),
                }
            }
            result.push('"'); Ok(result)
        }
        Value::Bool(_) | Value::Number(_) => Ok(value.to_string()),
        Value::Array(a) => Ok(format!("[{}]", a.iter().map(encoded).collect::<Result<Vec<_>, _>>()?.join(", "))),
        _ => Err(invalid_input()),
    }
}
fn modeled(input: &Value, index: Option<usize>) -> BTreeMap<String, Value> {
    if let Some(index) = index {
        let p = &input["proxies"][index];
        let mut fields = BTreeMap::from([("name".to_owned(), p["name"].clone()), ("type".to_owned(), p["type"].clone()), ("localIP".to_owned(), p["localAddress"].clone()), ("localPort".to_owned(), p["localPort"].clone())]);
        if p["type"] == json!("tcp") || p["type"] == json!("udp") { fields.insert("remotePort".to_owned(), p["remotePort"].clone()); }
        else { fields.insert("customDomains".to_owned(), p["domains"].clone()); } fields
    } else { BTreeMap::from([("serverAddr".to_owned(), input["serverAddress"].clone()), ("serverPort".to_owned(), input["serverPort"].clone()), ("transport.protocol".to_owned(), input["transport"].clone()), ("transport.tls.enable".to_owned(), input["tls"].clone())]) }
}
struct TextEdit { start: usize, end: usize, text: String }
fn patch_field(document: &FrpcDocument, raw: &str, path: Vec<String>, value: Option<&Value>, mapping: Option<usize>, edits: &mut Vec<TextEdit>, inserts: &mut BTreeMap<usize, Vec<String>>, newline: &str) -> Result<(), ApiError> {
    if let Some(statement) = document.statements.iter().find(|s| s.mapping == mapping && s.path == path) {
        if let Some(value) = value {
            if toml_scalar(&raw[statement.value_start..statement.value_end])? == *value { return Ok(()); }
            edits.push(TextEdit { start: statement.value_start, end: statement.value_end, text: encoded(value)? });
        } else { edits.push(TextEdit { start: statement.start, end: statement.end, text: String::new() }); } return Ok(());
    }
    let Some(value) = value else { return Ok(()); };
    let table = document.tables.iter().filter(|t| t.mapping == mapping && t.path.len() < path.len() && path.starts_with(&t.path)).max_by_key(|t| t.path.len()).ok_or_else(invalid_frpc)?;
    let key = path[table.path.len()..].iter().map(|s| if !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-')) { Ok(s.clone()) } else { encoded(&json!(s)) }).collect::<Result<Vec<_>, _>>()?.join(".");
    inserts.entry(table.end).or_default().push(format!("{key} = {}{newline}", encoded(value)?)); Ok(())
}
/// Input: `{ "input": FrpcPlanInput, "token"?: {"mode":
/// "preserve"|"replace"|"clear"|"delete", "value"?: string} }`.
/// Omitted intent preserves accepted credentials; replace requires a nonempty
/// value; clear/delete removes only auth.token. Private extensions survive.
/// Root must use RuntimeManager's fixed checker and generation-checked commit;
/// this pure edit does not start frpc or validate remote authentication/TLS.
pub fn edit_frpc(raw: &str, input: &[u8]) -> Result<String, ApiError> {
    if input.len() > MAX_TOML_BYTES { return Err(failure(413,"body_too_large","FRPC editor input exceeds its limit.")); }
    let request = strict_json(input, None).map_err(|_| invalid_input())?;
    frpc_fields(&request, &["input"], &["token"])?;
    if raw.len() > MAX_TOML_BYTES { return Err(failure(413,"document_too_large","FRPC configuration exceeds its editor limit.")); }
    let document = scan_frpc(raw)?; let next = &request["input"];
    validate_frpc_changes(&document.input, next)?;
    let (mode, token) = if let Some(intent) = request.get("token") {
        frpc_fields(intent, &["mode"], &["value"])?;
        (intent["mode"].as_str().ok_or_else(invalid_input)?, intent.get("value").map(|v| v.as_str().ok_or_else(invalid_input)).transpose()?.unwrap_or(""))
    } else { ("preserve", "") };
    if !["preserve", "replace", "clear", "delete"].contains(&mode) || mode == "replace" && token.is_empty() || mode != "replace" && !token.is_empty() || token.len() > MAX_RUNTIME_BYTES { return Err(invalid_input()); }
    let newline = if raw.contains("\r\n") { "\r\n" } else { "\n" }; let mut edits = Vec::new(); let mut inserts = BTreeMap::new();
    let root = modeled(next, None); let before = modeled(&document.input, None);
    for (name, value) in &root { if before.get(name) != Some(value) { patch_field(&document, raw, name.split('.').map(str::to_owned).collect(), Some(value), None, &mut edits, &mut inserts, newline)?; } }
    if mode == "replace" {
        patch_field(&document, raw, vec!["auth".to_owned(), "method".to_owned()], Some(&json!("token")), None, &mut edits, &mut inserts, newline)?;
        patch_field(&document, raw, vec!["auth".to_owned(), "token".to_owned()], Some(&json!(token)), None, &mut edits, &mut inserts, newline)?;
    } else if mode == "clear" || mode == "delete" { patch_field(&document, raw, vec!["auth".to_owned(), "token".to_owned()], None, None, &mut edits, &mut inserts, newline)?; }
    let proxies = next["proxies"].as_array().ok_or_else(invalid_input)?;
    for (index, mapping) in document.mappings.iter().enumerate() {
        let id = format!("accepted-{index}"); let next_index = proxies.iter().position(|p| p["sourceId"].as_str() == Some(id.as_str()));
        let Some(next_index) = next_index else { edits.push(TextEdit { start: mapping.start, end: mapping.end, text: String::new() }); continue; };
        let original = modeled(&document.input, Some(index)); let updated = modeled(next, Some(next_index));
        let keys: BTreeSet<&str> = original.keys().chain(updated.keys()).map(String::as_str).collect();
        for key in keys { if original.get(key) != updated.get(key) { patch_field(&document, raw, vec!["proxies".to_owned(), key.to_owned()], updated.get(key), Some(index), &mut edits, &mut inserts, newline)?; } }
    }
    for (index, proxy) in proxies.iter().enumerate() {
        if proxy.get("sourceId").is_some() { continue; }
        let mut lines = format!("{newline}[[proxies]]{newline}");
        for (key, value) in modeled(next, Some(index)) { lines.push_str(&format!("{key} = {}{newline}", encoded(&value)?)); }
        inserts.entry(raw.len()).or_default().push(lines);
    }
    for (position, lines) in inserts {
        let prefix = if position > 0 && !raw[..position].ends_with('\n') { newline } else { "" };
        edits.push(TextEdit { start: position, end: position, text: format!("{prefix}{}", lines.concat()) });
    }
    edits.sort_by(|a, b| b.start.cmp(&a.start).then(b.end.cmp(&a.end)));
    let mut previous_start = raw.len() + 1;
    for edit in &edits {
        if edit.end > previous_start || edit.start > edit.end || edit.end > raw.len() { return Err(invalid_frpc()); } previous_start = edit.start;
    }
    let mut result = raw.to_owned();
    for edit in edits { result.replace_range(edit.start..edit.end, &edit.text); }
    if result.len() > MAX_TOML_BYTES { return Err(failure(413,"document_too_large","FRPC configuration exceeds its editor limit.")); }
    let verified = scan_frpc(&result)?;
    // Mapping source IDs are positional after explicit deletion. Compare fields.
    let normalize = |value: &Value| {
        let mut value = value.clone();
        if let Some(ps) = value["proxies"].as_array_mut() { for p in ps { if let Some(object) = p.as_object_mut() { object.remove("sourceId"); } } } value
    };
    if normalize(&verified.input) != normalize(next) { return Err(invalid_frpc()); } Ok(result)
}

#[cfg(test)]
mod preview_expiry_tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    #[test]
    fn expiry_at_exact_deadline_prunes_private_content_without_touching_storage() {
        let maintenance = Maintenance::open(Path::new("/synthetic/private")).unwrap();
        maintenance.previews.borrow_mut().insert("synthetic-expired".into(), StoredPreview {
            envelope: Envelope { model:"RN02".into(), build:"fixture".into(), created_at:"2026-10-03T00:00:00Z".into(),
                generation:7, scopes:vec!["system".into()], documents:vec![Document {
                    module:"system".into(), content:"synthetic-private-config".into(), digest:String::new(), generation:0 }] },
            current:BTreeMap::new(), generation:7, model_mismatch:false, deadline:Instant::now(), bytes:24,
        });
        let cancel = AtomicBool::new(false);
        let budget = Budget { deadline:Instant::now()+Duration::from_secs(1), cancel:&cancel };
        assert_eq!(maintenance.candidates("synthetic-expired",7,&["system".into()],false,&budget).unwrap_err().code,"preview_not_found");
        assert!(maintenance.previews.borrow().is_empty());
    }
}

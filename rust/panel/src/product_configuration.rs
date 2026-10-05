//! Lossless native UCI drafts and durable, caller-supervised transactions.
//!
//! Raw documents are never normalized. Only commit/rollback replace native files.
//! The legacy state.json and journal.json remain the durable source of truth.
//! A caller must run tick on its serial server lane; no background thread exists.
use crate::http::Method;
use crate::product_io::{Backend, Error as IoError, Program, timestamp};
use crate::readiness_tun::Budget;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{CString, CStr};
use std::fmt;
use std::fs::{self, File, OpenOptions, Permissions};
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

pub const MAX_DOCUMENT_BYTES: usize = 128 << 10;
pub const MAX_DRAFTS: usize = 12;
pub const MAX_DRAFT_CONTENT_BYTES: usize = 512 << 10;
pub const MAX_STORE_BYTES: usize = 3 << 20;
pub const CONFIRMATION_SECONDS: u64 = 120;
pub const FREE_HEADROOM_BYTES: u64 = 1 << 20;
const MODULES: [&str; 6] = ["network", "wireless", "dhcp", "firewall", "system", "dropbear"];
const OUTPUT_LIMIT: usize = 256 << 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApiError { pub status: u16, pub code: &'static str, pub message: &'static str }
impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}: {}", self.code, self.message) }
}
impl std::error::Error for ApiError {}
fn failure(code: &'static str) -> ApiError {
    let (status, message) = match code {
        "generation_conflict" => (409, "Configuration changed; refresh and stage new drafts."),
        "pending_confirmation" | "confirmation_pending" => (409, "Confirm or roll back the pending operation first."),
        "risk_ack_required" | "risk_acknowledgement_required" => (409, "Acknowledge native connectivity risks before committing."),
        "operation_busy" | "manager_busy" => (409, "Another configuration operation owns this store."),
        "draft_not_found" => (404, "Private draft does not exist."),
        "operation_not_found" => (404, "Configuration operation does not exist."),
        "not_found" => (404, "Configuration endpoint does not exist."),
        "method_not_allowed" => (405, "Method is not supported by this configuration endpoint."),
        "document_too_large" | "body_too_large" => (413, "Native configuration exceeds the document limit."),
        "module_not_allowed" => (400, "This native configuration module is not editable."),
        "invalid_request" => (400, "Configuration request is invalid."),
        "invalid_commit" | "duplicate_module" => (400, "Select one unique draft per changed module."),
        "no_changes" => (400, "The selected drafts have no native configuration changes."),
        "not_pending" => (409, "This operation is not awaiting reachability confirmation."),
        "draft_limit" => (409, "Remove old drafts before staging more configuration."),
        "validation_failed" | "invalid_candidate" => (422, "Selected native configuration is invalid."),
        "invalid_reference" => (422, "Selected configuration contains an unresolved native reference."),
        "execution_hook_not_allowed" => (422, "User-selected firewall script includes are not supported."),
        "factory_include_removed" => (422, "Factory-owned firewall includes must be preserved."),
        "protocol_not_registered" => (422, "New network protocols must use a supported native protocol."),
        "management_migration_unavailable" => (422, "Preserve the current LAN management address and bridge."),
        "rescue_access_required" => (422, "Preserve existing SSH recovery access on ports 22 and 2222."),
        "uci_validation_failed" => (422, "Native UCI rejected the isolated candidate."),
        "validation_unavailable" => (503, "Native UCI validation is unavailable."),
        "reload_unsupported" => (422, "This module has no supported native reload operation."),
        "system_reload_unsafe" => (422, "Factory initialization must be complete before system reload."),
        "reload_failed" => (500, "A fixed native module reload failed."),
        "verification_failed" => (500, "Native configuration readback did not match the transaction."),
        "apply_failed" => (500, "Cannot replace a native configuration document."),
        "rollback_failed" => (500, "Configuration recovery must succeed before new changes."),
        "storage_insufficient" => (507, "Persistent storage needs free space for safe configuration recovery."),
        "state_corrupt" => (500, "Private configuration state is invalid; existing files were retained."),
        "journal_corrupt" => (500, "Rollback journal is invalid; existing files were retained."),
        "unsafe_data_dir" => (500, "Configuration storage must be a private directory outside native UCI."),
        "document_unavailable" => (500, "Cannot read a bounded native configuration document."),
        "cancelled" => (409, "Configuration operation was cancelled."),
        "deadline" => (504, "Configuration operation exceeded its deadline."),
        _ => (500, "Cannot persist private configuration state."),
    };
    ApiError { status, code, message }
}
fn budget_check(budget: &Budget<'_>) -> Result<(), ApiError> {
    budget.check().map_err(|e| match e {
        crate::readiness_tun::TunError::Cancelled => failure("cancelled"),
        _ => failure("deadline"),
    })
}
fn io_failure(error: IoError, fallback: &'static str) -> ApiError {
    match error { IoError::Cancelled => failure("cancelled"), IoError::Deadline => failure("deadline"), _ => failure(fallback) }
}
fn allowed(module: &str) -> bool { MODULES.contains(&module) }
fn safe_id(id: &str) -> bool { id.len() == 32 && id.bytes().all(|c| c.is_ascii_hexdigit()) }
fn random_id() -> Result<String, ApiError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| failure("storage_failed"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

// Private typed disk records deliberately do not implement Debug.
#[derive(Clone, Serialize, Deserialize)]
struct Issue { code: String, message: String }
fn issue(code: &str, message: &str) -> Issue { Issue { code: code.into(), message: message.into() } }
fn error_issue(error: ApiError) -> Issue { issue(error.code, error.message) }
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Draft {
    id: String, module: String, generation: u64, diff: String,
    #[serde(default, deserialize_with="issue_list")] risks: Vec<Issue>, valid: bool,
    #[serde(default, deserialize_with="issue_list")] errors: Vec<Issue>,
    #[serde(default, deserialize_with="issue_list", skip_serializing_if = "Vec::is_empty")] dependencies: Vec<Issue>, created_at: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct StoredDraft { #[serde(flatten)] draft: Draft, content: String }
#[derive(Clone, Serialize, Deserialize)]
struct State { generation: u64, #[serde(default)] fingerprint: String, #[serde(default, deserialize_with="stored_drafts")] drafts: Vec<StoredDraft> }
#[derive(Clone, Serialize, Deserialize)]
struct Snapshot { exists: bool, content: String, mode: u32 }
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Operation {
    id: String, state: String, generation: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")] deadline: Option<String>,
    #[serde(deserialize_with="module_list")] changed_modules: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Journal { operation: Operation, phase: String, #[serde(deserialize_with="snapshot_map")] before: BTreeMap<String, Snapshot>, base_generation: u64 }
fn snapshot_map<'de,D:serde::Deserializer<'de>>(d:D)->Result<BTreeMap<String,Snapshot>,D::Error>{
    struct Map;
    impl<'de> serde::de::Visitor<'de> for Map {
        type Value=BTreeMap<String,Snapshot>;
        fn expecting(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{f.write_str("six bounded native rollback snapshots")}
        fn visit_map<A:serde::de::MapAccess<'de>>(self,mut map:A)->Result<Self::Value,A::Error>{
            let mut out=BTreeMap::new();
            while let Some(key)=map.next_key::<String>()?{
                if out.len()>=6||!allowed(&key)||out.contains_key(&key){return Err(serde::de::Error::custom("rollback snapshot limit"));}
                out.insert(key,map.next_value()?);
            }
            Ok(out)
        }
    }
    d.deserialize_map(Map)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StageRequest { module: String, content: String, generation: u64 }
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CommitRequest { #[serde(deserialize_with="draft_ids")] draft_ids: Vec<String>, generation: u64, #[serde(default)] acknowledge_risks: bool }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OperationRequest { id: String }


fn bounded_sequence<'de,D:serde::Deserializer<'de>,T:Deserialize<'de>,const LIMIT:usize>(deserializer:D)->Result<Vec<T>,D::Error>{
    struct Sequence<T,const LIMIT:usize>(std::marker::PhantomData<T>);
    impl<'de,T:Deserialize<'de>,const LIMIT:usize> serde::de::Visitor<'de> for Sequence<T,LIMIT>{
        type Value=Vec<T>;
        fn expecting(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{f.write_str("a bounded configuration sequence")}
        fn visit_unit<E:serde::de::Error>(self)->Result<Vec<T>,E>{Ok(Vec::new())}
        fn visit_seq<A:serde::de::SeqAccess<'de>>(self,mut seq:A)->Result<Vec<T>,A::Error>{
            let mut out=Vec::new();
            while out.len()<LIMIT{
                let Some(item)=seq.next_element()?else{return Ok(out);};out.push(item);
            }
            if seq.next_element::<serde::de::IgnoredAny>()?.is_some(){return Err(serde::de::Error::custom("configuration collection limit"));}
            Ok(out)
        }
    }
    deserializer.deserialize_any(Sequence::<T,LIMIT>(std::marker::PhantomData))
}
fn stored_drafts<'de,D:serde::Deserializer<'de>>(d:D)->Result<Vec<StoredDraft>,D::Error>{bounded_sequence::<D,StoredDraft,MAX_DRAFTS>(d)}
fn issue_list<'de,D:serde::Deserializer<'de>>(d:D)->Result<Vec<Issue>,D::Error>{bounded_sequence::<D,Issue,512>(d)}
fn module_list<'de,D:serde::Deserializer<'de>>(d:D)->Result<Vec<String>,D::Error>{bounded_sequence::<D,String,6>(d)}
fn draft_ids<'de,D:serde::Deserializer<'de>>(d:D)->Result<Vec<String>,D::Error>{bounded_sequence::<D,String,6>(d)}

struct Directory { path: PathBuf, file: File, identity: (u64, u64) }
impl Directory {
    fn open(path: &Path, create: bool) -> Result<Self, ApiError> {
        if path.as_os_str().is_empty() { return Err(failure("unsafe_data_dir")); }
        if create { fs::create_dir_all(path).map_err(|_| failure("storage_failed"))?; }
        let file = OpenOptions::new().read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
            .open(path).map_err(|_| failure("unsafe_data_dir"))?;
        if create { file.set_permissions(Permissions::from_mode(0o700)).map_err(|_| failure("storage_failed"))?; }
        let metadata = file.metadata().map_err(|_| failure("storage_failed"))?;
        Ok(Self { path: path.to_owned(), file, identity: (metadata.dev(), metadata.ino()) })
    }
    fn check(&self) -> Result<(), ApiError> {
        let m = fs::symlink_metadata(&self.path).map_err(|_| failure("storage_failed"))?;
        if !m.is_dir() || m.file_type().is_symlink() || (m.dev(), m.ino()) != self.identity {
            return Err(failure("unsafe_data_dir"));
        }
        Ok(())
    }
    fn read(&self, name: &str, limit: usize) -> Result<Option<Vec<u8>>, ApiError> {
        self.check()?;
        let name = CString::new(name).map_err(|_| failure("storage_failed"))?;
        let fd = unsafe { libc::openat(self.file.as_raw_fd(), name.as_ptr(), libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC) };
        if fd < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::NotFound { return Ok(None); }
            return Err(failure("storage_failed"));
        }
        let file = unsafe { File::from_raw_fd(fd) };
        let m = file.metadata().map_err(|_| failure("storage_failed"))?;
        if !m.is_file() || m.len() > limit as u64 { return Err(failure("storage_failed")); }
        let mut data = Vec::new();
        file.take(limit as u64 + 1).read_to_end(&mut data).map_err(|_| failure("storage_failed"))?;
        if data.len() > limit { return Err(failure("storage_failed")); }
        Ok(Some(data))
    }
    fn free(&self) -> Result<u64, ApiError> {
        self.check()?;
        let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        if unsafe { libc::fstatvfs(self.file.as_raw_fd(), stat.as_mut_ptr()) } != 0 { return Err(failure("storage_insufficient")); }
        let stat = unsafe { stat.assume_init() };
        Ok((stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64))
    }
    fn admit(&self, size: u64) -> Result<(), ApiError> {
        if self.free()? < FREE_HEADROOM_BYTES.saturating_add(size) { return Err(failure("storage_insufficient")); }
        Ok(())
    }
    fn write(&self, name: &str, bytes: &[u8], mode: u32, reserved: bool) -> Result<(), ApiError> {
        self.check()?;
        if !reserved { self.admit(temporary_bytes(bytes.len()))?; }
        let final_name = CString::new(name).map_err(|_| failure("storage_failed"))?;
        // Never overwrite a symlink or special file in an accepted store.
        let mut st = std::mem::MaybeUninit::<libc::stat>::uninit();
        let exists = unsafe { libc::fstatat(self.file.as_raw_fd(), final_name.as_ptr(), st.as_mut_ptr(), libc::AT_SYMLINK_NOFOLLOW) };
        if exists == 0 && unsafe { st.assume_init() }.st_mode & libc::S_IFMT != libc::S_IFREG {
            return Err(failure("storage_failed"));
        }
        if exists != 0 && std::io::Error::last_os_error().kind() != std::io::ErrorKind::NotFound { return Err(failure("storage_failed")); }
        let temp_name = CString::new(format!(".control-{}", random_id()?)).map_err(|_| failure("storage_failed"))?;
        let fd = unsafe { libc::openat(self.file.as_raw_fd(), temp_name.as_ptr(), libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC, (mode & 0o777) as libc::mode_t) };
        if fd < 0 { return Err(failure("storage_failed")); }
        let result = (|| {
            let mut file = unsafe { File::from_raw_fd(fd) };
            file.set_permissions(Permissions::from_mode(mode & 0o777)).map_err(|_| failure("storage_failed"))?;
            file.write_all(bytes).map_err(|_| failure("storage_failed"))?;
            file.sync_all().map_err(|_| failure("storage_failed"))?;
            drop(file);
            self.check()?;
            if unsafe { libc::renameat(self.file.as_raw_fd(), temp_name.as_ptr(), self.file.as_raw_fd(), final_name.as_ptr()) } != 0 {
                return Err(failure("storage_failed"));
            }
            self.file.sync_all().map_err(|_| failure("storage_failed"))
        })();
        unsafe { libc::unlinkat(self.file.as_raw_fd(), temp_name.as_ptr(), 0); }
        result
    }
    fn remove(&self, name: &str) -> Result<(), ApiError> {
        self.check()?;
        let name = CString::new(name).map_err(|_| failure("storage_failed"))?;
        if unsafe { libc::unlinkat(self.file.as_raw_fd(), name.as_ptr(), 0) } != 0 && std::io::Error::last_os_error().kind() != std::io::ErrorKind::NotFound {
            return Err(failure("storage_failed"));
        }
        self.file.sync_all().map_err(|_| failure("storage_failed"))
    }
}
fn temporary_bytes(length: usize) -> u64 { (length as u64).div_ceil(4096) * 4096 + 4096 }
struct BoundedWriter(Vec<u8>);
impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > MAX_STORE_BYTES { return Err(std::io::Error::other("configuration store limit")); }
        self.0.extend_from_slice(bytes); Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}
fn encode<T: Serialize>(record: &T) -> Result<Vec<u8>, ApiError> {
    let mut writer = BoundedWriter(Vec::new());
    serde_json::to_writer(&mut writer, record).map_err(|_| failure("storage_failed"))?;
    Ok(writer.0)
}
fn decode_request<T: for<'de> Deserialize<'de>>(body: &[u8], limit: usize) -> Result<T, ApiError> {
    if body.len() > limit { return Err(failure("body_too_large")); }
    serde_json::from_slice(body).map_err(|_| failure("invalid_request"))
}
fn validate_state(state: &State) -> Result<(), ApiError> {
    if state.generation == 0 || state.drafts.len() > MAX_DRAFTS ||
        (!state.fingerprint.is_empty() && (state.fingerprint.len() != 64 || !state.fingerprint.bytes().all(|b| b.is_ascii_hexdigit()))) { return Err(failure("state_corrupt")); }
    let mut ids = BTreeSet::new(); let mut total = 0usize;
    for d in &state.drafts {
        let m = &d.draft;
        if !safe_id(&m.id) || !ids.insert(&m.id) || !allowed(&m.module) || d.content.len() > MAX_DOCUMENT_BYTES || m.diff.len() > 4*MAX_DOCUMENT_BYTES+4096 || m.risks.len() > 512 || m.errors.len() > 512 || m.dependencies.len() > 512 || parse_time(&m.created_at).is_none() { return Err(failure("state_corrupt")); }
        total = total.saturating_add(d.content.len());
    }
    if total > MAX_DRAFT_CONTENT_BYTES { return Err(failure("state_corrupt")); }
    Ok(())
}
fn validate_journal(j: &Journal) -> Result<(), ApiError> {
    if !safe_id(&j.operation.id) || j.before.is_empty() || j.before.len() > 6 || j.operation.changed_modules.len() != j.before.len() ||
        !matches!(j.phase.as_str(), "applying"|"pending"|"committed"|"rolling_back"|"rolled_back") ||
        !matches!(j.operation.state.as_str(), "committed"|"pending_confirmation"|"rolled_back") ||
        j.operation.deadline.as_ref().is_some_and(|s| parse_time(s).is_none()) ||
        (j.phase == "pending" && j.operation.deadline.is_none()) { return Err(failure("journal_corrupt")); }
    let mut seen = BTreeSet::new();
    for module in &j.operation.changed_modules {
        let Some(s) = j.before.get(module) else { return Err(failure("journal_corrupt")); };
        if !allowed(module) || !seen.insert(module) || s.content.len() > MAX_DOCUMENT_BYTES || s.mode & !0o777 != 0 { return Err(failure("journal_corrupt")); }
    }
    Ok(())
}
// Accept the RFC3339 UTC strings written by Go (including fractional seconds).
fn parse_time(text: &str) -> Option<u64> {
    let bytes = text.as_bytes();
    if bytes.len() < 20 || !text.is_ascii() || bytes[4] != b'-' || bytes[7] != b'-' || bytes[10] != b'T' || bytes[13] != b':' || bytes[16] != b':' || !text.ends_with('Z') { return None; }
    if bytes.len() > 20 && (bytes[19] != b'.' || bytes.len() < 22 || !bytes[20..bytes.len()-1].iter().all(u8::is_ascii_digit)) { return None; }
    let y = text[0..4].parse::<i64>().ok()?; let m = text[5..7].parse::<i64>().ok()?; let d = text[8..10].parse::<i64>().ok()?;
    let hh = text[11..13].parse::<u64>().ok()?; let mm = text[14..16].parse::<u64>().ok()?; let ss = text[17..19].parse::<u64>().ok()?;
    if !(1..=12).contains(&m) || d < 1 || hh > 23 || mm > 59 || ss > 59 { return None; }
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let days = [31, if leap {29} else {28},31,30,31,30,31,31,30,31,30,31];
    if d > days[(m-1) as usize] { return None; }
    let year = y - i64::from(m <= 2); let era = year.div_euclid(400); let yoe = year-era*400;
    let mp = m + if m > 2 {-3} else {9};
    let doy = (153*mp+2)/5+d-1; let doe = yoe*365+yoe/4-yoe/100+doy;
    let unix_days = era*146097+doe-719468;
    u64::try_from(unix_days).ok()?.checked_mul(86400)?.checked_add(hh*3600+mm*60+ss)
}

pub struct Configuration {
    data: Directory, native: Directory, _lock: File, state: State, journal: Option<Journal>,
    recovery_error: bool, startup_recovery: bool, reserved: bool, retry_at: u64, retry_delay: u64,
}
impl Configuration {
    /// data_dir is the existing dedicated configuration directory, not its parent.
    pub fn open(data_dir: &Path) -> Result<Self, ApiError> { Self::open_with_native_dir(data_dir, Path::new("/etc/config")) }
    /// Fixed administrator-selected filesystem seam for synthetic tests. Not an HTTP setting.
    pub fn open_with_native_dir(data_dir: &Path, native_dir: &Path) -> Result<Self, ApiError> {
        let native_real = fs::canonicalize(native_dir).map_err(|_| failure("unsafe_data_dir"))?;
        // Reject a native subtree before creating or changing any directory mode.
        let absolute = if data_dir.is_absolute() { data_dir.to_owned() } else {
            std::env::current_dir().map_err(|_| failure("unsafe_data_dir"))?.join(data_dir)
        };
        let mut existing = absolute.as_path();
        let mut suffix = Vec::new();
        while !existing.exists() {
            suffix.push(existing.file_name().ok_or_else(||failure("unsafe_data_dir"))?.to_owned());
            existing = existing.parent().ok_or_else(||failure("unsafe_data_dir"))?;
        }
        let mut data_real = fs::canonicalize(existing).map_err(|_|failure("unsafe_data_dir"))?;
        for component in suffix.iter().rev() { data_real.push(component); }
        if data_real.starts_with(&native_real) { return Err(failure("unsafe_data_dir")); }
        let data = Directory::open(data_dir, true)?;
        let native = Directory::open(native_dir, false)?;
        let lock_name: &CStr = c".lock";
        let fd = unsafe { libc::openat(data.file.as_raw_fd(), lock_name.as_ptr(), libc::O_RDWR | libc::O_CREAT | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK, 0o600) };
        if fd < 0 { return Err(failure("storage_failed")); }
        let lock = unsafe { File::from_raw_fd(fd) };
        if !lock.metadata().map_err(|_| failure("storage_failed"))?.is_file() { return Err(failure("unsafe_data_dir")); }
        lock.set_permissions(Permissions::from_mode(0o600)).map_err(|_| failure("storage_failed"))?;
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 { return Err(failure("manager_busy")); }
        let state = match data.read("state.json", MAX_STORE_BYTES).map_err(|_| failure("state_corrupt"))? {
            Some(bytes) => serde_json::from_slice::<State>(&bytes).map_err(|_| failure("state_corrupt"))?,
            None => State { generation: 1, fingerprint: String::new(), drafts: Vec::new() },
        };
        validate_state(&state)?;
        let journal = match data.read("journal.json", MAX_STORE_BYTES).map_err(|_| failure("journal_corrupt"))? {
            Some(bytes) => { let j = serde_json::from_slice::<Journal>(&bytes).map_err(|_| failure("journal_corrupt"))?; validate_journal(&j)?; Some(j) },
            None => None,
        };
        let startup_recovery = journal.as_ref().is_some_and(|j| !matches!(j.phase.as_str(), "committed"|"rolled_back"));
        // Unknown/old draft, backup, operation and temporary files are not cleared.
        Ok(Self { data, native, _lock: lock, state, journal, recovery_error: startup_recovery,
            startup_recovery, reserved: false, retry_at: 0, retry_delay: 0 })
    }
    fn save_state(&self) -> Result<(), ApiError> { self.data.write("state.json", &encode(&self.state)?, 0o600, self.reserved) }
    fn save_journal(&self) -> Result<(), ApiError> {
        let j = self.journal.as_ref().ok_or_else(|| failure("operation_not_found"))?;
        self.data.write("journal.json", &encode(j)?, 0o600, self.reserved)
    }
    fn readable(&self) -> Result<(), ApiError> { if self.recovery_error { Err(failure("rollback_failed")) } else { Ok(()) } }
    fn live(&self, io: &mut impl Backend, budget: &Budget<'_>) -> Result<(BTreeMap<String, Snapshot>, String), ApiError> {
        budget_check(budget)?; self.native.check()?;
        let mut live = BTreeMap::new(); let mut hash = Sha256::new();
        for module in MODULES {
            budget_check(budget)?;
            let path = self.native.path.join(module);
            let snapshot = match fs::symlink_metadata(&path) {
                Ok(m) => {
                    if !m.is_file() || m.file_type().is_symlink() || m.len() > MAX_DOCUMENT_BYTES as u64 { return Err(failure("document_unavailable")); }
                    let bytes = io.read(&path, MAX_DOCUMENT_BYTES, budget).map_err(|e| io_failure(e, "document_unavailable"))?;
                    if bytes.len() > MAX_DOCUMENT_BYTES { return Err(failure("document_unavailable")); }
                    let content = String::from_utf8(bytes).map_err(|_| failure("document_unavailable"))?;
                    Snapshot { exists: true, content, mode: m.mode() & 0o777 }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Snapshot { exists: false, content: String::new(), mode: 0o600 },
                Err(_) => return Err(failure("document_unavailable")),
            };
            hash.update(module.as_bytes()); hash.update([0]); hash.update(if snapshot.exists { b"true".as_slice() } else { b"false".as_slice() }); hash.update([0]); hash.update(snapshot.content.as_bytes()); hash.update([0]);
            live.insert(module.into(), snapshot);
        }
        Ok((live, format!("{:x}", hash.finalize())))
    }
    fn sync_generation(&mut self, io: &mut impl Backend, budget: &Budget<'_>) -> Result<BTreeMap<String, Snapshot>, ApiError> {
        self.readable()?;
        let (live, fingerprint) = self.live(io, budget)?;
        if self.state.fingerprint != fingerprint {
            if self.pending().is_some() { return Err(failure("generation_conflict")); }
            let old = (self.state.generation, std::mem::replace(&mut self.state.fingerprint, fingerprint));
            if !old.1.is_empty() { self.state.generation = self.state.generation.checked_add(1).ok_or_else(|| failure("storage_failed"))?; }
            if let Err(e) = self.save_state() { self.state.generation = old.0; self.state.fingerprint = old.1; return Err(e); }
        }
        Ok(live)
    }
    fn pending(&self) -> Option<Value> {
        self.journal.as_ref().filter(|j| j.phase == "pending").and_then(|j| j.operation.deadline.as_ref().map(|d| json!({"id":j.operation.id,"deadline":d})))
    }
    fn status(&self, now: u64) -> Value {
        let mut result = json!({"enabled":!self.recovery_error,"generation":self.state.generation});
        if let Some(p) = self.pending() { result["pendingCommit"] = p; }
        if self.recovery_error { result["errorCode"] = json!("rollback_failed"); }
        if let Some(j) = &self.journal {
            let mut op = operation_value(&j.operation);
            op["phase"] = json!(j.phase);
            op["canConfirm"] = json!(!self.recovery_error && j.phase == "pending" && j.operation.deadline.as_ref().and_then(|d|parse_time(d)).is_some_and(|d|now<d));
            op["canRollback"] = json!(matches!(j.phase.as_str(),"applying"|"pending"|"rolling_back") || j.phase == "committed" && self.state.generation == j.operation.generation);
            if self.recovery_error { op["errorCode"] = json!("rollback_failed"); }
            result["operation"] = op;
        }
        result
    }
    pub fn documents(&mut self, io: &mut impl Backend, budget: &Budget<'_>) -> Result<Value, ApiError> {
        self.ensure_started(io, budget, &mut |_, _| Ok(()))?;
        let live = self.sync_generation(io, budget)?;
        let docs: Vec<Value> = MODULES.iter().map(|m| json!({"module":m,"content":live[*m].content})).collect();
        let mut out = json!({"generation":self.state.generation,"documents":docs});
        if let Some(p) = self.pending() { out["pendingCommit"] = p; }
        Ok(out)
    }
    pub fn stage_for_import(&mut self, module: &str, content: &str, generation: u64, io: &mut impl Backend, budget: &Budget<'_>) -> Result<Value, ApiError> {
        self.ensure_started(io, budget, &mut |_, _| Ok(()))?;
        self.stage(StageRequest {module:module.into(),content:content.into(),generation}, io, budget)
    }
    pub fn discard_import_draft(&mut self, id: &str) -> Result<(), ApiError> { self.delete_draft(id) }
    fn stage(&mut self, input: StageRequest, io: &mut impl Backend, budget: &Budget<'_>) -> Result<Value, ApiError> {
        budget_check(budget)?; self.readable()?;
        if !allowed(&input.module) { return Err(failure("module_not_allowed")); }
        if input.content.len() > MAX_DOCUMENT_BYTES { return Err(failure("document_too_large")); }
        let live = self.sync_generation(io, budget)?;
        if input.generation != self.state.generation { return Err(failure("generation_conflict")); }
        if self.pending().is_some() { return Err(failure("pending_confirmation")); }
        let total: usize = self.state.drafts.iter().map(|d|d.content.len()).sum();
        if self.state.drafts.len() >= MAX_DRAFTS || total.saturating_add(input.content.len()) > MAX_DRAFT_CONTENT_BYTES { return Err(failure("draft_limit")); }
        let mut errors = validate(&input.module, &input.content);
        let candidates = BTreeMap::from([(input.module.clone(), input.content.clone())]);
        let mut dependencies = Vec::new();
        if errors.is_empty() {
            errors = execution_changes(&input.module, &live[&input.module].content, &input.content);
            if errors.is_empty() { errors = self.validate_native(&candidates, &live, true, io, budget)?; }
            if errors.is_empty() { dependencies = references(&candidates, &combined_documents(&live, &candidates)); }
        }
        let draft = Draft { id: random_id()?, module: input.module.clone(), generation: input.generation,
            diff: document_diff(&input.module, &live[&input.module].content, &input.content),
            risks: risks(&input.module, &live[&input.module].content, &input.content), valid: errors.is_empty(), errors, dependencies,
            created_at: timestamp(io.now_unix()) };
        let out = draft_value(&draft);
        self.state.drafts.push(StoredDraft {draft,content:input.content});
        if let Err(e) = self.save_state() { self.state.drafts.pop(); return Err(e); }
        Ok(out)
    }
    fn delete_draft(&mut self, id: &str) -> Result<(), ApiError> {
        self.readable()?;
        if !safe_id(id) { return Err(failure("draft_not_found")); }
        let i = self.state.drafts.iter().position(|d|d.draft.id==id).ok_or_else(||failure("draft_not_found"))?;
        let removed = self.state.drafts.remove(i);
        if let Err(e)=self.save_state() { self.state.drafts.insert(i,removed); return Err(e); }
        Ok(())
    }
    /// Root supplies capture withdrawal here. It runs only after validation/CAS/storage
    /// admission, immediately before the durable mutation boundary. It also covers
    /// startup, manual, deadline and failure recovery. `network_change` excludes SSH/system.
    pub fn handle_with_before_mutation<B: Backend>(&mut self, path: &str, method: Method, query: &str, body: &[u8], io: &mut B, budget: &Budget<'_>, before: &mut impl FnMut(&mut B, bool) -> Result<(), ApiError>) -> Result<Value, ApiError> {
        budget_check(budget)?;
        let path = path.strip_prefix("/api").unwrap_or(path);
        if path == "/configuration/status" && method == Method::Get {
            // Failed recovery remains inspectable, including authoritative journal phase.
            let _ = self.ensure_started(io, budget, before);
            return Ok(self.status(io.now_unix()));
        }
        if path == "/configuration/rollback" && method == Method::Post {
            let input: OperationRequest = decode_request(body, 4096)?;
            return self.rollback_id(&input.id, io, budget, before);
        }
        self.ensure_started(io, budget, before)?;
        match (path, method) {
            ("/configuration",Method::Get) => self.documents(io,budget),
            ("/configuration/drafts",Method::Get) => { self.readable()?; Ok(json!({"drafts":self.state.drafts.iter().map(|d|draft_value(&d.draft)).collect::<Vec<_>>()})) },
            ("/configuration/drafts",Method::Delete) => { self.delete_draft(query_id(query)?)?; Ok(json!({"deleted":true})) },
            ("/configuration/stage",Method::Post) => self.stage(decode_request(body,2<<20)?,io,budget),
            ("/configuration/commit",Method::Post) => self.commit(decode_request(body,64<<10)?,io,budget,before),
            ("/configuration/confirm",Method::Post) => { let input:OperationRequest=decode_request(body,4096)?; self.confirm(&input.id,io,budget,before) },
            (p,_) if matches!(p,"/configuration"|"/configuration/status"|"/configuration/drafts"|"/configuration/stage"|"/configuration/commit"|"/configuration/confirm"|"/configuration/rollback") => Err(failure("method_not_allowed")),
            _ => Err(failure("not_found")),
        }
    }
    pub fn handle(&mut self, path:&str, method:Method, query:&str, body:&[u8], io:&mut impl Backend, budget:&Budget<'_>) -> Result<Value,ApiError> {
        self.handle_with_before_mutation(path,method,query,body,io,budget,&mut |_,_|Ok(()))
    }
    fn ensure_started<B:Backend>(&mut self, io:&mut B, budget:&Budget<'_>, before:&mut impl FnMut(&mut B,bool)->Result<(),ApiError>) -> Result<(),ApiError> {
        if self.startup_recovery {
            self.startup_recovery=false;
            self.rollback_internal(io,budget,before)?;
        }
        self.readable()
    }
    pub fn tick(&mut self, io:&mut impl Backend, budget:&Budget<'_>) -> Result<(),ApiError> { self.tick_with_before_mutation(io,budget,&mut |_,_|Ok(())) }
    pub fn tick_with_before_mutation<B:Backend>(&mut self,io:&mut B,budget:&Budget<'_>,before:&mut impl FnMut(&mut B,bool)->Result<(),ApiError>) -> Result<(),ApiError> {
        budget_check(budget)?;
        if self.startup_recovery { return self.ensure_started(io,budget,before); }
        let now=io.now_unix();
        let expired=self.journal.as_ref().is_some_and(|j|j.phase=="pending" && j.operation.deadline.as_ref().and_then(|d|parse_time(d)).is_some_and(|d|now>=d));
        if expired || self.recovery_error && now>=self.retry_at { self.rollback_internal(io,budget,before)?; }
        Ok(())
    }

    fn validate_native(&self, candidates:&BTreeMap<String,String>,live:&BTreeMap<String,Snapshot>,defer_references:bool,io:&mut impl Backend,budget:&Budget<'_>) -> Result<Vec<Issue>,ApiError> {
        budget_check(budget)?;
        let combined=combined_documents(live,candidates);
        for (module,text) in candidates {
            let errors=execution_changes(module,&live[module].content,text);
            if !errors.is_empty() { return Ok(errors); }
        }
        if !defer_references {
            let mut errors=references(candidates,&combined);
            errors.extend(reverse_references(live,&combined,candidates));
            if !errors.is_empty() { return Ok(errors); }
        }
        let total=combined.values().map(|s|temporary_bytes(s.len())).sum::<u64>();
        self.data.admit(total)?;
        self.data.check()?;
        let name=format!("candidate-{}",random_id()?);
        let path=self.data.path.join(&name);
        fs::create_dir(&path).map_err(|_|failure("storage_failed"))?;
        fs::set_permissions(&path,Permissions::from_mode(0o700)).map_err(|_|failure("storage_failed"))?;
        let cleanup=CandidateDirectory(path.clone());
        let directory=Directory::open(&path,false)?;
        for module in MODULES {
            budget_check(budget)?;
            directory.write(module,combined[module].as_bytes(),0o600,true)?;
        }
        let path=path.to_str().ok_or_else(||failure("storage_failed"))?.to_owned();
        let mut issues=Vec::new();
        for module in MODULES {
            if !candidates.contains_key(module) {continue;}
            let args=vec!["-s".into(),"-c".into(),path.clone(),"-P".into(),path.clone(),"show".into(),module.into()];
            match command(io,Program::Uci,&args,10,budget) {
                Ok(output) if output.code==0 => (),
                Ok(_) => issues.push(error_issue(failure("uci_validation_failed"))),
                Err(e) if matches!(e.code,"deadline"|"cancelled") => return Err(e),
                Err(_) => issues.push(error_issue(failure("validation_unavailable"))),
            }
        }
        drop(directory); drop(cleanup);
        Ok(issues)
    }
    fn preflight(&self,changed:&[String],io:&mut impl Backend,budget:&Budget<'_>)->Result<(),ApiError> {
        if changed.iter().any(|m|m=="system") {
            let args=vec!["-q".into(),"get".into(),"xiaoqiang.common.INITTED".into()];
            let result=command(io,Program::Uci,&args,5,budget).map_err(|e|if matches!(e.code,"deadline"|"cancelled"){e}else{failure("system_reload_unsafe")})?;
            if result.code!=0 || result.stdout.len()>OUTPUT_LIMIT || result.stdout.as_slice().trim_ascii()!=b"YES" {return Err(failure("system_reload_unsafe"));}
        }
        Ok(())
    }
    fn reload(&self,changed:&[String],io:&mut impl Backend,budget:&Budget<'_>)->Result<(),ApiError> {
        let mut first=None;
        for module in changed {
            let service=match module.as_str(){"network"=>"network","wireless"=>"wifi","dhcp"=>"dnsmasq","firewall"=>"firewall","system"=>"system","dropbear"=>"dropbear",_=>return Err(failure("reload_unsupported"))};
            let args=vec![service.into(),"reload".into()];
            let result=command(io,Program::Service,&args,20,budget);
            let error=match result {Ok(o) if o.code==0=>None,Ok(_)=>Some(failure("reload_failed")),Err(e)=>Some(if matches!(e.code,"deadline"|"cancelled"){e}else{failure("reload_failed")})};
            if first.is_none(){first=error;}
            // Continue every affected module even after one reload fails.
        }
        match first{Some(e)=>Err(e),None=>Ok(())}
    }
    fn reserve(&self,j:&Journal,candidates:Option<&BTreeMap<String,String>>)->Result<(),ApiError> {
        let mut sized_state=self.state.clone(); sized_state.generation=u64::MAX;
        let mut sized_journal=j.clone(); sized_journal.base_generation=u64::MAX;
        sized_journal.operation.generation=u64::MAX;
        sized_journal.operation.state="pending_confirmation".into();
        sized_journal.operation.deadline=Some("9999-12-31T23:59:59.999999999Z".into());
        sized_journal.phase="rolling_back".into();
        let state_bytes=temporary_bytes(encode(&sized_state)?.len());
        let journal_bytes=temporary_bytes(encode(&sized_journal)?.len());
        let mut live_bytes=0u64;
        for module in &j.operation.changed_modules {
            if let Some(c)=candidates {live_bytes=live_bytes.saturating_add(temporary_bytes(c[module].len()));}
            if j.before[module].exists {live_bytes=live_bytes.saturating_add(temporary_bytes(j.before[module].content.len()));}
        }
        let data_bytes=if candidates.is_some(){4*journal_bytes+3*state_bytes}else{2*journal_bytes+state_bytes};
        if self.data.identity.0==self.native.identity.0 {
            self.data.admit(data_bytes.saturating_add(live_bytes))
        }else{self.data.admit(data_bytes)?;self.native.admit(live_bytes)}
    }
    fn commit<B:Backend>(&mut self,input:CommitRequest,io:&mut B,budget:&Budget<'_>,before_mutation:&mut impl FnMut(&mut B,bool)->Result<(),ApiError>)->Result<Value,ApiError> {
        budget_check(budget)?;self.readable()?;
        let live=self.sync_generation(io,budget)?;
        if input.generation!=self.state.generation {return Err(failure("generation_conflict"));}
        if self.pending().is_some(){return Err(failure("pending_confirmation"));}
        if input.draft_ids.is_empty() || input.draft_ids.len()>6 {return Err(failure("invalid_commit"));}
        let mut ids=BTreeSet::new();let mut candidates=BTreeMap::new();let mut snapshots=BTreeMap::new();let mut risk_required=false;
        for id in &input.draft_ids {
            if !safe_id(id) || !ids.insert(id.clone()){return Err(failure("invalid_commit"));}
            let d=self.state.drafts.iter().find(|d|d.draft.id==*id).ok_or_else(||failure("draft_not_found"))?;
            if d.draft.generation!=input.generation {return Err(failure("generation_conflict"));}
            if !d.draft.valid {return Err(failure("validation_failed"));}
            if candidates.insert(d.draft.module.clone(),d.content.clone()).is_some(){return Err(failure("invalid_commit"));}
            if d.content!=live[&d.draft.module].content || !live[&d.draft.module].exists {
                snapshots.insert(d.draft.module.clone(),live[&d.draft.module].clone());
                risk_required|=!risks(&d.draft.module,&live[&d.draft.module].content,&d.content).is_empty();
            }
        }
        let changed:Vec<String>=MODULES.iter().filter(|m|snapshots.contains_key(**m)).map(|m|(*m).to_owned()).collect();
        if changed.is_empty(){return Err(failure("no_changes"));}
        if risk_required && !input.acknowledge_risks{return Err(failure("risk_ack_required"));}
        for (module,text) in &candidates {
            if !validate(module,text).is_empty(){return Err(failure("validation_failed"));}
        }
        let errors=self.validate_native(&candidates,&live,false,io,budget)?;
        if let Some(error)=errors.first(){return Err(issue_failure(&error.code));}
        self.preflight(&changed,io,budget)?;
        if self.live(io,budget)?.1!=self.state.fingerprint{return Err(failure("generation_conflict"));}
        let generation=self.state.generation.checked_add(1).ok_or_else(||failure("storage_failed"))?;
        let op=Operation{id:random_id()?,state:if risk_required{"pending_confirmation"}else{"committed"}.into(),generation,
            deadline:if risk_required{Some(timestamp(io.now_unix().saturating_add(CONFIRMATION_SECONDS)))}else{None},changed_modules:changed.clone()};
        let journal=Journal{operation:op.clone(),phase:"applying".into(),before:snapshots,base_generation:self.state.generation};
        self.reserve(&journal,Some(&candidates))?;
        budget_check(budget)?;
        before_mutation(io,network_change(&changed))?;
        let previous_journal=self.journal.replace(journal);
        self.reserved=true;
        if let Err(e)=self.save_journal(){self.journal=previous_journal;self.reserved=false;return Err(e);}
        let apply_result=(||{
            for module in &changed {
                budget_check(budget)?;
                self.native.write(module,candidates[module].as_bytes(),0o600,true).map_err(|_|failure("apply_failed"))?;
            }
            self.reload(&changed,io,budget)?;
            let (after,fingerprint)=self.live(io,budget)?;
            for(module,text)in &candidates{
                if !after[module].exists || after[module].content!=*text{return Err(failure("verification_failed"));}
            }
            self.state.generation=op.generation;self.state.fingerprint=fingerprint;self.save_state()?;
            let applied=self.journal.as_mut().ok_or_else(||failure("operation_not_found"))?;
            applied.phase=if risk_required{"pending"}else{"committed"}.into();
            if risk_required { applied.operation.deadline=Some(timestamp(io.now_unix().saturating_add(CONFIRMATION_SECONDS))); }
            self.save_journal()?;
            let old=std::mem::take(&mut self.state.drafts);
            let (consumed,kept):(Vec<_>,Vec<_>)=old.into_iter().partition(|d|ids.contains(&d.draft.id));
            self.state.drafts=kept;
            if let Err(e)=self.save_state(){self.state.drafts.extend(consumed);return Err(e);}
            Ok(())
        })();
        if let Err(cause)=apply_result {
            // Recovery cannot inherit a disconnected/cancelled browser budget.
            let cancel=AtomicBool::new(false);
            let recovery=Budget{deadline:Instant::now()+Duration::from_secs(90),cancel:&cancel};
            let recovered=self.rollback_internal(io,&recovery,before_mutation);
            self.reserved=false;
            return match recovered{Ok(())=>Err(cause),Err(_)=>Err(failure("rollback_failed"))};
        }
        self.reserved=false;
        Ok(operation_value(&self.journal.as_ref().ok_or_else(||failure("operation_not_found"))?.operation))
    }
    fn confirm<B:Backend>(&mut self,id:&str,io:&mut B,budget:&Budget<'_>,before:&mut impl FnMut(&mut B,bool)->Result<(),ApiError>)->Result<Value,ApiError>{
        budget_check(budget)?;self.readable()?;
        let j=self.journal.as_ref().filter(|j|j.operation.id==id).ok_or_else(||failure("operation_not_found"))?;
        if j.phase=="committed"{return Ok(operation_value(&j.operation));}
        if j.phase!="pending"{return Err(failure("not_pending"));}
        if j.operation.deadline.as_ref().and_then(|d|parse_time(d)).is_none_or(|deadline|io.now_unix()>=deadline){
            self.rollback_internal(io,budget,before)?;
            return Ok(operation_value(&self.journal.as_ref().ok_or_else(||failure("operation_not_found"))?.operation));
        }
        self.sync_generation(io,budget)?;
        let old=self.journal.clone();
        let j=self.journal.as_mut().ok_or_else(||failure("operation_not_found"))?;
        j.phase="committed".into();j.operation.state="committed".into();j.operation.deadline=None;
        if let Err(e)=self.save_journal(){self.journal=old;return Err(e);}
        Ok(operation_value(&self.journal.as_ref().ok_or_else(||failure("operation_not_found"))?.operation))
    }
    fn rollback_id<B:Backend>(&mut self,id:&str,io:&mut B,budget:&Budget<'_>,before:&mut impl FnMut(&mut B,bool)->Result<(),ApiError>)->Result<Value,ApiError>{
        budget_check(budget)?;
        let j=self.journal.as_ref().filter(|j|j.operation.id==id).ok_or_else(||failure("operation_not_found"))?;
        if j.phase=="rolled_back"{return Ok(operation_value(&j.operation));}
        if j.phase=="committed"{
            let generation=j.operation.generation;
            self.sync_generation(io,budget)?;
            if self.state.generation!=generation{return Err(failure("generation_conflict"));}
        }
        self.startup_recovery=false;
        self.rollback_internal(io,budget,before)?;
        Ok(operation_value(&self.journal.as_ref().ok_or_else(||failure("operation_not_found"))?.operation))
    }
    fn restore_bytes(&self,j:&Journal)->Result<(),ApiError>{
        let mut first=None;
        for module in &j.operation.changed_modules{
            let snapshot=&j.before[module];
            let result=if snapshot.exists{self.native.write(module,snapshot.content.as_bytes(),snapshot.mode,true)}else{self.native.remove(module)};
            if first.is_none(){first=result.err();}
        }
        match first{Some(e)=>Err(e),None=>Ok(())}
    }
    fn rollback_internal<B:Backend>(&mut self,io:&mut B,budget:&Budget<'_>,before:&mut impl FnMut(&mut B,bool)->Result<(),ApiError>)->Result<(),ApiError>{
        let j=self.journal.as_ref().ok_or_else(||failure("operation_not_found"))?.clone();
        if j.phase=="rolled_back"{self.recovery_error=false;return Ok(());}
        // The transaction callback already withdrew capture before applying. Do
        // not re-run a fallible withdrawal between a failed apply and restoration.
        let mutation_already_admitted=self.reserved;
        let was_reserved=self.reserved;
        let result=(||{
            if !was_reserved{self.reserve(&j,None)?;}
            budget_check(budget)?;
            if !mutation_already_admitted { before(io,network_change(&j.operation.changed_modules))?; }
            self.reserved=true;
            self.journal.as_mut().ok_or_else(||failure("operation_not_found"))?.phase="rolling_back".into();
            self.save_journal()?;
            let restore=self.restore_bytes(&j);
            let reload=self.reload(&j.operation.changed_modules,io,budget);
            let verification=self.live(io,budget);
            let exact=verification.as_ref().is_ok_and(|(live,_)|j.before.iter().all(|(m,s)|{
                let a=&live[m];a.exists==s.exists && (!s.exists || a.content==s.content && a.mode==(s.mode&0o777))
            }));
            if restore.is_err() || reload.is_err() || !exact{
                // Keep previous committed bytes even if a reload script rewrote them.
                let _=self.restore_bytes(&j);
                return Err(failure("rollback_failed"));
            }
            let (_,fingerprint)=verification.map_err(|_|failure("rollback_failed"))?;
            let minimum=j.base_generation.checked_add(2).ok_or_else(||failure("rollback_failed"))?;
            let generation=if self.state.generation>=minimum{self.state.generation.checked_add(1).ok_or_else(||failure("rollback_failed"))?}else{minimum};
            self.state.generation=generation;self.state.fingerprint=fingerprint;
            self.save_state()?;
            let journal=self.journal.as_mut().ok_or_else(||failure("operation_not_found"))?;
            journal.phase="rolled_back".into();journal.operation.state="rolled_back".into();journal.operation.generation=generation;journal.operation.deadline=None;
            if let Err(e)=self.save_journal(){
                let journal=self.journal.as_mut().ok_or_else(||failure("operation_not_found"))?;
                journal.phase="rolling_back".into();journal.operation=j.operation.clone();return Err(e);
            }
            Ok(())
        })();
        self.reserved=was_reserved;
        match result{
            Ok(())=>{self.recovery_error=false;self.retry_at=0;self.retry_delay=0;Ok(())},
            Err(_)=>{
                self.recovery_error=true;
                if let Some(j)=self.journal.as_mut(){j.phase="rolling_back".into();}
                self.retry_delay=if self.retry_delay==0{1}else{(self.retry_delay*2).min(30)};
                self.retry_at=io.now_unix().saturating_add(self.retry_delay);
                Err(failure("rollback_failed"))
            },
        }
    }
}
struct CandidateDirectory(PathBuf);
impl Drop for CandidateDirectory{fn drop(&mut self){let _=fs::remove_dir_all(&self.0);}}
fn command(io:&mut impl Backend,program:Program,args:&[String],seconds:u64,budget:&Budget<'_>)->Result<crate::product_io::Output,ApiError>{
    budget_check(budget)?;
    let bounded=Budget{deadline:budget.deadline.min(Instant::now()+Duration::from_secs(seconds)),cancel:budget.cancel};
    let output=io.run(program,args,None,OUTPUT_LIMIT,&bounded).map_err(|e|io_failure(e,"validation_unavailable"))?;
    if output.stdout.len()>OUTPUT_LIMIT || output.stderr.len()>OUTPUT_LIMIT{return Err(failure("validation_unavailable"));}
    Ok(output)
}
fn network_change(changed:&[String])->bool{changed.iter().any(|m|matches!(m.as_str(),"network"|"wireless"|"dhcp"|"firewall"))}
fn operation_value(op:&Operation)->Value{serde_json::to_value(op).unwrap_or_else(|_|json!({}))}
fn draft_value(d:&Draft)->Value{serde_json::to_value(d).unwrap_or_else(|_|json!({}))}
fn query_id(query:&str)->Result<&str,ApiError>{
    if query.len()>2048{return Err(failure("invalid_request"));}
    let mut result=None;
    for pair in query.split('&'){
        let Some((key,value))=pair.split_once('=')else{continue};
        if key=="id"{if result.is_some() || !safe_id(value){return Err(failure("draft_not_found"));}result=Some(value);}
    }
    result.ok_or_else(||failure("draft_not_found"))
}
fn issue_failure(code:&str)->ApiError{failure(match code{
    "invalid_reference"=>"invalid_reference","execution_hook_not_allowed"=>"execution_hook_not_allowed",
    "factory_include_removed"=>"factory_include_removed","protocol_not_registered"=>"protocol_not_registered",
    "management_migration_unavailable"=>"management_migration_unavailable","rescue_access_required"=>"rescue_access_required",
    "uci_validation_failed"=>"uci_validation_failed","validation_unavailable"=>"validation_unavailable",_=>"validation_failed",
})}
fn combined_documents(live:&BTreeMap<String,Snapshot>,candidates:&BTreeMap<String,String>)->BTreeMap<String,String>{
    MODULES.iter().map(|m|((*m).into(),candidates.get(*m).cloned().unwrap_or_else(||live[*m].content.clone()))).collect()
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Section { kind:String, name:String, values:BTreeMap<String,Vec<String>> }
impl Section {
    fn one(&self,key:&str)->&str{self.values.get(key).and_then(|v|v.last()).map_or("",String::as_str)}
    fn disabled(&self)->bool{truthy(self.one("disabled"))}
}
fn truthy(s:&str)->bool{matches!(s.to_ascii_lowercase().as_str(),"1"|"true"|"on"|"yes")}
fn falsey(s:&str)->bool{matches!(s.to_ascii_lowercase().as_str(),"0"|"false"|"off"|"no")}
fn identifier(s:&str)->bool{!s.is_empty()&&s.len()<=128&&s.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'_'||b==b'-')}
fn tokens(content:&str)->Result<Vec<Vec<String>>,()>{
    if content.len()>MAX_DOCUMENT_BYTES{return Err(());}
    let mut records=Vec::new();let mut words=Vec::new();let mut word=String::new();let mut started=false;let mut escaped=false;let mut quote=None;
    let mut chars=content.chars().peekable();
    while let Some(c)=chars.next(){
        if c=='\0'||c.is_control()&&!matches!(c,'\n'|'\r'|'\t'){return Err(());}
        if escaped{if c!='\n'{word.push(c);started=true;}escaped=false;continue;}
        if let Some(q)=quote{
            if c==q{quote=None;continue;}
            if c=='\\'&&q=='"'{escaped=true;continue;}
            word.push(c);continue;
        }
        match c{
            '\\'=>{escaped=true;started=true;},
            '\''|'"'=>{quote=Some(c);started=true;},
            '#'=>{
                while let Some(next)=chars.next(){if next=='\n'{break;}}
                if started{words.push(std::mem::take(&mut word));started=false;}
                if !words.is_empty(){records.push(std::mem::take(&mut words));}
            },
            '\n'=>{
                if started{words.push(std::mem::take(&mut word));started=false;}
                if !words.is_empty(){records.push(std::mem::take(&mut words));}
            },
            ' '|'\t'|'\r'=>{if started{words.push(std::mem::take(&mut word));started=false;}},
            ';'|'`'=>return Err(()),
            _=>{word.push(c);started=true;},
        }
    }
    if escaped||quote.is_some(){return Err(());}
    if started{words.push(word);}
    if !words.is_empty(){records.push(words);}
    Ok(records)
}
fn parse(content:&str)->Result<Vec<Section>,()>{
    let records=tokens(content)?;let mut out:Vec<Section>=Vec::new();let mut names=BTreeSet::new();
    for r in records{
        match r[0].as_str(){
            "config"=>{
                if r.len()<2||r.len()>3||!identifier(&r[1]){return Err(());}
                let name=r.get(2).cloned().unwrap_or_default();
                if !name.is_empty()&&(!identifier(&name)||!names.insert(name.clone())){return Err(());}
                // An explicitly quoted empty section name is not an anonymous section.
                if r.len()==3&&name.is_empty(){return Err(());}
                out.push(Section{kind:r[1].clone(),name,values:BTreeMap::new()});
                if out.len()>512{return Err(());}
            },
            "option"|"list"=>{
                if r.len()!=3||!identifier(&r[1]){return Err(());}
                let s=out.last_mut().ok_or(())?;
                if r[0]=="option"{s.values.insert(r[1].clone(),vec![r[2].clone()]);}
                else{s.values.entry(r[1].clone()).or_default().push(r[2].clone());}
            },
            _=>return Err(()),
        }
    }
    Ok(out)
}
fn validate(module:&str,content:&str)->Vec<Issue>{
    let Ok(sections)=parse(content)else{return vec![issue("uci_syntax","Only valid native config, option and list statements are accepted.")];};
    let mut issues=Vec::new();
    for s in sections{
        for (key,values) in &s.values{
            for value in values{
                if !valid_native_field(module,&s.kind,key,value){issues.push(issue("invalid_field",&format!("Invalid {module} field: {key}.")));}
            }
        }
        if module=="wireless"&&s.kind=="wifi-iface"{
            let ssid=s.one("ssid");
            if ssid.len()>32||ssid.contains(['\r','\n']){issues.push(issue("invalid_field","Invalid wireless field: ssid."));}
            let encryption=s.one("encryption").to_ascii_lowercase();let key=s.one("key");
            if (encryption.starts_with("psk")||encryption.starts_with("sae"))&&!key.is_empty()&&!((8..=63).contains(&key.len())||key.len()==64&&hexadecimal(key)){
                issues.push(issue("invalid_field","Invalid wireless field: key."));
            }
        }
        if module=="network"&&s.kind=="interface"&&s.one("ipaddr").parse::<IpAddr>().is_ok_and(|ip|ip.is_ipv6())&&!s.one("netmask").is_empty(){issues.push(issue("invalid_field","Invalid network field: netmask."));}
    }
    // Diagnostic growth is bounded independently of a document's repeated values.
    issues.truncate(512);issues
}
fn integer(v:&str,min:i64,max:i64)->bool{v.parse::<i64>().is_ok_and(|n|n>=min&&n<=max)}
fn unsigned_integer(v:&str,min:u64)->bool{v.parse::<u64>().is_ok_and(|n|n>=min)}
fn boolean(v:&str)->bool{truthy(v)||falsey(v)}
fn hexadecimal(v:&str)->bool{v.bytes().all(|b|b.is_ascii_hexdigit())}
fn macs(v:&str,wildcard:bool)->bool{
    let mut found=false;
    for value in v.split_whitespace(){
        found=true;
        if wildcard&&value=="*"{continue;}
        let octets:Vec<_>=value.split(':').collect();
        if octets.len()!=6||octets.iter().any(|o|!(wildcard&&*o=="*")&&(o.len()!=2||!hexadecimal(o))){return false;}
    }
    found
}
fn ports(v:&str)->bool{
    let text=v.replace(','," ");let mut found=false;
    for field in text.split_whitespace(){
        found=true;let field=field.strip_prefix('!').unwrap_or(field);
        let ends:Vec<_>=field.split(['-',':']).filter(|s|!s.is_empty()).collect();
        if ends.is_empty()||ends.len()>2||ends.iter().any(|e|!integer(e,1,65535)){return false;}
        if ends.len()==2&&ends[0].parse::<u16>().ok()>ends[1].parse::<u16>().ok(){return false;}
    }
    found
}
fn ips(v:&str)->bool{
    let mut found=false;
    for field in v.split_whitespace(){
        found=true;let field=field.strip_prefix('!').unwrap_or(field);
        if field.parse::<IpAddr>().is_ok(){continue;}
        let Some((ip,bits))=field.split_once('/')else{return false;};
        let Ok(ip)=ip.parse::<IpAddr>()else{return false;};
        if !integer(bits,0,if ip.is_ipv4(){32}else{128}){return false;}
    }
    found
}
fn netmask(v:&str)->bool{
    if integer(v,0,32){return true;}
    let Ok(ip)=v.parse::<Ipv4Addr>()else{return false;};
    let mask=u32::from(ip);let inverse=!mask;
    inverse==u32::MAX||inverse & inverse.wrapping_add(1)==0
}
fn domain(v:&str)->bool{
    if v.is_empty()||v.len()>253{return false;}
    let v=v.strip_suffix('.').unwrap_or(v);let v=v.strip_prefix("*.").unwrap_or(v);
    v.split('.').all(|label|!label.is_empty()&&label.len()<=63&&!label.starts_with('-')&&!label.ends_with('-')&&label.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'))
}

fn valid_native_field(module:&str,kind:&str,key:&str,v:&str)->bool {
    match (module,kind) {
        ("network","interface") => match key {
            "ip6assign" => integer(v, 0, 128),
            "peerdns" | "defaultroute" | "delegate" | "auto" | "force_link" | "disabled" => boolean(v),
            "mtu" => integer(v, 576, 65535),
            "metric" | "demand" => unsigned_integer(v, 0),
            "ipaddr" | "ip6addr" | "gateway" | "ip6gw" | "broadcast" | "dns" => ips(v),
            "netmask" => netmask(v),
            "macaddr" => macs(v, false),
            _ => true,
        },
        ("network","device") => match key {
            "vid" => integer(v, 1, 4094),
            "mtu" => integer(v, 576, 65535),
            "ipv6" | "stp" | "igmp_snooping" | "multicast_querier" | "bridge_empty" | "vlan_filtering" | "disabled" => boolean(v),
            "ageing_time" => unsigned_integer(v, 0),
            "priority" => integer(v, 0, 65535),
            "macaddr" => macs(v, false),
            _ => true,
        },
        ("network","bridge-vlan") => match key {
            "vlan" => integer(v, 1, 4094),
            "local" => boolean(v),
            _ => true,
        },
        ("network","switch") => match key {
            "reset" | "enable_vlan" | "enable_mirror_rx" | "enable_mirror_tx" => boolean(v),
            "mirror_source_port" | "mirror_monitor_port" => unsigned_integer(v, 0),
            _ => true,
        },
        ("network","switch_vlan") => match key {
            "vlan" => unsigned_integer(v, 0),
            "vid" => integer(v, 1, 4094),
            _ => true,
        },
        ("network","route") => match key {
            "metric" => unsigned_integer(v, 0),
            "mtu" => integer(v, 576, 65535),
            "onlink" | "disabled" => boolean(v),
            "target" | "gateway" | "source" => ips(v),
            "netmask" => netmask(v),
            _ => true,
        },
        ("network","route6") => match key {
            "metric" => unsigned_integer(v, 0),
            "mtu" => integer(v, 576, 65535),
            "onlink" | "disabled" => boolean(v),
            "target" | "gateway" | "source" => ips(v),
            _ => true,
        },
        ("network","rule") => match key {
            "priority" | "goto" => unsigned_integer(v, 0),
            "invert" | "disabled" => boolean(v),
            "suppress_prefixlength" => integer(v, 0, 128),
            "src" | "dest" => ips(v),
            _ => true,
        },
        ("network","rule6") => match key {
            "priority" | "goto" => unsigned_integer(v, 0),
            "invert" | "disabled" => boolean(v),
            "suppress_prefixlength" => integer(v, 0, 128),
            "src" | "dest" => ips(v),
            _ => true,
        },
        ("network","globals") => match key {
            "ula_prefix" => ips(v),
            _ => true,
        },
        ("wireless","wifi-device") => match key {
            "txpower" => integer(v, 0, 40),
            "disabled" | "legacy_rates" | "noscan" => boolean(v),
            "beacon_int" => integer(v, 15, 65535),
            "distance" => unsigned_integer(v, 0),
            "macaddr" => macs(v, false),
            "channel" => v == "auto" || integer(v, 1, 233),
            _ => true,
        },
        ("wireless","wifi-iface") => match key {
            "disabled" | "hidden" | "isolate" | "wds" | "wmm" | "ieee80211r" | "ieee80211k" | "mesh_fwding" => boolean(v),
            "maxassoc" => unsigned_integer(v, 0),
            "dtim_period" => integer(v, 1, 255),
            "auth_port" | "acct_port" => integer(v, 1, 65535),
            "bssid" | "maclist" => macs(v, false),
            _ => true,
        },
        ("dhcp","dnsmasq") => match key {
            "domainneeded" | "boguspriv" | "filterwin2k" | "localise_queries" | "rebind_protection" | "rebind_localhost" | "expandhosts" | "authoritative" | "readethers" | "noresolv" | "nohosts" | "nonwildcard" | "localservice" | "strictorder" | "allservers" | "logqueries" | "logdhcp" => boolean(v),
            "port" | "queryport" => integer(v, 0, 65535),
            "cachesize" | "dnsforwardmax" | "dhcpleasemax" => unsigned_integer(v, 0),
            "ednspacket_max" => integer(v, 512, 65535),
            "domain" => domain(v),
            "listen_address" => ips(v),
            _ => true,
        },
        ("dhcp","dhcp") => match key {
            "start" | "limit" => integer(v, 0, 65535),
            "ignore" | "force" | "dynamicdhcp" | "master" | "ra_slaac" => boolean(v),
            "ra_mininterval" | "ra_maxinterval" | "ra_lifetime" => unsigned_integer(v, 0),
            "ra_mtu" => integer(v, 1280, 65535),
            "dns" => ips(v),
            "domain" => domain(v),
            "netmask" => netmask(v),
            _ => true,
        },
        ("dhcp","host") => match key {
            "dns" | "broadcast" => boolean(v),
            "ip" => v == "ignore" || ips(v),
            "mac" => macs(v, true),
            _ => true,
        },
        ("dhcp","domain") => match key {
            "ip" => ips(v),
            _ => true,
        },
        ("dhcp","odhcpd") => match key {
            "maindhcp" => boolean(v),
            "loglevel" => integer(v, 0, 7),
            _ => true,
        },
        ("dhcp","cname") => match key {
            "ttl" => unsigned_integer(v, 0),
            _ => true,
        },
        ("dhcp","boot") => match key {
            "serveraddress" => ips(v),
            _ => true,
        },
        ("dhcp","relay") => match key {
            "local_addr" | "server_addr" => ips(v),
            _ => true,
        },
        ("dhcp","srvhost") => match key {
            "port" => integer(v, 1, 65535),
            "class" | "weight" => integer(v, 0, 65535),
            _ => true,
        },
        ("dhcp","mxhost") => match key {
            "pref" => integer(v, 0, 65535),
            "domain" => domain(v),
            _ => true,
        },
        ("firewall","defaults") => match key {
            "synflood_protect" | "drop_invalid" | "flow_offloading" | "flow_offloading_hw" | "disable_ipv6" => boolean(v),
            "input" | "forward" | "output" => v == "ACCEPT" || v == "REJECT" || v == "DROP",
            _ => true,
        },
        ("firewall","zone") => match key {
            "masq" | "masq6" | "mtu_fix" | "log" | "enabled" => boolean(v),
            "input" | "forward" | "output" => v == "ACCEPT" || v == "REJECT" || v == "DROP",
            _ => true,
        },
        ("firewall","forwarding") => match key {
            "enabled" => boolean(v),
            _ => true,
        },
        ("firewall","rule") => match key {
            "enabled" | "utc_time" => boolean(v),
            "limit_burst" => unsigned_integer(v, 0),
            "src_ip" | "dest_ip" => ips(v),
            "src_mac" => macs(v, false),
            "src_port" | "dest_port" => ports(v),
            _ => true,
        },
        ("firewall","redirect") => match key {
            "enabled" | "reflection" => boolean(v),
            "limit_burst" => unsigned_integer(v, 0),
            "src_ip" | "dest_ip" | "src_dip" => ips(v),
            "src_mac" => macs(v, false),
            "src_port" | "dest_port" | "src_dport" => ports(v),
            _ => true,
        },
        ("firewall","nat") => match key {
            "enabled" => boolean(v),
            "limit_burst" => unsigned_integer(v, 0),
            "src_ip" | "dest_ip" | "snat_ip" => ips(v),
            "src_mac" => macs(v, false),
            "src_port" | "dest_port" | "snat_port" => ports(v),
            _ => true,
        },
        ("firewall","include") => match key {
            "enabled" | "reload" | "fw4_compatible" => boolean(v),
            _ => true,
        },
        ("firewall","ipset") => match key {
            "maxelem" => unsigned_integer(v, 1),
            "timeout" => unsigned_integer(v, 0),
            "enabled" => boolean(v),
            _ => true,
        },
        ("system","system") => match key {
            "log_size" => unsigned_integer(v, 0),
            "log_port" => integer(v, 1, 65535),
            "log_remote" => boolean(v),
            "conloglevel" | "cronloglevel" => integer(v, 0, 8),
            "hostname" => domain(v),
            _ => true,
        },
        ("system","timeserver") => match key {
            "enabled" | "enable_server" | "use_dhcp" => boolean(v),
            _ => true,
        },
        ("system","led") => match key {
            "default" => boolean(v),
            "delayon" | "delayoff" | "interval" => unsigned_integer(v, 0),
            _ => true,
        },
        ("dropbear","dropbear") => match key {
            "Port" => integer(v, 1, 65535),
            "PasswordAuth" | "RootPasswordAuth" | "RootLogin" | "GatewayPorts" | "enable" | "mdns" => boolean(v),
            "IdleTimeout" | "SSHKeepAlive" | "MaxAuthTries" => unsigned_integer(v, 0),
            _ => true,
        },
        _ => true,
    }
}

fn references(candidates:&BTreeMap<String,String>,combined:&BTreeMap<String,String>)->Vec<Issue>{
    let network=parse(&combined["network"]).unwrap_or_default();
    let interfaces:BTreeSet<&str>=network.iter().filter(|s|s.kind=="interface"&&!s.name.is_empty()).map(|s|s.name.as_str()).collect();
    let mut issues=Vec::new();
    for(module,text)in candidates{
        let p=parse(text).unwrap_or_default();
        let radios:BTreeSet<&str>=p.iter().filter(|s|s.kind=="wifi-device"&&!s.name.is_empty()).map(|s|s.name.as_str()).collect();
        for s in &p{
            let key=match(module.as_str(),s.kind.as_str()){
                ("wireless","wifi-iface")=>{
                    if !s.one("device").is_empty()&&!radios.contains(s.one("device")){issues.push(issue("invalid_reference","Unknown wireless configuration reference: device."));}
                    "network"
                },
                ("dhcp","dhcp")=>"interface",
                ("firewall","zone")=>"network",_=>continue,
            };
            if let Some(values)=s.values.get(key){
                for value in values{
                    for name in value.split_whitespace(){
                        if !interfaces.contains(name){issues.push(issue("invalid_reference",&format!("Unknown {module} configuration reference: {key}.")));}
                    }
                }
            }
        }
    }
    issues.truncate(512);issues
}
fn reverse_references(live:&BTreeMap<String,Snapshot>,combined:&BTreeMap<String,String>,selected:&BTreeMap<String,String>)->Vec<Issue>{
    if !selected.contains_key("network"){return Vec::new();}
    let interfaces=|text:&str|->BTreeSet<String>{parse(text).unwrap_or_default().into_iter().filter(|s|s.kind=="interface"&&!s.name.is_empty()).map(|s|s.name).collect()};
    let before=interfaces(&live["network"].content);let after=interfaces(&combined["network"]);
    let removed:BTreeSet<_>=before.difference(&after).collect();let mut issues=Vec::new();
    for module in ["wireless","dhcp","firewall"]{
        if selected.contains_key(module){continue;}
        let sections=parse(&live[module].content).unwrap_or_default();
        let broken=sections.iter().any(|s|{
            let key=match(module,s.kind.as_str()){("wireless","wifi-iface")=>"network",("dhcp","dhcp")=>"interface",("firewall","zone")=>"network",_=>return false};
            s.values.get(key).is_some_and(|values|values.iter().flat_map(|v|v.split_whitespace()).any(|name|removed.iter().any(|r|r.as_str()==name)))
        });
        if broken{issues.push(issue("invalid_reference",&format!("Network changes would break current {module} references; select its matching configuration document.")));}
    }
    issues
}
fn lan_projection(sections:&[Section])->BTreeMap<String,BTreeMap<String,Vec<String>>>{
    let mut result=BTreeMap::new();let mut devices=BTreeSet::new();
    for s in sections{
        if s.kind=="interface"&&s.name=="lan"{
            let fields=["device","ifname","proto","ipaddr","netmask","ip6addr","ip6assign","ip6hint","disabled","auto","type"];
            result.insert("lan".into(),s.values.iter().filter(|(key,_)|fields.contains(&key.as_str())).map(|(k,v)|(k.clone(),v.clone())).collect());
            for key in ["device","ifname"]{if let Some(v)=s.values.get(key){devices.extend(v.iter().cloned());}}
        }
    }
    for s in sections{
        if s.kind=="device"&&devices.contains(s.one("name")){result.insert(format!("device:{}",s.one("name")),s.values.clone());}
    }
    result
}
fn execution_changes(module:&str,before:&str,after:&str)->Vec<Issue>{
    let a=parse(before).unwrap_or_default();let b=parse(after).unwrap_or_default();
    if module=="firewall"{
        let mut existing:BTreeMap<Section,usize>=BTreeMap::new();
        for s in a.iter().filter(|s|s.kind=="include"){*existing.entry(s.clone()).or_default()+=1;}
        for s in b.iter().filter(|s|s.kind=="include"){
            let count=existing.entry(s.clone()).or_default();
            if *count==0{return vec![error_issue(failure("execution_hook_not_allowed"))];}*count-=1;
        }
        if existing.values().any(|n|*n>0){return vec![error_issue(failure("factory_include_removed"))];}
    }
    if module=="network"{
        if lan_projection(&a)!=lan_projection(&b){return vec![error_issue(failure("management_migration_unavailable"))];}
        let existing:BTreeSet<&str>=a.iter().filter(|s|s.kind=="interface").map(|s|s.one("proto")).collect();
        for s in b.iter().filter(|s|s.kind=="interface"){
            let proto=s.one("proto");
            if !proto.is_empty()&&!existing.contains(proto)&&!matches!(proto,"dhcp"|"static"|"pppoe"|"none"|"dhcpv6"|"l2tp"|"pptp"){return vec![error_issue(failure("protocol_not_registered"))];}
        }
    }
    if module=="dropbear"{
        // Protect existing factory/recovery listeners. Other SSH fields still use
        // provisional risk confirmation, including password-auth changes.
        let enabled=|s:&Section|s.kind=="dropbear"&&!s.disabled()&&!falsey(s.one("enable"))&&!falsey(s.one("RootLogin"));
        for s in a.iter().filter(|s|enabled(s)){
            let port=if s.one("Port").is_empty(){"22"}else{s.one("Port")};
            if matches!(port,"22"|"2222")&&!b.iter().filter(|s|enabled(s)).any(|n|{
                let next_port=if n.one("Port").is_empty(){"22"}else{n.one("Port")};
                next_port==port&&n.one("Interface")==s.one("Interface")
            }){return vec![error_issue(failure("rescue_access_required"))];}
        }
    }
    Vec::new()
}
fn project(sections:&[Section],kind:&str,name:Option<&str>,keys:&[&str])->Vec<(String,String,BTreeMap<String,Vec<String>>)>{
    sections.iter().filter(|s|s.kind==kind&&name.is_none_or(|n|s.name==n)).map(|s|{
        (s.kind.clone(),s.name.clone(),s.values.iter().filter(|(k,_)|keys.contains(&k.as_str())).map(|(k,v)|(k.clone(),v.clone())).collect())
    }).collect()
}
fn management_projection(sections:&[Section])->Vec<(String,String,BTreeMap<String,Vec<String>>)>{
    let keys=["Port","Interface","PasswordAuth","RootPasswordAuth","RootLogin","port","disabled","enabled","auth","rootdisabled","listen_http","listen_https"];
    sections.iter().map(|s|(s.kind.clone(),s.name.clone(),s.values.iter().filter(|(k,_)|keys.contains(&k.as_str())).map(|(k,v)|(k.clone(),v.clone())).collect())).collect()
}
fn input_rule(s:&Section)->bool{
    let destinations:Vec<_>=s.values.get("dest").into_iter().flatten().flat_map(|v|v.split_whitespace()).collect();
    destinations.is_empty()||destinations.contains(&"*")
}
fn firewall_projection(sections:&[Section])->Vec<Section>{
    let mut out=Vec::new();
    for s in sections{
        if matches!(s.kind.as_str(),"defaults"|"zone"){
            let keys=["name","input","forward","output","network","device"];
            out.push(Section{kind:s.kind.clone(),name:s.name.clone(),values:s.values.iter().filter(|(k,_)|keys.contains(&k.as_str())).map(|(k,v)|(k.clone(),v.clone())).collect()});
        }
        if s.kind!="rule"||s.disabled()||falsey(s.one("enabled")){continue;}
        let risk=match s.one("target"){
            "DROP"|"REJECT"=>input_rule(s)||s.one("dest_ip").is_empty()||s.one("src_ip").is_empty(),
            "ACCEPT"=>input_rule(s),_=>false,
        };
        if risk{out.push(s.clone());}
    }
    out
}
fn primary_wifi_projection(sections:&[Section])->Vec<Section>{
    let mut out=Vec::new();
    for s in sections{
        if s.kind=="wifi-device"{out.push(s.clone());continue;}
        if s.kind!="wifi-iface"{continue;}
        let ifname=s.one("ifname");
        if ifname.to_ascii_lowercase().contains("guest")||ifname.starts_with("bh")||ifname.ends_with(".1"){continue;}
        let keys=["ifname","ssid","key","encryption","disabled","device","network","mode"];
        out.push(Section{kind:s.kind.clone(),name:s.name.clone(),values:s.values.iter().filter(|(k,_)|keys.contains(&k.as_str())).map(|(k,v)|(k.clone(),v.clone())).collect()});
    }
    out
}
fn active_wifi(sections:&[Section])->usize{
    let radios:BTreeMap<&str,bool>=sections.iter().filter(|s|s.kind=="wifi-device").map(|s|(s.name.as_str(),!s.disabled())).collect();
    sections.iter().filter(|s|s.kind=="wifi-iface"&&!s.disabled()&&radios.get(s.one("device")).copied().unwrap_or(true)).count()
}
fn risks(module:&str,before:&str,after:&str)->Vec<Issue>{
    let a=parse(before).unwrap_or_default();let b=parse(after).unwrap_or_default();let mut out=Vec::new();
    match module{
        "network"=>{
            if before!=after{out.push(issue("network_reload","Network reload can disconnect LAN and Wi-Fi management sessions."));}
            let keys=["ipaddr","ip6addr","netmask","device","ifname","proto","disabled"];
            if project(&a,"interface",Some("lan"),&keys)!=project(&b,"interface",Some("lan"),&keys){out.push(issue("management_network","LAN address, mask or management interface changes can disconnect this panel."));}
            let keys=["device","ifname","proto","disabled"];
            if ["wan","wan6"].iter().any(|n|project(&a,"interface",Some(n),&keys)!=project(&b,"interface",Some(n),&keys)){out.push(issue("wan_replacement","WAN interface changes can interrupt connectivity."));}
        },
        "dropbear"|"system"=>{
            if management_projection(&a)!=management_projection(&b){out.push(issue("management_service","SSH port, authentication or service availability changes can remove recovery access."));}
        },
        "firewall"=>{
            if firewall_projection(&a)!=firewall_projection(&b){out.push(issue("firewall_policy","Firewall input, zone membership or broad policies can block management or forwarding."));}
        },
        "wireless"=>{
            if primary_wifi_projection(&a)!=primary_wifi_projection(&b){out.push(issue("wifi_access","Primary Wi-Fi credentials or radio settings can disconnect this browser."));}
            if active_wifi(&a)>0&&active_wifi(&b)==0{out.push(issue("all_wifi_disabled","Disabling all primary Wi-Fi access can disconnect clients."));}
        },_=>(),
    }
    out
}
fn document_diff(module:&str,before:&str,after:&str)->String{
    if before==after{return String::new();}
    let lines=|s:&str|if s.is_empty(){Vec::<String>::new()}else{s.strip_suffix('\n').unwrap_or(s).split('\n').map(str::to_owned).collect()};
    let a=lines(before);let b=lines(after);
    let mut diff=format!("--- a/{module}\n+++ b/{module}\n@@ -1,{} +1,{} @@\n",a.len(),b.len());
    for line in a{diff.push('-');diff.push_str(&line);diff.push('\n');}
    for line in b{diff.push('+');diff.push_str(&line);diff.push('\n');}
    if !after.is_empty()&&!after.ends_with('\n'){diff.push_str("\\ No newline at end of file\n");}
    diff
}

/// Pure, bounded import preview shared by maintenance and the native editor.
/// Arguments are public document arrays, not private durable state records.
/// Native UCI checks and generation CAS still run in stage and commit.
pub fn preview_documents(current:&Value,candidates:&Value)->Result<Value,ApiError>{
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Document{module:String,content:String}
    fn docs(value:&Value)->Result<Vec<Document>,ApiError>{
        let array=value.as_array().ok_or_else(||failure("invalid_request"))?;
        if array.len()>6{return Err(failure("invalid_request"));}
        array.iter().map(|v|{
            let object=v.as_object().ok_or_else(||failure("invalid_request"))?;
            if object.len()!=2{return Err(failure("invalid_request"));}
            let module=object.get("module").and_then(Value::as_str).ok_or_else(||failure("invalid_request"))?;
            let content=object.get("content").and_then(Value::as_str).ok_or_else(||failure("invalid_request"))?;
            if module.len()>128||content.len()>MAX_DOCUMENT_BYTES{return Err(failure("document_too_large"));}
            Ok(Document{module:module.into(),content:content.into()})
        }).collect()
    }
    let baseline=docs(current)?;let selected=docs(candidates)?;
    let mut live:BTreeMap<String,Snapshot>=MODULES.iter().map(|m|((*m).into(),Snapshot{exists:false,content:String::new(),mode:0o600})).collect();
    let mut names=BTreeSet::new();
    for d in baseline{
        if !allowed(&d.module)||!names.insert(d.module.clone()){return Err(failure("invalid_request"));}
        live.insert(d.module,Snapshot{exists:true,content:d.content,mode:0o600});
    }
    let mut set=BTreeMap::new();
    for d in &selected{if allowed(&d.module){set.insert(d.module.clone(),d.content.clone());}}
    let combined=combined_documents(&live,&set);let mut seen=BTreeSet::new();let mut out=Vec::new();
    for d in selected{
        let mut errors=if !allowed(&d.module){vec![error_issue(failure("module_not_allowed"))]}
            else if !seen.insert(d.module.clone()){vec![issue("duplicate_module","Select only one document for each native module.")]}
            else{validate(&d.module,&d.content)};
        let before=live.get(&d.module).map_or("",|s|s.content.as_str());
        if errors.is_empty(){errors=execution_changes(&d.module,before,&d.content);}
        let mut dependencies=Vec::new();
        if errors.is_empty(){
            dependencies=references(&BTreeMap::from([(d.module.clone(),d.content.clone())]),&combined);
            if d.module=="network"{dependencies.extend(reverse_references(&live,&combined,&set));}
        }
        let sort=|issues:&mut Vec<Issue>|issues.sort_by(|a,b|a.code.cmp(&b.code).then(a.message.cmp(&b.message)));
        sort(&mut errors);sort(&mut dependencies);let mut risk=risks(&d.module,before,&d.content);sort(&mut risk);
        out.push(json!({"module":d.module,"valid":errors.is_empty(),"diff":document_diff(&d.module,before,&d.content),"errors":errors,"dependencies":dependencies,"risks":risk}));
    }
    Ok(Value::Array(out))
}

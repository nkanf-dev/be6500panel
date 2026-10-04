//! Source-only, in-process capture intent and retained cleanup ownership.
//!
//! One future Rust manager calls these ordinary `&mut` methods. This is not a
//! daemon, executor, HTTP action, or kernel observer. `open` only reads existing
//! private files. A loaded journal is staged ownership, never installed proof.
//! Stored Apply/OnFailure are ignored; cleanup is regenerated and compared.
//!
//! Callbacks must honor the supplied finite deadline. We check time again on
//! return, but cannot bound an uncooperative callback's runtime. Failed Apply
//! gets an independent cleanup budget, including after cancellation. No process
//! executor is supplied. ActiveByApply means all Apply callbacks succeeded in
//! this process; it is NOT future GET/actual-resource observation proof.
//!
//! Desired is limited to 64 KiB. Journals are limited to 1 MiB, independently of
//! the compiler's 8192-command bound. Serialization measures the complete bytes
//! before admission. Actual fstatvfs admission preserves 1 MiB plus temporary
//! growth (bytes + 4096); no existing reserve or unrelated files are touched.
use crate::capture_plan::{self, OwnedRulesPlan, RulesOwnership, RulesPlanInput};
use crate::native::Ports;
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::ffi::{CStr, CString};
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

pub const MAX_DESIRED_BYTES: usize = 64 << 10;
pub const MAX_JOURNAL_BYTES: usize = 1 << 20;
pub const MAX_OUTPUT_BYTES: usize = 64 << 10;
pub const FREE_HEADROOM_BYTES: u64 = 1 << 20;
const OPERATION_BUDGET: Duration = Duration::from_secs(30);
const COMMAND_BUDGET: Duration = Duration::from_secs(8);
const DESIRED: &CStr = c"capture-desired.json";
const JOURNAL: &CStr = c"capture-journal.json";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DeviceSelection {
    pub mac: String,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Desired {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub scope: String,
    #[serde(
        rename = "lanIPv4Prefixes",
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "nil_vec"
    )]
    pub lan_ipv4_prefixes: Vec<String>,
    pub desired: bool,
    #[serde(
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "nil_devices"
    )]
    pub devices: Vec<DeviceSelection>,
    #[serde(rename = "clientIPv4", skip_serializing_if = "String::is_empty")]
    pub client_ipv4: String,
    #[serde(rename = "clientIPv6", skip_serializing_if = "String::is_empty")]
    pub client_ipv6: String,
    pub ipv6: String,
}
impl Default for Desired {
    fn default() -> Self {
        Self {
            scope: String::new(),
            lan_ipv4_prefixes: vec![],
            desired: false,
            devices: vec![],
            client_ipv4: String::new(),
            client_ipv6: String::new(),
            ipv6: "direct".into(),
        }
    }
}
impl fmt::Debug for Desired {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Desired")
            .field("desired", &self.desired)
            .field("scope", &self.scope)
            .finish_non_exhaustive()
    }
}
fn nil_vec<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    Ok(Option::<Vec<String>>::deserialize(d)?.unwrap_or_default())
}
fn nil_devices<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<DeviceSelection>, D::Error> {
    Ok(Option::<Vec<DeviceSelection>>::deserialize(d)?.unwrap_or_default())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Off,
    Staged,
    ActiveByApply,
    CleanupPending,
}
/// No hit counters, installed clients, or fabricated observation results.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub phase: Phase,
    pub desired: bool,
    pub cleanup_pending: bool,
    pub disable_not_persisted: bool,
    pub pending_cleanup_commands: usize,
    pub storage_uncertain: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub desired: Desired,
    pub status: Status,
    pub ownership: Option<RulesOwnership>,
    /// Original compiler input, read-only metadata; never replay authority.
    pub input: Option<RulesPlanInput>,
}
/// Fixed, private-safe error text. Kernel diagnostics and argv are never shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Storage,
    UnsafeFile,
    TooLarge,
    InvalidDesired,
    InvalidJournal,
    LegacyJournal,
    InsufficientSpace,
    Measurement,
    Durability,
    AlreadyOwned,
    DesiredOff,
    IntentMismatch,
    Plan,
    Preflight,
    Deadline,
    CleanupFailed,
    ApplyFailed {
        cleanup_failed: bool,
    },
    DisableFailed {
        persistence_failed: bool,
        cleanup_failed: bool,
    },
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Storage => "capture storage unavailable",
            Self::UnsafeFile => "capture storage is not private regular pinned state",
            Self::TooLarge => "capture state exceeds size limit",
            Self::InvalidDesired => "capture desired state invalid",
            Self::InvalidJournal => {
                "capture journal does not match fixed compiled cleanup ownership"
            }
            Self::LegacyJournal => {
                "legacy capture journal retained; dedicated withdrawal migration required"
            }
            Self::InsufficientSpace => {
                "capture storage needs temporary growth and recovery headroom"
            }
            Self::Measurement => "capture storage free space unavailable",
            Self::Durability => "capture state durability uncertain",
            Self::AlreadyOwned => "capture ownership already staged; cleanup first",
            Self::DesiredOff => "capture desired state is off",
            Self::IntentMismatch => "capture input does not match selected desired scope",
            Self::Plan => "capture input invalid",
            Self::Preflight => "capture preflight failed",
            Self::Deadline => "capture callback deadline exceeded",
            Self::CleanupFailed => "capture cleanup pending",
            Self::ApplyFailed { .. } => "capture apply failed",
            Self::DisableFailed { .. } => {
                "capture disable incomplete; retry before process restart"
            }
        })
    }
}
impl std::error::Error for Error {}

/// Narrow internal-manager/test seam, not a public HTTP preflight success flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreflightError {
    Refused,
    Cancelled,
    Timeout,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandError {
    Failure,
    Cancelled,
    Timeout,
}
/// Nonzero native exit may return `success: false` with bounded fixed output.
/// Only exact cleanup-specific native diagnostics can establish absence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandResult {
    pub success: bool,
    pub output: Vec<u8>,
}
impl CommandResult {
    pub fn success() -> Self {
        Self {
            success: true,
            output: vec![],
        }
    }
}

#[derive(Serialize)]
struct Journal<'a> {
    #[serde(flatten)]
    plan: &'a OwnedRulesPlan,
    input: &'a RulesPlanInput,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredJournal {
    // Untrusted stored execution fields are parsed only as JSON and discarded.
    #[serde(rename = "Apply", alias = "apply", default)]
    _apply: Value,
    #[serde(rename = "Cleanup", alias = "cleanup")]
    cleanup: Vec<Vec<String>>,
    #[serde(rename = "OnFailure", alias = "onFailure", default)]
    _on_failure: Value,
    #[serde(rename = "Ownership", alias = "ownership")]
    ownership: RulesOwnership,
    #[serde(rename = "Warnings", alias = "warnings", default)]
    _warnings: Value,
    #[serde(alias = "Input", default)]
    input: Option<RulesPlanInput>,
}
struct OwnedState {
    plan: OwnedRulesPlan,
    input: Option<RulesPlanInput>,
    raw: Vec<u8>,
}
struct Directory {
    path: PathBuf,
    file: File,
    identity: (u64, u64),
}
/// No lock, background task, Drop command, generic store, or secondary owner.
/// The future manager serializes all mutations through `&mut Controller`.
pub struct Controller {
    directory: Directory,
    desired: Desired,
    desired_pin: Option<File>,
    journal_pin: Option<File>,
    owned: Option<OwnedState>,
    phase: Phase,
    disable_not_persisted: bool,
    pending_cleanup_commands: usize,
    storage_uncertain: bool,
    #[cfg(test)]
    fault: Option<Fault>,
}
impl fmt::Debug for Controller {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Controller")
            .field("status", &self.status())
            .finish_non_exhaustive()
    }
}
impl Controller {
    /// Existing directory required. Reads only; never creates, resets, removes,
    /// restores desired capture, inspects kernel state, or calls a runner.
    pub fn open(data_dir: impl AsRef<Path>) -> Result<Self, Error> {
        let path = absolute_path(data_dir.as_ref())?;
        let file = open_directory(&path)?;
        let meta = file.metadata().map_err(|_| Error::Storage)?;
        private(&meta, true, u64::MAX)?;
        let directory = Directory {
            path,
            file,
            identity: identity(&meta),
        };
        let (desired_pin, desired_raw) = read_state(&directory.file, DESIRED, MAX_DESIRED_BYTES)?;
        let desired = match desired_raw {
            Some(raw) => normalize_desired(parse_strict(&raw).map_err(|_| Error::InvalidDesired)?)?,
            None => Desired::default(),
        };
        let (journal_pin, journal_raw) = read_state(&directory.file, JOURNAL, MAX_JOURNAL_BYTES)?;
        let owned = journal_raw.map(recover).transpose()?;
        directory.checked()?;
        let phase = if owned.is_some() {
            Phase::Staged
        } else {
            Phase::Off
        };
        let pending_cleanup_commands = owned.as_ref().map_or(0, |o| o.plan.cleanup.len());
        Ok(Self {
            directory,
            desired,
            desired_pin,
            journal_pin,
            owned,
            phase,
            disable_not_persisted: false,
            pending_cleanup_commands,
            storage_uncertain: false,
            #[cfg(test)]
            fault: None,
        })
    }
    pub fn desired(&self) -> Desired {
        self.desired.clone()
    }
    pub fn status(&self) -> Status {
        Status {
            phase: self.phase,
            desired: self.desired.desired,
            cleanup_pending: self.owned.is_some() && self.phase != Phase::ActiveByApply,
            disable_not_persisted: self.disable_not_persisted,
            pending_cleanup_commands: self.pending_cleanup_commands,
            storage_uncertain: self.storage_uncertain,
        }
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            desired: self.desired(),
            status: self.status(),
            ownership: self.owned.as_ref().map(|o| o.plan.ownership.clone()),
            input: self.owned.as_ref().and_then(|o| o.input.clone()),
        }
    }
    /// Internal cleanup admission from the validated journal only. It does not
    /// authorize restoration; that always needs fresh accepted configuration
    /// and LAN intent from the runtime integration owner.
    pub(crate) fn cleanup_input(&self) -> Option<RulesPlanInput> {
        let owned = self.owned.as_ref()?;
        Some(
            owned
                .input
                .clone()
                .unwrap_or_else(|| recovery_input(&owned.plan.ownership)),
        )
    }

    /// Saves user intent only. No automatic Apply/cleanup; no saved listener,
    /// interface, last-IP device observation, or command authority.
    pub fn set_desired(&mut self, desired: Desired) -> Result<(), Error> {
        let desired = normalize_desired(desired)?;
        let raw = serialize_bounded(&desired, MAX_DESIRED_BYTES)?;
        let save = self.save(DESIRED, &raw, self.desired_pin.as_ref())?;
        self.desired_pin = Some(save.file);
        // Rename committed the accepted bytes even if directory sync failed.
        // Preserve a failed disable latch until an explicit durable save.
        if save.durable || !self.disable_not_persisted || !desired.desired {
            self.desired = desired;
        }
        if !save.durable {
            self.storage_uncertain = true;
            return Err(Error::Durability);
        }
        self.disable_not_persisted = false;
        self.storage_uncertain = false;
        Ok(())
    }
    /// Fresh native accepted input only. A staged/recovered journal must first
    /// be withdrawn. Preflight occurs before persistence and every Apply call.
    pub fn apply<P, R>(
        &mut self,
        input: RulesPlanInput,
        mut preflight: P,
        mut runner: R,
    ) -> Result<(), Error>
    where
        P: FnMut(&RulesPlanInput, &OwnedRulesPlan, Instant) -> Result<(), PreflightError>,
        R: FnMut(&[String], Instant) -> Result<CommandResult, CommandError>,
    {
        if self.owned.is_some() {
            return Err(Error::AlreadyOwned);
        }
        if !self.desired.desired {
            return Err(Error::DesiredOff);
        }
        let plan = capture_plan::plan_owned_rules(&input).map_err(|_| Error::Plan)?;
        desired_matches(&self.desired, &plan.ownership)?;
        let raw = serialize_bounded(
            &Journal {
                plan: &plan,
                input: &input,
            },
            MAX_JOURNAL_BYTES,
        )?;
        let deadline = Instant::now() + OPERATION_BUDGET;
        preflight(&input, &plan, deadline).map_err(|_| Error::Preflight)?;
        if Instant::now() >= deadline {
            return Err(Error::Deadline);
        }
        let save = self.save(JOURNAL, &raw, self.journal_pin.as_ref())?;
        self.journal_pin = Some(save.file);
        self.pending_cleanup_commands = plan.cleanup.len();
        self.owned = Some(OwnedState {
            plan,
            input: Some(input),
            raw,
        });
        self.phase = Phase::Staged;
        if !save.durable {
            self.storage_uncertain = true;
            return Err(Error::Durability);
        }
        for argv in &self.owned.as_ref().expect("staged").plan.apply {
            if execute(&mut runner, argv, deadline, false).is_err() {
                let cleanup_failed = self.cleanup(&mut runner).is_err();
                return Err(Error::ApplyFailed { cleanup_failed });
            }
        }
        self.phase = Phase::ActiveByApply;
        self.pending_cleanup_commands = 0;
        self.storage_uncertain = false;
        Ok(())
    }
    /// ALL internally compiled cleanup commands are attempted after failures,
    /// until the independent 30-second budget expires. Failed/unattempted
    /// commands remain counted. Err must prevent runtime TERM/KILL admission.
    pub fn cleanup<R>(&mut self, mut runner: R) -> Result<(), Error>
    where
        R: FnMut(&[String], Instant) -> Result<CommandResult, CommandError>,
    {
        self.cleanup_until(&mut runner, Instant::now() + OPERATION_BUDGET)
    }
    pub(crate) fn cleanup_until<R>(&mut self, mut runner: R, deadline: Instant) -> Result<(), Error>
    where
        R: FnMut(&[String], Instant) -> Result<CommandResult, CommandError>,
    {
        let Some(owned) = &self.owned else {
            return Ok(());
        };
        self.phase = Phase::CleanupPending;
        let mut pending = 0;
        for (index, argv) in owned.plan.cleanup.iter().enumerate() {
            if Instant::now() >= deadline {
                pending += owned.plan.cleanup.len() - index;
                break;
            }
            if execute(&mut runner, argv, deadline, true).is_err() {
                pending += 1;
            }
        }
        self.pending_cleanup_commands = pending;
        if pending != 0 {
            return Err(Error::CleanupFailed);
        }
        if let Err(error) = self.remove_journal() {
            self.storage_uncertain = true;
            // Removal may have happened before fsync failed. Restore the exact
            // retained validated bytes where possible; never declare Off.
            let missing = check_pin(&self.directory.file, JOURNAL, None, MAX_JOURNAL_BYTES).is_ok();
            if missing {
                let raw = self.owned.as_ref().expect("retained").raw.clone();
                if let Ok(save) = self.save(JOURNAL, &raw, None) {
                    self.journal_pin = Some(save.file);
                }
            }
            return Err(error);
        }
        self.owned = None;
        self.journal_pin = None;
        self.phase = Phase::Off;
        self.storage_uncertain = false;
        Ok(())
    }
    /// Latches off BEFORE normalization/admission/write, then always attempts
    /// best-effort owned cleanup. GET/snapshot/drop never clear a failed latch.
    /// Gateway ranges and device identities remain selected for explicit retry.
    pub fn disable<R>(&mut self, runner: R) -> Result<(), Error>
    where
        R: FnMut(&[String], Instant) -> Result<CommandResult, CommandError>,
    {
        self.desired.desired = false;
        self.disable_not_persisted = true;
        let desired = self.desired.clone();
        let persistence_failed = self.set_desired(desired).is_err();
        let cleanup_failed = self.cleanup(runner).is_err();
        if persistence_failed {
            self.disable_not_persisted = true;
        }
        if persistence_failed || cleanup_failed {
            Err(Error::DisableFailed {
                persistence_failed,
                cleanup_failed,
            })
        } else {
            Ok(())
        }
    }
    fn save(&self, name: &CStr, raw: &[u8], pin: Option<&File>) -> Result<Saved, Error> {
        self.directory.checked()?;
        check_pin(&self.directory.file, name, pin, file_limit(name))?;
        self.inject(Fault::BeforeSave)?;
        admit(&self.directory.file, raw.len())?;
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).map_err(|_| Error::Storage)?;
        let temporary = CString::new(format!(
            ".capture-{}",
            random
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        ))
        .map_err(|_| Error::Storage)?;
        let mut file = open_at(
            &self.directory.file,
            &temporary,
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
            0o600,
        )
        .map_err(|_| Error::Storage)?;
        let result = (|| {
            private(
                &file.metadata().map_err(|_| Error::Storage)?,
                false,
                file_limit(name) as u64,
            )?;
            file.write_all(raw).map_err(|_| Error::Storage)?;
            file.sync_all().map_err(|_| Error::Storage)?;
            self.directory.checked()?;
            check_pin(&self.directory.file, name, pin, file_limit(name))?;
            check_pin(
                &self.directory.file,
                &temporary,
                Some(&file),
                file_limit(name),
            )?;
            // SAFETY: both names and the pinned directory are live and owned.
            if unsafe {
                libc::renameat(
                    self.directory.file.as_raw_fd(),
                    temporary.as_ptr(),
                    self.directory.file.as_raw_fd(),
                    name.as_ptr(),
                )
            } != 0
            {
                return Err(Error::Storage);
            }
            let durable = self
                .inject(Fault::AfterRename)
                .and_then(|()| {
                    self.directory
                        .file
                        .sync_all()
                        .map_err(|_| Error::Durability)
                })
                .and_then(|()| self.directory.checked())
                .is_ok();
            Ok(durable)
        })();
        if result.is_err() {
            unlink_owned(&self.directory.file, &temporary, &file);
        }
        result.map(|durable| Saved { file, durable })
    }
    fn remove_journal(&mut self) -> Result<(), Error> {
        self.directory.checked()?;
        self.inject(Fault::BeforeRemove)?;
        check_pin(
            &self.directory.file,
            JOURNAL,
            self.journal_pin.as_ref(),
            MAX_JOURNAL_BYTES,
        )?;
        // SAFETY: fixed validated name and pinned directory, no path traversal.
        if unsafe { libc::unlinkat(self.directory.file.as_raw_fd(), JOURNAL.as_ptr(), 0) } != 0
            && io::Error::last_os_error().kind() != io::ErrorKind::NotFound
        {
            return Err(Error::Storage);
        }
        self.journal_pin = None;
        self.inject(Fault::AfterRemove)?;
        self.directory
            .file
            .sync_all()
            .map_err(|_| Error::Durability)?;
        self.directory.checked()
    }
    fn inject(&self, fault: Fault) -> Result<(), Error> {
        #[cfg(test)]
        if self.fault == Some(fault) {
            return Err(Error::Durability);
        }
        let _ = fault;
        Ok(())
    }
}
struct Saved {
    file: File,
    durable: bool,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fault {
    BeforeSave,
    AfterRename,
    BeforeRemove,
    AfterRemove,
}

fn normalize_desired(mut d: Desired) -> Result<Desired, Error> {
    if d.ipv6.is_empty() {
        d.ipv6 = "direct".into();
    }
    if d.ipv6 != "direct" {
        return Err(Error::InvalidDesired);
    }
    match d.scope.as_str() {
        "gateway" => {
            if !d.devices.is_empty() || !d.client_ipv4.is_empty() || !d.client_ipv6.is_empty() {
                return Err(Error::InvalidDesired);
            }
            d.lan_ipv4_prefixes = capture_plan::canonical_gateway_prefixes(&d.lan_ipv4_prefixes)
                .map_err(|_| Error::InvalidDesired)?;
            return Ok(d);
        }
        "" | "devices" => {
            if !d.lan_ipv4_prefixes.is_empty() {
                return Err(Error::InvalidDesired);
            }
        }
        _ => return Err(Error::InvalidDesired),
    }
    if d.devices.len() > 64 {
        return Err(Error::InvalidDesired);
    }
    if !d.devices.is_empty() {
        if !d.client_ipv4.is_empty() {
            return Err(Error::InvalidDesired);
        }
        let mut seen = BTreeSet::new();
        for device in &mut d.devices {
            device.mac = canonical_mac(&device.mac).ok_or(Error::InvalidDesired)?;
            if !seen.insert(device.mac.clone()) {
                return Err(Error::InvalidDesired);
            }
        }
        d.devices.sort_by(|a, b| a.mac.cmp(&b.mac));
    } else if !d.client_ipv4.is_empty() {
        let ip: Ipv4Addr = d.client_ipv4.parse().map_err(|_| Error::InvalidDesired)?;
        if ip.is_unspecified() || ip.is_loopback() || ip.is_multicast() || ip == Ipv4Addr::BROADCAST
        {
            return Err(Error::InvalidDesired);
        }
        d.client_ipv4 = ip.to_string();
    } else if d.desired {
        return Err(Error::InvalidDesired);
    }
    // Old direct-mode clientIPv6 is compatibility metadata, not authority.
    if !d.client_ipv6.is_empty() {
        let ip: IpAddr = d.client_ipv6.parse().map_err(|_| Error::InvalidDesired)?;
        if !matches!(ip, IpAddr::V6(v6) if v6.to_ipv4_mapped().is_none() && !v6.is_unspecified() && !v6.is_loopback() && !v6.is_multicast())
        {
            return Err(Error::InvalidDesired);
        }
    }
    d.client_ipv6.clear();
    Ok(d)
}
fn canonical_mac(raw: &str) -> Option<String> {
    let text = if raw.len() == 17 && matches!(raw.as_bytes()[2], b':' | b'-') {
        let separator = raw.as_bytes()[2] as char;
        if raw.split(separator).count() != 6 {
            return None;
        }
        raw.replace(separator, ":")
    } else if raw.len() == 14 {
        let groups = raw.split('.').collect::<Vec<_>>();
        if groups.len() != 3
            || groups
                .iter()
                .any(|g| g.len() != 4 || !g.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return None;
        }
        groups
            .join("")
            .as_bytes()
            .chunks(2)
            .map(|b| std::str::from_utf8(b).ok())
            .collect::<Option<Vec<_>>>()?
            .join(":")
    } else {
        return None;
    };
    let parts = text.split(':').collect::<Vec<_>>();
    if parts.len() != 6
        || parts
            .iter()
            .any(|p| p.len() != 2 || !p.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return None;
    }
    let bytes = parts
        .iter()
        .map(|p| u8::from_str_radix(p, 16).ok())
        .collect::<Option<Vec<_>>>()?;
    if bytes[0] & 1 != 0 || bytes.iter().all(|b| *b == 0) {
        return None;
    }
    Some(
        bytes
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(":"),
    )
}
fn desired_matches(d: &Desired, o: &RulesOwnership) -> Result<(), Error> {
    let matches = if d.scope == "gateway" {
        o.scope == "gateway" && d.lan_ipv4_prefixes == o.lan_ipv4_prefixes
    } else if o.scope == "gateway" {
        false
    } else if d.devices.is_empty() {
        o.client_ipv4 == d.client_ipv4 && o.client_ipv4s.is_empty()
            || o.client_ipv4.is_empty() && o.client_ipv4s == [d.client_ipv4.clone()]
    } else {
        let wanted: BTreeSet<_> = d.devices.iter().map(|d| d.mac.as_str()).collect();
        let actual: BTreeSet<_> = o.client_macs.values().map(String::as_str).collect();
        wanted == actual
    };
    if matches {
        Ok(())
    } else {
        Err(Error::IntentMismatch)
    }
}
fn recovery_input(o: &RulesOwnership) -> RulesPlanInput {
    RulesPlanInput {
        scope: o.scope.clone(),
        lan_ipv4_prefixes: o.lan_ipv4_prefixes.clone(),
        datapath: o.datapath.clone(),
        tun_interface: o.tun_interface.clone(),
        tun_address: o.tun_address.clone(),
        client_ipv4: o.client_ipv4.clone(),
        client_ipv6: o.client_ipv6.clone(),
        client_ipv4s: (!o.client_ipv4s.is_empty()).then(|| o.client_ipv4s.clone()),
        client_ipv6s: (!o.client_ipv6s.is_empty()).then(|| o.client_ipv6s.clone()),
        client_macs: (o.scope != "gateway").then(|| o.client_macs.clone()),
        lan_interface: o.lan_interface.clone(),
        ipv6: "direct".into(),
        failure: "direct".into(),
        ports: Ports {
            mixed: 2080,
            tproxy: 7893,
            dns: 1053,
        },
        ..RulesPlanInput::default()
    }
}

fn recover(raw: Vec<u8>) -> Result<OwnedState, Error> {
    let stored: StoredJournal = parse_strict(&raw).map_err(|_| Error::InvalidJournal)?;
    if stored.ownership.datapath != "routed-tun" {
        return Err(Error::LegacyJournal);
    }
    let o = &stored.ownership;
    let input = stored.input.clone().unwrap_or_else(|| recovery_input(o));
    let plan = capture_plan::plan_owned_rules(&input).map_err(|_| Error::InvalidJournal)?;
    if plan.ownership != stored.ownership || plan.cleanup != stored.cleanup {
        return Err(Error::InvalidJournal);
    }
    Ok(OwnedState {
        plan,
        input: stored.input,
        raw,
    })
}
fn execute<R>(
    runner: &mut R,
    argv: &[String],
    budget: Instant,
    cleanup: bool,
) -> Result<(), CommandError>
where
    R: FnMut(&[String], Instant) -> Result<CommandResult, CommandError>,
{
    if Instant::now() >= budget {
        return Err(CommandError::Timeout);
    }
    let deadline = budget.min(Instant::now() + COMMAND_BUDGET);
    let result = runner(argv, deadline);
    if Instant::now() >= deadline {
        return Err(CommandError::Timeout);
    }
    let result = result?;
    if result.output.len() > MAX_OUTPUT_BYTES {
        return Err(CommandError::Failure);
    }
    if result.success || cleanup && resource_absent(argv, &result.output) {
        Ok(())
    } else {
        Err(CommandError::Failure)
    }
}
fn resource_absent(argv: &[String], output: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(output) else {
        return false;
    };
    let text = text.trim();
    if argv.len() > 3 && argv[0] == "ip" && argv[3] == "del" {
        return match argv[2].as_str() {
            "route" => text == "RTNETLINK answers: No such process",
            "rule" => matches!(
                text,
                "RTNETLINK answers: No such process"
                    | "RTNETLINK answers: No such file or directory"
            ),
            _ => false,
        };
    }
    if argv.len() <= 6 || argv[0] != "iptables" {
        return false;
    }
    let op = argv[5].as_str();
    let no_chain = matches!(
        text,
        "iptables: No chain/target/match by that name." | "No chain/target/match by that name."
    );
    if no_chain {
        if matches!(op, "-F" | "-X") && argv.len() == 7 {
            return true;
        }
        return op == "-D"
            && matches!(argv[6].as_str(), "PREROUTING" | "FORWARD")
            && !argv.iter().any(|a| a == "--mac-source");
    }
    op == "-D"
        && matches!(
            text,
            "iptables: Bad rule (does a matching rule exist in that chain?)."
                | "Bad rule (does a matching rule exist in that chain?)."
        )
}
fn serialize_bounded<T: Serialize>(value: &T, limit: usize) -> Result<Vec<u8>, Error> {
    struct BoundedWriter {
        bytes: Vec<u8>,
        limit: usize,
        full: bool,
    }
    impl Write for BoundedWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
                self.full = true;
                return Err(io::Error::other("capture size limit"));
            }
            // Grow geometrically, but never request capacity over the cap.
            let needed = self.bytes.len() + bytes.len();
            if needed > self.bytes.capacity() {
                let capacity = self
                    .bytes
                    .capacity()
                    .saturating_mul(2)
                    .max(needed)
                    .min(self.limit);
                self.bytes
                    .try_reserve_exact(capacity - self.bytes.len())
                    .map_err(|_| io::Error::other("capture allocation unavailable"))?;
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut writer = BoundedWriter {
        bytes: Vec::with_capacity(limit.min(4096)),
        limit,
        full: false,
    };
    if serde_json::to_writer(&mut writer, value).is_err() {
        return Err(if writer.full {
            Error::TooLarge
        } else {
            Error::InvalidJournal
        });
    }
    Ok(writer.bytes)
}

// Reject duplicate keys recursively, including map keys/ignored stored Apply.
// Deserialize through this one bounded Value so serde's BTreeMap cannot silently
// accept duplicate client-IP authority. No permissive unknown-field fallback.
struct StrictValue(Value);
impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct StrictVisitor;
        impl<'de> Visitor<'de> for StrictVisitor {
            type Value = StrictValue;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("JSON without duplicate keys")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::Bool(v)))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::from(v)))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::from(v)))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| StrictValue(Value::Number(n)))
                    .ok_or_else(|| E::custom("invalid number"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::String(v.into())))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::String(v)))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::Null))
            }
            fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
                self.visit_unit()
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
                let mut v = vec![];
                while let Some(StrictValue(value)) = a.next_element()? {
                    v.push(value);
                }
                Ok(StrictValue(Value::Array(v)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
                let mut v = serde_json::Map::new();
                while let Some(key) = a.next_key::<String>()? {
                    if v.contains_key(&key) {
                        return Err(de::Error::custom("duplicate key"));
                    }
                    v.insert(key, a.next_value::<StrictValue>()?.0);
                }
                Ok(StrictValue(Value::Object(v)))
            }
        }
        d.deserialize_any(StrictVisitor)
    }
}
fn parse_strict<T: de::DeserializeOwned>(raw: &[u8]) -> Result<T, serde_json::Error> {
    let StrictValue(value) = serde_json::from_slice(raw)?;
    serde_json::from_value(value)
}

fn identity(meta: &std::fs::Metadata) -> (u64, u64) {
    (meta.dev(), meta.ino())
}
fn private(meta: &std::fs::Metadata, directory: bool, max: u64) -> Result<(), Error> {
    // SAFETY: geteuid has no preconditions or side effects.
    let uid = unsafe { libc::geteuid() };
    if meta.uid() != uid
        || meta.mode() & 0o7777 != if directory { 0o700 } else { 0o600 }
        || if directory {
            !meta.is_dir()
        } else {
            !meta.is_file() || meta.nlink() != 1
        }
    {
        return Err(Error::UnsafeFile);
    }
    if meta.len() > max {
        return Err(Error::TooLarge);
    }
    Ok(())
}
fn absolute_path(path: &Path) -> Result<PathBuf, Error> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|_| Error::Storage)?
            .join(path)
    };
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(Error::UnsafeFile);
    }
    Ok(path)
}
fn open_directory(path: &Path) -> Result<File, Error> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open("/")
        .map_err(|_| Error::Storage)?;
    for component in path.components() {
        match component {
            Component::RootDir | Component::CurDir => (),
            Component::Normal(part) => {
                use std::os::unix::ffi::OsStrExt;
                let name = CString::new(part.as_bytes()).map_err(|_| Error::UnsafeFile)?;
                file = open_at(&file, &name, libc::O_RDONLY | libc::O_DIRECTORY, 0)
                    .map_err(|_| Error::UnsafeFile)?;
            }
            _ => return Err(Error::UnsafeFile),
        }
    }
    Ok(file)
}
impl Directory {
    fn checked(&self) -> Result<(), Error> {
        let file = open_directory(&self.path)?;
        let meta = file.metadata().map_err(|_| Error::Storage)?;
        private(&meta, true, u64::MAX)?;
        if identity(&meta) != self.identity {
            return Err(Error::UnsafeFile);
        }
        Ok(())
    }
}
fn open_at(dir: &File, name: &CStr, flags: i32, mode: libc::mode_t) -> io::Result<File> {
    // SAFETY: live descriptor/name; NONBLOCK prevents waiting on hostile FIFOs.
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
            mode as libc::c_uint,
        )
    };
    if fd < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}
fn read_state(
    dir: &File,
    name: &CStr,
    limit: usize,
) -> Result<(Option<File>, Option<Vec<u8>>), Error> {
    let mut file = match open_at(dir, name, libc::O_RDONLY, 0) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok((None, None)),
        Err(_) => return Err(Error::UnsafeFile),
    };
    let before = file.metadata().map_err(|_| Error::Storage)?;
    private(&before, false, limit as u64)?;
    let mut raw = vec![];
    (&mut file)
        .take(limit as u64 + 1)
        .read_to_end(&mut raw)
        .map_err(|_| Error::Storage)?;
    if raw.len() > limit {
        return Err(Error::TooLarge);
    }
    let after = file.metadata().map_err(|_| Error::Storage)?;
    if identity(&after) != identity(&before)
        || after.len() != before.len()
        || after.mtime() != before.mtime()
        || after.mtime_nsec() != before.mtime_nsec()
    {
        return Err(Error::UnsafeFile);
    }
    check_pin(dir, name, Some(&file), limit)?;
    Ok((Some(file), Some(raw)))
}
fn file_limit(name: &CStr) -> usize {
    if name == DESIRED {
        MAX_DESIRED_BYTES
    } else {
        MAX_JOURNAL_BYTES
    }
}
fn check_pin(dir: &File, name: &CStr, pin: Option<&File>, limit: usize) -> Result<(), Error> {
    match open_at(dir, name, libc::O_RDONLY, 0) {
        Err(e) if e.kind() == io::ErrorKind::NotFound && pin.is_none() => Ok(()),
        Err(_) => Err(Error::UnsafeFile),
        Ok(file) => {
            let meta = file.metadata().map_err(|_| Error::Storage)?;
            private(&meta, false, limit as u64)?;
            let expected = pin
                .ok_or(Error::UnsafeFile)?
                .metadata()
                .map_err(|_| Error::Storage)?;
            if identity(&meta) == identity(&expected) {
                Ok(())
            } else {
                Err(Error::UnsafeFile)
            }
        }
    }
}
fn unlink_owned(dir: &File, name: &CStr, file: &File) {
    if check_pin(dir, name, Some(file), MAX_JOURNAL_BYTES).is_ok() {
        // SAFETY: validated exact temporary inode in pinned directory.
        unsafe {
            libc::unlinkat(dir.as_raw_fd(), name.as_ptr(), 0);
        }
    }
}
fn admit(dir: &File, bytes: usize) -> Result<(), Error> {
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: valid pinned descriptor and appropriately sized output pointer.
    if unsafe { libc::fstatvfs(dir.as_raw_fd(), stat.as_mut_ptr()) } != 0 {
        return Err(Error::Measurement);
    }
    let stat = unsafe { stat.assume_init() };
    let available = u128::from(stat.f_bavail) * u128::from(stat.f_frsize);
    let required = u128::from(FREE_HEADROOM_BYTES) + bytes as u128 + 4096;
    if available < required {
        Err(Error::InsufficientSpace)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, DirBuilder};
    use std::os::unix::fs::DirBuilderExt;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = fs::canonicalize(std::env::temp_dir())
                .unwrap()
                .join(format!(
                    "be6500-capture-fault-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
            DirBuilder::new().mode(0o700).create(&path).unwrap();
            Self(path)
        }
        fn open(&self) -> Controller {
            Controller::open(&self.0).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn input() -> RulesPlanInput {
        RulesPlanInput {
            datapath: "routed-tun".into(),
            tun_interface: "b6p-tun0".into(),
            tun_address: "172.30.0.1/30".into(),
            client_ipv4: "192.168.31.10".into(),
            client_macs: Some([("192.168.31.10".into(), "02:aa:bb:cc:dd:ee".into())].into()),
            lan_interface: "br-lan".into(),
            ipv6: "direct".into(),
            ..RulesPlanInput::default()
        }
    }
    fn desired() -> Desired {
        Desired {
            desired: true,
            devices: vec![DeviceSelection {
                mac: "02:aa:bb:cc:dd:ee".into(),
            }],
            ..Desired::default()
        }
    }
    fn success(_: &[String], _: Instant) -> Result<CommandResult, CommandError> {
        Ok(CommandResult::success())
    }
    fn active(f: &Fixture) -> Controller {
        let mut c = f.open();
        c.set_desired(desired()).unwrap();
        c.apply(input(), |_, _, _| Ok(()), success).unwrap();
        c
    }
    #[test]
    fn desired_precommit_keeps_old_but_postrename_tracks_committed_authority() {
        let f = Fixture::new();
        let mut c = f.open();
        c.fault = Some(Fault::BeforeSave);
        assert_eq!(c.set_desired(desired()), Err(Error::Durability));
        assert!(!c.desired().desired);
        assert!(!f.0.join("capture-desired.json").exists());
        c.fault = Some(Fault::AfterRename);
        assert_eq!(c.set_desired(desired()), Err(Error::Durability));
        assert!(c.desired().desired);
        assert!(c.status().storage_uncertain);
        assert_eq!(c.desired(), f.open().desired());
        c.fault = None;
        c.set_desired(desired()).unwrap();
        assert!(!c.status().storage_uncertain);
    }
    #[test]
    fn uncertain_journal_save_retains_owned_cleanup_and_runs_zero_apply() {
        let f = Fixture::new();
        let mut c = f.open();
        c.set_desired(desired()).unwrap();
        c.fault = Some(Fault::AfterRename);
        assert_eq!(
            c.apply(
                input(),
                |_, _, _| Ok(()),
                |_, _| panic!("apply after uncertain persistence")
            ),
            Err(Error::Durability)
        );
        assert!(c.status().storage_uncertain);
        assert_eq!(c.status().phase, Phase::Staged);
        assert!(c.snapshot().ownership.is_some());
        assert!(f.0.join("capture-journal.json").exists());
        c.fault = None;
        c.cleanup(success).unwrap();
    }
    #[test]
    fn failed_disable_preserves_off_latch_with_best_effort_cleanup_before_and_after_rename() {
        for fault in [Fault::BeforeSave, Fault::AfterRename] {
            let f = Fixture::new();
            let mut c = active(&f);
            c.fault = Some(fault);
            let mut calls = 0;
            assert_eq!(
                c.disable(|_, _| {
                    calls += 1;
                    Ok(CommandResult::success())
                }),
                Err(Error::DisableFailed {
                    persistence_failed: true,
                    cleanup_failed: false
                })
            );
            assert_eq!(
                calls,
                capture_plan::plan_owned_rules(&input())
                    .unwrap()
                    .cleanup
                    .len()
            );
            assert!(!c.desired().desired);
            assert!(c.status().disable_not_persisted);
            assert!(!c.snapshot().desired.desired);
            assert!(!f.0.join("capture-journal.json").exists());
            assert_eq!(f.open().desired().desired, fault == Fault::BeforeSave);
            c.fault = None;
            c.disable(success).unwrap();
            assert!(!c.status().disable_not_persisted);
        }
    }
    #[test]
    fn journal_remove_failure_and_postremove_dirsync_uncertainty_retain_then_retry() {
        for fault in [Fault::BeforeRemove, Fault::AfterRemove] {
            let f = Fixture::new();
            let mut c = active(&f);
            let original = fs::read(f.0.join("capture-journal.json")).unwrap();
            c.fault = Some(fault);
            assert_eq!(c.cleanup(success), Err(Error::Durability));
            assert_eq!(c.status().phase, Phase::CleanupPending);
            assert!(c.status().storage_uncertain);
            assert!(c.snapshot().ownership.is_some());
            assert_eq!(
                fs::read(f.0.join("capture-journal.json")).unwrap(),
                original
            );
            assert_eq!(f.open().status().phase, Phase::Staged);
            c.fault = None;
            c.cleanup(success).unwrap();
            assert_eq!(c.status().phase, Phase::Off);
        }
    }
    #[test]
    fn expired_cleanup_counts_every_unattempted_command_without_false_off() {
        let f = Fixture::new();
        let mut c = active(&f);
        assert_eq!(
            c.cleanup_until(
                |_, _| panic!("expired cleanup must not call runner"),
                Instant::now()
            ),
            Err(Error::CleanupFailed)
        );
        assert_eq!(
            c.status().pending_cleanup_commands,
            capture_plan::plan_owned_rules(&input())
                .unwrap()
                .cleanup
                .len()
        );
        assert_eq!(c.status().phase, Phase::CleanupPending);
        assert!(f.0.join("capture-journal.json").exists());
    }
    #[test]
    fn deadline_is_checked_after_callback_and_expired_absence_is_not_proof() {
        let argv = capture_plan::plan_owned_rules(&input())
            .unwrap()
            .cleanup
            .pop()
            .unwrap();
        let deadline = Instant::now() + Duration::from_millis(1);
        let result = execute(
            &mut |_, passed| {
                // One deliberately uncooperative finite fake callback, no process.
                while Instant::now() < passed {
                    std::hint::spin_loop();
                }
                Ok(CommandResult {
                    success: false,
                    output: b"RTNETLINK answers: No such process".to_vec(),
                })
            },
            &argv,
            deadline,
            true,
        );
        assert_eq!(result, Err(CommandError::Timeout));
    }
    #[test]
    fn serialization_refuses_during_generation_before_admission_or_write() {
        let raw = "a".repeat(MAX_JOURNAL_BYTES + 1);
        assert_eq!(
            serialize_bounded(&raw, MAX_JOURNAL_BYTES),
            Err(Error::TooLarge)
        );
        assert_eq!(
            serialize_bounded(&raw, MAX_DESIRED_BYTES),
            Err(Error::TooLarge)
        );
        assert!(serialize_bounded(&"ok", MAX_DESIRED_BYTES).is_ok());
    }
    #[test]
    fn insufficient_space_arithmetic_does_not_consume_reserve() {
        let f = Fixture::new();
        let c = f.open();
        assert_eq!(
            admit(&c.directory.file, usize::MAX),
            Err(Error::InsufficientSpace)
        );
        assert_eq!(fs::read_dir(&f.0).unwrap().count(), 0);
    }
}

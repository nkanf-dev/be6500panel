//! Source-only ownership of the two fixed services. No readiness or capture is applied here.
//!
//! Each service has at most one owner in this process, with one 128 KiB supervisor
//! stack and a one-entry command channel. It owns at most one Run and one finite
//! Check child (four output pipe fds, 64 KiB combined retained tail at peak). Idle
//! owners block without periodic wakeups. The same OS thread creates and retains
//! every child until group termination and exact-child reaping finish. Linux uses
//! `waitid(WNOWAIT)` and creator-thread `PDEATHSIG`; macOS also retains a waitable
//! Child with WNOWAIT, but does not implement Linux parent-death semantics.
//!
//! Run children require explicit `stop_with_cleanup`, including after natural
//! exit. A failed cleanup leaves the owned handle intact. Dropping a live owner
//! does NOT kill, reap, or claim it stopped: its bounded service slot and supervisor
//! retain the child. This intentional fail-closed retention is not a substitute
//! for caller lifecycle handling. Always stop/finish and then close the owner.
//!
//! Roots and files must be private, same-user, non-symlink controlled paths. The
//! caller owns those roots and must not mutate files concurrently with launch.
//! Digests are rechecked at launch and after verification; these checks do not
//! establish protection against a malicious writer with the same Unix uid.
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::ffi::CString;
use std::fmt;
use std::fs::{File, Metadata};
use std::io::{self, Read};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::CommandExt;
use std::path::{Component, Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdout, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub const SUPERVISOR_STACK_BYTES: usize = 128 << 10;
pub const OUTPUT_TAIL_BYTES: usize = 32 << 10;
pub const MAX_ARTIFACT_BYTES: u64 = 40 << 20;
pub const MAX_CONFIG_BYTES: u64 = 4 << 20;
const TICK: Duration = Duration::from_millis(20);
const DRAIN_BYTES_PER_STREAM: usize = 64 << 10;
static SING_BOX_OWNED: AtomicBool = AtomicBool::new(false);
static FRPC_OWNED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceId {
    SingBox,
    Frpc,
}
impl ServiceId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SingBox => "sing-box",
            Self::Frpc => "frpc",
        }
    }
    fn slot(self) -> &'static AtomicBool {
        match self {
            Self::SingBox => &SING_BOX_OWNED,
            Self::Frpc => &FRPC_OWNED,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchMode {
    Run,
    Check,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Running,
    Exited,
    Stopped,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitReport {
    pub code: Option<i32>,
    pub signal: Option<i32>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    pub service: ServiceId,
    pub phase: Phase,
    /// Present only while this supervisor holds the exact, unreaped Child.
    /// This is observation, not authority to adopt or signal an arbitrary PID.
    pub pid: Option<u32>,
    pub mode: Option<LaunchMode>,
    pub exit: Option<ExitReport>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessError {
    InvalidInput,
    UntrustedPath,
    Integrity,
    Busy,
    Closed,
    Launch,
    Observation,
    CleanupFailed,
    StopDeadline,
    CheckFailed,
    CheckDeadline,
    OperationDeadline,
    Cancelled,
}
impl fmt::Display for ProcessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidInput => "invalid managed process input",
            Self::UntrustedPath => "managed process path is not trusted",
            Self::Integrity => "managed process file integrity check failed",
            Self::Busy => "managed service already has an owner or child",
            Self::Closed => "managed process owner is closed",
            Self::Launch => "cannot launch managed service",
            Self::Observation => "cannot observe owned child identity",
            Self::CleanupFailed => "managed service cleanup failed",
            Self::StopDeadline => "owned child termination deadline exceeded",
            Self::CheckFailed => "candidate verification failed",
            Self::CheckDeadline => "candidate verification deadline exceeded",
            Self::OperationDeadline => "managed process operation deadline exceeded",
            Self::Cancelled => "candidate verification cancelled",
        })
    }
}
impl std::error::Error for ProcessError {}

/// Internal immutable operation budget shared through the existing command lane.
/// Cleanup deliberately does not use it: an expired operation still owns children.
#[derive(Default)]
struct OperationBudget {
    deadline: Option<Instant>,
    cancel: Option<Arc<AtomicBool>>,
}
impl OperationBudget {
    fn check_cancelled(&self) -> Result<(), ProcessError> {
        if self.cancel.as_ref().is_some_and(|flag| flag.load(Ordering::Acquire)) {
            return Err(ProcessError::Cancelled);
        }
        Ok(())
    }
    fn check(&self) -> Result<(), ProcessError> {
        self.check_cancelled()?;
        if self.deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return Err(ProcessError::OperationDeadline);
        }
        Ok(())
    }
    fn io<T>(
        &self,
        action: impl FnOnce() -> Result<T, ProcessError>,
    ) -> Result<T, ProcessError> {
        self.check()?;
        let result = action();
        self.check()?;
        result
    }
}

/// Config root is physically separate from run/artifact roots. Artifact root may
/// equal run root or be its private child, matching the existing volatile layout.
#[derive(Clone)]
pub struct TrustedRoots {
    artifact: PathBuf,
    config: PathBuf,
    run: PathBuf,
}
impl TrustedRoots {
    pub fn new(
        artifact: impl Into<PathBuf>,
        config: impl Into<PathBuf>,
        run: impl Into<PathBuf>,
    ) -> Self {
        Self {
            artifact: artifact.into(),
            config: config.into(),
            run: run.into(),
        }
    }
}
impl fmt::Debug for TrustedRoots {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TrustedRoots([private])")
    }
}
#[derive(Clone)]
pub struct LaunchSpec {
    artifact: PathBuf,
    artifact_sha256: [u8; 32],
    config: PathBuf,
    config_sha256: [u8; 32],
    config_bytes: u64,
}
impl LaunchSpec {
    pub fn new(
        artifact: impl Into<PathBuf>,
        artifact_sha256: [u8; 32],
        config: impl Into<PathBuf>,
        config_sha256: [u8; 32],
        config_bytes: u64,
    ) -> Self {
        Self {
            artifact: artifact.into(),
            artifact_sha256,
            config: config.into(),
            config_sha256,
            config_bytes,
        }
    }
}
impl fmt::Debug for LaunchSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LaunchSpec([private])")
    }
}
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub term_grace: Duration,
    pub kill_grace: Duration,
    pub check_timeout: Duration,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            term_grace: Duration::from_millis(200),
            kill_grace: Duration::from_secs(1),
            check_timeout: Duration::from_secs(5),
        }
    }
}
impl Limits {
    fn validate(self) -> Result<(), ProcessError> {
        if self.term_grace.is_zero()
            || self.term_grace > Duration::from_secs(2)
            || self.kill_grace.is_zero()
            || self.kill_grace > Duration::from_secs(2)
            || self.check_timeout.is_zero()
            || self.check_timeout > Duration::from_secs(30)
        {
            return Err(ProcessError::InvalidInput);
        }
        Ok(())
    }
}
struct PinnedRoots {
    paths: TrustedRoots,
    artifact: File,
    config: File,
    run: File,
}
impl PinnedRoots {
    fn open(paths: TrustedRoots) -> Result<Self, ProcessError> {
        for (a, b) in [
            (&paths.artifact, &paths.config),
            (&paths.config, &paths.run),
        ] {
            if a.starts_with(b) || b.starts_with(a) {
                return Err(ProcessError::UntrustedPath);
            }
        }
        let artifact = private_directory(&paths.artifact)?;
        let config = private_directory(&paths.config)?;
        let run = private_directory(&paths.run)?;
        Ok(Self {
            paths,
            artifact,
            config,
            run,
        })
    }
    fn validate(
        &self,
        service: ServiceId,
        mode: LaunchMode,
        spec: &LaunchSpec,
        budget: &OperationBudget,
    ) -> Result<(), ProcessError> {
        budget.check()?;
        for (path, pinned) in [
            (&self.paths.artifact, &self.artifact),
            (&self.paths.config, &self.config),
            (&self.paths.run, &self.run),
        ] {
            let current = private_directory_until(path, budget)?;
            let current_metadata = budget.io(|| current.metadata().map_err(|_| ProcessError::UntrustedPath))?;
            let pinned_metadata = budget.io(|| pinned.metadata().map_err(|_| ProcessError::UntrustedPath))?;
            if identity(&current_metadata) != identity(&pinned_metadata)
            {
                return Err(ProcessError::UntrustedPath);
            }
        }
        if spec.artifact.parent() != Some(self.paths.artifact.as_path())
            || spec.config.parent() != Some(self.paths.config.as_path())
            || !controlled_config_name(service, mode, &spec.config)
            || spec.config_bytes == 0
            || spec.config_bytes > MAX_CONFIG_BYTES
        {
            return Err(ProcessError::UntrustedPath);
        }
        validate_file(
            &self.artifact,
            &spec.artifact,
            spec.artifact_sha256,
            None,
            (MAX_ARTIFACT_BYTES, true),
            budget,
        )?;
        validate_file(
            &self.config,
            &spec.config,
            spec.config_sha256,
            Some(spec.config_bytes),
            (MAX_CONFIG_BYTES, false),
            budget,
        )
    }
}
fn controlled_config_name(service: ServiceId, mode: LaunchMode, path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    let stem = match service {
        ServiceId::SingBox => name.strip_suffix(".json"),
        ServiceId::Frpc => name
            .strip_suffix(".toml")
            .or_else(|| name.strip_suffix(".json")),
    };
    let Some(stem) = stem else {
        return false;
    };
    if let Some(number) = stem.strip_prefix("config-") {
        return !number.is_empty()
            && !number.starts_with('0')
            && number.bytes().all(|b| b.is_ascii_digit())
            && number.parse::<u64>().is_ok();
    }
    mode == LaunchMode::Check
        && stem.strip_prefix(".candidate-").is_some_and(|hex| {
            hex.len() == 32
                && hex
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
}
fn identity(m: &Metadata) -> (u64, u64) {
    (m.dev(), m.ino())
}
fn private_directory(path: &Path) -> Result<File, ProcessError> {
    private_directory_until(path, &OperationBudget::default())
}
fn private_directory_until(path: &Path, budget: &OperationBudget) -> Result<File, ProcessError> {
    budget.check()?;
    if !path.is_absolute() {
        return Err(ProcessError::UntrustedPath);
    }
    let mut fd = unsafe {
        libc::open(
            c"/".as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        budget.check()?;
        return Err(ProcessError::UntrustedPath);
    }
    let mut directory = unsafe { OwnedFd::from_raw_fd(fd) };
    budget.check()?;
    let raw = path.as_os_str().as_bytes();
    if raw.split(|b| *b == b'/').any(|c| c == b"." || c == b"..") {
        return Err(ProcessError::UntrustedPath);
    }
    for component in path.components() {
        match component {
            Component::RootDir => continue,
            Component::Normal(name) => {
                budget.check()?;
                let name =
                    CString::new(name.as_bytes()).map_err(|_| ProcessError::UntrustedPath)?;
                fd = unsafe {
                    libc::openat(
                        directory.as_raw_fd(),
                        name.as_ptr(),
                        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                    )
                };
                if fd < 0 {
                    budget.check()?;
                    return Err(ProcessError::UntrustedPath);
                }
                directory = unsafe { OwnedFd::from_raw_fd(fd) };
                budget.check()?;
            }
            _ => return Err(ProcessError::UntrustedPath),
        }
    }
    let file = File::from(directory);
    let m = budget.io(|| file.metadata().map_err(|_| ProcessError::UntrustedPath))?;
    if !m.is_dir() || m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o7777 != 0o700 {
        return Err(ProcessError::UntrustedPath);
    }
    Ok(file)
}
fn validate_file(
    root: &File,
    path: &Path,
    digest: [u8; 32],
    length: Option<u64>,
    limits: (u64, bool),
    budget: &OperationBudget,
) -> Result<(), ProcessError> {
    let (max, executable) = limits;
    budget.check()?;
    let name = path.file_name().ok_or(ProcessError::UntrustedPath)?;
    let name = CString::new(name.as_bytes()).map_err(|_| ProcessError::UntrustedPath)?;
    let fd = unsafe {
        libc::openat(
            root.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
        )
    };
    if fd < 0 {
        budget.check()?;
        return Err(ProcessError::UntrustedPath);
    }
    let mut file = unsafe { File::from_raw_fd(fd) };
    budget.check()?;
    let before = budget.io(|| file.metadata().map_err(|_| ProcessError::UntrustedPath))?;
    let mode = if executable { 0o700 } else { 0o600 };
    if !before.is_file()
        || before.uid() != unsafe { libc::geteuid() }
        || before.mode() & 0o7777 != mode
        || before.nlink() != 1
        || before.len() == 0
        || before.len() > max
    {
        return Err(ProcessError::UntrustedPath);
    }
    if length.is_some_and(|n| n != before.len()) {
        return Err(ProcessError::Integrity);
    }
    let (actual_digest, total) = hash_reader(&mut file, max, || budget.check())?;
    let after = budget.io(|| file.metadata().map_err(|_| ProcessError::Integrity))?;
    if total != before.len()
        || identity(&before) != identity(&after)
        || before.len() != after.len()
        || before.mtime() != after.mtime()
        || before.mtime_nsec() != after.mtime_nsec()
        || before.ctime() != after.ctime()
        || before.ctime_nsec() != after.ctime_nsec()
        || actual_digest != digest
    {
        return Err(ProcessError::Integrity);
    }
    budget.check()
}

// Private Read seam permits deterministic slow-hash fixtures without a public
// fault hook or extra worker. Production checks use the operation's real clock.
fn hash_reader(
    reader: &mut impl Read,
    max: u64,
    mut check: impl FnMut() -> Result<(), ProcessError>,
) -> Result<([u8; 32], u64), ProcessError> {
    let mut hash = Sha256::new();
    let mut bytes = [0u8; 8192];
    let mut total = 0u64;
    loop {
        check()?;
        let size = max.saturating_sub(total).max(1).min(bytes.len() as u64) as usize;
        let result = reader.read(&mut bytes[..size]);
        check()?;
        let n = match result {
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            result => result.map_err(|_| ProcessError::Integrity)?,
        };
        if n == 0 { break; }
        total = total.checked_add(n as u64).ok_or(ProcessError::Integrity)?;
        if total > max { return Err(ProcessError::Integrity); }
        hash.update(&bytes[..n]);
    }
    check()?;
    let digest = hash.finalize().into();
    check()?;
    Ok((digest, total))
}

#[must_use = "explicitly stop/finish the owned child and close this service owner"]
pub struct ProcessOwner {
    service: ServiceId,
    sender: Option<SyncSender<Request>>,
    thread: Option<JoinHandle<()>>,
}
impl fmt::Debug for ProcessOwner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProcessOwner")
            .field("service", &self.service)
            .finish_non_exhaustive()
    }
}
impl ProcessOwner {
    /// Pins trusted roots and starts the fixed supervisor, but does not launch a child.
    pub fn new(
        service: ServiceId,
        roots: TrustedRoots,
        limits: Limits,
    ) -> Result<Self, ProcessError> {
        limits.validate()?;
        let roots = PinnedRoots::open(roots)?;
        if service
            .slot()
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(ProcessError::Busy);
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        let thread = match thread::Builder::new()
            .name(format!("{}-owner", service.as_str()))
            .stack_size(SUPERVISOR_STACK_BYTES)
            .spawn(move || {
                let _slot = Slot(service);
                Supervisor::new(service, roots, limits).run(receiver);
            }) {
            Ok(thread) => thread,
            Err(_) => {
                service.slot().store(false, Ordering::Release);
                return Err(ProcessError::Launch);
            }
        };
        Ok(Self {
            service,
            sender: Some(sender),
            thread: Some(thread),
        })
    }
    pub fn start(&mut self, spec: LaunchSpec) -> Result<Status, ProcessError> {
        self.start_budgeted(spec, OperationBudget::default())
    }
    /// Validate and spawn only within this absolute operation budget. If the
    /// budget expires after spawn, the exact Run remains owned for withdrawal.
    pub fn start_until(
        &mut self,
        spec: LaunchSpec,
        deadline: Instant,
        cancel: Option<Arc<AtomicBool>>,
    ) -> Result<Status, ProcessError> {
        self.start_budgeted(spec, OperationBudget { deadline: Some(deadline), cancel })
    }
    fn start_budgeted(&mut self, spec: LaunchSpec, budget: OperationBudget) -> Result<Status, ProcessError> {
        budget.check()?;
        self.request(Action::Start(spec, budget))
    }
    pub fn status(&mut self) -> Result<Status, ProcessError> {
        self.request(Action::Status)
    }
    /// Internal readiness sampling of this retained owner. It grants no PID
    /// adoption or mutation and uses the existing supervisor/channel only.
    pub(crate) fn observe_retained(&self) -> Result<Status, ProcessError> {
        self.request(Action::Status)
    }
    /// Exit is observed without reaping. Caller cleanup is still required.
    pub fn poll_exited(&mut self) -> Result<Option<ExitReport>, ProcessError> {
        Ok(self.status()?.exit)
    }
    /// A failed callback sends no termination command. The same handle can be retried.
    /// Repeated stop after successful reaping is a no-op and does not call cleanup again.
    pub fn stop_with_cleanup<E>(
        &mut self,
        cleanup: impl FnOnce() -> Result<(), E>,
    ) -> Result<Status, ProcessError> {
        let status = self.status()?;
        if status.pid.is_none() {
            return Ok(status);
        }
        cleanup().map_err(|_| ProcessError::CleanupFailed)?;
        self.request(Action::Stop)
    }
    /// Check owns no network resources. It finishes descendants and reaps the leader
    /// before returning success/failure. Cancellation is a narrow optional flag.
    /// A termination deadline retains the Check child; retry with `abort_check`.
    pub fn verify(
        &mut self,
        spec: LaunchSpec,
        cancel: Option<Arc<AtomicBool>>,
    ) -> Result<(), ProcessError> {
        self.verify_budgeted(spec, OperationBudget { deadline: None, cancel })
    }
    /// Hashing before launch, finite check and post-check integrity all share
    /// the same absolute deadline. Expiry never skips child finish/reaping.
    pub fn verify_until(
        &mut self,
        spec: LaunchSpec,
        deadline: Instant,
        cancel: Option<Arc<AtomicBool>>,
    ) -> Result<(), ProcessError> {
        self.verify_budgeted(spec, OperationBudget { deadline: Some(deadline), cancel })
    }
    fn verify_budgeted(&mut self, spec: LaunchSpec, budget: OperationBudget) -> Result<(), ProcessError> {
        budget.check()?;
        self.request(Action::Verify(spec, budget)).map(|_| ())
    }
    pub fn abort_check(&mut self) -> Result<Status, ProcessError> {
        self.request(Action::AbortCheck)
    }
    /// Refuses a live or unreaped child. No Drop path substitutes for explicit cleanup.
    pub fn close(&mut self) -> Result<(), ProcessError> {
        if self.sender.is_none() {
            return Ok(());
        }
        self.request(Action::Close)?;
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            thread.join().map_err(|_| ProcessError::Closed)?;
        }
        Ok(())
    }
    fn request(&self, action: Action) -> Result<Status, ProcessError> {
        let (reply, result) = mpsc::sync_channel(1);
        self.sender
            .as_ref()
            .ok_or(ProcessError::Closed)?
            .send(Request { action, reply })
            .map_err(|_| ProcessError::Closed)?;
        result.recv().map_err(|_| ProcessError::Closed)?
    }
}
impl Drop for ProcessOwner {
    fn drop(&mut self) {
        // Disconnect is NOT a stop command. A live child keeps its creator thread
        // and fixed slot. Idle supervisors exit and release their slot normally.
        self.sender.take();
        self.thread.take();
    }
}
struct Slot(ServiceId);
impl Drop for Slot {
    fn drop(&mut self) {
        self.0.slot().store(false, Ordering::Release);
    }
}
enum Action {
    Start(LaunchSpec, OperationBudget),
    Status,
    Stop,
    Verify(LaunchSpec, OperationBudget),
    AbortCheck,
    Close,
}
struct Request {
    action: Action,
    reply: SyncSender<Result<Status, ProcessError>>,
}
struct Supervisor {
    service: ServiceId,
    roots: PinnedRoots,
    limits: Limits,
    child: Option<OwnedChild>,
    check: Option<OwnedChild>,
    last_exit: Option<ExitReport>,
    stopped: bool,
}
impl Supervisor {
    fn new(service: ServiceId, roots: PinnedRoots, limits: Limits) -> Self {
        Self {
            service,
            roots,
            limits,
            child: None,
            check: None,
            last_exit: None,
            stopped: false,
        }
    }
    fn run(mut self, receiver: Receiver<Request>) {
        let mut connected = true;
        loop {
            let observed = self.pump();
            if !connected {
                if self.child.is_none() && self.check.is_none() {
                    return;
                }
                // Retain the exact handle and stable creator thread, without spinning.
                thread::park_timeout(TICK);
                continue;
            }
            // Idle owners do not wake at 50 Hz. Only an owned Child needs the
            // bounded observation/drain deadline.
            let request = if self.child.is_some() || self.check.is_some() {
                receiver.recv_timeout(TICK)
            } else {
                receiver.recv().map_err(|_| RecvTimeoutError::Disconnected)
            };
            match request {
                Ok(request) => {
                    let closing = matches!(request.action, Action::Close)
                        && self.child.is_none()
                        && self.check.is_none();
                    let result = match request.action {
                        Action::Status => observed.map(|_| self.status()),
                        Action::Start(spec, budget) => {
                            self.launch(spec, LaunchMode::Run, &budget).map(|_| self.status())
                        }
                        Action::Stop => self.stop().map(|_| self.status()),
                        Action::Verify(spec, budget) => {
                            self.verify(spec, &budget).map(|_| self.status())
                        }
                        Action::AbortCheck => self.finish(LaunchMode::Check).map(|_| self.status()),
                        Action::Close => {
                            if self.child.is_some() || self.check.is_some() {
                                Err(ProcessError::Busy)
                            } else {
                                Ok(self.status())
                            }
                        }
                    };
                    let _ = request.reply.send(result);
                    if closing {
                        return;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => connected = false,
            }
        }
    }
    fn status(&self) -> Status {
        match self.child.as_ref().or(self.check.as_ref()) {
            Some(c) => Status {
                service: self.service,
                phase: if c.exit.is_some() {
                    Phase::Exited
                } else {
                    Phase::Running
                },
                pid: Some(c.child.id()),
                mode: Some(c.mode),
                exit: c.exit,
            },
            None => Status {
                service: self.service,
                phase: if self.stopped {
                    Phase::Stopped
                } else {
                    Phase::Idle
                },
                pid: None,
                mode: None,
                exit: self.last_exit,
            },
        }
    }
    fn pump(&mut self) -> Result<(), ProcessError> {
        if let Some(child) = &mut self.child {
            child.pump()?;
        }
        if let Some(check) = &mut self.check {
            check.pump()?;
        }
        Ok(())
    }
    fn launch(
        &mut self,
        spec: LaunchSpec,
        mode: LaunchMode,
        budget: &OperationBudget,
    ) -> Result<(), ProcessError> {
        budget.check()?;
        if self.check.is_some() || (mode == LaunchMode::Run && self.child.is_some()) {
            return Err(ProcessError::Busy);
        }
        self.roots.validate(self.service, mode, &spec, budget)?;
        let mut command = Command::new(&spec.artifact);
        match (self.service, mode) {
            (ServiceId::SingBox, LaunchMode::Run) => {
                command.arg("run");
            }
            (ServiceId::SingBox, LaunchMode::Check) => {
                command.arg("check");
            }
            (ServiceId::Frpc, LaunchMode::Run) => {}
            (ServiceId::Frpc, LaunchMode::Check) => {
                command.arg("verify");
            }
        }
        command
            .arg("-c")
            .arg(&spec.config)
            .env_clear()
            .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
            .env("HOME", &self.roots.paths.run)
            .env("TMPDIR", &self.roots.paths.run)
            .current_dir(&self.roots.paths.run)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Everything captured here is prepared before fork. Child closure uses
        // only async-signal-safe libc syscalls; failure exits without formatting,
        // allocation, Rust locks, or invoking child-side destructors.
        #[cfg(target_os = "linux")]
        let expected_parent = unsafe { libc::getpid() };
        unsafe {
            command.pre_exec(move || {
                if libc::setpgid(0, 0) != 0 {
                    libc::_exit(127);
                }
                #[cfg(target_os = "linux")]
                {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL, 0, 0, 0) != 0
                        || libc::getppid() != expected_parent
                    {
                        libc::_exit(127);
                    }
                }
                Ok(())
            });
        }
        budget.check()?;
        let result = command.spawn();
        let child = match result {
            Ok(child) => child,
            Err(_) => { budget.check()?; return Err(ProcessError::Launch); }
        };
        match mode {
            LaunchMode::Run => {
                self.child = Some(OwnedChild::new(child, mode));
                self.stopped = false;
                self.last_exit = None;
            }
            LaunchMode::Check => self.check = Some(OwnedChild::new(child, mode)),
        }
        // Pipe setup is part of ownership safety: cleanup must not encounter
        // blocking pipes even if the budget expired during spawn. Keep the exact
        // child before either error; Verify finishes it, Run needs withdrawal.
        let result = self.owned_mut(mode)
            .ok_or(ProcessError::Launch)?
            .nonblocking();
        budget.check()?;
        result
    }
    fn owned_mut(&mut self, mode: LaunchMode) -> Option<&mut OwnedChild> {
        match mode {
            LaunchMode::Run => self.child.as_mut(),
            LaunchMode::Check => self.check.as_mut(),
        }
    }
    fn stop(&mut self) -> Result<(), ProcessError> {
        self.finish(LaunchMode::Run)
    }
    fn finish(&mut self, mode: LaunchMode) -> Result<(), ProcessError> {
        let Some(child) = self.owned_mut(mode) else {
            return Ok(());
        };
        child.signal(libc::SIGTERM)?;
        self.wait_exit(mode, Instant::now() + self.limits.term_grace)?;
        // Leader stays unreaped until this group signal has killed descendants.
        self.owned_mut(mode)
            .ok_or(ProcessError::Observation)?
            .signal(libc::SIGKILL)?;
        self.wait_exit(mode, Instant::now() + self.limits.kill_grace)?;
        let child = self.owned_mut(mode).ok_or(ProcessError::Observation)?;
        if child.exit.is_none() {
            return Err(ProcessError::StopDeadline);
        }
        child.pump()?;
        let status = child.child.wait().map_err(|_| ProcessError::Observation)?;
        use std::os::unix::process::ExitStatusExt;
        let report = ExitReport {
            code: status.code(),
            signal: status.signal(),
        };
        match mode {
            LaunchMode::Run => {
                self.child = None;
                self.last_exit = Some(report);
                self.stopped = true;
            }
            LaunchMode::Check => {
                self.check = None;
                if self.child.is_none() {
                    self.last_exit = Some(report);
                    self.stopped = true;
                }
            }
        }
        Ok(())
    }
    fn wait_exit(&mut self, mode: LaunchMode, deadline: Instant) -> Result<(), ProcessError> {
        loop {
            self.pump()?; // Also drains the Run child during a finite Check.
            let child = self.owned_mut(mode).ok_or(ProcessError::Observation)?;
            if child.exit.is_some() || Instant::now() >= deadline {
                return Ok(());
            }
            child.wait_tick(deadline)?;
        }
    }
    fn verify(
        &mut self,
        spec: LaunchSpec,
        budget: &OperationBudget,
    ) -> Result<(), ProcessError> {
        budget.check()?;
        let had_check = self.check.is_some();
        if let Err(error) = self.launch(spec.clone(), LaunchMode::Check, budget) {
            // Launch may have created a Check before a post-spawn failure.
            // Independent finish budgets remain valid after operation expiry.
            if !had_check { self.finish(LaunchMode::Check)?; }
            return Err(error);
        }
        let local_deadline = Instant::now() + self.limits.check_timeout;
        let deadline = budget.deadline.map_or(local_deadline, |outer| outer.min(local_deadline));
        let check_execution = || {
            budget.check_cancelled()?;
            if Instant::now() >= deadline {
                return Err(if budget.deadline.is_some_and(|outer| outer <= local_deadline) {
                    ProcessError::OperationDeadline
                } else {
                    ProcessError::CheckDeadline
                });
            }
            Ok(())
        };
        let outcome = loop {
            if let Err(error) = check_execution() { break Err(error); }
            let pumped = self.pump();
            if let Err(error) = check_execution() { break Err(error); }
            if let Err(error) = pumped { break Err(error); }
            if let Some(exit) = self.check.as_ref().and_then(|c| c.exit) {
                break if exit.code == Some(0) { Ok(()) } else { Err(ProcessError::CheckFailed) };
            }
            if let Some(child) = &mut self.check
                && let Err(error) = child.wait_tick(deadline)
            {
                break Err(error);
            }
        };
        self.finish(LaunchMode::Check)?;
        outcome?;
        // Cleanup may itself finish after expiry; never attest success then.
        budget.check()?;
        self.roots.validate(self.service, LaunchMode::Check, &spec, budget)
    }
}
struct OwnedChild {
    child: Child,
    mode: LaunchMode,
    stdout: Option<ChildStdout>,
    stderr: Option<ChildStderr>,
    tail: VecDeque<u8>,
    exit: Option<ExitReport>,
}
impl OwnedChild {
    fn new(mut child: Child, mode: LaunchMode) -> Self {
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        Self {
            child,
            mode,
            stdout,
            stderr,
            tail: VecDeque::with_capacity(OUTPUT_TAIL_BYTES),
            exit: None,
        }
    }
    fn nonblocking(&mut self) -> Result<(), ProcessError> {
        for fd in self.fds().into_iter().filter(|fd| *fd >= 0) {
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
            {
                return Err(ProcessError::Observation);
            }
        }
        Ok(())
    }
    fn fds(&self) -> [RawFd; 2] {
        [
            self.stdout.as_ref().map_or(-1, AsRawFd::as_raw_fd),
            self.stderr.as_ref().map_or(-1, AsRawFd::as_raw_fd),
        ]
    }
    fn pump(&mut self) -> Result<(), ProcessError> {
        if let Some(stdout) = &mut self.stdout
            && drain(stdout, &mut self.tail)?
        {
            self.stdout = None;
        }
        if let Some(stderr) = &mut self.stderr
            && drain(stderr, &mut self.tail)?
        {
            self.stderr = None;
        }
        if self.exit.is_none() {
            self.exit = observe(self.child.id())?;
        }
        Ok(())
    }
    fn wait_tick(&mut self, deadline: Instant) -> Result<(), ProcessError> {
        let millis = deadline
            .saturating_duration_since(Instant::now())
            .min(TICK)
            .as_millis() as i32;
        let mut fds = self.fds().map(|fd| libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        });
        if unsafe { libc::poll(fds.as_mut_ptr(), 2, millis.max(1)) } < 0
            && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted
        {
            return Err(ProcessError::Observation);
        }
        self.pump()
    }
    fn signal(&self, signal: i32) -> Result<(), ProcessError> {
        // No arbitrary PID entry point. This Child has never been waited/reaped.
        if unsafe { libc::kill(-(self.child.id() as libc::pid_t), signal) } != 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) {
                // Darwin returns EPERM for an owned group containing only its
                // retained zombie leader. This exception is host-only and only
                // after waitid proved that the exact leader has exited. Live
                // descendants under the trusted fixture uid are still signalled.
                #[cfg(target_os = "macos")]
                if self.exit.is_some() && error.raw_os_error() == Some(libc::EPERM) {
                    return Ok(());
                }
                return Err(ProcessError::Observation);
            }
        }
        Ok(())
    }
}
fn drain(reader: &mut impl Read, tail: &mut VecDeque<u8>) -> Result<bool, ProcessError> {
    let mut buffer = [0u8; 4096];
    let mut drained = 0;
    while drained < DRAIN_BYTES_PER_STREAM {
        match reader.read(&mut buffer) {
            Ok(0) => return Ok(true),
            Ok(n) => {
                push_tail(tail, &buffer[..n]);
                drained += n;
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(false),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(ProcessError::Observation),
        }
    }
    Ok(false)
}
fn push_tail(tail: &mut VecDeque<u8>, bytes: &[u8]) {
    let bytes = if bytes.len() > OUTPUT_TAIL_BYTES {
        &bytes[bytes.len() - OUTPUT_TAIL_BYTES..]
    } else {
        bytes
    };
    let discard = (tail.len() + bytes.len()).saturating_sub(OUTPUT_TAIL_BYTES);
    tail.drain(..discard);
    tail.extend(bytes);
}
fn observe(pid: u32) -> Result<Option<ExitReport>, ProcessError> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        loop {
            if unsafe {
                libc::waitid(
                    libc::P_PID,
                    pid as libc::id_t,
                    &mut info,
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            } == 0
            {
                break;
            }
            if io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
                return Err(ProcessError::Observation);
            }
        }
        #[cfg(target_os = "linux")]
        let (child_pid, status) = unsafe { (info.si_pid(), info.si_status()) };
        #[cfg(target_os = "macos")]
        let (child_pid, status) = (info.si_pid, info.si_status);
        if child_pid == 0 {
            return Ok(None);
        }
        if child_pid != pid as libc::pid_t {
            return Err(ProcessError::Observation);
        }
        Ok(Some(if info.si_code == libc::CLD_EXITED {
            ExitReport {
                code: Some(status),
                signal: None,
            }
        } else {
            ExitReport {
                code: None,
                signal: Some(status),
            }
        }))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = pid;
        Err(ProcessError::Observation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hash_callback_expiry_and_cancel_are_checked_before_and_after_reads() {
        use std::cell::Cell;
        struct Advances<'a> { elapsed: &'a Cell<bool>, reads: &'a Cell<usize>, fail: bool }
        impl Read for Advances<'_> {
            fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
                assert!(bytes.len() <= 8192);
                self.reads.set(self.reads.get() + 1);
                self.elapsed.set(true);
                if self.fail { return Err(io::Error::other("private source error")); }
                bytes[0] = 7;
                Ok(1)
            }
        }
        for fail in [false, true] {
            let elapsed = Cell::new(false);
            let reads = Cell::new(0);
            let mut reader = Advances { elapsed: &elapsed, reads: &reads, fail };
            let result = hash_reader(&mut reader, 8192, || {
                if elapsed.get() { Err(ProcessError::OperationDeadline) } else { Ok(()) }
            });
            assert_eq!(result, Err(ProcessError::OperationDeadline));
            assert_eq!(reads.get(), 1);
        }
        let elapsed = Cell::new(false);
        let reads = Cell::new(0);
        let mut reader = Advances { elapsed: &elapsed, reads: &reads, fail: false };
        assert_eq!(hash_reader(&mut reader, 8192, || Err(ProcessError::Cancelled)), Err(ProcessError::Cancelled));
        assert_eq!(reads.get(), 0);
    }

    #[test]
    fn cancelling_hash_reader_after_io_stops_before_another_read() {
        struct Cancels { flag: Arc<AtomicBool>, reads: usize }
        impl Read for Cancels {
            fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
                self.reads += 1;
                self.flag.store(true, Ordering::Release);
                bytes[0] = 7;
                Ok(1)
            }
        }
        let flag = Arc::new(AtomicBool::new(false));
        let budget = OperationBudget {
            deadline: Some(Instant::now() + Duration::from_secs(10)),
            cancel: Some(flag.clone()),
        };
        let mut reader = Cancels { flag, reads: 0 };
        assert_eq!(hash_reader(&mut reader, 8192, || budget.check()), Err(ProcessError::Cancelled));
        assert_eq!(reader.reads, 1);
    }

    #[test]
    fn hash_boundaries_and_real_budget_errors_stay_fixed() {
        let raw = [7u8; 8192];
        assert_eq!(hash_reader(&mut raw.as_slice(), 8192, || Ok(())),
            Ok((Sha256::digest(raw).into(), 8192)));
        assert_eq!(hash_reader(&mut raw.as_slice(), 8191, || Ok(())), Err(ProcessError::Integrity));
        let budget = OperationBudget { deadline: Some(Instant::now()), cancel: None };
        assert_eq!(budget.check(), Err(ProcessError::OperationDeadline));
        let cancel = Arc::new(AtomicBool::new(true));
        let budget = OperationBudget { deadline: Some(Instant::now()), cancel: Some(cancel) };
        assert_eq!(budget.check(), Err(ProcessError::Cancelled));
        assert_eq!(ProcessError::OperationDeadline.to_string(), "managed process operation deadline exceeded");
    }

    #[test]
    fn combined_tail_is_bounded_suffix_and_private() {
        let mut tail = VecDeque::with_capacity(OUTPUT_TAIL_BYTES);
        push_tail(&mut tail, &vec![b'a'; OUTPUT_TAIL_BYTES + 500]);
        push_tail(&mut tail, b"private-checker-output");
        assert_eq!(tail.len(), OUTPUT_TAIL_BYTES);
        assert!(tail.make_contiguous().ends_with(b"private-checker-output"));
        assert_eq!(tail.capacity(), OUTPUT_TAIL_BYTES);
        assert!(!format!("{:?}", ProcessError::CheckFailed).contains("private-checker-output"));
    }
    #[test]
    fn controlled_config_names_are_service_specific() {
        assert!(controlled_config_name(
            ServiceId::SingBox,
            LaunchMode::Run,
            Path::new("config-1.json")
        ));
        assert!(controlled_config_name(
            ServiceId::Frpc,
            LaunchMode::Run,
            Path::new("config-18446744073709551615.toml")
        ));
        for name in [
            "config-0.json",
            "config-01.json",
            "arbitrary.json",
            "config-1.toml",
            "config-18446744073709551616.json",
        ] {
            assert!(!controlled_config_name(
                ServiceId::SingBox,
                LaunchMode::Run,
                Path::new(name)
            ));
        }
    }
}

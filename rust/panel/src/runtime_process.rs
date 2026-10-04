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
            Self::Cancelled => "candidate verification cancelled",
        })
    }
}
impl std::error::Error for ProcessError {}

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
    ) -> Result<(), ProcessError> {
        for (path, pinned) in [
            (&self.paths.artifact, &self.artifact),
            (&self.paths.config, &self.config),
            (&self.paths.run, &self.run),
        ] {
            let current = private_directory(path)?;
            if identity(
                &current
                    .metadata()
                    .map_err(|_| ProcessError::UntrustedPath)?,
            ) != identity(&pinned.metadata().map_err(|_| ProcessError::UntrustedPath)?)
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
            MAX_ARTIFACT_BYTES,
            true,
        )?;
        validate_file(
            &self.config,
            &spec.config,
            spec.config_sha256,
            Some(spec.config_bytes),
            MAX_CONFIG_BYTES,
            false,
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
        return Err(ProcessError::UntrustedPath);
    }
    let mut directory = unsafe { OwnedFd::from_raw_fd(fd) };
    let raw = path.as_os_str().as_bytes();
    if raw.split(|b| *b == b'/').any(|c| c == b"." || c == b"..") {
        return Err(ProcessError::UntrustedPath);
    }
    for component in path.components() {
        match component {
            Component::RootDir => continue,
            Component::Normal(name) => {
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
                    return Err(ProcessError::UntrustedPath);
                }
                directory = unsafe { OwnedFd::from_raw_fd(fd) };
            }
            _ => return Err(ProcessError::UntrustedPath),
        }
    }
    let file = File::from(directory);
    let m = file.metadata().map_err(|_| ProcessError::UntrustedPath)?;
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
    max: u64,
    executable: bool,
) -> Result<(), ProcessError> {
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
        return Err(ProcessError::UntrustedPath);
    }
    let mut file = unsafe { File::from_raw_fd(fd) };
    let before = file.metadata().map_err(|_| ProcessError::UntrustedPath)?;
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
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 8192];
    let mut total = 0u64;
    loop {
        let n = file
            .read(&mut buffer)
            .map_err(|_| ProcessError::Integrity)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > max {
            return Err(ProcessError::Integrity);
        }
        hash.update(&buffer[..n]);
    }
    let after = file.metadata().map_err(|_| ProcessError::Integrity)?;
    if total != before.len()
        || identity(&before) != identity(&after)
        || before.len() != after.len()
        || before.mtime() != after.mtime()
        || before.mtime_nsec() != after.mtime_nsec()
        || before.ctime() != after.ctime()
        || before.ctime_nsec() != after.ctime_nsec()
        || <[u8; 32]>::from(hash.finalize()) != digest
    {
        return Err(ProcessError::Integrity);
    }
    Ok(())
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
        self.request(Action::Start(spec))
    }
    pub fn status(&mut self) -> Result<Status, ProcessError> {
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
        self.request(Action::Verify(spec, cancel)).map(|_| ())
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
    fn request(&mut self, action: Action) -> Result<Status, ProcessError> {
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
    Start(LaunchSpec),
    Status,
    Stop,
    Verify(LaunchSpec, Option<Arc<AtomicBool>>),
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
                        Action::Start(spec) => {
                            self.launch(spec, LaunchMode::Run).map(|_| self.status())
                        }
                        Action::Stop => self.stop().map(|_| self.status()),
                        Action::Verify(spec, cancel) => {
                            self.verify(spec, cancel).map(|_| self.status())
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
    fn launch(&mut self, spec: LaunchSpec, mode: LaunchMode) -> Result<(), ProcessError> {
        if self.check.is_some() || (mode == LaunchMode::Run && self.child.is_some()) {
            return Err(ProcessError::Busy);
        }
        self.roots.validate(self.service, mode, &spec)?;
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
        let child = command.spawn().map_err(|_| ProcessError::Launch)?;
        match mode {
            LaunchMode::Run => {
                self.child = Some(OwnedChild::new(child, mode));
                self.stopped = false;
                self.last_exit = None;
            }
            LaunchMode::Check => self.check = Some(OwnedChild::new(child, mode)),
        }
        // Even a pipe-setup failure retains identity for explicit stop/retry.
        self.owned_mut(mode)
            .ok_or(ProcessError::Launch)?
            .nonblocking()
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
        cancel: Option<Arc<AtomicBool>>,
    ) -> Result<(), ProcessError> {
        if cancel
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Acquire))
        {
            return Err(ProcessError::Cancelled);
        }
        self.launch(spec.clone(), LaunchMode::Check)?;
        let deadline = Instant::now() + self.limits.check_timeout;
        let outcome = loop {
            if let Err(error) = self.pump() {
                break Err(error);
            }
            if let Some(exit) = self.check.as_ref().and_then(|c| c.exit) {
                break if exit.code == Some(0) {
                    Ok(())
                } else {
                    Err(ProcessError::CheckFailed)
                };
            }
            if cancel
                .as_ref()
                .is_some_and(|flag| flag.load(Ordering::Acquire))
            {
                break Err(ProcessError::Cancelled);
            }
            if Instant::now() >= deadline {
                break Err(ProcessError::CheckDeadline);
            }
            if let Some(child) = &mut self.check
                && let Err(error) = child.wait_tick(deadline)
            {
                break Err(error);
            }
        };
        self.finish(LaunchMode::Check)?;
        outcome?;
        // Candidate bytes and executable must still match after the checker.
        self.roots.validate(self.service, LaunchMode::Check, &spec)
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

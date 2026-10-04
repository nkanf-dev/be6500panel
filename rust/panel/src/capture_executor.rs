//! Execute only exact internally regenerated routed-TUN argv. No shell, public
//! command endpoint, background worker or command execution during admission.
use crate::capture_plan::{RulesPlanInput, plan_owned_rules};
use crate::capture_state::{CommandError, CommandResult, MAX_OUTPUT_BYTES};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Read},
    os::{
        fd::AsRawFd,
        unix::{
            fs::{MetadataExt, OpenOptionsExt},
            process::CommandExt,
        },
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
const MAX_EXECUTABLE_BYTES: u64 = 40 << 20;
const DRAIN_PER_TICK: usize = 64 << 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdmissionError {
    Plan,
    Binary,
    Directory,
}
impl fmt::Display for AdmissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("restricted capture executor admission failed")
    }
}
impl std::error::Error for AdmissionError {}
#[derive(Clone, Copy, PartialEq, Eq)]
struct Identity {
    dev: u64,
    ino: u64,
    len: u64,
    mode: u32,
    uid: u32,
    links: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}
fn identity(metadata: &fs::Metadata) -> Identity {
    Identity {
        dev: metadata.dev(),
        ino: metadata.ino(),
        len: metadata.len(),
        mode: metadata.mode(),
        uid: metadata.uid(),
        links: metadata.nlink(),
        modified: (metadata.mtime(), metadata.mtime_nsec()),
        changed: (metadata.ctime(), metadata.ctime_nsec()),
    }
}
/// Internal trusted binding, never accepted from an HTTP body. The checksum is
/// checked once at admission; unchanged inode metadata is checked per command.
pub struct TrustedBinary {
    path: PathBuf,
    file: File,
    identity: Identity,
}
impl fmt::Debug for TrustedBinary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TrustedBinary([private])")
    }
}
impl TrustedBinary {
    pub fn admit(path: &Path, expected_sha256: [u8; 32]) -> Result<Self, AdmissionError> {
        if !path.is_absolute() {
            return Err(AdmissionError::Binary);
        }
        let path = path.canonicalize().map_err(|_| AdmissionError::Binary)?;
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(&path)
            .map_err(|_| AdmissionError::Binary)?;
        let before = file.metadata().map_err(|_| AdmissionError::Binary)?;
        if !before.is_file()
            || before.mode() & 0o111 == 0
            || before.mode() & 0o022 != 0
            || before.nlink() != 1
            || before.len() == 0
            || before.len() > MAX_EXECUTABLE_BYTES
            || !(before.uid() == 0 || before.uid() == unsafe { libc::geteuid() })
        {
            return Err(AdmissionError::Binary);
        }
        let mut hash = Sha256::new();
        let mut total = 0u64;
        let mut scratch = [0u8; 8192];
        loop {
            let n = file
                .read(&mut scratch)
                .map_err(|_| AdmissionError::Binary)?;
            if n == 0 {
                break;
            }
            total = total.checked_add(n as u64).ok_or(AdmissionError::Binary)?;
            if total > MAX_EXECUTABLE_BYTES {
                return Err(AdmissionError::Binary);
            }
            hash.update(&scratch[..n]);
        }
        if total != before.len()
            || identity(&file.metadata().map_err(|_| AdmissionError::Binary)?) != identity(&before)
            || <[u8; 32]>::from(hash.finalize()) != expected_sha256
        {
            return Err(AdmissionError::Binary);
        }
        Ok(Self {
            path,
            file,
            identity: identity(&before),
        })
    }
    fn unchanged(&self) -> bool {
        self.file
            .metadata()
            .ok()
            .is_some_and(|m| identity(&m) == self.identity)
            && fs::symlink_metadata(&self.path)
                .ok()
                .is_some_and(|m| identity(&m) == self.identity)
    }
}
pub struct Binaries {
    pub ip: TrustedBinary,
    pub iptables: TrustedBinary,
}
impl fmt::Debug for Binaries {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CaptureBinaries([private])")
    }
}
/// One synchronous executor, with an exceptional retained child only if cleanup
/// cannot reap by its bounded deadline. No next command starts until retry_abort.
pub struct Executor {
    binaries: Binaries,
    allowed: HashSet<Vec<String>>,
    run_dir: PathBuf,
    run_identity: Identity,
    pending: Option<Child>,
}
impl fmt::Debug for Executor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CaptureExecutor([private])")
    }
}
impl Executor {
    pub fn admit(
        input: &RulesPlanInput,
        binaries: Binaries,
        run_dir: &Path,
    ) -> Result<Self, AdmissionError> {
        let plan = plan_owned_rules(input).map_err(|_| AdmissionError::Plan)?;
        let metadata = fs::symlink_metadata(run_dir).map_err(|_| AdmissionError::Directory)?;
        if !run_dir.is_absolute()
            || metadata.file_type().is_symlink()
            || !metadata.is_dir()
            || metadata.mode() & 0o7777 != 0o700
            || metadata.uid() != unsafe { libc::geteuid() }
        {
            return Err(AdmissionError::Directory);
        }
        let allowed = plan
            .apply
            .into_iter()
            .chain(plan.cleanup)
            .chain(plan.on_failure)
            .collect();
        Ok(Self {
            binaries,
            allowed,
            run_dir: run_dir.to_owned(),
            run_identity: identity(&metadata),
            pending: None,
        })
    }
    pub fn execute(
        &mut self,
        argv: &[String],
        deadline: Instant,
        cancel: Option<&AtomicBool>,
    ) -> Result<CommandResult, CommandError> {
        if self.pending.is_some() || !self.allowed.contains(argv) {
            return Err(CommandError::Failure);
        }
        check(deadline, cancel)?;
        let binary = match argv.first().map(String::as_str) {
            Some("ip") => &self.binaries.ip,
            Some("iptables") => &self.binaries.iptables,
            _ => return Err(CommandError::Failure),
        };
        if !binary.unchanged()
            || !fs::symlink_metadata(&self.run_dir).ok().is_some_and(|m| {
                identity(&m).dev == self.run_identity.dev
                    && identity(&m).ino == self.run_identity.ino
                    && m.mode() & 0o7777 == 0o700
            })
        {
            return Err(CommandError::Failure);
        }
        let mut command = Command::new(&binary.path);
        command
            .args(&argv[1..])
            .env_clear()
            .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
            .env("HOME", &self.run_dir)
            .env("TMPDIR", &self.run_dir)
            .current_dir(&self.run_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        unsafe {
            command.pre_exec(|| {
                if libc::setpgid(0, 0) != 0 {
                    libc::_exit(127)
                }
                Ok(())
            });
        }
        let mut child = command.spawn().map_err(|_| CommandError::Failure)?;
        let mut stdout = child.stdout.take();
        let mut stderr = child.stderr.take();
        for fd in [
            stdout.as_ref().map_or(-1, AsRawFd::as_raw_fd),
            stderr.as_ref().map_or(-1, AsRawFd::as_raw_fd),
        ]
        .into_iter()
        .filter(|fd| *fd >= 0)
        {
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
            {
                return self.finish_failure(child, CommandError::Failure);
            }
        }
        let mut output = Vec::with_capacity(4096);
        let outcome = loop {
            if let Err(error) = check(deadline, cancel) {
                break Err(error);
            }
            if let Some(pipe) = &mut stdout {
                match drain(pipe, &mut output) {
                    Ok(true) => stdout = None,
                    Ok(false) => {}
                    Err(error) => break Err(error),
                }
            }
            if let Some(pipe) = &mut stderr {
                match drain(pipe, &mut output) {
                    Ok(true) => stderr = None,
                    Ok(false) => {}
                    Err(error) => break Err(error),
                }
            }
            match exited(child.id()) {
                Ok(true) => break Ok(()),
                Ok(false) => {}
                Err(error) => break Err(error),
            }
            let mut fds = [
                libc::pollfd {
                    fd: stdout.as_ref().map_or(-1, AsRawFd::as_raw_fd),
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: stderr.as_ref().map_or(-1, AsRawFd::as_raw_fd),
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];
            let ms = deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(20))
                .as_millis()
                .max(1) as i32;
            if unsafe { libc::poll(fds.as_mut_ptr(), 2, ms) } < 0
                && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted
            {
                break Err(CommandError::Failure);
            }
        };
        // Leader is unreaped: its PID/group cannot be recycled before this signal.
        if !kill_owned_group(&child) {
            self.pending = Some(child);
            return Err(CommandError::Failure);
        }
        if !wait_exit(&child, Instant::now() + Duration::from_secs(1)) {
            self.pending = Some(child);
            return Err(CommandError::Timeout);
        }
        let mut drain_error = None;
        for pipe in [
            &mut stdout.as_mut().map(|p| p as &mut dyn Read),
            &mut stderr.as_mut().map(|p| p as &mut dyn Read),
        ] {
            if let Some(pipe) = pipe
                && let Err(error) = drain(pipe, &mut output)
            {
                drain_error = Some(error);
            }
        }
        let status = child.wait().map_err(|_| CommandError::Failure)?;
        outcome?;
        if let Some(error) = drain_error {
            return Err(error);
        }
        Ok(CommandResult {
            success: status.success(),
            output,
        })
    }
    fn finish_failure(
        &mut self,
        mut child: Child,
        error: CommandError,
    ) -> Result<CommandResult, CommandError> {
        if !kill_owned_group(&child) || !wait_exit(&child, Instant::now() + Duration::from_secs(1))
        {
            self.pending = Some(child);
            return Err(error);
        }
        let _ = child.wait();
        Err(error)
    }
    pub fn retry_abort(&mut self) -> Result<(), CommandError> {
        let Some(mut child) = self.pending.take() else {
            return Ok(());
        };
        if !kill_owned_group(&child) || !wait_exit(&child, Instant::now() + Duration::from_secs(1))
        {
            self.pending = Some(child);
            return Err(CommandError::Timeout);
        }
        child.wait().map_err(|_| CommandError::Failure)?;
        Ok(())
    }
}
fn check(deadline: Instant, cancel: Option<&AtomicBool>) -> Result<(), CommandError> {
    if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
        Err(CommandError::Cancelled)
    } else if Instant::now() >= deadline {
        Err(CommandError::Timeout)
    } else {
        Ok(())
    }
}
fn drain(pipe: &mut impl Read, output: &mut Vec<u8>) -> Result<bool, CommandError> {
    let mut scratch = [0u8; 8192];
    let mut total = 0;
    while total < DRAIN_PER_TICK {
        match pipe.read(&mut scratch) {
            Ok(0) => return Ok(true),
            Ok(n) => {
                if n > MAX_OUTPUT_BYTES.saturating_sub(output.len()) {
                    return Err(CommandError::Failure);
                }
                let needed = output.len() + n;
                if needed > output.capacity() {
                    let capacity = output
                        .capacity()
                        .saturating_mul(2)
                        .max(needed)
                        .min(MAX_OUTPUT_BYTES);
                    output
                        .try_reserve_exact(capacity - output.len())
                        .map_err(|_| CommandError::Failure)?;
                }
                output.extend_from_slice(&scratch[..n]);
                total += n;
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(false),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(CommandError::Failure),
        }
    }
    Ok(false)
}
fn exited(pid: u32) -> Result<bool, CommandError> {
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
                return Err(CommandError::Failure);
            }
        }
        #[cfg(target_os = "linux")]
        let child_pid = unsafe { info.si_pid() };
        #[cfg(target_os = "macos")]
        let child_pid = info.si_pid;
        Ok(child_pid == pid as libc::pid_t)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = pid;
        Err(CommandError::Failure)
    }
}
fn kill_owned_group(child: &Child) -> bool {
    if unsafe { libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL) } == 0 {
        return true;
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        return true;
    }
    #[cfg(target_os = "macos")]
    if error.raw_os_error() == Some(libc::EPERM) && exited(child.id()) == Ok(true) {
        return true;
    }
    false
}
fn wait_exit(child: &Child, deadline: Instant) -> bool {
    while Instant::now() < deadline {
        match exited(child.id()) {
            Ok(true) => return true,
            Err(_) => return false,
            _ => {}
        }
        unsafe {
            libc::poll(std::ptr::null_mut(), 0, 10);
        }
    }
    false
}

//! Fixed local native IO for migrated product modules. One caller-owned lane.
//! Command vectors are built by internal modules, never an HTTP argv endpoint.
use crate::readiness_tun::{Budget, TunError};
use std::{
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    os::{
        fd::AsRawFd,
        unix::{fs::OpenOptionsExt, process::CommandExt},
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Program {
    Uci,
    Ubus,
    Ip,
    Iptables,
    Ip6tables,
    Service,
    Curl,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Unavailable,
    Invalid,
    Limit,
    Deadline,
    Cancelled,
    Failed,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unavailable => "native source unavailable",
            Self::Invalid => "native input invalid",
            Self::Limit => "native resource limit",
            Self::Deadline => "native operation deadline",
            Self::Cancelled => "native operation cancelled",
            Self::Failed => "native operation failed",
        })
    }
}
impl std::error::Error for Error {}
pub struct Output {
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}
impl std::fmt::Debug for Output {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeOutput")
            .field("code", &self.code)
            .field("stdoutBytes", &self.stdout.len())
            .field("stderrBytes", &self.stderr.len())
            .finish()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Metadata {
    pub regular: bool,
    pub directory: bool,
    pub symlink: bool,
    pub mode: u32,
    pub uid: u32,
    pub size: u64,
    pub dev: u64,
    pub ino: u64,
}
pub trait Backend {
    fn read(&mut self, path: &Path, limit: usize, budget: &Budget<'_>) -> Result<Vec<u8>, Error>;
    fn run(
        &mut self,
        program: Program,
        args: &[String],
        stdin: Option<&[u8]>,
        limit: usize,
        budget: &Budget<'_>,
    ) -> Result<Output, Error>;
    fn now_unix(&self) -> u64;
    fn metadata(
        &mut self,
        _path: &Path,
        _follow: bool,
        _budget: &Budget<'_>,
    ) -> Result<Metadata, Error> {
        Err(Error::Unavailable)
    }
    fn list(
        &mut self,
        _path: &Path,
        _limit: usize,
        _budget: &Budget<'_>,
    ) -> Result<Vec<String>, Error> {
        Err(Error::Unavailable)
    }
    fn read_link(&mut self, _path: &Path, _budget: &Budget<'_>) -> Result<PathBuf, Error> {
        Err(Error::Unavailable)
    }
}
fn check(b: &Budget<'_>) -> Result<(), Error> {
    b.check().map_err(|e| {
        if e == TunError::Cancelled {
            Error::Cancelled
        } else {
            Error::Deadline
        }
    })
}
fn native_error(e: io::Error) -> Error {
    if matches!(
        e.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
    ) {
        Error::Unavailable
    } else {
        Error::Failed
    }
}
fn nonblocking(fd: i32) -> Result<(), Error> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        Err(Error::Failed)
    } else {
        Ok(())
    }
}
/// The exceptional unreaped finite command stays in this object. Another
/// command cannot replace it; the next explicit call retries only reaping.
#[derive(Default)]
pub struct Native {
    pending: Option<Child>,
}
impl std::fmt::Debug for Native {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NativeProductIO([fixed])")
    }
}
impl Native {
    pub fn new() -> Self {
        Self::default()
    }
    fn retry_pending(&mut self) -> Result<(), Error> {
        if let Some(child) = &mut self.pending {
            match child.try_wait() {
                Ok(Some(_)) => {
                    self.pending.take();
                }
                _ => return Err(Error::Failed),
            }
        }
        Ok(())
    }
    fn stop(&mut self, mut child: Child) {
        unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
        let until = Instant::now() + Duration::from_secs(1);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => return,
                Err(_) => break,
                _ => {}
            }
            if Instant::now() >= until {
                break;
            }
            let mut fd = libc::pollfd {
                fd: -1,
                events: 0,
                revents: 0,
            };
            unsafe { libc::poll(&mut fd, 0, 10) };
        }
        self.pending = Some(child);
    }
}
fn selector(program: Program, args: &[String]) -> Result<(PathBuf, &[String]), Error> {
    let path = match program {
        Program::Uci => "/sbin/uci",
        Program::Ubus => "/bin/ubus",
        Program::Ip => "/usr/sbin/ip",
        Program::Iptables => "/usr/sbin/iptables",
        Program::Ip6tables => "/usr/sbin/ip6tables",
        Program::Curl => "/usr/bin/curl",
        Program::Service => {
            let name = args.first().ok_or(Error::Invalid)?;
            if name.is_empty()
                || name.len() > 64
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
            {
                return Err(Error::Invalid);
            }
            if args.len() != 2
                || !matches!(
                    args[1].as_str(),
                    "start"
                        | "stop"
                        | "restart"
                        | "reload"
                        | "enable"
                        | "disable"
                        | "status"
                        | "enabled"
                )
            {
                return Err(Error::Invalid);
            }
            return Ok((
                if name == "wifi" {
                    PathBuf::from("/sbin/wifi")
                } else {
                    Path::new("/etc/init.d").join(name)
                },
                &args[1..],
            ));
        }
    };
    Ok((path.into(), args))
}
fn collect(reader: &mut impl Read, bytes: &mut Vec<u8>, limit: usize) -> Result<bool, Error> {
    let mut chunk = [0u8; 8192];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => return Ok(true),
            Ok(n) => {
                if n > limit.saturating_sub(bytes.len()) {
                    return Err(Error::Limit);
                }
                let need = bytes.len() + n;
                if need > bytes.capacity() {
                    let cap = need.max(bytes.capacity().saturating_mul(2)).min(limit);
                    bytes
                        .try_reserve_exact(cap - bytes.len())
                        .map_err(|_| Error::Limit)?;
                }
                bytes.extend_from_slice(&chunk[..n]);
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(false),
            Err(_) => return Err(Error::Failed),
        }
    }
}
impl Backend for Native {
    fn read(&mut self, path: &Path, limit: usize, b: &Budget<'_>) -> Result<Vec<u8>, Error> {
        check(b)?;
        if !path.is_absolute() || limit > 8 << 20 {
            return Err(Error::Invalid);
        }
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(path);
        check(b)?;
        let mut file = file.map_err(native_error)?;
        let metadata = file.metadata().map_err(native_error)?;
        if !metadata.is_file() {
            return Err(Error::Invalid);
        }
        if metadata.len() > limit as u64 {
            return Err(Error::Limit);
        }
        let mut raw = Vec::with_capacity((metadata.len() as usize).min(limit));
        let mut chunk = [0; 8192];
        loop {
            check(b)?;
            let size = chunk.len().min(limit.saturating_sub(raw.len()) + 1);
            let n = match file.read(&mut chunk[..size]) {
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                result => result.map_err(native_error)?,
            };
            check(b)?;
            if n == 0 {
                return Ok(raw);
            }
            if n > limit.saturating_sub(raw.len()) {
                return Err(Error::Limit);
            }
            let need = raw.len() + n;
            if need > raw.capacity() {
                let cap = need.max(raw.capacity().saturating_mul(2)).min(limit);
                raw.try_reserve_exact(cap - raw.len())
                    .map_err(|_| Error::Limit)?;
            }
            raw.extend_from_slice(&chunk[..n]);
        }
    }
    fn run(
        &mut self,
        program: Program,
        args: &[String],
        stdin: Option<&[u8]>,
        limit: usize,
        b: &Budget<'_>,
    ) -> Result<Output, Error> {
        check(b)?;
        self.retry_pending()?;
        if args.len() > 64
            || args
                .iter()
                .any(|arg| arg.len() > 8192 || arg.as_bytes().contains(&0))
            || stdin.is_some_and(|raw| raw.len() > 2 << 20)
            || limit > 8 << 20
        {
            return Err(Error::Limit);
        }
        let (path, args) = selector(program, args)?;
        let mut command = Command::new(path);
        command
            .args(args)
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_clear()
            .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
            .env("LC_ALL", "C");
        if let Some(libdir) = std::env::var_os("XTABLES_LIBDIR") {
            command.env("XTABLES_LIBDIR", libdir);
        }
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = command.spawn().map_err(native_error)?;
        let setup = (|| {
            nonblocking(child.stdout.as_ref().ok_or(Error::Failed)?.as_raw_fd())?;
            nonblocking(child.stderr.as_ref().ok_or(Error::Failed)?.as_raw_fd())?;
            if let Some(input) = &child.stdin {
                nonblocking(input.as_raw_fd())?;
            }
            Ok(())
        })();
        if let Err(error) = setup {
            self.stop(child);
            return Err(error);
        }
        let (mut stdout, mut stderr) = (
            Vec::with_capacity(8192.min(limit)),
            Vec::with_capacity(4096.min(limit)),
        );
        let (mut out_done, mut err_done, mut written) = (false, false, 0usize);
        let input = stdin.unwrap_or_default();
        let result = (|| {
            loop {
                check(b)?;
                if let Some(pipe) = &mut child.stdin {
                    if written == input.len() {
                        child.stdin.take();
                    } else {
                        match pipe.write(&input[written..]) {
                            Ok(0) => return Err(Error::Failed),
                            Ok(n) => written += n,
                            Err(e)
                                if matches!(
                                    e.kind(),
                                    io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                                ) => {}
                            Err(_) => return Err(Error::Failed),
                        }
                    }
                }
                if !out_done {
                    out_done = collect(
                        child.stdout.as_mut().ok_or(Error::Failed)?,
                        &mut stdout,
                        limit,
                    )?;
                }
                if !err_done {
                    err_done = collect(
                        child.stderr.as_mut().ok_or(Error::Failed)?,
                        &mut stderr,
                        limit,
                    )?;
                }
                check(b)?;
                if let Some(status) = child.try_wait().map_err(native_error)?
                    && out_done
                    && err_done
                {
                    return Ok(status.code().unwrap_or(128));
                }
                let mut descriptors = [
                    libc::pollfd {
                        fd: if out_done {
                            -1
                        } else {
                            child.stdout.as_ref().ok_or(Error::Failed)?.as_raw_fd()
                        },
                        events: libc::POLLIN,
                        revents: 0,
                    },
                    libc::pollfd {
                        fd: if err_done {
                            -1
                        } else {
                            child.stderr.as_ref().ok_or(Error::Failed)?.as_raw_fd()
                        },
                        events: libc::POLLIN,
                        revents: 0,
                    },
                    libc::pollfd {
                        fd: child.stdin.as_ref().map_or(-1, AsRawFd::as_raw_fd),
                        events: libc::POLLOUT,
                        revents: 0,
                    },
                ];
                let millis = b
                    .deadline
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(if out_done && err_done {
                        1
                    } else {
                        20
                    }))
                    .as_millis()
                    .max(1) as i32;
                let polled = unsafe {
                    libc::poll(
                        descriptors.as_mut_ptr(),
                        descriptors.len() as libc::nfds_t,
                        millis,
                    )
                };
                if polled < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
                    return Err(Error::Failed);
                }
            }
        })();
        match result {
            Ok(code) => Ok(Output {
                code,
                stdout,
                stderr,
            }),
            Err(error) => {
                self.stop(child);
                Err(error)
            }
        }
    }
    fn metadata(&mut self, path: &Path, follow: bool, b: &Budget<'_>) -> Result<Metadata, Error> {
        use std::os::unix::fs::MetadataExt;
        check(b)?;
        let result = if follow {
            fs::metadata(path)
        } else {
            fs::symlink_metadata(path)
        }
        .map_err(native_error);
        check(b)?;
        let m = result?;
        Ok(Metadata {
            regular: m.is_file(),
            directory: m.is_dir(),
            symlink: m.file_type().is_symlink(),
            mode: m.mode(),
            uid: m.uid(),
            size: m.len(),
            dev: m.dev(),
            ino: m.ino(),
        })
    }
    fn now_unix(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs())
    }
    fn list(&mut self, path: &Path, limit: usize, b: &Budget<'_>) -> Result<Vec<String>, Error> {
        check(b)?;
        if !path.is_absolute() || limit > 16384 {
            return Err(Error::Invalid);
        }
        let mut names = Vec::new();
        for entry in fs::read_dir(path).map_err(native_error)? {
            check(b)?;
            if names.len() == limit {
                return Err(Error::Limit);
            }
            let name = entry
                .map_err(native_error)?
                .file_name()
                .into_string()
                .map_err(|_| Error::Invalid)?;
            if name.len() > 255 {
                return Err(Error::Limit);
            }
            names.push(name);
        }
        names.sort();
        check(b)?;
        Ok(names)
    }
    fn read_link(&mut self, path: &Path, b: &Budget<'_>) -> Result<PathBuf, Error> {
        check(b)?;
        let result = fs::read_link(path).map_err(native_error);
        check(b)?;
        result
    }
}
/// UTC RFC3339 from Unix seconds. No locale/process or date framework.
pub fn timestamp(seconds: u64) -> String {
    let days = (seconds / 86400) as i64;
    let time = seconds % 86400;
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        time / 3600,
        time / 60 % 60,
        time % 60
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utc_epoch_and_known_calendar_are_exact() {
        assert_eq!(timestamp(0), "1970-01-01T00:00:00Z");
        assert_eq!(timestamp(951782400), "2000-02-29T00:00:00Z");
        assert_eq!(timestamp(1791158400), "2026-10-05T00:00:00Z");
    }
    #[test]
    fn service_selectors_are_fixed_not_shell_vectors() {
        assert!(selector(Program::Service, &["dropbear".into(), "restart".into()]).is_ok());
        assert!(selector(Program::Service, &["../dropbear".into(), "stop".into()]).is_err());
        assert!(selector(Program::Service, &["network".into(), "exec".into()]).is_err());
    }
}

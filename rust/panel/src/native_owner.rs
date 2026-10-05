//! Exclusive native assembly. Opening performs no network cleanup, fetch,
//! checker or core start. The caller explicitly initializes and retains this
//! owner after every cleanup/startup/close failure; no Drop stop bypass.
use crate::{
    artifact_source::SourcePolicy,
    capture_executor::Binaries,
    capture_kernel::TableNames,
    capture_runtime::{CaptureHandle, CaptureRuntime, CurrentStatus},
    capture_state::Controller,
    native_runtime::NativeReadiness,
    readiness_tun::NativeObserver,
    runtime_http::{RestoreError, RuntimeHttp},
    runtime_manager::{ArtifactBindings, HookError, Limits, Manager, ManagerError, ServiceId},
};
use std::{
    fmt,
    fs::{self, File},
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::{
            ffi::OsStrExt,
            fs::{DirBuilderExt, MetadataExt},
        },
    },
    path::{Component, Path, PathBuf},
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
#[derive(Clone)]
pub struct Options {
    pub data_dir: PathBuf,
    pub run_dir: PathBuf,
}
impl fmt::Debug for Options {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("NativeOwnerOptions([private])")
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenError {
    Roots,
    Capture,
    Manager(ManagerError),
    Setup,
}
impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Roots => "native owner roots invalid",
            Self::Capture => "saved capture ownership unavailable",
            Self::Manager(_) => "exclusive native manager unavailable",
            Self::Setup => "native owner setup unavailable",
        })
    }
}
impl std::error::Error for OpenError {}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitError {
    Closed,
    Cancelled,
    Capture(HookError),
}
impl fmt::Display for InitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Closed => "native owner is closing",
            Self::Cancelled => "native owner startup cancelled",
            Self::Capture(_) => "startup capture withdrawal pending",
        })
    }
}
impl std::error::Error for InitError {}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CloseError {
    pub capture: Option<HookError>,
    pub runtime: Option<ManagerError>,
}
impl fmt::Display for CloseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("native owner cleanup needs retry")
    }
}
impl std::error::Error for CloseError {}
pub type StartupOutcomes = Vec<(
    ServiceId,
    Result<crate::runtime_manager::Status, RestoreError>,
)>;
#[must_use = "retain this native owner and retry explicit close errors"]
pub struct NativeOwner {
    runtime: RuntimeHttp,
    capture: CaptureHandle<NativeObserver>,
    cancel: Rc<AtomicBool>,
    closing: bool,
    closed: bool,
    startup_withdrawn: bool,
    restore_dispatched: bool,
    #[cfg(test)]
    cancel_after_withdraw: bool,
}
impl fmt::Debug for NativeOwner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeOwner")
            .field("closing", &self.closing)
            .field("closed", &self.closed)
            .finish_non_exhaustive()
    }
}
fn private_root(path: &Path) -> Result<File, OpenError> {
    if !path.is_absolute()
        || path
            .components()
            .any(|p| matches!(p, Component::ParentDir | Component::CurDir))
    {
        return Err(OpenError::Roots);
    }
    let fd = unsafe {
        libc::open(
            c"/".as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(OpenError::Roots);
    }
    let mut directory = unsafe { OwnedFd::from_raw_fd(fd) };
    for part in path.components() {
        match part {
            Component::RootDir => {}
            Component::Normal(name) => {
                let name = std::ffi::CString::new(name.as_bytes()).map_err(|_| OpenError::Roots)?;
                let fd = unsafe {
                    libc::openat(
                        directory.as_raw_fd(),
                        name.as_ptr(),
                        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                    )
                };
                if fd < 0 {
                    return Err(OpenError::Roots);
                }
                directory = unsafe { OwnedFd::from_raw_fd(fd) };
            }
            _ => return Err(OpenError::Roots),
        }
    }
    let file = File::from(directory);
    let metadata = file.metadata().map_err(|_| OpenError::Roots)?;
    if !metadata.is_dir()
        || metadata.mode() & 0o7777 != 0o700
        || metadata.uid() != unsafe { libc::geteuid() }
    {
        return Err(OpenError::Roots);
    }
    Ok(file)
}
fn private_child(root: &Path, name: &str) -> Result<PathBuf, OpenError> {
    let path = root.join(name);
    match fs::symlink_metadata(&path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(&path)
                .map_err(|_| OpenError::Roots)?;
        }
        Err(_) => return Err(OpenError::Roots),
    }
    private_root(&path)?;
    Ok(path)
}
impl NativeOwner {
    pub fn open(
        options: Options,
        binaries: Binaries,
        names: TableNames,
        source: SourcePolicy,
        cancel: Rc<AtomicBool>,
    ) -> Result<Self, OpenError> {
        Self::open_with_artifacts(
            options, binaries, names, source, ArtifactBindings::default(), cancel,
        )
    }
    /// Attach only caller-supplied trusted local release bindings. Loading does
    /// not check, fetch or start a core; explicit initialization owns startup.
    pub fn open_with_artifacts(
        options: Options,
        binaries: Binaries,
        names: TableNames,
        source: SourcePolicy,
        artifacts: ArtifactBindings,
        cancel: Rc<AtomicBool>,
    ) -> Result<Self, OpenError> {
        let data = private_root(&options.data_dir)?;
        let run = private_root(&options.run_dir)?;
        let dm = data.metadata().map_err(|_| OpenError::Roots)?;
        let rm = run.metadata().map_err(|_| OpenError::Roots)?;
        if options.data_dir.starts_with(&options.run_dir)
            || options.run_dir.starts_with(&options.data_dir)
            || (dm.dev(), dm.ino()) == (rm.dev(), rm.ino())
        {
            return Err(OpenError::Roots);
        }
        let services = private_child(&options.data_dir, "services")?;
        for name in ["sing-box", "frpc"] {
            private_child(&services, name)?;
        }
        let sing_box = private_child(&options.run_dir, "sing-box")?;
        let frpc = private_child(&options.run_dir, "frpc")?;
        let exec = private_child(&options.run_dir, "capture-exec")?;
        // Only saved private state is read here; no action before Manager lock.
        let controller = Controller::open(&options.data_dir).map_err(|_| OpenError::Capture)?;
        let capture = CaptureRuntime::with_observer_cancel(
            controller,
            binaries,
            exec,
            names,
            NativeObserver::new(),
            cancel.clone(),
        );
        let (hooks, handle) =
            capture.into_hooks_with_handle(NativeReadiness::native(cancel.clone()));
        let manager = Manager::open(
            services,
            &options.run_dir,
            artifacts,
            hooks,
            Limits::default(),
        )
        .map_err(OpenError::Manager)?;
        let mut runtime = RuntimeHttp::new(manager);
        let setup = runtime.load_saved_intent(&options.data_dir).is_ok()
            && runtime
                .load_artifact_source(source, &sing_box, &frpc)
                .is_ok()
            && runtime.load_capture(handle.clone()).is_ok();
        if !setup {
            let _ = runtime.close();
            return Err(OpenError::Setup);
        }
        runtime.set_startup_blocked(true);
        Ok(Self {
            runtime,
            capture: handle,
            cancel,
            closing: false,
            closed: false,
            startup_withdrawn: false,
            restore_dispatched: false,
            #[cfg(test)]
            cancel_after_withdraw: false,
        })
    }
    pub fn runtime_mut(&mut self) -> &mut RuntimeHttp {
        &mut self.runtime
    }
    pub fn capture_unobserved(&self) -> Result<CurrentStatus, HookError> {
        self.capture.current_unobserved()
    }
    /// Explicit startup withdrawal precedes any saved-on acquisition or Run.
    /// The exclusive Manager lock is already held. Failure leaves owner intact.
    pub fn initialize(&mut self) -> Result<StartupOutcomes, InitError> {
        if self.closing || self.closed {
            return Err(InitError::Closed);
        }
        if crate::shutdown::requested() || self.cancel.load(Ordering::Acquire) {
            return Err(InitError::Cancelled);
        }
        if !self.startup_withdrawn {
            self.capture
                .startup_withdraw(Instant::now() + Duration::from_secs(30))
                .map_err(InitError::Capture)?;
            self.startup_withdrawn = true;
        }
        #[cfg(test)]
        if self.cancel_after_withdraw {
            self.cancel_after_withdraw = false;
            self.cancel.store(true, Ordering::Release);
        }
        if crate::shutdown::requested() || self.cancel.load(Ordering::Acquire) {
            return Err(InitError::Cancelled);
        }
        // Completed withdrawal is remembered, but cancellation must not
        // unblock activation. The initial restore dispatch runs once; bounded
        // retries after a per-service failure belong to poll_recovery.
        self.runtime.set_startup_blocked(false);
        if self.restore_dispatched {
            return Ok(Vec::new());
        }
        self.restore_dispatched = true;
        Ok(self.runtime.restore_saved())
    }
    /// Attempts both capture and core withdrawal. Failed close preserves all
    /// owned handles and prevents initialize/recovery. No artifact scavenging.
    pub fn close(&mut self) -> Result<(), CloseError> {
        if self.closed {
            return Ok(());
        }
        self.closing = true;
        let capture = self
            .capture
            .startup_withdraw(Instant::now() + Duration::from_secs(30))
            .err();
        let runtime = self.runtime.close().err();
        if capture.is_some() || runtime.is_some() {
            return Err(CloseError { capture, runtime });
        }
        self.closed = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{auth::Auth, capture_executor::TrustedBinary, capture_kernel, server::Service};
    use sha2::{Digest, Sha256};
    use std::{
        io::{Read, Write},
        net::{Shutdown, TcpListener, TcpStream},
        os::unix::fs::PermissionsExt,
        sync::atomic::AtomicU64,
        thread,
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    #[test]
    fn explicit_local_bindings_and_saved_on_are_load_only_until_initialize() {
        use crate::runtime_manager::{ArtifactBinding, ArtifactProvenance};
        let root = fs::canonicalize(std::env::temp_dir()).unwrap().join(format!(
            "native-owner-local-{}-{}", std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        for part in ["data", "run"] {
            fs::DirBuilder::new().mode(0o700).create(root.join(part)).unwrap();
        }
        let marker = root.join("executed");
        let source = format!("#!/bin/sh\nprintf executed > '{}'\nexit 9\n", marker.display());
        let command = root.join("command");
        fs::write(&command, source.as_bytes()).unwrap();
        fs::set_permissions(&command, fs::Permissions::from_mode(0o700)).unwrap();
        let digest = Sha256::digest(source.as_bytes()).into();
        for explicit in [false, true] {
            let mut artifacts = ArtifactBindings::default();
            if explicit {
                for service in [ServiceId::SingBox, ServiceId::Frpc] {
                    let directory = root.join("run").join(service.as_str());
                    // The default constructor already creates service run roots.
                    let path = directory.join(".artifact-release");
                    fs::write(&path, source.as_bytes()).unwrap();
                    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
                    let binding = ArtifactBinding::trusted_local(
                        service, directory, path, digest,
                        ArtifactProvenance::TrustedLocalModule,
                    );
                    match service {
                        ServiceId::SingBox => artifacts.sing_box = Some(binding),
                        ServiceId::Frpc => artifacts.frpc = Some(binding),
                    }
                }
                let intent = root.join("data/desired-services.json");
                fs::write(&intent, br#"{"sing-box":true,"frpc":true}"#).unwrap();
                fs::set_permissions(&intent, fs::Permissions::from_mode(0o600)).unwrap();
            }
            let binaries = Binaries {
                ip: TrustedBinary::admit(&command, digest).unwrap(),
                iptables: TrustedBinary::admit(&command, digest).unwrap(),
            };
            let options = Options { data_dir: root.join("data"), run_dir: root.join("run") };
            let source = SourcePolicy::native("127.0.0.1:53".parse().unwrap()).unwrap();
            let cancel = Rc::new(AtomicBool::new(false));
            let mut owner = if explicit {
                NativeOwner::open_with_artifacts(
                    options, binaries, capture_kernel::table_names(b"").unwrap(),
                    source, artifacts, cancel,
                )
            } else {
                NativeOwner::open(
                    options, binaries, capture_kernel::table_names(b"").unwrap(), source, cancel,
                )
            }.unwrap();
            assert!(!marker.exists());
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let peer = thread::spawn(move || {
                let mut stream = TcpStream::connect(address).unwrap();
                stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                stream.write_all(b"GET /api/runtime HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
                stream.shutdown(Shutdown::Write).unwrap();
                let mut response = Vec::new();
                stream.read_to_end(&mut response).unwrap();
                response
            });
            Service::new("/proc".into()).with_auth(Auth::new(""))
                .handle_with_runtime(listener.accept().unwrap().0, owner.runtime_mut()).unwrap();
            let response = peer.join().unwrap();
            assert!(response.starts_with(b"HTTP/1.1 200 "));
            let body = response.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
            let status: serde_json::Value = serde_json::from_slice(&response[body..]).unwrap();
            for service in status["services"].as_array().unwrap() {
                assert_eq!(service["artifactAvailable"], explicit);
                assert_eq!(service["configured"], false);
                assert!(service["pid"].is_null());
            }
            assert!(!marker.exists());
            owner.close().unwrap();
            drop(owner);
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn cancellation_at_withdrawal_completion_keeps_api_startup_gate_until_retry() {
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "native-owner-cancel-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        for part in ["data", "run"] {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(root.join(part))
                .unwrap();
        }
        let path = root.join("fake-command");
        let source = b"#!/bin/sh\nexit 0\n";
        fs::write(&path, source).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        let digest = Sha256::digest(source).into();
        let binaries = Binaries {
            ip: TrustedBinary::admit(&path, digest).unwrap(),
            iptables: TrustedBinary::admit(&path, digest).unwrap(),
        };
        let cancel = Rc::new(AtomicBool::new(false));
        let mut owner = NativeOwner::open(
            Options {
                data_dir: root.join("data"),
                run_dir: root.join("run"),
            },
            binaries,
            capture_kernel::table_names(b"").unwrap(),
            SourcePolicy::native("127.0.0.1:53".parse().unwrap()).unwrap(),
            cancel.clone(),
        )
        .unwrap();
        owner.cancel_after_withdraw = true;
        assert_eq!(owner.initialize(), Err(InitError::Cancelled));
        assert!(owner.startup_withdrawn);
        assert!(owner.runtime_mut().restore_saved().is_empty());
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let peer = thread::spawn(move || {
            let mut stream = TcpStream::connect(address).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let body =
                b"{\"service\":\"frpc\",\"config\":\"serverPort = 7000\\n\",\"generation\":0}";
            let headers = format!(
                "POST /api/runtime/configure HTTP/1.1\r\nHost: localhost\r\nOrigin: http://localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                body.len()
            );
            stream.write_all(headers.as_bytes()).unwrap();
            stream.write_all(body).unwrap();
            stream.shutdown(Shutdown::Write).unwrap();
            let mut response = Vec::new();
            stream.read_to_end(&mut response).unwrap();
            response
        });
        Service::new("/proc".into())
            .with_auth(Auth::new(""))
            .handle_with_runtime(listener.accept().unwrap().0, owner.runtime_mut())
            .unwrap();
        let response = peer.join().unwrap();
        assert!(
            response.starts_with(b"HTTP/1.1 503 "),
            "{}",
            String::from_utf8_lossy(&response)
        );
        assert!(
            String::from_utf8(response)
                .unwrap()
                .contains("startup_cleanup_pending")
        );
        assert!(!root.join("data/services/frpc/state.json").exists());
        cancel.store(false, Ordering::Release);
        assert!(owner.initialize().unwrap().is_empty());
        owner.close().unwrap();
        drop(owner);
        fs::remove_dir_all(root).unwrap();
    }
}

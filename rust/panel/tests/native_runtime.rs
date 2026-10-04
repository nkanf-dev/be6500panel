#![cfg(any(target_os = "linux", target_os = "macos"))]
use be6500_panel::native_runtime::NativeReadiness;
use be6500_panel::readiness_tun::{
    Budget, FileIdentity, Interface, Ipv4Prefix, Observer, TunError,
};
use be6500_panel::runtime_manager::{
    ArtifactBinding, ArtifactBindings, ArtifactProvenance, Failure, HookError, HookStage, Hooks,
    Limits, Manager, ServiceId,
};
use be6500_panel::runtime_process::Limits as ProcessLimits;
use sha2::{Digest, Sha256};
use std::{
    cell::Cell,
    fs,
    net::{TcpListener, UdpSocket},
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};
static SERIAL: Mutex<()> = Mutex::new(());
static NEXT: AtomicU64 = AtomicU64::new(0);
const HELPER: &str = r#"#!/bin/sh
umask 077
case "$1" in check|verify) exit 0;; esac
trap 'exit 0' TERM
printf '%s\n' "$$" > "$TMPDIR/started"
IFS= read -r value < "$TMPDIR/wait"
"#;
struct NoHostObserver;
impl Observer for NoHostObserver {
    fn read_file(&mut self, _: &Path, _: usize, _: &Budget<'_>) -> Result<Vec<u8>, TunError> {
        Err(TunError::Unavailable)
    }
    fn read_link(&mut self, _: &Path, _: &Budget<'_>) -> Result<PathBuf, TunError> {
        Err(TunError::Unavailable)
    }
    fn metadata(&mut self, _: &Path, _: bool, _: &Budget<'_>) -> Result<FileIdentity, TunError> {
        Err(TunError::Unavailable)
    }
    fn list_dir(&mut self, _: &Path, _: usize, _: &Budget<'_>) -> Result<Vec<String>, TunError> {
        Err(TunError::Unavailable)
    }
    fn interfaces(&mut self, _: &Budget<'_>) -> Result<Vec<Interface>, TunError> {
        Err(TunError::Unavailable)
    }
    fn ipv4_routes(&mut self, _: &Budget<'_>) -> Result<Vec<Ipv4Prefix>, TunError> {
        Err(TunError::Unavailable)
    }
}
struct Fixture {
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "b6p-native-hook-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        for dir in ["artifacts", "run", "run/sing-box"] {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(root.join(dir))
                .unwrap();
        }
        fs::write(root.join("artifacts/.artifact-fake"), HELPER).unwrap();
        fs::set_permissions(
            root.join("artifacts/.artifact-fake"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        let fifo = std::ffi::CString::new(
            root.join("run/sing-box/wait")
                .as_os_str()
                .as_encoded_bytes(),
        )
        .unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        Self { root }
    }
    fn manager(&self, hooks: Hooks) -> Guard {
        let artifact = self.root.join("artifacts/.artifact-fake");
        let binding = ArtifactBinding::trusted_local(
            ServiceId::SingBox,
            self.root.join("artifacts"),
            artifact,
            Sha256::digest(HELPER.as_bytes()).into(),
            ArtifactProvenance::TrustedLocalModule,
        );
        Guard(
            Manager::open(
                self.root.join("services"),
                self.root.join("run"),
                ArtifactBindings {
                    sing_box: Some(binding),
                    frpc: None,
                },
                hooks,
                Limits {
                    process: ProcessLimits {
                        term_grace: Duration::from_millis(100),
                        kill_grace: Duration::from_secs(1),
                        check_timeout: Duration::from_secs(2),
                    },
                    readiness_timeout: Duration::from_millis(600),
                    resource_timeout: Duration::from_secs(1),
                },
            )
            .unwrap(),
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
struct Guard(Manager);
impl std::ops::Deref for Guard {
    type Target = Manager;
    fn deref(&self) -> &Manager {
        &self.0
    }
}
impl std::ops::DerefMut for Guard {
    fn deref_mut(&mut self) -> &mut Manager {
        &mut self.0
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.close().expect("fake native owner cleanup");
    }
}
fn config(mixed: u16, dns: u16) -> Vec<u8> {
    format!(r#"{{"inbounds":[{{"type":"mixed","listen":"127.0.0.1","listen_port":{mixed}}},{{"type":"direct","tag":"dns-in","listen":"127.0.0.1","listen_port":{dns},"network":"udp"}}],"route":{{"rules":[{{"action":"hijack-dns","inbound":"dns-in"}}]}},"dns":{{"rules":[{{"server":"dns-direct","domain":"bootstrap.test"}}]}}}}"#).into_bytes()
}
fn answer(query: &[u8]) -> Vec<u8> {
    let mut reply = query.to_vec();
    reply[2..4].copy_from_slice(&0x8180u16.to_be_bytes());
    reply[6..8].copy_from_slice(&1u16.to_be_bytes());
    reply.extend_from_slice(&[0xc0, 12, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 192, 0, 2, 1]);
    reply
}
#[test]
fn native_callbacks_use_retained_owner_and_actual_local_dns() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
    let mixed = tcp.local_addr().unwrap().port();
    let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
    udp.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let dns = udp.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let mut bytes = [0u8; 4096];
        let (size, peer) = udp.recv_from(&mut bytes).unwrap();
        udp.send_to(&answer(&bytes[..size]), peer).unwrap();
    });
    let cleanup = Rc::new(Cell::new(0));
    let cleanup_calls = cleanup.clone();
    let hooks = NativeReadiness::with_observer(NoHostObserver, Rc::new(AtomicBool::new(false)))
        .into_hooks(
            move |context| {
                assert!(context.run.is_some());
                cleanup_calls.set(cleanup_calls.get() + 1);
                Ok(())
            },
            |context| {
                assert!(context.owned_status.is_some());
                Ok(())
            },
        );
    let mut manager = fixture.manager(hooks);
    manager
        .configure(ServiceId::SingBox, 0, &config(mixed, dns), None)
        .unwrap();
    let status = manager.start(ServiceId::SingBox).unwrap();
    server.join().unwrap();
    assert!(status.ready && status.active);
    assert!(status.pid.is_some());
    manager.stop(ServiceId::SingBox).unwrap();
    assert_eq!(cleanup.get(), 1);
    drop(tcp);
}
#[test]
fn native_dns_failure_never_marks_configuration_ready() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
    let mixed = tcp.local_addr().unwrap().port();
    let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
    udp.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let dns = udp.local_addr().unwrap().port();
    let (stop, stopped) = mpsc::channel();
    let server = thread::spawn(move || {
        let mut query = [0u8; 4096];
        let _ = udp.recv_from(&mut query);
        let _ = stopped.recv_timeout(Duration::from_secs(2));
    });
    let hooks = NativeReadiness::with_observer(NoHostObserver, Rc::new(AtomicBool::new(false)))
        .into_hooks(|_| Ok(()), |_| Ok(()));
    let mut manager = fixture.manager(hooks);
    manager
        .configure(ServiceId::SingBox, 0, &config(mixed, dns), None)
        .unwrap();
    let error = manager.start(ServiceId::SingBox).unwrap_err();
    let _ = stop.send(());
    server.join().unwrap();
    assert!(matches!(
        error.failure,
        Failure::Hook(HookStage::Readiness, HookError::Deadline)
    ));
    let status = manager.status(ServiceId::SingBox).unwrap();
    assert!(!status.ready && !status.active);
    assert!(status.pid.is_none());
    drop(tcp);
}
#[test]
fn cancelled_native_prestart_runs_no_observer_or_child() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let hooks = NativeReadiness::with_observer(NoHostObserver, Rc::new(AtomicBool::new(true)))
        .into_hooks(|_| Ok(()), |_| Ok(()));
    let mut manager = fixture.manager(hooks);
    manager
        .configure(ServiceId::SingBox, 0, &config(2080, 1053), None)
        .unwrap();
    let error = manager.start(ServiceId::SingBox).unwrap_err();
    assert_eq!(
        error.failure,
        Failure::Hook(HookStage::PreStart, HookError::Cancelled)
    );
    assert!(!fixture.root.join("run/sing-box/started").exists());
    assert!(manager.status(ServiceId::SingBox).unwrap().pid.is_none());
}

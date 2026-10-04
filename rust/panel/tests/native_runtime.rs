#![cfg(any(target_os = "linux", target_os = "macos"))]
use be6500_panel::native_runtime::NativeReadiness;
use be6500_panel::readiness_tun::{
    Budget, FileIdentity, Interface, InterfaceAddress, Ipv4Prefix, Observer, TunError,
};
use be6500_panel::runtime_manager::{
    ArtifactBinding, ArtifactBindings, ArtifactProvenance, Failure, HookError, HookStage, Hooks,
    HookContext, Limits, Manager, ServiceId,
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
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
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
        self.manager_for_service(hooks, ServiceId::SingBox)
    }
    fn manager_for_service(&self, hooks: Hooks, service: ServiceId) -> Guard {
        if service == ServiceId::Frpc {
            fs::DirBuilder::new().mode(0o700).create(self.root.join("run/frpc")).unwrap();
            let fifo = std::ffi::CString::new(self.root.join("run/frpc/wait").as_os_str().as_encoded_bytes()).unwrap();
            assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        }
        let artifact = self.root.join("artifacts/.artifact-fake");
        let binding = ArtifactBinding::trusted_local(
            service,
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
                    sing_box: (service == ServiceId::SingBox).then(|| binding.clone()),
                    frpc: (service == ServiceId::Frpc).then_some(binding),
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

// Current-proof fixtures use the real Manager-owned callback/PID/config/artifact.
// Only proc/netlink views are synthetic: no host TUN, kernel mutation or device.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CurrentCase {
    Good,
    MissingTun,
    WrongSocket,
    ProcChange,
    ArtifactChange,
    ConfigChange,
    CancelBefore,
    DeadlineBefore,
    CancelAfter,
    NoDns,
    InvalidDns,
    SilentDns,
}
struct CurrentObserver {
    pid: u32,
    artifact: PathBuf,
    root: PathBuf,
    after_dns: Arc<AtomicBool>,
    cancel: Rc<AtomicBool>,
    case: CurrentCase,
    calls: Rc<Cell<usize>>,
}
impl CurrentObserver {
    fn new(context: &HookContext<'_>, case: CurrentCase, after_dns: Arc<AtomicBool>,
        cancel: Rc<AtomicBool>, calls: Rc<Cell<usize>>) -> Self {
        let real = context.owned_status.unwrap()().unwrap();
        assert_eq!(real.pid, Some(context.run.unwrap().pid()));
        Self { pid: real.pid.unwrap(), artifact: context.artifact_path.unwrap().into(),
            root: context.artifact_root.unwrap().into(), after_dns, cancel, case, calls }
    }
    fn checked(&self, budget: &Budget<'_>) -> Result<(), TunError> {
        self.calls.set(self.calls.get() + 1);
        if self.case == CurrentCase::CancelAfter && self.after_dns.load(Ordering::Acquire) {
            self.cancel.store(true, Ordering::Release);
        }
        budget.check()
    }
    fn base(&self) -> PathBuf { PathBuf::from(format!("/proc/{}", self.pid)) }
}
impl Observer for CurrentObserver {
    fn read_file(&mut self, path: &Path, _: usize, budget: &Budget<'_>) -> Result<Vec<u8>, TunError> {
        self.checked(budget)?;
        if path == self.base().join("stat") {
            let changed = self.case == CurrentCase::ProcChange && self.after_dns.load(Ordering::Acquire);
            return Ok(format!("{} (private fixture) S {} {}\n", self.pid,
                ["0"; 18].join(" "), if changed { 101 } else { 100 }).into_bytes());
        }
        if path == Path::new("/proc/sys/net/ipv4/conf/b6p-tun/rp_filter") { return Ok(b"2\n".to_vec()); }
        if path == self.base().join("fdinfo/7") { return Ok(b"iff:\tb6p-tun\n".to_vec()); }
        if path == Path::new("/proc/net/tcp") {
            let ip = u32::from_ne_bytes([172, 31, 255, 253]);
            return Ok(format!("sl local_address rem_address st tx_queue rx_queue tr tm->when retrnsmt uid timeout inode\n0: {ip:08X}:88B9 00000000:0000 0A 00000000:00000000 00:00000000 00000000 0 0 111 1\n").into_bytes());
        }
        Err(TunError::Unavailable)
    }
    fn read_link(&mut self, path: &Path, budget: &Budget<'_>) -> Result<PathBuf, TunError> {
        self.checked(budget)?;
        if path == self.base().join("exe") { return Ok(self.artifact.clone()); }
        if path == self.base().join("fd/7") { return Ok("/dev/net/tun".into()); }
        if path == self.base().join("fd/8") {
            return Ok(if self.case == CurrentCase::WrongSocket { "socket:[222]" } else { "socket:[111]" }.into());
        }
        Err(TunError::Unavailable)
    }
    fn metadata(&mut self, path: &Path, _: bool, budget: &Budget<'_>) -> Result<FileIdentity, TunError> {
        self.checked(budget)?;
        let actual = if path == self.artifact || path == self.base().join("exe") {
            &self.artifact
        } else if path == self.root { &self.root } else { return Err(TunError::Unavailable); };
        fs::metadata(actual).map(|m| FileIdentity::from_metadata(&m)).map_err(|_| TunError::Observation)
    }
    fn list_dir(&mut self, path: &Path, _: usize, budget: &Budget<'_>) -> Result<Vec<String>, TunError> {
        self.checked(budget)?;
        if path != self.base().join("fd") { return Err(TunError::Unavailable); }
        Ok(vec!["7".into(), "8".into()])
    }
    fn interfaces(&mut self, budget: &Budget<'_>) -> Result<Vec<Interface>, TunError> {
        self.checked(budget)?;
        if self.case == CurrentCase::MissingTun { return Ok(vec![]); }
        Ok(vec![Interface { name: "b6p-tun".into(), up: true, mtu: 1500,
            addresses: vec![InterfaceAddress { address: "172.31.255.253".parse().unwrap(), bits: 30 }] }])
    }
    fn ipv4_routes(&mut self, budget: &Budget<'_>) -> Result<Vec<Ipv4Prefix>, TunError> {
        self.checked(budget)?;
        Ok(vec![])
    }
}
fn current_config(mixed: u16, dns: u16, tun: bool, no_dns: bool) -> Vec<u8> {
    let raw = config(mixed, dns);
    let mut value: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    if no_dns { value["inbounds"].as_array_mut().unwrap().pop(); }
    if tun {
        value["inbounds"].as_array_mut().unwrap().push(serde_json::json!({
            "type":"tun", "tag":"tun-in", "interface_name":"b6p-tun",
            "address":["172.31.255.253/30"], "mtu":1500, "stack":"system",
            "dns_mode":"disabled", "auto_route":false, "auto_redirect":false,
            "udp_timeout":"2m", "udp_nat_max":1024
        }));
        value["route"]["rules"].as_array_mut().unwrap().push(serde_json::json!({"ip_version":6,"outbound":"direct"}));
    }
    serde_json::to_vec(&value).unwrap()
}
fn exercise_current(case: CurrentCase, tun: bool) -> (Result<(), Failure>, usize, bool) {
    let fixture = Fixture::new();
    let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
    let mixed = tcp.local_addr().unwrap().port();
    let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
    udp.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let dns = udp.local_addr().unwrap().port();
    let after_dns = Arc::new(AtomicBool::new(false));
    let completed_dns = after_dns.clone();
    let (paths, receive_paths) = mpsc::channel::<(PathBuf, PathBuf)>();
    let serve = !matches!(case, CurrentCase::MissingTun | CurrentCase::WrongSocket
        | CurrentCase::CancelBefore | CurrentCase::DeadlineBefore | CurrentCase::NoDns);
    let server = if serve {
        Some(thread::spawn(move || {
            let (config_path, artifact_path) = receive_paths.recv_timeout(Duration::from_secs(2)).unwrap();
            let mut bytes = [0u8; 4096];
            let (size, peer) = udp.recv_from(&mut bytes).unwrap();
            if case == CurrentCase::ArtifactChange { fs::write(artifact_path, format!("{HELPER}\n")).unwrap(); }
            if case == CurrentCase::ConfigChange { fs::write(config_path, b"changed accepted bytes").unwrap(); }
            completed_dns.store(true, Ordering::Release);
            if case != CurrentCase::SilentDns {
                let reply = if case == CurrentCase::InvalidDns { vec![0; 12] } else { answer(&bytes[..size]) };
                udp.send_to(&reply, peer).unwrap();
            } else {
                // Keep bound socket alive until the one-shot caller returns.
                let _ = receive_paths.recv_timeout(Duration::from_secs(2));
            }
        }))
    } else { None };
    let cancel = Rc::new(AtomicBool::new(case == CurrentCase::CancelBefore));
    let calls = Rc::new(Cell::new(0));
    let observed_calls = calls.clone();
    let hooks = Hooks::new(|_| Ok(()), move |context| {
        let observer = CurrentObserver::new(context, case, after_dns.clone(), cancel.clone(), observed_calls.clone());
        let mut readiness = NativeReadiness::with_observer(observer, cancel.clone());
        let _ = paths.send((context.config_path.into(), context.artifact_path.unwrap().into()));
        if case == CurrentCase::DeadlineBefore {
            let expired = HookContext { deadline: Instant::now(), ..*context };
            readiness.observe_current(&expired)
        } else { readiness.observe_current(context) }
    }, |_| Ok(()), |_| Ok(()));
    let mut manager = fixture.manager(hooks);
    manager.configure(ServiceId::SingBox, 0, &current_config(mixed, dns, tun, case == CurrentCase::NoDns), None).unwrap();
    let result = manager.start(ServiceId::SingBox).map(|_| ()).map_err(|error| error.failure);
    let active = manager.status(ServiceId::SingBox).unwrap().active;
    // Release any silent DNS server by dropping the callback channel.
    drop(manager);
    if let Some(server) = server { server.join().unwrap(); }
    drop(tcp);
    (result, calls.get(), active)
}
#[test]
fn current_tun_and_non_tun_use_retained_owner_and_actual_dns_answer_once() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    for tun in [false, true] {
        let (result, calls, active) = exercise_current(CurrentCase::Good, tun);
        assert_eq!(result, Ok(()));
        assert!(calls > 0 && active);
    }
}
#[test]
fn current_tun_without_actual_dns_or_owned_socket_never_passes() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    for case in [CurrentCase::MissingTun, CurrentCase::WrongSocket, CurrentCase::NoDns,
        CurrentCase::InvalidDns, CurrentCase::SilentDns] {
        let (result, _, active) = exercise_current(case, true);
        assert!(result.is_err());
        assert!(!active);
    }
}
#[test]
fn current_identity_and_accepted_hash_must_survive_dns_interval() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    for tun in [false, true] {
        for case in [CurrentCase::ProcChange, CurrentCase::ArtifactChange, CurrentCase::ConfigChange] {
            let (result, _, active) = exercise_current(case, tun);
            assert_eq!(result, Err(Failure::Hook(HookStage::Readiness, HookError::Failed)));
            assert!(!active);
        }
    }
}
#[test]
fn current_budget_refuses_before_observation_and_after_dns() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    for (case, error) in [(CurrentCase::CancelBefore, HookError::Cancelled),
        (CurrentCase::DeadlineBefore, HookError::Deadline), (CurrentCase::CancelAfter, HookError::Cancelled)] {
        let (result, calls, active) = exercise_current(case, true);
        assert_eq!(result, Err(Failure::Hook(HookStage::Readiness, error)));
        if case != CurrentCase::CancelAfter { assert_eq!(calls, 0); }
        assert!(!active);
    }
}

#[test]
fn current_frpc_proves_actual_retained_process_identity_without_dns_or_tun() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    for changed in [false, true] {
        let fixture = Fixture::new();
        let after = Arc::new(AtomicBool::new(false));
        let calls = Rc::new(Cell::new(0));
        let observed_calls = calls.clone();
        let hooks = Hooks::new(|_| Ok(()), move |context| {
            assert_eq!(context.service, ServiceId::Frpc);
            let cancel = Rc::new(AtomicBool::new(false));
            let mut observer = CurrentObserver::new(context, CurrentCase::Good, after.clone(), cancel.clone(), observed_calls.clone());
            if changed {
                // The actual admitted artifact changes AFTER the hook received
                // its retained metadata. No PID/client success injection.
                fs::write(context.artifact_path.unwrap(), format!("{HELPER}\n")).unwrap();
            }
            // Fail any TUN/DNS attempt by providing no listener/interface view.
            observer.case = CurrentCase::MissingTun;
            NativeReadiness::with_observer(observer, cancel).observe_current(context)
        }, |_| Ok(()), |_| Ok(()));
        let mut manager = fixture.manager_for_service(hooks, ServiceId::Frpc);
        manager.configure(ServiceId::Frpc, 0, b"serverAddr = \"127.0.0.1\"\nserverPort = 7000\n", None).unwrap();
        let started = manager.start(ServiceId::Frpc);
        if changed {
            assert_eq!(started.unwrap_err().failure, Failure::Hook(HookStage::Readiness, HookError::Failed));
        } else {
            assert!(started.unwrap().active);
        }
        assert!(calls.get() > 0);
    }
}

#[test]
fn current_without_a_retained_run_cannot_observe_or_promote_a_config() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let reached = Rc::new(Cell::new(0));
    let calls = reached.clone();
    let hooks = Hooks::new(move |context| {
        assert!(context.run.is_none());
        calls.set(calls.get() + 1);
        NativeReadiness::with_observer(NoHostObserver, Rc::new(AtomicBool::new(false)))
            .observe_current(context)
    }, |_| Ok(()), |_| Ok(()), |_| Ok(()));
    let mut manager = fixture.manager(hooks);
    manager.configure(ServiceId::SingBox, 0, &config(2080, 1053), None).unwrap();
    assert_eq!(manager.start(ServiceId::SingBox).unwrap_err().failure,
        Failure::Hook(HookStage::PreStart, HookError::Failed));
    assert_eq!(reached.get(), 1);
    assert!(manager.status(ServiceId::SingBox).unwrap().pid.is_none());
    assert!(!fixture.root.join("run/sing-box/started").exists());
}

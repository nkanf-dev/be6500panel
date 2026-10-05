#![cfg(unix)]
use be6500_panel::{
    artifact_source::SourcePolicy,
    auth::Auth,
    capture_executor::{Binaries, TrustedBinary},
    capture_kernel::table_names,
    capture_plan::RulesPlanInput,
    capture_state::{CommandResult, Controller, Desired},
    native::Ports,
    native_owner::{NativeOwner, Options},
    runtime_http::RuntimeHttp,
    server::Service,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::PathBuf,
    rc::Rc,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    thread,
    time::Duration,
};
static NEXT: AtomicU64 = AtomicU64::new(0);
const COMMAND: &str = r#"#!/bin/sh
umask 077
printf '%s\n' "$*" >> "$TMPDIR/commands"
IFS= read -r mode < "$TMPDIR/mode" || :
case "$mode" in fail) exit 8;; esac
exit 0
"#;
struct Fixture {
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "b6p-native-owner-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        for part in ["data", "run", "commands"] {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(root.join(part))
                .unwrap();
        }
        fs::write(root.join("commands/fake-network"), COMMAND).unwrap();
        fs::set_permissions(
            root.join("commands/fake-network"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        Self { root }
    }
    fn options(&self) -> Options {
        Options {
            data_dir: self.root.join("data"),
            run_dir: self.root.join("run"),
        }
    }
    fn binaries(&self) -> Binaries {
        let path = self.root.join("commands/fake-network");
        let digest = Sha256::digest(COMMAND.as_bytes()).into();
        Binaries {
            ip: TrustedBinary::admit(&path, digest).unwrap(),
            iptables: TrustedBinary::admit(&path, digest).unwrap(),
        }
    }
    fn owner(&self) -> NativeOwner {
        NativeOwner::open(
            self.options(),
            self.binaries(),
            table_names(b"").unwrap(),
            SourcePolicy::loopback_fixture("127.0.0.1:53".parse().unwrap()).unwrap(),
            Rc::new(AtomicBool::new(false)),
        )
        .unwrap()
    }
    fn journal(&self) {
        let mut capture = Controller::open(self.root.join("data")).unwrap();
        capture
            .set_desired(Desired {
                scope: "gateway".into(),
                lan_ipv4_prefixes: vec!["192.168.50.0/24".into()],
                desired: false,
                ..Desired::default()
            })
            .unwrap();
        let mut desired = capture.desired();
        desired.desired = true;
        capture.set_desired(desired).unwrap();
        capture
            .apply(
                input(),
                |_, _, _| Ok(()),
                |_, _| Ok(CommandResult::success()),
            )
            .unwrap();
        let mut desired = capture.desired();
        desired.desired = false;
        capture.set_desired(desired).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
fn input() -> RulesPlanInput {
    RulesPlanInput {
        scope: "gateway".into(),
        lan_ipv4_prefixes: vec!["192.168.50.0/24".into()],
        datapath: "routed-tun".into(),
        tun_interface: "b6p-test".into(),
        tun_address: "172.30.0.1/30".into(),
        lan_interface: "br-lan".into(),
        ports: Ports {
            mixed: 2080,
            tproxy: 7893,
            dns: 1053,
        },
        ipv6: "direct".into(),
        failure: "direct".into(),
        ..RulesPlanInput::default()
    }
}
fn exchange(service: &Service, runtime: &mut RuntimeHttp, request: Vec<u8>) -> Vec<u8> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let peer = thread::spawn(move || {
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream.write_all(&request).unwrap();
        stream.shutdown(Shutdown::Write).unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).unwrap();
        response
    });
    let stream = listener.accept().unwrap().0;
    service.handle_with_runtime(stream, runtime).unwrap();
    peer.join().unwrap()
}
fn response(response: Vec<u8>, expected: u16) -> Value {
    assert!(
        response.starts_with(format!("HTTP/1.1 {expected} ").as_bytes()),
        "{}",
        String::from_utf8_lossy(&response)
    );
    let end = response.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    serde_json::from_slice(&response[end..]).unwrap()
}
fn status(owner: &mut NativeOwner) -> Value {
    let service = Service::new("/proc".into()).with_auth(Auth::new(""));
    response(
        exchange(
            &service,
            owner.runtime_mut(),
            b"GET /api/runtime HTTP/1.1\r\nHost: localhost\r\n\r\n".to_vec(),
        ),
        200,
    )
}
#[test]
fn constructor_and_duplicate_owner_do_no_startup_cleanup_or_child_adoption() {
    let fixture = Fixture::new();
    fixture.journal();
    fs::write(fixture.root.join("data/.rollback-reserve"), b"preserved").unwrap();
    let journal = fs::read(fixture.root.join("data/capture-journal.json")).unwrap();
    let mut owner = fixture.owner();
    assert!(!fixture.root.join("run/capture-exec/commands").exists());
    assert!(owner.capture_unobserved().unwrap().intent.cleanup_pending);
    let current = status(&mut owner);
    assert_eq!(current["services"][0]["artifactAvailable"], false);
    assert!(
        NativeOwner::open(
            fixture.options(),
            fixture.binaries(),
            table_names(b"").unwrap(),
            SourcePolicy::loopback_fixture("127.0.0.1:53".parse().unwrap()).unwrap(),
            Rc::new(AtomicBool::new(false))
        )
        .is_err()
    );
    assert_eq!(
        fs::read(fixture.root.join("data/capture-journal.json")).unwrap(),
        journal
    );
    assert!(!fixture.root.join("run/capture-exec/commands").exists());
    assert_eq!(
        fs::read(fixture.root.join("data/.rollback-reserve")).unwrap(),
        b"preserved"
    );
    owner.close().unwrap();
    assert!(!fixture.root.join("data/capture-journal.json").exists());
}
#[test]
fn failed_startup_withdrawal_retains_owner_and_journal_for_same_owner_retry() {
    let fixture = Fixture::new();
    fixture.journal();
    let desired = fs::read(fixture.root.join("data/capture-desired.json")).unwrap();
    let mut owner = fixture.owner();
    fs::write(fixture.root.join("run/capture-exec/mode"), b"fail").unwrap();
    assert!(owner.initialize().is_err());
    assert!(fixture.root.join("data/capture-journal.json").exists());
    assert!(owner.capture_unobserved().unwrap().intent.cleanup_pending);
    assert!(status(&mut owner)["services"][0].get("pid").is_none());
    fs::write(fixture.root.join("run/capture-exec/mode"), b"success").unwrap();
    assert!(owner.initialize().unwrap().is_empty());
    assert!(!fixture.root.join("data/capture-journal.json").exists());
    assert_eq!(
        fs::read(fixture.root.join("data/capture-desired.json")).unwrap(),
        desired
    );
    owner.close().unwrap();
    owner.close().unwrap();
}
#[test]
fn private_root_admission_refuses_public_symlink_and_overlapping_roots_without_repair() {
    for unsafe_kind in 0..3 {
        let fixture = Fixture::new();
        let mut options = fixture.options();
        match unsafe_kind {
            0 => fs::set_permissions(&options.run_dir, fs::Permissions::from_mode(0o755)).unwrap(),
            1 => {
                fs::rename(&options.run_dir, fixture.root.join("original-run")).unwrap();
                std::os::unix::fs::symlink("original-run", &options.run_dir).unwrap();
            }
            _ => options.run_dir = options.data_dir.join("volatile"),
        };
        assert!(
            NativeOwner::open(
                options,
                fixture.binaries(),
                table_names(b"").unwrap(),
                SourcePolicy::native("127.0.0.1:53".parse().unwrap()).unwrap(),
                Rc::new(AtomicBool::new(false))
            )
            .is_err()
        );
        if unsafe_kind == 0 {
            assert_eq!(
                fs::metadata(fixture.root.join("run"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o7777,
                0o755
            );
        }
        assert!(!fixture.root.join("run/capture-exec/commands").exists());
    }
}
#[test]
fn close_without_run_attempts_journal_cleanup_and_retains_retry_authority() {
    let fixture = Fixture::new();
    fixture.journal();
    let mut owner = fixture.owner();
    fs::write(fixture.root.join("run/capture-exec/mode"), b"fail").unwrap();
    assert!(owner.close().is_err());
    assert!(owner.initialize().is_err());
    assert!(fixture.root.join("data/capture-journal.json").exists());
    fs::write(fixture.root.join("run/capture-exec/mode"), b"success").unwrap();
    owner.close().unwrap();
    assert!(!fixture.root.join("data/capture-journal.json").exists());
    owner.close().unwrap();
}

fn api_request(
    method: &str,
    target: &str,
    payload: Option<&Value>,
    cookie: &str,
    origin: &str,
) -> Vec<u8> {
    let text = payload.map(Value::to_string).unwrap_or_default();
    format!(
        "{method} {target} HTTP/1.1\r\nHost: localhost\r\nCookie: {cookie}\r\n{}{}\r\n{text}",
        if method == "POST" {
            format!(
                "Content-Type: application/json\r\nContent-Length: {}\r\n",
                text.len()
            )
        } else {
            String::new()
        },
        if origin.is_empty() {
            String::new()
        } else {
            format!("Origin: {origin}\r\n")
        }
    )
    .into_bytes()
}
fn login(service: &Service, owner: &mut NativeOwner) -> String {
    let received = exchange(
        service,
        owner.runtime_mut(),
        api_request(
            "POST",
            "/api/session/login",
            Some(&serde_json::json!({"password":"boot-secret"})),
            "",
            "http://localhost",
        ),
    );
    response(received.clone(), 200);
    std::str::from_utf8(&received)
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("Set-Cookie: "))
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .into()
}
fn call(
    service: &Service,
    owner: &mut NativeOwner,
    method: &str,
    target: &str,
    payload: Option<&Value>,
    cookie: &str,
    expected: u16,
) -> Value {
    response(
        exchange(
            service,
            owner.runtime_mut(),
            api_request(method, target, payload, cookie, "http://localhost"),
        ),
        expected,
    )
}
#[test]
fn authenticated_mutation_cannot_bypass_failed_startup_withdrawal_admission() {
    let fixture = Fixture::new();
    fixture.journal();
    let journal = fs::read(fixture.root.join("data/capture-journal.json")).unwrap();
    let mut owner = fixture.owner();
    let service = Service::new("/proc".into()).with_auth(Auth::new("boot-secret"));
    let cookie = login(&service, &mut owner);
    for (target, payload) in [
        (
            "/api/runtime/start",
            serde_json::json!({"service":"sing-box"}),
        ),
        (
            "/api/runtime/acquire",
            serde_json::json!({"service":"sing-box","artifact":{"url":"https://example.invalid/core","sha256":"ab".repeat(32),"compression":"none","version":"boot"}}),
        ),
        (
            "/api/runtime/configure",
            serde_json::json!({"service":"frpc","generation":0,"config":"serverAddr = \"example.invalid\"\nserverPort = 7000\n"}),
        ),
        (
            "/api/proxy/capture",
            serde_json::json!({"scope":"gateway","ipv6":"direct"}),
        ),
    ] {
        assert_eq!(
            call(
                &service,
                &mut owner,
                "POST",
                target,
                Some(&payload),
                &cookie,
                503
            )["error"]["code"],
            "startup_cleanup_pending"
        );
    }
    assert!(owner.runtime_mut().restore_saved().is_empty());
    assert!(
        owner
            .runtime_mut()
            .poll_recovery(std::time::Instant::now() + Duration::from_secs(600))
            .is_empty()
    );
    assert_eq!(
        fs::read(fixture.root.join("data/capture-journal.json")).unwrap(),
        journal
    );
    assert!(!fixture.root.join("run/capture-exec/commands").exists());
    fs::write(fixture.root.join("run/capture-exec/mode"), b"fail").unwrap();
    assert!(owner.initialize().is_err());
    assert_eq!(
        call(
            &service,
            &mut owner,
            "POST",
            "/api/runtime/start",
            Some(&serde_json::json!({"service":"sing-box"})),
            &cookie,
            503
        )["error"]["code"],
        "startup_cleanup_pending"
    );
    fs::write(fixture.root.join("run/capture-exec/mode"), b"success").unwrap();
    let disabled = call(
        &service,
        &mut owner,
        "DELETE",
        "/api/proxy/capture",
        None,
        &cookie,
        200,
    );
    assert_eq!(disabled["desired"], false);
    assert_eq!(disabled["cleanupPending"], false);
    assert!(owner.initialize().unwrap().is_empty());
    assert_eq!(
        call(
            &service,
            &mut owner,
            "POST",
            "/api/runtime/start",
            Some(&serde_json::json!({"service":"sing-box"})),
            &cookie,
            409
        )["error"]["code"],
        "not_configured"
    );
    owner.close().unwrap();
}
const FRPC_CORE: &str = r#"#!/bin/sh
umask 077
case "$1" in verify) printf 'verify\n' >> "$TMPDIR/events"; exit 0;; esac
trap 'exit 0' TERM
printf '%s\n' "$$" > "$TMPDIR/ready"
printf 'start\n' >> "$TMPDIR/events"
IFS= read -r value < "$TMPDIR/wait"
"#;
#[test]
fn native_owner_rebuilds_saved_on_frpc_after_journal_withdrawal_without_tunnel_claim() {
    let fixture = Fixture::new();
    fixture.journal();
    let mut owner = fixture.owner();
    assert!(owner.initialize().unwrap().is_empty());
    let wait = fixture.root.join("run/frpc/wait");
    let fifo = std::ffi::CString::new(wait.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let data = fixture.root.join("data");
    let peer = thread::spawn(move || {
        for _ in 0..2 {
            let mut stream = listener.accept().unwrap().0;
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut query = [0u8; 2048];
            let _ = stream.read(&mut query).unwrap();
            assert!(
                !data.join("capture-journal.json").exists(),
                "exclusive startup cleanup must precede source fetch"
            );
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                FRPC_CORE.len()
            );
            stream.write_all(header.as_bytes()).unwrap();
            stream.write_all(FRPC_CORE.as_bytes()).unwrap();
        }
    });
    let service = Service::new("/proc".into()).with_auth(Auth::new("boot-secret"));
    let cookie = login(&service, &mut owner);
    let acquired = call(
        &service,
        &mut owner,
        "POST",
        "/api/runtime/acquire",
        Some(
            &serde_json::json!({"service":"frpc","artifact":{"url":format!("http://{address}/frpc"),"sha256":format!("{:x}",Sha256::digest(FRPC_CORE.as_bytes())),"compression":"none","version":"fake-frpc"}}),
        ),
        &cookie,
        200,
    );
    assert_eq!(acquired["configured"], false);
    assert_eq!(acquired["desired"], false);
    call(
        &service,
        &mut owner,
        "POST",
        "/api/runtime/configure",
        Some(
            &serde_json::json!({"service":"frpc","generation":0,"config":"serverAddr = \"example.invalid\"\nserverPort = 7000\n"}),
        ),
        &cookie,
        200,
    );
    let running = call(
        &service,
        &mut owner,
        "POST",
        "/api/runtime/start",
        Some(&serde_json::json!({"service":"frpc"})),
        &cookie,
        200,
    );
    assert_eq!(running["ready"], true);
    let first_pid = running["pid"].as_u64().unwrap();
    owner.close().unwrap();
    assert_eq!(unsafe { libc::kill(first_pid as i32, 0) }, -1);
    assert_eq!(
        fs::read_dir(fixture.root.join("run/frpc"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry
                .file_name()
                .as_encoded_bytes()
                .starts_with(b".artifact-"))
            .count(),
        0
    );
    drop(owner);
    fixture.journal();
    let mut rebuilt = fixture.owner();
    let before = fs::read(fixture.root.join("run/capture-exec/commands")).unwrap();
    let outcomes = rebuilt.initialize().unwrap();
    assert_eq!(outcomes.len(), 1);
    let state = outcomes[0].1.as_ref().unwrap();
    assert_eq!(
        state.service,
        be6500_panel::runtime_manager::ServiceId::Frpc
    );
    assert!(state.ready && state.active && state.desired);
    assert_eq!(state.generation, 1);
    assert_ne!(state.pid.map(u64::from), Some(first_pid));
    peer.join().unwrap();
    let after = fs::read(fixture.root.join("run/capture-exec/commands")).unwrap();
    assert!(after.len() > before.len());
    let repeated = rebuilt.initialize().unwrap();
    assert!(repeated.is_empty());
    assert_eq!(
        fs::read(fixture.root.join("run/capture-exec/commands")).unwrap(),
        after
    );
    assert!(
        call(
            &service,
            &mut rebuilt,
            "GET",
            "/api/runtime",
            None,
            &cookie,
            200
        )["services"][1]
            .get("tunnelConnected")
            .is_none()
    );
    rebuilt.close().unwrap();
}
#[test]
fn cancellation_and_malformed_saved_intent_refuse_before_network_cleanup_and_release_lock() {
    let fixture = Fixture::new();
    fixture.journal();
    let cancel = Rc::new(AtomicBool::new(true));
    let mut owner = NativeOwner::open(
        fixture.options(),
        fixture.binaries(),
        table_names(b"").unwrap(),
        SourcePolicy::native("127.0.0.1:53".parse().unwrap()).unwrap(),
        cancel.clone(),
    )
    .unwrap();
    assert_eq!(
        owner.initialize(),
        Err(be6500_panel::native_owner::InitError::Cancelled)
    );
    assert!(!fixture.root.join("run/capture-exec/commands").exists());
    cancel.store(false, Ordering::Release);
    owner.close().unwrap();
    drop(owner);
    let bad = fixture.root.join("data/desired-services.json");
    fs::write(&bad, b"{\"sing-box\":true,\"sing-box\":false}").unwrap();
    fs::set_permissions(&bad, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(
        NativeOwner::open(
            fixture.options(),
            fixture.binaries(),
            table_names(b"").unwrap(),
            SourcePolicy::native("127.0.0.1:53".parse().unwrap()).unwrap(),
            Rc::new(AtomicBool::new(false))
        )
        .is_err()
    );
    assert_eq!(
        fs::read(&bad).unwrap(),
        b"{\"sing-box\":true,\"sing-box\":false}"
    );
    fs::remove_file(bad).unwrap();
    let mut next = fixture.owner();
    next.close().unwrap();
}

#[test]
fn repeated_initialize_does_not_refetch_or_reset_failed_saved_service_recovery() {
    let fixture = Fixture::new();
    fixture.journal();
    let service_dir = fixture.root.join("data/services/frpc");
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&service_dir)
        .unwrap();
    let config = b"serverAddr = \"example.invalid\"\nserverPort = 7000\n";
    fs::write(service_dir.join("config-1.toml"), config).unwrap();
    fs::set_permissions(
        service_dir.join("config-1.toml"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let requests = std::sync::Arc::new(AtomicU64::new(0));
    let count = requests.clone();
    let stopped = std::sync::Arc::new(AtomicBool::new(false));
    let halt = stopped.clone();
    struct Stop(std::sync::Arc<AtomicBool>);
    impl Drop for Stop {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }
    let stop_guard = Stop(stopped.clone());
    let peer = thread::spawn(move || {
        while !halt.load(Ordering::Acquire) {
            if let Ok((mut stream, _)) = listener.accept() {
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = [0u8; 2048];
                let _ = stream.read(&mut bytes).unwrap();
                count.fetch_add(1, Ordering::Relaxed);
                let _ = stream.write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
            }
            thread::park_timeout(Duration::from_millis(1));
        }
    });
    let state = serde_json::json!({"generation":1,"current":{"generation":1,"file":"config-1.toml","sha256":format!("{:x}",Sha256::digest(config))},"artifact":{"url":format!("http://{address}/frpc"),"sha256":"ab".repeat(32),"compression":"none","version":"saved"}});
    fs::write(service_dir.join("state.json"), state.to_string()).unwrap();
    fs::set_permissions(
        service_dir.join("state.json"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let desired = fixture.root.join("data/desired-services.json");
    fs::write(&desired, b"{\"sing-box\":false,\"frpc\":true}").unwrap();
    fs::set_permissions(&desired, fs::Permissions::from_mode(0o600)).unwrap();
    let mut owner = fixture.owner();
    let first = owner.initialize().unwrap();
    assert_eq!(first.len(), 1);
    assert!(first[0].1.is_err());
    assert_eq!(requests.load(Ordering::Relaxed), 1);
    let after_first = status(&mut owner);
    assert_eq!(after_first["services"][1]["recoveryAttempts"], 1);
    let second = owner.initialize().unwrap();
    let after_second = status(&mut owner);
    let count = requests.load(Ordering::Relaxed);
    owner.close().unwrap();
    drop(stop_guard);
    peer.join().unwrap();
    assert!(
        second.is_empty(),
        "initial startup restore dispatch must run only once"
    );
    assert_eq!(count, 1);
    assert_eq!(after_second["services"][1]["recoveryAttempts"], 1);
}

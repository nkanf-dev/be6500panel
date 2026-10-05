#![cfg(any(target_os = "linux", target_os = "macos"))]
use be6500_panel::{
    auth::Auth,
    capture_executor::{Binaries, TrustedBinary},
    capture_kernel::table_names,
    capture_plan::RulesPlanInput,
    capture_runtime::CaptureRuntime,
    capture_state::{Controller, Desired},
    native::Ports,
    native_runtime::NativeReadiness,
    readiness_tun::{
        Budget, FileIdentity, Interface, InterfaceAddress, Ipv4Prefix, Observer, TunError,
    },
    runtime_http::RuntimeHttp,
    runtime_manager::{
        ArtifactBinding, ArtifactBindings, ArtifactProvenance, Limits, Manager, ServiceId,
    },
    runtime_process::Limits as ProcessLimits,
    server::Service,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    cell::Cell,
    fs,
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream, UdpSocket},
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::Duration,
};
static SERIAL: Mutex<()> = Mutex::new(());
static NEXT: AtomicU64 = AtomicU64::new(0);
const CORE: &str = r#"#!/bin/sh
umask 077
case "$1" in check|verify) exit 0;; esac
trap 'exit 0' TERM
printf '%s\n' "$$" > "$TMPDIR/ready"
IFS= read -r value < "$TMPDIR/wait"
"#;
fn proof_input() -> RulesPlanInput {
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
        management_ips: vec!["192.168.50.1".into()],
        ..RulesPlanInput::default()
    }
}
struct CurrentObserver {
    root: PathBuf,
    artifact: PathBuf,
    identity_fault: Rc<Cell<bool>>,
}
impl CurrentObserver {
    fn pid_until(&self, budget: &Budget<'_>) -> Result<u32, TunError> {
        loop {
            budget.check()?;
            if let Ok(pid) = self.pid() {
                return Ok(pid);
            }
            thread::yield_now();
        }
    }
    fn pid(&self) -> Result<u32, TunError> {
        fs::read_to_string(self.root.join("run/sing-box/ready"))
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .ok_or(TunError::Unavailable)
    }
}
impl Observer for CurrentObserver {
    fn read_file(&mut self, path: &Path, _: usize, b: &Budget<'_>) -> Result<Vec<u8>, TunError> {
        b.check()?;
        let pid = self.pid_until(b)?;
        if path == PathBuf::from(format!("/proc/{pid}/stat")) {
            return Ok(format!(
                "{pid} (fake current core) {} {} 100\n",
                if self.identity_fault.get() { "Z" } else { "S" },
                ["0"; 18].join(" ")
            )
            .into_bytes());
        }
        if path == Path::new("/proc/sys/net/ipv4/conf/b6p-test/rp_filter") {
            return Ok(b"2\n".to_vec());
        }
        if path == PathBuf::from(format!("/proc/{pid}/fdinfo/7")) {
            return Ok(b"iff:\tb6p-test\n".to_vec());
        }
        if path == Path::new("/proc/net/tcp") {
            let ip = u32::from_ne_bytes([172, 30, 0, 1]);
            return Ok(format!("sl local_address rem_address st tx_queue rx_queue tr tm->when retrnsmt uid timeout inode\n0: {ip:08X}:88B9 00000000:0000 0A 00000000:00000000 00:00000000 00000000 0 0 111 1\n").into_bytes());
        }
        Err(TunError::Unavailable)
    }
    fn read_link(&mut self, path: &Path, b: &Budget<'_>) -> Result<PathBuf, TunError> {
        b.check()?;
        let pid = self.pid_until(b)?;
        if path == PathBuf::from(format!("/proc/{pid}/exe")) {
            return Ok(self.artifact.clone());
        }
        if path == PathBuf::from(format!("/proc/{pid}/fd/7")) {
            return Ok("/dev/net/tun".into());
        }
        if path == PathBuf::from(format!("/proc/{pid}/fd/8")) {
            return Ok("socket:[111]".into());
        }
        Err(TunError::Unavailable)
    }
    fn metadata(&mut self, path: &Path, _: bool, b: &Budget<'_>) -> Result<FileIdentity, TunError> {
        b.check()?;
        let pid = self.pid_until(b)?;
        let real = if path == self.artifact || path == PathBuf::from(format!("/proc/{pid}/exe")) {
            &self.artifact
        } else if path == self.artifact.parent().unwrap() {
            self.artifact.parent().unwrap()
        } else {
            return Err(TunError::Unavailable);
        };
        fs::metadata(real)
            .map(|m| FileIdentity::from_metadata(&m))
            .map_err(|_| TunError::Unavailable)
    }
    fn list_dir(&mut self, path: &Path, _: usize, b: &Budget<'_>) -> Result<Vec<String>, TunError> {
        b.check()?;
        if path == PathBuf::from(format!("/proc/{}/fd", self.pid_until(b)?)) {
            Ok(vec!["7".into(), "8".into()])
        } else {
            Err(TunError::Unavailable)
        }
    }
    fn interfaces(&mut self, b: &Budget<'_>) -> Result<Vec<Interface>, TunError> {
        b.check()?;
        let mut out = vec![Interface {
            name: "br-lan".into(),
            up: true,
            mtu: 1500,
            addresses: vec![InterfaceAddress {
                address: "192.168.50.1".parse().unwrap(),
                bits: 24,
            }],
        }];
        if self.pid().is_ok() {
            out.push(Interface {
                name: "b6p-test".into(),
                up: true,
                mtu: 1500,
                addresses: vec![InterfaceAddress {
                    address: "172.30.0.1".parse().unwrap(),
                    bits: 30,
                }],
            })
        }
        Ok(out)
    }
    fn ipv4_routes(&mut self, b: &Budget<'_>) -> Result<Vec<Ipv4Prefix>, TunError> {
        b.check()?;
        Ok(vec![])
    }
}
struct Fixture {
    root: PathBuf,
    stop: Arc<AtomicBool>,
    peers: Vec<thread::JoinHandle<()>>,
    config: Vec<u8>,
    input: RulesPlanInput,
}
impl Fixture {
    fn new() -> Self {
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "b6p-capture-http-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        for part in ["capture", "artifacts", "exec", "run", "run/sing-box"] {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(root.join(part))
                .unwrap();
        }
        let core = root.join("artifacts/.artifact-core");
        fs::write(&core, CORE).unwrap();
        fs::set_permissions(&core, fs::Permissions::from_mode(0o700)).unwrap();
        let fifo = std::ffi::CString::new(
            root.join("run/sing-box/wait")
                .as_os_str()
                .as_encoded_bytes(),
        )
        .unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        let mixed = TcpListener::bind("127.0.0.1:0").unwrap();
        let mixed_port = mixed.local_addr().unwrap().port();
        mixed.set_nonblocking(true).unwrap();
        let dns_tcp = TcpListener::bind("127.0.0.1:0").unwrap();
        let dns_port = dns_tcp.local_addr().unwrap().port();
        dns_tcp.set_nonblocking(true).unwrap();
        let dns_udp = UdpSocket::bind(("127.0.0.1", dns_port)).unwrap();
        dns_udp.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let halt = stop.clone();
        let tcp_peer = thread::spawn(move || {
            while !halt.load(Ordering::Acquire) {
                for (listener, dns) in [(&mixed, false), (&dns_tcp, true)] {
                    if let Ok((mut stream, _)) = listener.accept() {
                        stream
                            .set_read_timeout(Some(Duration::from_millis(200)))
                            .unwrap();
                        stream
                            .set_write_timeout(Some(Duration::from_millis(200)))
                            .unwrap();
                        if dns {
                            let mut len = [0u8; 2];
                            if stream.read_exact(&mut len).is_ok() {
                                let mut query = vec![0; u16::from_be_bytes(len) as usize];
                                if query.len() <= 4096 && stream.read_exact(&mut query).is_ok() {
                                    let answer = dns_answer(&query);
                                    let _ = stream.write_all(&(answer.len() as u16).to_be_bytes());
                                    let _ = stream.write_all(&answer);
                                }
                            }
                        }
                    }
                }
                thread::park_timeout(Duration::from_millis(1));
            }
        });
        let halt = stop.clone();
        let udp_peer = thread::spawn(move || {
            let mut buffer = [0u8; 4096];
            while !halt.load(Ordering::Acquire) {
                if let Ok((n, peer)) = dns_udp.recv_from(&mut buffer) {
                    let _ = dns_udp.send_to(&dns_answer(&buffer[..n]), peer);
                }
                thread::park_timeout(Duration::from_millis(1));
            }
        });
        let mut input = proof_input();
        input.ports.mixed = mixed_port;
        input.ports.dns = dns_port;
        let command = root.join("artifacts/proof-command");
        fs::write(&command, kernel_proof_command(&input)).unwrap();
        fs::set_permissions(&command, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(root.join("exec/mode"), b"preflight").unwrap();
        let config=serde_json::to_vec(&json!({"inbounds":[{"type":"mixed","tag":"mixed-in","listen":"127.0.0.1","listen_port":mixed_port},{"type":"tun","tag":"tun-in","interface_name":"b6p-test","address":["172.30.0.1/30"],"mtu":1500,"stack":"system","dns_mode":"disabled","auto_route":false,"auto_redirect":false,"udp_timeout":"2m","udp_nat_max":1024},{"type":"direct","tag":"dns-in","listen":"0.0.0.0","listen_port":dns_port}],"route":{"rules":[{"inbound":["dns-in"],"action":"hijack-dns"},{"ip_version":6,"outbound":"direct"}]},"dns":{"rules":[{"server":"dns-direct","domain":["bootstrap.test"]}]}})).unwrap();
        Self {
            root,
            stop,
            peers: vec![tcp_peer, udp_peer],
            config,
            input,
        }
    }
    fn runtime(&self) -> Guard {
        self.runtime_with_input(self.input.clone())
    }
    fn runtime_with_input(&self, input: RulesPlanInput) -> Guard {
        fs::write(
            self.root.join("artifacts/proof-command"),
            kernel_proof_command(&input),
        )
        .unwrap();
        let command = self.root.join("artifacts/proof-command");
        let bytes = fs::read(&command).unwrap();
        let sha = Sha256::digest(&bytes).into();
        let current_input = input.clone();
        let selected_input = input;
        let identity_fault = Rc::new(Cell::new(false));
        let native_fault = identity_fault.clone();
        let build_count = Rc::new(Cell::new(0));
        let builds = build_count.clone();
        let trip_build = Rc::new(Cell::new(0));
        let trip = trip_build.clone();
        let build_fault = identity_fault.clone();
        let selected = Rc::new(Cell::new(0));
        let count = selected.clone();
        let capture = CaptureRuntime::new(
            Controller::open(self.root.join("capture")).unwrap(),
            Binaries {
                ip: TrustedBinary::admit(&command, sha).unwrap(),
                iptables: TrustedBinary::admit(&command, sha).unwrap(),
            },
            self.root.join("exec"),
            table_names(b"").unwrap(),
            move |_, _, _| {
                builds.set(builds.get() + 1);
                if trip.get() != 0 && builds.get() == trip.get() {
                    build_fault.set(true);
                }
                Ok(current_input.clone())
            },
        )
        .with_selection_observer(move |selection, _| {
            count.set(count.get() + 1);
            if selected_input.scope == "gateway" {
                assert_eq!(selection.scope, "gateway");
                Ok(Desired {
                    scope: "gateway".into(),
                    lan_ipv4_prefixes: selected_input.lan_ipv4_prefixes.clone(),
                    desired: true,
                    ..Desired::default()
                })
            } else {
                let mac = "02:aa:bb:cc:dd:01";
                if !selection.client_ipv4.is_empty()
                    && selection.client_ipv4 != selected_input.client_ipv4
                {
                    return Err(be6500_panel::runtime_manager::HookError::Failed);
                }
                if !selection.devices.is_empty()
                    && (selection.devices.len() != 1 || selection.devices[0].mac != mac)
                {
                    return Err(be6500_panel::runtime_manager::HookError::Failed);
                }
                Ok(Desired {
                    scope: "devices".into(),
                    devices: vec![be6500_panel::capture_state::DeviceSelection { mac: mac.into() }],
                    desired: true,
                    ..Desired::default()
                })
            }
        });
        let native = NativeReadiness::with_observer(
            CurrentObserver {
                root: self.root.clone(),
                artifact: self.root.join("artifacts/.artifact-core"),
                identity_fault: native_fault,
            },
            Rc::new(AtomicBool::new(false)),
        );
        let (hooks, handle) = capture.into_hooks_with_handle(native);
        let manager = Manager::open(
            self.root.join("services"),
            self.root.join("run"),
            ArtifactBindings {
                sing_box: Some(ArtifactBinding::trusted_local(
                    ServiceId::SingBox,
                    self.root.join("artifacts"),
                    self.root.join("artifacts/.artifact-core"),
                    Sha256::digest(CORE.as_bytes()).into(),
                    ArtifactProvenance::TrustedLocalModule,
                )),
                frpc: None,
            },
            hooks,
            Limits {
                process: ProcessLimits {
                    term_grace: Duration::from_millis(100),
                    kill_grace: Duration::from_secs(1),
                    check_timeout: Duration::from_secs(2),
                },
                readiness_timeout: Duration::from_secs(2),
                resource_timeout: Duration::from_secs(5),
            },
        )
        .unwrap();
        let mut runtime = RuntimeHttp::new(manager);
        runtime.load_capture(handle).unwrap();
        Guard {
            runtime,
            root: self.root.clone(),
            selected,
            identity_fault,
            build_count,
            trip_build,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        for peer in self.peers.drain(..) {
            peer.join().unwrap();
        }
        fs::remove_dir_all(&self.root).unwrap();
    }
}
struct Guard {
    runtime: RuntimeHttp,
    root: PathBuf,
    selected: Rc<Cell<u32>>,
    identity_fault: Rc<Cell<bool>>,
    build_count: Rc<Cell<u32>>,
    trip_build: Rc<Cell<u32>>,
}
impl Drop for Guard {
    fn drop(&mut self) {
        self.identity_fault.set(false);
        fs::write(self.root.join("exec/mode"), b"success").unwrap();
        self.runtime.close().unwrap();
    }
}
fn dns_answer(query: &[u8]) -> Vec<u8> {
    let mut reply = query.to_vec();
    reply[2..4].copy_from_slice(&0x8180u16.to_be_bytes());
    reply[6..8].copy_from_slice(&1u16.to_be_bytes());
    reply.extend_from_slice(&[0xc0, 12, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 192, 0, 2, 1]);
    reply
}
fn request(
    method: &str,
    target: &str,
    payload: Option<&Value>,
    cookie: &str,
    origin: &str,
) -> Vec<u8> {
    let text = payload.map(|value| value.to_string()).unwrap_or_default();
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
fn exchange(service: &Service, runtime: Option<&mut RuntimeHttp>, request: Vec<u8>) -> Vec<u8> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let client = thread::spawn(move || {
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        stream.write_all(&request).unwrap();
        stream.shutdown(Shutdown::Write).unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).unwrap();
        response
    });
    let stream = listener.accept().unwrap().0;
    if let Some(runtime) = runtime {
        service.handle_with_runtime(stream, runtime).unwrap();
    } else {
        service.handle(stream).unwrap();
    }
    client.join().unwrap()
}
fn body(response: Vec<u8>, expected: u16) -> Value {
    assert!(
        response.starts_with(format!("HTTP/1.1 {expected} ").as_bytes()),
        "{}",
        String::from_utf8_lossy(&response)
    );
    let end = response.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    serde_json::from_slice(&response[end..]).unwrap()
}
fn login(service: &Service) -> String {
    let response = exchange(
        service,
        None,
        request(
            "POST",
            "/api/session/login",
            Some(&json!({"password":"isolated-secret"})),
            "",
            "http://localhost",
        ),
    );
    body(response.clone(), 200);
    std::str::from_utf8(&response)
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
    runtime: &mut RuntimeHttp,
    method: &str,
    target: &str,
    payload: Option<&Value>,
    cookie: &str,
    expected: u16,
) -> Value {
    body(
        exchange(
            service,
            Some(runtime),
            request(method, target, payload, cookie, "http://localhost"),
        ),
        expected,
    )
}
#[test]
fn authenticated_gateway_capture_observes_current_and_delete_latches_off_without_core_restart() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut owner = fixture.runtime();
    let service = Service::new(fixture.root.clone()).with_auth(Auth::new("isolated-secret"));
    let cookie = login(&service);
    assert!(!fixture.root.join("exec/commands").exists());
    let off = call(
        &service,
        &mut owner.runtime,
        "GET",
        "/api/proxy/capture",
        None,
        &cookie,
        200,
    );
    assert_eq!(off["active"], false);
    assert_eq!(off["desired"], false);
    assert!(!fixture.root.join("exec/commands").exists());
    call(
        &service,
        &mut owner.runtime,
        "POST",
        "/api/runtime/configure",
        Some(
            &json!({"service":"sing-box","config":String::from_utf8(fixture.config.clone()).unwrap(),"generation":0}),
        ),
        &cookie,
        200,
    );
    let started = call(
        &service,
        &mut owner.runtime,
        "POST",
        "/api/runtime/start",
        Some(&json!({"service":"sing-box"})),
        &cookie,
        200,
    );
    let pid = started["pid"].as_u64().unwrap();
    let apply = json!({"scope":"gateway","ipv6":"direct"});
    body(
        exchange(
            &service,
            Some(&mut owner.runtime),
            request(
                "POST",
                "/api/proxy/capture",
                Some(&apply),
                "",
                "http://localhost",
            ),
        ),
        401,
    );
    body(
        exchange(
            &service,
            Some(&mut owner.runtime),
            request(
                "DELETE",
                "/api/proxy/capture",
                None,
                &cookie,
                "http://foreign.test",
            ),
        ),
        403,
    );
    assert_eq!(owner.selected.get(), 0);
    assert!(!fixture.root.join("capture/capture-desired.json").exists());
    let active = call(
        &service,
        &mut owner.runtime,
        "POST",
        "/api/proxy/capture",
        Some(&apply),
        &cookie,
        200,
    );
    assert_eq!(active["active"], true);
    assert_eq!(active["desired"], true);
    assert_eq!(active["state"], "active");
    assert_eq!(active["lanIPv4Prefixes"], json!(["192.168.50.0/24"]));
    assert!(active.get("lanIpv4Prefixes").is_none());
    assert_eq!(
        active["installedLanIPv4Prefixes"],
        active["lanIPv4Prefixes"]
    );
    assert_eq!(owner.selected.get(), 1);
    let desired = fs::read(fixture.root.join("capture/capture-desired.json")).unwrap();
    let journal = fs::read(fixture.root.join("capture/capture-journal.json")).unwrap();
    let commands = fs::read(fixture.root.join("exec/commands")).unwrap();
    let head = exchange(
        &service,
        Some(&mut owner.runtime),
        request("HEAD", "/api/proxy/capture", None, &cookie, ""),
    );
    assert!(head.ends_with(b"\r\n\r\n"));
    assert_eq!(
        fs::read(fixture.root.join("exec/commands")).unwrap(),
        commands
    );
    let active = call(
        &service,
        &mut owner.runtime,
        "GET",
        "/api/proxy/capture",
        None,
        &cookie,
        200,
    );
    assert_eq!(active["active"], true);
    assert_eq!(
        fs::read(fixture.root.join("capture/capture-desired.json")).unwrap(),
        desired
    );
    assert_eq!(
        fs::read(fixture.root.join("capture/capture-journal.json")).unwrap(),
        journal
    );
    fs::write(fixture.root.join("exec/mode"), b"fail").unwrap();
    let unknown = call(
        &service,
        &mut owner.runtime,
        "GET",
        "/api/proxy/capture",
        None,
        &cookie,
        200,
    );
    assert_eq!(unknown["active"], false);
    assert_eq!(unknown["state"], "unknown");
    assert_eq!(unknown["cleanupPending"], false);
    assert_eq!(
        fs::read(fixture.root.join("capture/capture-journal.json")).unwrap(),
        journal
    );
    let failed = call(
        &service,
        &mut owner.runtime,
        "DELETE",
        "/api/proxy/capture",
        None,
        &cookie,
        500,
    );
    assert_eq!(failed["error"]["code"], "cleanup_failed");
    assert_eq!(failed["capture"]["desired"], false);
    assert_eq!(failed["capture"]["cleanupPending"], true);
    assert!(fixture.root.join("capture/capture-journal.json").exists());
    assert_eq!(
        call(
            &service,
            &mut owner.runtime,
            "GET",
            "/api/runtime",
            None,
            &cookie,
            200
        )["services"][0]["pid"],
        pid
    );
    fs::write(fixture.root.join("exec/mode"), b"success").unwrap();
    let inactive = call(
        &service,
        &mut owner.runtime,
        "DELETE",
        "/api/proxy/capture",
        None,
        &cookie,
        200,
    );
    assert_eq!(inactive["desired"], false);
    assert_eq!(inactive["active"], false);
    assert_eq!(inactive["cleanupPending"], false);
    assert_eq!(inactive["state"], "inactive");
    assert!(!fixture.root.join("capture/capture-journal.json").exists());
    assert_eq!(
        call(
            &service,
            &mut owner.runtime,
            "GET",
            "/api/runtime",
            None,
            &cookie,
            200
        )["services"][0]["pid"],
        pid
    );
}

fn quote_shell(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
fn kernel_proof_command(input: &RulesPlanInput) -> String {
    let plan = be6500_panel::capture_plan::plan_owned_rules(input).unwrap();
    let mut source = String::from(
        "#!/bin/sh\numask 077\nprintf '%s\\n' \"$*\" >> \"$TMPDIR/commands\"\nIFS= read -r mode < \"$TMPDIR/mode\" || :\ncase \"$mode\" in fail) exit 8;; esac\n",
    );
    let cleanup_last = plan.cleanup.last().unwrap()[1..].join(" ");
    source.push_str(&format!(
        "case \"$*\" in {}) printf 'preflight\\n' > \"$TMPDIR/mode\"; exit 0;; esac\n",
        quote_shell(&cleanup_last)
    ));
    let last = plan.apply.last().unwrap()[1..].join(" ");
    source.push_str(&format!(
        "case \"$*\" in {}) printf 'installed\\n' > \"$TMPDIR/mode\"; exit 0;; esac\n",
        quote_shell(&last)
    ));
    source.push_str("if [ \"$mode\" = preflight ]; then\ncase \"$*\" in\n");
    source.push_str("'-4 route show table all') printf '%s\\n' '172.30.0.0/30 dev b6p-test proto kernel scope link src 172.30.0.1' 'local 172.30.0.1 dev b6p-test table local proto kernel scope host'; exit 0;;\n'-4 route show table 16500') printf '%s\\n' 'Error: ipv4: FIB table does not exist.'; exit 1;;\n'-4 rule show') printf '%s\\n' '0: from all lookup local' '32766: from all lookup main'; exit 0;;\n'-w 5 -t mangle -S') printf '%s\\n' '-P PREROUTING ACCEPT'; exit 0;;\n");
    for chain in &plan.ownership.chains {
        source.push_str(&format!(
            "{}) printf '%s\\n' 'iptables: No chain/target/match by that name.'; exit 1;;\n",
            quote_shell(&format!("-w 5 -t {} -S {}", chain.table, chain.name))
        ));
    }
    source.push_str("esac\nfi\nif [ \"$mode\" = installed ]; then\ncase \"$*\" in\n'-4 route show table 16500') printf '%s\\n' 'default dev b6p-test proto static scope link'; exit 0;;\n");
    let mut rules = vec!["0: from all lookup local".to_owned()];
    for argv in &plan.apply {
        if argv.len() > 3 && argv[0] == "ip" && argv[2] == "rule" && argv[3] == "add" {
            let source_index = argv.iter().position(|v| v == "from").unwrap() + 1;
            rules.push(format!(
                "16500: from {} iif br-lan fwmark 0x4000/0x4000 lookup 16500",
                argv[source_index]
            ));
        }
    }
    rules.push("32766: from all lookup main".into());
    source.push_str(&format!(
        "'-4 rule show') printf '%s\\n' {}; exit 0;;\n",
        rules
            .iter()
            .map(|row| quote_shell(row))
            .collect::<Vec<_>>()
            .join(" ")
    ));
    let mut seen = std::collections::BTreeSet::new();
    for chain in &plan.ownership.chains {
        let mut rows = vec![format!("-N {}", chain.name)];
        for argv in &plan.apply {
            if argv.len() > 6
                && argv[0] == "iptables"
                && argv[4] == chain.table
                && argv[5] == "-A"
                && argv[6] == chain.name
            {
                rows.push(argv[5..].join(" "));
            }
        }
        source.push_str(&format!(
            "{}) printf '%s\\n' {}; exit 0;;\n",
            quote_shell(&format!("-w 5 -t {} -S {}", chain.table, chain.name)),
            rows.iter()
                .map(|row| quote_shell(row))
                .collect::<Vec<_>>()
                .join(" ")
        ));
        if seen.insert((chain.table.clone(), chain.hook.clone())) {
            let mut rows = vec![format!("-P {} ACCEPT", chain.hook)];
            for argv in plan.apply.iter().rev() {
                if argv.len() > 8
                    && argv[0] == "iptables"
                    && argv[4] == chain.table
                    && argv[5] == "-I"
                    && argv[6] == chain.hook
                {
                    let mut row = argv[5..].to_vec();
                    row[0] = "-A".into();
                    row.remove(2);
                    rows.push(row.join(" "));
                }
            }
            source.push_str(&format!(
                "{}) printf '%s\\n' {}; exit 0;;\n",
                quote_shell(&format!("-w 5 -t {} -S {}", chain.table, chain.hook)),
                rows.iter()
                    .map(|row| quote_shell(row))
                    .collect::<Vec<_>>()
                    .join(" ")
            ));
        }
    }
    source.push_str("esac\nfi\nexit 0\n");
    source
}

#[test]
fn capture_auth_methods_invalid_input_and_unready_owner_have_zero_write_or_command() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut owner = fixture.runtime();
    let service = Service::new(fixture.root.clone()).with_auth(Auth::new("isolated-secret"));
    let cookie = login(&service);
    for method in ["GET", "POST", "DELETE"] {
        let payload = (method == "POST").then(|| json!({"scope":"gateway","ipv6":"direct"}));
        body(
            exchange(
                &service,
                Some(&mut owner.runtime),
                request(
                    method,
                    "/api/proxy/capture",
                    payload.as_ref(),
                    "",
                    "http://localhost",
                ),
            ),
            401,
        );
    }
    call(
        &service,
        &mut owner.runtime,
        "POST",
        "/api/proxy/capture",
        Some(&json!({"scope":"gateway","ipv6":"direct"})),
        &cookie,
        409,
    );
    for invalid in [
        json!({"scope":"gateway"}),
        json!({"scope":"gateway","ipv6":"block"}),
        json!({"scope":"gateway","ipv6":"follow"}),
        json!({"scope":"gateway","ipv6":"direct","lanIPv4Prefixes":["10.0.0.0/8"]}),
        json!({"scope":"gateway","ipv6":"direct","devices":[{"mac":"02:aa:bb:cc:dd:01"}]}),
        json!({"devices":[["02:aa:bb:cc:dd:01"]],"ipv6":"direct"}),
        json!({"devices":null,"ipv6":"direct"}),
        json!({"devices":[],"ipv6":"direct"}),
        json!({"scope":"foreign","ipv6":"direct"}),
    ] {
        assert_eq!(
            call(
                &service,
                &mut owner.runtime,
                "POST",
                "/api/proxy/capture",
                Some(&invalid),
                &cookie,
                400
            )["error"]["code"],
            "invalid_json"
        );
    }
    for method in ["GET", "POST", "DELETE"] {
        let payload = (method == "POST").then(|| json!({"scope":"gateway","ipv6":"direct"}));
        assert_eq!(
            call(
                &service,
                &mut owner.runtime,
                method,
                "/api/proxy/capture?other=1",
                payload.as_ref(),
                &cookie,
                400
            )["error"]["code"],
            "invalid_input"
        );
    }
    let request_bytes=format!("DELETE /api/proxy/capture HTTP/1.1\r\nHost: localhost\r\nCookie: {cookie}\r\nOrigin: http://localhost\r\nContent-Length: 1\r\n\r\nx").into_bytes();
    body(
        exchange(&service, Some(&mut owner.runtime), request_bytes),
        400,
    );
    for target in [
        "/api/session/logout",
        "/api/runtime/stop",
        "/api/health",
        "/index.html",
    ] {
        body(
            exchange(
                &service,
                Some(&mut owner.runtime),
                request("DELETE", target, None, &cookie, "http://localhost"),
            ),
            405,
        );
    }
    let head = exchange(
        &service,
        Some(&mut owner.runtime),
        request("HEAD", "/api/proxy/capture", None, &cookie, ""),
    );
    assert!(head.ends_with(b"\r\n\r\n"));
    assert_eq!(owner.selected.get(), 0);
    assert!(!fixture.root.join("exec/commands").exists());
    assert!(!fixture.root.join("capture/capture-desired.json").exists());
    assert!(!fixture.root.join("capture/capture-journal.json").exists());
}
#[test]
fn capture_delete_persistence_failure_still_cleans_and_keeps_effective_off() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut owner = fixture.runtime();
    let service = Service::new(fixture.root.clone()).with_auth(Auth::new("isolated-secret"));
    let cookie = login(&service);
    call(
        &service,
        &mut owner.runtime,
        "POST",
        "/api/runtime/configure",
        Some(
            &json!({"service":"sing-box","config":String::from_utf8(fixture.config.clone()).unwrap(),"generation":0}),
        ),
        &cookie,
        200,
    );
    let started = call(
        &service,
        &mut owner.runtime,
        "POST",
        "/api/runtime/start",
        Some(&json!({"service":"sing-box"})),
        &cookie,
        200,
    );
    let pid = started["pid"].as_u64().unwrap();
    call(
        &service,
        &mut owner.runtime,
        "POST",
        "/api/proxy/capture",
        Some(&json!({"scope":"gateway","ipv6":"direct"})),
        &cookie,
        200,
    );
    let desired = fixture.root.join("capture/capture-desired.json");
    fs::remove_file(&desired).unwrap();
    std::os::unix::fs::symlink("foreign-desired", &desired).unwrap();
    let failed = call(
        &service,
        &mut owner.runtime,
        "DELETE",
        "/api/proxy/capture",
        None,
        &cookie,
        500,
    );
    assert_eq!(failed["error"]["code"], "capture_disable_not_persisted");
    assert_eq!(failed["capture"]["desired"], false);
    assert_eq!(failed["capture"]["active"], false);
    assert_eq!(failed["capture"]["cleanupPending"], false);
    assert!(!fixture.root.join("capture/capture-journal.json").exists());
    assert!(
        fs::symlink_metadata(&desired)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    let state = call(
        &service,
        &mut owner.runtime,
        "GET",
        "/api/proxy/capture",
        None,
        &cookie,
        200,
    );
    assert_eq!(state["desired"], false);
    assert_eq!(state["error"], "capture_disable_not_persisted");
    assert_eq!(
        call(
            &service,
            &mut owner.runtime,
            "GET",
            "/api/runtime",
            None,
            &cookie,
            200
        )["services"][0]["pid"],
        pid
    );
}
#[test]
fn no_capture_attachment_is_explicitly_unavailable_and_does_not_borrow_observer() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let service = Service::new(fixture.root.clone()).with_auth(Auth::new("isolated-secret"));
    let cookie = login(&service);
    for method in ["GET", "DELETE", "POST"] {
        let payload = (method == "POST").then(|| json!({"scope":"gateway","ipv6":"direct"}));
        assert_eq!(
            body(
                exchange(
                    &service,
                    None,
                    request(
                        method,
                        "/api/proxy/capture",
                        payload.as_ref(),
                        &cookie,
                        "http://localhost"
                    )
                ),
                503
            )["error"]["code"],
            "runtime_unavailable"
        );
    }
    assert!(!fixture.root.join("exec/commands").exists());
    assert!(!fixture.root.join("capture/capture-desired.json").exists());
}

#[test]
fn device_capture_saves_only_current_mac_and_never_reuses_installed_ip_on_unknown_get() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut input = fixture.input.clone();
    input.scope = "devices".into();
    input.lan_ipv4_prefixes.clear();
    input.client_ipv4 = "192.168.50.2".into();
    input.client_macs = Some(std::collections::BTreeMap::from([(
        "192.168.50.2".into(),
        "02:aa:bb:cc:dd:01".into(),
    )]));
    let mut owner = fixture.runtime_with_input(input);
    let service = Service::new(fixture.root.clone()).with_auth(Auth::new("isolated-secret"));
    let cookie = login(&service);
    call(
        &service,
        &mut owner.runtime,
        "POST",
        "/api/runtime/configure",
        Some(
            &json!({"service":"sing-box","config":String::from_utf8(fixture.config.clone()).unwrap(),"generation":0}),
        ),
        &cookie,
        200,
    );
    call(
        &service,
        &mut owner.runtime,
        "POST",
        "/api/runtime/start",
        Some(&json!({"service":"sing-box"})),
        &cookie,
        200,
    );
    call(
        &service,
        &mut owner.runtime,
        "POST",
        "/api/proxy/capture",
        Some(&json!({"devices":[{"mac":"02:aa:bb:cc:dd:ff"}],"ipv6":"direct"})),
        &cookie,
        409,
    );
    assert!(!fixture.root.join("capture/capture-desired.json").exists());
    let active = call(
        &service,
        &mut owner.runtime,
        "POST",
        "/api/proxy/capture",
        Some(&json!({"clientIPv4":"192.168.50.2","ipv6":"direct"})),
        &cookie,
        200,
    );
    assert_eq!(active["active"], true);
    assert_eq!(active["scope"], "devices");
    assert_eq!(
        active["clients"],
        json!([{"mac":"02:aa:bb:cc:dd:01","ip":"192.168.50.2","hostname":""}])
    );
    let desired: Value = serde_json::from_slice(
        &fs::read(fixture.root.join("capture/capture-desired.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(desired["devices"], json!([{"mac":"02:aa:bb:cc:dd:01"}]));
    assert!(desired.get("clientIPv4").is_none());
    fs::write(fixture.root.join("exec/mode"), b"fail").unwrap();
    let unknown = call(
        &service,
        &mut owner.runtime,
        "GET",
        "/api/proxy/capture",
        None,
        &cookie,
        200,
    );
    assert_eq!(unknown["active"], false);
    assert_eq!(unknown["clients"][0]["ip"], "");
    assert_eq!(unknown["installedClients"][0]["ip"], "192.168.50.2");
    fs::write(fixture.root.join("exec/mode"), b"success").unwrap();
    call(
        &service,
        &mut owner.runtime,
        "DELETE",
        "/api/proxy/capture",
        None,
        &cookie,
        200,
    );
}

#[test]
fn selection_identity_loss_during_preparation_refuses_before_cleanup_and_before_new_apply() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    for window in [1, 2] {
        let fixture = Fixture::new();
        let mut owner = fixture.runtime();
        let service = Service::new(fixture.root.clone()).with_auth(Auth::new("isolated-secret"));
        let cookie = login(&service);
        call(
            &service,
            &mut owner.runtime,
            "POST",
            "/api/runtime/configure",
            Some(
                &json!({"service":"sing-box","config":String::from_utf8(fixture.config.clone()).unwrap(),"generation":0}),
            ),
            &cookie,
            200,
        );
        let started = call(
            &service,
            &mut owner.runtime,
            "POST",
            "/api/runtime/start",
            Some(&json!({"service":"sing-box"})),
            &cookie,
            200,
        );
        let pid = started["pid"].as_u64().unwrap();
        let selection = json!({"scope":"gateway","ipv6":"direct"});
        call(
            &service,
            &mut owner.runtime,
            "POST",
            "/api/proxy/capture",
            Some(&selection),
            &cookie,
            200,
        );
        let before = fs::read_to_string(fixture.root.join("exec/commands")).unwrap();
        let journal = fs::read(fixture.root.join("capture/capture-journal.json")).unwrap();
        owner.trip_build.set(owner.build_count.get() + window);
        let refused = call(
            &service,
            &mut owner.runtime,
            "POST",
            "/api/proxy/capture",
            Some(&selection),
            &cookie,
            409,
        );
        assert_eq!(refused["error"]["code"], "capture_failed");
        assert_eq!(refused["capture"]["desired"], true);
        let commands = fs::read_to_string(fixture.root.join("exec/commands")).unwrap();
        let tail = &commands[before.len()..];
        if window == 1 {
            assert!(
                tail.is_empty(),
                "lost identity during prepare must not withdraw old rules: {tail}"
            );
            assert_eq!(
                fs::read(fixture.root.join("capture/capture-journal.json")).unwrap(),
                journal
            );
        } else {
            let expected = be6500_panel::capture_plan::plan_owned_rules(&fixture.input)
                .unwrap()
                .cleanup
                .into_iter()
                .map(|argv| argv[1..].join(" "))
                .collect::<Vec<_>>();
            assert_eq!(
                tail.lines().collect::<Vec<_>>(),
                expected.iter().map(String::as_str).collect::<Vec<_>>(),
                "lost identity after cleanup must not install fresh rules"
            );
            assert!(!fixture.root.join("capture/capture-journal.json").exists());
        }
        assert_eq!(
            call(
                &service,
                &mut owner.runtime,
                "GET",
                "/api/runtime",
                None,
                &cookie,
                200
            )["services"][0]["pid"],
            pid
        );
        owner.identity_fault.set(false);
        owner.trip_build.set(0);
        let current = call(
            &service,
            &mut owner.runtime,
            "GET",
            "/api/proxy/capture",
            None,
            &cookie,
            200,
        );
        assert_eq!(current["active"], window == 1);
        call(
            &service,
            &mut owner.runtime,
            "DELETE",
            "/api/proxy/capture",
            None,
            &cookie,
            200,
        );
    }
}

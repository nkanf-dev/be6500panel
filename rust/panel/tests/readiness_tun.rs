#![cfg(unix)]
use be6500_panel::readiness_tun::*;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

const CONFIG: &str = r#"{"inbounds":[{"type":"mixed","tag":"mixed-in","listen":"127.0.0.1","listen_port":2080},{"type":"tun","tag":"tun-in","interface_name":"b6p-tun","address":["172.31.255.253/30"],"mtu":1500,"stack":"system","dns_mode":"disabled","auto_route":false,"auto_redirect":false,"udp_timeout":"2m","udp_nat_max":1024}],"route":{"rules":[{"ip_version":6,"outbound":"direct"}]}}"#;

#[test]
fn exact_target_and_native_parameters() {
    let target = native_target(CONFIG.as_bytes()).unwrap().unwrap();
    assert_eq!(target.interface_name(), "b6p-tun");
    assert_eq!(target.address().to_string(), "172.31.255.253/30");
    assert_eq!(target.peer().to_string(), "172.31.255.254");
    assert!(
        native_target(
            br#"{"inbounds":[{"type":"mixed","unknown":{"list":[1,2]},"address":"anything"}]}"#
        )
        .unwrap()
        .is_none()
    );
    for (from, to) in [
        (r#"["172.31.255.253/30"]"#, r#""172.31.255.253/30""#),
        (r#""mtu":1500"#, r#""mtu":1500,"MTU":1500"#),
        (r#""mtu":1500"#, r#""mtu":1500,"mtu":1500"#),
        ("172.31.255.253/30", "172.31.255.254/30"),
        (r#""stack":"system""#, r#""stack":"gvisor""#),
    ] {
        assert!(
            native_target(CONFIG.replace(from, to).as_bytes()).is_err(),
            "{from}"
        );
    }
    assert!(native_target(br#"{"inbounds":[{"type":"tproxy"}]}"#).is_err());
}

#[test]
fn route_text_is_all_table_canonical_and_default_ignored() {
    let routes = parse_route_output(b"default via 1.2.3.4 dev wan\nlocal 172.31.255.253 dev tun table 255\nblackhole 10.0.0.0/8 table 100\n").unwrap();
    assert_eq!(routes.len(), 2);
    assert!(parse_route_output(b"172.31.255.253/30 dev tun").is_err());
    assert!(parse_route_output(b"010.0.0.0/8 dev tun").is_err());
    assert!(parse_route_output(b"local").is_err());
}

#[test]
fn early_deadline_and_cancel_do_not_observe() {
    let cancel = AtomicBool::new(true);
    let mut observer = NativeObserver::new();
    assert_eq!(
        check_prestart_once(
            CONFIG.as_bytes(),
            &mut observer,
            Instant::now() + Duration::from_secs(1),
            &cancel
        ),
        Err(TunError::Cancelled)
    );
    assert_eq!(
        check_prestart_once(
            CONFIG.as_bytes(),
            &mut observer,
            Instant::now(),
            &AtomicBool::new(false)
        ),
        Err(TunError::Deadline)
    );
}

use be6500_panel::runtime_process::ServiceId;
use std::fs;
use std::net::Ipv4Addr;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
const PID: u32 = 4321;
const HEADER: &str =
    "  sl local_address rem_address st tx_queue rx_queue tr tm->when retrnsmt uid timeout inode\n";
fn tcp(ip: &str, port: u16, state: &str, inode: u64) -> String {
    let ip = ip.parse::<Ipv4Addr>().unwrap();
    format!(
        "  0: {:08X}:{port:04X} 00000000:0000 {state} 00000000:00000000 00:00000000 00000000 0 0 {inode} 1\n",
        u32::from_ne_bytes(ip.octets())
    )
}
fn stat(start: u64, state: &str) -> String {
    format!(
        "{PID} (core name ) worker) {state} {} {start}\n",
        ["0"; 18].join(" ")
    )
}
fn addr(ip: &str, bits: u8) -> InterfaceAddress {
    InterfaceAddress {
        address: ip.parse().unwrap(),
        bits,
    }
}
struct Fixture {
    base: PathBuf,
    proc: PathBuf,
    artifact: PathBuf,
    native: NativeObserver,
    interfaces: Vec<Interface>,
    routes: Result<Vec<Ipv4Prefix>, TunError>,
    reads: Vec<PathBuf>,
    list_override: Option<Vec<String>>,
    oversized: bool,
}
impl Fixture {
    fn new() -> Self {
        let base = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "b6p-tun-fixture-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::DirBuilder::new().mode(0o700).create(&base).unwrap();
        let root = base.join("artifacts");
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let artifact = root.join(".artifact-fixture");
        fs::write(&artifact, b"private inode fixture").unwrap();
        fs::set_permissions(&artifact, fs::Permissions::from_mode(0o700)).unwrap();
        let proc = base.join("proc");
        for sub in ["4321/fd", "4321/fdinfo", "net", "sys/net/ipv4/conf/b6p-tun"] {
            fs::create_dir_all(proc.join(sub)).unwrap();
        }
        fs::write(proc.join("4321/stat"), stat(123456, "S")).unwrap();
        fs::write(
            proc.join("4321/fdinfo/7"),
            b"pos:\t0\nflags:\t0104002\niff:\tb6p-tun\n",
        )
        .unwrap();
        fs::write(
            proc.join("net/tcp"),
            format!(
                "{HEADER}{}{}",
                tcp("172.31.255.253", 53, "0A", 222),
                tcp("172.31.255.253", 35001, "0A", 111)
            ),
        )
        .unwrap();
        fs::write(proc.join("sys/net/ipv4/conf/b6p-tun/rp_filter"), b"2\n").unwrap();
        symlink(&artifact, proc.join("4321/exe")).unwrap();
        symlink("/dev/net/tun", proc.join("4321/fd/7")).unwrap();
        symlink("socket:[111]", proc.join("4321/fd/8")).unwrap();
        Self {
            base,
            proc: proc.clone(),
            artifact,
            native: NativeObserver::with_proc_root(proc),
            interfaces: vec![
                Interface {
                    name: "lo".into(),
                    up: true,
                    mtu: 65536,
                    addresses: vec![addr("127.0.0.1", 8)],
                },
                Interface {
                    name: "b6p-tun".into(),
                    up: true,
                    mtu: 1500,
                    addresses: vec![addr("172.31.255.253", 30), addr("fe80::1234", 64)],
                },
            ],
            routes: Ok(vec![]),
            reads: vec![],
            list_override: None,
            oversized: false,
        }
    }
    fn owner() -> OwnedStatus {
        OwnedStatus {
            service: ServiceId::SingBox,
            pid: PID,
            generation: 8,
            running: true,
        }
    }
    fn bind(&mut self) -> Result<OwnedIdentity, TunError> {
        OwnedIdentity::bind(
            Self::owner(),
            self.artifact.parent().unwrap().to_path_buf(),
            self.artifact.clone(),
            FileIdentity::from_metadata(&fs::metadata(&self.artifact).unwrap()),
            FileIdentity::from_metadata(&fs::metadata(self.artifact.parent().unwrap()).unwrap()),
            self,
            Instant::now() + Duration::from_secs(1),
            &AtomicBool::new(false),
        )
    }
    fn observe(&mut self, id: &OwnedIdentity) -> Result<OwnedObservation, TunError> {
        observe_owned_once(
            CONFIG.as_bytes(),
            id,
            self,
            Instant::now() + Duration::from_secs(1),
            &AtomicBool::new(false),
            &mut || Ok(Self::owner()),
        )
    }
    fn write(&self, path: &str, raw: impl AsRef<[u8]>) {
        fs::write(self.proc.join(path), raw).unwrap();
    }
    fn relink(&self, path: &str, target: impl AsRef<Path>) {
        fs::remove_file(self.proc.join(path)).unwrap();
        symlink(target, self.proc.join(path)).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.base).unwrap();
    }
}
impl Observer for Fixture {
    fn read_file(&mut self, p: &Path, l: usize, b: &Budget<'_>) -> Result<Vec<u8>, TunError> {
        self.reads.push(p.to_path_buf());
        if self.oversized {
            return Ok(vec![0; l + 1]);
        }
        self.native.read_file(p, l, b)
    }
    fn read_link(&mut self, p: &Path, b: &Budget<'_>) -> Result<PathBuf, TunError> {
        self.native.read_link(p, b)
    }
    fn metadata(
        &mut self,
        p: &Path,
        follow: bool,
        b: &Budget<'_>,
    ) -> Result<FileIdentity, TunError> {
        self.native.metadata(p, follow, b)
    }
    fn list_dir(&mut self, p: &Path, l: usize, b: &Budget<'_>) -> Result<Vec<String>, TunError> {
        if let Some(v) = &self.list_override {
            return Ok(v.clone());
        }
        self.native.list_dir(p, l, b)
    }
    fn interfaces(&mut self, b: &Budget<'_>) -> Result<Vec<Interface>, TunError> {
        b.check()?;
        Ok(self.interfaces.clone())
    }
    fn ipv4_routes(&mut self, b: &Budget<'_>) -> Result<Vec<Ipv4Prefix>, TunError> {
        b.check()?;
        self.routes.clone()
    }
}
#[test]
fn synthetic_linux_tree_owned_once_without_dns_or_artifact_reads() {
    let mut f = Fixture::new();
    let id = f.bind().unwrap();
    assert_eq!(f.observe(&id), Ok(OwnedObservation::TunOwned));
    assert!(
        !f.reads
            .iter()
            .any(|p| p == &f.artifact || p.to_string_lossy().contains("cmdline"))
    );
    assert_eq!(
        check_startup_once(
            CONFIG.as_bytes(),
            &id,
            &mut f,
            Instant::now() + Duration::from_secs(1),
            &AtomicBool::new(false),
            &mut || Ok(Fixture::owner()),
            None
        ),
        Err(TunError::ListenerNotWired)
    );
    let mut probes = 0;
    let mut listener = |_raw: &[u8], budget: &Budget<'_>| {
        probes += 1;
        budget.check()
    };
    assert_eq!(
        check_startup_once(
            CONFIG.as_bytes(),
            &id,
            &mut f,
            Instant::now() + Duration::from_secs(1),
            &AtomicBool::new(false),
            &mut || Ok(Fixture::owner()),
            Some(&mut listener)
        ),
        Ok(OwnedObservation::TunOwned)
    );
    assert_eq!(probes, 1);
    assert!(!format!("{id:?} {:?}", f.interfaces).contains("172.31"));
}
#[test]
fn interface_and_fd_socket_proof_fail_closed() {
    let changes: Vec<fn(&mut Fixture)> = vec![
        |f| {
            f.interfaces.pop();
        },
        |f| f.interfaces[1].up = false,
        |f| f.interfaces[1].mtu = 1400,
        |f| f.interfaces.push(f.interfaces[1].clone()),
        |f| f.interfaces[1].addresses[0] = addr("172.31.255.254", 30),
        |f| f.interfaces[1].addresses[0].bits = 32,
        |f| f.interfaces[1].addresses.push(addr("10.1.2.3", 30)),
        |f| f.interfaces[1].addresses.push(addr("fd00::1", 64)),
        |f| f.interfaces[1].addresses.push(addr("::ffff:10.1.2.3", 128)),
        |f| f.write("sys/net/ipv4/conf/b6p-tun/rp_filter", "1\n"),
        |f| f.write("sys/net/ipv4/conf/b6p-tun/rp_filter", "0\n"),
        |f| f.write("4321/fdinfo/7", "pos:\t0\n"),
        |f| f.write("4321/fdinfo/7", "iff:\tb6p-other\n"),
        |f| f.write("4321/fdinfo/7", "iff:\tb6p-tun\niff:\tb6p-tun\n"),
        |f| f.relink("4321/fd/7", "/dev/null"),
        |f| f.relink("4321/fd/8", "socket:[999]"),
        |f| {
            f.write(
                "net/tcp",
                format!("{HEADER}{}", tcp("172.31.255.253", 53, "0A", 111)),
            )
        },
        |f| {
            f.write(
                "net/tcp",
                format!("{HEADER}{}", tcp("172.31.255.254", 35001, "0A", 111)),
            )
        },
        |f| {
            f.write(
                "net/tcp",
                format!("{HEADER}{}", tcp("0.0.0.0", 35001, "0A", 111)),
            )
        },
        |f| {
            f.write(
                "net/tcp",
                format!(
                    "{HEADER}{}{}",
                    tcp("172.31.255.253", 35001, "0A", 111),
                    tcp("172.31.255.253", 35002, "0A", 111)
                ),
            )
        },
        |f| {
            f.write(
                "net/tcp",
                format!("{HEADER}{}", tcp("172.31.255.253", 35001, "01", 111)),
            )
        },
        |f| {
            f.write(
                "net/tcp",
                format!("{HEADER}{}", tcp("172.31.255.253", 0, "0A", 111)),
            )
        },
        |f| {
            f.write(
                "net/tcp",
                format!("{HEADER}{}", tcp("172.31.255.253", 35001, "0A", 0)),
            )
        },
        |f| f.write("net/tcp", format!("{HEADER}truncated\n")),
        |f| f.list_override = Some(vec!["7".into(); MAX_FDS + 1]),
        |f| f.oversized = true,
    ];
    for (index, change) in changes.into_iter().enumerate() {
        let mut f = Fixture::new();
        let id = f.bind().unwrap();
        change(&mut f);
        assert!(f.observe(&id).is_err(), "case {index}");
    }
}
#[test]
fn bind_rejects_foreign_deleted_and_replaced_inodes() {
    for change in [
        |f: &mut Fixture| f.relink("4321/exe", "/usr/bin/foreign"),
        |f: &mut Fixture| {
            f.relink(
                "4321/exe",
                PathBuf::from(format!("{} (deleted)", f.artifact.display())),
            )
        },
        |f: &mut Fixture| f.write("4321/stat", stat(0, "S")),
        |f: &mut Fixture| f.write("4321/stat", stat(123, "Z")),
        |f: &mut Fixture| f.write("4321/stat", stat(123, "X")),
    ] {
        let mut f = Fixture::new();
        change(&mut f);
        assert!(f.bind().is_err());
    }
    let mut f = Fixture::new();
    let id = f.bind().unwrap();
    let replacement = f.artifact.with_file_name(".artifact-replacement");
    fs::write(&replacement, b"private inode fixture").unwrap();
    fs::set_permissions(&replacement, fs::Permissions::from_mode(0o700)).unwrap();
    fs::rename(&replacement, &f.artifact).unwrap();
    assert!(f.observe(&id).is_err());
}
#[test]
fn process_stat_adversarial_comm_and_pid_reuse() {
    assert_eq!(
        parse_process_start(stat(123456, "S").as_bytes(), PID),
        Ok(123456)
    );
    assert!(parse_process_start(stat(123456, "S").as_bytes(), PID + 1).is_err());
    assert!(parse_process_start(b"4321 (broken) S 1", PID).is_err());
    let mut f = Fixture::new();
    let id = f.bind().unwrap();
    f.write("4321/stat", stat(654321, "S"));
    assert_eq!(f.observe(&id), Err(TunError::IdentityChanged));
}
#[test]
fn before_after_generation_drift_is_refused() {
    let mut f = Fixture::new();
    let id = f.bind().unwrap();
    let mut calls = 0;
    let mut status = || {
        calls += 1;
        let mut s = Fixture::owner();
        if calls > 1 {
            s.generation += 1;
        }
        Ok(s)
    };
    assert_eq!(
        observe_owned_once(
            CONFIG.as_bytes(),
            &id,
            &mut f,
            Instant::now() + Duration::from_secs(1),
            &AtomicBool::new(false),
            &mut status
        ),
        Err(TunError::IdentityChanged)
    );
    for owner in [
        OwnedStatus {
            pid: 0,
            ..Fixture::owner()
        },
        OwnedStatus {
            service: ServiceId::Frpc,
            ..Fixture::owner()
        },
        OwnedStatus {
            running: false,
            ..Fixture::owner()
        },
    ] {
        assert!(
            observe_owned_once(
                CONFIG.as_bytes(),
                &id,
                &mut f,
                Instant::now() + Duration::from_secs(1),
                &AtomicBool::new(false),
                &mut || Ok(owner)
            )
            .is_err()
        );
    }
}
#[test]
fn startup_reobserves_after_callback_and_budget() {
    let mut f = Fixture::new();
    let id = f.bind().unwrap();
    let proc = f.proc.clone();
    let mut listener = move |_raw: &[u8], b: &Budget<'_>| {
        fs::write(proc.join("4321/stat"), stat(654321, "S")).unwrap();
        b.check()
    };
    assert_eq!(
        check_startup_once(
            CONFIG.as_bytes(),
            &id,
            &mut f,
            Instant::now() + Duration::from_secs(1),
            &AtomicBool::new(false),
            &mut || Ok(Fixture::owner()),
            Some(&mut listener)
        ),
        Err(TunError::IdentityChanged)
    );
    let mut f = Fixture::new();
    let id = f.bind().unwrap();
    let cancel = AtomicBool::new(false);
    let mut listener = |_raw: &[u8], _b: &Budget<'_>| {
        cancel.store(true, Ordering::Relaxed);
        Ok(())
    };
    assert_eq!(
        check_startup_once(
            CONFIG.as_bytes(),
            &id,
            &mut f,
            Instant::now() + Duration::from_secs(1),
            &cancel,
            &mut || Ok(Fixture::owner()),
            Some(&mut listener)
        ),
        Err(TunError::Cancelled)
    );
}
#[test]
fn prestart_pure_collision_and_all_table_requirement() {
    let mut f = Fixture::new();
    assert_eq!(
        check_prestart_once(
            CONFIG.as_bytes(),
            &mut f,
            Instant::now() + Duration::from_secs(1),
            &AtomicBool::new(false)
        ),
        Err(TunError::Collision)
    );
    f.interfaces.pop();
    assert_eq!(
        check_prestart_once(
            CONFIG.as_bytes(),
            &mut f,
            Instant::now() + Duration::from_secs(1),
            &AtomicBool::new(false)
        ),
        Ok(())
    );
    f.routes = Err(TunError::Unavailable);
    assert_eq!(
        check_prestart_once(
            CONFIG.as_bytes(),
            &mut f,
            Instant::now() + Duration::from_secs(1),
            &AtomicBool::new(false)
        ),
        Err(TunError::Unavailable)
    );
    for output in [
        "local 172.31.255.253 dev foreign table local",
        "unicast 172.31.255.252/30 table 123 dev foreign",
        "blackhole 172.31.0.0/16 table 99",
    ] {
        f.routes = Ok(parse_route_output(output.as_bytes()).unwrap());
        assert_eq!(
            check_prestart_once(
                CONFIG.as_bytes(),
                &mut f,
                Instant::now() + Duration::from_secs(1),
                &AtomicBool::new(false)
            ),
            Err(TunError::Collision)
        );
    }
    f.routes = Ok(parse_route_output(
        b"default via 172.31.255.253 dev wan table 10\n10.0.0.0/8 dev lan\n",
    )
    .unwrap());
    assert_eq!(
        check_prestart_once(
            CONFIG.as_bytes(),
            &mut f,
            Instant::now() + Duration::from_secs(1),
            &AtomicBool::new(false)
        ),
        Ok(())
    );
    f.interfaces[0].addresses.push(addr("172.31.255.254", 30));
    assert_eq!(
        check_prestart_once(
            CONFIG.as_bytes(),
            &mut f,
            Instant::now() + Duration::from_secs(1),
            &AtomicBool::new(false)
        ),
        Err(TunError::Collision)
    );
}
#[test]
fn bounded_source_and_native_fixture_readers() {
    assert_eq!(
        native_target(&vec![b' '; MAX_BYTES + 1]),
        Err(TunError::Limit)
    );
    let mut f = Fixture::new();
    let cancel = AtomicBool::new(false);
    let b = Budget {
        deadline: Instant::now() + Duration::from_secs(1),
        cancel: &cancel,
    };
    f.write("4321/stat", vec![b'x'; 129]);
    assert_eq!(
        f.native.read_file(Path::new("/proc/4321/stat"), 128, &b),
        Err(TunError::Limit)
    );
    assert_eq!(
        f.native.list_dir(Path::new("/proc/4321/fd"), 1, &b),
        Err(TunError::Limit)
    );
    cancel.store(true, Ordering::Relaxed);
    assert_eq!(
        f.native.read_file(Path::new("/proc/4321/stat"), 128, &b),
        Err(TunError::Cancelled)
    );
}
fn nlmsg(kind: u16, flags: u16, body: &[u8]) -> Vec<u8> {
    let len = 16 + body.len();
    let mut raw = vec![0; len.next_multiple_of(4)];
    raw[..4].copy_from_slice(&(len as u32).to_ne_bytes());
    raw[4..6].copy_from_slice(&kind.to_ne_bytes());
    raw[6..8].copy_from_slice(&flags.to_ne_bytes());
    raw[8..12].copy_from_slice(&7u32.to_ne_bytes());
    raw[16..len].copy_from_slice(body);
    raw
}
fn route_body(table: u8, ip: Ipv4Addr, bits: u8) -> Vec<u8> {
    let mut raw = vec![0u8; 20];
    raw[0] = libc::AF_INET as u8;
    raw[1] = bits;
    raw[4] = table;
    raw[7] = 1;
    raw[12..14].copy_from_slice(&8u16.to_ne_bytes());
    raw[14..16].copy_from_slice(&1u16.to_ne_bytes());
    raw[16..].copy_from_slice(&ip.octets());
    raw
}
#[test]
fn netlink_fixtures_include_local_policy_and_interrupted_dump() {
    let mut raw = nlmsg(
        24,
        2,
        &route_body(255, "172.31.255.253".parse().unwrap(), 32),
    );
    raw.extend(nlmsg(
        24,
        2,
        &route_body(123, "10.0.0.0".parse().unwrap(), 8),
    ));
    raw.extend(nlmsg(24, 2, &route_body(254, Ipv4Addr::UNSPECIFIED, 0)));
    raw.extend(nlmsg(3, 2, &0u32.to_ne_bytes()));
    let dump = parse_netlink_routes(&raw, 7).unwrap();
    assert!(dump.done);
    assert_eq!(dump.routes.len(), 2);
    assert!(parse_netlink_routes(&raw, 8).is_err());
    assert!(parse_netlink_routes(&nlmsg(3, 0x10, &0u32.to_ne_bytes()), 7).is_err());
    assert!(parse_netlink_routes(&nlmsg(2, 2, &0u32.to_ne_bytes()), 7).is_err());
    assert!(
        parse_netlink_routes(
            &nlmsg(
                24,
                2,
                &route_body(123, "172.31.255.253".parse().unwrap(), 30)
            ),
            7
        )
        .is_err()
    );
    for length in 1..16 {
        assert!(parse_netlink_routes(&raw[..length], 7).is_err());
    }
}

#[test]
fn strict_controlled_fields_ipv6_policy_and_no_guessed_address() {
    let original: serde_json::Value = serde_json::from_str(CONFIG).unwrap();
    let replacements = [
        ("type", serde_json::json!("direct")),
        ("tag", serde_json::json!("other")),
        ("interface_name", serde_json::json!("br-lan")),
        ("interface_name", serde_json::json!("b6p-+")),
        ("interface_name", serde_json::json!("b6p-tun.1")),
        ("interface_name", serde_json::json!("b6p-123456789012")),
        ("mtu", serde_json::json!(1400)),
        ("dns_mode", serde_json::json!("hijack")),
        ("auto_route", serde_json::json!(true)),
        ("auto_redirect", serde_json::json!(true)),
        ("auto_route", serde_json::Value::Null),
        ("udp_timeout", serde_json::json!("60s")),
        ("udp_nat_max", serde_json::json!(8192)),
        ("address", serde_json::json!([])),
        (
            "address",
            serde_json::json!(["172.31.255.253/30", "10.0.0.1/30"]),
        ),
        ("address", serde_json::json!(["172.31.255.252/30"])),
        ("address", serde_json::json!(["172.31.255.255/30"])),
        ("address", serde_json::json!(["203.0.113.1/30"])),
        ("address", serde_json::json!(["100.64.0.1/30"])),
        ("address", serde_json::json!(["fd00::1/126"])),
        ("address", serde_json::json!(["172.031.255.253/30"])),
        ("address", serde_json::json!(["172.31.255.253/32"])),
    ];
    for (field, value) in replacements {
        let mut config = original.clone();
        config["inbounds"][1][field] = value;
        assert!(
            native_target(&serde_json::to_vec(&config).unwrap()).is_err(),
            "{field}"
        );
    }
    for field in [
        "type",
        "tag",
        "interface_name",
        "address",
        "mtu",
        "stack",
        "dns_mode",
        "auto_route",
        "auto_redirect",
        "udp_timeout",
        "udp_nat_max",
    ] {
        let mut config = original.clone();
        config["inbounds"][1].as_object_mut().unwrap().remove(field);
        assert!(
            native_target(&serde_json::to_vec(&config).unwrap()).is_err(),
            "missing {field}"
        );
    }
    for rule in [
        serde_json::json!({"ip_version":6,"outbound":"proxy"}),
        serde_json::json!({"ip_version":6,"outbound":"direct","inbound":"tun-in"}),
        serde_json::json!({"ip_version":6,"action":"reject"}),
    ] {
        let mut config = original.clone();
        config["route"]["rules"] = serde_json::json!([rule]);
        assert!(native_target(&serde_json::to_vec(&config).unwrap()).is_err());
    }
    let mut config = original.clone();
    config.as_object_mut().unwrap().remove("route");
    assert!(native_target(&serde_json::to_vec(&config).unwrap()).is_err());
    let mut config = original.clone();
    let extra = config["inbounds"][1].clone();
    config["inbounds"].as_array_mut().unwrap().push(extra);
    assert!(native_target(&serde_json::to_vec(&config).unwrap()).is_err());
    assert!(
        native_target(
            CONFIG
                .replace(
                    r#""outbound":"direct""#,
                    r#""outbound":"direct","outbound":"direct""#
                )
                .as_bytes()
        )
        .is_err()
    );
}
#[test]
fn startup_after_callback_rechecks_fd_socket_and_inode() {
    for mutation in 0..3 {
        let mut f = Fixture::new();
        let id = f.bind().unwrap();
        let proc = f.proc.clone();
        let artifact = f.artifact.clone();
        let mut callback = move |_raw: &[u8], budget: &Budget<'_>| {
            match mutation {
                0 => fs::write(proc.join("net/tcp"), HEADER).unwrap(),
                1 => fs::write(proc.join("4321/fdinfo/7"), b"iff:\tforeign\n").unwrap(),
                _ => {
                    let replacement = artifact.with_file_name(".artifact-new");
                    fs::write(&replacement, b"new inode").unwrap();
                    fs::set_permissions(&replacement, fs::Permissions::from_mode(0o700)).unwrap();
                    fs::rename(replacement, &artifact).unwrap();
                }
            }
            budget.check()
        };
        assert!(
            check_startup_once(
                CONFIG.as_bytes(),
                &id,
                &mut f,
                Instant::now() + Duration::from_secs(1),
                &AtomicBool::new(false),
                &mut || Ok(Fixture::owner()),
                Some(&mut callback)
            )
            .is_err()
        );
    }
}

#[test]
fn compiled_route_expansion_retains_only_exact_direct_ipv6_proof() {
    let many = std::iter::repeat_n("{}", MAX_FDS + 1)
        .collect::<Vec<_>>()
        .join(",");
    let raw = CONFIG.replace(
        r#"{"ip_version":6,"outbound":"direct"}"#,
        &format!(r#"{many},{{"ip_version":6,"outbound":"direct"}}"#),
    );
    assert!(native_target(raw.as_bytes()).unwrap().is_some());
    let invalid = raw.replace(
        r#""ip_version":6,"outbound":"direct""#,
        r#""ip_version":6,"outbound":"proxy""#,
    );
    assert_eq!(native_target(invalid.as_bytes()), Err(TunError::Ipv6Policy));
    let too_many = std::iter::repeat_n("{}", be6500_panel::policy::MAX_RULES * 2 + 129)
        .collect::<Vec<_>>()
        .join(",");
    let over = CONFIG.replace(r#"{"ip_version":6,"outbound":"direct"}"#, &too_many);
    assert!(native_target(over.as_bytes()).is_err());
}

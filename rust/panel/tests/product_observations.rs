//! Synthetic native sources only. These tests do not execute Go, shell
//! commands, or live service/configuration actions.
use be6500_panel::product_io::{Backend, Error, Metadata, Output, Program};
use be6500_panel::product_observations::Observations;
use be6500_panel::readiness_tun::Budget;
use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

static NEVER_CANCEL: AtomicBool = AtomicBool::new(false);
fn budget() -> Budget<'static> {
    Budget {
        deadline: Instant::now() + Duration::from_secs(5),
        cancel: &NEVER_CANCEL,
    }
}
fn command_key(program: Program, args: &[String]) -> String {
    format!("{program:?}:{}", args.join("\0"))
}
struct Fake {
    files: BTreeMap<String, Result<Vec<u8>, Error>>,
    sequences: BTreeMap<String, VecDeque<Result<Vec<u8>, Error>>>,
    #[allow(clippy::type_complexity)] // Synthetic fixed-command fixtures.
    commands: BTreeMap<String, Result<(i32, Vec<u8>, Vec<u8>), Error>>,
    directories: BTreeMap<String, Result<Vec<String>, Error>>,
    // kind: regular, directory, symlink. mode is exact native permission bits.
    metadata: BTreeMap<String, (u8, u32, u64, u64)>,
    links: BTreeMap<String, PathBuf>,
    calls: Vec<(Program, Vec<String>)>,
    read_paths: Vec<String>,
    now: u64,
}
impl Fake {
    fn new() -> Self {
        Self {
            files: BTreeMap::new(),
            sequences: BTreeMap::new(),
            commands: BTreeMap::new(),
            directories: BTreeMap::new(),
            metadata: BTreeMap::new(),
            links: BTreeMap::new(),
            calls: vec![],
            read_paths: vec![],
            now: 1700000000,
        }
    }
    fn file(&mut self, path: &str, data: &str) {
        self.files
            .insert(path.to_owned(), Ok(data.as_bytes().to_vec()));
    }
    fn command(&mut self, program: Program, args: &[&str], data: &str) {
        let args: Vec<_> = args.iter().map(|s| (*s).to_owned()).collect();
        self.commands.insert(
            command_key(program, &args),
            Ok((0, data.as_bytes().to_vec(), vec![])),
        );
    }
    fn dir(&mut self, path: &str, names: &[&str]) {
        self.directories.insert(
            path.to_owned(),
            Ok(names.iter().map(|s| (*s).to_owned()).collect()),
        );
    }
    fn meta(&mut self, path: &str, kind: u8, mode: u32) {
        self.metadata.insert(
            path.to_owned(),
            (kind, mode, 1, self.metadata.len() as u64 + 1),
        );
    }
    fn executable(&mut self, path: &str) {
        let path = Path::new(path);
        for parent in path.ancestors().skip(1) {
            if parent != Path::new("/") {
                self.meta(parent.to_str().unwrap(), 1, 0o755);
            }
        }
        self.meta(path.to_str().unwrap(), 0, 0o755);
    }
    fn proc(&mut self, pid: u32, exe: &str, start: u64) {
        let prefix = format!("/proc/{pid}");
        self.meta("/proc", 1, 0o555);
        self.meta(&prefix, 1, 0o555);
        self.meta(&format!("{prefix}/exe"), 2, 0o777);
        self.links
            .insert(format!("{prefix}/exe"), PathBuf::from(exe));
        self.executable(exe);
        self.file(&format!("{prefix}/stat"), &stat(pid, start));
        self.file(
            &format!("{prefix}/status"),
            &format!("Name:\ttest\nPid:\t{pid}\nVmRSS:\t2048 kB\n"),
        );
    }
    fn system(&mut self) {
        self.file("/proc/sys/kernel/hostname", "router-fixture\n");
        self.file("/proc/sys/kernel/osrelease", "5.4.213\n");
        self.file("/proc/sys/kernel/ostype", "Linux\n");
        self.file("/proc/uptime", "12345.5 8000.0\n");
        self.file("/proc/loadavg", "0.10 0.20 0.30 1/33 10\n");
        self.file(
            "/proc/meminfo",
            "MemTotal: 524288 kB\nMemAvailable: 327680 kB\nCached: 10 kB\n",
        );
        self.file("/proc/stat", "cpu 1 2 3 4\ncpu0 1 2 3 4\ncpu1 1 2 3 4\n");
        self.file(
            "/etc/openwrt_release",
            "DISTRIB_ARCH='arm_cortex-a7_neon-vfpv4'\nDISTRIB_RELEASE='23.05.3'\n",
        );
        self.file("/usr/share/xiaoqiang/xiaoqiang_version","config core 'version'\n option HARDWARE 'RN02'\n option ROM '1.1.2'\n option token 'PRIVATE_VERSION_TOKEN'\n");
        let mut auxv = Vec::new();
        for n in [17usize, 100, 0, 0] {
            auxv.extend_from_slice(&n.to_ne_bytes());
        }
        self.files.insert("/proc/self/auxv".to_owned(), Ok(auxv));
    }
    fn network(&mut self) {
        self.dir("/sys/class/net", &["br-lan", "eth1", "lo"]);
        for iface in ["br-lan", "eth1", "lo"] {
            self.file(&format!("/sys/class/net/{iface}/flags"), "0x1003\n");
            self.file(&format!("/sys/class/net/{iface}/mtu"), "1500\n");
        }
        self.command(Program::Ip,&["-o","address","show"],
            "1: lo inet 127.0.0.1/8 scope host lo\n2: br-lan inet 192.168.31.1/24 brd 192.168.31.255 scope global br-lan\n2: br-lan inet6 fd00::1/64 scope global\n3: eth1 inet 10.0.0.2/24 scope global eth1\n");
        self.file(
            "/proc/net/route",
            concat!(
                "Iface Destination Gateway Flags RefCnt Use Metric Mask MTU Window IRTT\n",
                "br-lan 001FA8C0 00000000 0001 0 0 0 00FFFFFF 0 0 0\n",
                "eth1 00000000 0100000A 0003 0 0 10 00000000 0 0 0\n"
            ),
        );
        self.file("/proc/net/ipv6_route","00000000000000000000000000000000 00 00000000000000000000000000000000 00 fe800000000000000000000000000001 00000010 00000000 00000000 00000003 eth1\n");
        self.command(Program::Ubus,&["call","network.interface.wan","status","{}"],
            r#"{"up":true,"l3_device":"eth1","proto":"dhcp","uptime":123,"dns-server":["1.1.1.1"],"private":"WAN_PRIVATE"}"#);
        self.command(
            Program::Ubus,
            &["call", "network.interface.wan6", "status", "{}"],
            r#"{"up":false,"proto":"dhcpv6"}"#,
        );
        self.file("/tmp/dhcp.leases","1700003600 02:11:22:33:44:55 192.168.31.5 laptop *\n0 02:11:22:33:44:66 192.168.31.6 * *\n1699999999 02:11:22:33:44:77 192.168.31.7 expired *\n");
        self.file(
            "/proc/net/arp",
            concat!(
                "IP address HW type Flags HW address Mask Device\n",
                "192.168.31.5 0x1 0x2 02:11:22:33:44:55 * br-lan\n",
                "192.168.31.8 0x1 0x0 00:00:00:00:00:00 * br-lan\n"
            ),
        );
    }
    fn router(&mut self) {
        self.system();
        self.network();
        self.file("/etc/config/wireless",concat!("config wifi-device 'radio0'\n option band '5g'\n option channel 'auto'\n option htmode 'HE160'\n",
            "config wifi-iface 'ap0'\n option device 'radio0'\n option ifname 'ath0'\n option ssid 'Router WiFi 6'\n option encryption 'psk2'\n",
            " option key 'PRIVATE_WIRELESS_KEY'\n option radius_secret 'PRIVATE_RADIUS_SECRET'\n"));
        self.file(
            "/tmp/resolv.conf.d/resolv.conf.auto",
            "nameserver 1.1.1.1\nnameserver 2606:4700:4700::1111\n",
        );
        self.file("/proc/net/ip_tables_names", "filter\nnat\n");
        self.file("/proc/net/ip6_tables_names", "filter\n");
        self.command(Program::Iptables,&["-t","filter","-S"],"-P INPUT ACCEPT\n-P FORWARD DROP\n-P OUTPUT ACCEPT\n-N custom\n-A INPUT -j custom\n-A FORWARD -j DROP\n");
        self.command(Program::Iptables,&["-t","nat","-S"],"-P PREROUTING ACCEPT\n-P INPUT ACCEPT\n-P OUTPUT ACCEPT\n-P POSTROUTING ACCEPT\n-A POSTROUTING -o eth1 -j MASQUERADE\n");
        self.command(
            Program::Ip6tables,
            &["-t", "filter", "-S"],
            "-P INPUT DROP\n-P FORWARD DROP\n-P OUTPUT ACCEPT\n",
        );
        self.file("/proc/net/dev", &counters(1000, 500));
    }
    fn services(&mut self) {
        self.system();
        self.dir(
            "/etc/init.d",
            &["dnsmasq", "dropbear", "ddns", "native-installed"],
        );
        for service in ["dnsmasq", "dropbear", "ddns", "native-installed"] {
            self.meta(&format!("/etc/init.d/{service}"), 0, 0o755);
        }
        self.command(Program::Ubus,&["call","service","list","{}"],r#"{
            "dnsmasq":{"instances":{"main":{"running":true,"pid":42,"command":["/usr/sbin/dnsmasq","--password=PRIVATE_PROCESS_ARG"]}}},
            "dropbear":{"instances":{"main":{"running":false}}},
            "ddns":{"instances":{"main":{"running":false}}}}
        "#);
        self.proc(42, "/usr/sbin/dnsmasq", 10000);
    }
}
impl Backend for Fake {
    fn read(&mut self, path: &Path, limit: usize, _: &Budget<'_>) -> Result<Vec<u8>, Error> {
        let path = path.to_str().ok_or(Error::Invalid)?;
        self.read_paths.push(path.to_owned());
        let value = if let Some(sequence) = self.sequences.get_mut(path) {
            sequence.pop_front().unwrap_or(Err(Error::Unavailable))
        } else {
            self.files
                .get(path)
                .cloned()
                .unwrap_or(Err(Error::Unavailable))
        }?;
        if value.len() > limit {
            return Err(Error::Limit);
        }
        Ok(value)
    }
    fn run(
        &mut self,
        program: Program,
        args: &[String],
        stdin: Option<&[u8]>,
        limit: usize,
        _: &Budget<'_>,
    ) -> Result<Output, Error> {
        assert!(stdin.is_none());
        self.calls.push((program, args.to_vec()));
        let (code, stdout, stderr) = self
            .commands
            .get(&command_key(program, args))
            .cloned()
            .unwrap_or(Err(Error::Unavailable))?;
        if stdout.len() > limit || stderr.len() > limit {
            return Err(Error::Limit);
        }
        Ok(Output {
            code,
            stdout,
            stderr,
        })
    }
    fn now_unix(&self) -> u64 {
        self.now
    }
    fn list(&mut self, path: &Path, limit: usize, _: &Budget<'_>) -> Result<Vec<String>, Error> {
        let rows = self
            .directories
            .get(path.to_str().ok_or(Error::Invalid)?)
            .cloned()
            .unwrap_or(Err(Error::Unavailable))?;
        if rows.len() > limit {
            return Err(Error::Limit);
        }
        Ok(rows)
    }
    fn metadata(&mut self, path: &Path, follow: bool, _: &Budget<'_>) -> Result<Metadata, Error> {
        let selected = if follow {
            self.links
                .get(path.to_str().ok_or(Error::Invalid)?)
                .map(PathBuf::as_path)
                .unwrap_or(path)
        } else {
            path
        };
        let (kind, mode, dev, ino) = self
            .metadata
            .get(selected.to_str().ok_or(Error::Invalid)?)
            .copied()
            .ok_or(Error::Unavailable)?;
        Ok(Metadata {
            regular: kind == 0,
            directory: kind == 1,
            symlink: kind == 2,
            mode,
            uid: 0,
            size: 128,
            dev,
            ino,
        })
    }
    fn read_link(&mut self, path: &Path, _: &Budget<'_>) -> Result<PathBuf, Error> {
        self.links
            .get(path.to_str().ok_or(Error::Invalid)?)
            .cloned()
            .ok_or(Error::Unavailable)
    }
}
fn stat(pid: u32, start: u64) -> String {
    let mut fields = vec!["0".to_owned(); 22];
    fields[0] = "S".to_owned();
    fields[19] = start.to_string();
    format!("{pid} (a name with ) bracket) {}\n", fields.join(" "))
}
fn counters(rx: u64, tx: u64) -> String {
    format!(
        "Inter-| Receive | Transmit\n face |bytes packets errs drop fifo frame compressed multicast|bytes packets errs drop fifo colls carrier compressed\n eth1: {rx} 1 0 0 0 0 0 0 {tx} 1 0 0 0 0 0 0\n"
    )
}
fn call(observations: &mut Observations, fake: &mut Fake, path: &str) -> Value {
    observations.get(path, "", None, fake, &budget()).unwrap()
}
fn service<'a>(snapshot: &'a Value, name: &str) -> &'a Value {
    snapshot["services"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == name)
        .unwrap()
}

#[test]
fn system_native_proc_projection_has_exact_wire_and_no_demo_data() {
    let mut fake = Fake::new();
    fake.system();
    let v = call(&mut Observations::new(), &mut fake, "/api/system");
    assert_eq!(v["mode"], "host");
    assert_eq!(v["hostname"], "router-fixture");
    assert_eq!(v["os"], "linux");
    assert_eq!(v["arch"], "arm_cortex-a7_neon-vfpv4");
    assert_eq!(v["cpuCount"], 2);
    assert_eq!(
        v["memory"],
        json!({"totalBytes":536870912u64,"availableBytes":335544320u64})
    );
    assert_eq!(v["load"], json!([0.1, 0.2, 0.3]));
    assert_eq!(v["sampledAt"], "2023-11-14T22:13:20Z");
    assert!(fake.calls.is_empty());
}
#[test]
fn system_rejects_unknown_memory_nan_and_oversize_without_ready_zeroes() {
    let mut fake = Fake::new();
    fake.system();
    fake.file("/proc/meminfo", "MemTotal: 10 kB\n");
    assert_eq!(
        Observations::new()
            .get("/api/system", "", None, &mut fake, &budget())
            .unwrap_err()
            .status,
        503
    );
    fake.system();
    fake.file("/proc/loadavg", "NaN 1 1 1/2 3");
    assert_eq!(
        Observations::new()
            .get("/api/system", "", None, &mut fake, &budget())
            .unwrap_err()
            .code,
        "observation_invalid"
    );
    fake.system();
    fake.files
        .insert("/proc/meminfo".to_owned(), Ok(vec![b'x'; 65537]));
    assert_eq!(
        Observations::new()
            .get("/api/system", "", None, &mut fake, &budget())
            .unwrap_err()
            .code,
        "observation_too_large"
    );
}
#[test]
fn router_projects_platform_wifi6_firewall_wan_routes_devices_and_peer() {
    let mut fake = Fake::new();
    fake.router();
    let v = Observations::new()
        .get(
            "/api/router",
            "",
            Some("192.168.31.5".parse::<IpAddr>().unwrap()),
            &mut fake,
            &budget(),
        )
        .unwrap();
    assert_eq!(v["platform"]["model"], "RN02");
    assert_eq!(v["platform"]["firmware"], "1.1.2");
    assert_eq!(v["currentClientIP"], "192.168.31.5");
    assert_eq!(v["wifi"][0]["bandwidth"], "HE160");
    assert_eq!(v["wifi"][0]["wifi6"], true);
    assert_eq!(v["wifi"][0]["channel"], 0);
    assert_eq!(v["wifi"][0]["ssid"], "Router WiFi 6");
    assert_eq!(v["firewall"]["ipv4"]["rules"], 3);
    assert_eq!(v["firewall"]["ipv4"]["forward"], "DROP");
    assert_eq!(v["firewall"]["ipv6"]["input"], "DROP");
    assert_eq!(v["wan"][0]["device"], "eth1");
    assert!(
        v["routes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["destination"] == "0.0.0.0/0" && r["gateway"] == "10.0.0.1")
    );
    assert_eq!(v["devices"].as_array().unwrap().len(), 2);
    assert_eq!(v["devices"][0]["online"], true);
    assert_eq!(v["devices"][0]["eligible"], true);
    assert_eq!(v["devices"][1]["online"], false);
    assert_eq!(v["devices"][1]["expiresAt"], Value::Null);
    assert_eq!(v["dns"]["leaseCount"], 2);
    assert_eq!(v["traffic"][0]["rateAvailable"], false);
    let serialized = v.to_string();
    for secret in [
        "PRIVATE_WIRELESS_KEY",
        "PRIVATE_RADIUS_SECRET",
        "PRIVATE_VERSION_TOKEN",
        "WAN_PRIVATE",
    ] {
        assert!(!serialized.contains(secret));
    }
    assert!(
        fake.calls
            .iter()
            .all(|(program, _)| *program != Program::Service
                && *program != Program::Uci
                && *program != Program::Curl)
    );
}
#[test]
fn network_interface_flags_mtu_routes_are_actual_source_rows() {
    let mut fake = Fake::new();
    fake.network();
    let v = call(&mut Observations::new(), &mut fake, "/api/network");
    assert_eq!(v["routeObservationSupported"], true);
    assert_eq!(v["interfaces"][0]["name"], "br-lan");
    assert_eq!(
        v["interfaces"][0]["addresses"],
        json!(["192.168.31.1/24", "fd00::1/64"])
    );
    assert_eq!(v["interfaces"][0]["up"], true);
    assert_eq!(v["interfaces"][0]["mtu"], 1500);
    assert!(
        fake.read_paths
            .iter()
            .all(|p| !p.contains("dhcp.leases") && !p.contains("wireless"))
    );
}
#[test]
fn partial_device_sources_keep_rows_but_remove_capture_eligibility() {
    let mut fake = Fake::new();
    fake.network();
    fake.files.remove("/proc/net/arp");
    let v = call(&mut Observations::new(), &mut fake, "/api/devices");
    assert_eq!(v["supported"], true);
    assert_eq!(v["devices"].as_array().unwrap().len(), 2);
    assert!(
        v["devices"]
            .as_array()
            .unwrap()
            .iter()
            .all(|d| d["eligible"] == false && d["online"] == false)
    );
    assert_eq!(v["availability"]["devices.arp"]["available"], false);
    assert!(
        v["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["module"] == "devices.arp")
    );
}
#[test]
fn device_identity_conflicts_incomplete_arp_and_stale_arp_never_authorize() {
    let mut fake = Fake::new();
    fake.network();
    fake.file(
        "/proc/net/arp",
        concat!(
            "IP address HW type Flags HW address Mask Device\n",
            "192.168.31.5 0x1 0x2 02:ff:22:33:44:55 * br-lan\n",
            "192.168.31.9 0x1 0x2 02:11:22:33:44:66 * br-lan\n"
        ),
    );
    let v = call(&mut Observations::new(), &mut fake, "/api/devices");
    assert_eq!(v["devices"].as_array().unwrap().len(), 3);
    let laptop = v["devices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["hostname"] == "laptop")
        .unwrap();
    assert_eq!(laptop["eligible"], false);
    assert_eq!(laptop["online"], false);
    assert!(
        !v["devices"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["ip"] == "192.168.31.9")
    );
}
#[test]
fn traffic_first_reset_failure_and_recovery_are_qualified_not_synthetic_history() {
    let mut fake = Fake::new();
    fake.router();
    let mut observer = Observations::new();
    let first = call(&mut observer, &mut fake, "/api/router");
    assert_eq!(first["traffic"][0]["rateAvailable"], false);
    fake.file("/proc/net/dev", &counters(2000, 1500));
    let second = call(&mut observer, &mut fake, "/api/router");
    assert_eq!(second["traffic"][0]["rateAvailable"], true);
    let dt = second["traffic"][0]["rateIntervalSeconds"]
        .as_f64()
        .unwrap();
    let rate = second["traffic"][0]["rxBytesPerSecond"].as_f64().unwrap();
    assert!((rate * dt - 1000.0).abs() < 0.01);
    fake.file("/proc/net/dev", &counters(1, 1));
    let reset = call(&mut observer, &mut fake, "/api/router");
    assert_eq!(reset["traffic"][0]["rateAvailable"], false);
    fake.files.remove("/proc/net/dev");
    let lost = call(&mut observer, &mut fake, "/api/router");
    assert_eq!(lost["traffic"], json!([]));
    fake.file("/proc/net/dev", &counters(5, 6));
    let fresh = call(&mut observer, &mut fake, "/api/router");
    assert_eq!(fresh["traffic"][0]["rateAvailable"], false);
}
#[test]
fn service_get_preserves_registration_presence_executable_rss_and_allowed_actions_without_execution()
 {
    let mut fake = Fake::new();
    fake.services();
    let v = call(&mut Observations::new(), &mut fake, "/api/system/services");
    assert_eq!(v["stale"], false);
    assert_eq!(v["sampledAt"], "2023-11-14T22:13:20Z");
    let dns = service(&v, "dnsmasq");
    assert_eq!(dns["configured"], "present");
    assert_eq!(dns["registered"], "registered");
    assert_eq!(dns["processState"], "running");
    assert_eq!(dns["reportedPID"], 42);
    assert_eq!(dns["pid"], 42);
    assert_eq!(dns["executable"], "/usr/sbin/dnsmasq");
    assert_eq!(dns["rssBytes"], 2097152);
    assert_eq!(dns["startTicks"], 10000);
    assert_eq!(dns["uptimeSeconds"], 12245.5);
    assert_eq!(dns["actions"], json!(["reload", "restart"]));
    assert_eq!(service(&v, "dropbear")["protected"], true);
    assert!(service(&v, "dropbear").get("actions").is_none());
    assert_eq!(
        service(&v, "native-installed")["registered"],
        "unregistered"
    );
    assert_eq!(service(&v, "native-installed")["configured"], "present");
    assert_eq!(
        service(&v, "ddns")["actions"],
        json!(["start", "restart", "reload"])
    );
    assert!(!v.to_string().contains("PRIVATE_PROCESS_ARG"));
    assert_eq!(fake.calls.len(), 1);
    assert_eq!(fake.calls[0].0, Program::Ubus);
}
#[test]
fn service_pid_reuse_stays_unknown_until_procd_changes_identity() {
    let mut fake = Fake::new();
    fake.services();
    let mut observer = Observations::new();
    let before = call(&mut observer, &mut fake, "/api/system/services");
    assert_eq!(service(&before, "dnsmasq")["processState"], "running");
    fake.file("/proc/42/stat", &stat(42, 10001));
    for _ in 0..2 {
        let reused = call(&mut observer, &mut fake, "/api/system/services");
        let row = service(&reused, "dnsmasq");
        assert_eq!(row["processState"], "unknown");
        assert_eq!(row["errorCode"], "pid_reused");
        assert!(row.get("pid").is_none());
    }
    fake.command(
        Program::Ubus,
        &["call", "service", "list", "{}"],
        r#"{"dnsmasq":{"instances":{"main":{"running":false}}}}"#,
    );
    call(&mut observer, &mut fake, "/api/system/services");
    fake.services();
    fake.file("/proc/42/stat", &stat(42, 10001));
    let new_identity = call(&mut observer, &mut fake, "/api/system/services");
    assert_eq!(service(&new_identity, "dnsmasq")["processState"], "running");
}
#[test]
fn service_command_and_race_sources_do_not_fabricate_running() {
    let mut fake = Fake::new();
    fake.services();
    fake.links
        .insert("/proc/42/exe".to_owned(), PathBuf::from("/usr/sbin/other"));
    fake.executable("/usr/sbin/other");
    let v = call(&mut Observations::new(), &mut fake, "/api/system/services");
    assert_eq!(service(&v, "dnsmasq")["processState"], "unknown");
    fake.services();
    fake.sequences.insert(
        "/proc/42/stat".to_owned(),
        VecDeque::from([
            Ok(stat(42, 10000).into_bytes()),
            Ok(stat(42, 10001).into_bytes()),
        ]),
    );
    let raced = call(&mut Observations::new(), &mut fake, "/api/system/services");
    assert_eq!(service(&raced, "dnsmasq")["processState"], "unknown");
    assert!(service(&raced, "dnsmasq").get("rssBytes").is_none());
}
#[test]
fn unavailable_duplicate_and_oversize_procd_remain_unknown_with_null_sample_time() {
    let mut fake = Fake::new();
    fake.services();
    fake.commands.clear();
    let v = call(&mut Observations::new(), &mut fake, "/api/system/services");
    assert_eq!(v["stale"], true);
    assert_eq!(v["sampledAt"], Value::Null);
    assert_eq!(service(&v, "dnsmasq")["registered"], "unknown");
    assert!(service(&v, "dnsmasq").get("actions").is_none());
    fake.command(
        Program::Ubus,
        &["call", "service", "list", "{}"],
        r#"{"dnsmasq":{"instances":{"main":{"pid":42,"pid":43}}}}"#,
    );
    let duplicate = call(&mut Observations::new(), &mut fake, "/api/system/services");
    assert_eq!(duplicate["errorCode"], "invalid");
    let rows = (0..129)
        .map(|i| format!("\"service{i}\":{{}}"))
        .collect::<Vec<_>>()
        .join(",");
    fake.command(
        Program::Ubus,
        &["call", "service", "list", "{}"],
        &format!("{{{rows}}}"),
    );
    let oversized = call(&mut Observations::new(), &mut fake, "/api/system/services");
    assert_eq!(oversized["errorCode"], "too_large");
}
#[test]
fn frpc_json_toml_ini_projection_excludes_credentials_and_reports_process_gap() {
    for (path, config) in [
        (
            "/etc/frpc.json",
            r#"{"auth":{"token":"FRPC_PRIVATE"},"proxies":[{"name":"ssh","type":"tcp","localIP":"127.0.0.1","localPort":22,"remotePort":2222,"secretKey":"PRIVATE_PROXY"}]}"#,
        ),
        (
            "/etc/frpc.toml",
            "auth.token = 'FRPC_PRIVATE'\n[[proxies]]\nname = 'ssh'\ntype = 'tcp'\nlocalIP = '127.0.0.1'\nlocalPort = 22\nremotePort = 2222\nsecretKey = 'PRIVATE_PROXY'\n",
        ),
        (
            "/etc/frpc.ini",
            "[common]\ntoken = FRPC_PRIVATE\n[ssh]\ntype = tcp\nlocal_ip = 127.0.0.1\nlocal_port = 22\nremote_port = 2222\nsecret_key = PRIVATE_PROXY\n",
        ),
    ] {
        let mut fake = Fake::new();
        fake.services();
        fake.file(path, config);
        let v = call(&mut Observations::new(), &mut fake, "/api/frpc");
        assert_eq!(v["supported"], true);
        assert_eq!(v["running"], false);
        assert_eq!(v["processObservationAvailable"], false);
        assert_eq!(v["configObservationAvailable"], true);
        assert_eq!(
            v["proxies"],
            json!([{"name":"ssh","type":"tcp","localAddress":"127.0.0.1","localPort":22,"remotePort":2222}])
        );
        assert!(!v.to_string().contains("FRPC_PRIVATE"));
        assert!(!v.to_string().contains("PRIVATE_PROXY"));
    }
}
#[test]
fn modules_are_evidence_backed_and_root_mutation_seams_are_not_claimed() {
    let mut fake = Fake::new();
    fake.router();
    fake.services();
    let v = call(&mut Observations::new(), &mut fake, "/api/modules");
    assert_eq!(v["modules"].as_array().unwrap().len(), 8);
    assert!(
        v["modules"]
            .as_array()
            .unwrap()
            .iter()
            .all(|m| m["state"] == "ready")
    );
    assert!(
        v["modules"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|m| m["capabilities"].as_array().unwrap())
            .all(|c| !["apply", "configure", "runtime", "plan"]
                .contains(&c["id"].as_str().unwrap())
                || c["supported"] == false)
    );
    let unavailable = call(&mut Observations::new(), &mut Fake::new(), "/api/modules");
    assert!(
        unavailable["modules"]
            .as_array()
            .unwrap()
            .iter()
            .all(|m| m["state"] == "unavailable")
    );
}
#[test]
fn invalid_queries_paths_and_cancelled_or_expired_budget_do_no_io() {
    let mut fake = Fake::new();
    let mut observer = Observations::new();
    assert_eq!(
        observer
            .get("/api/router", "argv=stop", None, &mut fake, &budget())
            .unwrap_err()
            .status,
        400
    );
    assert_eq!(
        observer
            .get("/api/private", "", None, &mut fake, &budget())
            .unwrap_err()
            .status,
        404
    );
    let cancel = AtomicBool::new(true);
    let b = Budget {
        deadline: Instant::now() + Duration::from_secs(5),
        cancel: &cancel,
    };
    assert_eq!(
        observer
            .get("/api/router", "", None, &mut fake, &b)
            .unwrap_err()
            .code,
        "observation_cancelled"
    );
    let b = Budget {
        deadline: Instant::now() - Duration::from_secs(1),
        cancel: &NEVER_CANCEL,
    };
    assert_eq!(
        observer
            .get("/api/router", "", None, &mut fake, &b)
            .unwrap_err()
            .code,
        "observation_timeout"
    );
    assert!(fake.read_paths.is_empty());
    assert!(fake.calls.is_empty());
}

#[test]
fn service_nonexecutable_script_no_actions_and_auxv_failure_no_invented_uptime() {
    let mut fake = Fake::new();
    fake.services();
    fake.meta("/etc/init.d/dnsmasq", 0, 0o644);
    fake.files.remove("/proc/self/auxv");
    let v = call(&mut Observations::new(), &mut fake, "/api/system/services");
    let row = service(&v, "dnsmasq");
    assert_eq!(row["configured"], "present");
    assert_eq!(row["processState"], "running");
    assert!(row.get("actions").is_none());
    assert!(row.get("uptimeSeconds").is_none());
    assert!(
        v["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["module"] == "services.proc_timing")
    );
}
#[test]
fn service_bare_command_resolves_same_native_executable_through_bounded_symlinks() {
    let mut fake = Fake::new();
    fake.services();
    fake.command(
        Program::Ubus,
        &["call", "service", "list", "{}"],
        r#"{"dnsmasq":{"instances":{"main":{"running":true,"pid":42,"command":["dnsmasq"]}}}}"#,
    );
    fake.meta("/sbin", 2, 0o777);
    fake.links
        .insert("/sbin".to_owned(), PathBuf::from("usr/sbin"));
    let v = call(&mut Observations::new(), &mut fake, "/api/system/services");
    assert_eq!(service(&v, "dnsmasq")["processState"], "running");
    assert_eq!(service(&v, "dnsmasq")["executable"], "/usr/sbin/dnsmasq");
}
#[test]
fn generated_resolver_symlink_is_observed_and_never_executed() {
    let mut fake = Fake::new();
    fake.router();
    fake.files.remove("/tmp/resolv.conf.d/resolv.conf.auto");
    fake.meta("/tmp", 1, 0o755);
    fake.meta("/tmp/resolv.conf.d", 1, 0o755);
    fake.meta("/tmp/resolv.conf.d/resolv.conf.auto", 2, 0o777);
    fake.links.insert(
        "/tmp/resolv.conf.d/resolv.conf.auto".to_owned(),
        PathBuf::from("../resolv-native"),
    );
    fake.meta("/tmp/resolv-native", 0, 0o644);
    fake.file("/tmp/resolv-native", "nameserver 9.9.9.9\n");
    let v = call(&mut Observations::new(), &mut fake, "/api/router");
    assert_eq!(v["dns"]["resolvers"], json!(["9.9.9.9"]));
    assert!(fake.read_paths.iter().any(|p| p == "/tmp/resolv-native"));
}
#[test]
fn malformed_native_rows_remain_partial_and_private_command_errors_are_redacted() {
    let mut fake = Fake::new();
    fake.router();
    fake.file(
        "/proc/net/route",
        concat!(
            "Iface Destination Gateway Flags RefCnt Use Metric Mask MTU Window IRTT\n",
            "eth1 00000000 0100000A 0003 0 0 10 00000000 0 0 0\nBROKEN_PRIVATE_ROUTE\n"
        ),
    );
    let key = command_key(
        Program::Ip6tables,
        &["-t".to_owned(), "filter".to_owned(), "-S".to_owned()],
    );
    fake.commands
        .insert(key, Ok((1, vec![], b"PRIVATE_FIREWALL_ERROR".to_vec())));
    let v = call(&mut Observations::new(), &mut fake, "/api/router");
    assert!(
        v["routes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["destination"] == "0.0.0.0/0")
    );
    assert_eq!(v["availability"]["routes.ipv4"]["complete"], false);
    assert_eq!(v["firewall"]["ipv6"]["input"], "");
    assert_eq!(v["firewall"]["ipv6"]["policyAvailable"], false);
    assert!(!v.to_string().contains("PRIVATE_FIREWALL_ERROR"));
    assert!(!v.to_string().contains("BROKEN_PRIVATE_ROUTE"));
}
#[test]
fn lease_cap_and_bad_unicast_identity_return_bounded_ineligible_public_rows() {
    let mut fake = Fake::new();
    fake.network();
    let mut source = String::new();
    for i in 0..300 {
        source.push_str(&format!(
            "0 02:11:22:33:{:02x}:{:02x} 192.168.31.5 host{i} *\n",
            (i / 256) as u8,
            (i % 256) as u8
        ));
    }
    fake.file("/tmp/dhcp.leases", &source);
    let v = call(&mut Observations::new(), &mut fake, "/api/devices");
    assert!(v["devices"].as_array().unwrap().len() <= 256);
    assert!(
        v["devices"]
            .as_array()
            .unwrap()
            .iter()
            .all(|d| d["eligible"] == false)
    );
    assert!(
        v["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["code"] == "too_large")
    );
}

#[test]
fn native_product_cache_reuses_public_snapshots_and_invalidation_refreshes() {
    let mut fake = Fake::new();
    fake.system();
    let mut observations = Observations::new().with_cache();
    let first = call(&mut observations, &mut fake, "/api/system");
    let reads = fake.read_paths.len();
    let again = call(&mut observations, &mut fake, "/api/system");
    assert_eq!(first, again);
    assert_eq!(fake.read_paths.len(), reads);
    observations.invalidate();
    let refreshed = call(&mut observations, &mut fake, "/api/system");
    assert_eq!(first, refreshed);
    assert!(fake.read_paths.len() > reads);
}

#[test]
fn cached_router_client_identity_is_always_current_request() {
    let mut fake = Fake::new();
    fake.router();
    let mut observations = Observations::new().with_cache();
    let first = observations
        .get(
            "/api/router",
            "",
            Some("192.168.31.2".parse().unwrap()),
            &mut fake,
            &budget(),
        )
        .unwrap();
    assert_eq!(first["currentClientIP"], "192.168.31.2");
    let reads = fake.read_paths.len();
    let second = observations
        .get(
            "/api/router",
            "",
            Some("192.168.31.3".parse().unwrap()),
            &mut fake,
            &budget(),
        )
        .unwrap();
    assert_eq!(second["currentClientIP"], "192.168.31.3");
    let no_peer = observations
        .get("/api/router", "", None, &mut fake, &budget())
        .unwrap();
    assert!(no_peer.get("currentClientIP").is_none());
    assert_eq!(fake.read_paths.len(), reads);
}

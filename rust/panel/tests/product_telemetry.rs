#![cfg(any(target_os = "linux", target_os = "macos"))]
use be6500_panel::{
    http::Method,
    product_io::{Backend, Error, Output, Program},
    product_telemetry::Telemetry,
    readiness_tun::Budget,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
static CANCEL: AtomicBool = AtomicBool::new(false);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "b6p-product-telemetry-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn telemetry(&self) -> Telemetry {
        Telemetry::open(&self.0).unwrap()
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn budget() -> Budget<'static> {
    Budget {
        deadline: Instant::now() + Duration::from_secs(3),
        cancel: &CANCEL,
    }
}
struct Native {
    now: u64,
    files: BTreeMap<PathBuf, Vec<u8>>,
    traffic: Option<Vec<u8>>,
    calls: usize,
}
impl Native {
    fn new(now: u64) -> Self {
        let mut n = Self {
            now,
            files: BTreeMap::new(),
            traffic: Some(b"{}".to_vec()),
            calls: 0,
        };
        n.wan(100, 50);
        n
    }
    fn wan(&mut self, rx: u64, tx: u64) {
        self.files.insert(PathBuf::from("/proc/net/route"),b"Iface Destination Gateway Flags RefCnt Use Metric Mask MTU Window IRTT\nwan 00000000 0100000A 0003 0 0 10 00000000 0 0 0\n".to_vec());
        self.files.insert(PathBuf::from("/proc/net/dev"),format!("Inter-| Receive | Transmit\n face |bytes packets errs drop fifo frame compressed multicast|bytes packets errs drop fifo colls carrier compressed\nwan: {rx} 1 0 0 0 0 0 0 {tx} 1 0 0 0 0 0 0\n").into_bytes());
    }
    fn devices(&mut self, rows: Vec<(String, u64, u64)>) {
        let mut map = serde_json::Map::new();
        for (id, rx, tx) in rows {
            map.insert(id.clone(),json!({"hw":id,"hostname":"real-host","ifname":"wlan0","assoc":1,"online_timer":self.now,
                "ageing_timer":0,"mld":1,"signal":"-44","noise":"-95","wifiprotocol":"802.11be","nego_rx_rate":"2882Mbps",
                "nego_tx_rate":"2161Mbps","wireless_ageing":0,
                "ip_list":[{"ip":"192.0.2.10","rx_bytes":rx,"tx_bytes":tx}]}));
        }
        self.traffic = Some(serde_json::to_vec(&map).unwrap());
    }
}
impl Backend for Native {
    fn read(&mut self, path: &Path, limit: usize, _: &Budget<'_>) -> Result<Vec<u8>, Error> {
        let data = self.files.get(path).ok_or(Error::Unavailable)?;
        if data.len() > limit {
            return Err(Error::Limit);
        }
        Ok(data.clone())
    }
    fn run(
        &mut self,
        p: Program,
        args: &[String],
        stdin: Option<&[u8]>,
        limit: usize,
        _: &Budget<'_>,
    ) -> Result<Output, Error> {
        assert_eq!(p, Program::Ubus);
        assert_eq!(
            args,
            [
                "call",
                "trafficd",
                "hw",
                r#"{"detail":true,"wlan":true,"mlo":true}"#
            ]
        );
        assert!(stdin.is_none());
        self.calls += 1;
        let data = self.traffic.as_ref().ok_or(Error::Unavailable)?;
        if data.len() > limit {
            return Err(Error::Limit);
        }
        Ok(Output {
            code: 0,
            stdout: data.clone(),
            stderr: Vec::new(),
        })
    }
    fn now_unix(&self) -> u64 {
        self.now
    }
}
fn get(t: &mut Telemetry, n: &mut Native, path: &str, q: &str) -> Value {
    t.handle(path, Method::Get, q, &[], None, n, &budget())
        .unwrap()
}
fn tick(t: &mut Telemetry, n: &mut Native) {
    t.tick(None, n, &budget()).unwrap();
}
fn mac(i: usize) -> String {
    format!("02:00:00:00:{:02X}:{:02X}", i / 256, i % 256)
}

#[test]
fn wan_deltas_coverage_gaps_counter_reset_and_legacy_reopen() {
    let dir = Directory::new();
    let mut t = dir.telemetry();
    let mut n = Native::new(1_800_000_000);
    tick(&mut t, &mut n);
    tick(&mut t, &mut n);
    assert_eq!(n.calls, 1);
    n.now += 2;
    n.wan(500, 250);
    tick(&mut t, &mut n);
    let h = get(
        &mut t,
        &mut n,
        "/api/traffic/history",
        "range=30m&maxPoints=100",
    );
    assert_eq!(h["summary"]["rxBytes"], 400);
    assert_eq!(h["summary"]["txBytes"], 200);
    assert_eq!(h["summary"]["coverageSeconds"], 2.0);
    assert_eq!(h["source"], "wan");
    assert!(
        h["samples"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["coverageSeconds"] == 0.0)
    );
    n.now += 2;
    n.wan(2, 1);
    tick(&mut t, &mut n);
    assert!(
        get(&mut t, &mut n, "/api/traffic/history", "")
            .get("current")
            .is_none()
    );
    n.now += 60;
    n.wan(30, 15);
    tick(&mut t, &mut n);
    let h = get(&mut t, &mut n, "/api/traffic/history", "");
    assert_eq!(h["summary"]["rxBytes"], 400);
    assert!(h.get("lastFlushAt").is_some());
    drop(t);
    let mut t = dir.telemetry();
    let h = get(
        &mut t,
        &mut n,
        "/api/traffic/history",
        "range=1y&maxPoints=30",
    );
    assert_eq!(h["summary"]["rxBytes"], 400);
    assert!(h.get("lastFlushAt").is_none());
    for (seconds, size) in [(30, 737344), (300, 1105984), (3600, 1228864)] {
        let file = fs::read(dir.0.join(format!("traffic/wan-{seconds}s.ring"))).unwrap();
        assert_eq!(file.len(), size);
        assert_eq!(&file[..8], b"WANRING\0");
    }
}

#[test]
fn trafficd_actual_rate_partial_failure_conflicts_paging_and_missing_nulls() {
    let dir = Directory::new();
    let mut t = dir.telemetry();
    let mut n = Native::new(1_800_000_000);
    n.devices((0..70).map(|i| (mac(i), 100, 50)).collect());
    tick(&mut t, &mut n);
    let first = get(
        &mut t,
        &mut n,
        "/api/devices/activity",
        "range=30m&limit=64",
    );
    assert_eq!(first["matchedCount"], 70);
    assert_eq!(first["devices"].as_array().unwrap().len(), 64);
    assert_eq!(first["nextOffset"], 64);
    assert!(first["devices"][0].get("rxBytesPerSecond").is_none());
    assert!(
        first["devices"][0]["samples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["rxBytes"].is_null())
    );
    n.now += 15;
    n.devices((0..70).map(|i| (mac(i), 400, 200)).collect());
    tick(&mut t, &mut n);
    let next = get(
        &mut t,
        &mut n,
        "/api/devices/activity",
        "range=30m&limit=64&offset=64",
    );
    assert_eq!(next["devices"].as_array().unwrap().len(), 6);
    assert_eq!(next["devices"][0]["rxBytesPerSecond"], 20.0);
    assert_eq!(next["devices"][0]["rxBytes"], 300);
    assert_eq!(next["devices"][0]["links"][0]["negotiatedRX"], "2882Mbps");
    assert_eq!(next["groups"][0]["deviceCount"], 70);
    assert_eq!(next["devices"][0]["addressConflicts"][0], "192.0.2.10");
    n.now += 15;
    let mut rows: Value = serde_json::from_slice(n.traffic.as_ref().unwrap()).unwrap();
    rows[&mac(0)]["ip_list"][0]["rx_bytes"] = json!("invalid");
    n.traffic = Some(serde_json::to_vec(&rows).unwrap());
    tick(&mut t, &mut n);
    let partial = get(
        &mut t,
        &mut n,
        "/api/devices/activity",
        &format!("search={}&range=30m", mac(0)),
    );
    assert_eq!(partial["devices"][0]["rawRXBytes"], 400);
    assert_eq!(
        partial["devices"][0]["lastSeen"],
        be6500_panel::product_io::timestamp(n.now - 15)
    );
    assert_eq!(partial["devices"][0]["stale"], true);
    assert!(partial["devices"][0].get("rxBytesPerSecond").is_none());
    n.now += 15;
    n.traffic = None;
    tick(&mut t, &mut n);
    let unavailable = get(&mut t, &mut n, "/api/devices/activity", "range=30m");
    assert_eq!(unavailable["state"], "unavailable");
    assert_eq!(unavailable["deviceCount"], 70);
}

#[test]
fn annotations_legacy_cas_draft_orphan_backup_preservation_and_reopen() {
    let dir = Directory::new();
    let mut t = dir.telemetry();
    let mut n = Native::new(1_800_000_000);
    fs::write(dir.0.join("device-names.json.bak"), b"existing backup").unwrap();
    fs::write(dir.0.join("device-label-drafts.json"), b"existing draft").unwrap();
    fs::write(
        dir.0.join("orphan-device-history.json"),
        b"historical orphan",
    )
    .unwrap();
    let body = json!({"mac":"02-00-00-00-00-01","label":"办公机","note":"plaintext note","tags":["work"],"expectedRevision":0});
    let a = t
        .handle(
            "/api/devices/annotations",
            Method::Post,
            "",
            &serde_json::to_vec(&body).unwrap(),
            None,
            &mut n,
            &budget(),
        )
        .unwrap();
    assert_eq!(a["revision"], 1);
    assert_eq!(a["devices"][&mac(1)]["label"], "办公机");
    let error = t
        .handle(
            "/api/devices/annotations",
            Method::Post,
            "",
            &serde_json::to_vec(&body).unwrap(),
            None,
            &mut n,
            &budget(),
        )
        .unwrap_err();
    assert_eq!(error.code, "revision_conflict");
    drop(t);
    let mut t = dir.telemetry();
    assert_eq!(
        get(&mut t, &mut n, "/api/devices/annotations", "")["revision"],
        1
    );
    for (file, value) in [
        ("device-names.json.bak", "existing backup"),
        ("device-label-drafts.json", "existing draft"),
        ("orphan-device-history.json", "historical orphan"),
    ] {
        assert_eq!(fs::read_to_string(dir.0.join(file)).unwrap(), value);
    }
    fs::write(
        dir.0.join("device-names.json"),
        br#"{"revision":2,"revision":3,"devices":{}}"#,
    )
    .unwrap();
    let mut reopened = dir.telemetry();
    let e = reopened
        .handle(
            "/api/devices/annotations",
            Method::Get,
            "",
            &[],
            None,
            &mut n,
            &budget(),
        )
        .unwrap_err();
    assert_eq!(e.code, "storage_failed");
    assert!(
        fs::read_to_string(dir.0.join("device-names.json"))
            .unwrap()
            .contains("revision\":3")
    );
    // Source-qualified annotation failure does not disable real WAN observation.
    tick(&mut reopened, &mut n);
    assert_eq!(
        get(&mut reopened, &mut n, "/api/traffic/history", "")["source"],
        "wan"
    );
}

#[test]
fn corrupt_one_legacy_tier_keeps_other_history_without_replacing_corrupt_bytes() {
    let dir = Directory::new();
    let mut t = dir.telemetry();
    let mut n = Native::new(1_800_000_000);
    tick(&mut t, &mut n);
    n.now += 2;
    n.wan(500, 250);
    tick(&mut t, &mut n);
    n.now += 60;
    tick(&mut t, &mut n);
    drop(t);
    let corrupt = dir.0.join("traffic/wan-300s.ring");
    fs::write(&corrupt, b"orphan legacy damaged file").unwrap();
    let mut t = dir.telemetry();
    let valid = get(&mut t, &mut n, "/api/traffic/history", "range=30m");
    assert_eq!(valid["summary"]["rxBytes"], 400);
    assert_eq!(fs::read(&corrupt).unwrap(), b"orphan legacy damaged file");
    let unavailable = get(&mut t, &mut n, "/api/traffic/history", "range=7d");
    assert!(unavailable["samples"].as_array().unwrap().is_empty());
    assert!(unavailable["error"].as_str().unwrap().contains("tier"));
}

fn core_fixture(responses: Vec<Value>) -> (Vec<u8>, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let raw=serde_json::to_vec(&json!({"experimental":{"clash_api":{"external_controller":address.to_string(),"secret":"NEVER_ECHO_PRIVATE_SECRET"}},
        "outbounds":[{"tag":"proxy","type":"selector","outbounds":["node-real"]},{"tag":"node-real","type":"trojan","server":"PRIVATE_SERVER","password":"PRIVATE_PASSWORD"}]})).unwrap();
    let worker = thread::spawn(move || {
        let mut requests = Vec::new();
        for response in responses {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut part = [0u8; 4096];
            while !bytes.ends_with(b"\r\n\r\n") {
                let n = stream.read(&mut part).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&part[..n]);
            }
            requests.push(String::from_utf8(bytes).unwrap());
            let body = serde_json::to_vec(&response).unwrap();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(&body).unwrap();
        }
        requests
    });
    (raw, worker)
}
fn core_connections(upload: u64, download: u64, start: &str) -> Value {
    json!({"uploadTotal":upload,"downloadTotal":download,"connections":[{"id":"raw-private-id","start":start,"upload":40,"download":80,
        "chains":["node-real","proxy"],"rule":"auth_user=NEVER_ECHO_PRIVATE_SECRET => route(proxy)",
        "metadata":{"network":"tcp","sourceIP":"192.0.2.10","sourcePort":"12345","destinationIP":"198.51.100.2","destinationPort":"443","host":"example.test"}}]})
}
#[test]
fn literal_core_real_totals_selected_chain_probe_redaction_and_partial_recovery() {
    let dir = Directory::new();
    let mut t = dir.telemetry();
    let mut n = Native::new(1_800_000_000);
    let start = be6500_panel::product_io::timestamp(n.now - 2);
    let proxies = json!({"proxies":{"proxy":{"type":"Selector","now":"node-real","all":["node-real"]},"node-real":{"name":"node-real","type":"Trojan"}}});
    let (native, server) = core_fixture(vec![
        core_connections(100, 200, &start),
        proxies.clone(),
        core_connections(500, 1000, &start),
        proxies,
        json!({"delay":93}),
    ]);
    t.tick(Some(&native), &mut n, &budget()).unwrap();
    n.now += 2;
    t.tick(Some(&native), &mut n, &budget()).unwrap();
    let ready = get(&mut t, &mut n, "/api/proxy/metrics", "");
    assert_eq!(ready["state"], "ready");
    assert_eq!(ready["traffic"][1]["uploadRate"], 200.0);
    assert_eq!(ready["connections"][0]["outbound"], "proxy");
    assert!(ready["connections"][0].get("nodeId").is_some());
    assert_eq!(ready["selectedOutbounds"][0]["nodeName"], "node-real");
    assert_eq!(ready["selectionState"], "ready");
    assert_eq!(ready["connections"][0]["sourceIP"], "192.0.2.10");
    assert_eq!(
        ready["connections"][0]["rule"],
        "实际匹配规则（非公开条件）"
    );
    let probe = t
        .handle(
            "/api/proxy/probe",
            Method::Post,
            "",
            b"{}",
            Some(&native),
            &mut n,
            &budget(),
        )
        .unwrap();
    assert_eq!(probe["probes"][0]["delayMs"], 93);
    let requests = server.join().unwrap();
    assert!(requests[1].starts_with("GET /proxies HTTP/1.1"));
    assert!(requests[4].starts_with("GET /proxies/proxy/delay?timeout=5000&url="));
    assert!(
        requests
            .iter()
            .all(|r| r.contains("Authorization: Bearer NEVER_ECHO_PRIVATE_SECRET"))
    );
    let serialized = serde_json::to_string(&probe).unwrap();
    for private in [
        "NEVER_ECHO_PRIVATE_SECRET",
        "PRIVATE_SERVER",
        "PRIVATE_PASSWORD",
        "raw-private-id",
    ] {
        assert!(!serialized.contains(private));
    }
    n.now += 2;
    n.traffic = None;
    t.tick(Some(&native), &mut n, &budget()).unwrap();
    let stale = get(&mut t, &mut n, "/api/proxy/metrics", "");
    assert_eq!(stale["state"], "stale");
    assert_eq!(stale["totals"], ready["totals"]);
    assert_eq!(stale["connections"], ready["connections"]);
    assert_eq!(
        get(&mut t, &mut n, "/api/traffic/history", "")["source"],
        "wan"
    );
}
#[test]
fn query_method_and_cancellation_admission_are_strict() {
    let dir = Directory::new();
    let mut t = dir.telemetry();
    let mut n = Native::new(1_800_000_000);
    for (path, q) in [
        ("/api/traffic/history", "range=30m&range=1y"),
        ("/api/traffic/history", "maxPoints=2001"),
        ("/api/devices/activity", "limit=65"),
        ("/api/devices/activity", "offset=129"),
        ("/api/proxy/metrics", "secret=echo"),
    ] {
        assert_eq!(
            t.handle(path, Method::Get, q, &[], None, &mut n, &budget())
                .unwrap_err()
                .status,
            400
        );
    }
    assert_eq!(
        t.handle(
            "/api/proxy/probe",
            Method::Get,
            "",
            &[],
            None,
            &mut n,
            &budget()
        )
        .unwrap_err()
        .status,
        405
    );
    let cancel = AtomicBool::new(true);
    let expired = Budget {
        deadline: Instant::now() + Duration::from_secs(1),
        cancel: &cancel,
    };
    assert_eq!(t.tick(None, &mut n, &expired).unwrap_err().status, 408);
    assert_eq!(n.calls, 0);
}

#[test]
fn ipv6_default_route_fallback_reads_actual_sysfs_counter_pair() {
    let dir = Directory::new();
    let mut t = dir.telemetry();
    let mut n = Native::new(1_800_000_000);
    n.files.remove(Path::new("/proc/net/route"));
    n.files.remove(Path::new("/proc/net/dev"));
    n.files.insert(PathBuf::from("/proc/net/ipv6_route"),b"00000000000000000000000000000000 00 00000000000000000000000000000000 00 fe800000000000000000000000000001 00000010 00000000 00000000 00000003 wan6\n".to_vec());
    for (kind, value) in [("rx", 100), ("tx", 200)] {
        n.files.insert(
            PathBuf::from(format!("/sys/class/net/wan6/statistics/{kind}_bytes")),
            format!("{value}\n").into_bytes(),
        );
    }
    tick(&mut t, &mut n);
    n.now += 2;
    for (kind, value) in [("rx", 300), ("tx", 600)] {
        n.files.insert(
            PathBuf::from(format!("/sys/class/net/wan6/statistics/{kind}_bytes")),
            format!("{value}\n").into_bytes(),
        );
    }
    tick(&mut t, &mut n);
    let h = get(&mut t, &mut n, "/api/traffic/history", "");
    assert_eq!(h["source"], "wan6");
    assert_eq!(h["summary"]["rxBytes"], 200);
    assert_eq!(h["current"]["tx"], 200.0);
    n.now += 2;
    n.files
        .remove(Path::new("/sys/class/net/wan6/statistics/tx_bytes"));
    tick(&mut t, &mut n);
    let h = get(&mut t, &mut n, "/api/traffic/history", "");
    assert!(h.get("current").is_none());
    assert_eq!(h["summary"]["rxBytes"], 200);
}
#[test]
fn device_observed_zero_is_covered_reset_and_recovery_intervals_are_not() {
    let dir = Directory::new();
    let mut t = dir.telemetry();
    let mut n = Native::new(1_800_000_000);
    n.devices(vec![(mac(1), 100, 200)]);
    tick(&mut t, &mut n);
    n.now += 15;
    n.devices(vec![(mac(1), 100, 200)]);
    tick(&mut t, &mut n);
    let zero = get(&mut t, &mut n, "/api/devices/activity", "range=30m");
    assert_eq!(zero["devices"][0]["rxBytesPerSecond"], 0.0);
    assert_eq!(zero["devices"][0]["coverageSeconds"], 15);
    assert!(
        zero["devices"][0]["samples"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["rxBytes"] == 0 && p["coverageSeconds"] == 15)
    );
    n.now += 15;
    n.devices(vec![(mac(1), 1, 2)]);
    tick(&mut t, &mut n);
    let reset = get(&mut t, &mut n, "/api/devices/activity", "range=30m");
    assert!(reset["devices"][0].get("rxBytesPerSecond").is_none());
    assert_eq!(reset["devices"][0]["coverageSeconds"], 15);
    n.now += 15;
    n.traffic = None;
    tick(&mut t, &mut n);
    n.now += 15;
    n.devices(vec![(mac(1), 101, 202)]);
    tick(&mut t, &mut n);
    let recovery = get(&mut t, &mut n, "/api/devices/activity", "range=30m");
    assert!(recovery["devices"][0].get("rxBytesPerSecond").is_none());
    n.now += 15;
    n.devices(vec![(mac(1), 201, 402)]);
    tick(&mut t, &mut n);
    let measured = get(&mut t, &mut n, "/api/devices/activity", "range=30m");
    assert_eq!(measured["devices"][0]["rxBytes"], 100);
    assert_eq!(measured["devices"][0]["coverageSeconds"], 30);
}

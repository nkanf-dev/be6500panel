//! Synthetic Fake IO only. No router, process, service or shell command runs here.
use be6500_panel::{
    features::Impact,
    features_gateway::Features,
    features_recovery::Recovery,
    http::Method,
    product_io::{Backend, Error, Output, Program},
    readiness_tun::Budget,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
struct Fake {
    now: u64,
    replies: BTreeMap<String, Value>,
    calls: Vec<(Program, Vec<String>, Value)>,
    service_code: i32,
    scalar_values: BTreeMap<String, String>,
    unavailable_call: Option<(Program, Vec<String>)>,
    fail_vendor: bool,
    break_generation: Option<PathBuf>,
    mutate_on_reload: Option<PathBuf>,
    boot: String,
    lan: String,
    kernel_lan: Option<String>,
    rules: String,
}
impl Fake {
    fn new() -> Self {
        Self {
            now: 1_800_000_000,
            replies: BTreeMap::new(),
            calls: vec![],
            service_code: 0,
            scalar_values: BTreeMap::new(),
            unavailable_call: None,
            fail_vendor: false,
            break_generation: None,
            mutate_on_reload: None,
            boot: "11111111-1111-1111-1111-111111111111".into(),
            lan: "192.168.31.1".into(),
            kernel_lan: None,
            rules: "-N zone_wan_prerouting\n".into(),
        }
    }
    fn reply(&mut self, handler: &str, value: Value) {
        self.replies.insert(handler.into(), value);
    }
}
impl Backend for Fake {
    fn read(&mut self, path: &Path, limit: usize, b: &Budget<'_>) -> Result<Vec<u8>, Error> {
        b.check().map_err(|_| Error::Deadline)?;
        let raw = if path == Path::new("/proc/sys/kernel/random/boot_id") {
            self.boot.as_bytes().to_vec()
        } else {
            fs::read(path).map_err(|_| Error::Unavailable)?
        };
        if raw.len() > limit {
            Err(Error::Limit)
        } else {
            Ok(raw)
        }
    }
    fn run(
        &mut self,
        p: Program,
        args: &[String],
        stdin: Option<&[u8]>,
        _: usize,
        b: &Budget<'_>,
    ) -> Result<Output, Error> {
        b.check().map_err(|_| Error::Deadline)?;
        let input = stdin
            .and_then(|s| serde_json::from_slice(s).ok())
            .unwrap_or(json!({}));
        self.calls.push((p, args.to_vec(), input));
        if self
            .unavailable_call
            .as_ref()
            .is_some_and(|(program, expected)| *program == p && expected == args)
        {
            return Err(Error::Unavailable);
        }
        let mut code = 0;
        let stdout = match p {
            Program::Vendor => {
                if self.fail_vendor {
                    return Err(Error::Unavailable);
                }
                if let Some(path) = self.break_generation.take() {
                    fs::create_dir(path).unwrap();
                }
                serde_json::to_vec(
                    self.replies
                        .get(args[1].as_str())
                        .unwrap_or(&json!({"code":0})),
                )
                .unwrap()
            }
            Program::Service => {
                // Current-factory synthetic allow-list: phantom daemons must fail.
                assert!(matches!(
                    args[0].as_str(),
                    "port_service"
                        | "network"
                        | "wifi"
                        | "dnsmasq"
                        | "firewall"
                        | "mwan3"
                        | "ddns"
                        | "miqos"
                        | "miniupnpd"
                        | "nginx"
                        | "system"
                        | "scan"
                        | "led_ctl"
                        | "parentalctl"
                ));
                code = self.service_code;
                if args[1] == "reload"
                    && let Some(path) = self.mutate_on_reload.take()
                {
                    fs::write(path, b"reload rewrote config").unwrap();
                }
                Vec::new()
            }
            Program::Uci => {
                let key = args.last().map(String::as_str).unwrap();
                if let Some(value) = self.scalar_values.get(key) {
                    format!("{value}\n").into_bytes()
                } else {
                    match key {
                        "network.lan.ipaddr" => format!("{}\n", self.lan).into_bytes(),
                        "xiaoqiang.common.NETMODE" => b"router\n".to_vec(),
                        "xiaoqiang.common.INITTED" => b"NO\n".to_vec(),
                        _ => {
                            code = 1;
                            Vec::new()
                        }
                    }
                }
            }
            Program::Ubus => Vec::new(),
            Program::Ip => serde_json::to_vec(
                &json!([{"addr_info":[{"local":self.kernel_lan.as_ref().unwrap_or(&self.lan)}]}]),
            )
            .unwrap(),
            Program::Iptables => self.rules.as_bytes().to_vec(),
            _ => return Err(Error::Unavailable),
        };
        Ok(Output {
            code,
            stdout,
            stderr: vec![],
        })
    }
    fn now_unix(&self) -> u64 {
        self.now
    }
}
struct Fixture {
    base: PathBuf,
    data: PathBuf,
    native: PathBuf,
    db: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let mut random = [0; 12];
        getrandom::fill(&mut random).unwrap();
        let base = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "feature-gateway-{}",
                random
                    .iter()
                    .map(|v| format!("{v:02x}"))
                    .collect::<String>()
            ));
        fs::DirBuilder::new().mode(0o700).create(&base).unwrap();
        let data = base.join("data");
        let native = base.join("native");
        for p in [&data, &native] {
            fs::DirBuilder::new().mode(0o700).create(p).unwrap();
        }
        let db_parent = base.join("db");
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&db_parent)
            .unwrap();
        let db = db_parent.join("xqDb");
        Self {
            base,
            data,
            native,
            db,
        }
    }
    fn open(&self) -> Features {
        Features::open_with_paths(&self.data, &self.native, &self.db)
    }
    fn checkpoint_root(&self) -> PathBuf {
        self.data.join("feature-operations")
    }
    fn put(&self, key: &str, raw: &[u8]) {
        let p = self.native.join(key);
        fs::write(&p, raw).unwrap();
        fs::set_permissions(p, fs::Permissions::from_mode(0o600)).unwrap();
    }
    fn private(&self, key: &str, raw: &[u8]) {
        let r = self.checkpoint_root();
        if !r.exists() {
            fs::DirBuilder::new().mode(0o700).create(&r).unwrap();
        }
        let p = r.join(key);
        fs::write(&p, raw).unwrap();
        fs::set_permissions(p, fs::Permissions::from_mode(0o600)).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}
fn budget(flag: &AtomicBool) -> Budget<'_> {
    Budget {
        deadline: Instant::now() + Duration::from_secs(10),
        cancel: flag,
    }
}
fn call(
    f: &mut Features,
    io: &mut Fake,
    path: &str,
    method: Method,
    query: &str,
    body: &[u8],
) -> Result<Value, be6500_panel::features::Error> {
    let flag = AtomicBool::new(false);
    f.handle(path, method, query, body, io, &budget(&flag), &mut |_| {
        Ok(())
    })
}
fn ready(f: &mut Features, io: &mut Fake) {
    let flag = AtomicBool::new(false);
    f.tick(io, &budget(&flag));
    assert!(!f.busy());
}
fn tick(f: &mut Features, io: &mut Fake) {
    let flag = AtomicBool::new(false);
    f.tick(io, &budget(&flag));
}
fn apply(f: &mut Features, io: &mut Fake, domain: &str, id: &str, input: Value) -> Value {
    let body=serde_json::to_vec(&json!({"actionId":id,"input":input,"generation":f.catalog()["generation"],"acknowledgeImpact":true})).unwrap();
    call(
        f,
        io,
        &format!("/api/features/{domain}/apply"),
        Method::Post,
        "",
        &body,
    )
    .unwrap()
}
fn wifi() -> Value {
    json!({"code":0,"bsd":1,"info":[
    {"ssid":"Home","password":"exact-secret","ssidHtmlEncode":1,"status":"1","encryption":"psk2","channel":"1","bandwidth":"40","txpwr":"max","hidden":"0","bsd":"1","ax":"1","wifimode":"11beg","ssid_len_limit":28,"available_channels":[{"c":0,"b":["20","40"]},{"c":1,"b":["20","40"]}]},
    {"ssid":"Home5","password":"second-secret","status":"1","encryption":"psk2","channel":"36","bandwidth":"40","bsd":"1","ax":"1","wifimode":"11bea","ssid_len_limit":31,"available_channels":[{"c":0,"b":["20","40"]},{"c":36,"b":["20","40"]}]}]})
}
#[test]
fn open_get_and_invalid_cas_never_cleanup_or_mutate() {
    let fixture = Fixture::new();
    let mut f = fixture.open();
    let mut io = Fake::new();
    assert!(f.busy());
    assert_eq!(
        call(
            &mut f,
            &mut io,
            "/api/features/catalog",
            Method::Get,
            "",
            b""
        )
        .unwrap()["generation"],
        1
    );
    assert!(io.calls.is_empty());
    let bad = br#"{"actionId":"ddns_reload","input":{},"generation":0}"#;
    assert_eq!(
        call(
            &mut f,
            &mut io,
            "/api/features/services/apply",
            Method::Post,
            "",
            bad
        )
        .unwrap_err()
        .code,
        "generation_conflict"
    );
    let duplicate = br#"{"actionId":"ddns_reload","input":{"id":1,"id":2},"generation":1}"#;
    assert_eq!(
        call(
            &mut f,
            &mut io,
            "/api/features/services/apply",
            Method::Post,
            "",
            duplicate
        )
        .unwrap_err()
        .status,
        400
    );
    assert!(io.calls.is_empty());
    ready(&mut f, &mut io);
    assert!(io.calls.is_empty());
}
#[test]
fn unreadable_generation_isolated_catalog_and_reads_but_no_writes() {
    let fixture = Fixture::new();
    fixture.private("generation", b"not-a-sequence");
    let mut f = fixture.open();
    let mut io = Fake::new();
    io.reply("getWanSpeed", json!({"code":0,"speed":1000}));
    assert_eq!(f.catalog()["generation"], 0);
    tick(&mut f, &mut io);
    assert_eq!(
        call(
            &mut f,
            &mut io,
            "/api/features/network/state",
            Method::Get,
            "read=wan_speed",
            b""
        )
        .unwrap()["data"]["speed"],
        1000
    );
    let body = br#"{"actionId":"ddns_reload","input":{},"generation":0}"#;
    assert_eq!(
        call(
            &mut f,
            &mut io,
            "/api/features/services/apply",
            Method::Post,
            "",
            body
        )
        .unwrap_err()
        .code,
        "feature_storage_unavailable"
    );
    assert_eq!(
        fs::read(fixture.checkpoint_root().join("generation")).unwrap(),
        b"not-a-sequence"
    );
}
#[test]
fn read_parameters_are_strict_cached_and_unknown_route_rejected() {
    let fixture = Fixture::new();
    let mut f = fixture.open();
    let mut io = Fake::new();
    ready(&mut f, &mut io);
    io.reply("getWanInfo",json!({"code":0,"info":{"details":{"wanType":"pppoe","username":"private-user","password":"private-pwd"}}}));
    let first = call(
        &mut f,
        &mut io,
        "/api/features/network/state",
        Method::Get,
        "read=wan_info&wan_name=WAN2",
        b"",
    )
    .unwrap();
    let second = call(
        &mut f,
        &mut io,
        "/api/features/network/state",
        Method::Get,
        "read=wan_info&wan_name=WAN2",
        b"",
    )
    .unwrap();
    assert_eq!(first["data"], second["data"]);
    assert_eq!(io.calls.len(), 1);
    assert_eq!(io.calls[0].2, json!({"wan_name":"WAN2"}));
    assert!(!first.to_string().contains("private-"));
    assert!(
        call(
            &mut f,
            &mut io,
            "/api/features/network/state",
            Method::Get,
            "read=wan_info&wan_name=WAN1&wan_name=WAN2",
            b""
        )
        .is_err()
    );
    assert!(
        call(
            &mut f,
            &mut io,
            "/api/features/network/execute",
            Method::Post,
            "",
            b"{}"
        )
        .is_err()
    );
}
#[test]
fn ddns_add_prepares_without_fetching_nonexistent_detail_then_private_readback() {
    let fixture = Fixture::new();
    let mut f = fixture.open();
    let mut io = Fake::new();
    ready(&mut f, &mut io);
    let input = json!({"id":1,"enable":1,"domain":"example.net","username":"private-user","password":"private-token","wanindex":"WAN1","iptype":"0","checkinterval":5,"forceinterval":24});
    io.reply("addServer", json!({"code":0}));
    let accepted = apply(&mut f, &mut io, "services", "ddns_add", input);
    assert_eq!(io.calls[0].1[1], "addServer");
    assert_eq!(io.calls.len(), 1);
    assert_eq!(accepted["operation"]["state"], "pending");
    io.reply("getServer",json!({"code":0,"domain":"example.net","username":"private-user","password":"private-token","wanindex":"WAN1","iptype":"0","checkinterval":5,"forceinterval":24}));
    io.reply(
        "ddnsStatus",
        json!({"code":0,"list":[{"id":1,"enabled":1}]}),
    );
    tick(&mut f, &mut io);
    assert!(!f.busy());
    let q = format!("id={}", accepted["operation"]["id"].as_str().unwrap());
    let op = call(
        &mut f,
        &mut io,
        "/api/features/operations",
        Method::Get,
        &q,
        b"",
    )
    .unwrap();
    assert_eq!(op["operation"]["state"], "completed");
    assert!(!op.to_string().contains("private-"));
    assert!(
        !fs::read_to_string(fixture.checkpoint_root().join("current.json"))
            .unwrap()
            .contains("private-")
    );
    assert_eq!(
        fs::metadata(fixture.checkpoint_root().join("generation"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}
#[test]
fn optional_wireless_secret_preserved_and_exact_private_equality_is_required() {
    let fixture = Fixture::new();
    let mut f = fixture.open();
    let mut io = Fake::new();
    ready(&mut f, &mut io);
    io.reply("getAllWifiInfo", wifi());
    io.reply("setWifi", json!({"code":0}));
    let accepted = apply(
        &mut f,
        &mut io,
        "wireless",
        "set_wifi",
        json!({"wifiIndex":1,"encryption":"psk2","pwd":""}),
    );
    assert_eq!(io.calls[1].2["pwd"], "exact-secret");
    let mut wrong = wifi();
    wrong["info"][0]["password"] = json!("different-secret");
    io.reply("getAllWifiInfo", wrong);
    tick(&mut f, &mut io);
    assert!(f.busy());
    assert_eq!(
        io.calls
            .iter()
            .filter(|c| c.0 == Program::Vendor && c.1[1] == "setWifi")
            .count(),
        1
    );
    assert!(io.calls.iter().all(|c| c.0 != Program::Service));
    let q = format!("id={}", accepted["operation"]["id"].as_str().unwrap());
    assert_eq!(
        call(
            &mut f,
            &mut io,
            "/api/features/operations",
            Method::Get,
            &q,
            b""
        )
        .unwrap()["operation"]["state"],
        "pending"
    );
    assert!(
        !fs::read_to_string(fixture.checkpoint_root().join("current.json"))
            .unwrap()
            .contains("secret")
    );
}
#[test]
fn scan_verifies_direct_result_not_merely_code_zero() {
    let fixture = Fixture::new();
    let mut f = fixture.open();
    let mut io = Fake::new();
    ready(&mut f, &mut io);
    io.reply("scanMeshNode", json!({"code":0,"list":[]}));
    let accepted = apply(&mut f, &mut io, "wireless", "scan_mesh_node", json!({}));
    assert_eq!(accepted["operation"]["state"], "pending");
    tick(&mut f, &mut io);
    assert!(!f.busy());
    let q = format!("id={}", accepted["operation"]["id"].as_str().unwrap());
    assert_eq!(
        call(
            &mut f,
            &mut io,
            "/api/features/operations",
            Method::Get,
            &q,
            b""
        )
        .unwrap()["operation"]["state"],
        "completed"
    );
    assert_eq!(
        io.calls.iter().filter(|c| c.0 == Program::Vendor).count(),
        1
    );
}
#[test]
fn native_write_sequence_save_failure_retains_operation_and_checkpoint() {
    let fixture = Fixture::new();
    let mut f = fixture.open();
    let mut io = Fake::new();
    ready(&mut f, &mut io);
    io.break_generation = Some(fixture.checkpoint_root().join("generation"));
    io.reply("ddnsReload", json!({"code":0}));
    let accepted = apply(&mut f, &mut io, "services", "ddns_reload", json!({}));
    assert!(f.busy());
    assert_eq!(f.catalog()["generation"], 1);
    assert_eq!(
        accepted["operation"]["error"],
        "feature_storage_unavailable"
    );
    let id = accepted["operation"]["id"].as_str().unwrap();
    assert!(
        fixture
            .checkpoint_root()
            .join(id)
            .join("journal.json")
            .exists()
    );
    tick(&mut f, &mut io);
    assert!(f.busy());
    assert_eq!(
        io.calls
            .iter()
            .filter(|c| c.0 == Program::Vendor && c.1[1] == "ddnsReload")
            .count(),
        1
    );
}
#[test]
fn startup_recovery_uses_all_scope_exit_code_and_preserves_checkpoint_on_reload_failure() {
    let fixture = Fixture::new();
    fixture.put("network", b"original-network");
    fixture.put("dhcp", b"original-dhcp");
    let mut io = Fake::new();
    let flag = AtomicBool::new(false);
    Recovery::prepare_with_paths(
        &fixture.checkpoint_root(),
        "1800000000-2",
        &["network", "dhcp"],
        false,
        &fixture.native,
        &fixture.db,
        &mut io,
        &budget(&flag),
    )
    .unwrap();
    fixture.put("network", b"changed-network");
    fixture.put("dhcp", b"changed-dhcp");
    let mut f = fixture.open();
    assert_eq!(
        fs::read(fixture.native.join("network")).unwrap(),
        b"changed-network"
    );
    io.service_code = 1;
    tick(&mut f, &mut io);
    assert!(f.busy());
    assert!(
        fixture
            .checkpoint_root()
            .join("1800000000-2/journal.json")
            .exists()
    );
    assert_eq!(
        fs::read(fixture.native.join("network")).unwrap(),
        b"original-network"
    );
    assert!(io.calls.iter().any(|c| c.1 == ["network", "reload"]));
    assert!(io.calls.iter().any(|c| c.1 == ["dnsmasq", "reload"]));
    let before = io.calls.len();
    call(
        &mut f,
        &mut io,
        "/api/features/catalog",
        Method::Get,
        "",
        b"",
    )
    .unwrap();
    tick(&mut f, &mut io);
    assert_eq!(
        io.calls.len(),
        before,
        "retry is backed off, GET has no recovery side effect"
    );
    // Restart resets only local backoff. Disk checkpoint remains authoritative.
    io.service_code = 0;
    let mut restarted = fixture.open();
    ready(&mut restarted, &mut io);
    assert!(
        !fixture
            .checkpoint_root()
            .join("1800000000-2/journal.json")
            .exists()
    );
}
#[test]
fn successful_reload_that_changes_restored_bytes_keeps_recovery_armed() {
    let fixture = Fixture::new();
    fixture.put("network", b"original");
    let mut io = Fake::new();
    let flag = AtomicBool::new(false);
    Recovery::prepare_with_paths(
        &fixture.checkpoint_root(),
        "1800000000-2",
        &["network"],
        false,
        &fixture.native,
        &fixture.db,
        &mut io,
        &budget(&flag),
    )
    .unwrap();
    fixture.put("network", b"changed");
    io.mutate_on_reload = Some(fixture.native.join("network"));
    let mut f = fixture.open();
    tick(&mut f, &mut io);
    assert!(f.busy());
    assert!(
        fixture
            .checkpoint_root()
            .join("1800000000-2/journal.json")
            .exists()
    );
}
#[test]
fn root_before_hook_is_after_durable_admission_and_before_vendor_invocation() {
    let fixture = Fixture::new();
    let mut f = fixture.open();
    let mut io = Fake::new();
    ready(&mut f, &mut io);
    let flag = AtomicBool::new(false);
    let body =
        br#"{"actionId":"forward_apply","input":{},"generation":1,"acknowledgeImpact":true}"#;
    let r = f.handle(
        "/api/features/services/apply",
        Method::Post,
        "",
        body,
        &mut io,
        &budget(&flag),
        &mut |impact| {
            assert_eq!(impact, Impact::Network);
            assert!(fixture.checkpoint_root().join("current.json").exists());
            Err(be6500_panel::features::Error {
                status: 409,
                code: "test_cleanup_failed",
                message: "test",
            })
        },
    );
    assert_eq!(r.unwrap_err().code, "test_cleanup_failed");
    assert!(io.calls.is_empty());
    assert!(f.busy());
    tick(&mut f, &mut io);
    assert!(!f.busy());
    assert!(io.calls.iter().all(|c| c.0 != Program::Vendor));
}
#[test]
fn lan_address_waits_for_authenticated_confirm_and_live_listener_native_proof() {
    let fixture = Fixture::new();
    let mut f = fixture.open();
    let mut io = Fake::new();
    ready(&mut f, &mut io);
    io.reply("setLanIp", json!({"code":0}));
    let accepted = apply(
        &mut f,
        &mut io,
        "network",
        "set_lan_ip",
        json!({"ip":"192.168.10.1","mask":"255.255.255.0"}),
    );
    let id = accepted["operation"]["id"].as_str().unwrap();
    let confirm = serde_json::to_vec(&json!({"id":id})).unwrap();
    assert_eq!(
        call(
            &mut f,
            &mut io,
            "/api/features/confirm",
            Method::Post,
            "",
            &confirm
        )
        .unwrap_err()
        .code,
        "reconnect_not_verified"
    );
    // Reconnect through the new manager. Recovery must not revert intended LAN before proof.
    let mut restarted = fixture.open();
    restarted.set_management_listener("192.168.10.1:8787".parse().unwrap());
    io.lan = "192.168.10.1".into();
    io.reply(
        "getLanInfo",
        json!({"code":0,"info":{"ipv4":[{"ip":"192.168.10.1","mask":"255.255.255.0"}]}}),
    );
    let before = io.calls.len();
    tick(&mut restarted, &mut io);
    assert!(restarted.busy());
    assert!(io.calls[before..].iter().all(|c| c.0 != Program::Service));
    let done = call(
        &mut restarted,
        &mut io,
        "/api/features/confirm",
        Method::Post,
        "",
        &confirm,
    )
    .unwrap();
    assert_eq!(done["operation"]["state"], "completed");
    assert_eq!(done["operation"]["reconnectAddress"], "192.168.10.1");
    assert!(!restarted.busy());
}
#[test]
fn runtime_only_forward_reload_uses_native_rules_not_always_false_or_ack() {
    let fixture = Fixture::new();
    let mut f = fixture.open();
    let mut io = Fake::new();
    ready(&mut f, &mut io);
    io.reply("redirectApply", json!({"code":0}));
    io.reply("portForward", json!({"code":0,"status":1,"list":[]}));
    let accepted = apply(&mut f, &mut io, "services", "forward_apply", json!({}));
    tick(&mut f, &mut io);
    assert!(!f.busy());
    assert!(
        io.calls
            .iter()
            .any(|c| c.0 == Program::Iptables && c.1 == ["-t", "nat", "-S"])
    );
    let q = format!("id={}", accepted["operation"]["id"].as_str().unwrap());
    assert_eq!(
        call(
            &mut f,
            &mut io,
            "/api/features/operations",
            Method::Get,
            &q,
            b""
        )
        .unwrap()["operation"]["state"],
        "completed"
    );
}

#[test]
fn interrupted_ordinary_operation_restores_once_at_startup_then_is_failed_not_replayed() {
    let fixture = Fixture::new();
    fixture.put("firewall", b"before-apply");
    let mut f = fixture.open();
    let mut io = Fake::new();
    ready(&mut f, &mut io);
    io.reply("redirectApply", json!({"code":0}));
    let accepted = apply(&mut f, &mut io, "services", "forward_apply", json!({}));
    fixture.put("firewall", b"partial-change");
    let before = io.calls.len();
    let mut restarted = fixture.open();
    tick(&mut restarted, &mut io);
    assert!(!restarted.busy());
    assert_eq!(
        fs::read(fixture.native.join("firewall")).unwrap(),
        b"before-apply"
    );
    assert!(io.calls[before..].iter().all(|c| c.0 == Program::Service));
    assert_eq!(
        io.calls
            .iter()
            .filter(|c| c.0 == Program::Vendor && c.1[1] == "redirectApply")
            .count(),
        1
    );
    let q = format!("id={}", accepted["operation"]["id"].as_str().unwrap());
    let op = call(
        &mut restarted,
        &mut io,
        "/api/features/operations",
        Method::Get,
        &q,
        b"",
    )
    .unwrap();
    assert_eq!(op["operation"]["state"], "failed");
    assert_eq!(op["operation"]["error"], "operation_interrupted");
    let calls = io.calls.len();
    tick(&mut restarted, &mut io);
    assert_eq!(calls, io.calls.len());
}
#[test]
fn pending_lane_checks_json_and_cas_before_busy_and_never_repeats_action() {
    let fixture = Fixture::new();
    let mut f = fixture.open();
    let mut io = Fake::new();
    ready(&mut f, &mut io);
    io.reply("ddnsReload", json!({"code":0}));
    apply(&mut f, &mut io, "services", "ddns_reload", json!({}));
    let calls = io.calls.len();
    assert_eq!(
        call(
            &mut f,
            &mut io,
            "/api/features/services/apply",
            Method::Post,
            "",
            b"{"
        )
        .unwrap_err()
        .status,
        400
    );
    let stale = br#"{"actionId":"ddns_reload","input":{},"generation":1}"#;
    assert_eq!(
        call(
            &mut f,
            &mut io,
            "/api/features/services/apply",
            Method::Post,
            "",
            stale
        )
        .unwrap_err()
        .code,
        "generation_conflict"
    );
    let current = br#"{"actionId":"ddns_reload","input":{},"generation":2}"#;
    assert_eq!(
        call(
            &mut f,
            &mut io,
            "/api/features/services/apply",
            Method::Post,
            "",
            current
        )
        .unwrap_err()
        .code,
        "operation_busy"
    );
    assert_eq!(calls, io.calls.len());
}
#[test]
fn multi_mac_unbind_polls_unfiltered_table_with_exact_empty_parameters() {
    let fixture = Fixture::new();
    let mut f = fixture.open();
    let mut io = Fake::new();
    ready(&mut f, &mut io);
    io.reply("macUnbind", json!({"code":0}));
    let accepted = apply(
        &mut f,
        &mut io,
        "network",
        "mac_unbind",
        json!({"mac":"02:11:22:33:44:55,02:11:22:33:44:66"}),
    );
    io.reply("getMacBindInfo", json!({"code":0,"list":[]}));
    tick(&mut f, &mut io);
    assert!(!f.busy());
    assert_eq!(io.calls.last().unwrap().1[1], "getMacBindInfo");
    assert_eq!(io.calls.last().unwrap().2, json!({}));
    let q = format!("id={}", accepted["operation"]["id"].as_str().unwrap());
    assert_eq!(
        call(
            &mut f,
            &mut io,
            "/api/features/operations",
            Method::Get,
            &q,
            b""
        )
        .unwrap()["operation"]["state"],
        "completed"
    );
}
#[test]
fn reboot_requires_new_boot_and_explicit_confirm_not_code_zero() {
    let fixture = Fixture::new();
    let mut f = fixture.open();
    let mut io = Fake::new();
    ready(&mut f, &mut io);
    f.set_management_listener("192.168.31.1:8787".parse().unwrap());
    io.reply("reboot", json!({"code":0}));
    let accepted = apply(
        &mut f,
        &mut io,
        "services",
        "reboot",
        json!({"client":"web"}),
    );
    io.reply(
        "getLanInfo",
        json!({"code":0,"info":{"ipv4":[{"ip":"192.168.31.1","mask":"255.255.255.0"}]}}),
    );
    let confirm = serde_json::to_vec(&json!({"id":accepted["operation"]["id"]})).unwrap();
    assert_eq!(
        call(
            &mut f,
            &mut io,
            "/api/features/confirm",
            Method::Post,
            "",
            &confirm
        )
        .unwrap_err()
        .code,
        "reconnect_not_verified"
    );
    io.boot = "22222222-2222-2222-2222-222222222222".into();
    assert_eq!(
        call(
            &mut f,
            &mut io,
            "/api/features/confirm",
            Method::Post,
            "",
            &confirm
        )
        .unwrap()["operation"]["state"],
        "completed"
    );
    assert!(io.calls.iter().all(|c| c.0 != Program::Service));
}
#[test]
fn rejected_native_write_durable_task_retained_and_restoration_reads_original_bytes() {
    let fixture = Fixture::new();
    fixture.put("firewall", b"original");
    let mut f = fixture.open();
    let mut io = Fake::new();
    ready(&mut f, &mut io);
    io.reply(
        "redirectApply",
        json!({"code":17,"msg":"private-vendor-message"}),
    );
    let accepted = apply(&mut f, &mut io, "services", "forward_apply", json!({}));
    assert_eq!(accepted["operation"]["error"], "vendor_rejected");
    assert!(!accepted.to_string().contains("private-vendor"));
    fixture.put("firewall", b"partial");
    tick(&mut f, &mut io);
    assert!(!f.busy());
    assert_eq!(
        fs::read(fixture.native.join("firewall")).unwrap(),
        b"original"
    );
    let q = format!("id={}", accepted["operation"]["id"].as_str().unwrap());
    assert_eq!(
        call(
            &mut f,
            &mut io,
            "/api/features/operations",
            Method::Get,
            &q,
            b""
        )
        .unwrap()["operation"]["state"],
        "failed"
    );
}

fn saved_checkpoint(fixture: &Fixture, io: &mut Fake, configs: &[&str]) {
    for name in configs {
        fixture.put(name, format!("original-{name}").as_bytes());
    }
    let flag = AtomicBool::new(false);
    Recovery::prepare_with_paths(
        &fixture.checkpoint_root(),
        "1800000000-2",
        configs,
        false,
        &fixture.native,
        &fixture.db,
        io,
        &budget(&flag),
    )
    .unwrap();
    for name in configs {
        fixture.put(name, format!("changed-{name}").as_bytes());
    }
}
fn native_calls(io: &Fake) -> Vec<(Program, Vec<String>)> {
    io.calls
        .iter()
        .map(|(p, args, _)| (*p, args.clone()))
        .collect()
}
fn native_call(program: Program, args: &[&str]) -> (Program, Vec<String>) {
    (program, args.iter().map(|s| (*s).into()).collect())
}
#[test]
fn current_factory_recovery_calls_only_actual_consumers_for_full_scope() {
    let fixture = Fixture::new();
    let mut io = Fake::new();
    let configs = [
        "port_service",
        "port_map",
        "network",
        "ipv6",
        "dhcp",
        "macbind",
        "firewall",
        "mwan3",
        "wireless",
        "misc",
        "ddns",
        "miqos",
        "hwnat",
        "upnpd",
        "nginx",
        "system",
        "webfilter",
        "mipctl_user",
        "miscan",
        "otapred",
        "backup",
    ];
    saved_checkpoint(&fixture, &mut io, &configs);
    io.scalar_values
        .insert("miscan.config.enabled".into(), "1".into());
    let mut f = fixture.open();
    ready(&mut f, &mut io);
    assert_eq!(
        native_calls(&io),
        vec![
            native_call(Program::Service, &["port_service", "restart"]),
            native_call(Program::Service, &["network", "reload"]),
            native_call(Program::Service, &["wifi", "reload"]),
            native_call(Program::Service, &["dnsmasq", "reload"]),
            native_call(Program::Service, &["firewall", "reload"]),
            native_call(Program::Service, &["mwan3", "reload"]),
            native_call(Program::Service, &["ddns", "reload"]),
            native_call(Program::Service, &["miqos", "reload"]),
            native_call(Program::Service, &["miniupnpd", "reload"]),
            native_call(Program::Service, &["nginx", "reload"]),
            native_call(Program::Service, &["system", "reload"]),
            native_call(Program::Uci, &["-q", "get", "miscan.config.enabled"]),
            native_call(Program::Service, &["scan", "start"]),
            native_call(
                Program::Ubus,
                &["call", "uci", "commit", r#"{"config":"mipctl_user"}"#]
            ),
        ]
    );
    for name in configs {
        assert_eq!(
            fs::read(fixture.native.join(name)).unwrap(),
            format!("original-{name}").as_bytes()
        );
    }
    assert!(
        !fixture
            .checkpoint_root()
            .join("1800000000-2/journal.json")
            .exists()
    );
}
#[test]
fn webfilter_recovery_proves_saved_bytes_without_any_phantom_reload() {
    let fixture = Fixture::new();
    let mut io = Fake::new();
    saved_checkpoint(&fixture, &mut io, &["webfilter"]);
    let mut f = fixture.open();
    ready(&mut f, &mut io);
    assert!(io.calls.is_empty());
    assert_eq!(
        fs::read(fixture.native.join("webfilter")).unwrap(),
        b"original-webfilter"
    );
}
#[test]
fn scan_recovery_uses_restored_switch_and_native_absent_default() {
    for enabled in [Some("0"), None] {
        let fixture = Fixture::new();
        let mut io = Fake::new();
        saved_checkpoint(&fixture, &mut io, &["miscan"]);
        if let Some(enabled) = enabled {
            io.scalar_values
                .insert("miscan.config.enabled".into(), enabled.into());
        }
        let mut f = fixture.open();
        ready(&mut f, &mut io);
        assert_eq!(
            native_calls(&io),
            vec![
                native_call(Program::Uci, &["-q", "get", "miscan.config.enabled"]),
                native_call(Program::Service, &["scan", "stop"]),
            ]
        );
    }
}
#[test]
fn missing_native_consumer_or_bad_scan_switch_keeps_recovery_armed() {
    for (config, unavailable, scalar) in [
        (
            "miscan",
            Some(native_call(Program::Service, &["scan", "start"])),
            "1",
        ),
        (
            "miscan",
            Some(native_call(
                Program::Uci,
                &["-q", "get", "miscan.config.enabled"],
            )),
            "1",
        ),
        ("miscan", None, "invalid"),
        (
            "mipctl_user",
            Some(native_call(
                Program::Ubus,
                &["call", "uci", "commit", r#"{"config":"mipctl_user"}"#],
            )),
            "1",
        ),
    ] {
        let fixture = Fixture::new();
        let mut io = Fake::new();
        saved_checkpoint(&fixture, &mut io, &[config]);
        io.scalar_values
            .insert("miscan.config.enabled".into(), scalar.into());
        io.unavailable_call = unavailable;
        let mut f = fixture.open();
        tick(&mut f, &mut io);
        assert!(f.busy());
        assert_eq!(f.catalog()["writeError"], "restore_reload_failed");
        assert!(
            fixture
                .checkpoint_root()
                .join("1800000000-2/journal.json")
                .exists()
        );
        assert_eq!(
            fs::read(fixture.native.join(config)).unwrap(),
            format!("original-{config}").as_bytes()
        );
        assert!(io.calls.iter().all(|c| c.0 != Program::Vendor));
    }
}
fn interrupted_local_action(
    fixture: &Fixture,
    io: &mut Fake,
    action: &str,
    handler: &str,
    input: Value,
) {
    fixture.put("xiaoqiang", b"original-local-settings");
    let mut f = fixture.open();
    ready(&mut f, io);
    io.reply(handler, json!({"code":17}));
    apply(&mut f, io, "services", action, input);
    fixture.put("xiaoqiang", b"partially-changed-local-settings");
    io.calls.clear();
}
#[test]
fn router_name_recovery_notifies_exact_native_consumers_without_network_reload() {
    let fixture = Fixture::new();
    let mut io = Fake::new();
    interrupted_local_action(
        &fixture,
        &mut io,
        "router_name",
        "setRouterName",
        json!({"name":"Home"}),
    );
    let mut restarted = fixture.open();
    ready(&mut restarted, &mut io);
    assert_eq!(
        native_calls(&io),
        vec![native_call(
            Program::Ubus,
            &["call", "xq_info_sync_mqtt", "topo_changed", "{}"]
        ),]
    );
    assert_eq!(
        fs::read(fixture.native.join("xiaoqiang")).unwrap(),
        b"original-local-settings"
    );
}
#[test]
fn status_led_recovery_reapplies_restored_switch_and_timer_with_fixed_primitives() {
    let fixture = Fixture::new();
    let mut io = Fake::new();
    interrupted_local_action(&fixture, &mut io, "led_set", "ledCtl", json!({"on":1}));
    for (key, value) in [
        ("BLUE_LED", "0"),
        ("BLUE_LED_TIMER", "1"),
        ("BLUE_LED_TIMER_OPEN", "23:04"),
        ("BLUE_LED_TIMER_CLOSE", "07:35"),
    ] {
        io.scalar_values
            .insert(format!("xiaoqiang.common.{key}"), value.into());
    }
    let mut restarted = fixture.open();
    ready(&mut restarted, &mut io);
    assert_eq!(
        native_calls(&io),
        vec![
            native_call(Program::Uci, &["-q", "get", "xiaoqiang.common.BLUE_LED"]),
            native_call(Program::Service, &["led_ctl", "led_off"]),
            native_call(
                Program::Uci,
                &["-q", "get", "xiaoqiang.common.BLUE_LED_TIMER"]
            ),
            native_call(
                Program::Uci,
                &["-q", "get", "xiaoqiang.common.BLUE_LED_TIMER_OPEN"]
            ),
            native_call(
                Program::Uci,
                &["-q", "get", "xiaoqiang.common.BLUE_LED_TIMER_CLOSE"]
            ),
            native_call(
                Program::Service,
                &["led_ctl", "timer_on", "23", "04", "07", "35"]
            ),
        ]
    );
    assert_eq!(
        fs::read(fixture.native.join("xiaoqiang")).unwrap(),
        b"original-local-settings"
    );
}
#[test]
fn ethernet_led_recovery_keeps_native_target_and_absent_defaults() {
    let fixture = Fixture::new();
    let mut io = Fake::new();
    interrupted_local_action(
        &fixture,
        &mut io,
        "eth_led_set",
        "DoEthLED",
        json!({"on":0}),
    );
    let mut restarted = fixture.open();
    ready(&mut restarted, &mut io);
    assert_eq!(
        native_calls(&io),
        vec![
            native_call(Program::Uci, &["-q", "get", "xiaoqiang.common.ETHLED"]),
            native_call(Program::Service, &["led_ctl", "led_on", "ethled"]),
            native_call(
                Program::Uci,
                &["-q", "get", "xiaoqiang.common.ETHLED_TIMER"]
            ),
            native_call(Program::Service, &["led_ctl", "timer_off", "ethled"]),
        ]
    );
}
#[test]
fn all_led_recovery_restores_distinct_switches_not_aggregate_status() {
    let fixture = Fixture::new();
    let mut io = Fake::new();
    interrupted_local_action(
        &fixture,
        &mut io,
        "all_led_set",
        "DoAllLED",
        json!({"on":1}),
    );
    for (key, value) in [("BLUE_LED", "0"), ("ETHLED", "1"), ("XLED", "0")] {
        io.scalar_values
            .insert(format!("xiaoqiang.common.{key}"), value.into());
    }
    let mut restarted = fixture.open();
    ready(&mut restarted, &mut io);
    assert_eq!(
        native_calls(&io),
        vec![
            native_call(Program::Uci, &["-q", "get", "xiaoqiang.common.BLUE_LED"]),
            native_call(Program::Service, &["led_ctl", "led_off"]),
            native_call(Program::Uci, &["-q", "get", "xiaoqiang.common.ETHLED"]),
            native_call(Program::Service, &["led_ctl", "led_on", "ethled"]),
            native_call(Program::Uci, &["-q", "get", "xiaoqiang.common.XLED"]),
            native_call(Program::Service, &["led_ctl", "led_off", "xled"]),
        ]
    );
}
#[test]
fn invalid_restored_led_timer_or_unavailable_helper_retains_checkpoint() {
    for unavailable in [false, true] {
        let fixture = Fixture::new();
        let mut io = Fake::new();
        interrupted_local_action(&fixture, &mut io, "led_set", "ledCtl", json!({"on":0}));
        io.scalar_values
            .insert("xiaoqiang.common.BLUE_LED_TIMER".into(), "1".into());
        io.scalar_values.insert(
            "xiaoqiang.common.BLUE_LED_TIMER_OPEN".into(),
            "24:00".into(),
        );
        if unavailable {
            io.unavailable_call = Some(native_call(Program::Service, &["led_ctl", "led_on"]));
        }
        let mut restarted = fixture.open();
        tick(&mut restarted, &mut io);
        assert!(restarted.busy());
        assert_eq!(restarted.catalog()["writeError"], "restore_reload_failed");
        assert!(
            fs::read_dir(fixture.checkpoint_root())
                .unwrap()
                .any(|entry| { entry.unwrap().path().join("journal.json").exists() })
        );
        assert!(!io.calls.iter().any(|c| c.0 == Program::Vendor));
        assert!(
            !io.calls
                .iter()
                .any(|c| c.1.get(1).is_some_and(|v| v == "timer_on"))
        );
    }
}

#[test]
fn fresh_maintenance_getter_uses_private_application_root_without_initializing_storage() {
    let f = Fixture::new();
    let mut gateway = f.open();
    let mut io = Fake::new();
    ready(&mut gateway, &mut io);
    let value = call(
        &mut gateway,
        &mut io,
        "/api/features/services/state",
        Method::Get,
        "read=scheduled_reboot",
        b"",
    )
    .unwrap();
    assert_eq!(value["data"]["enabled"], false);
    assert_eq!(value["data"]["timeBasis"], "router");
    assert!(!f.data.join("maintenance-schedule.json").exists());
    assert!(io.calls.iter().all(|(p, _, _)| *p != Program::Service));
}

#[test]
fn same_manager_lan_tick_proposes_exact_rebind_but_still_requires_confirm() {
    let fixture = Fixture::new();
    let mut f = fixture.open();
    let mut io = Fake::new();
    ready(&mut f, &mut io);
    f.set_management_listener("192.168.31.1:8787".parse().unwrap());
    let accepted = apply(
        &mut f,
        &mut io,
        "network",
        "set_lan_ip",
        json!({"ip":"192.168.10.1","mask":"255.255.255.0"}),
    );
    io.lan = "192.168.10.1".into();
    io.reply(
        "getLanInfo",
        json!({"code":0,"info":{"ipv4":[{"ip":"192.168.10.1","mask":"255.255.255.0"}]}}),
    );
    tick(&mut f, &mut io);
    let target = "192.168.10.1:8787".parse().unwrap();
    assert_eq!(f.management_rebind_address(), Some(target));
    assert_eq!(
        f.catalog()["pendingOperation"]["id"],
        accepted["operation"]["id"]
    );
    assert_eq!(f.catalog()["pendingOperation"]["canConfirm"], false);
    let confirm = serde_json::to_vec(&json!({"id":accepted["operation"]["id"]})).unwrap();
    assert_eq!(
        call(
            &mut f,
            &mut io,
            "/api/features/confirm",
            Method::Post,
            "",
            &confirm
        )
        .unwrap_err()
        .code,
        "reconnect_not_verified"
    );
    // The existing serve lane publishes this only AFTER successful exact bind.
    // No Features reopen, runtime recreation or restoration is involved.
    f.set_management_listener(target);
    let done = call(
        &mut f,
        &mut io,
        "/api/features/confirm",
        Method::Post,
        "",
        &confirm,
    )
    .unwrap();
    assert_eq!(done["operation"]["state"], "completed");
    assert!(!f.busy());
    assert!(f.catalog().get("pendingOperation").is_none());
    assert!(f.management_rebind_address().is_none());
    assert!(io.calls.iter().all(|c| c.0 != Program::Service));
}

#[test]
fn missing_kernel_or_native_lan_proof_never_offers_rebind() {
    for kernel_missing in [true, false] {
        let fixture = Fixture::new();
        let mut f = fixture.open();
        let mut io = Fake::new();
        ready(&mut f, &mut io);
        f.set_management_listener("192.168.31.1:8787".parse().unwrap());
        apply(
            &mut f,
            &mut io,
            "network",
            "set_lan_ip",
            json!({"ip":"192.168.10.1","mask":"255.255.255.0"}),
        );
        io.lan = "192.168.10.1".into();
        if kernel_missing {
            io.kernel_lan = Some("192.168.31.1".into());
            io.reply(
                "getLanInfo",
                json!({"code":0,"info":{"ipv4":[{"ip":"192.168.10.1","mask":"255.255.255.0"}]}}),
            );
        }
        tick(&mut f, &mut io);
        assert!(f.management_rebind_address().is_none());
        assert_eq!(f.catalog()["pendingOperation"]["canConfirm"], false);
    }
}

#[test]
fn ap_dhcp_hostip_requires_mode_and_kernel_and_native_lan_not_stale_static_uci() {
    let fixture = Fixture::new();
    let mut f = fixture.open();
    let mut io = Fake::new();
    ready(&mut f, &mut io);
    f.set_management_listener("192.168.31.1:8787".parse().unwrap());
    apply(&mut f, &mut io, "network", "set_lan_ap", json!({}));
    io.scalar_values
        .insert("xiaoqiang.common.NETMODE".into(), "lanapmode".into());
    io.reply(
        "getMode",
        json!({"code":0,"mode":2,"hostip":"192.168.20.2"}),
    );
    io.kernel_lan = Some("192.168.20.2".into());
    io.reply(
        "getLanInfo",
        json!({"code":0,"info":{"ipv4":[{"ip":"192.168.20.2","mask":"255.255.255.0"}]}}),
    );
    tick(&mut f, &mut io);
    assert_eq!(
        f.management_rebind_address(),
        Some("192.168.20.2:8787".parse().unwrap())
    );
    assert_eq!(io.lan, "192.168.31.1");
}

#[test]
fn nonpending_ticks_and_catalog_gets_do_not_collect_reconnect_evidence() {
    let fixture = Fixture::new();
    let mut f = fixture.open();
    let mut io = Fake::new();
    ready(&mut f, &mut io);
    io.calls.clear();
    for _ in 0..30 {
        tick(&mut f, &mut io);
        assert!(f.management_rebind_address().is_none());
        let catalog = call(
            &mut f,
            &mut io,
            "/api/features/catalog",
            Method::Get,
            "",
            b"",
        )
        .unwrap();
        assert!(catalog.get("pendingOperation").is_none());
    }
    assert!(io.calls.is_empty());
}

#[test]
fn catalog_returns_only_public_pending_lifecycle_and_get_never_proves_reconnect() {
    let fixture = Fixture::new();
    let mut f = fixture.open();
    let mut io = Fake::new();
    ready(&mut f, &mut io);
    let accepted = apply(
        &mut f,
        &mut io,
        "network",
        "set_lan_ip",
        json!({"ip":"192.168.10.1","mask":"255.255.255.0"}),
    );
    io.calls.clear();
    let catalog = call(
        &mut f,
        &mut io,
        "/api/features/catalog",
        Method::Get,
        "",
        b"",
    )
    .unwrap();
    assert_eq!(catalog["pendingOperation"], accepted["operation"]);
    for key in [
        "input",
        "reconnect",
        "bootId",
        "targetMask",
        "previousVersion",
        "nativeAttempted",
        "phase",
    ] {
        assert!(catalog["pendingOperation"].get(key).is_none());
    }
    assert!(io.calls.is_empty());
    assert!(f.management_rebind_address().is_none());
    let reopened = fixture.open();
    assert_eq!(
        reopened.catalog()["pendingOperation"]["id"],
        accepted["operation"]["id"]
    );
}

fn ota_ready(fixture: &Fixture) -> (Features, Fake) {
    let mut f = fixture.open();
    let mut io = Fake::new();
    ready(&mut f, &mut io);
    f.set_management_listener("192.168.31.1:8787".parse().unwrap());
    io.reply("getInitInfo", json!({"code":0,"romversion":"1.0.0"}));
    (f, io)
}

#[test]
fn official_ota_explicit_native_rejections_settle_without_restore_or_custom_input() {
    for code in [1568, 1577, 1523] {
        let fixture = Fixture::new();
        fixture.put("network", b"original-network");
        let (mut f, mut io) = ota_ready(&fixture);
        io.reply("upgradeRom", json!({"code":code}));
        let rejected = apply(&mut f, &mut io, "services", "official_upgrade", json!({}));
        assert_eq!(rejected["operation"]["state"], "failed");
        assert_eq!(rejected["operation"]["error"], "vendor_rejected");
        assert!(!f.busy());
        assert!(
            io.calls
                .iter()
                .any(|c| c.0 == Program::Vendor && c.1[1] == "upgradeRom" && c.2 == json!({}))
        );
        assert!(io.calls.iter().all(|c| c.0 != Program::Service));
        assert_eq!(
            fs::read(fixture.native.join("network")).unwrap(),
            b"original-network"
        );
        assert!(
            !fixture
                .checkpoint_root()
                .join("1800000000-2/journal.json")
                .exists()
        );
        let mut reopened = fixture.open();
        io.calls.clear();
        ready(&mut reopened, &mut io);
        assert!(io.calls.is_empty());
    }
}

#[test]
fn official_ota_no_flash_terminal_status_discards_without_uci_rollback() {
    for (status, error) in [
        (6, "official_upgrade_no_update"),
        (7, "official_upgrade_missing_metadata"),
        (8, "official_upgrade_download_failed"),
        (9, "official_upgrade_image_invalid"),
        (10, "official_upgrade_secboot_failed"),
    ] {
        let fixture = Fixture::new();
        fixture.put("network", b"original-network");
        let (mut f, mut io) = ota_ready(&fixture);
        apply(&mut f, &mut io, "services", "official_upgrade", json!({}));
        fixture.put("network", b"unchanged-by-failed-download");
        io.reply(
            "upgradeStatus",
            json!({"code":0,"status":status,"percent":0}),
        );
        tick(&mut f, &mut io);
        assert!(!f.busy());
        assert!(f.catalog().get("pendingOperation").is_none());
        let q = "id=1800000000-2";
        let result = call(
            &mut f,
            &mut io,
            "/api/features/operations",
            Method::Get,
            q,
            b"",
        )
        .unwrap();
        assert_eq!(result["operation"]["state"], "failed");
        assert_eq!(result["operation"]["error"], error);
        assert!(io.calls.iter().all(|c| c.0 != Program::Service));
        assert_eq!(
            fs::read(fixture.native.join("network")).unwrap(),
            b"unchanged-by-failed-download"
        );
        assert!(
            !fixture
                .checkpoint_root()
                .join("1800000000-2/journal.json")
                .exists()
        );
    }
}

#[test]
fn official_ota_progress_unknown_and_nonack_getter_stay_pending() {
    for reply in [
        json!({"code":0,"status":1}),
        json!({"code":0,"status":2}),
        json!({"code":0,"status":3}),
        json!({"code":0,"status":4}),
        json!({"code":0,"status":5}),
        json!({"code":0,"status":11}),
        json!({"code":0,"status":99}),
        json!({"code":0}),
        json!({"code":8,"status":8}),
    ] {
        let fixture = Fixture::new();
        let (mut f, mut io) = ota_ready(&fixture);
        apply(&mut f, &mut io, "services", "official_upgrade", json!({}));
        io.reply("upgradeStatus", reply);
        tick(&mut f, &mut io);
        assert!(f.busy());
        assert_eq!(f.catalog()["pendingOperation"]["state"], "pending");
        assert_eq!(f.catalog()["pendingOperation"]["canConfirm"], false);
        assert!(io.calls.iter().all(|c| c.0 != Program::Service));
    }
}

#[test]
fn official_ota_reply_lost_or_unknown_does_not_restore_and_changed_boot_needs_version_confirm() {
    for lost in [true, false] {
        let fixture = Fixture::new();
        let (mut f, mut io) = ota_ready(&fixture);
        if lost {
            io.unavailable_call = Some(native_call(Program::Vendor, &["xqsystem", "upgradeRom"]));
        } else {
            io.reply("upgradeRom", json!({"unexpected":"output"}));
        }
        let accepted = apply(&mut f, &mut io, "services", "official_upgrade", json!({}));
        assert_eq!(accepted["operation"]["state"], "pending");
        io.unavailable_call = None;
        io.boot = "22222222-2222-2222-2222-222222222222".into();
        // A stale terminal download status from the old boot must not settle now.
        io.reply("upgradeStatus", json!({"code":0,"status":8}));
        io.reply(
            "getLanInfo",
            json!({"code":0,"info":{"ipv4":[{"ip":"192.168.31.1","mask":"255.255.255.0"}]}}),
        );
        tick(&mut f, &mut io);
        assert!(f.busy());
        let confirm = serde_json::to_vec(&json!({"id":accepted["operation"]["id"]})).unwrap();
        assert_eq!(
            call(
                &mut f,
                &mut io,
                "/api/features/confirm",
                Method::Post,
                "",
                &confirm
            )
            .unwrap_err()
            .code,
            "reconnect_not_verified"
        );
        io.reply("getInitInfo", json!({"code":0,"romversion":"1.0.1"}));
        let done = call(
            &mut f,
            &mut io,
            "/api/features/confirm",
            Method::Post,
            "",
            &confirm,
        )
        .unwrap();
        assert_eq!(done["operation"]["state"], "completed");
        assert!(!f.busy());
        assert!(io.calls.iter().all(|c| c.0 != Program::Service));
    }
}

#[test]
fn parental_devices_recovery_applies_exact_native_consumer_and_verifies_saved_bytes() {
    for unavailable in [false, true] {
        let fixture = Fixture::new();
        let mut io = Fake::new();
        saved_checkpoint(&fixture, &mut io, &["mipctl_user", "parentalctl"]);
        if unavailable {
            io.unavailable_call = Some(native_call(Program::Service, &["parentalctl", "apply"]));
        }
        let mut f = fixture.open();
        tick(&mut f, &mut io);
        assert_eq!(
            native_calls(&io),
            vec![
                native_call(Program::Service, &["parentalctl", "apply"]),
                native_call(
                    Program::Ubus,
                    &["call", "uci", "commit", r#"{"config":"mipctl_user"}"#]
                ),
            ]
        );
        for name in ["mipctl_user", "parentalctl"] {
            assert_eq!(
                fs::read(fixture.native.join(name)).unwrap(),
                format!("original-{name}").as_bytes()
            );
        }
        if unavailable {
            assert!(f.busy());
            assert_eq!(f.catalog()["writeError"], "restore_reload_failed");
            assert!(
                fixture
                    .checkpoint_root()
                    .join("1800000000-2/journal.json")
                    .exists()
            );
        } else {
            assert!(!f.busy());
            assert!(
                !fixture
                    .checkpoint_root()
                    .join("1800000000-2/journal.json")
                    .exists()
            );
        }
    }
}

#[test]
fn wildcard_listener_never_satisfies_authenticated_reconnect_proof() {
    let fixture = Fixture::new();
    let mut f = fixture.open();
    let mut io = Fake::new();
    ready(&mut f, &mut io);
    f.set_management_listener("0.0.0.0:8787".parse().unwrap());
    let accepted = apply(
        &mut f,
        &mut io,
        "network",
        "set_lan_ip",
        json!({"ip":"192.168.10.1","mask":"255.255.255.0"}),
    );
    io.lan = "192.168.10.1".into();
    io.reply(
        "getLanInfo",
        json!({"code":0,"info":{"ipv4":[{"ip":"192.168.10.1","mask":"255.255.255.0"}]}}),
    );
    tick(&mut f, &mut io);
    assert!(f.management_rebind_address().is_none());
    let confirm = serde_json::to_vec(&json!({"id":accepted["operation"]["id"]})).unwrap();
    assert_eq!(
        call(
            &mut f,
            &mut io,
            "/api/features/confirm",
            Method::Post,
            "",
            &confirm
        )
        .unwrap_err()
        .code,
        "reconnect_not_verified"
    );
}

//! Synthetic NETWORK contracts. No live firmware, shell, Go, or device action.
use be6500_panel::{
    features::{self, FieldKind, Impact},
    features_network::{DOMAIN, prepare_input, project, validate, verify, verify_private},
    product_io::{Backend, Error, Output, Program},
    readiness_tun::Budget,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeSet, VecDeque},
    path::Path,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

static NEVER_CANCEL: AtomicBool = AtomicBool::new(false);
fn budget() -> Budget<'static> {
    Budget {
        deadline: Instant::now() + Duration::from_secs(5),
        cancel: &NEVER_CANCEL,
    }
}
struct FakeBackend {
    replies: VecDeque<Value>,
    calls: Vec<(Program, Vec<String>, Value)>,
}
impl FakeBackend {
    fn new(replies: Vec<Value>) -> Self {
        Self {
            replies: replies.into(),
            calls: vec![],
        }
    }
}
impl Backend for FakeBackend {
    fn read(&mut self, _: &Path, _: usize, _: &Budget<'_>) -> Result<Vec<u8>, Error> {
        panic!("NETWORK must not open arbitrary native files")
    }
    fn run(
        &mut self,
        p: Program,
        args: &[String],
        stdin: Option<&[u8]>,
        limit: usize,
        _: &Budget<'_>,
    ) -> Result<Output, Error> {
        assert_eq!(p, Program::Vendor);
        assert_eq!(args.len(), 2);
        assert_eq!(limit, 256 << 10);
        let input: Value =
            serde_json::from_slice(stdin.expect("private JSON belongs on stdin")).unwrap();
        self.calls.push((p, args.to_vec(), input));
        Ok(Output {
            code: 0,
            stdout: serde_json::to_vec(&self.replies.pop_front().expect("registered reply"))
                .unwrap(),
            stderr: vec![],
        })
    }
    fn now_unix(&self) -> u64 {
        1_700_000_000
    }
}
fn read(id: &str) -> &'static be6500_panel::features::Read {
    DOMAIN.reads.iter().find(|r| r.id == id).unwrap()
}
fn action(id: &str) -> &'static be6500_panel::features::Action {
    DOMAIN.actions.iter().find(|a| a.id == id).unwrap()
}
fn keys(id: &str) -> Vec<&'static str> {
    action(id).fields.iter().map(|f| f.key).collect()
}

#[test]
fn descriptor_ids_fields_recovery_and_readbacks_are_unique_and_fixed() {
    assert_eq!(DOMAIN.id, "network");
    assert_eq!(DOMAIN.reads.len(), 24);
    assert_eq!(DOMAIN.actions.len(), 32);
    let mut reads = BTreeSet::new();
    let mut actions = BTreeSet::new();
    for r in DOMAIN.reads {
        assert!(reads.insert(r.id));
        assert!(!r.title.is_empty());
        assert!(["xqnetwork", "misystem"].contains(&r.controller));
        assert!(r.handler.bytes().all(|b| b.is_ascii_alphanumeric()));
        let mut fields = BTreeSet::new();
        for f in r.fields {
            assert!(fields.insert(f.key));
        }
    }
    for a in DOMAIN.actions {
        assert!(actions.insert(a.id));
        assert!(reads.contains(a.readback));
        assert!(!a.configs.is_empty());
        assert!(!a.title.is_empty());
        let mut fields = BTreeSet::new();
        for f in a.fields {
            assert!(fields.insert(f.key));
            if f.kind == FieldKind::Select {
                assert!(!f.options.is_empty());
            }
            if f.kind == FieldKind::Integer {
                assert!(f.min.unwrap() <= f.max.unwrap());
            }
        }
        assert!(!a.configs.contains(&"system"));
    }
    for id in [
        "set_lan_ip",
        "set_lan_ap",
        "disable_lan_ap",
        "set_wifi_ap",
        "disable_wifi_ap",
    ] {
        assert_eq!(action(id).impact, Impact::Maintenance);
    }
    assert!(action("set_wan6").configs.contains(&"ipv6"));
    assert!(action("set_wan6").configs.contains(&"dhcp"));
    assert!(action("ps_multiwan").configs.contains(&"miqos"));
    assert!(action("ps_multiwan").configs.contains(&"port_map"));
    assert!(action("mac_bind").configs.contains(&"devicelist"));
    assert!(!action("mac_bind").configs.contains(&"deviceinfo"));
    // These registered shared-model handlers execute switch2.5Gwan.sh, absent
    // from RN02. getWanLanPort also writes, so none belongs in our read list.
    assert!(
        !DOMAIN
            .reads
            .iter()
            .any(|r| ["getWanLanPort", "getAutoWanLink", "getAutoWanType"].contains(&r.handler))
    );
    assert!(
        !DOMAIN
            .actions
            .iter()
            .any(|a| ["setWanLanPort", "setWanLanSwap"].contains(&a.handler))
    );
}

#[test]
fn registered_factory_handler_names_and_form_keys_are_exact() {
    let mappings = [
        ("wan_info", "xqnetwork", "getWanInfo"),
        ("pppoe_status", "xqnetwork", "pppoeStatus"),
        ("wan6_config", "xqnetwork", "getWan6V2"),
        ("wan6_status", "xqnetwork", "getWan6InfoV2"),
        ("ipmac_check", "xqnetwork", "getIPMACCheckStatus"),
        ("port_map", "misystem", "getPSMap"),
        ("port_service", "misystem", "getPSService"),
        ("vlan_internet", "misystem", "getVlanInternet"),
        ("vlan_iptv", "misystem", "getVlanIPTV"),
        ("netmode", "xqnetwork", "getNetMode"),
        ("bridge_lan", "xqnetwork", "getBridgeLanStatus"),
    ];
    for (id, c, h) in mappings {
        assert_eq!((read(id).controller, read(id).handler), (c, h));
    }
    assert_eq!(action("set_wan").handler, "setWan");
    assert_eq!(action("set_vlan_internet").handler, "setVlanService");
    assert_eq!(action("set_wan6").handler, "setWan6V2");
    assert_eq!(action("disable_wifi_ap").handler, "disableap");
    assert_eq!(
        keys("set_wan"),
        vec![
            "wan_name",
            "wanType",
            "pppoeName",
            "pppoePwd",
            "staticIp",
            "staticMask",
            "staticGateway",
            "dns1",
            "dns2",
            "autoset",
            "special",
            "mtu",
            "service"
        ]
    );
    assert_eq!(keys("mac_bind"), vec!["data"]);
    assert_eq!(keys("ps_game"), vec!["service", "enable", "ports"]);
    assert!(keys("ps_multiwan").contains(&"policy%5Bmode%5D"));
    assert!(keys("ps_multiwan").contains(&"port_map%5B0%5D%5Bport%5D"));
    assert!(!keys("ps_multiwan").contains(&"port_map[0][port]"));
    assert!(!keys("set_wan").contains(&"username"));
    let modes = action("set_wan6")
        .fields
        .iter()
        .find(|f| f.key == "ipv6_mode")
        .unwrap()
        .options;
    assert_eq!(
        modes,
        &["off", "native", "dhcpv6", "pppoev6", "static", "pi_relay"]
    );
    assert!(!modes.contains(&"6in4"));
    assert!(!modes.contains(&"6rd"));
}

#[test]
fn fake_vendor_lane_uses_fixed_handlers_and_private_stdin() {
    let r = read("wan_info");
    let mut fake = FakeBackend::new(vec![
        json!({"code":0,"info":{"status":1,"details":{"wanType":"pppoe","username":"PRIVATE_ACCOUNT","password":"PRIVATE_PASSWORD"}},"token":"PRIVATE_TOKEN"}),
    ]);
    let raw = features::invoke(
        &mut fake,
        r.controller,
        r.handler,
        &json!({"wan_name":"WAN2"}),
        &budget(),
    )
    .unwrap();
    let public = project(r.id, raw).unwrap();
    assert_eq!(fake.calls[0].1, vec!["xqnetwork", "getWanInfo"]);
    assert_eq!(fake.calls[0].2, json!({"wan_name":"WAN2"}));
    assert_eq!(public["info"]["details"]["usernameConfigured"], true);
    assert_eq!(public["info"]["details"]["passwordConfigured"], true);
    assert!(!public.to_string().contains("PRIVATE_"));
    let a = action("set_wan");
    let request = json!({"wan_name":"WAN1","wanType":"pppoe","pppoeName":"PRIVATE_ACCOUNT","pppoePwd":"PRIVATE_PASSWORD"});
    let mut fake = FakeBackend::new(vec![json!({"code":0})]);
    features::invoke(&mut fake, a.controller, a.handler, &request, &budget()).unwrap();
    assert_eq!(fake.calls[0].1, vec!["xqnetwork", "setWan"]);
    assert_eq!(fake.calls[0].2, request);
    assert!(!fake.calls[0].1.join(" ").contains("PRIVATE_"));
}

#[test]
fn every_read_discards_unregistered_root_and_nested_fields() {
    for r in DOMAIN.reads {
        let fixture = if r.id == "ipv6_firewall" {
            json!({"code":"1","private":"PRIVATE_SENTINEL"})
        } else {
            json!({"code":0,"private":"PRIVATE_SENTINEL","token":"PRIVATE_SENTINEL","password":"PRIVATE_SENTINEL"})
        };
        let dto = project(r.id, fixture).unwrap();
        assert!(!dto.to_string().contains("PRIVATE_SENTINEL"), "{}", r.id);
    }
    assert!(project("not_a_registered_read", json!({"code":0})).is_err());
    assert!(project("wan_info", json!([])).is_err());
    assert!(
        project(
            "wan_info",
            json!({"info":{"status":{"password":"PRIVATE"}}})
        )
        .is_err()
    );
    assert!(
        project(
            "wan6_status",
            json!({"wan6_info":{"dns":[{"password":"PRIVATE"}]}})
        )
        .is_err()
    );
    assert!(
        project(
            "macbind_info",
            json!({"list":vec![json!({"mac":"02:11:22:33:44:55"});513]})
        )
        .is_err()
    );
    assert!(project("mode", json!({"hostname":"x".repeat(4097)})).is_err());
    assert!(project("mode", json!({"hostname":"bad\nname"})).is_err());
}

#[test]
fn wan_and_ipv6_nested_projection_hides_all_credentials_and_error_text() {
    let dto=project("wan_info",json!({"code":0,"info":{"status":1,"mac":"02:11:22:33:44:55","private":"PRIVATE","ipv4":[{"ip":"10.0.0.2","mask":"255.255.255.0","password":"PRIVATE"}],"details":{"wanType":"pppoe","username":"PRIVATE_ACCOUNT","password":"PRIVATE_PASSWORD","service":"service","token":"PRIVATE"},"vpnInfo":{"username":"PRIVATE","password":"PRIVATE"},"ipv6_info":{"ipv6_mode":"native","up":true,"ip6addr":["2001:db8::2/64"],"dns":["2001:4860:4860::8888"],"lan_ip6addr":[["2001:db8:1::1/64"]],"private":"PRIVATE"}}})).unwrap();
    assert_eq!(
        dto["info"]["ipv4"][0],
        json!({"ip":"10.0.0.2","mask":"255.255.255.0"})
    );
    assert!(dto["info"].get("vpnInfo").is_none());
    assert!(!dto.to_string().contains("PRIVATE"));
    let status=project("pppoe_status",json!({"code":0,"proto":"pppoe","status":2,"pppoename":"PRIVATE","password":"PRIVATE","errcode":"PRIVATE ERRORS","errmsg":"PRIVATE","msg":"PRIVATE","ip":{"address":"10.0.0.2","mask":"255.255.255.0","token":"PRIVATE"},"dns":["1.1.1.1"]})).unwrap();
    assert!(status.get("errcode").is_none());
    assert!(status.get("msg").is_none());
    assert!(!status.to_string().contains("PRIVATE"));
    let cfg=project("wan6_config",json!({"code":0,"wan6_cfg":{"ipv6_mode":"pppoev6","use_pppoev4":0,"username":"PRIVATE","password":"PRIVATE","dns":["2001:4860:4860::8888"],"nat6_enabled":1,"ip6prefix":"fd00::","ip6prefixlen":"64","extra":{"token":"PRIVATE"}}})).unwrap();
    assert_eq!(cfg["wan6_cfg"]["usernameConfigured"], true);
    assert_eq!(cfg["wan6_cfg"]["passwordConfigured"], true);
    assert!(!cfg.to_string().contains("PRIVATE"));
    assert_eq!(
        project("ipv6_firewall", json!({"code":"1"})).unwrap(),
        json!({"mode":1})
    );
    assert!(project("ipv6_firewall", json!({"code":"2"})).is_err());
    assert_eq!(
        project("macbind_info", json!({"list":{}})).unwrap(),
        json!({"list":[]})
    );
}

#[test]
fn port_service_projection_preserves_exact_registered_shapes_not_raw_objects() {
    let dto=project("port_service",json!({"code":0,"lag":{"enable":1,"ports":"1 2","mode":2,"status":"3","info":[{"port":"1","link":"up","speed":"2.5G","token":"PRIVATE"},{"port":"2","link":"up","speed":"1G","password":"PRIVATE"}],"reason":"PRIVATE"},"multiwan":{"enable":1,"port_map":[{"name":"WAN1","port":"3","password":"PRIVATE"},{"name":"WAN2","port":"4"}],"policy":{"mode":0,"currwan":"WAN1","weight1":1,"weight2":2,"bandwidth_wan1":"100","bandwidth_wan2":"200","password":"PRIVATE"}},"wantag":{"interface":"wan","profile":1,"vid":100,"priority":4,"forbid_vid":"1,2","permit_vid":"1~4094","token":"PRIVATE"},"unknownService":{"secret":"PRIVATE"}})).unwrap();
    assert_eq!(dto["lag"]["status"], "3");
    assert_eq!(
        dto["lag"]["info"][0],
        json!({"port":"1","link":"up","speed":"2.5G"})
    );
    assert_eq!(dto["multiwan"]["policy"]["bandwidth_wan2"], "200");
    assert!(dto.get("unknownService").is_none());
    assert!(!dto.to_string().contains("PRIVATE"));
    let map=project("port_map",json!({"code":0,"description":"四口","ports":{"1":{"port":"1","index":"1","label":"LAN1","speed":"2.5G","service":"LAN","raw":"PRIVATE"}}})).unwrap();
    assert_eq!(map["ports"]["1"]["label"], "LAN1");
    assert!(!map.to_string().contains("PRIVATE"));
    assert!(
        project(
            "port_service",
            json!({"lag":{"status":{"code":"0","secret":"PRIVATE"}}})
        )
        .is_err()
    );
}

#[test]
fn wan_static_dhcp_pppoe_forms_are_canonical_and_cross_validated() {
    assert!(validate("set_wan", &json!({"wanType":"dhcp","wan_name":"WAN2"})).is_ok());
    let static_wan = json!({"wanType":"static","staticIp":"10.20.30.2","staticMask":"255.255.255.0","staticGateway":"10.20.30.1","dns1":"1.1.1.1"});
    assert!(validate("set_wan", &static_wan).is_ok());
    for (key, value) in [
        ("staticIp", json!("10.20.30.0")),
        ("staticIp", json!("010.20.30.2")),
        ("staticMask", json!("255.0.255.0")),
        ("staticGateway", json!("10.21.30.1")),
        ("staticGateway", json!("10.20.30.2")),
        ("dns1", json!("127.0.0.1")),
    ] {
        let mut bad = static_wan.clone();
        bad[key] = value;
        assert!(validate("set_wan", &bad).is_err(), "{key}");
    }
    assert!(validate("set_wan",&json!({"wanType":"static","staticIp":"10.0.0.2","staticMask":"255.255.255.0","staticGateway":"10.0.0.1"})).is_err());
    assert!(validate("set_wan", &json!({"wanType":"dhcp","pppoePwd":"discarded"})).is_err());
    assert!(
        validate(
            "set_wan",
            &json!({"wanType":"pppoe","pppoeName":"user","pppoePwd":"password","mtu":1492})
        )
        .is_ok()
    );
    assert!(validate("set_wan", &json!({"wanType":"pppoe","mtu":1493})).is_err());
    assert!(
        validate(
            "set_wan",
            &json!({"wanType":"dhcp","dns1":"1.1.1.1","autoset":"1"})
        )
        .is_err()
    );
    assert!(validate("set_wan", &json!({"wanType":"DHCP"})).is_err());
    assert!(validate("set_wan", &json!({"wanType":"dhcp","wan_name":"eth1"})).is_err());
    assert!(
        validate(
            "set_wan",
            &json!({"wanType":"dhcp","controller":"xqsystem"})
        )
        .is_err()
    );
    for speed in [0, 100, 1000, 2500] {
        assert!(validate("set_wan_speed", &json!({"speed":speed})).is_ok());
    }
    for bad in [
        json!({"speed":10}),
        json!({"speed":10000}),
        json!({"speed":"2500"}),
        json!({"speed":2500,"wan_name":"WAN2"}),
    ] {
        assert!(validate("set_wan_speed", &bad).is_err());
    }
}

#[test]
fn private_keep_credentials_uses_exact_values_not_public_presence() {
    let raw = json!({"info":{"details":{"wanType":"pppoe","username":"PRIVATE_OLD_USER","password":"PRIVATE_OLD_PASSWORD"}}});
    let prepared = prepare_input(
        "set_wan",
        json!({"wanType":"pppoe","pppoePwd":"","dns1":"1.1.1.1"}),
        &raw,
    )
    .unwrap();
    assert_eq!(prepared["pppoeName"], "PRIVATE_OLD_USER");
    assert_eq!(prepared["pppoePwd"], "PRIVATE_OLD_PASSWORD");
    assert!(verify_private("set_wan", &prepared, &raw));
    let wrong = json!({"info":{"details":{"username":"PRIVATE_OLD_USER","password":"WRONG"}}});
    assert!(!verify_private("set_wan", &prepared, &wrong));
    let dto = project("wan_info", raw.clone()).unwrap();
    assert!(prepare_input("set_wan", json!({"wanType":"pppoe"}), &dto).is_err());
    assert!(!verify_private("set_wan", &prepared, &dto));
    assert!(prepare_input("set_wan", json!({"wanType":"pppoe"}), &json!({})).is_err());
    let changed = prepare_input(
        "set_wan",
        json!({"wanType":"pppoe","pppoeName":"NEW_USER","pppoePwd":"NEW_PASSWORD"}),
        &raw,
    )
    .unwrap();
    assert_eq!(changed["pppoePwd"], "NEW_PASSWORD");
    assert!(!verify_private("set_wan", &changed, &raw));
    let v6 = prepare_input(
        "set_wan6",
        json!({"ipv6_mode":"pppoev6","use_pppoev4":0}),
        &json!({"wan6_cfg":{"username":"OLD_USER","password":"OLD_PASSWORD"}}),
    )
    .unwrap();
    assert_eq!(v6["automode"], 0);
    assert_eq!(v6["ipv6DialPassword"], "OLD_PASSWORD");
    assert!(verify_private(
        "set_wan6",
        &v6,
        &json!({"wan6_cfg":{"username":"OLD_USER","password":"OLD_PASSWORD"}})
    ));
    assert!(!verify_private(
        "set_wan6",
        &v6,
        &json!({"wan6_cfg":{"usernameConfigured":true,"passwordConfigured":true}})
    ));
    assert!(!verify_private("unknown", &json!({}), &json!({})));
}

#[test]
fn dhcp_and_mac_binding_inputs_enforce_ranges_shapes_and_native_text_boundaries() {
    let dhcp = json!({"ignore":"0","start":10,"end":30,"leasetime":"12h","dns1":"1.1.1.1"});
    assert!(validate("set_lan_dhcp", &dhcp).is_ok());
    assert!(validate("set_lan_dhcp", &json!({"ignore":"1"})).is_ok());
    for lease in ["1m", "2881m", "49h", "1d", "01h", "0h", "2m junk"] {
        let mut bad = dhcp.clone();
        bad["leasetime"] = json!(lease);
        assert!(validate("set_lan_dhcp", &bad).is_err(), "{lease}");
    }
    assert!(
        validate(
            "set_lan_dhcp",
            &json!({"ignore":"0","start":30,"end":10,"leasetime":"2h"})
        )
        .is_err()
    );
    assert!(validate("set_lan_dhcp",&json!({"ignore":"0","start":10,"end":20,"startip":"192.168.31.10","endip":"192.168.31.20","leasetime":"2h"})).is_err());
    assert!(validate("set_lan_dhcp",&json!({"ignore":"0","startip":"192.168.31.10","endip":"192.168.31.20","leasetime":"2h"})).is_ok());
    let binding = json!({"data":[{"mac":"02:11:22:33:44:55","ip":"192.168.31.10","name":"laptop","instance":1}]});
    assert!(validate("mac_bind", &binding).is_ok());
    for data in [
        json!([]),
        json!([{ "mac":"01:11:22:33:44:55","ip":"192.168.31.10","name":"laptop"}]),
        json!([{ "mac":"00:00:00:00:00:00","ip":"192.168.31.10","name":"laptop"}]),
        json!([{ "mac":"02:11:22:33:44:55","ip":"192.168.31.10","name":"x' SQL"}]),
        json!([{ "mac":"02:11:22:33:44:55","ip":"192.168.31.10","name":"laptop","extra":"private"}]),
        json!([{ "mac":"02:11:22:33:44:55","ip":"192.168.31.10","name":"one"},{"mac":"02:11:22:33:44:56","ip":"192.168.31.10","name":"two"}]),
        json!([{ "mac":"02:11:22:33:44:55","ip":"192.168.31.10","name":"one"},{"mac":"02:11:22:33:44:55","ip":"192.168.31.11","name":"two"}]),
    ] {
        assert!(validate("mac_bind", &json!({"data":data})).is_err());
    }
    assert!(
        validate(
            "mac_unbind",
            &json!({"mac":"02:11:22:33:44:55,02:11:22:33:44:66"})
        )
        .is_ok()
    );
    assert!(validate("mac_unbind", &json!({"mac":"02:11:22:33:44:55;reboot"})).is_err());
    assert!(
        validate(
            "set_lan_ip",
            &json!({"ip":"192.168.31.1","mask":"255.255.255.0"})
        )
        .is_ok()
    );
    assert!(
        validate(
            "set_lan_ip",
            &json!({"ip":"192.168.31.255","mask":"255.255.255.0"})
        )
        .is_err()
    );
}

#[test]
fn ipv6_and_ports_use_rn02_modes_literal_forms_and_valid_service_parameters() {
    assert!(validate("set_wan6",&json!({"ipv6_mode":"static","ip6addr":"2001:db8::2/64","ip6gw":"2001:db8::1","ip6prefix":"2001:db8:1::","ip6prefixlen":64})).is_ok());
    assert!(validate("set_wan6", &json!({"ipv6_mode":"6in4"})).is_err());
    assert!(validate("set_wan6",&json!({"ipv6_mode":"static","ip6addr":"2001:db8::2","ip6gw":"2001:db8::1","ip6prefix":"2001:db8:1::1","ip6prefixlen":64})).is_err());
    assert!(validate("set_wan6", &json!({"ipv6_mode":"dhcpv6","nat6_enabled":1})).is_err());
    assert!(
        validate(
            "set_wan6",
            &json!({"ipv6_mode":"native","ip6addr":"2001:db8::2"})
        )
        .is_err()
    );
    assert!(
        validate(
            "set_wan6",
            &json!({"ipv6_mode":"pppoev6","use_pppoev4":0,"ipv6DialPassword":"transformed$secret"})
        )
        .is_err()
    );
    assert!(
        validate(
            "ps_lag",
            &json!({"service":"lag","enable":1,"mode":2,"ports":"1 2"})
        )
        .is_ok()
    );
    for ports in ["1 1", "1,2", "1 5", "1  2", "1 2 3"] {
        assert!(
            validate(
                "ps_lag",
                &json!({"service":"lag","enable":1,"mode":2,"ports":ports})
            )
            .is_err()
        );
    }
    assert!(
        validate(
            "ps_lag",
            &json!({"service":"game","enable":1,"mode":2,"ports":"1 2"})
        )
        .is_err()
    );
    assert!(validate("ps_game", &json!({"service":"game","enable":1,"ports":"4"})).is_ok());
    assert!(
        validate(
            "ps_game",
            &json!({"service":"game","enable":1,"ports":"3 4"})
        )
        .is_err()
    );
    assert!(validate("ps_wan", &json!({"service":"wan","mode":2})).is_ok());
    assert!(validate("ps_wan", &json!({"service":"wan","mode":1})).is_err());
    assert!(
        validate(
            "ps_iptv",
            &json!({"service":"iptv","enable":1,"profile":0,"vid":100,"priority":4,"ports":"4"})
        )
        .is_ok()
    );
    assert!(
        validate(
            "ps_iptv",
            &json!({"service":"iptv","enable":1,"profile":1,"vid":0,"priority":0,"ports":"4"})
        )
        .is_ok()
    );
    assert!(
        validate(
            "ps_iptv",
            &json!({"service":"iptv","enable":1,"profile":0,"vid":0,"priority":0,"ports":"4"})
        )
        .is_err()
    );
    assert!(
        validate(
            "ps_wantag",
            &json!({"service":"wantag","interface":"wan","profile":1,"vid":100,"priority":4})
        )
        .is_ok()
    );
    assert!(
        validate(
            "ps_wantag",
            &json!({"service":"wantag","interface":"eth0","profile":1,"vid":100,"priority":4})
        )
        .is_err()
    );
    assert!(validate("set_vlan_internet", &json!({"opt":"clean"})).is_ok());
    assert!(
        validate(
            "set_vlan_internet",
            &json!({"opt":"set","internet_profile":1})
        )
        .is_err()
    );
    let multi = json!({"service":"multiwan","enable":1,"port_map%5B0%5D%5Bname%5D":"WAN1","port_map%5B0%5D%5Bport%5D":"1","port_map%5B1%5D%5Bname%5D":"WAN2","port_map%5B1%5D%5Bport%5D":"2","policy%5Bmode%5D":0,"policy%5Bbandwidth_wan1%5D":100,"policy%5Bbandwidth_wan2%5D":200});
    assert!(validate("ps_multiwan", &multi).is_ok());
    let mut bad = multi;
    bad["port_map%5B1%5D%5Bport%5D"] = json!("1");
    assert!(validate("ps_multiwan", &bad).is_err());
    assert!(
        validate(
            "set_multiwan_weight",
            &json!({"bandwidth_wan1":0,"bandwidth_wan2":200})
        )
        .is_err()
    );
}

#[test]
fn verification_never_counts_code_zero_as_the_expected_change() {
    let examples = [
        ("set_wan", json!({"wanType":"dhcp"})),
        ("pppoe_start", json!({})),
        ("pppoe_stop", json!({})),
        ("wan_up", json!({})),
        ("wan_down", json!({})),
        ("mac_clone", json!({"mac":"02:11:22:33:44:55"})),
        ("set_wan_speed", json!({"speed":2500})),
        (
            "set_lan_ip",
            json!({"ip":"192.168.30.1","mask":"255.255.255.0"}),
        ),
        ("set_lan_dhcp", json!({"ignore":"1"})),
        (
            "mac_bind",
            json!({"data":[{"mac":"02:11:22:33:44:55","ip":"192.168.31.5","name":"laptop"}]}),
        ),
        ("mac_unbind", json!({"mac":"02:11:22:33:44:55"})),
        ("ipmac_check_enable", json!({"enable":1})),
        ("set_wan6_switch", json!({"enabled":"1"})),
        ("set_wan6", json!({"ipv6_mode":"native"})),
        ("set_lan6", json!({"mode":2,"ip6assign":64})),
        ("set_ipv6_firewall", json!({"mode":"1"})),
        ("set_multiwan_enable", json!({"enable":"1"})),
        ("set_multiwan_policy", json!({"policy":0})),
        (
            "set_multiwan_weight",
            json!({"bandwidth_wan1":100,"bandwidth_wan2":200}),
        ),
        (
            "set_multiwan_dev_policy",
            json!({"mac":"02:11:22:33:44:55","wan":"WAN2","manual":"0","opt":"1"}),
        ),
        ("ps_wan", json!({"service":"wan","mode":2})),
        ("ps_wandt", json!({"service":"wandt","enable":1})),
        ("ps_multiwan", json!({"service":"multiwan","enable":0})),
        ("ps_lag", json!({"service":"lag","enable":0,"mode":2})),
        ("ps_game", json!({"service":"game","enable":0})),
        ("ps_iptv", json!({"service":"iptv","enable":0})),
        (
            "ps_wantag",
            json!({"service":"wantag","interface":"wan","profile":0}),
        ),
        ("set_vlan_internet", json!({"opt":"clean"})),
        ("set_lan_ap", json!({})),
        ("disable_lan_ap", json!({})),
        ("set_wifi_ap", json!({"ssid":"upstream","password":""})),
        ("disable_wifi_ap", json!({})),
    ];
    assert_eq!(examples.len(), DOMAIN.actions.len());
    for (id, input) in examples {
        assert!(validate(id, &input).is_ok(), "{id}");
        assert!(!verify(id, &input, &json!({"code":0})), "{id}");
    }
}

#[test]
fn verification_matches_exact_configuration_and_runtime_not_unrelated_records() {
    assert!(verify(
        "pppoe_start",
        &json!({}),
        &json!({"proto":"pppoe","status":2})
    ));
    assert!(!verify(
        "pppoe_start",
        &json!({}),
        &json!({"proto":"dhcp","status":2})
    ));
    assert!(verify(
        "pppoe_stop",
        &json!({}),
        &json!({"proto":"pppoe","status":4})
    ));
    assert!(verify(
        "set_wan",
        &json!({"wanType":"dhcp"}),
        &json!({"info":{"status":1,"details":{"wanType":"dhcp"}}})
    ));
    assert!(!verify(
        "set_wan",
        &json!({"wanType":"dhcp"}),
        &json!({"info":{"status":0,"details":{"wanType":"dhcp"}}})
    ));
    assert!(verify(
        "set_lan_dhcp",
        &json!({"ignore":"0","start":10,"end":20,"leasetime":"12h"}),
        &json!({"info":{"ignore":"0","start":"10","limit":"11","leasetime":"12h"}})
    ));
    let bind = json!({"data":[{"mac":"02:11:22:33:44:55","ip":"192.168.31.5","name":"laptop"}]});
    assert!(verify(
        "mac_bind",
        &bind,
        &json!({"list":[{"mac":"02:11:22:33:44:55","ip":"192.168.31.5","name":"laptop"}]})
    ));
    assert!(!verify(
        "mac_bind",
        &bind,
        &json!({"list":[{"mac":"02:11:22:33:44:55","ip":"192.168.31.6","name":"laptop"}]})
    ));
    assert!(verify(
        "mac_unbind",
        &json!({"mac":"02:11:22:33:44:55"}),
        &json!({"list":[]})
    ));
    let lag = json!({"service":"lag","enable":1,"mode":2,"ports":"1 2"});
    assert!(verify(
        "ps_lag",
        &lag,
        &json!({"lag":{"enable":1,"mode":2,"ports":"1 2","status":"0"}})
    ));
    assert!(!verify(
        "ps_lag",
        &lag,
        &json!({"lag":{"enable":1,"mode":2,"ports":"1 2","status":"3"}})
    ));
    assert!(verify(
        "ps_wantag",
        &json!({"service":"wantag","interface":"wan","profile":1,"vid":100,"priority":4}),
        &json!({"wantag":{"interface":"wan","profile":1,"vid":100,"priority":4}})
    ));
    assert!(verify(
        "set_multiwan_dev_policy",
        &json!({"mac":"02:11:22:33:44:55","wan":"WAN2","manual":"0","opt":"0"}),
        &json!({"info":{"dev_policies":[{"mac":"02:11:22:33:44:55","wan":"WAN2","manual":"0"}]}})
    ));
    assert!(verify(
        "set_wan6",
        &json!({"ipv6_mode":"static","ip6addr":"2001:db8::2/64","ip6gw":"2001:db8::1","ip6prefix":"2001:db8:1::","ip6prefixlen":64}),
        &json!({"wan6_cfg":{"ipv6_mode":"static","peerdns":1,"ip6addr":"2001:db8::2/64","ip6gw":"2001:db8::1","ip6prefix":"2001:db8:1::","ip6prefixlen":"64"}})
    ));
    // Management-path changes need a new-address health/reconnect job, even
    // when the old getter happens to report the intended config already.
    assert!(!verify(
        "set_lan_ip",
        &json!({"ip":"192.168.30.1","mask":"255.255.255.0"}),
        &json!({"info":{"status":1,"ipv4":[{"ip":"192.168.30.1","mask":"255.255.255.0"}]}})
    ));
    assert!(!verify(
        "set_lan_ap",
        &json!({}),
        &json!({"mode":2,"hostip":"192.168.1.1"})
    ));
}

#[test]
fn factory_dhcp_lan_address_is_a_bounded_array_not_a_scalar() {
    let raw = serde_json::json!({"code":0,"info":{"lanIp":[{"ip":"192.168.31.1","mask":"255.255.255.0","password":"hidden"}],"start":"5","limit":"250","ignore":"0"}});
    let projected = be6500_panel::features_network::project("lan_dhcp", raw).unwrap();
    assert_eq!(projected["info"]["lanIp"][0]["ip"], "192.168.31.1");
    assert!(projected["info"]["lanIp"][0].get("password").is_none());
}

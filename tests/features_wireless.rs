//! Synthetic native-controller contracts. These tests do not contact a router.
use be6500_panel::features::{Domain, FieldKind, Impact};
use be6500_panel::features_wireless::{
    DOMAIN, prepare_input, project, validate, validate_environment, verify, verify_private,
};
use serde_json::{Value, json};
use std::collections::HashSet;

fn radio(ssid: &str, password: &str, channel: i64) -> Value {
    json!({"ssid":ssid,"password":password,"ssidHtmlEncode":1,"status":"1",
        "encryption":"psk2","channel":channel.to_string(),"bandwidth":"40","txpwr":"max",
        "hidden":"0","bsd":"1","ax":"1","txbf":"3","wifimode":if channel<=13 {"11beg"} else {"11bea"},
        "weakenable":"1","weakthreshold":"-65","kickthreshold":"-75",
        "ssid_len_limit":if channel<=13 {28} else {31},
        "channelInfo":{"channel":channel,"bandwidth":"40","bandList":["20","40"]},
        "available_channels":[{"c":0,"b":["20","40"]},{"c":channel,"b":["20","40"]}]})
}
fn wifi() -> Value {
    json!({"code":0,"bsd":1,"info":[radio("Home","same-secret",1),radio("Home","same-secret",36)]})
}
fn all_intent() -> Value {
    json!({"bsd":"1","ver":"1","on1":"1","ssid1":"Home","encryption1":"psk2","pwd1":"same-secret",
        "on2":"1","ssid2":"Home","encryption2":"psk2","pwd2":"same-secret"})
}
fn guest() -> Value {
    json!({"code":0,"closingTime":0,"info":{"guest":1,"share":0,"need":0,"sns":[],
        "data":{"ssid":"Guest","encryption":"psk2","hidden":"0","password":"guest-secret","ssidHtmlEncode":1}}})
}
fn assert_domain_unique(domain: Domain) {
    let mut reads = HashSet::new();
    let mut actions = HashSet::new();
    for r in domain.reads {
        assert!(reads.insert(r.id), "duplicate read {}", r.id);
        assert!(!r.controller.is_empty() && !r.handler.is_empty());
        let mut fields = HashSet::new();
        for f in r.fields {
            assert!(fields.insert(f.key));
        }
    }
    for a in domain.actions {
        assert!(actions.insert(a.id), "duplicate action {}", a.id);
        assert!(!a.controller.is_empty() && !a.handler.is_empty());
        assert!(!a.configs.is_empty(), "uncheckpointed action {}", a.id);
        assert!(a.impact == Impact::Wireless || a.impact == Impact::Network);
        assert!(
            a.readback.is_empty() || reads.contains(a.readback),
            "unresolved readback {}",
            a.readback
        );
        let mut fields = HashSet::new();
        for f in a.fields {
            assert!(fields.insert(f.key));
        }
    }
}

#[test]
fn wireless_descriptors_are_unique_and_have_resolvable_recovery_domains() {
    assert_eq!(DOMAIN.id, "wireless");
    assert_domain_unique(DOMAIN);
    assert!(
        !DOMAIN.reads.iter().any(|r| r.id == "scan_mesh_node"),
        "scan modifies backhaul/discovery state"
    );
    let scan = DOMAIN
        .actions
        .iter()
        .find(|a| a.id == "scan_mesh_node")
        .unwrap();
    assert_eq!(scan.readback, "");
    let join = DOMAIN
        .actions
        .iter()
        .find(|a| a.id == "add_mesh_node")
        .unwrap();
    assert_eq!(join.readback, "get_addnode_status");
    let mac = DOMAIN
        .reads
        .iter()
        .find(|r| r.id == join.readback)
        .unwrap()
        .fields;
    assert_eq!(mac[0].key, "mac");
    assert_eq!(mac[0].kind, FieldKind::Mac);
}

#[test]
fn descriptors_bind_real_controller_globals_not_route_aliases() {
    for (id, controller, handler) in [
        ("set_wifi", "xqnetwork", "setWifi"),
        ("set_all_wifi", "xqnetwork", "setAllWifi"),
        ("set_wifi_txpwr", "xqnetwork", "setWifiTxpwr"),
        ("set_wifi_ax", "xqnetwork", "setWifiAx"),
        ("set_wifi_txbf", "xqnetwork", "setWifiTxbf"),
        ("set_hostap_mlo", "xqnetwork", "setHostapMLO"),
        ("set_twt", "xqnetwork", "setTwt"),
        ("set_guest_wifi", "xqnetwork", "setWifiWithoutRestart"),
        ("qos_guest", "misystem", "qosGuest"),
        ("miotrelay_switch", "xqnetwork", "miotrelaySwitch"),
        ("miscan_switch", "xqnetwork", "miscanSwitch"),
        ("scan_mesh_node", "xqnetwork", "scanMeshNode"),
        ("add_mesh_node", "xqnetwork", "addMeshNode"),
        ("set_mesh_switch", "xqnetwork", "setMeshSwitch"),
        ("set_mesh_bh_mode", "misystem", "setMeshBhMode"),
        ("set_wifi_weak", "xqnetwork", "setWifiWeakInfo"),
    ] {
        let action = DOMAIN.actions.iter().find(|a| a.id == id).unwrap();
        assert_eq!((action.controller, action.handler), (controller, handler));
    }
    for (id, controller, handler) in [
        ("wifi_detail_all", "xqnetwork", "getAllWifiInfo"),
        ("get_hostap_mlo", "xqnetwork", "getHostapMLO"),
        ("get_twt", "xqnetwork", "getTwt"),
        ("get_addnode_status", "xqnetwork", "getMeshNodeStatus"),
        ("wifi_share_info", "misns", "wifiShareInfo"),
        ("topo_graph", "misystem", "getTopoGraph"),
        ("get_mesh_bh_mode", "misystem", "getMeshBhMode"),
    ] {
        let read = DOMAIN.reads.iter().find(|r| r.id == id).unwrap();
        assert_eq!((read.controller, read.handler), (controller, handler));
    }
    let guest = DOMAIN
        .actions
        .iter()
        .find(|a| a.id == "set_guest_wifi")
        .unwrap();
    assert!(
        guest.configs.contains(&"network")
            && guest.configs.contains(&"firewall")
            && guest.configs.contains(&"dhcp")
    );
    let index = guest.fields.iter().find(|f| f.key == "wifiIndex").unwrap();
    assert_eq!((index.min, index.max), (Some(3), Some(3)));
    assert!(
        DOMAIN
            .actions
            .iter()
            .flat_map(|a| a.fields)
            .filter(|f| f.key.starts_with("pwd"))
            .all(|f| f.kind == FieldKind::Secret)
    );
    let all = DOMAIN
        .actions
        .iter()
        .find(|a| a.id == "set_all_wifi")
        .unwrap();
    assert!(all.fields.iter().any(|f| f.key == "pwd1"));
    assert!(all.fields.iter().any(|f| f.key == "pwd2"));
    assert!(
        !all.fields.iter().any(|f| f.key.ends_with('3')),
        "RN02 has no third primary radio"
    );
}

#[test]
fn ssid_limit_is_native_utf8_byte_limit_and_password_rules_match_helper() {
    for (input, valid) in [
        (
            json!({"wifiIndex":1,"ssid":"Home","encryption":"psk2","pwd":"abcdefgh"}),
            true,
        ),
        (json!({"wifiIndex":1,"ssid":"中".repeat(9)}), true),
        (json!({"wifiIndex":1,"ssid":"中".repeat(10)}), false),
        (json!({"wifiIndex":2,"ssid":"a".repeat(31)}), true),
        (json!({"wifiIndex":2,"ssid":"a".repeat(32)}), false),
        (json!({"wifiIndex":1,"ssid":""}), false),
        (json!({"wifiIndex":1,"ssid":"bad\nname"}), false),
        (
            json!({"wifiIndex":1,"encryption":"psk2","pwd":"1234567"}),
            false,
        ),
        (
            json!({"wifiIndex":1,"encryption":"psk2","pwd":"a".repeat(64)}),
            false,
        ),
        (
            json!({"wifiIndex":1,"encryption":"ccmp","pwd":"abcdefgh"}),
            true,
        ),
        (
            json!({"wifiIndex":1,"encryption":"psk2+ccmp","pwd":"abcdefgh"}),
            true,
        ),
        (
            json!({"wifiIndex":1,"encryption":"psk2","pwd":"密码abcdefgh"}),
            false,
        ),
        (json!({"wifiIndex":1,"encryption":"none"}), true),
        (
            json!({"wifiIndex":1,"encryption":"none","pwd":"ignored-secret"}),
            false,
        ),
        (
            json!({"wifiIndex":1,"encryption":"wep-open","pwd":"abcde"}),
            true,
        ),
        (
            json!({"wifiIndex":1,"encryption":"wep-open","pwd":"abcdefgh"}),
            false,
        ),
        (json!({"wifiIndex":3,"ssid":"Guest"}), false),
        (json!({"wifiIndex":1}), false),
        (json!({"wifiIndex":1,"script":"reboot"}), false),
    ] {
        assert_eq!(validate("set_wifi", &input).is_ok(), valid, "{input}");
    }
}

#[test]
fn suffix_contract_and_common_ssid_conflicts_are_checked_before_factory_apply() {
    let input = all_intent();
    assert!(validate("set_all_wifi", &input).is_ok());
    for key in ["ssid2", "encryption2", "pwd2", "on2"] {
        let mut different = input.clone();
        different[key] = json!(if key == "on2" { "0" } else { "different" });
        assert!(validate("set_all_wifi", &different).is_err(), "{key}");
    }
    let mut hidden = input.clone();
    hidden["hidden1"] = json!("1");
    assert!(validate("set_all_wifi", &hidden).is_err());
    let mut split = input.clone();
    split["bsd"] = json!("0");
    split["ssid2"] = json!("Other");
    assert!(validate("set_all_wifi", &split).is_ok());
    let mut third = input.clone();
    third["ssid3"] = json!("No third radio");
    assert!(validate("set_all_wifi", &third).is_err());
    let mut auto_rename = input.clone();
    auto_rename.as_object_mut().unwrap().remove("ver");
    assert!(validate("set_all_wifi", &auto_rename).is_err());
}

#[test]
fn radio_channels_widths_modes_and_weak_signal_have_semantic_checks() {
    for input in [
        json!({"wifiIndex":1,"channel":36}),
        json!({"wifiIndex":1,"bandwidth":"80"}),
        json!({"wifiIndex":2,"channel":1}),
        json!({"wifiIndex":2,"channel":37}),
        json!({"wifiIndex":2,"channel":165,"bandwidth":"80"}),
        json!({"wifiIndex":1,"wifimode":"11bea"}),
        json!({"wifiIndex":2,"wifimode":"11beg"}),
        json!({"wifiIndex":1,"weakenable":"1"}),
        json!({"wifiIndex":1,"weakenable":"1","weakthreshold":-75,"kickthreshold":-65}),
    ] {
        assert!(validate("set_wifi", &input).is_err(), "{input}");
    }
    for input in [
        json!({"wifiIndex":1,"channel":0,"bandwidth":"40"}),
        json!({"wifiIndex":2,"channel":149,"bandwidth":"80"}),
        json!({"wifiIndex":2,"channel":165,"bandwidth":"20"}),
        json!({"wifiIndex":1,"wifimode":"11beg"}),
    ] {
        assert!(validate("set_wifi", &input).is_ok(), "{input}");
    }
    assert!(
        validate(
            "set_wifi_weak",
            &json!({"wifiIndex":2,"weakenable":"1","weakthreshold":-65,"kickthreshold":-75})
        )
        .is_ok()
    );
    assert!(
        validate(
            "set_wifi_weak",
            &json!({"wifiIndex":2,"weakenable":"1","weakthreshold":0,"kickthreshold":0})
        )
        .is_err()
    );
}

#[test]
fn native_mesh_parameters_do_not_cross_shell_or_json_boundaries() {
    let input = json!({"mac":"02:11:22:33:44:55","locate":"客厅 2-A"});
    assert!(validate("add_mesh_node", &input).is_ok());
    for locate in [
        "x'; reboot #",
        "x\"}",
        "$(id)",
        "`id`",
        "bad\\name",
        "bad\nname",
    ] {
        assert!(
            validate(
                "add_mesh_node",
                &json!({"mac":"02:11:22:33:44:55","locate":locate})
            )
            .is_err()
        );
    }
    for mac in [
        "../../etc/passwd",
        "ff:ff:ff:ff:ff:ff",
        "01:11:22:33:44:55",
        "00:00:00:00:00:00",
        "02:11:22:33:44:55;id",
    ] {
        assert!(validate("add_mesh_node", &json!({"mac":mac,"locate":"Room"})).is_err());
    }
    assert!(validate("set_mesh_bh_mode", &json!({"bhmode":"wired"})).is_ok());
    assert!(validate("set_mesh_bh_mode", &json!({"bhmode":"wireless"})).is_err());
    assert!(
        validate("set_mesh_switch", &json!({"on":"2"})).is_err(),
        "native 2 is a read-only in-Mesh status"
    );
}

#[test]
fn guest_lifecycle_and_fractional_qos_use_actual_fields() {
    assert!(
        validate(
            "set_guest_wifi",
            &json!({"wifiIndex":3,"ssid":"Guest","encryption":"psk2","pwd":"guest-secret","on":"1"})
        )
        .is_ok()
    );
    assert!(
        validate(
            "set_guest_wifi",
            &json!({"wifiIndex":1,"ssid":"Guest","encryption":"none","on":"1"})
        )
        .is_err()
    );
    assert!(
        validate(
            "set_guest_wifi",
            &json!({"wifiIndex":3,"ssid":"Guest","encryption":"none","on":"1","hidden":"1"})
        )
        .is_err(),
        "guest handler does not forward hidden"
    );
    for input in [
        json!({"percent":"0"}),
        json!({"percent":"0.6","percent_up":"1"}),
    ] {
        assert!(validate("qos_guest", &input).is_ok());
    }
    for input in [
        json!({"percent":"60"}),
        json!({"percent":"NaN"}),
        json!({"percent":"0.6;id"}),
        json!({"percent":"0.6","percent_up":"-0.1"}),
    ] {
        assert!(validate("qos_guest", &input).is_err());
    }
}

#[test]
fn prepare_preserves_private_credentials_without_decoding_entities_twice() {
    let mut raw = wifi();
    raw["bsd"] = json!(0);
    raw["info"][0]["bsd"] = json!("0");
    raw["info"][0]["password"] = json!("p&amp;&lt;&quot;&#039;1234&amp;lt;");
    let input = json!({"wifiIndex":1,"ssid":"Changed","encryption":"psk2"});
    let prepared = prepare_input("set_wifi", input, &raw).unwrap();
    assert_eq!(prepared["pwd"], "p&<\"'1234&lt;");
    let blank = prepare_input(
        "set_wifi",
        json!({"wifiIndex":1,"encryption":"psk2","pwd":""}),
        &raw,
    )
    .unwrap();
    assert_eq!(blank["pwd"], prepared["pwd"]);
    let only_password =
        prepare_input("set_wifi", json!({"wifiIndex":1,"pwd":"new-secret"}), &raw).unwrap();
    assert_eq!(only_password["encryption"], "psk2");
    let open = prepare_input("set_wifi", json!({"wifiIndex":1,"encryption":"none"}), &raw).unwrap();
    assert_eq!(open["pwd"], "");
    assert!(
        prepare_input(
            "set_wifi",
            json!({"wifiIndex":1,"encryption":"psk2"}),
            &project("wifi_detail_all", raw.clone()).unwrap()
        )
        .is_err(),
        "presence is not the secret"
    );
    let mut input = all_intent();
    input.as_object_mut().unwrap().remove("pwd1");
    input.as_object_mut().unwrap().remove("pwd2");
    let mut separate = wifi();
    separate["info"][1]["password"] = json!("other-secret");
    let prepared = prepare_input("set_all_wifi", input, &separate).unwrap();
    assert_eq!(prepared["pwd1"], "same-secret");
    assert_eq!(prepared["pwd2"], "same-secret");
    let guest_intent = json!({"wifiIndex":3,"ssid":"Guest","encryption":"psk2","on":"1"});
    assert_eq!(
        prepare_input("set_guest_wifi", guest_intent, &guest()).unwrap()["pwd"],
        "guest-secret"
    );
}

#[test]
fn current_native_channel_capabilities_are_required_for_rf_writes() {
    let raw = wifi();
    assert!(
        prepare_input(
            "set_wifi",
            json!({"wifiIndex":2,"channel":36,"bandwidth":"40"}),
            &raw
        )
        .is_ok()
    );
    assert!(
        prepare_input("set_wifi", json!({"wifiIndex":2,"channel":149}), &raw).is_err(),
        "syntactic channel is not regional capability"
    );
    assert!(prepare_input("set_wifi", json!({"wifiIndex":2,"bandwidth":"160"}), &raw).is_err());
    let mut small = raw.clone();
    small["info"][0]["ssid_len_limit"] = json!(4);
    assert!(
        prepare_input(
            "set_wifi",
            json!({"wifiIndex":1,"ssid":"Long name"}),
            &small
        )
        .is_err()
    );
    let mut unknown = raw.clone();
    unknown["info"][1]
        .as_object_mut()
        .unwrap()
        .remove("available_channels");
    assert!(prepare_input("set_wifi", json!({"wifiIndex":2,"channel":36}), &unknown).is_err());
}

#[test]
fn single_radio_setter_cannot_break_an_existing_common_ssid() {
    let current = wifi();
    assert!(prepare_input("set_wifi", json!({"wifiIndex":1,"ssid":"Other"}), &current).is_err());
    assert!(
        prepare_input(
            "set_wifi",
            json!({"wifiIndex":1,"pwd":"new-secret"}),
            &current
        )
        .is_err()
    );
    assert!(prepare_input("set_wifi", json!({"wifiIndex":1,"hidden":"1"}), &current).is_err());
    assert!(prepare_input("set_wifi", json!({"wifiIndex":1,"on":"0"}), &current).is_err());
    assert!(prepare_input("set_wifi", json!({"wifiIndex":1,"txpwr":"mid"}), &current).is_ok());
    assert!(prepare_input("set_all_wifi", all_intent(), &current).is_ok());
}

#[test]
fn mlo_and_twt_require_current_dependencies_but_disabling_does_not() {
    let raw = wifi();
    let on = json!({"mlo_enable":"1"});
    let twt = json!({"on":"1"});
    assert!(validate_environment("set_hostap_mlo", &on, &raw).is_ok());
    assert!(validate_environment("set_twt", &twt, &raw).is_ok());
    let mut split = raw.clone();
    split["bsd"] = json!(0);
    split["info"][0]["bsd"] = json!("0");
    assert!(validate_environment("set_hostap_mlo", &on, &split).is_err());
    let mut wifi5 = raw.clone();
    for radio in wifi5["info"].as_array_mut().unwrap() {
        radio["ax"] = json!("0");
        radio["wifimode"] = json!("11ac");
    }
    assert!(validate_environment("set_twt", &twt, &wifi5).is_err());
    assert!(validate_environment("set_hostap_mlo", &on, &wifi5).is_err());
    assert!(validate_environment("set_twt", &json!({"on":"0"}), &wifi5).is_ok());
    assert!(validate_environment("set_hostap_mlo", &json!({"mlo_enable":"0"}), &wifi5).is_ok());
    assert!(
        prepare_input(
            "set_hostap_mlo",
            on,
            &json!({"code":0,"mlo_support":0,"mlo_enable":0})
        )
        .is_err()
    );
    assert!(validate_environment("set_mesh_switch", &json!({"on":"1"}), &Value::Null).is_ok());
}

#[test]
fn wireless_projection_is_deep_allowlist_and_secret_presence_only() {
    let mut raw = wifi();
    raw["token"] = json!("TOP_SECRET");
    raw["arbitrary"] = json!({"secret":"OUTER_SECRET"});
    raw["info"][0]["key"] = json!("RAW_KEY");
    raw["info"][0]["sae_password"] = json!("SAE_SECRET");
    raw["info"][0]["vendor_blob"] = json!({"password":"NESTED_SECRET"});
    raw["info"][0]["available_channels"][0]["password"] = json!("CHANNEL_SECRET");
    raw["info"][0]["channelInfo"]["token"] = json!("CHANNEL_INFO_SECRET");
    let out = project("wifi_detail_all", raw).unwrap();
    assert_eq!(out["info"][0]["passwordConfigured"], true);
    assert!(out["info"][0].get("password").is_none());
    assert!(out.get("token").is_none());
    let serialized = out.to_string();
    for secret in [
        "same-secret",
        "TOP_SECRET",
        "OUTER_SECRET",
        "RAW_KEY",
        "SAE_SECRET",
        "NESTED_SECRET",
        "CHANNEL_SECRET",
        "CHANNEL_INFO_SECRET",
    ] {
        assert!(!serialized.contains(secret), "leaked {secret}");
    }
    let mut empty = wifi();
    empty["info"][0]["password"] = json!("");
    assert_eq!(
        project("wifi_detail_all", empty).unwrap()["info"][0]["passwordConfigured"],
        false
    );
    let mut guest = guest();
    guest["info"]["data"]["unmodeled"] = json!({"password":"hidden"});
    guest["info"]["sns"] = json!([{"token":"sns-secret"}]);
    let out = project("wifi_share_info", guest).unwrap();
    assert_eq!(out["info"]["data"]["passwordConfigured"], true);
    assert!(!out.to_string().contains("guest-secret"));
    assert!(out["info"].get("sns").is_none());
}

#[test]
fn malicious_shapes_missing_native_code_or_unknown_reads_fail_closed() {
    assert!(project("wifi_detail_all", json!({"info":[]})).is_err());
    assert!(
        project(
            "wifi_detail_all",
            json!({"code":1537,"msg":"secret","info":[]})
        )
        .is_err()
    );
    assert!(project("wifi_detail_all", json!({"code":"0","info":[]})).is_err());
    assert!(project("not_registered", json!({"code":0})).is_err());
    assert!(
        project(
            "wifi_detail_all",
            json!({"code":0,"info":[{"ssid":{"password":"danger"}}]})
        )
        .is_err()
    );
    assert!(
        project(
            "wifi_detail_all",
            json!({"code":0,"info":[{"channelInfo":{"bandList":[{"password":"danger"}]}}]})
        )
        .is_err()
    );
    assert!(
        project("wifi_detail_all", json!({"code":0,"info":[{}, {}, {}]})).is_err(),
        "no invented third radio"
    );
    assert!(project("get_hostap_mlo", json!({"code":0})).is_err());
    assert!(project("get_addnode_status", json!({"code":0,"status":99})).is_err());
    assert_eq!(
        project("wifi_status", json!({"code":0,"status":{}})).unwrap()["status"],
        json!([]),
        "empty native Lua tables are valid"
    );
}

#[test]
fn native_topology_children_are_projected_recursively_and_depth_is_bounded() {
    let raw = json!({"code":0,"show":1,"graph":{"name":"CAP","ip":"192.168.31.1","renumber":1,"token":"hidden",
        "leafs":[{"name":"RE","version":"1.0.43","link_type":"wireless","signal":2,"password":"hidden",
            "leafs":[{"name":"Nested","private":{"key":"hidden"}}]}]}});
    let out = project("topo_graph", raw).unwrap();
    assert_eq!(out["graph"]["leafs"][0]["link_type"], "wireless");
    assert!(!out.to_string().contains("hidden"));
    let mut tree = json!({"name":"end"});
    for _ in 0..20 {
        tree = json!({"name":"node","leafs":[tree]});
    }
    assert!(project("topo_graph", json!({"code":0,"graph":tree})).is_err());
}

#[test]
fn other_native_reads_have_exact_narrow_projection_shapes() {
    for (id, raw, expected) in [
        (
            "get_hostap_mlo",
            json!({"code":0,"mlo_support":1,"mlo_enable":1,"password":"hidden"}),
            json!({"code":0,"mlo_support":1,"mlo_enable":1}),
        ),
        (
            "get_twt",
            json!({"code":0,"status":"1","token":"hidden"}),
            json!({"code":0,"status":"1"}),
        ),
        (
            "get_miotrelay_switch",
            json!({"code":0,"enabled":1,"bindstatus":1,"key":"hidden"}),
            json!({"code":0,"enabled":1}),
        ),
        (
            "get_miscan_switch",
            json!({"code":0,"enabled":0,"token":"hidden"}),
            json!({"code":0,"enabled":0}),
        ),
        (
            "get_mesh_bh_mode",
            json!({"code":0,"bhmode":"wired","secret":"hidden"}),
            json!({"code":0,"bhmode":"wired"}),
        ),
        (
            "wifi_connect_devices",
            json!({"code":0,"list":[{"mac":"02:11:22:33:44:55","wifiIndex":1,"signal":-40,"key":"hidden"}]}),
            json!({"code":0,"list":[{"mac":"02:11:22:33:44:55","wifiIndex":1,"signal":-40}]}),
        ),
        (
            "guest_qos",
            json!({"code":0,"band":{"secret":"hidden"},"list":[{"password":"hidden"}],"guest":{"percent":"0.6","percent_up":"0.5","UP":20,"DOWN":30,"key":"hidden"}}),
            json!({"code":0,"guest":{"percent":"0.6","percent_up":"0.5","UP":20,"DOWN":30}}),
        ),
    ] {
        assert_eq!(project(id, raw).unwrap(), expected, "{id}");
    }
}

#[test]
fn setters_do_not_echo_secrets_or_claim_async_mesh_success() {
    let ack = project(
        "add_mesh_node",
        json!({"code":0,"password":"hidden","mac":"02:11:22:33:44:55"}),
    )
    .unwrap();
    assert_eq!(ack["pending"], true);
    assert_eq!(ack["complete"], false);
    assert!(!ack.to_string().contains("hidden"));
    let input = json!({"mac":"02:11:22:33:44:55","locate":"Room"});
    assert!(!verify("add_mesh_node", &input, &ack));
    for status in 0..=4 {
        let progress = project("get_addnode_status", json!({"code":0,"status":status})).unwrap();
        assert_eq!(progress["complete"], status == 0);
        assert_eq!(progress["pending"], (1..=3).contains(&status));
        assert_eq!(progress["failed"], status == 4);
        assert_eq!(verify("add_mesh_node", &input, &progress), status == 0);
    }
    let scan=project("scan_mesh_node",json!({"code":0,"list":[{"mac":"02:11:22:33:44:55","obssid":"02:11:22:33:44:56","ssid":"mesh","rssi":-45,"mesh_ver":4,"password":"hidden"}]})).unwrap();
    assert!(verify("scan_mesh_node", &json!({}), &scan));
    assert!(!scan.to_string().contains("hidden"));
    assert!(!verify("scan_mesh_node", &json!({}), &json!({"code":0})));
    assert!(!verify(
        "add_mesh_node",
        &input,
        &json!({"code":0,"status":0})
    ));
    assert_eq!(
        project(
            "set_wifi_ax",
            json!({"code":0,"cac_time":600,"need_confirm":1,"pwd":"hidden","msg":"hidden"})
        )
        .unwrap(),
        json!({"code":0,"cac_time":600,"need_confirm":1})
    );
}

#[test]
fn readback_checks_expected_fields_not_a_success_envelope() {
    let raw = wifi();
    let after = project("wifi_detail_all", raw.clone()).unwrap();
    let input = json!({"wifiIndex":1,"ssid":"Home","pwd":"same-secret","encryption":"psk2","on":"1","channel":1,"bandwidth":"40","hidden":"0","txpwr":"max"});
    assert!(verify("set_wifi", &input, &after));
    assert!(verify_private("set_wifi", &input, &raw));
    assert!(!verify("set_wifi", &input, &json!({"code":0})));
    for (key, value) in [
        ("ssid", json!("Wrong")),
        ("status", json!("0")),
        ("channel", json!("6")),
        ("bandwidth", json!("20")),
        ("txpwr", json!("min")),
        ("encryption", json!("ccmp")),
    ] {
        let mut wrong = after.clone();
        wrong["info"][0][key] = value;
        assert!(!verify("set_wifi", &input, &wrong), "{key}");
    }
    assert!(verify("set_all_wifi", &all_intent(), &after));
    let mut wrong_secret = raw.clone();
    wrong_secret["info"][0]["password"] = json!("another-secret");
    assert!(
        verify(
            "set_wifi",
            &input,
            &project("wifi_detail_all", wrong_secret.clone()).unwrap()
        ),
        "public presence is deliberately not secret equality"
    );
    assert!(!verify_private("set_wifi", &input, &wrong_secret));
    let mut changed_ssid = raw.clone();
    changed_ssid["info"][0]["ssid"] = json!("A&amp;B&lt;&gt;&quot;&#039;");
    assert!(verify(
        "set_wifi",
        &json!({"wifiIndex":1,"ssid":"A&B<>\"'"}),
        &project("wifi_detail_all", changed_ssid).unwrap()
    ));
    assert!(verify(
        "set_guest_wifi",
        &json!({"wifiIndex":3,"ssid":"Guest","pwd":"guest-secret","encryption":"psk2","on":"1"}),
        &project("wifi_share_info", guest()).unwrap()
    ));
    assert!(verify("set_wifi_ax", &json!({"ax":"1"}), &after));
    assert!(!verify("set_wifi_ax", &json!({"ax":"0"}), &after));
    assert!(!verify(
        "set_hostap_mlo",
        &json!({"mlo_enable":"1"}),
        &json!({"code":0,"mlo_support":0,"mlo_enable":1})
    ));
    assert!(verify(
        "set_twt",
        &json!({"on":"1"}),
        &json!({"code":0,"status":1})
    ));
    assert!(
        !verify(
            "set_mesh_switch",
            &json!({"on":"1"}),
            &json!({"code":0,"enabled":2})
        ),
        "in-mesh immutable state is not a switch apply"
    );
    assert!(verify(
        "qos_guest",
        &json!({"percent":"0.6","percent_up":"0.5"}),
        &json!({"code":0,"guest":{"percent":"0.6","percent_up":"0.5"}})
    ));
}

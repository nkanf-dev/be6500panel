use be6500_panel::{
    features::{FieldKind, Impact},
    features_services::{DOMAIN, prepare_input, project, validate, verify, verify_private},
};
use serde_json::{Value, json};
use std::collections::HashSet;

fn action(id: &str) -> &'static be6500_panel::features::Action {
    DOMAIN.actions.iter().find(|a| a.id == id).unwrap()
}
fn public(id: &str, native: Value) -> Value {
    project(id, native).unwrap()
}
const MAC: &str = "02:11:22:33:44:55";

#[test]
fn descriptors_are_unique_fixed_native_contracts_and_have_registered_readback() {
    assert_eq!(DOMAIN.id, "services");
    let mut ids = HashSet::new();
    for r in DOMAIN.reads {
        assert!(ids.insert(("read", r.id)), "duplicate read {}", r.id);
        assert!(!r.handler.contains('/') && !r.controller.contains('/'));
        assert!(r.fields.iter().map(|f| f.key).collect::<HashSet<_>>().len() == r.fields.len());
        assert!(
            project(
                r.id,
                json!({"code":0,"password":"sentinel","unknown":{"token":"sentinel"}})
            )
            .is_ok()
        );
    }
    for a in DOMAIN.actions {
        assert!(ids.insert(("action", a.id)), "duplicate action {}", a.id);
        assert!(
            DOMAIN.reads.iter().any(|r| r.id == a.readback),
            "missing readback {}",
            a.id
        );
        assert!(
            !a.configs.is_empty(),
            "missing protected configuration {}",
            a.id
        );
        assert!(!a.handler.contains('/') && !a.controller.contains('/'));
        assert!(a.fields.iter().map(|f| f.key).collect::<HashSet<_>>().len() == a.fields.len());
        assert!(
            !a.fields
                .iter()
                .any(|f| ["url", "path", "command", "shell", "payload"].contains(&f.key))
        );
    }
    let native = [
        (
            "forward_add",
            "xqsystem",
            "addRedirect",
            &["name", "ip", "proto", "sport", "dport"][..],
        ),
        (
            "forward_range_add",
            "xqsystem",
            "addRangeRedirect",
            &["name", "ip", "proto", "fport", "tport"][..],
        ),
        (
            "forward_delete",
            "xqsystem",
            "deleteRedirect",
            &["port", "proto"][..],
        ),
        ("dmz_set", "xqsystem", "setDMZ", &["ip", "mac", "mode"][..]),
        ("upnp_switch", "xqsystem", "upnpSwitch", &["switch"][..]),
        (
            "ddns_add",
            "xqnetwork",
            "addServer",
            &[
                "id",
                "enable",
                "domain",
                "username",
                "password",
                "wanindex",
                "iptype",
                "checkinterval",
                "forceinterval",
            ][..],
        ),
        (
            "qos_device",
            "misystem",
            "setMACQoSInfo",
            &["mac", "upload", "download"][..],
        ),
        (
            "qos_guest",
            "misystem",
            "qosGuest",
            &["percent", "percent_up"][..],
        ),
        (
            "access_edit",
            "xqnetwork",
            "editDevice",
            &["mac", "model", "option"][..],
        ),
        (
            "web_access",
            "misystem",
            "webAccess",
            &["open", "mac", "opt"][..],
        ),
        ("anti_scan", "anti_attack", "set_scan_api", &["enable"][..]),
        (
            "ntp_set",
            "sysutil",
            "setNTPServer",
            &["server1", "server2"][..],
        ),
        (
            "scheduled_reboot_set",
            "maintenance",
            "setSchedule",
            &["enabled", "time", "weekdays"][..],
        ),
    ];
    for (id, c, h, keys) in native {
        let a = action(id);
        assert_eq!((a.controller, a.handler), (c, h));
        assert_eq!(a.fields.iter().map(|f| f.key).collect::<Vec<_>>(), keys);
    }
    assert_eq!(action("qos_limit").readback, "qos_history");
    assert_eq!(action("official_upgrade").handler, "upgradeRom");
    assert_eq!(action("parental_time_set").handler, "setPctl");
    assert_eq!(action("parental_time_set").readback, "parental_time");
    assert!(
        action("ddns_edit")
            .fields
            .iter()
            .filter(|f| f.kind == FieldKind::Secret)
            .all(|f| !f.required)
    );
}

#[test]
fn destructive_maintenance_has_explicit_impact_and_no_success_shortcut() {
    for (id, input) in [
        ("reboot", json!({})),
        ("factory_reset", json!({"format":0})),
        ("official_upgrade", json!({})),
    ] {
        assert_eq!(action(id).impact, Impact::Maintenance);
        assert!(action(id).configs.contains(&"system"));
        assert!(validate(id, &input).is_ok());
        assert!(!verify(
            id,
            &input,
            &json!({"code":0,"status":0,"romversion":"1.0.43"})
        ));
    }
    assert!(validate("factory_reset", &json!({"format":1})).is_err());
    assert!(validate("official_upgrade", &json!({"custom":1,"recovery":1})).is_err());
    assert!(
        validate(
            "official_upgrade",
            &json!({"custom":1,"recovery":0,"url":"https://example.invalid/image.bin"})
        )
        .is_err()
    );
    assert_eq!(action("ota_set").impact, Impact::Maintenance);
    assert_eq!(action("scheduled_reboot_set").impact, Impact::Maintenance);
}

#[test]
fn forwarding_native_scalar_range_and_semantic_validation() {
    let one = json!({"name":"Web","ip":"192.168.31.10","proto":1,"sport":8443,"dport":443});
    assert!(validate("forward_add", &one).is_ok());
    assert!(verify(
        "forward_add",
        &one,
        &public(
            "forwarding",
            json!({"code":0,"status":1,"list":[{"name":"Web","destip":"192.168.31.10","proto":1,"ftype":1,"srcport":8443,"destport":"443"}]})
        )
    ));
    assert!(!verify("forward_add", &one, &json!({"code":0})));
    assert!(!verify(
        "forward_add",
        &one,
        &public(
            "forwarding",
            json!({"list":[{"name":"Web","destip":"192.168.31.11","proto":1,"ftype":1,"srcport":8443,"destport":443}]})
        )
    ));
    let range = json!({"name":"Game","ip":"192.168.31.10","proto":3,"fport":10000,"tport":10100});
    assert!(validate("forward_range_add", &range).is_ok());
    let after = public(
        "forwarding",
        json!({"status":1,"list":[{"name":"Game","destip":"192.168.31.10","proto":3,"ftype":2,"srcport":{"f":10000,"t":10100,"token":"sentinel"}}]}),
    );
    assert_eq!(after["list"][0]["srcport"], json!({"f":10000,"t":10100}));
    assert!(verify("forward_range_add", &range, &after));
    assert!(!verify(
        "forward_delete",
        &json!({"port":10000,"proto":3}),
        &after
    ));
    assert!(verify(
        "forward_delete",
        &json!({"port":10000,"proto":3}),
        &json!({"list":[]})
    ));
    assert!(!verify("forward_apply", &json!({}), &after));
    for patch in [
        json!({"sport":0}),
        json!({"sport":65536}),
        json!({"proto":0}),
        json!({"proto":4}),
        json!({"ip":"127.0.0.1"}),
        json!({"ip":"224.0.0.1"}),
        json!({"ip":"192.168.31.255"}),
        json!({"ip":"$(id)"}),
        json!({"name":"Web;reboot"}),
    ] {
        let mut bad = one.clone();
        for (k, v) in patch.as_object().unwrap() {
            bad[k] = v.clone();
        }
        assert!(validate("forward_add", &bad).is_err(), "{bad}");
    }
    let mut bad = range;
    bad["fport"] = json!(10101);
    assert!(validate("forward_range_add", &bad).is_err());
    assert!(validate("forward_delete", &json!({"port":0,"proto":1})).is_err());
}

fn ddns_add() -> Value {
    json!({"id":9,"enable":1,"domain":"router.example.org","username":"private-user","password":"private-password","checkinterval":10,"forceinterval":24,"wanindex":"WAN1","iptype":"0"})
}
#[test]
fn ddns_secret_projection_preserve_and_private_equality_gate() {
    let add = ddns_add();
    assert!(validate("ddns_add", &add).is_ok());
    let raw = json!({"code":0,"enabled":1,"domain":"router.example.org","checkinterval":10,"forceinterval":24,"wanindex":"WAN1","iptype":"0","username":"private-user","password":"private-password","url":"https://private-user:private-password@example.org","unknown":{"token":"private-password"}});
    let after = public("ddns_detail", raw.clone());
    assert_eq!(
        after,
        json!({"domain":"router.example.org","checkinterval":10,"forceinterval":24,"wanindex":"WAN1","iptype":"0","usernameConfigured":true,"passwordConfigured":true})
    );
    assert!(verify("ddns_add", &add, &after));
    assert!(verify_private("ddns_add", &add, &raw));
    let mut wrong = raw.clone();
    wrong["password"] = json!("different-but-configured");
    assert!(verify(
        "ddns_add",
        &add,
        &public("ddns_detail", wrong.clone())
    ));
    assert!(!verify_private("ddns_add", &add, &wrong));
    let edit = json!({"id":9,"username":"","password":"","domain":"router.example.org","wanindex":"WAN1","iptype":"0"});
    let prepared = prepare_input("ddns_edit", edit, &raw).unwrap();
    assert!(prepared.get("username").is_none() && prepared.get("password").is_none());
    assert!(verify_private("ddns_edit", &prepared, &raw));
    assert!(verify("ddns_edit", &prepared, &after));
    assert!(validate("ddns_add",&json!({"id":11,"enable":1,"domain":"router.example.org","username":"u","password":"p","checkinterval":10,"forceinterval":24,"wanindex":"WAN1","iptype":"0"})).is_err());
    let mut disabled = add.clone();
    disabled["enable"] = json!(0);
    assert!(validate("ddns_add", &disabled).is_err());
    for domain in [
        "http://router.example.org",
        "example.org/path",
        "example.org;reboot",
        "-bad.example.org",
    ] {
        let mut bad = add.clone();
        bad["domain"] = json!(domain);
        assert!(validate("ddns_add", &bad).is_err());
    }
    let native = public(
        "ddns",
        json!({"on":1,"list":[{"id":9,"enabled":1,"domain":"router.example.org","status":2,"error":"private-password","username":"private-user"}]}),
    );
    assert!(verify("ddns_switch", &json!({"id":9,"on":1}), &native));
    assert!(!verify("ddns_delete", &json!({"id":9}), &native));
    assert!(verify(
        "ddns_delete",
        &json!({"id":9}),
        &public("ddns", json!({"list":{}}))
    ));
    assert!(!native.to_string().contains("private"));
}

#[test]
fn qos_priorities_limits_guest_and_actual_acceleration_are_not_conflated() {
    let qos = public(
        "qos",
        json!({"status":{"on":1,"mode":2,"password":"sentinel"},"band":{"upload":100,"download":1000},"guest":{"percent":"0.6","percent_up":"0.4","token":"sentinel"},"list":[{"mac":MAC,"name":"Laptop","statistics":{"token":"sentinel"},"qos":{"upmax":128,"downmax":1024,"password":"sentinel"}}]}),
    );
    assert!(verify("qos_switch", &json!({"on":1}), &qos));
    assert!(verify(
        "qos_band",
        &json!({"upload":100,"download":1000,"manual":1}),
        &qos
    ));
    assert!(verify(
        "qos_guest",
        &json!({"percent":"0.6","percent_up":"0.4"}),
        &qos
    ));
    assert!(validate("qos_guest", &json!({"percent":"60","percent_up":"40"})).is_err());
    let priority = json!({"mac":MAC,"mode":1,"upload":3,"download":3});
    assert!(validate("qos_limit", &priority).is_ok());
    let history = public(
        "qos_history",
        json!({"status":{"on":1,"mode":1},"dict":{MAC:{"mac":MAC,"level":3,"flag":"on","password":"sentinel"},"invalid-key":{"token":"sentinel"}}}),
    );
    assert!(verify("qos_limit", &priority, &history));
    assert!(!verify(
        "qos_limit",
        &json!({"mac":MAC,"mode":1,"upload":2,"download":3}),
        &history
    ));
    assert!(
        validate(
            "qos_limit",
            &json!({"mac":MAC,"mode":1,"upload":99,"download":3})
        )
        .is_err()
    );
    let batch = json!({"mode":2,"data":[{"mac":MAC,"maxup":128,"maxdown":1024}]});
    let after = public(
        "qos_history",
        json!({"status":{"on":1,"mode":2},"dict":{MAC:{"mac":MAC,"upmax":128,"downmax":1024,"flag":"on"}}}),
    );
    assert!(verify("qos_limits", &batch, &after));
    assert!(!verify("qos_limits", &batch, &history));
    assert!(
        validate(
            "qos_limits",
            &json!({"mode":2,"data":[{"mac":MAC,"maxup":128,"maxdown":1024,"shell":"reboot"}]})
        )
        .is_err()
    );
    let device = public(
        "qos_device",
        json!({"limit":{"upmax":128,"downmax":1024,"flag":"on"}}),
    );
    assert!(verify(
        "qos_device",
        &json!({"mac":MAC,"upload":128,"download":1024}),
        &device
    ));
    assert!(!verify("qos_offlimit", &json!({"mac":MAC}), &device));
    assert!(!qos.to_string().contains("sentinel") && !history.to_string().contains("sentinel"));
    let setting = public("acceleration_setting", json!({"status":1,"active":true}));
    assert_eq!(setting["runtimeObserved"], false);
    assert!(setting.get("active").is_none());
    let runtime = public(
        "acceleration_status",
        json!({"engine":"unknown","state":"unknown","frontend":"","ecm":null,"sfe":null,"ppe":null,"counters":{"hits":2,"password":"sentinel","drops":{"token":"sentinel"}},"force_start":1}),
    );
    assert_eq!(runtime["state"], "unknown");
    assert_eq!(runtime["counters"], json!({"hits":2}));
    assert!(runtime.get("force_start").is_none());
}

#[test]
fn access_security_switches_reject_unknown_keys_and_verify_expected_list() {
    let input = json!({"mac":MAC,"model":1,"option":0});
    let after = public(
        "access",
        json!({"enable":1,"model":1,"macfilter":[{"mac":MAC,"name":"Laptop","password":"sentinel"}],"list":[{"mac":MAC,"authority":{"wan":1,"token":"sentinel"}}],"weblist":[MAC,{"token":"sentinel"}]}),
    );
    assert!(verify("access_edit", &input, &after));
    assert!(!verify(
        "access_edit",
        &json!({"mac":MAC,"model":1,"option":1}),
        &after
    ));
    assert!(
        validate(
            "access_edit",
            &json!({"mac":"01:11:22:33:44:55","model":1,"option":0})
        )
        .is_err()
    );
    assert!(
        validate(
            "access_edit",
            &json!({"mac":"02:11:22:33:44:55;reboot","model":1,"option":0})
        )
        .is_err()
    );
    assert!(validate("web_access", &json!({"open":1})).is_err());
    assert!(verify(
        "web_access",
        &json!({"open":1,"mac":MAC,"opt":0}),
        &public("web_access", json!({"open":true,"list":[MAC]}))
    ));
    assert!(!verify(
        "web_access",
        &json!({"open":1,"mac":MAC,"opt":0}),
        &json!({"open":true,"list":[]})
    ));
    for (action_id, read_id, field, key) in [
        (
            "firewall_switch",
            "firewall",
            "firewall_enable",
            "firewall_enable",
        ),
        ("spi_switch", "spi", "spi_firewall", "spi_firewall"),
        ("dos_switch", "dos", "dos_firewall", "dos_firewall"),
        (
            "wan_ping_switch",
            "wan_ping",
            "wanping_firewall",
            "wanping_firewall",
        ),
        ("https_switch", "https", "on", "on"),
        ("gateway_security", "gateway_security", "on", "enable"),
        ("anti_scan", "anti_attack", "enable", "scan"),
    ] {
        let input = json!({field:1});
        let after = public(read_id, json!({key:"1","password":"sentinel"}));
        assert!(verify(action_id, &input, &after), "{action_id}");
        assert!(
            !verify(action_id, &input, &json!({"code":0})),
            "{action_id}"
        );
        assert!(validate(action_id, &json!({field:1,"enabled":true})).is_err());
    }
    assert!(!after.to_string().contains("sentinel"));
}

#[test]
fn parental_v2_opt_list_is_typed_bounded_single_operation_and_readback_matches() {
    let user = json!({"opt_list":[{"opt":"mipctl_add_user","user_name":"Child","icon":"child1"}]});
    assert!(validate("parental_user_add", &user).is_ok());
    assert!(verify(
        "parental_user_add",
        &user,
        &public(
            "parental_users",
            json!({"user_list":[{"user_id":1,"user_name":"Child","icon":"child1","status":1,"password":"sentinel"}]})
        )
    ));
    let devices = json!({"opt_list":[{"opt":"mipctl_set_device","user_id":1,"devices":[MAC]}]});
    assert!(verify(
        "parental_devices_set",
        &devices,
        &public(
            "parental_devices",
            json!({"list":[{"user_id":1,"devices":[MAC]}]})
        )
    ));
    let time = json!({"opt_list":[{"opt":"mipctl_set_deny_time","user_id":1,"time_list":[{"start":1200,"end":1560,"enable":[1,1,1,1,1,0,0]}]}]});
    assert!(validate("parental_time_set", &time).is_ok());
    assert!(verify(
        "parental_time_set",
        &time,
        &public(
            "parental_time",
            json!({"time_list":[{"id":"1_1200_1560_31","start":1200,"end":1560,"enable":[1,1,1,1,1,0,0]}]})
        )
    ));
    assert!(validate("parental_time_set",&json!({"opt_list":[{"opt":"mipctl_set_deny_time","user_id":1,"time_list":[{"start":1300,"end":1200,"enable":[1,1,1,1,1,0,0]}]}]})).is_err());
    assert!(validate("parental_time_set",&json!({"opt_list":[{"opt":"mipctl_set_deny_time","user_id":1,"time_list":[{"start":1200,"end":1560,"enable":[1]}]}]})).is_err());
    let hosts =
        json!({"opt_list":[{"opt":"mipctl_set_filting_net","user_id":1,"list":["example.org"]}]});
    assert!(verify(
        "parental_hosts_set",
        &hosts,
        &public(
            "parental_hosts",
            json!({"list":["example.org",{"token":"sentinel"}]})
        )
    ));
    let deny = json!({"opt_list":[{"opt":"mipctl_set_temp_deny","user_id":1,"deny":true}]});
    assert!(verify(
        "parental_temporary_set",
        &deny,
        &public(
            "parental_temporary",
            json!({"deny":true,"show":true,"nxt_permit":"10/05/2026-06:00-1"})
        )
    ));
    for bad in [
        json!({"opt_list":[{"opt":"arbitrary_shell","user_id":1,"command":"reboot"}]}),
        json!({"opt_list":[{"opt":"mipctl_set_temp_deny","user_id":1,"deny":1}]}),
        json!({"opt_list":[{"opt":"mipctl_set_device","user_id":1,"devices":[MAC,MAC]}]}),
        json!({"opt_list":[{"opt":"mipctl_set_filting_net","user_id":1,"list":["https://example.org/path"]}]}),
    ] {
        assert!(validate("parental_temporary_set", &bad).is_err());
    }
    let encoded = json!({"opt_list":deny["opt_list"].to_string()});
    assert_eq!(
        prepare_input("parental_temporary_set", encoded, &json!({})).unwrap(),
        deny
    );
    assert!(!verify("parental_temporary_set", &deny, &json!({"code":0})));
}

#[test]
fn system_time_ntp_led_schedule_have_semantic_validation_and_expected_state() {
    let time = json!({"time":"2024-02-29 12:34:56","index":"0"});
    assert!(validate("time_set", &time).is_ok());
    assert!(
        validate(
            "time_set",
            &json!({"time":"2023-02-29 12:34:56","index":"0"})
        )
        .is_err()
    );
    assert!(
        validate(
            "time_set",
            &json!({"time":"2024-02-29 24:34:56","index":"0"})
        )
        .is_err()
    );
    assert!(validate("time_set", &json!({"index":"../system"})).is_err());
    assert!(verify(
        "time_set",
        &time,
        &public(
            "time",
            json!({"time":{"index":"0","timezone":"CST-8","year":2024,"month":2,"day":29,"hour":12,"min":34,"sec":58,"token":"sentinel"}})
        )
    ));
    let led = json!({"on":1,"timer_on":1,"timer_open":"06:00","timer_close":"22:30"});
    assert!(verify(
        "led_set",
        &led,
        &public(
            "led",
            json!({"status":1,"timer_status":1,"timer_open":"06:00","timer_close":"22:30","error":0})
        )
    ));
    assert!(validate("led_set", &json!({"on":1,"timer_on":1})).is_err());
    assert!(
        validate(
            "led_set",
            &json!({"on":1,"timer_on":1,"timer_open":"25:00","timer_close":"22:30"})
        )
        .is_err()
    );
    assert!(verify(
        "ntp_set",
        &json!({"server1":"time.example.org","server2":"192.168.31.2"}),
        &public(
            "ntp",
            json!({"servers":["time.example.org","192.168.31.2"]})
        )
    ));
    assert!(validate("ntp_set", &json!({"server1":"time.example.org;reboot"})).is_err());
    assert!(validate("ntp_set", &json!({"server1":"https://time.example.org"})).is_err());
    assert!(verify(
        "time_mode_set",
        &json!({"mode":0,"sync":0}),
        &public("time_mode", json!({"info":{"mode":0,"sync":0}}))
    ));
    assert!(!verify(
        "time_mode_set",
        &json!({"mode":0,"sync":1}),
        &json!({"info":{"mode":0,"sync":0}})
    ));
    let schedule = json!({"enabled":true,"time":"03:15","weekdays":[0,2,4]});
    assert!(verify(
        "scheduled_reboot_set",
        &schedule,
        &public(
            "scheduled_reboot",
            json!({"enabled":true,"time":"03:15","weekdays":[4,0,2],"timeBasis":"router","reloadPending":false})
        )
    ));
    assert!(!verify(
        "scheduled_reboot_set",
        &schedule,
        &json!({"enabled":true,"time":"03:15","weekdays":[0,2,4],"reloadPending":true})
    ));
    for days in [json!([]), json!([7]), json!([0, 0]), json!(["shell"])] {
        assert!(
            validate(
                "scheduled_reboot_set",
                &json!({"enabled":true,"time":"03:15","weekdays":days})
            )
            .is_err()
        );
    }
    assert!(verify(
        "ota_set",
        &json!({"auto":0}),
        &public("ota", json!({"auto":0,"time":"03:00"}))
    ));
}

#[test]
fn projections_never_clone_nested_known_key_objects_or_unknown_private_dtos() {
    let injected = json!({"code":0,"password":"sentinel","token":"sentinel","unknown":{"secret":"sentinel"},"name":{"password":"sentinel"},"enabled":{"token":"sentinel"},"status":{"password":"sentinel"},"list":[{"mac":MAC,"name":{"token":"sentinel"},"qos":{"upmax":{"token":"sentinel"},"downmax":12},"password":"sentinel"}],"time":{"year":{"token":"sentinel"}},"user_list":[{"user_name":{"token":"sentinel"}}]});
    for r in DOMAIN.reads {
        let out = project(r.id, injected.clone()).unwrap();
        assert!(!out.to_string().contains("sentinel"), "{} -> {out}", r.id);
        assert!(
            out.get("password").is_none()
                && out.get("token").is_none()
                && out.get("unknown").is_none()
        );
    }
    assert!(project("not_registered", json!({})).is_err());
    assert!(project("ddns", json!([])).is_err());
    assert!(project("ddns", json!({"code":5,"msg":"sentinel"})).is_err());
    let update = public(
        "rom_update",
        json!({"needUpdate":1,"version":"1.0.44","fileSize":1000000,"downloadUrl":"https://user:sentinel@example.org","changeLog":{"token":"sentinel"},"otherParam":{"password":"sentinel"},"status":{"status":0,"percent":0}}),
    );
    assert!(!update.to_string().contains("sentinel") && update.get("downloadUrl").is_none());
    let version = public(
        "version",
        json!({"hardware":"RN02","romversion":"1.0.43","routerId":"private-id","miioDid":"private-id","imei":"private-id","id":"private-id"}),
    );
    assert_eq!(version, json!({"hardware":"RN02","romversion":"1.0.43"}));
    assert!(validate("not_registered", &json!({})).is_err());
    assert!(validate("upnp_switch", &json!({"switch":1,"shell":"reboot"})).is_err());
}

#[test]
fn absent_ddns_provider_is_not_configured_without_invented_secrets() {
    assert_eq!(
        project(
            "ddns_detail",
            json!({"code":1614,"msg":"private-native-text"})
        )
        .unwrap(),
        json!({"configured":false})
    );
    assert!(project("ddns_detail", json!({"code":5})).is_err());
    assert!(project("ddns", json!({"code":1614})).is_err());
    let absent = json!({"configured":false});
    assert!(!verify("ddns_add", &ddns_add(), &absent));
}

use be6500_panel::capture_plan::{RulesPlanInput, plan_owned_rules};
use serde_json::{Value, json};

fn gateway() -> RulesPlanInput {
    serde_json::from_value(json!({
        "scope":"gateway", "datapath":"routed-tun", "lanInterface":"br-lan",
        "lanIPv4Prefixes":["192.168.50.0/24"],
        "tunInterface":"b6p-tun", "tunAddress":"172.31.255.253/30",
        "ports":{"mixed":2080,"dns":1053},
        "managementIPs":["192.168.50.1"], "endpointIPs":["203.0.113.9"]
    }))
    .unwrap()
}
fn devices() -> RulesPlanInput {
    serde_json::from_value(json!({
        "datapath":"routed-tun", "lanInterface":"br-lan",
        "clientIPv4":"192.168.50.10", "clientMACs":{"192.168.50.10":"02:AA:BB:CC:DD:01"},
        "tunInterface":"b6p-tun", "tunAddress":"172.31.255.253/30"
    }))
    .unwrap()
}
fn has(command: &[String], parts: &[&str]) -> bool {
    command
        .windows(parts.len())
        .any(|window| window.iter().map(String::as_str).eq(parts.iter().copied()))
}

#[test]
fn gateway_and_device_plans_preserve_ordered_owned_resources() {
    for input in [gateway(), devices()] {
        let plan = plan_owned_rules(&input).unwrap();
        assert_eq!(
            plan.apply[0],
            [
                "ip", "-4", "route", "add", "default", "dev", "b6p-tun", "table", "16500"
            ]
        );
        assert_eq!(plan.ownership.mark, 0x4000);
        assert_eq!(plan.ownership.mask, 0x4000);
        assert_eq!(plan.ownership.route_families, [4]);
        assert_eq!(plan.ownership.chains.len(), 6);
        assert!(has(plan.apply.last().unwrap(), &["-j", "B6P_V4_TUN_MARK"]));
        assert!(has(&plan.cleanup[0], &["-D", "PREROUTING"]));
        assert!(has(&plan.cleanup[0], &["-j", "B6P_V4_TUN_MARK"]));
        assert_eq!(plan.cleanup, plan.on_failure);
        let value: Value = serde_json::to_value(&plan).unwrap();
        assert_eq!(value["Ownership"]["Datapath"], "routed-tun");
        assert_eq!(value["Ownership"]["TUNAddress"], "172.31.255.253/30");
    }
    let plan = plan_owned_rules(&devices()).unwrap();
    assert_eq!(
        plan.ownership.client_macs["192.168.50.10"],
        "02:aa:bb:cc:dd:01"
    );
    assert!(
        plan.apply
            .iter()
            .any(|c| has(c, &["--mac-source", "02:aa:bb:cc:dd:01"]))
    );
}

#[test]
fn scope_and_retired_or_unqualified_modes_are_rejected() {
    let mut input = gateway();
    input.client_ipv4s = Some(vec![]);
    assert!(plan_owned_rules(&input).is_err());
    for datapath in ["", "tproxy", "routed-tun;id"] {
        let mut input = devices();
        input.datapath = datapath.into();
        assert!(plan_owned_rules(&input).is_err());
    }
    for mode in ["follow", "block", "secret-mode"] {
        let mut input = devices();
        input.ipv6 = mode.into();
        let error = plan_owned_rules(&input).unwrap_err().to_string();
        assert!(!error.contains(mode));
    }
    let mut input = devices();
    input.failure = "block-proxy".into();
    assert!(plan_owned_rules(&input).is_err());
}

#[test]
fn lower_camel_dto_preserves_nil_and_acronyms_and_denies_unknown_fields() {
    let raw = json!({"scope":"gateway","lanIPv4Prefixes":["192.168.50.0/24"],
        "datapath":"routed-tun","tunInterface":"b6p-tun","tunAddress":"172.31.255.253/30",
        "clientIPv4":"","clientIPv6":"","clientIPv4s":null,"clientIPv6s":null,"clientMACs":null,
        "lanInterface":"br-lan","ports":{"mixed":2080,"tProxy":0,"dns":1053},
        "ipv6":"direct","failure":"direct","endpointIPs":null,"managementIPs":[],
        "routerDNSAddresses":null,"fakeIP":false});
    let input: RulesPlanInput = serde_json::from_value(raw.clone()).unwrap();
    assert!(plan_owned_rules(&input).is_ok());
    assert!(input.client_ipv4s.is_none());
    assert!(input.management_ips.is_empty());
    for (field, value) in [
        ("clientIPv4s", json!([])),
        ("clientIPv6s", json!([])),
        ("clientMACs", json!({})),
    ] {
        let mut raw = raw.clone();
        raw[field] = value;
        let input: RulesPlanInput = serde_json::from_value(raw).unwrap();
        assert!(plan_owned_rules(&input).is_err());
    }
    let mut unknown = raw.clone();
    unknown["runCommand"] = json!("forbidden");
    assert!(serde_json::from_value::<RulesPlanInput>(unknown).is_err());
    let mut unknown_port = raw;
    unknown_port["ports"]["Mixed"] = json!(2081);
    assert!(serde_json::from_value::<RulesPlanInput>(unknown_port).is_err());
}

#[test]
fn canonical_prefixes_sort_lexically_and_never_broaden_host_bits() {
    use be6500_panel::capture_plan::canonical_gateway_prefixes;
    let mut input = gateway();
    input.lan_ipv4_prefixes = vec![
        "192.168.9.7/32".into(),
        "10.20.0.0/24".into(),
        "172.16.8.0/24".into(),
    ];
    let original = input.lan_ipv4_prefixes.clone();
    let plan = plan_owned_rules(&input).unwrap();
    assert_eq!(input.lan_ipv4_prefixes, original);
    assert_eq!(
        plan.ownership.lan_ipv4_prefixes,
        ["10.20.0.0/24", "172.16.8.0/24", "192.168.9.7/32"]
    );
    assert!(has(&plan.apply[1], &["from", "10.20.0.0/24"]));
    assert!(has(&plan.apply[2], &["from", "172.16.8.0/24"]));
    assert!(has(&plan.apply[3], &["from", "192.168.9.7/32"]));
    for bad in [
        "192.168.50.1/24",
        "192.168.50.0/024",
        "192.168.050.0/24",
        " 192.168.50.0/24",
        "192.168.50.0/24 ",
        "192.168.51.0/23",
        "192.168.50.0/16",
        "192.168.0.0/15",
        "172.16.0.0/11",
        "10.0.0.0/7",
        "203.0.113.0/24",
        "100.64.0.0/10",
        "fd00::/64",
        "0.0.0.0/0",
        "10.0.0.0/33",
        "router.local/24",
    ] {
        let raw = vec![bad.into()];
        assert!(canonical_gateway_prefixes(&raw).is_err(), "{bad}");
    }
    for raw in [
        vec![],
        vec!["192.168.0.0/16".into(), "192.168.50.0/24".into()],
        vec!["10.0.0.0/8".into(), "10.0.0.0/8".into()],
        vec!["10.1.1.1/32".into(); 9],
    ] {
        assert!(canonical_gateway_prefixes(&raw).is_err());
    }
    for good in [
        "10.0.0.0/8",
        "172.16.0.0/12",
        "192.168.0.0/16",
        "192.168.50.0/24",
        "192.168.50.123/32",
    ] {
        assert!(canonical_gateway_prefixes(&[good.into()]).is_ok(), "{good}");
    }
}

#[test]
fn device_metadata_deduplicates_lexically_and_ipv6_stays_direct_only() {
    let mut input = devices();
    input.scope = "devices".into();
    input.client_ipv4s = Some(vec![
        "192.168.50.2".into(),
        "192.168.50.10".into(),
        "192.168.50.2".into(),
    ]);
    input
        .client_macs
        .as_mut()
        .unwrap()
        .insert("192.168.50.2".into(), "02aA.bBcC.dD02".into());
    input.client_ipv6 = "2001:0DB8:0:0::9".into();
    input.client_ipv6s = Some(vec!["2001:db8::2".into(), "2001:db8::9".into()]);
    let plan = plan_owned_rules(&input).unwrap();
    assert_eq!(plan.ownership.client_ipv4, "");
    assert_eq!(
        plan.ownership.client_ipv4s,
        ["192.168.50.10", "192.168.50.2"]
    );
    assert_eq!(plan.ownership.client_ipv6, "");
    assert_eq!(plan.ownership.client_ipv6s, ["2001:db8::2", "2001:db8::9"]);
    assert_eq!(
        plan.ownership.client_macs["192.168.50.2"],
        "02:aa:bb:cc:dd:02"
    );
    assert!(has(&plan.apply[1], &["from", "192.168.50.10/32"]));
    assert!(has(&plan.apply[2], &["from", "192.168.50.2/32"]));
    let value = serde_json::to_value(plan).unwrap();
    assert!(value["Ownership"].get("Scope").is_none()); // legacy device shape
    assert_eq!(value["Ownership"]["ClientIPv6s"][0], "2001:db8::2");
    assert!(!value["Apply"].to_string().contains("ip6tables"));
    assert!(!value["Apply"].to_string().contains("2001:db8"));
    input
        .client_macs
        .as_mut()
        .unwrap()
        .insert("2001:db8::2".into(), "02:aa:bb:cc:dd:02".into());
    assert!(plan_owned_rules(&input).is_err()); // direct IPv6 is not an active MAC source
}

#[test]
fn exact_source_and_mac_validation_refuses_unicast_or_map_gaps() {
    for bad in [
        "",
        "0.0.0.0",
        "127.0.0.1",
        "224.0.0.1",
        "255.255.255.255",
        "192.168.50.10/32",
        "192.168.50.10,192.168.50.11",
        "router.local",
        "::ffff:192.168.50.10",
        "2001:db8::1",
        "192.168.050.10",
    ] {
        let mut input = devices();
        input.client_ipv4 = bad.into();
        assert!(plan_owned_rules(&input).is_err(), "{bad}");
    }
    for bad in [
        "::",
        "::1",
        "ff02::1",
        "::ffff:192.168.50.10",
        "fe80::1%br-lan",
        "2001:db8::1/128",
        "192.168.50.10",
    ] {
        let mut input = devices();
        input.client_ipv6 = bad.into();
        assert!(plan_owned_rules(&input).is_err(), "{bad}");
    }
    for bad in [
        "00:00:00:00:00:00",
        "01:aa:bb:cc:dd:ee",
        "ff:ff:ff:ff:ff:ff",
        "02:aa:bb:cc:dd",
        "02:aa:bb:cc:dd:ee:ff:00",
        "02:aa:bb-cc:dd:ee",
        "02aabbccddee",
        "02:GG:bb:cc:dd:ee",
        "+2:aa:bb:cc:dd:ee",
    ] {
        let mut input = devices();
        input
            .client_macs
            .as_mut()
            .unwrap()
            .insert(input.client_ipv4.clone(), bad.into());
        assert!(plan_owned_rules(&input).is_err(), "{bad}");
    }
    for good in ["02:AA:BB:CC:DD:EE", "02-AA-BB-CC-DD-EE", "02AA.BBCC.DDEE"] {
        let mut input = devices();
        input
            .client_macs
            .as_mut()
            .unwrap()
            .insert(input.client_ipv4.clone(), good.into());
        assert_eq!(
            plan_owned_rules(&input).unwrap().ownership.client_macs[&input.client_ipv4],
            "02:aa:bb:cc:dd:ee"
        );
    }
    let mut input = devices();
    input.client_macs = None;
    assert!(plan_owned_rules(&input).is_err());
    let mut input = devices();
    input.client_macs.as_mut().unwrap().clear();
    assert!(plan_owned_rules(&input).is_err());
    let mut input = devices();
    input
        .client_macs
        .as_mut()
        .unwrap()
        .insert("192.168.50.99".into(), "02:aa:bb:cc:dd:99".into());
    assert!(plan_owned_rules(&input).is_err());
}

#[test]
fn gateway_has_no_client_inventory_authority() {
    let mut input = gateway();
    input.client_ipv4 = "192.168.50.10".into();
    assert!(plan_owned_rules(&input).is_err());
    let mut input = gateway();
    input.client_ipv6 = "2001:db8::1".into();
    assert!(plan_owned_rules(&input).is_err());
    let mut input = gateway();
    input.client_ipv4s = Some(vec![]);
    assert!(plan_owned_rules(&input).is_err());
    let mut input = gateway();
    input.client_ipv6s = Some(vec![]);
    assert!(plan_owned_rules(&input).is_err());
    let mut input = gateway();
    input.client_macs = Some(Default::default());
    assert!(plan_owned_rules(&input).is_err());
    let plan = plan_owned_rules(&gateway()).unwrap();
    assert_eq!(plan.ownership.scope, "gateway");
    assert!(plan.ownership.client_macs.is_empty());
    assert!(
        plan.apply
            .iter()
            .all(|c| !c.iter().any(|a| a == "--mac-source"))
    );
}

#[test]
fn ports_default_only_for_all_zero_devices_and_gateway_ignores_unused_tproxy() {
    use be6500_panel::native::Ports;
    let devices = devices();
    assert_eq!(devices.ports, Ports::default());
    assert!(
        plan_owned_rules(&devices)
            .unwrap()
            .apply
            .iter()
            .any(|c| has(c, &["--to-ports", "1053"]))
    );
    for ports in [
        Ports {
            mixed: 2080,
            tproxy: 0,
            dns: 1053,
        },
        Ports {
            mixed: 2080,
            tproxy: 7894,
            dns: 1053,
        },
        Ports {
            mixed: 7893,
            tproxy: 7893,
            dns: 1053,
        },
        Ports {
            mixed: 2080,
            tproxy: 7893,
            dns: 7893,
        },
        Ports {
            mixed: 1053,
            tproxy: 7893,
            dns: 1053,
        },
        Ports {
            mixed: 0,
            tproxy: 7893,
            dns: 1053,
        },
    ] {
        let mut input = devices.clone();
        input.ports = ports;
        assert!(plan_owned_rules(&input).is_err());
    }
    for tproxy in [0, 53, 1053, 2080, 7893, 65535] {
        let mut input = gateway();
        input.ports.tproxy = tproxy;
        assert!(plan_owned_rules(&input).is_ok());
    }
    let mut input = gateway();
    input.ports.mixed = 7893;
    assert!(plan_owned_rules(&input).is_ok());
    for ports in [
        Ports::default(),
        Ports {
            mixed: 2080,
            tproxy: 0,
            dns: 0,
        },
        Ports {
            mixed: 1053,
            tproxy: 65535,
            dns: 1053,
        },
    ] {
        let mut input = gateway();
        input.ports = ports;
        assert!(plan_owned_rules(&input).is_err());
    }
    assert!(serde_json::from_value::<RulesPlanInput>(json!({"ports":{"mixed":65536}})).is_err());
    assert!(serde_json::from_value::<RulesPlanInput>(json!({"ports":{"dns":-1}})).is_err());
}

#[test]
fn interface_names_are_owned_exact_and_bounded() {
    for bad in [
        "",
        "-br-lan",
        "br-lan+",
        "br lan",
        "br-lan\n",
        "br-lan;id",
        "abcdefghijklmnop",
        "网卡",
    ] {
        let mut input = gateway();
        input.lan_interface = bad.into();
        assert!(plan_owned_rules(&input).is_err(), "{bad}");
    }
    for bad in [
        "",
        "tun0",
        "b6p-",
        "b6p--x",
        "b6p-x.y",
        "b6p-x:y",
        "b6p-x+",
        "b6p-abcdefghijkl",
        "b6p-x;id",
        "b6p-网卡",
    ] {
        let mut input = gateway();
        input.tun_interface = bad.into();
        assert!(plan_owned_rules(&input).is_err(), "{bad}");
    }
    let mut input = gateway();
    input.lan_interface = "b6p-tun".into();
    assert!(plan_owned_rules(&input).is_err());
    for good in ["br-lan", "eth0.1", "lan:1", "abcdefghijklmno", "_lan"] {
        let mut input = gateway();
        input.lan_interface = good.into();
        assert!(plan_owned_rules(&input).is_ok());
    }
    for good in ["b6p-x", "b6p-abcdefghijk", "b6p-0_-x"] {
        let mut input = gateway();
        input.tun_interface = good.into();
        assert!(plan_owned_rules(&input).is_ok());
    }
}

#[test]
fn tun_must_be_private_first_usable_30_and_collision_free() {
    for bad in [
        "172.31.255.252/30",
        "172.31.255.254/30",
        "172.31.255.255/30",
        "172.31.255.253/31",
        "172.31.255.253",
        "172.31.255.253/030",
        "203.0.113.1/30",
        "100.64.0.1/30",
        "fd00::1/30",
    ] {
        let mut input = gateway();
        input.tun_address = bad.into();
        assert!(plan_owned_rules(&input).is_err(), "{bad}");
    }
    let mut input = gateway();
    input.lan_ipv4_prefixes = vec!["172.31.255.0/24".into()];
    assert!(plan_owned_rules(&input).is_err());
    let mut input = gateway();
    input.lan_ipv4_prefixes = vec!["172.31.255.254/32".into()];
    assert!(plan_owned_rules(&input).is_err());
    for address in [
        "172.31.255.252",
        "172.31.255.253",
        "172.31.255.254",
        "172.31.255.255",
    ] {
        let mut input = devices();
        input.client_ipv4 = address.into();
        input.client_macs = Some([(address.into(), "02:aa:bb:cc:dd:01".into())].into());
        assert!(plan_owned_rules(&input).is_err());
        let mut input = gateway();
        input.management_ips = vec![address.into()];
        assert!(plan_owned_rules(&input).is_err());
        let mut input = gateway();
        input.endpoint_ips = vec![address.into()];
        assert!(plan_owned_rules(&input).is_err());
    }
    let mut input = gateway();
    input.tun_address = "10.255.255.253/30".into();
    assert!(plan_owned_rules(&input).is_ok());
}

#[test]
fn router_dns_is_exact_unicast_management_subset_and_ipv6_is_ignored() {
    let mut input = gateway();
    input.router_dns_addresses = vec!["192.168.50.1".into(), "2001:0DB8::1".into()];
    input.management_ips.push("2001:db8::1".into());
    let plan = plan_owned_rules(&input).unwrap();
    assert!(plan.apply.iter().any(|c| has(
        c,
        &[
            "-d",
            "192.168.50.1/32",
            "-p",
            "tcp",
            "--dport",
            "53",
            "-j",
            "REDIRECT"
        ]
    )));
    assert!(
        plan.apply
            .iter()
            .all(|c| !c.iter().any(|a| a.contains("2001:db8")))
    );
    let mut unmatched = gateway();
    unmatched.router_dns_addresses = vec!["192.168.50.2".into()];
    assert!(plan_owned_rules(&unmatched).is_err());
    for bad in [
        "0.0.0.0",
        "127.0.0.1",
        "224.0.0.1",
        "169.254.1.2",
        "255.255.255.255",
        "::",
        "::1",
        "ff02::1",
        "fe80::1",
    ] {
        let mut input = gateway();
        input.management_ips.push(bad.into());
        input.router_dns_addresses = vec![bad.into()];
        assert!(plan_owned_rules(&input).is_err(), "{bad}");
    }
    for field in ["endpointIPs", "managementIPs", "routerDNSAddresses"] {
        for bad in [
            "private-secret.example",
            "1.2.3.4/32",
            "::ffff:1.2.3.4",
            "fe80::1%br-lan",
            "1.2.3.4;id",
        ] {
            let mut raw = json!({"scope":"gateway", "datapath":"routed-tun", "lanIPv4Prefixes":["192.168.50.0/24"],
                "lanInterface":"br-lan", "tunInterface":"b6p-tun", "tunAddress":"172.31.255.253/30", "ports":{"mixed":2080,"dns":1053}});
            raw[field] = json!([bad]);
            let input: RulesPlanInput = serde_json::from_value(raw).unwrap();
            let error = plan_owned_rules(&input).unwrap_err().to_string();
            assert!(!error.contains(bad));
        }
    }
}

#[test]
fn dns_opt_in_precedes_local_and_managed_dns_precedes_endpoint_private_returns() {
    let mut input = gateway();
    input.router_dns_addresses = vec!["192.168.50.1".into()];
    input.endpoint_ips = vec![
        "192.168.50.1".into(),
        "203.0.113.9".into(),
        "10.2.3.4".into(),
    ];
    let plan = plan_owned_rules(&input).unwrap();
    let nat: Vec<_> = plan
        .apply
        .iter()
        .filter(|c| has(c, &["-t", "nat", "-A", "B6P_V4_DNS"]))
        .collect();
    assert!(has(
        nat[0],
        &[
            "-d",
            "192.168.50.1/32",
            "-p",
            "tcp",
            "--dport",
            "53",
            "-j",
            "REDIRECT"
        ]
    ));
    assert!(has(
        nat[1],
        &[
            "-d",
            "192.168.50.1/32",
            "-p",
            "udp",
            "--dport",
            "53",
            "-j",
            "REDIRECT"
        ]
    ));
    assert!(has(nat[2], &["--dst-type", "LOCAL", "-j", "RETURN"]));
    assert!(has(nat[3], &["-d", "192.168.50.1/32", "-j", "RETURN"]));
    for protocol in ["tcp", "udp"] {
        let dns = nat
            .iter()
            .position(|c| {
                has(
                    c,
                    &[
                        "-A",
                        "B6P_V4_DNS",
                        "-p",
                        protocol,
                        "--dport",
                        "53",
                        "-j",
                        "REDIRECT",
                    ],
                )
            })
            .unwrap();
        for destination in ["203.0.113.9/32", "10.2.3.4/32", "10.0.0.0/8"] {
            let bypass = nat
                .iter()
                .position(|c| has(c, &["-d", destination, "-j", "RETURN"]))
                .unwrap();
            assert!(dns < bypass);
        }
        assert!(plan.apply.iter().any(|c| has(
            c,
            &[
                "-A",
                "B6P_V4_TUN_MARK",
                "-p",
                protocol,
                "--dport",
                "53",
                "-j",
                "RETURN"
            ]
        )));
    }
    assert_eq!(
        nat.iter()
            .filter(|c| has(c, &["-d", "192.168.50.1/32", "-j", "RETURN"]))
            .count(),
        1
    );
}

#[test]
fn fake_ip_marking_is_opt_in_after_prelude_but_before_private_bypass() {
    for fake_ip in [false, true] {
        let mut input = gateway();
        input.fake_ip = fake_ip;
        input.endpoint_ips.push("198.18.1.2".into());
        let plan = plan_owned_rules(&input).unwrap();
        let mark: Vec<_> = plan
            .apply
            .iter()
            .filter(|c| has(c, &["-A", "B6P_V4_TUN_MARK"]))
            .collect();
        let private = mark
            .iter()
            .position(|c| has(c, &["-d", "198.18.0.0/15", "-j", "RETURN"]))
            .unwrap();
        let endpoint = mark
            .iter()
            .position(|c| has(c, &["-d", "198.18.1.2/32", "-j", "RETURN"]))
            .unwrap();
        let fake: Vec<_> = mark
            .iter()
            .enumerate()
            .filter(|(_, c)| has(c, &["-d", "198.18.0.0/15", "-j", "MARK"]))
            .collect();
        assert_eq!(fake.len(), if fake_ip { 2 } else { 0 });
        for (index, c) in fake {
            assert!(endpoint < index && index < private);
            assert!(has(c, &["--set-xmark", "0x4000/0x4000"]));
        }
        assert_eq!(plan.warnings.len(), if fake_ip { 10 } else { 8 });
    }
}

#[test]
fn gateway_return_preserves_management_and_private_stack_tuple_is_narrow() {
    let input = gateway();
    let plan = plan_owned_rules(&input).unwrap();
    let returns: Vec<_> = plan
        .apply
        .iter()
        .filter(|c| has(c, &["-A", "B6P_V4_TUN_RETURN"]))
        .collect();
    assert!(has(returns[0], &["-d", "192.168.50.1/32", "-j", "RETURN"]));
    assert!(has(returns[1], &["-p", "tcp", "-j", "ACCEPT"]));
    assert!(has(returns[2], &["-p", "udp", "-j", "ACCEPT"]));
    assert!(has(returns[3], &["-j", "RETURN"]));
    let input_hook = plan
        .apply
        .iter()
        .find(|c| has(c, &["-I", "INPUT"]))
        .unwrap();
    assert!(has(
        input_hook,
        &[
            "-i",
            "b6p-tun",
            "-s",
            "172.31.255.254/32",
            "-d",
            "172.31.255.253/32",
            "-p",
            "tcp",
            "-j",
            "B6P_V4_TUN_INPUT"
        ]
    ));
    let output_hook = plan
        .apply
        .iter()
        .find(|c| has(c, &["-I", "OUTPUT"]))
        .unwrap();
    assert!(has(
        output_hook,
        &[
            "-o",
            "b6p-tun",
            "-s",
            "172.31.255.253/32",
            "-d",
            "172.31.255.254/32",
            "-p",
            "tcp",
            "-j",
            "B6P_V4_TUN_OUTPUT"
        ]
    ));
    for c in plan
        .apply
        .iter()
        .filter(|c| has(c, &["-I", "FORWARD"]) && has(c, &["-j", "B6P_V4_TUN_RETURN"]))
    {
        assert!(has(
            c,
            &["-i", "b6p-tun", "-o", "br-lan", "-d", "192.168.50.0/24"]
        ));
        assert!(c.iter().any(|s| s == "tcp" || s == "udp"));
    }
}

#[test]
fn cleanup_reverses_all_hooks_before_owned_deletion_and_has_independent_storage() {
    let mut input = devices();
    input.client_ipv4s = Some(vec!["192.168.50.2".into()]);
    input
        .client_macs
        .as_mut()
        .unwrap()
        .insert("192.168.50.2".into(), "02:aa:bb:cc:dd:02".into());
    let mut plan = plan_owned_rules(&input).unwrap();
    let hooks: Vec<_> = plan
        .apply
        .iter()
        .filter(|c| c.get(5).is_some_and(|s| s == "-I"))
        .cloned()
        .collect();
    for (apply, cleanup) in hooks.iter().rev().zip(&plan.cleanup) {
        let mut expected = apply.clone();
        expected[5] = "-D".into();
        expected.remove(7);
        assert_eq!(&expected, cleanup);
    }
    assert!(
        plan.cleanup[hooks.len()..]
            .iter()
            .all(|c| c.get(5).is_none_or(|s| s != "-D"))
    );
    let mut index = hooks.len();
    for chain in plan.ownership.chains.iter().rev() {
        assert!(has(&plan.cleanup[index], &["-F", &chain.name]));
        index += 1;
        assert!(has(&plan.cleanup[index], &["-X", &chain.name]));
        index += 1;
    }
    assert!(has(&plan.cleanup[index], &["from", "192.168.50.2/32"]));
    assert!(has(&plan.cleanup[index + 1], &["from", "192.168.50.10/32"]));
    assert_eq!(
        plan.cleanup.last().unwrap(),
        &[
            "ip", "-4", "route", "del", "default", "dev", "b6p-tun", "table", "16500"
        ]
    );
    let original_failure = plan.on_failure.clone();
    plan.cleanup[0][0] = "mutated".into();
    plan.cleanup[1].push("mutated".into());
    assert_eq!(plan.on_failure, original_failure);
}

#[test]
fn all_generated_resources_stay_in_the_fixed_capture_namespace() {
    let plan = plan_owned_rules(&devices()).unwrap();
    for c in plan
        .apply
        .iter()
        .chain(&plan.cleanup)
        .chain(&plan.on_failure)
    {
        assert!(matches!(c[0].as_str(), "ip" | "iptables"));
        for forbidden in [
            "sysctl",
            "conntrack",
            "offload",
            "netns",
            "link",
            "addr",
            "flush",
            "local",
            "lo",
            "TPROXY",
            "-P",
            "-Z",
            "--on-port",
            "DROP",
            "REJECT",
        ] {
            assert!(!c.iter().any(|a| a == forbidden), "{c:?}");
        }
        if c[0] == "ip" {
            assert_eq!(c[1], "-4");
            assert!(c.iter().any(|a| a == "16500"));
        } else {
            assert_eq!(&c[1..4], &["-w", "5", "-t"]);
            assert!(matches!(c[4].as_str(), "mangle" | "nat" | "filter"));
            if matches!(c[5].as_str(), "-F" | "-X" | "-N" | "-A") {
                assert!(c[6].starts_with("B6P_V4_"));
            }
            if c.iter().any(|a| a == "OUTPUT") {
                assert_eq!(c[4], "filter");
                assert!(has(c, &["-j", "B6P_V4_TUN_OUTPUT"]));
            }
        }
    }
}

#[test]
fn raw_bounds_are_checked_before_dedup_and_largest_intent_is_bounded() {
    use be6500_panel::capture_plan::MAX_PLAN_COMMANDS;
    let mut input = devices();
    input.client_ipv4s = Some(vec![input.client_ipv4.clone(); 64]);
    assert!(plan_owned_rules(&input).is_err());
    let mut input = devices();
    input.client_ipv6 = "2001:db8::1".into();
    input.client_ipv6s = Some(vec![input.client_ipv6.clone(); 64]);
    assert!(plan_owned_rules(&input).is_err());
    let mut input = devices();
    input.client_macs = Some(
        (0..129)
            .map(|i| (format!("10.0.0.{i}"), "02:aa:bb:cc:dd:01".into()))
            .collect(),
    );
    assert!(plan_owned_rules(&input).is_err());
    let mut input = gateway();
    input.endpoint_ips = vec!["203.0.113.9".into(); 257];
    assert!(plan_owned_rules(&input).is_err());
    let mut input = gateway();
    input.management_ips = vec!["192.168.50.1".into(); 129];
    assert!(plan_owned_rules(&input).is_err());
    let mut input = gateway();
    input.router_dns_addresses = vec!["192.168.50.1".into(); 17];
    assert!(plan_owned_rules(&input).is_err());
    let mut input = devices();
    input.client_ipv4.clear();
    input.client_ipv4s = Some((1..=64).map(|i| format!("192.168.50.{i}")).collect());
    input.client_macs = Some(
        (1..=64)
            .map(|i| (format!("192.168.50.{i}"), format!("02:aa:bb:cc:dd:{i:02x}")))
            .collect(),
    );
    input.client_ipv6s = Some((1..=64).map(|i| format!("2001:db8::{i:x}")).collect());
    input.endpoint_ips = (0..256).map(|i| format!("203.0.113.{i}")).collect();
    input.management_ips = (0..128).map(|i| format!("10.1.1.{i}")).collect();
    input.router_dns_addresses = (1..=16).map(|i| format!("10.1.1.{i}")).collect();
    let plan = plan_owned_rules(&input).unwrap();
    assert_eq!(plan.ownership.client_ipv4s.len(), 64);
    assert!(plan.apply.len() + plan.cleanup.len() + plan.on_failure.len() < MAX_PLAN_COMMANDS);
    assert!(serde_json::to_vec(&plan).unwrap().len() < 1024 * 1024);
    assert_eq!(plan, plan_owned_rules(&input).unwrap());
}

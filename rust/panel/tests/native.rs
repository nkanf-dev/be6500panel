use be6500_panel::native::{
    CompileInput, DNSEndpoint, LocalDNSConfig, MAX_CONFIG_BYTES, Node, Ports, RoutedTUNConfig,
    RuleSetReference, compile_native,
};
use be6500_panel::policy::{Diagnostic, Rule, RuleKind, Target};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn node() -> Node {
    serde_json::from_value(json!({
        "id": "synthetic-node", "name": "Synthetic only", "server": "example.com", "port": 443,
        "uuid": "12345678-1234-1234-1234-123456789abc", "serverName": "node.example",
        "realityPublicKey": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", "realityShortID": "0123456789abcdef",
        "fingerprint": "chrome", "flow": "xtls-rprx-vision", "udp": true
    })).unwrap()
}
fn input() -> CompileInput {
    CompileInput {
        node: node(),
        ..CompileInput::default()
    }
}
fn config(input: &CompileInput) -> Value {
    serde_json::from_slice(&compile_native(input).unwrap().config).unwrap()
}

#[test]
fn defaults_keep_original_packet_and_native_credentials() {
    let input = input();
    let output = compile_native(&input).unwrap();
    let c: Value = serde_json::from_slice(&output.config).unwrap();
    assert_eq!(output.core_version, "1.14.2");
    assert_eq!(output.ipv6, "direct");
    assert_eq!(output.failure, "direct");
    assert_eq!(
        output.sha256,
        format!("{:x}", Sha256::digest(&output.config))
    );
    assert_eq!(output.config.last(), Some(&b'\n'));
    assert_eq!(
        c["inbounds"][0],
        json!({"type":"mixed","tag":"mixed-in","listen":"127.0.0.1","listen_port":2080})
    );
    assert_eq!(
        c["inbounds"][1],
        json!({"type":"tun","tag":"tun-in","interface_name":"b6p-tun","address":["172.31.255.253/30"],"mtu":1500,"dns_mode":"disabled","auto_route":false,"auto_redirect":false,"stack":"system","udp_timeout":"2m","udp_nat_max":1024})
    );
    assert_eq!(c["inbounds"][2]["listen_port"], 1053);
    let proxy = &c["outbounds"][1];
    assert_eq!(proxy["uuid"], input.node.uuid);
    assert_eq!(
        proxy["tls"]["reality"]["public_key"],
        input.node.reality_public_key
    );
    assert_eq!(
        proxy["tls"]["reality"]["short_id"],
        input.node.reality_short_id
    );
    assert_eq!(proxy["tls"]["utls"]["fingerprint"], "chrome");
    assert_eq!(proxy["flow"], "xtls-rprx-vision");
    assert_eq!(proxy["packet_encoding"], "xudp");
    assert!(proxy.get("network").is_none());
    assert!(proxy.get("transport").is_none());
    assert_eq!(
        output.required_features,
        [
            "with_utls",
            "badlinkname",
            "tcp_fast_open",
            "system_tun_tcp_udp",
            "tls_dns"
        ]
    );
    assert!(input.datapath.is_empty());
    assert!(input.routed_tun.is_none());
}

#[test]
fn private_debug_and_public_metadata_never_echo_credentials() {
    let mut input = input();
    input.accept_unsupported_rules = true;
    input.diagnostics = serde_json::from_value(
        json!([{"scope":"rule","index":99,"code":"private-credential","message":input.node.uuid}]),
    )
    .unwrap();
    let out = compile_native(&input).unwrap();
    let public = serde_json::to_string(&out).unwrap();
    for secret in [
        &input.node.uuid,
        &input.node.reality_public_key,
        &input.node.reality_short_id,
    ] {
        assert!(!public.contains(secret));
        assert!(
            !format!("{input:?} {input:#?} {:?} {out:?} {out:#?}", input.node).contains(secret)
        );
    }
    assert!(!public.contains("private-credential"));
    assert!(!public.contains("example.com"));
    input.node.uuid = "private-credential".into();
    let error = compile_native(&input).unwrap_err();
    assert!(!format!("{error} {error:?}").contains("private-credential"));
}

#[test]
fn retired_and_unsupported_policies_are_rejected() {
    for (field, value) in [
        ("datapath", "tproxy"),
        ("datapath", "unknown"),
        ("ipv6", "follow"),
        ("ipv6", "block"),
        ("failure", "block-proxy"),
    ] {
        let mut in_value = json!({"node": serde_json::from_str::<Value>(r#"{}"#).unwrap()});
        // Explicit DTO mapping avoids Serialize on private input types.
        in_value["node"] = synthetic_node_json();
        in_value[field] = value.into();
        let input: CompileInput = serde_json::from_value(in_value).unwrap();
        assert!(compile_native(&input).is_err());
    }
}

fn synthetic_node_json() -> Value {
    json!({"server":"example.com","port":443,"uuid":"12345678-1234-1234-1234-123456789abc","serverName":"node.example","realityPublicKey":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","realityShortID":"0123456789abcdef","fingerprint":"chrome","flow":"xtls-rprx-vision","udp":true})
}

fn rule(kind: RuleKind, value: &str, target: Target, index: i64) -> Rule {
    Rule {
        kind,
        value: value.into(),
        target,
        no_resolve: false,
        index,
    }
}
fn refs() -> Vec<RuleSetReference> {
    ["proxy-domain", "cn-ip", "cn-domain"]
        .iter()
        .map(|tag| RuleSetReference {
            tag: (*tag).into(),
            kind: if *tag == "cn-ip" { "ip" } else { "domain" }.into(),
            path: format!("/synthetic/nonexistent/{tag}.srs"),
            sha256: "01".repeat(32),
            source_url: format!("https://example.com/{tag}.srs"),
            max_bytes: 2 << 20,
        })
        .collect()
}
fn position(rules: &[Value], key: &str, value: &str) -> usize {
    rules
        .iter()
        .position(|rule| {
            rule[key]
                .as_array()
                .is_some_and(|a| a.contains(&json!(value)))
        })
        .unwrap()
}

#[test]
fn mandatory_prelude_order_overrides_fallback_and_terminal_actions() {
    let mut input = input();
    input.rule_sets = refs();
    input.endpoints = vec!["203.0.113.7".into()];
    input.management_ips = vec!["192.168.31.1".into()];
    input.bootstrap_domains = vec!["boot.example".into()];
    input.overrides = vec![rule(
        RuleKind::Domain,
        "override.example",
        Target::Block,
        -1,
    )];
    input.rules = vec![
        rule(RuleKind::Domain, "direct.example", Target::Direct, 0),
        rule(RuleKind::DomainSuffix, "proxy.example", Target::Proxy, 1),
        rule(RuleKind::DomainKeyword, "blocked", Target::Block, 2),
        Rule {
            no_resolve: true,
            ..rule(RuleKind::IpCidr, "203.0.113.0/24", Target::Block, 3)
        },
        rule(RuleKind::Match, "", Target::Direct, 4),
        rule(RuleKind::Domain, "unreachable.example", Target::Block, 5),
    ];
    let output = compile_native(&input).unwrap();
    let c: Value = serde_json::from_slice(&output.config).unwrap();
    let route = c["route"]["rules"].as_array().unwrap();
    assert_eq!(
        route[0],
        json!({"action":"hijack-dns","inbound":["dns-in"]})
    );
    assert_eq!(
        route[3],
        json!({"action":"hijack-dns","inbound":["mixed-in","tun-in"],"port":[53]})
    );
    let sniff = route.iter().position(|r| r["action"] == "sniff").unwrap();
    assert_eq!(
        route[sniff]["sniffer"],
        json!(["http", "tls", "dns", "quic"])
    );
    assert_eq!(route[sniff]["timeout"], "300ms");
    for (key, value) in [
        ("ip_cidr", "192.168.31.1/32"),
        ("ip_cidr", "203.0.113.7/32"),
        ("ip_cidr", "223.5.5.5/32"),
        ("domain", "boot.example"),
        ("domain", "example.com"),
        ("domain_suffix", "lan"),
    ] {
        assert!(position(route, key, value) < sniff);
    }
    assert_eq!(
        route[sniff - 1],
        json!({"ip_version":6,"outbound":"direct"})
    );
    let mut previous = sniff;
    for (key, value, action_key, action) in [
        ("domain", "override.example", "action", "reject"),
        ("domain", "direct.example", "outbound", "direct"),
        ("domain_suffix", "proxy.example", "outbound", "proxy"),
        ("domain_keyword", "blocked", "action", "reject"),
        ("ip_cidr", "203.0.113.0/24", "action", "reject"),
        ("rule_set", "cn-domain", "outbound", "direct"),
        ("rule_set", "proxy-domain", "outbound", "proxy"),
        ("rule_set", "cn-ip", "outbound", "direct"),
    ] {
        let pos = position(route, key, value);
        assert!(pos > previous);
        previous = pos;
        assert_eq!(route[pos][action_key], action);
    }
    assert_ne!(
        route[position(route, "ip_cidr", "203.0.113.0/24") - 1]["action"],
        "resolve"
    );
    assert_eq!(
        route[position(route, "rule_set", "cn-ip") - 1]["action"],
        "resolve"
    );
    assert_eq!(route.last().unwrap(), &json!({"outbound":"direct"}));
    assert!(
        !String::from_utf8(output.config)
            .unwrap()
            .contains("unreachable.example")
    );
    assert_eq!(
        output.diagnostics,
        vec![Diagnostic {
            scope: "rule".into(),
            index: 5,
            code: "unreachable-rule".into(),
            message: "rule follows terminal MATCH and is unreachable".into()
        }]
    );
    assert_eq!(c["route"]["rule_set"][0]["tag"], "cn-domain");
    assert_eq!(c["route"]["rule_set"][1]["tag"], "cn-ip");
    assert_eq!(c["route"]["rule_set"][2]["tag"], "proxy-domain");
}

#[test]
fn resolving_cidr_and_ip_set_are_explicit_no_resolve_stays_native() {
    for kind in [RuleKind::IpCidr, RuleKind::RuleSet] {
        for no_resolve in [false, true] {
            let mut input = input();
            input.rule_sets = refs();
            let value = if kind == RuleKind::IpCidr {
                "198.51.100.0/24"
            } else {
                "cn-ip"
            };
            input.rules = vec![Rule {
                no_resolve,
                ..rule(kind.clone(), value, Target::Proxy, 0)
            }];
            let c = config(&input);
            let routes = c["route"]["rules"].as_array().unwrap();
            let key = if kind == RuleKind::IpCidr {
                "ip_cidr"
            } else {
                "rule_set"
            };
            let pos = position(routes, key, value);
            assert_eq!(routes[pos - 1]["action"] == "resolve", !no_resolve);
            assert_eq!(routes[pos]["outbound"], "proxy");
        }
    }
}

#[test]
fn terminal_targets_fake_ip_dns_and_unreachable_validation() {
    for target in [Target::Direct, Target::Proxy, Target::Block] {
        let mut input = input();
        input.fake_ip = true;
        input.rules = vec![rule(RuleKind::Match, "", target.clone(), 0)];
        let c = config(&input);
        let route = c["route"]["rules"].as_array().unwrap();
        let dns = c["dns"]["rules"].as_array().unwrap();
        match target {
            Target::Direct => {
                assert_eq!(route.last().unwrap(), &json!({"outbound":"direct"}));
                assert_eq!(
                    dns.last().unwrap(),
                    &json!({"server":"dns-direct","rewrite_ttl":300})
                );
            }
            Target::Block => {
                assert_eq!(route.last().unwrap(), &json!({"action":"reject"}));
                assert_eq!(dns.last().unwrap(), &json!({"action":"reject"}));
            }
            Target::Proxy => {
                assert_eq!(route.last().unwrap(), &json!({"outbound":"proxy"}));
                assert_eq!(
                    dns[dns.len() - 2],
                    json!({"query_type":["A","AAAA"],"server":"dns-fake","rewrite_ttl":60})
                );
                assert_eq!(
                    dns.last().unwrap(),
                    &json!({"server":"dns-proxy","rewrite_ttl":300})
                );
            }
            _ => unreachable!(),
        }
        assert_eq!(
            c["dns"]["servers"][3],
            json!({"type":"fakeip","tag":"dns-fake","inet4_range":"198.18.0.0/15"})
        );
        let aaaa = dns
            .iter()
            .position(|r| r["query_type"] == json!(["AAAA"]))
            .unwrap();
        assert_eq!(dns[aaaa]["server"], "dns-direct");
        input.rules.push(rule(
            RuleKind::Domain,
            "bad private value",
            Target::Proxy,
            1,
        ));
        assert!(
            compile_native(&input).is_err(),
            "invalid unreachable rules must still fail validation"
        );
    }
}

#[test]
fn custom_local_dns_aliases_private_ptr_and_bootstrap_are_authoritative() {
    let mut input = input();
    input.node.server = "203.0.113.9".into();
    input.management_ips = vec!["192.168.31.1".into()];
    input.local_dns = Some(LocalDNSConfig {
        server: "192.168.31.1".into(),
        port: 0,
        domains: vec!["Office.Home.".into(), "office.home".into()],
        hostnames: vec!["Router.Office.Home.".into()],
    });
    input.direct_dns = DNSEndpoint {
        server: "192.0.2.53".into(),
        port: 8853,
        server_name: "resolver.example".into(),
    };
    input.proxy_dns = DNSEndpoint {
        server: "198.51.100.53".into(),
        port: 853,
        server_name: "proxy-resolver.example".into(),
    };
    input.rules = vec![
        rule(RuleKind::Domain, "resolver.example", Target::Proxy, 0),
        rule(RuleKind::DomainSuffix, "office.home", Target::Block, 1),
        rule(RuleKind::Match, "", Target::Proxy, 2),
    ];
    let c = config(&input);
    assert_eq!(c["dns"]["servers"][0]["tls"]["min_version"], "1.2");
    assert_eq!(c["dns"]["servers"][0]["server_port"], 8853);
    assert_eq!(
        c["dns"]["servers"][2],
        json!({"type":"udp","tag":"dns-local","server":"192.168.31.1","server_port":53,"detour":"direct"})
    );
    let dns = c["dns"]["rules"].as_array().unwrap();
    assert_eq!(
        dns[position(dns, "domain", "resolver.example")]["server"],
        "dns-direct"
    );
    for (key, name) in [
        ("domain_suffix", "lan"),
        ("domain_suffix", "office.home"),
        ("domain", "miwifi.com"),
        ("domain", "www.router.miwifi.com"),
        ("domain", "router.office.home"),
        ("domain_regex", "^[^.]+$"),
    ] {
        let pos = position(dns, key, name);
        assert_eq!(dns[pos]["server"], "dns-local");
        assert_eq!(dns[pos]["disable_cache"], true);
        assert!(dns[pos].get("query_type").is_none());
    }
    for name in [
        "10.in-addr.arpa",
        "16.172.in-addr.arpa",
        "31.172.in-addr.arpa",
        "64.100.in-addr.arpa",
        "127.100.in-addr.arpa",
        "c.f.ip6.arpa",
        "b.e.f.ip6.arpa",
    ] {
        let pos = position(dns, "domain_suffix", name);
        assert_eq!(dns[pos]["server"], "dns-local");
        assert_eq!(dns[pos]["query_type"], json!(["PTR"]));
    }
    assert!(
        !dns.iter()
            .any(|r| r["domain_suffix"] == json!(["in-addr.arpa"]))
    );
    let route = c["route"]["rules"].as_array().unwrap();
    let pos = position(route, "domain_suffix", "office.home");
    assert_eq!(route[pos]["action"], "resolve");
    assert_eq!(route[pos]["server"], "dns-local");
    assert_eq!(route[pos + 1]["outbound"], "direct");
    let saved = input.clone();
    assert_eq!(config(&input), c);
    assert_eq!(input, saved);
}

#[test]
fn tun_listener_collisions_and_unused_tproxy_inputs() {
    let mut input = input();
    let first = compile_native(&input).unwrap();
    for tproxy in [0, 53, 1053, 2080, 7893, 65535] {
        input.ports.tproxy = tproxy;
        input.tproxy_listen_address = "unused-private".into();
        assert_eq!(compile_native(&input).unwrap(), first);
    }
    for address in [
        "172.31.255.252",
        "172.31.255.253",
        "172.31.255.254",
        "172.31.255.255",
    ] {
        for field in [
            "node",
            "endpoint",
            "management",
            "direct-dns",
            "proxy-dns",
            "mixed",
            "dns",
            "mapped",
        ] {
            let mut bad = input.clone();
            match field {
                "node" => bad.node.server = address.into(),
                "endpoint" => bad.endpoints = vec![address.into()],
                "management" => bad.management_ips = vec![address.into()],
                "direct-dns" => {
                    bad.direct_dns = DNSEndpoint {
                        server: address.into(),
                        port: 853,
                        server_name: "resolver.example".into(),
                    }
                }
                "proxy-dns" => {
                    bad.proxy_dns = DNSEndpoint {
                        server: address.into(),
                        port: 853,
                        server_name: "resolver.example".into(),
                    }
                }
                "mixed" => bad.mixed_listen_address = address.into(),
                "dns" => bad.dns_listen_address = address.into(),
                "mapped" => bad.dns_listen_address = format!("::ffff:{address}"),
                _ => unreachable!(),
            }
            let error = compile_native(&bad).unwrap_err().to_string();
            assert!(error.contains("address prefix overlaps"));
            assert!(!error.contains(address));
        }
    }
    input.routed_tun = Some(RoutedTUNConfig {
        interface_name: "b6p-TUN_2".into(),
        address: "10.0.0.1/30".into(),
    });
    input.ports = Ports {
        mixed: 12080,
        dns: 11053,
        tproxy: 12080,
    };
    input.mixed_listen_address = "127.0.0.2".into();
    input.dns_listen_address = "::".into();
    let c = config(&input);
    assert_eq!(c["inbounds"][1]["address"], json!(["10.0.0.1/30"]));
    assert_eq!(c["inbounds"][0]["listen_port"], 12080);
    assert_eq!(c["inbounds"][2]["listen"], "::");
    input.local_dns = Some(LocalDNSConfig {
        port: 11053,
        ..Default::default()
    });
    assert!(compile_native(&input).is_err());
}

#[test]
fn invalid_node_fields_refuse_with_fixed_safe_errors() {
    let fields = [
        "server",
        "uuid",
        "serverName",
        "flow",
        "fingerprint",
        "realityPublicKey",
        "realityShortID",
    ];
    for field in fields {
        let mut value = synthetic_node_json();
        value[field] = "private secret".into();
        let bad = CompileInput {
            node: serde_json::from_value(value).unwrap(),
            ..Default::default()
        };
        let error = compile_native(&bad).unwrap_err();
        assert!(!format!("{error} {error:?}").contains("private secret"));
    }
    for server in ["0.0.0.0", "::", "224.0.0.1", "ff02::1", "fe80::1%private"] {
        let mut bad = input();
        bad.node.server = server.into();
        assert!(compile_native(&bad).is_err());
    }
    let mut bad = input();
    bad.node.udp = false;
    assert!(compile_native(&bad).is_err());
    bad = input();
    bad.node.port = 0;
    assert!(compile_native(&bad).is_err());
}

#[test]
fn controlled_metadata_is_bounded_and_never_opens_assets() {
    let mut input = input();
    input.rule_sets = refs();
    let baseline = compile_native(&input).unwrap();
    assert!(
        !String::from_utf8(baseline.config.clone())
            .unwrap()
            .contains("https://")
    );
    input.rule_sets.reverse();
    assert_eq!(compile_native(&input).unwrap(), baseline);
    for field in ["tag", "kind", "path", "sha256", "sourceURL", "maxBytes"] {
        let mut reference = json!({"tag":"cn-domain","kind":"domain","path":"/synthetic/nonexistent/cn-domain.srs","sha256":"01".repeat(32),"sourceURL":"https://example.com/cn.srs","maxBytes":1024});
        reference[field] = match field {
            "maxBytes" => json!(9 << 20),
            "sourceURL" => json!("https://private:secret@example.com/a.srs"),
            _ => json!("private secret"),
        };
        let bad = CompileInput {
            rule_sets: vec![serde_json::from_value(reference).unwrap()],
            ..input.clone()
        };
        let error = compile_native(&bad).unwrap_err();
        assert!(!format!("{error} {error:?}").contains("private secret"));
    }
    input.rule_sets.push(input.rule_sets[0].clone());
    assert!(compile_native(&input).is_err());
    input.rule_sets = vec![refs()[0].clone(), refs()[0].clone()];
    assert!(compile_native(&input).is_err());
    input.rule_sets.clear();
    input.rules = vec![rule(RuleKind::RuleSet, "cn-ip", Target::Direct, 0)];
    assert!(compile_native(&input).is_err());
}

#[test]
fn local_dns_settings_and_tun_ownership_are_validated() {
    let mut input = input();
    for name in [
        "",
        "tun0",
        "br-lan",
        "b6p-",
        "b6p-a.1",
        "b6p-a;private",
        "b6p-123456789012",
    ] {
        input.routed_tun = Some(RoutedTUNConfig {
            interface_name: name.into(),
            address: "10.0.0.1/30".into(),
        });
        assert!(compile_native(&input).is_err());
    }
    for address in [
        "10.0.0.0/30",
        "10.0.0.2/30",
        "10.0.0.3/30",
        "10.0.0.1/29",
        "100.64.0.1/30",
        "8.8.8.9/30",
        "fd00::1/30",
        "010.0.0.1/30",
    ] {
        input.routed_tun = Some(RoutedTUNConfig {
            interface_name: "b6p-tun".into(),
            address: address.into(),
        });
        assert!(compile_native(&input).is_err());
    }
    input.routed_tun = None;
    for server in [
        "resolver.example",
        "0.0.0.0",
        "255.255.255.255",
        "fe80::1",
        "::ffff:127.0.0.1",
        "192.168.31.2",
    ] {
        input.local_dns = Some(LocalDNSConfig {
            server: server.into(),
            ..Default::default()
        });
        assert!(compile_native(&input).is_err());
    }
    input.local_dns = Some(LocalDNSConfig {
        domains: vec!["bad..".into()],
        ..Default::default()
    });
    assert!(compile_native(&input).is_err());
    input.local_dns = Some(LocalDNSConfig {
        hostnames: vec!["bad name".into()],
        ..Default::default()
    });
    assert!(compile_native(&input).is_err());
}

#[test]
fn go_json_escaping_and_determinism_are_preserved() {
    let mut input = input();
    input.rules = vec![rule(
        RuleKind::DomainKeyword,
        "<>&\u{2028}\u{2029}\u{1}é",
        Target::Proxy,
        0,
    )];
    let output = compile_native(&input).unwrap();
    let bytes = String::from_utf8(output.config.clone()).unwrap();
    assert!(bytes.contains(r"\u003c\u003e\u0026\u2028\u2029\u0001é"));
    assert_eq!(
        config(&input)["route"]["rules"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r.get("domain_keyword").is_some())
            .unwrap()["domain_keyword"],
        json!(["<>&\u{2028}\u{2029}\u{1}é"])
    );
    let before = input.clone();
    assert_eq!(compile_native(&input).unwrap(), output);
    assert_eq!(input, before);
}

#[test]
fn input_and_output_resource_limits_are_fail_closed() {
    let mut input = input();
    input.rules = vec![rule(RuleKind::DomainKeyword, &"x".repeat(253), Target::Proxy, 0); 8192];
    input.fake_ip = true;
    assert_eq!(
        compile_native(&input).unwrap_err().to_string(),
        "native configuration output limit exceeded"
    );
    input.rules = vec![rule(RuleKind::Match, "", Target::Direct, 0); 8193];
    assert_eq!(
        compile_native(&input).unwrap_err().to_string(),
        "compiler input limit exceeded"
    );
    input.rules.clear();
    input.endpoints = vec!["192.0.2.2".into(); 257];
    assert!(compile_native(&input).is_err());
    input.endpoints.clear();
    input.management_ips = vec!["192.168.31.1".into(); 129];
    assert!(compile_native(&input).is_err());
    input.management_ips.clear();
    input.bootstrap_domains = vec!["a.example".into(); 257];
    assert!(compile_native(&input).is_err());
    input.bootstrap_domains.clear();
    input.fake_ip = false;
    assert!(compile_native(&input).unwrap().config.len() <= MAX_CONFIG_BYTES);
}

#[test]
fn explicit_fixture_dto_null_slices_and_private_unknown_fields() {
    let dto = json!({"node":synthetic_node_json(),"rules":null,"overrides":null,"ruleSets":null,"endpoints":null,"bootstrapDomains":null,"managementIPs":null,"routedTUN":null,"localDNS":null,"diagnostics":null,"ports":{"mixed":0,"tProxy":1053,"dns":0},"fakeIP":false});
    let decoded: CompileInput = serde_json::from_value(dto.clone()).unwrap();
    assert!(compile_native(&decoded).is_ok());
    let mut invalid = dto;
    invalid["privateTypo"] = "secret".into();
    assert!(serde_json::from_value::<CompileInput>(invalid).is_err());
}

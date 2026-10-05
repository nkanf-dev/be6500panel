use be6500_panel::policy::{RuleKind, Target};
use be6500_panel::subscription::{MAX_SUBSCRIPTION_BYTES, parse_clash_yaml, summarize_policy};
use sha2::{Digest, Sha256};

fn node(name: &str) -> String {
    format!(
        r#"  - name: '{name}'
    type: vless
    server: 192.0.2.1
    port: 443
    uuid: 11111111-1111-4111-8111-111111111111
    tls: true
    udp: true
    network: tcp
    servername: example.com
    flow: xtls-rprx-vision
    client-fingerprint: chrome
    reality-opts:
      public-key: AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA
      short-id: '0123456789abcdef'
"#
    )
}
fn yaml(tail: &str) -> String {
    format!("proxies:\n{}{tail}", node("fake"))
}
fn parse(tail: &str) -> be6500_panel::subscription::Subscription {
    parse_clash_yaml(yaml(tail).as_bytes()).unwrap()
}

#[test]
fn supported_private_node_public_contract_and_stable_id() {
    let sub = parse("dns: {enhanced-mode: fake-ip}\nrules: ['MATCH,PROXY']\n");
    let hash = Sha256::digest(b"192.0.2.1:443\x0011111111-1111-4111-8111-111111111111\x00fake");
    assert_eq!(sub.nodes[0].id, format!("{:x}", hash)[..16]);
    assert!(sub.fake_ip);
    let public = serde_json::to_value(sub.public_nodes()).unwrap();
    assert_eq!(
        public[0],
        serde_json::json!({"id":sub.nodes[0].id,"label":"fake","server":"192.0.2.1","port":443,"protocol":"vless","transport":"tcp","reality":true,"vision":true,"utls":true,"udp":true})
    );
    assert!(!format!("{sub:?}").contains("11111111"));
    assert!(!public.to_string().contains("uuid"));
}

#[test]
fn invalid_duplicate_and_unsupported_options_are_indexed_safe_omissions() {
    let bad = node("private-bad").replace("tls: true", "tls: 'true'");
    let opts = format!("{}    ws-opts: {{path: secret-path}}\n", node("private-ws"));
    let doc = format!(
        "proxies:\n{}{}{}{}rules: ['MATCH,PROXY']\n",
        node("fake"),
        bad,
        node("fake"),
        opts
    );
    let sub = parse_clash_yaml(doc.as_bytes()).unwrap();
    assert_eq!(sub.nodes.len(), 1);
    assert_eq!(
        sub.diagnostics
            .iter()
            .map(|d| (d.index, d.code.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (1, "unsupported-node"),
            (2, "duplicate-node"),
            (3, "unsupported-node")
        ]
    );
    let public = serde_json::to_string(&sub.diagnostics).unwrap();
    assert!(!public.contains("private-bad"));
    assert!(!public.contains("secret-path"));
}

#[test]
fn boolean_types_original_spellings_and_tags_are_not_coerced() {
    for boolean in ["'true'", "True", "TRUE", "yes", "!!str true"] {
        let mut doc = format!(
            "proxies:\n{}{}",
            node("valid"),
            node("bad").replace("tls: true", &format!("tls: {boolean}"))
        );
        doc.push_str("rules: []\n");
        let sub = parse_clash_yaml(doc.as_bytes()).unwrap();
        assert_eq!(sub.nodes.len(), 1, "{boolean}");
        assert_eq!(sub.diagnostics[0].message, "TLS is required");
    }
    let doc = yaml("")
        .replace("tls: true", "tls: !!bool true")
        .replace("port: 443", "port: 00443");
    assert_eq!(parse_clash_yaml(doc.as_bytes()).unwrap().nodes[0].port, 443);
    for port in ["0x1bb", "+443", "443.0", "44_3"] {
        let doc = format!(
            "proxies:\n{}{}",
            node("valid"),
            node("bad").replace("port: 443", &format!("port: {port}"))
        );
        assert_eq!(
            parse_clash_yaml(doc.as_bytes()).unwrap().diagnostics[0].message,
            "invalid server port"
        );
    }
}

#[test]
fn uniform_nested_groups_preserve_intent_and_mixed_cycles_unknowns_do_not_guess() {
    let sub = parse(
        r#"proxy-groups:
  - {name: direct-leaf, type: select, proxies: [DIRECT]}
  - {name: direct-nested, type: select, proxies: [direct-leaf, DIRECT]}
  - {name: reject-leaf, type: fallback, proxies: [REJECT, REJECT-DROP]}
  - {name: reject-nested, type: select, proxies: [reject-leaf]}
  - {name: mixed, type: select, proxies: [DIRECT, fake]}
  - {name: cycle, type: select, proxies: [cycle]}
  - {name: unknown, type: made-up, proxies: [orphan]}
rules:
  - DOMAIN,a.example,direct-nested
  - DOMAIN,b.example,reject-nested
  - DOMAIN,c.example,mixed
  - DOMAIN,d.example,cycle
  - DOMAIN,e.example,unknown
  - DOMAIN,f.example,orphan
"#,
    );
    assert_eq!(sub.group_count, 7);
    assert_eq!(
        sub.rules
            .iter()
            .map(|r| r.target.clone())
            .collect::<Vec<_>>(),
        vec![
            Target::Direct,
            Target::Block,
            Target::Proxy,
            Target::Proxy,
            Target::Proxy
        ]
    );
    assert_eq!(
        sub.diagnostics
            .iter()
            .map(|d| (d.scope.as_str(), d.index, d.code.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("group", 6, "unsupported-group"),
            ("subscription", -1, "selected-node-policy"),
            ("rule", 5, "unknown-rule-target")
        ]
    );
}

#[test]
fn all_rule_kinds_options_source_order_and_terminal_policy_summary() {
    let sub = parse(
        r#"rules:
  - DOMAIN,example.com,DIRECT
  - DOMAIN-SUFFIX,example.org,PROXY
  - DOMAIN-KEYWORD,hello<世界>&,REJECT
  - IP-CIDR,192.0.2.0/24,DIRECT,no-resolve
  - IP-CIDR6,2001:db8::/32,PROXY
  - GEOIP,CN,DIRECT,no-resolve
  - GEOSITE,CN,PROXY
  - PROCESS-NAME,private.exe,PROXY
  - GEOSITE,US,PROXY
  - DOMAIN,bad_domain,DIRECT
  - FINAL,REJECT-DROP
  - DOMAIN,after.example,PROXY
"#,
    );
    assert_eq!(sub.rules.len(), 9);
    assert_eq!(sub.rules[3].kind, RuleKind::IpCidr);
    assert!(sub.rules[3].no_resolve);
    assert_eq!(sub.rules[7].index, 10);
    assert_eq!(sub.rules[7].kind, RuleKind::Match);
    let summary = summarize_policy(&sub);
    assert_eq!(
        (summary.total, summary.supported, summary.omitted),
        (12, 8, 4)
    );
    assert_eq!(
        summary
            .omitted_rules
            .iter()
            .map(|r| (r.index, r.code.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (7, "unsupported-process-rule"),
            (8, "unsupported-rule"),
            (9, "invalid-rule"),
            (11, "unreachable-rule")
        ]
    );
    assert_eq!(summary.reasons[0].code, "invalid-rule");
    assert!(
        !serde_json::to_string(&summary)
            .unwrap()
            .contains("private.exe")
    );
    let changed_node = yaml("").replace("192.0.2.1", "192.0.2.2");
    assert_eq!(
        summarize_policy(&parse_clash_yaml(changed_node.as_bytes()).unwrap()).revision,
        summarize_policy(&parse("")).revision
    );
    assert_ne!(
        summarize_policy(&parse("dns: {enhanced-mode: fake-ip}\n")).revision,
        summarize_policy(&parse("")).revision
    );
}

#[test]
fn ignored_fields_still_validate_unique_string_keys_and_syntax() {
    for tail in [
        "ignored: {x: 1, x: 2}\n",
        "ignored: {1: a}\n",
        "ignored: {true: a}\n",
        "ignored: {<<: {a: b}}\n",
        "ignored: [a, b\n",
    ] {
        let err = parse_clash_yaml(yaml(tail).as_bytes()).unwrap_err();
        assert!(!format!("{err:?} {err}").contains(tail));
    }
}

#[test]
fn native_alias_anchor_multidoc_depth_scalar_and_source_limits() {
    for tail in [
        "ignored: &x [a,b]\n",
        "ignored: &x a\nother: *x\n",
        "---\nother: b\n",
    ] {
        assert!(parse_clash_yaml(yaml(tail).as_bytes()).is_err());
    }
    let depth = format!("ignored: {}0{}\n", "[".repeat(33), "]".repeat(33));
    assert!(parse_clash_yaml(yaml(&depth).as_bytes()).is_err());
    let scalar = format!("ignored: '{}'\n", "x".repeat(8193));
    assert!(parse_clash_yaml(yaml(&scalar).as_bytes()).is_err());
    let scalar_limit = format!("ignored: '{}'\n", "x".repeat(8192));
    assert!(parse_clash_yaml(yaml(&scalar_limit).as_bytes()).is_ok());
    assert!(parse_clash_yaml(&vec![b' '; MAX_SUBSCRIPTION_BYTES + 1]).is_err());
    let nodes = format!("proxies:\n{}", node("fake").repeat(2049));
    assert!(parse_clash_yaml(nodes.as_bytes()).is_err());
    let rules = format!("rules:\n{}", "  - MATCH,PROXY\n".repeat(8193));
    assert!(parse_clash_yaml(yaml(&rules).as_bytes()).is_err());
    let groups = format!(
        "proxy-groups:\n{}",
        "  - {name: group, type: select, proxies: [DIRECT]}\n".repeat(129)
    );
    assert!(parse_clash_yaml(yaml(&groups).as_bytes()).is_err());
    let many = format!("ignored: [{}]\n", "0,".repeat(100001));
    assert!(parse_clash_yaml(yaml(&many).as_bytes()).is_err());
}

// Fake-only exact Go fixture comparison. The root also installs a permanent
// checked-in fixture test. This worker accepts an explicit read-only fixture
// path so it never copies or owns the golden worker's fixture source.

#[test]
fn duplicate_group_names_append_and_selector_dag_resolution_is_bounded() {
    let sub = parse(
        "proxy-groups:\n  - {name: duplicate, type: select, proxies: [DIRECT]}\n  - {name: duplicate, type: select, proxies: [REJECT]}\nrules: ['MATCH,duplicate']\n",
    );
    assert_eq!(sub.rules[0].target, Target::Proxy);
    let mut tail = "proxy-groups:\n  - {name: g0, type: select, proxies: [DIRECT]}\n".to_string();
    for i in 1..128 {
        tail.push_str(&format!(
            "  - {{name: g{i}, type: select, proxies: [g{}, g{}]}}\n",
            i - 1,
            i - 1
        ));
    }
    tail.push_str("rules: ['MATCH,g127']\n");
    assert_eq!(parse(&tail).rules[0].target, Target::Direct);
}

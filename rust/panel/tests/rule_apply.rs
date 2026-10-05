use be6500_panel::native::{CompileInput, compile_native};
use be6500_panel::policy::{Rule, RuleKind, Target};
use be6500_panel::rule_apply::compile_preserving;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
fn accepted() -> (Vec<u8>, CompileInput) {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/native-go.json")).unwrap();
    let mut input: CompileInput =
        serde_json::from_value(fixture["cases"][2]["input"].clone()).unwrap();
    input.rules = vec![Rule {
        kind: RuleKind::Match,
        target: Target::Proxy,
        ..Rule::default()
    }];
    let output = compile_native(&input).unwrap();
    let mut doc: Value = serde_json::from_slice(&output.config).unwrap();
    doc["log"]["level"] = "debug".into();
    doc["experimental"] = json!({"clash_api":{"external_controller":"127.0.0.1:9090","secret":"synthetic-private-secret"},"cache_file":{"enabled":true}});
    doc["dns"]["strategy"] = "ipv4_only".into();
    doc["route"]["auto_detect_interface"] = false.into();
    doc["inbounds"][0]["tcp_fast_open"] = false.into();
    doc["outbounds"][0]["bind_interface"] = "wan0".into();
    (serde_json::to_vec(&doc).unwrap(), input)
}
#[test]
fn rule_only_compile_preserves_non_policy_native_settings_exactly() {
    let (raw, input) = accepted();
    let rules = vec![
        Rule {
            kind: RuleKind::Domain,
            value: "gpt.kanglives.top".into(),
            target: Target::Direct,
            index: 0,
            ..Rule::default()
        },
        Rule {
            kind: RuleKind::Match,
            target: Target::Proxy,
            index: 1,
            ..Rule::default()
        },
    ];
    let output = compile_preserving(
        &raw,
        std::slice::from_ref(&input.node),
        rules,
        vec![],
        input.rule_sets.clone(),
        true,
    )
    .unwrap();
    let old: Value = serde_json::from_slice(&raw).unwrap();
    let next: Value = serde_json::from_slice(&output.config).unwrap();
    for key in ["inbounds", "outbounds", "log", "experimental"] {
        assert_eq!(old[key], next[key], "{key}");
    }
    for section in ["dns", "route"] {
        for (key, value) in old[section].as_object().unwrap() {
            if key != "rules" && !(section == "route" && key == "rule_set") {
                assert_eq!(next[section][key], *value, "{section}/{key}");
            }
        }
    }
    assert!(
        next["route"]["rules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|rule| rule["domain"] == json!(["gpt.kanglives.top"])
                && rule["outbound"] == "direct")
    );
    assert_eq!(
        output.sha256,
        format!("{:x}", Sha256::digest(&output.config))
    );
    assert!(!format!("{output:?}").contains("synthetic-private-secret"));
    assert_eq!(raw, serde_json::to_vec(&old).unwrap());
}
#[test]
fn private_identity_mismatch_or_ambiguous_node_is_refused() {
    let (raw, input) = accepted();
    let mut changed = input.node.clone();
    changed.uuid = "11111111-1111-4111-8111-111111111111".into();
    assert!(
        compile_preserving(
            &raw,
            &[changed],
            vec![],
            vec![],
            input.rule_sets.clone(),
            true
        )
        .is_err()
    );
    assert!(
        compile_preserving(
            &raw,
            &[input.node.clone(), input.node.clone()],
            vec![],
            vec![],
            input.rule_sets.clone(),
            true
        )
        .is_err()
    );
    let mut doc: Value = serde_json::from_slice(&raw).unwrap();
    doc["inbounds"][0]["listen_port"] = 0.into();
    assert!(
        compile_preserving(
            &serde_json::to_vec(&doc).unwrap(),
            &[input.node],
            vec![],
            vec![],
            input.rule_sets,
            true
        )
        .is_err()
    );
}
#[test]
fn accepted_output_budget_and_duplicate_settings_fail_closed() {
    let (raw, input) = accepted();
    let mut duplicate = String::from_utf8(raw.clone()).unwrap();
    duplicate = duplicate.replacen("\"log\":{", "\"log\":{},\"log\":{", 1);
    assert!(
        compile_preserving(
            duplicate.as_bytes(),
            std::slice::from_ref(&input.node),
            vec![],
            vec![],
            input.rule_sets.clone(),
            true
        )
        .is_err()
    );
    let mut doc: Value = serde_json::from_slice(&raw).unwrap();
    doc["experimental"] = json!({"padding":"x".repeat(512<<10)});
    assert!(
        compile_preserving(
            &serde_json::to_vec(&doc).unwrap(),
            &[input.node],
            vec![],
            vec![],
            input.rule_sets,
            true
        )
        .is_err()
    );
}


fn new_node(input: &CompileInput) -> be6500_panel::native::Node {
    let mut node = input.node.clone();
    node.server = "new-node.example".into();
    node.port = 8443;
    node.uuid = "11111111-1111-4111-8111-111111111111".into();
    node.server_name = "new-tls.example".into();
    node.reality_short_id = "abcd".into();
    node
}
#[test]
fn explicit_new_node_not_in_old_source_preserves_private_native_settings() {
    use be6500_panel::rule_apply::{compile_selection, selection_settings};
    let (raw, old_input) = accepted();
    let mut doc: Value = serde_json::from_slice(&raw).unwrap();
    doc["private_extension"] = json!({"telemetry":false,"secret":"synthetic-private-selection"});
    doc["dns"]["cache_capacity"] = json!(256);
    doc["dns"]["timeout"] = json!("11s");
    doc["dns"]["servers"][0]["tls"]["min_version"] = json!("1.3");
    doc["dns"]["servers"][2]["server"] = json!("192.168.31.1");
    doc["dns"]["servers"].as_array_mut().unwrap().push(json!({"type":"udp","tag":"custom-dns","server":"192.168.31.2","server_port":5353,"detour":"direct","private_option":false}));
    doc["route"]["private_router_extension"] = json!({"enabled":true});
    let proxy = doc["outbounds"].as_array_mut().unwrap().iter_mut().find(|v|v["tag"]=="proxy").unwrap();
    proxy["bind_interface"] = json!("wan9");
    proxy["connect_timeout"] = json!("19s");
    proxy["tls"]["min_version"] = json!("1.3");
    proxy["tls"]["utls"]["private_option"] = json!(false);
    proxy["tls"]["reality"]["private_option"] = json!("keep");
    doc["outbounds"].as_array_mut().unwrap().push(json!({"type":"direct","tag":"other-direct","bind_interface":"wan-other","private_option":true}));
    doc["inbounds"].as_array_mut().unwrap().push(json!({"type":"direct","tag":"other-listener","listen":"127.0.0.1","listen_port":3333,"private_option":"keep"}));
    let raw = serde_json::to_vec(&doc).unwrap();
    let selected = new_node(&old_input);
    let mut input = selection_settings(&raw,&selected).unwrap();
    assert_eq!(input.node.uuid,selected.uuid);
    assert_eq!(input.mixed_listen_address,doc["inbounds"].as_array().unwrap().iter().find(|v|v["tag"]=="mixed-in").unwrap()["listen"].as_str().unwrap());
    input.rule_sets = old_input.rule_sets;
    input.rules = old_input.rules;
    input.accept_unsupported_rules = true;
    let output = compile_selection(Some(&raw),input).unwrap();
    let next: Value = serde_json::from_slice(&output.config).unwrap();
    for key in ["log","experimental","private_extension"] { assert_eq!(next[key],doc[key]); }
    assert_eq!(next["dns"]["servers"],doc["dns"]["servers"]);
    for (key,value) in doc["dns"].as_object().unwrap() { if key!="rules" {assert_eq!(next["dns"][key],*value);} }
    for (key,value) in doc["route"].as_object().unwrap() { if key!="rules"&&key!="rule_set" {assert_eq!(next["route"][key],*value);} }
    let old_proxy=doc["outbounds"].as_array().unwrap().iter().find(|v|v["tag"]=="proxy").unwrap();
    let proxy=next["outbounds"].as_array().unwrap().iter().find(|v|v["tag"]=="proxy").unwrap();
    assert_eq!(proxy["uuid"],selected.uuid); assert_eq!(proxy["server"],selected.server);
    assert_eq!(proxy["tls"]["server_name"],selected.server_name);
    assert_eq!(proxy["tls"]["reality"]["short_id"],selected.reality_short_id);
    for key in ["bind_interface","connect_timeout","domain_resolver","tcp_fast_open","packet_encoding"] {assert_eq!(proxy[key],old_proxy[key]);}
    assert_eq!(proxy["tls"]["min_version"],old_proxy["tls"]["min_version"]);
    assert_eq!(proxy["tls"]["utls"]["private_option"],json!(false));
    assert_eq!(proxy["tls"]["reality"]["private_option"],json!("keep"));
    for old in doc["outbounds"].as_array().unwrap().iter().filter(|v|v["tag"]!="proxy") {
        assert_eq!(next["outbounds"].as_array().unwrap().iter().find(|v|v["tag"]==old["tag"]).unwrap(),old);
    }
    assert_eq!(next["inbounds"],doc["inbounds"]);
    assert_eq!(output.sha256,format!("{:x}",Sha256::digest(&output.config)));
    assert!(!format!("{output:?}").contains("synthetic-private-selection"));
}
#[test]
fn selection_explicit_ports_listen_tun_and_fresh_endpoint_scope_replace_only_controls() {
    use be6500_panel::rule_apply::{compile_selection, selection_settings};
    let (raw,old)=accepted(); let mut input=selection_settings(&raw,&new_node(&old)).unwrap();
    input.ports.mixed=28080; input.ports.dns=21053;
    input.mixed_listen_address="127.0.0.1".into(); input.dns_listen_address="192.168.31.1".into();
    input.management_ips=vec!["192.168.31.1".into()];
    input.endpoints=vec!["203.0.113.9".into()]; input.bootstrap_domains=vec!["new-bootstrap.example".into()];
    input.routed_tun.as_mut().unwrap().interface_name="b6p-next".into();
    input.routed_tun.as_mut().unwrap().address="172.31.254.253/30".into();
    input.rule_sets=old.rule_sets; input.rules=old.rules; input.accept_unsupported_rules=true;
    let output=compile_selection(Some(&raw),input).unwrap(); let next:Value=serde_json::from_slice(&output.config).unwrap();
    let mixed=next["inbounds"].as_array().unwrap().iter().find(|v|v["tag"]=="mixed-in").unwrap();
    assert_eq!(mixed["listen_port"],json!(28080)); assert_eq!(mixed["listen"],json!("127.0.0.1"));
    assert_eq!(mixed["tcp_fast_open"],json!(false));
    let tun=next["inbounds"].as_array().unwrap().iter().find(|v|v["tag"]=="tun-in").unwrap();
    assert_eq!(tun["interface_name"],json!("b6p-next")); assert_eq!(tun["address"],json!(["172.31.254.253/30"]));
    assert!(next["route"]["rules"].as_array().unwrap().iter().any(|r|r["domain"]==json!(["new-bootstrap.example"])));
    assert!(next["route"]["rules"].as_array().unwrap().iter().any(|r|r["ip_cidr"].as_array().is_some_and(|ips|ips.contains(&json!("203.0.113.9/32")))));
}
#[test]
fn first_selection_none_is_exact_native_compile() {
    use be6500_panel::rule_apply::compile_selection;
    let (_,mut input)=accepted(); input.node=new_node(&input);
    let expected=compile_native(&input).unwrap(); let actual=compile_selection(None,input).unwrap();
    assert_eq!(actual.config,expected.config); assert_eq!(actual.sha256,expected.sha256);
}
#[test]
fn selection_rejects_unsupported_transport_ambiguous_shapes_and_limits() {
    use be6500_panel::rule_apply::{compile_selection, selection_settings};
    let (raw,input)=accepted(); let selected=new_node(&input);
    for key in ["transport","multiplex","network"] {
        let mut doc:Value=serde_json::from_slice(&raw).unwrap();
        let proxy=doc["outbounds"].as_array_mut().unwrap().iter_mut().find(|v|v["tag"]=="proxy").unwrap();
        proxy[key]=json!({"type":"unsupported"});
        let bad=serde_json::to_vec(&doc).unwrap(); assert!(selection_settings(&bad,&selected).is_err());
        assert!(compile_selection(Some(&bad),input.clone()).is_err());
    }
    for change in 0..5 {
        let mut doc:Value=serde_json::from_slice(&raw).unwrap();
        match change {
            0=>{let p=doc["outbounds"].as_array_mut().unwrap().iter_mut().find(|v|v["tag"]=="proxy").unwrap();p["tls"]["enabled"]=json!(false);}
            1=>{let p=doc["outbounds"].as_array_mut().unwrap().iter_mut().find(|v|v["tag"]=="proxy").unwrap();p["packet_encoding"]=json!("other");}
            2=>{doc["inbounds"].as_array_mut().unwrap().pop();}
            3=>{let p=doc["outbounds"].as_array().unwrap().iter().find(|v|v["tag"]=="proxy").unwrap().clone();doc["outbounds"].as_array_mut().unwrap().push(p);}
            _=>{doc["experimental"]=json!({"padding":"x".repeat(512<<10)});}
        }
        let bad=serde_json::to_vec(&doc).unwrap(); assert!(compile_selection(Some(&bad),input.clone()).is_err());
    }
    let duplicate=String::from_utf8(raw).unwrap().replacen("\"log\":{","\"log\":{},\"log\":{",1);
    assert!(selection_settings(duplicate.as_bytes(),&selected).is_err());
}
#[test]
fn selection_nested_duplicate_controlled_fields_are_refused() {
    use be6500_panel::rule_apply::selection_settings;
    let (raw,input)=accepted(); let text=String::from_utf8(raw).unwrap();
    for (from,to) in [("\"uuid\":","\"uuid\":\"private\",\"uuid\":"),
        ("\"utls\":{","\"utls\":{},\"utls\":{"),
        ("\"fingerprint\":","\"fingerprint\":\"private\",\"fingerprint\":"),
        ("\"short_id\":","\"short_id\":\"private\",\"short_id\":")] {
        let bad=text.replacen(from,to,1); assert_ne!(bad,text); assert!(selection_settings(bad.as_bytes(),&input.node).is_err());
    }
}

#[test]
fn selected_node_id_requires_one_exact_credential_match_not_label_or_old_fallback() {
    use be6500_panel::rule_apply::selected_node_id;
    let (raw,input)=accepted();
    assert_eq!(selected_node_id(&raw,std::slice::from_ref(&input.node)).unwrap(),input.node.id);
    let different=new_node(&input);
    assert!(selected_node_id(&raw,&[different]).is_err());
    let mut duplicate=input.node.clone();duplicate.id="same-credential-different-id".into();
    assert!(selected_node_id(&raw,&[input.node.clone(),duplicate]).is_err());
    assert!(selected_node_id(&raw,&[]).is_err());
}

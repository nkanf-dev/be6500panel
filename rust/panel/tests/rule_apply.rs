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

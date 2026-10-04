use be6500_panel::native::Node;
use be6500_panel::policy::{Diagnostic, Rule};
use be6500_panel::subscription::{parse_clash_yaml, summarize_policy};
use serde::Deserialize;
#[derive(Deserialize)]
struct Fixtures {
    version: u32,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    name: String,
    yaml: String,
    valid: bool,
    subscription: Option<Expected>,
    policy_summary: Option<serde_json::Value>,
    error: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Expected {
    nodes: Option<Vec<Node>>,
    public_nodes: serde_json::Value,
    rules: Option<Vec<Rule>>,
    diagnostics: Option<Vec<Diagnostic>>,
    group_count: usize,
    #[serde(rename = "fakeIP")]
    fake_ip: bool,
}
fn identity(
    node: &Node,
) -> (
    &str,
    &str,
    &str,
    u16,
    &str,
    &str,
    &str,
    &str,
    &str,
    &str,
    bool,
) {
    (
        &node.id,
        &node.name,
        &node.server,
        node.port,
        &node.uuid,
        &node.server_name,
        &node.reality_public_key,
        &node.reality_short_id,
        &node.fingerprint,
        &node.flow,
        node.udp,
    )
}
#[test]
fn exact_go_reference_subscription_and_policy_summary() {
    let fixtures: Fixtures = serde_json::from_str(include_str!("fixtures/subscription-go.json"))
        .expect("fake Go reference");
    assert_eq!(fixtures.version, 1);
    assert!(!fixtures.cases.is_empty());
    for case in fixtures.cases {
        let parsed = parse_clash_yaml(case.yaml.as_bytes());
        assert_eq!(parsed.is_ok(), case.valid, "validity {}", case.name);
        if let Ok(actual) = parsed {
            let expected = case.subscription.expect("Go parsed output");
            let expected_nodes = expected.nodes.unwrap_or_default();
            assert_eq!(
                actual.nodes.iter().map(identity).collect::<Vec<_>>(),
                expected_nodes.iter().map(identity).collect::<Vec<_>>(),
                "private synthetic node {}",
                case.name
            );
            assert_eq!(
                serde_json::to_value(actual.public_nodes()).unwrap(),
                expected.public_nodes,
                "public nodes {}",
                case.name
            );
            assert_eq!(
                actual.rules,
                expected.rules.unwrap_or_default(),
                "rules {}",
                case.name
            );
            assert_eq!(
                actual.diagnostics,
                expected.diagnostics.unwrap_or_default(),
                "diagnostics {}",
                case.name
            );
            assert_eq!(
                actual.group_count, expected.group_count,
                "groups {}",
                case.name
            );
            assert_eq!(actual.fake_ip, expected.fake_ip, "fake-IP {}", case.name);
            assert_eq!(
                serde_json::to_value(summarize_policy(&actual)).unwrap(),
                case.policy_summary.expect("Go policy summary"),
                "summary/hash {}",
                case.name
            );
        } else {
            let error = parsed.err().unwrap().to_string();
            assert!(!error.is_empty());
            assert!(!error.contains("00000000-0000-4000-8000-000000000001"));
            assert!(case.error.is_some(), "Go refusal {}", case.name);
        }
    }
}

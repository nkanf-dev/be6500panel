use be6500_panel::capture_plan::{RulesPlanInput, plan_owned_rules};
use serde::Deserialize;
#[derive(Deserialize)]
struct Fixtures {
    version: u32,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    input: RulesPlanInput,
    valid: bool,
    plan: Option<serde_json::Value>,
    error: Option<String>,
}
#[test]
fn native_regression_routed_tun_owned_plan() {
    let fixture: Fixtures = serde_json::from_str(include_str!("fixtures/capture.json"))
        .expect("synthetic capture reference");
    assert_eq!(fixture.version, 1);
    assert_eq!(fixture.cases.len(), 30);
    for case in fixture.cases {
        let result = plan_owned_rules(&case.input);
        assert_eq!(result.is_ok(), case.valid, "validity {}", case.name);
        match result {
            Ok(plan) => {
                let actual = serde_json::to_value(plan).expect("owned intent serialization");
                assert_eq!(
                    actual,
                    case.plan.expect("Go plan"),
                    "exact argv ownership warnings {}",
                    case.name
                );
            }
            Err(error) => {
                assert!(case.error.is_some(), "Go error {}", case.name);
                assert!(!error.to_string().is_empty(), "fixed refusal {}", case.name);
            }
        }
    }
}

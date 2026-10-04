use be6500_panel::policy::{
    EffectivePolicy, Policy, Rule, merge_effective_policy, policy_revision,
    subscription_fingerprints, validate_policy,
};
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
    subscription: Vec<Rule>,
    policy: Policy,
    valid: bool,
    revision: Option<String>,
    fingerprints: Vec<String>,
    effective: EffectivePolicy,
    validation_error: Option<String>,
}

#[test]
fn exact_go_reference_policy_semantics() {
    let fixtures: Fixtures = serde_json::from_str(include_str!("fixtures/local-policy-go.json"))
        .expect("typed Go fixture");
    assert_eq!(fixtures.version, 1);
    assert_eq!(fixtures.cases.len(), 23);
    for case in fixtures.cases {
        let validation = validate_policy(&case.policy);
        assert_eq!(validation.is_ok(), case.valid, "validation {}", case.name);
        let fingerprints =
            subscription_fingerprints(&case.subscription).expect("all fixture subscriptions valid");
        assert_eq!(
            fingerprints, case.fingerprints,
            "fingerprints {}",
            case.name
        );
        if case.valid {
            assert_eq!(
                policy_revision(&case.policy).expect("valid revision"),
                case.revision.expect("Go revision"),
                "revision {}",
                case.name
            );
            let effective =
                merge_effective_policy(&case.subscription, &case.policy).expect("valid merge");
            assert_eq!(effective, case.effective, "effective {}", case.name);
        } else {
            assert_eq!(
                validation.expect_err("invalid policy").to_string(),
                case.validation_error.expect("Go fixed token"),
                "safe error {}",
                case.name
            );
            assert!(
                policy_revision(&case.policy).is_err(),
                "invalid revision {}",
                case.name
            );
            let failure = merge_effective_policy(&case.subscription, &case.policy)
                .expect_err("invalid merge");
            assert_eq!(
                failure.diagnostics, case.effective.diagnostics,
                "diagnostics {}",
                case.name
            );
            assert!(
                case.effective.rules.is_empty() && case.effective.provenance.is_empty(),
                "Go invalid merge output {}",
                case.name
            );
        }
    }
}

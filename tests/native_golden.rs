use be6500_panel::native::{CompileInput, compile_native};
use be6500_panel::policy::Diagnostic;
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
    input: CompileInput,
    valid: bool,
    config: Option<String>,
    sha256: Option<String>,
    core_version: Option<String>,
    diagnostics: Option<Vec<Diagnostic>>,
    endpoint_hosts: Option<Vec<String>>,
    required_features: Option<Vec<String>>,
    ipv6: Option<String>,
    failure: Option<String>,
    error: Option<String>,
}
#[test]
fn native_regression_native_configuration() {
    let fixtures: Fixtures = serde_json::from_str(include_str!("fixtures/native.json"))
        .expect("typed synthetic Go input");
    assert_eq!(fixtures.version, 1);
    assert_eq!(fixtures.cases.len(), 30);
    for case in fixtures.cases {
        let compiled = compile_native(&case.input);
        assert_eq!(compiled.is_ok(), case.valid, "validity {}", case.name);
        if case.valid {
            let out = compiled.expect("valid reference");
            assert_eq!(
                out.config,
                case.config.expect("Go config").into_bytes(),
                "exact emitted bytes {}",
                case.name
            );
            assert_eq!(
                out.sha256,
                case.sha256.expect("Go hash"),
                "hash {}",
                case.name
            );
            assert_eq!(
                out.core_version,
                case.core_version.expect("core version"),
                "core version {}",
                case.name
            );
            assert_eq!(
                out.diagnostics,
                case.diagnostics.unwrap_or_default(),
                "diagnostics {}",
                case.name
            );
            assert_eq!(
                out.endpoint_hosts,
                case.endpoint_hosts.unwrap_or_default(),
                "endpoints {}",
                case.name
            );
            assert_eq!(
                out.required_features,
                case.required_features.unwrap_or_default(),
                "features {}",
                case.name
            );
            assert_eq!(out.ipv6, case.ipv6.expect("ipv6"), "ipv6 {}", case.name);
            assert_eq!(
                out.failure,
                case.failure.expect("failure"),
                "failure {}",
                case.name
            );
        } else {
            let error = compiled.expect_err("invalid reference");
            // Exact safe strings are expected except indexed dynamic refusals,
            // which must still reject and never echo a private input value.
            let expected = case.error.expect("Go safe error");
            let actual = error.to_string();
            assert!(!actual.is_empty(), "empty refusal {}", case.name);
            if !expected.starts_with("unsupported subscription rule at index ") {
                assert_eq!(actual, expected, "safe refusal {}", case.name);
            } else {
                assert!(
                    actual.contains("unsupported") && actual.contains("acknowledg"),
                    "omission refusal {}",
                    case.name
                );
            }
        }
    }
}

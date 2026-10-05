use be6500_panel::policy::{
    LocalRule, Policy, Rule, RuleKind, SubscriptionEdit, Target, merge_effective_policy,
    policy_revision, subscription_fingerprints,
};
use std::{env, fs, time::Instant};
fn memory() -> (Option<u64>, Option<u64>) {
    let text = fs::read_to_string("/proc/self/status").unwrap_or_default();
    let value = |name: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(name))
            .and_then(|tail| tail.split_whitespace().next())
            .and_then(|value| value.parse::<u64>().ok())
    };
    (value("VmRSS:"), value("VmHWM:"))
}
fn run() -> Result<(), String> {
    let rounds = env::args()
        .nth(1)
        .unwrap_or_else(|| "3".into())
        .parse::<usize>()
        .map_err(|_| "invalid rounds")?;
    if !(1..=10).contains(&rounds) {
        return Err("round limit".into());
    }
    let mut subscription = Vec::with_capacity(8192);
    for index in 0..8192 {
        subscription.push(Rule {
            kind: RuleKind::DomainSuffix,
            value: format!("site-{index}.example.test"),
            target: Target::Proxy,
            no_resolve: false,
            index: index as i64,
        });
    }
    let mut policy = Policy::default();
    for index in 0..512 {
        policy.rules.push(LocalRule {
            id: format!("local-{index}"),
            enabled: true,
            label: format!("local direct {index}"),
            note: "synthetic bounded policy memory test".into(),
            rule: Rule {
                kind: RuleKind::Domain,
                value: format!("direct-{index}.example.test"),
                target: Target::Direct,
                no_resolve: false,
                index: index as i64,
            },
        });
    }
    let fingerprints =
        subscription_fingerprints(&subscription).map_err(|_| "fingerprints refused")?;
    for (index, fingerprint) in fingerprints.iter().enumerate().take(1024) {
        policy.subscription_edits.push(SubscriptionEdit {
            id: format!("edit-{index}"),
            source_fingerprint: fingerprint.clone(),
            disabled: true,
            replacement: None,
            label: String::new(),
            note: String::new(),
        });
    }
    let encoded = serde_json::to_vec(&policy).map_err(|_| "encode failed")?;
    if encoded.len() > 256 * 1024 {
        return Err("synthetic policy exceeds store cap".into());
    }
    let before = memory();
    let start = Instant::now();
    let mut effective_count = 0;
    let mut diagnostic_count = 0;
    let revision = policy_revision(&policy).map_err(|_| "revision refused")?;
    for _ in 0..rounds {
        let output = merge_effective_policy(&subscription, &policy).map_err(|_| "merge refused")?;
        effective_count = output.rules.len();
        diagnostic_count = output.diagnostics.len();
        if effective_count != 512 + 8192 - 1024 || diagnostic_count != 1024 {
            return Err("unexpected output counts".into());
        }
        std::hint::black_box(output);
    }
    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
    let after = memory();
    println!(
        "{}",
        serde_json::json!({"source":"synthetic bounded policy operation","subscriptionRules":subscription.len(),"localRules":policy.rules.len(),"subscriptionEdits":policy.subscription_edits.len(),"documentBytes":encoded.len(),"rounds":rounds,"effectiveRules":effective_count,"diagnostics":diagnostic_count,"revision":revision,"elapsedMs":elapsed,"beforeRSSKiB":before.0,"afterRSSKiB":after.0,"peakRSSKiB":after.1,"errors":[]})
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("policy benchmark: {error}");
        std::process::exit(1)
    }
}

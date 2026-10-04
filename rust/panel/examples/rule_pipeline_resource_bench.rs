use be6500_panel::native::{CompileInput, MAX_CONFIG_BYTES, Node, compile_native};
use be6500_panel::policy::{
    LocalRule, Policy, Rule, RuleKind, SubscriptionEdit, Target, merge_effective_policy,
    subscription_fingerprints,
};
use be6500_panel::policy_store::Store;
use std::{env, fs, path::PathBuf, time::Instant};
fn memory() -> (Option<u64>, Option<u64>) {
    let text = fs::read_to_string("/proc/self/status").unwrap_or_default();
    let value = |key: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(key))
            .and_then(|tail| tail.split_whitespace().next())
            .and_then(|value| value.parse().ok())
    };
    (value("VmRSS:"), value("VmHWM:"))
}
fn run() -> Result<(), String> {
    let args = env::args().collect::<Vec<_>>();
    if args.len() != 3 {
        return Err("usage: rule-pipeline-bench SUBSCRIPTION-COUNT SCRATCH-DIRECTORY".into());
    }
    let count = args[1].parse::<usize>().map_err(|_| "invalid count")?;
    if !(1024..=8192).contains(&count) {
        return Err("count bound".into());
    }
    let directory = PathBuf::from(&args[2]);
    if directory.exists() {
        return Err("scratch directory must not exist".into());
    }
    let started = Instant::now();
    let before = memory();
    let subscription = (0..count)
        .map(|i| Rule {
            kind: RuleKind::DomainSuffix,
            value: format!("site-{i}.example.test"),
            target: Target::Proxy,
            no_resolve: false,
            index: i as i64,
        })
        .collect::<Vec<_>>();
    let fingerprints =
        subscription_fingerprints(&subscription).map_err(|_| "fingerprints failed")?;
    let mut policy = Policy::default();
    for i in 0..512 {
        policy.rules.push(LocalRule {
            id: format!("local-{i}"),
            enabled: true,
            label: format!("local direct {i}"),
            note: "synthetic bounded policy memory test".into(),
            rule: Rule {
                kind: RuleKind::Domain,
                value: format!("direct-{i}.example.test"),
                target: Target::Direct,
                no_resolve: false,
                index: i as i64,
            },
        });
    }
    for (i, fingerprint) in fingerprints.iter().enumerate().take(1024) {
        policy.subscription_edits.push(SubscriptionEdit {
            id: format!("edit-{i}"),
            source_fingerprint: fingerprint.clone(),
            disabled: true,
            replacement: None,
            label: String::new(),
            note: String::new(),
        });
    }
    drop(fingerprints);
    let document_bytes = serde_json::to_vec(&policy)
        .map_err(|_| "encode failed")?
        .len();
    if document_bytes > 256 * 1024 {
        return Err("draft cap".into());
    }
    let effective = merge_effective_policy(&subscription, &policy).map_err(|_| "merge failed")?;
    let effective_count = effective.rules.len();
    let diagnostics = effective.diagnostics.len();
    let after_merge = memory();
    let mut input = CompileInput {
        node: Node {
            id: "synthetic-node".into(),
            name: "synthetic".into(),
            server: "node.example".into(),
            port: 443,
            uuid: "00000000-0000-4000-8000-000000000001".into(),
            server_name: "certificate.example".into(),
            reality_public_key: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
            reality_short_id: "01020304".into(),
            fingerprint: "chrome".into(),
            flow: "xtls-rprx-vision".into(),
            udp: true,
        },
        ..CompileInput::default()
    };
    input.rules = effective.rules;
    drop(effective.provenance);
    drop(effective.diagnostics);
    let compile_started = Instant::now();
    let compiled = compile_native(&input);
    let compile_ms = compile_started.elapsed().as_secs_f64() * 1000.0;
    let (config_bytes, compile_error) = match compiled {
        Ok(output) => {
            if output.config.len() > MAX_CONFIG_BYTES {
                return Err("compiler cap bypassed".into());
            }
            (Some(output.config.len()), None)
        }
        Err(error) => (None, Some(error.to_string())),
    };
    if count == 2048 && config_bytes.is_none() {
        return Err("expected normal configuration was refused".into());
    }
    if count == 8192
        && compile_error.as_deref() != Some("native configuration output limit exceeded")
    {
        return Err("oversized configuration was not refused at the resource limit".into());
    }
    let after_compile = memory();
    drop(input);
    drop(subscription);
    let mut store = Store::open(&directory).map_err(|_| "store open failed")?;
    let save_started = Instant::now();
    let outcome = store.save(&policy).map_err(|_| "save failed")?;
    if !outcome.committed || outcome.durability_error.is_some() {
        return Err("save not durable".into());
    }
    let revision = outcome.snapshot.revision.clone();
    drop(outcome);
    drop(store);
    let reopened = Store::open(&directory).map_err(|_| "reopen failed")?;
    let readback = reopened.snapshot();
    if readback.revision != revision || readback.policy != policy {
        return Err("store readback mismatch".into());
    }
    let save_ms = save_started.elapsed().as_secs_f64() * 1000.0;
    let after_save = memory();
    drop(readback);
    drop(reopened);
    drop(policy);
    fs::remove_dir_all(&directory).map_err(|_| "scratch cleanup failed")?;
    println!(
        "{}",
        serde_json::json!({"source":"synthetic merge/compiler/private-store pipeline","subscriptionRules":count,"localRules":512,"subscriptionEdits":1024,"draftBytes":document_bytes,"effectiveRules":effective_count,"diagnostics":diagnostics,"configBytes":config_bytes,"compileError":compile_error,"compileMs":compile_ms,"saveReopenMs":save_ms,"totalMs":started.elapsed().as_secs_f64()*1000.0,"initialRSSKiB":before.0,"afterMergeRSSKiB":after_merge.0,"afterCompileRSSKiB":after_compile.0,"afterStoreRSSKiB":after_save.0,"peakRSSKiB":after_save.1,"scratchRemoved":true,"revision":revision,"errors":[]})
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("rule pipeline benchmark: {error}");
        std::process::exit(1)
    }
}

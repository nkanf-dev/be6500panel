use be6500_panel::policy::{
    EffectivePolicy, LocalRule, MAX_LOCAL_RULES, MAX_POLICY_ID_BYTES, MAX_RULE_LABEL_RUNES,
    MAX_RULE_NOTE_RUNES, MAX_RULE_VALUE_BYTES, MAX_RULES, MAX_SUBSCRIPTION_EDITS, Policy, Rule,
    RuleKind, SubscriptionEdit, Target, merge_effective_policy, policy_revision,
    subscription_fingerprints, validate_policy,
};

fn rule(kind: RuleKind, value: &str, target: Target, index: i64) -> Rule {
    Rule {
        kind,
        value: value.into(),
        target,
        no_resolve: false,
        index,
    }
}

fn domain(value: &str, index: i64) -> Rule {
    rule(RuleKind::Domain, value, Target::Proxy, index)
}

fn local(id: &str, enabled: bool, rule: Rule) -> LocalRule {
    LocalRule {
        id: id.into(),
        enabled,
        label: String::new(),
        note: String::new(),
        rule,
    }
}

fn policy_with(rule: Rule) -> Policy {
    Policy {
        rules: vec![local("local-1", true, rule)],
        subscription_edits: vec![],
    }
}

fn fingerprint(occurrence: usize) -> String {
    format!("{}:{occurrence}", "a".repeat(64))
}

fn disable(id: &str, source_fingerprint: String) -> SubscriptionEdit {
    SubscriptionEdit {
        id: id.into(),
        source_fingerprint,
        disabled: true,
        replacement: None,
        label: String::new(),
        note: String::new(),
    }
}

fn codes(out: &EffectivePolicy) -> Vec<&str> {
    out.diagnostics.iter().map(|d| d.code.as_str()).collect()
}

#[test]
fn policy_counts_have_exact_bounds_and_do_not_hide_invalid_drafts() {
    let mut policy = Policy {
        rules: (0..MAX_LOCAL_RULES)
            .map(|i| local(&format!("local-{i}"), false, domain("valid", 0)))
            .collect(),
        subscription_edits: (0..MAX_SUBSCRIPTION_EDITS)
            .map(|i| disable(&format!("edit-{i}"), fingerprint(i + 1)))
            .collect(),
    };
    assert_eq!(validate_policy(&policy), Ok(()));
    policy
        .rules
        .push(local("overflow", false, domain("valid", 0)));
    let error = validate_policy(&policy).unwrap_err();
    assert_eq!(error.to_string(), "invalid local proxy policy");
    assert_eq!(error.diagnostics.len(), 1);
    assert_eq!(error.diagnostics[0].scope, "policy");
    assert_eq!(error.diagnostics[0].index, -1);
    assert_eq!(error.diagnostics[0].code, "policy-limit");
    policy.rules.pop();
    policy
        .subscription_edits
        .push(disable("overflow", fingerprint(MAX_SUBSCRIPTION_EDITS + 1)));
    assert!(validate_policy(&policy).is_err());
    policy.subscription_edits.pop();
    policy.rules[0].rule.value = "not a domain".into();
    assert!(validate_policy(&policy).is_err());
}

#[test]
fn ids_are_bounded_ascii_and_unique_across_layers_without_normalization() {
    let mut policy = policy_with(domain("example.com", 0));
    policy.rules[0].id = "A".repeat(MAX_POLICY_ID_BYTES);
    assert!(validate_policy(&policy).is_ok());
    for invalid in [
        "".into(),
        "A".repeat(MAX_POLICY_ID_BYTES + 1),
        "é".into(),
        " leading".into(),
        "x/y".into(),
        "line\nbreak".into(),
        "x:y".into(),
    ] {
        policy.rules[0].id = invalid;
        let error = validate_policy(&policy).unwrap_err();
        assert_eq!(error.diagnostics[0].code, "invalid-id");
    }
    policy.rules[0].id = "A.z-0_9".into();
    policy
        .subscription_edits
        .push(disable("A.z-0_9", fingerprint(1)));
    assert_eq!(
        validate_policy(&policy).unwrap_err().diagnostics[0].code,
        "invalid-id"
    );
    policy.subscription_edits[0].id = "a.z-0_9".into();
    assert!(validate_policy(&policy).is_ok());
    let snapshot = policy.clone();
    let _ = policy_revision(&policy).unwrap();
    assert_eq!(snapshot, policy);
}

#[test]
fn unicode_metadata_counts_scalars_and_only_notes_allow_lf_and_tab() {
    let mut policy = policy_with(domain("example.com", 0));
    policy.rules[0].label = "🦀".repeat(MAX_RULE_LABEL_RUNES);
    policy.rules[0].note = "中".repeat(MAX_RULE_NOTE_RUNES);
    assert!(validate_policy(&policy).is_ok());
    policy.rules[0].label.push('a');
    assert_eq!(
        validate_policy(&policy).unwrap_err().diagnostics[0].code,
        "invalid-metadata"
    );
    policy.rules[0].label.pop();
    policy.rules[0].note.push('a');
    assert!(validate_policy(&policy).is_err());
    policy.rules[0].note = "line\n\ttab <>&\u{2028}\u{2029}".into();
    policy.rules[0].label = " <>&\u{2028}\u{2029} ".into();
    assert!(validate_policy(&policy).is_ok());
    for control in ['\0', '\r', '\u{7f}', '\u{85}', '\u{9f}'] {
        policy.rules[0].note = control.to_string();
        assert!(validate_policy(&policy).is_err());
    }
    policy.rules[0].note.clear();
    for control in ['\n', '\t', '\r', '\0', '\u{7f}', '\u{85}'] {
        policy.rules[0].label = control.to_string();
        assert!(validate_policy(&policy).is_err());
    }
    policy.rules[0].label = "e\u{301}".repeat(32);
    assert!(validate_policy(&policy).is_ok());
    policy.rules[0].label.push('e');
    assert!(validate_policy(&policy).is_err());
}

#[test]
fn private_domain_and_keyword_validation_is_exact_and_has_no_action_aliases() {
    let valid_max_domain = [
        "a".repeat(63),
        "b".repeat(63),
        "c".repeat(63),
        "d".repeat(61),
    ]
    .join(".");
    assert_eq!(valid_max_domain.len(), MAX_RULE_VALUE_BYTES);
    for value in [
        "localhost",
        "127.0.0.1",
        "EXample.COM",
        "a-b.example",
        &valid_max_domain,
    ] {
        assert!(
            validate_policy(&policy_with(domain(value, 0))).is_ok(),
            "{value}"
        );
    }
    for value in [
        "",
        ".example.com",
        "example.com.",
        "bad..name",
        "-bad",
        "bad-",
        "foo_bar",
        "*.example",
        "é.test",
        "white space",
    ] {
        assert!(
            validate_policy(&policy_with(domain(value, 0))).is_err(),
            "{value}"
        );
    }
    assert!(validate_policy(&policy_with(domain(&"x".repeat(64), 0))).is_err());
    for kind in [RuleKind::Domain, RuleKind::DomainSuffix] {
        assert!(validate_policy(&policy_with(rule(kind, "bad_", Target::Direct, 0))).is_err());
    }
    for value in ["X", "a-b_/?:<>&", &"x".repeat(MAX_RULE_VALUE_BYTES)] {
        assert!(
            validate_policy(&policy_with(rule(
                RuleKind::DomainKeyword,
                value,
                Target::Block,
                0
            )))
            .is_ok()
        );
    }
    for value in ["", "has space", "a\tb", "a\r", "a\n", "a\0", "a\u{7f}", "é"] {
        assert!(
            validate_policy(&policy_with(rule(
                RuleKind::DomainKeyword,
                value,
                Target::Block,
                0
            )))
            .is_err()
        );
    }
    assert!(
        validate_policy(&policy_with(rule(
            RuleKind::DomainKeyword,
            &"x".repeat(MAX_RULE_VALUE_BYTES + 1),
            Target::Block,
            0
        )))
        .is_err()
    );
    for action in ["reject", "REJECT", "DIRECT", "", "drop", "unknown"] {
        let error = validate_policy(&policy_with(rule(
            RuleKind::Match,
            "",
            Target::from(action),
            0,
        )))
        .unwrap_err();
        assert_eq!(error.diagnostics[0].code, "invalid-rule");
        assert!(!error.to_string().contains(action) || action.is_empty());
    }
    for kind in ["process-name", "process-path", "geoip", "DOMAIN", ""] {
        let error = validate_policy(&policy_with(rule(
            RuleKind::from(kind),
            "invalid secret",
            Target::Proxy,
            0,
        )))
        .unwrap_err();
        assert_eq!(error.diagnostics[0].code, "unsupported-matcher");
        assert_eq!(
            error.diagnostics[0].message,
            "rule matcher is not supported for forwarded clients"
        );
    }
}

#[test]
fn controlled_sets_and_no_resolve_match_the_go_rule_problem() {
    for value in ["cn-domain", "cn-ip", "proxy-domain"] {
        let mut rule = rule(RuleKind::RuleSet, value, Target::Direct, 0);
        assert!(validate_policy(&policy_with(rule.clone())).is_ok());
        rule.no_resolve = true;
        assert_eq!(
            validate_policy(&policy_with(rule)).is_ok(),
            value == "cn-ip"
        );
    }
    for value in ["CN-IP", "geosite-cn", "", "custom-secret"] {
        let error = validate_policy(&policy_with(rule(
            RuleKind::RuleSet,
            value,
            Target::from("invalid"),
            0,
        )))
        .unwrap_err();
        assert_eq!(error.diagnostics[0].code, "unsupported-rule-set");
    }
    for kind in [
        RuleKind::Domain,
        RuleKind::DomainSuffix,
        RuleKind::DomainKeyword,
        RuleKind::Match,
    ] {
        let value = if kind == RuleKind::Match {
            ""
        } else {
            "example.com"
        };
        let mut rule = rule(kind, value, Target::Direct, 0);
        rule.no_resolve = true;
        let error = validate_policy(&policy_with(rule)).unwrap_err();
        assert_eq!(
            error.diagnostics[0].message,
            "rule value or option exceeds supported limits"
        );
    }
    assert!(
        validate_policy(&policy_with(rule(
            RuleKind::Match,
            "not-empty",
            Target::Direct,
            0
        )))
        .is_err()
    );
    let mut cidr = rule(RuleKind::IpCidr, "10.0.0.9/8", Target::Block, 0);
    cidr.no_resolve = true;
    assert!(validate_policy(&policy_with(cidr)).is_ok());
}

#[test]
fn cidr_prefix_parser_masks_hosts_rejects_mapped_ipv6_and_handles_go_format() {
    for (input, canonical) in [
        ("192.0.2.253/24", "192.0.2.0/24"),
        ("255.255.255.255/0", "0.0.0.0/0"),
        ("192.0.2.1/32", "192.0.2.1/32"),
        ("2001:0DB8:0000:0000:1:0:0:1/64", "2001:db8::/64"),
        ("2001:db8:0:1:0:0:0:1/128", "2001:db8:0:1::1/128"),
        ("::ffff:0:192.0.2.1/128", "::ffff:0:c000:201/128"),
        ("::192.0.2.1/128", "::c000:201/128"),
        ("ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff/0", "::/0"),
    ] {
        let a = rule(RuleKind::IpCidr, input, Target::Direct, 1);
        let b = rule(RuleKind::IpCidr, canonical, Target::Direct, 2);
        assert!(validate_policy(&policy_with(a.clone())).is_ok(), "{input}");
        assert_eq!(
            subscription_fingerprints(&[a]).unwrap(),
            subscription_fingerprints(&[b]).unwrap(),
            "{input}"
        );
    }
    for invalid in [
        "10.0.0.1",
        "10.0.0.1/33",
        "10.0.0.1/-1",
        "10.0.0.1/+1",
        "10.0.0.1/01",
        "10.0.0.1/00",
        "010.0.0.1/8",
        "10.0.0.1/ 1",
        "::1/129",
        "fe80::1%eth0/64",
        "::ffff:192.0.2.1/128",
        "::ffff:c000:0201/128",
        "::ffff:0:0/96",
        "[::1]/128",
        "::1/064",
        "1.2.3/8",
        "1.2.3.4/",
        "1.2.3.4/1/2",
    ] {
        assert!(
            validate_policy(&policy_with(rule(
                RuleKind::IpCidr,
                invalid,
                Target::Direct,
                0
            )))
            .is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn normalized_semantic_fingerprints_exclude_index_and_count_duplicates_in_source_order() {
    let original = domain("Example.COM", 72);
    let duplicate = domain("example.com", -9);
    let distinct = domain("another.example", 15);
    let refs = subscription_fingerprints(&[original.clone(), distinct.clone(), duplicate.clone()])
        .unwrap();
    assert_eq!(&refs[0][..64], &refs[2][..64]);
    assert!(refs[0].ends_with(":1"));
    assert!(refs[2].ends_with(":2"));
    assert_eq!(refs[1], subscription_fingerprints(&[distinct]).unwrap()[0]);
    let mut policy = policy_with(original);
    let revision = policy_revision(&policy).unwrap();
    policy.rules[0].rule = duplicate;
    assert_eq!(revision, policy_revision(&policy).unwrap());
    policy.rules[0].label = " visible spacing ".into();
    assert_ne!(revision, policy_revision(&policy).unwrap());
    assert_eq!(policy.rules[0].label, " visible spacing ");
}

#[test]
fn edit_validation_checks_exact_unique_references_and_exclusive_intent() {
    let mut policy = Policy {
        rules: vec![],
        subscription_edits: vec![disable("edit", fingerprint(MAX_RULES))],
    };
    assert!(validate_policy(&policy).is_ok());
    for invalid in [
        fingerprint(0),
        fingerprint(MAX_RULES + 1),
        format!("{}:01", "a".repeat(64)),
        format!("{}:+1", "a".repeat(64)),
        format!("{}:1", "A".repeat(64)),
        "a:1".into(),
        format!("{}:1 ", "a".repeat(64)),
        format!("{}:1", "a".repeat(63)),
    ] {
        policy.subscription_edits[0].source_fingerprint = invalid;
        assert_eq!(
            validate_policy(&policy).unwrap_err().diagnostics[0].code,
            "invalid-reference"
        );
    }
    policy.subscription_edits[0].source_fingerprint = fingerprint(1);
    policy.subscription_edits[0].disabled = false;
    assert_eq!(
        validate_policy(&policy).unwrap_err().diagnostics[0].code,
        "invalid-edit"
    );
    policy.subscription_edits[0].replacement = Some(domain("replacement.example", 0));
    assert!(validate_policy(&policy).is_ok());
    policy.subscription_edits[0].disabled = true;
    assert_eq!(
        validate_policy(&policy).unwrap_err().diagnostics[0].code,
        "invalid-edit"
    );
    policy.subscription_edits[0].disabled = false;
    policy
        .subscription_edits
        .push(disable("second", fingerprint(1)));
    assert_eq!(
        validate_policy(&policy).unwrap_err().diagnostics[0].code,
        "invalid-reference"
    );
    policy.subscription_edits[1].source_fingerprint = fingerprint(2);
    policy.subscription_edits[0].replacement = Some(domain("bad..domain", 0));
    assert_eq!(
        validate_policy(&policy).unwrap_err().diagnostics[0].code,
        "invalid-rule"
    );
}

#[test]
fn enabled_local_precedence_disabled_drafts_and_provenance_are_stable_and_owned() {
    let subscription = vec![domain("UPSTREAM.example", 902), domain("second.example", 4)];
    let policy = Policy {
        rules: vec![
            local("first", true, domain("LOCAL.example", 812)),
            local("draft", false, domain("draft.example", 813)),
            local("second", true, domain("second-local.example", 814)),
        ],
        subscription_edits: vec![],
    };
    let snapshot = policy.clone();
    let mut out = merge_effective_policy(&subscription, &policy).unwrap();
    assert_eq!(
        out.rules
            .iter()
            .map(|r| r.value.as_str())
            .collect::<Vec<_>>(),
        [
            "LOCAL.example",
            "second-local.example",
            "UPSTREAM.example",
            "second.example"
        ]
    );
    assert_eq!(codes(&out), ["disabled-rule"]);
    assert_eq!(out.diagnostics[0].scope, "local-rule");
    assert_eq!(out.diagnostics[0].index, 1);
    assert_eq!(out.provenance[0].source_index, -1);
    assert_eq!(out.provenance[0].source_ordinal, 0);
    assert_eq!(out.provenance[1].source_ordinal, 2);
    assert_eq!(out.provenance[2].source_index, 902);
    assert_eq!(out.provenance[2].source_ordinal, 0);
    assert_eq!(out.provenance[2].effective_index, 2);
    assert_eq!(out.rules[2], subscription[0]);
    assert_eq!(
        out.provenance[2].source_fingerprint,
        subscription_fingerprints(&subscription).unwrap()[0]
    );
    out.rules[0].value.clear();
    out.provenance[0].stable_id.clear();
    assert_eq!(snapshot, policy);
    assert_eq!(subscription[0].value, "UPSTREAM.example");
}

#[test]
fn edits_match_duplicate_occurrence_replace_preserving_source_index_and_disable_explicitly() {
    let subscription = vec![
        domain("EXAMPLE.com", 22),
        domain("unrelated.com", 41),
        domain("example.COM", 90),
    ];
    let refs = subscription_fingerprints(&subscription).unwrap();
    let policy = Policy {
        rules: vec![],
        subscription_edits: vec![
            disable("disable-first", refs[0].clone()),
            SubscriptionEdit {
                id: "replace-second".into(),
                source_fingerprint: refs[2].clone(),
                disabled: false,
                replacement: Some(rule(
                    RuleKind::DomainKeyword,
                    "Changed",
                    Target::Block,
                    -100,
                )),
                label: "Visible label".into(),
                note: "note".into(),
            },
        ],
    };
    let out = merge_effective_policy(&subscription, &policy).unwrap();
    assert_eq!(out.rules.len(), 2);
    assert_eq!(out.rules[0], subscription[1]);
    assert_eq!(out.rules[1].value, "Changed");
    assert_eq!(out.rules[1].index, 90);
    assert_eq!(out.provenance[1].stable_id, "replace-second");
    assert_eq!(out.provenance[1].label, "Visible label");
    assert_eq!(out.provenance[1].layer, "subscription");
    assert_eq!(out.provenance[1].source_fingerprint, refs[2]);
    assert_eq!(out.provenance[1].source_index, 90);
    assert_eq!(out.provenance[1].source_ordinal, 2);
    assert_eq!(codes(&out), ["disabled-rule"]);
    let refreshed = vec![subscription[1].clone()];
    let orphaned = merge_effective_policy(&refreshed, &policy).unwrap();
    assert_eq!(orphaned.rules, refreshed);
    assert_eq!(codes(&orphaned), ["orphaned-edit", "orphaned-edit"]);
    assert_eq!(orphaned.diagnostics[0].index, 0);
    assert_eq!(orphaned.diagnostics[1].index, 1);
    assert_eq!(
        policy.subscription_edits[1]
            .replacement
            .as_ref()
            .unwrap()
            .index,
        -100
    );
}

#[test]
fn terminal_match_retains_unreachable_preview_and_diagnostic_order() {
    let subscription = vec![
        domain("upstream.example", 10),
        rule(RuleKind::Match, "", Target::Proxy, 11),
    ];
    let policy = Policy {
        rules: vec![
            local(
                "terminal",
                true,
                rule(RuleKind::Match, "", Target::Block, 77),
            ),
            local("disabled", false, domain("disabled.example", 78)),
            local("after", true, domain("after.example", 79)),
        ],
        subscription_edits: vec![disable("orphan", fingerprint(1))],
    };
    let out = merge_effective_policy(&subscription, &policy).unwrap();
    assert_eq!(out.rules.len(), 4);
    assert_eq!(
        codes(&out),
        [
            "disabled-rule",
            "unreachable-rule",
            "unreachable-rule",
            "unreachable-rule",
            "orphaned-edit"
        ]
    );
    for (i, d) in out.diagnostics[1..4].iter().enumerate() {
        assert_eq!(d.scope, "effective-rule");
        assert_eq!(d.index, (i + 1) as i64);
    }
    assert_eq!(out.provenance[0].effective_index, 0);
    assert_eq!(out.rules[0].index, 77);
}

#[test]
fn subscription_bound_and_errors_are_fixed_and_prefer_policy_errors() {
    let subscription = vec![domain("example.com", 0); MAX_RULES];
    let refs = subscription_fingerprints(&subscription).unwrap();
    assert_eq!(refs.len(), MAX_RULES);
    assert_eq!(
        refs.last().unwrap(),
        &format!("{}:{MAX_RULES}", &refs[0][..64])
    );
    assert!(merge_effective_policy(&subscription, &Policy::default()).is_ok());
    let too_many = vec![domain("example.com", 0); MAX_RULES + 1];
    assert!(subscription_fingerprints(&too_many).is_err());
    let error = merge_effective_policy(&too_many, &Policy::default()).unwrap_err();
    assert_eq!(error.diagnostics[0].code, "subscription-limit");
    let bad_policy = policy_with(domain("bad..name", 0));
    let error = merge_effective_policy(&too_many, &bad_policy).unwrap_err();
    assert_eq!(error.diagnostics[0].scope, "local-rule");
    let error = merge_effective_policy(
        &[
            domain("bad..name", 0),
            rule(RuleKind::from("process-path"), "private", Target::Proxy, 1),
        ],
        &Policy::default(),
    )
    .unwrap_err();
    assert_eq!(
        error
            .diagnostics
            .iter()
            .map(|d| d.code.as_str())
            .collect::<Vec<_>>(),
        ["invalid-rule", "unsupported-matcher"]
    );
    assert_eq!(error.diagnostics[0].index, 0);
    assert_eq!(error.diagnostics[1].index, 1);
    assert!(!format!("{error}").contains("private"));
}

#[test]
fn serde_shapes_use_go_tokens_omissions_and_owned_empty_arrays() {
    let value: Rule =
        serde_json::from_str(r#"{"kind":"match","target":"block","index":-1}"#).unwrap();
    assert_eq!(value.kind, RuleKind::Match);
    assert_eq!(value.target, Target::Block);
    assert_eq!(
        serde_json::to_string(&value).unwrap(),
        r#"{"kind":"match","target":"block","index":-1}"#
    );
    let unknown: Rule = serde_json::from_str(
        r#"{"kind":"process-name","value":"private","target":"reject","index":1}"#,
    )
    .unwrap();
    assert_eq!(unknown.kind, RuleKind::from("process-name"));
    assert!(validate_policy(&policy_with(unknown)).is_err());
    let out = merge_effective_policy(&[], &Policy::default()).unwrap();
    assert_eq!(
        serde_json::to_string(&out).unwrap(),
        r#"{"rules":[],"provenance":[],"diagnostics":[]}"#
    );
    let edit = disable("edit", fingerprint(1));
    assert!(
        !serde_json::to_string(&edit)
            .unwrap()
            .contains("replacement")
    );
    assert_eq!(
        serde_json::to_string(&Policy::default()).unwrap(),
        r#"{"rules":[],"subscriptionEdits":[]}"#
    );
}

#[test]
fn merge_allocates_only_eligible_rule_and_provenance_slots() {
    let subscription = vec![
        domain("first.example", 11),
        domain("second.example", 22),
        domain("third.example", 33),
    ];
    let refs = subscription_fingerprints(&subscription).unwrap();
    let policy = Policy {
        rules: vec![
            local("enabled", true, domain("local.example", 44)),
            local("disabled", false, domain("draft.example", 55)),
        ],
        subscription_edits: vec![
            disable("disabled-source", refs[0].clone()),
            SubscriptionEdit {
                id: "replacement".into(),
                source_fingerprint: refs[1].clone(),
                replacement: Some(domain("replacement.example", -1)),
                ..SubscriptionEdit::default()
            },
            disable("orphan", fingerprint(1)),
        ],
    };
    let out = merge_effective_policy(&subscription, &policy).unwrap();
    assert_eq!(out.rules.len(), 3);
    assert_eq!(out.rules.capacity(), out.rules.len());
    assert_eq!(out.provenance.capacity(), out.provenance.len());
    assert_eq!(out.rules[1].value, "replacement.example");
    assert_eq!(out.rules[1].index, 22);
    assert_eq!(out.provenance[1].stable_id, "replacement");
    assert_eq!(out.provenance[1].source_fingerprint, refs[1]);
    assert_eq!(
        codes(&out),
        ["disabled-rule", "disabled-rule", "orphaned-edit"]
    );
    assert_eq!(out.diagnostics[0].index, 1);
    assert_eq!(out.diagnostics[1].index, 0);
    assert_eq!(out.diagnostics[2].index, 2);

    let all_disabled = Policy {
        rules: vec![local("draft", false, domain("draft.example", 0))],
        subscription_edits: refs
            .into_iter()
            .enumerate()
            .map(|(i, reference)| disable(&format!("edit-{i}"), reference))
            .collect(),
    };
    let empty = merge_effective_policy(&subscription, &all_disabled).unwrap();
    assert!(empty.rules.is_empty() && empty.provenance.is_empty());
    assert_eq!(empty.rules.capacity(), 0);
    assert_eq!(empty.provenance.capacity(), 0);
    assert_eq!(empty.diagnostics.len(), 4);
}

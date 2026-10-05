#![cfg(unix)]

use be6500_panel::policy::{LocalRule, Policy, Rule, RuleKind, Target, policy_revision};
use be6500_panel::policy_store::{FILE_NAME, MAX_FILE_BYTES, Store, StoreError};
use serde::Deserialize;
use std::fs;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "be6500-policy-store-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
    fn document(&self) -> PathBuf {
        self.0.join(FILE_NAME)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn policy() -> Policy {
    Policy {
        rules: vec![LocalRule {
            id: "local-1".into(),
            enabled: true,
            label: "private draft".into(),
            note: "private note".into(),
            rule: Rule {
                kind: RuleKind::Domain,
                value: "EXample.com".into(),
                target: Target::Proxy,
                index: 19,
                no_resolve: false,
            },
        }],
        subscription_edits: vec![],
    }
}

#[test]
fn missing_is_empty_and_save_is_private_owned_and_reopenable() {
    let temp = Temp::new();
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o755)).unwrap();
    let mut store = Store::open(temp.path()).unwrap();
    assert_eq!(store.snapshot().policy, Policy::default());
    assert_eq!(
        store.snapshot().revision,
        policy_revision(&Policy::default()).unwrap()
    );
    assert!(!temp.document().exists());
    assert_eq!(
        fs::metadata(temp.path()).unwrap().permissions().mode() & 0o777,
        0o700
    );
    let input = policy();
    let result = store.save(&input).unwrap();
    assert!(result.committed);
    assert_eq!(result.durability_error, None);
    assert_eq!(result.snapshot.policy, input);
    assert_eq!(result.snapshot.revision, policy_revision(&input).unwrap());
    let stored_bytes = fs::read(temp.document()).unwrap();
    let encoded = serde_json::to_vec(&input).unwrap();
    assert_eq!(stored_bytes, encoded);
    assert_eq!(
        fs::metadata(temp.document()).unwrap().len(),
        encoded.len() as u64
    );
    use sha2::{Digest, Sha256};
    assert_eq!(Sha256::digest(&stored_bytes), Sha256::digest(&encoded));
    assert_eq!(
        fs::metadata(temp.document()).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let mut detached = store.snapshot();
    detached.policy.rules[0].rule.value = "different.test".into();
    assert_eq!(store.snapshot().policy, input);
    let mut returned = result.snapshot;
    returned.policy.rules[0].note.clear();
    assert_eq!(store.snapshot().policy, input);
    let reopened = Store::open(temp.path()).unwrap();
    assert_eq!(reopened.snapshot(), store.snapshot());
    assert!(!format!("{store:?} {reopened:?} {:?}", store.snapshot()).contains("private note"));
    assert!(!format!("{store:?}").contains(temp.path().to_str().unwrap()));
}

#[test]
fn required_directory_and_directory_identity_are_checked() {
    assert_eq!(Store::open("").unwrap_err(), StoreError::InvalidInput);
    let temp = Temp::new();
    let data = temp.path().join("data");
    let mut store = Store::open(&data).unwrap();
    store.save(&policy()).unwrap();
    let accepted = store.snapshot();
    fs::rename(&data, temp.path().join("old")).unwrap();
    fs::create_dir(&data).unwrap();
    assert_eq!(store.load().unwrap_err(), StoreError::Storage);
    assert_eq!(
        store.save(&Policy::default()).unwrap_err(),
        StoreError::Storage
    );
    assert_eq!(store.snapshot(), accepted);
    assert!(!data.join(FILE_NAME).exists());
    fs::remove_dir(&data).unwrap();
    symlink(temp.path().join("old"), &data).unwrap();
    assert_eq!(Store::open(&data).unwrap_err(), StoreError::Storage);
    assert_eq!(
        Store::open(data.join(".")).unwrap_err(),
        StoreError::Storage
    );
}

#[test]
fn malformed_documents_never_erase_accepted_memory_or_replace_disk() {
    let temp = Temp::new();
    let mut store = Store::open(temp.path()).unwrap();
    store.save(&policy()).unwrap();
    let accepted = store.snapshot();
    let cases = [
        "",
        "null",
        "[]",
        "{}",
        "{",
        "{\"rules\":[]}",
        r#"{"rules":null,"subscriptionEdits":[]}"#,
        r#"{"rules":[],"subscriptionEdits":null}"#,
        r#"{"rules":[],"subscriptionEdits":[],"private-secret":0}"#,
        r#"{"rules":[],"rules":[],"subscriptionEdits":[]}"#,
        r#"{"rules":[],"subscriptionEdits":[],"subscriptionEdits":[]}"#,
        r#"{"rules":[{"id":"x","id":"x"}],"subscriptionEdits":[]}"#,
        r#"{"rules":[{"id":"x","unknown":true}],"subscriptionEdits":[]}"#,
        r#"{"rules":[{"id":"x","rule":{"kind":"match","target":"direct","unknown":0}}],"subscriptionEdits":[]}"#,
        r#"{"rules":[{"id":"x","rule":{"kind":"match","kind":"match","target":"direct"}}],"subscriptionEdits":[]}"#,
        r#"{"rules":[],"subscriptionEdits":[{"id":"x","replacement":null,"replacement":null}]}"#,
        r#"{"rules":[],"subscriptionEdits":[{"id":"x","unknown":0}]}"#,
        r#"{"rules":[],"subscriptionEdits":[{"id":"x","sourceFingerprint":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa:1","disabled":true,"replacement":null}]}"#,
        r#"{"rules":[null],"subscriptionEdits":[]}"#,
        r#"{"rules":[{"id":null}],"subscriptionEdits":[]}"#,
        r#"{"rules":[{"id":"x","rule":null}],"subscriptionEdits":[]}"#,
        r#"{"rules":[{"id":"x","rule":{"kind":"invalid-private","target":"direct"}}],"subscriptionEdits":[]}"#,
        r#"{"rules":[],"subscriptionEdits":[]} null"#,
    ];
    for raw in cases {
        fs::write(temp.document(), raw).unwrap();
        let error = store.load().unwrap_err();
        assert_eq!(error, StoreError::Storage, "input case failed");
        assert_eq!(store.snapshot(), accepted);
        assert_eq!(fs::read(temp.document()).unwrap(), raw.as_bytes());
        assert_eq!(error.to_string(), "local proxy rule storage unavailable");
        assert_eq!(Store::open(temp.path()).unwrap_err(), StoreError::Storage);
    }
}

#[test]
fn bounded_documents_and_short_collection_overflows_fail_closed() {
    let temp = Temp::new();
    let mut store = Store::open(temp.path()).unwrap();
    let empty = br#"{"rules":[],"subscriptionEdits":[]}"#;
    let mut exact = empty.to_vec();
    exact.resize(MAX_FILE_BYTES, b' ');
    fs::write(temp.document(), &exact).unwrap();
    assert_eq!(store.load().unwrap().policy, Policy::default());
    exact.push(b' ');
    fs::write(temp.document(), &exact).unwrap();
    assert_eq!(store.load().unwrap_err(), StoreError::Storage);
    for (field, count, other) in [
        ("rules", 513, "subscriptionEdits"),
        ("subscriptionEdits", 1025, "rules"),
    ] {
        let raw = format!(
            "{{\"{field}\":[{}],\"{other}\":[]}}",
            vec!["{}"; count].join(",")
        );
        fs::write(temp.document(), raw).unwrap();
        assert_eq!(store.load().unwrap_err(), StoreError::Storage);
    }
    fs::remove_file(temp.document()).unwrap();
    let mut huge = policy();
    huge.rules = (0..512)
        .map(|i| {
            let mut rule = huge.rules[0].clone();
            rule.id = format!("id-{i}");
            rule.note = "🦀".repeat(256);
            rule
        })
        .collect();
    assert_eq!(store.save(&huge).unwrap_err(), StoreError::DocumentSize);
    assert!(!temp.document().exists());
    huge.rules[0].rule.value = "not valid private".into();
    assert_eq!(store.save(&huge).unwrap_err(), StoreError::InvalidPolicy);
    assert!(!temp.document().exists());
}

#[test]
fn symlink_fifo_and_nonregular_documents_are_rejected_without_blocking() {
    let temp = Temp::new();
    let mut store = Store::open(temp.path()).unwrap();
    let external = temp.path().join("external");
    fs::write(&external, b"private external").unwrap();
    symlink(&external, temp.document()).unwrap();
    assert_eq!(store.load().unwrap_err(), StoreError::Storage);
    assert_eq!(store.save(&policy()).unwrap_err(), StoreError::Storage);
    assert_eq!(fs::read(&external).unwrap(), b"private external");
    fs::remove_file(temp.document()).unwrap();
    let name = std::ffi::CString::new(temp.document().as_os_str().as_encoded_bytes()).unwrap();
    // SAFETY: CString is terminated and points to a test-only path.
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    assert_eq!(store.load().unwrap_err(), StoreError::Storage);
    assert_eq!(store.save(&policy()).unwrap_err(), StoreError::Storage);
    fs::remove_file(temp.document()).unwrap();
    fs::create_dir(temp.document()).unwrap();
    assert_eq!(store.load().unwrap_err(), StoreError::Storage);
    assert_eq!(store.save(&policy()).unwrap_err(), StoreError::Storage);
}

#[test]
fn loading_existing_valid_file_repairs_private_mode_but_invalid_file_is_untouched() {
    let temp = Temp::new();
    fs::write(temp.document(), serde_json::to_vec(&policy()).unwrap()).unwrap();
    fs::set_permissions(temp.document(), fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        Store::open(temp.path()).unwrap().snapshot().policy,
        policy()
    );
    assert_eq!(
        fs::metadata(temp.document()).unwrap().permissions().mode() & 0o777,
        0o600
    );
    fs::write(temp.document(), b"private invalid").unwrap();
    fs::set_permissions(temp.document(), fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(Store::open(temp.path()).unwrap_err(), StoreError::Storage);
    assert_eq!(
        fs::metadata(temp.document()).unwrap().permissions().mode() & 0o777,
        0o644
    );
}

#[test]
fn public_errors_are_fixed_and_debug_is_safe() {
    for (error, message) in [
        (StoreError::Storage, "local proxy rule storage unavailable"),
        (
            StoreError::InvalidInput,
            "invalid local proxy rule store input",
        ),
        (StoreError::InvalidPolicy, "invalid local proxy policy"),
        (
            StoreError::DocumentSize,
            "local proxy policy document exceeds storage limit",
        ),
        (
            StoreError::InsufficientSpace,
            "persistent storage needs free space for safe configuration recovery",
        ),
        (
            StoreError::Measurement,
            "persistent storage free space is unavailable",
        ),
    ] {
        assert_eq!(error.to_string(), message);
        assert!(!format!("{error:?}").contains("private"));
    }
}

#[test]
fn go_reference_policies_keep_their_semantic_revisions_through_storage() {
    #[derive(Deserialize)]
    struct Fixtures {
        cases: Vec<Case>,
    }
    #[derive(Deserialize)]
    struct Case {
        policy: Policy,
        valid: bool,
        revision: Option<String>,
    }
    let fixtures: Fixtures =
        serde_json::from_str(include_str!("fixtures/local-policy.json")).unwrap();
    let temp = Temp::new();
    let mut store = Store::open(temp.path()).unwrap();
    for case in fixtures.cases.into_iter().filter(|case| case.valid) {
        let saved = store.save(&case.policy).unwrap();
        assert_eq!(Some(saved.snapshot.revision), case.revision);
        assert_eq!(
            Store::open(temp.path()).unwrap().snapshot().policy,
            case.policy
        );
    }
    let trusted: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/local-policy.json")).unwrap();
    for case in trusted["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["valid"] == true)
    {
        fs::write(
            temp.document(),
            serde_json::to_vec(&case["policy"]).unwrap(),
        )
        .unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.revision, case["revision"].as_str().unwrap());
    }
}

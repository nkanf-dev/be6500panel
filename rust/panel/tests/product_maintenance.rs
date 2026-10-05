//! Synthetic source-only coverage. No router, daemon, command or Go dependency.
use be6500_panel::product_io::{Backend, Error, Output, Program};
use be6500_panel::product_maintenance::{
    MAX_BACKUP_BYTES, MAX_DOCUMENT_BYTES, MAX_PREVIEWS, Maintenance, edit_frpc, frpc_projection,
};
use be6500_panel::readiness_tun::Budget;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

struct NeverIo {
    calls: usize,
}
impl Backend for NeverIo {
    fn read(&mut self, _: &Path, _: usize, _: &Budget<'_>) -> Result<Vec<u8>, Error> {
        self.calls += 1;
        Err(Error::Unavailable)
    }
    fn run(
        &mut self,
        _: Program,
        _: &[String],
        _: Option<&[u8]>,
        _: usize,
        _: &Budget<'_>,
    ) -> Result<Output, Error> {
        self.calls += 1;
        Err(Error::Unavailable)
    }
    fn now_unix(&self) -> u64 {
        1790985600
    }
}
fn budget(cancel: &AtomicBool) -> Budget<'_> {
    Budget {
        deadline: Instant::now() + Duration::from_secs(10),
        cancel,
    }
}
fn store() -> Maintenance {
    Maintenance::open(Path::new("/synthetic/private-data")).unwrap()
}
fn native_documents() -> Value {
    json!({"generation":7,"pendingCommit":null,"documents":[
        {"module":"network","content":"config interface 'lan'\n option proto 'static'\n option ipaddr '192.168.31.1'\n"},
        {"module":"wireless","content":"config wifi-device 'radio0'\n option channel 'auto'\nconfig wifi-iface 'main'\n option device 'radio0'\n option network 'lan'\n option key 'synthetic-wifi-only'\n"},
        {"module":"dhcp","content":"config dhcp 'lan'\n option interface 'lan'\n option start '100'\n"},
        {"module":"firewall","content":"config defaults\n option input 'ACCEPT'\n"},
        {"module":"system","content":"# vendor comment\nconfig system\n option hostname 'synthetic'\n option vendor_value 'unchanged'\n"},
        {"module":"dropbear","content":"config dropbear\n option Port '22'\n"}
    ]})
}
fn metadata() -> Value {
    let mut value = json!({"model":"RN02","build":"synthetic-build","generation":7,"runtimeGenerations":{"runtime.frpc":3,"runtime.sing-box":4},"pendingCommit":null});
    value["documents"] = native_documents()["documents"].clone();
    value
}
fn backup(scopes: &[&str]) -> Vec<u8> {
    let cancel = AtomicBool::new(false);
    let mut io = NeverIo { calls: 0 };
    store()
        .backup(
            &scopes.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            metadata(),
            native_documents(),
            &[],
            None,
            &mut io,
            &budget(&cancel),
        )
        .unwrap()
}
fn changed() -> Vec<u8> {
    let mut value: Value = serde_json::from_slice(&backup(&["system", "dropbear"])).unwrap();
    value["documents"][0]["content"] = json!(format!(
        "{} option timezone 'UTC'\n",
        value["documents"][0]["content"].as_str().unwrap()
    ));
    value["documents"][1]["content"] = json!(format!(
        "{} option SSHKeepAlive '60'\n",
        value["documents"][1]["content"].as_str().unwrap()
    ));
    seal(value)
}
fn seal(mut value: Value) -> Vec<u8> {
    for document in value["documents"].as_array_mut().unwrap() {
        document["digest"] = json!(format!(
            "{:x}",
            Sha256::digest(document["content"].as_str().unwrap().as_bytes())
        ));
    }
    serde_json::to_vec(&value).unwrap()
}
fn preview_code(raw: &[u8]) -> &'static str {
    let cancel = AtomicBool::new(false);
    store()
        .preview(raw, metadata(), 7, &budget(&cancel))
        .unwrap_err()
        .code
}
#[test]
fn legacy_scoped_backup_is_real_exact_content_and_never_mutates() {
    let cancel = AtomicBool::new(false);
    let maintenance = store();
    let mut io = NeverIo { calls: 0 };
    let raw = maintenance
        .backup(
            &["system".into(), "wireless".into()],
            metadata(),
            native_documents(),
            &[],
            None,
            &mut io,
            &budget(&cancel),
        )
        .unwrap();
    let value: Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(value["generation"], 7);
    assert_eq!(value["model"], "RN02");
    assert_eq!(value["documents"].as_array().unwrap().len(), 2);
    for document in value["documents"].as_array().unwrap() {
        assert_eq!(
            document["digest"],
            format!(
                "{:x}",
                Sha256::digest(document["content"].as_str().unwrap().as_bytes())
            )
        );
        assert!(document.get("generation").is_none());
    }
    assert!(
        String::from_utf8(raw.clone())
            .unwrap()
            .contains("synthetic-wifi-only")
    );
    let p = maintenance
        .preview(&raw, metadata(), 7, &budget(&cancel))
        .unwrap();
    assert_eq!(p["summary"]["unchanged"], 2);
    assert_eq!(io.calls, 0);
}
#[test]
fn original_json_strictly_rejects_duplicate_unknown_case_null_and_trailing_fields() {
    let raw = String::from_utf8(backup(&["system"])).unwrap();
    let malformed = [
        raw.replacen(
            "\"model\":\"RN02\"",
            "\"model\":\"RN02\",\"model\":\"OTHER\"",
            1,
        ),
        raw.replacen(
            "\"module\":\"system\"",
            "\"module\":\"system\",\"module\":\"system\"",
            1,
        ),
        raw.replacen("\"model\"", "\"Model\"", 1),
        raw.replacen(
            "\"model\":\"RN02\"",
            "\"model\":\"RN02\",\"files\":[{\"path\":\"/etc/shadow\"}]",
            1,
        ),
        raw.replacen("\"scopes\":[\"system\"]", "\"scopes\":null", 1),
        format!("{raw} {{}}"),
    ];
    for bad in malformed {
        assert_eq!(preview_code(bad.as_bytes()), "invalid_backup");
    }
    assert_eq!(
        preview_code(&vec![b' '; MAX_BACKUP_BYTES + 1]),
        "backup_too_large"
    );
    assert_eq!(preview_code(b"{\"model\":\"\xff\"}"), "invalid_backup");
}
#[test]
fn manifests_hashes_document_generation_and_bounds_are_not_guessed() {
    let original: Value = serde_json::from_slice(&backup(&["system"])).unwrap();
    let mut value = original.clone();
    value["documents"][0]["digest"] = json!("0".repeat(64));
    assert_eq!(
        preview_code(&serde_json::to_vec(&value).unwrap()),
        "digest_mismatch"
    );
    let mut value = original.clone();
    value["generation"] = json!(0);
    assert_eq!(preview_code(&seal(value)), "invalid_manifest");
    let mut value = original.clone();
    value["createdAt"] = json!("2026-02-30T00:00:00Z");
    assert_eq!(preview_code(&seal(value)), "invalid_manifest");
    let mut value = original.clone();
    value["documents"][0]["content"] = json!("x".repeat(MAX_DOCUMENT_BYTES + 1));
    assert_eq!(preview_code(&seal(value)), "document_too_large");
    let mut value = original.clone();
    value["scopes"] = json!(["system", "network"]);
    assert_eq!(preview_code(&seal(value)), "missing_component");
    let mut value = original.clone();
    value["documents"][0]["privateFields"] = json!({"password":"synthetic"});
    assert_eq!(preview_code(&seal(value)), "invalid_backup");
    let mut value = original;
    value["scopes"] = json!(["system", "system"]);
    assert_eq!(preview_code(&seal(value)), "duplicate_scope");
}
#[test]
fn preview_model_generation_selection_and_native_order_gates() {
    let cancel = AtomicBool::new(false);
    let maintenance = store();
    let b = budget(&cancel);
    let mut raw: Value = serde_json::from_slice(&changed()).unwrap();
    raw["model"] = json!("OTHER");
    let preview = maintenance.preview(&seal(raw), metadata(), 7, &b).unwrap();
    let id = preview["id"].as_str().unwrap();
    assert_eq!(preview["modelMismatch"], true);
    assert_eq!(preview["summary"]["modified"], 2);
    assert_eq!(
        maintenance
            .candidates(id, 7, &["system".into()], false, &b)
            .unwrap_err()
            .code,
        "model_mismatch"
    );
    assert_eq!(
        maintenance
            .candidates(id, 8, &["system".into()], true, &b)
            .unwrap_err()
            .code,
        "generation_conflict"
    );
    assert_eq!(
        maintenance
            .candidates(id, 7, &["runtime.frpc".into()], true, &b)
            .unwrap_err()
            .code,
        "invalid_selection"
    );
    assert_eq!(
        maintenance
            .candidates(id, 7, &["system".into(), "system".into()], true, &b)
            .unwrap_err()
            .code,
        "invalid_selection"
    );
    let candidates = maintenance
        .candidates(id, 7, &["dropbear".into(), "system".into()], true, &b)
        .unwrap();
    assert_eq!(candidates[0].module, "system");
    assert_eq!(candidates[1].module, "dropbear");
    assert_eq!(format!("{:?}", candidates[0]), "ImportCandidate([private])");
    assert!(maintenance.discard(id));
    assert!(!maintenance.discard(id));
    assert_eq!(
        maintenance
            .candidates(id, 7, &["system".into()], true, &b)
            .unwrap_err()
            .code,
        "preview_not_found"
    );
}
#[test]
fn selected_dependencies_not_unselected_upload_are_authoritative() {
    let mut value: Value = serde_json::from_slice(&backup(&["network", "dhcp"])).unwrap();
    for (index, extra) in [
        "config interface 'guest'\n option proto 'static'\n",
        "config dhcp 'guest'\n option interface 'guest'\n",
    ]
    .iter()
    .enumerate()
    {
        value["documents"][index]["content"] = json!(format!(
            "{}{extra}",
            value["documents"][index]["content"].as_str().unwrap()
        ));
    }
    let cancel = AtomicBool::new(false);
    let b = budget(&cancel);
    let maintenance = store();
    let preview = maintenance
        .preview(&seal(value), metadata(), 7, &b)
        .unwrap();
    let id = preview["id"].as_str().unwrap();
    assert_eq!(
        maintenance
            .candidates(id, 7, &["dhcp".into()], false, &b)
            .unwrap_err()
            .code,
        "invalid_reference"
    );
    let selected = maintenance
        .candidates(id, 7, &["dhcp".into(), "network".into()], false, &b)
        .unwrap();
    assert_eq!(selected[0].module, "network");
}
#[test]
fn pending_cancelled_expired_and_cleared_previews_publish_no_private_candidate() {
    let maintenance = store();
    let cancel = AtomicBool::new(false);
    let b = budget(&cancel);
    let mut current = metadata();
    current["pendingCommit"] = json!({"id":"synthetic"});
    assert_eq!(
        maintenance
            .preview(&changed(), current, 7, &b)
            .unwrap_err()
            .code,
        "confirmation_pending"
    );
    let deadline = Budget {
        deadline: Instant::now(),
        cancel: &cancel,
    };
    assert_eq!(
        maintenance
            .preview(&changed(), metadata(), 7, &deadline)
            .unwrap_err()
            .code,
        "operation_timeout"
    );
    let preview = maintenance.preview(&changed(), metadata(), 7, &b).unwrap();
    let id = preview["id"].as_str().unwrap();
    cancel.store(true, Ordering::Relaxed);
    assert_eq!(
        maintenance
            .candidates(id, 7, &["system".into()], false, &b)
            .unwrap_err()
            .code,
        "operation_cancelled"
    );
    cancel.store(false, Ordering::Relaxed);
    maintenance.clear();
    assert_eq!(
        maintenance
            .candidates(id, 7, &["system".into()], false, &b)
            .unwrap_err()
            .code,
        "preview_not_found"
    );
}
#[test]
fn preview_capacity_and_discard_are_bounded_and_memory_only() {
    let cancel = AtomicBool::new(false);
    let b = budget(&cancel);
    let maintenance = store();
    let raw = changed();
    let mut ids = Vec::new();
    for _ in 0..MAX_PREVIEWS {
        ids.push(
            maintenance.preview(&raw, metadata(), 7, &b).unwrap()["id"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
    }
    assert_eq!(
        maintenance
            .preview(&raw, metadata(), 7, &b)
            .unwrap_err()
            .code,
        "preview_limit"
    );
    assert!(maintenance.discard(&ids[0]));
    maintenance.preview(&raw, metadata(), 7, &b).unwrap();
    maintenance.clear();
    assert!(!maintenance.discard(&ids[1]));
}
#[test]
fn runtime_annotations_history_are_explicit_exact_private_preview_only_scopes() {
    let cancel = AtomicBool::new(false);
    let b = budget(&cancel);
    let maintenance = store();
    let mut io = NeverIo { calls: 0 };
    let runtime =
        b"serverAddr = \"saved.test\"\nauth.token = \"synthetic-private-token\"\n".to_vec();
    let annotations = b"{\"revision\":2,\"devices\":{}}";
    let mut current = metadata();
    current["deviceHistory"] = json!({"persistent":false,"retentionDays":7,"devices":[]});
    let raw = maintenance
        .backup(
            &[
                "runtime.frpc".into(),
                "device.annotations".into(),
                "device.history".into(),
            ],
            current.clone(),
            native_documents(),
            &[("frpc".into(), runtime.clone())],
            Some(annotations),
            &mut io,
            &b,
        )
        .unwrap();
    let envelope: Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(envelope["documents"][0]["generation"], 3);
    assert_eq!(
        envelope["documents"][0]["content"],
        String::from_utf8(runtime).unwrap()
    );
    assert_eq!(
        envelope["documents"][1]["content"],
        std::str::from_utf8(annotations).unwrap()
    );
    let preview = maintenance.preview(&raw, current, 7, &b).unwrap();
    assert!(!preview.to_string().contains("synthetic-private-token"));
    for change in preview["changes"].as_array().unwrap() {
        assert_eq!(change["diff"], "");
        assert_eq!(change["stageable"], false);
    }
    assert_eq!(io.calls, 0);
    let mut missing = metadata();
    missing
        .as_object_mut()
        .unwrap()
        .remove("runtimeGenerations");
    assert_eq!(
        maintenance
            .backup(
                &["runtime.frpc".into()],
                missing,
                native_documents(),
                &[("frpc".into(), b"x".to_vec())],
                None,
                &mut io,
                &b
            )
            .unwrap_err()
            .code,
        "runtime_unavailable"
    );
}
#[test]
fn forbidden_backup_paths_and_implicit_private_files_never_get_read() {
    let cancel = AtomicBool::new(false);
    let b = budget(&cancel);
    let mut io = NeverIo { calls: 0 };
    for scope in [
        "/etc/shadow",
        "../network",
        "ssh-key",
        "native",
        "runtime",
        "files",
    ] {
        assert_eq!(
            store()
                .backup(
                    &[scope.into()],
                    metadata(),
                    native_documents(),
                    &[],
                    None,
                    &mut io,
                    &b
                )
                .unwrap_err()
                .code,
            "invalid_scope"
        );
    }
    assert_eq!(io.calls, 0);
}

const TOML: &str = "# accepted private configuration\nserverAddr = 'saved.example.test' # keep\nserverPort = 7443\ntransport.protocol = \"quic\"\ntransport.tls.enable = false\ntransport.poolCount = 3 # unknown\n[auth]\nmethod = \"token\"\ntoken = \"synthetic-saved-token\" # private\nadditionalScopes = [\"HeartBeats\", \"NewWorkConns\"]\n[[proxies]]\nname = \"ssh\"\ntype = \"tcp\"\nlocalIP = \"192.168.31.9\"\nlocalPort = 22\nremotePort = 0 # native assigned\ntransport.useEncryption = true\nmetadata.owner = \"keep me\"\n[[proxies]]\nname = \"web\"\ntype = \"https\"\nlocalPort = 443\ncustomDomains = [\n \"home.example.test\", # keep domain comment\n]\n[proxies.transport]\nuseCompression = true\n";
fn edit_request(
    raw: &str,
    modify: impl FnOnce(&mut Value),
    token: Option<Value>,
) -> Result<String, be6500_panel::product_maintenance::ApiError> {
    let projection = frpc_projection(raw).unwrap();
    assert_eq!(projection["supported"], true);
    let mut input = projection["input"].clone();
    modify(&mut input);
    let mut request = json!({"input":input});
    if let Some(token) = token {
        request["token"] = token;
    }
    edit_frpc(raw, &serde_json::to_vec(&request).unwrap())
}
#[test]
fn frpc_public_projection_has_frontend_fields_not_saved_credential_or_source() {
    let value = frpc_projection(TOML).unwrap();
    assert_eq!(value["hasToken"], true);
    assert_eq!(value["input"]["serverAddress"], "saved.example.test");
    assert_eq!(value["input"]["serverPort"], 7443);
    assert_eq!(value["input"]["proxies"][0]["remotePort"], 0);
    assert_eq!(value["input"]["proxies"][1]["localAddress"], "127.0.0.1");
    assert_eq!(value["input"]["proxies"][0]["sourceId"], "accepted-0");
    for secret in [
        "synthetic-saved-token",
        "keep me",
        "poolCount",
        "source",
        "remoteConnected",
        "tlsNegotiated",
    ] {
        assert!(!value.as_object().unwrap().contains_key(secret));
    }
    assert!(!value.to_string().contains("synthetic-saved-token"));
    assert_eq!(edit_request(TOML, |_| {}, None).unwrap(), TOML);
}
#[test]
fn frpc_lossless_changed_spans_preserve_unknown_options_comments_and_special_ports() {
    let changed = edit_request(
        TOML,
        |input| {
            input["serverPort"] = json!(7001);
            input["proxies"][0]["localPort"] = json!(2222);
        },
        None,
    )
    .unwrap();
    assert_eq!(
        changed,
        TOML.replace("serverPort = 7443", "serverPort = 7001")
            .replace("localPort = 22", "localPort = 2222")
    );
    let crlf = TOML.replace('\n', "\r\n");
    assert_eq!(
        edit_request(&crlf, |input| input["serverPort"] = json!(8000), None).unwrap(),
        crlf.replace("serverPort = 7443", "serverPort = 8000")
    );
}
#[test]
fn frpc_credentials_omit_preserve_replace_clear_delete_strict_intent() {
    assert_eq!(
        edit_request(TOML, |_| {}, Some(json!({"mode":"preserve","value":""}))).unwrap(),
        TOML
    );
    let replaced = edit_request(
        TOML,
        |_| {},
        Some(json!({"mode":"replace","value":"synthetic-new-token"})),
    )
    .unwrap();
    assert_eq!(
        replaced,
        TOML.replace("synthetic-saved-token", "synthetic-new-token")
    );
    for mode in ["clear", "delete"] {
        let cleared = edit_request(TOML, |_| {}, Some(json!({"mode":mode}))).unwrap();
        assert!(!cleared.contains("token ="));
        assert!(cleared.contains("additionalScopes"));
        assert!(cleared.contains("useCompression"));
    }
    for intent in [
        json!({"mode":"replace","value":""}),
        json!({"mode":"preserve","value":"synthetic"}),
        json!({"mode":"clear","extra":true}),
        json!({"mode":"auto"}),
    ] {
        assert_eq!(
            edit_request(TOML, |_| {}, Some(intent)).unwrap_err().code,
            "invalid_field"
        );
    }
}
#[test]
fn frpc_mapping_removal_and_type_changes_affect_only_requested_mapping() {
    let removed = edit_request(
        TOML,
        |v| {
            v["proxies"].as_array_mut().unwrap().pop();
        },
        None,
    )
    .unwrap();
    assert!(!removed.contains("useCompression"));
    assert!(!removed.contains("name = \"web\""));
    assert!(removed.contains("metadata.owner"));
    let switched = edit_request(
        TOML,
        |v| {
            let p = v["proxies"][0].as_object_mut().unwrap();
            p.insert("type".into(), json!("http"));
            p.remove("remotePort");
            p.insert(
                "domains".into(),
                json!(["app.example.test", "*.example.test"]),
            );
        },
        None,
    )
    .unwrap();
    assert!(!switched.contains("remotePort ="));
    assert!(switched.contains("customDomains = [\"app.example.test\", \"*.example.test\"]"));
    assert!(switched.contains("metadata.owner"));
}
#[test]
fn frpc_opaque_multiline_inline_arrays_quoted_keys_and_plugin_secrets_are_retained_not_projected() {
    let raw = format!(
        "{TOML}\n[custom]\n\"key.with.dots\" = {{ options = [1, 2], password = \"synthetic-plugin\" }}\nmessage = \"\"\"first\n# not comment\n[[proxies]]\nlast\"\"\"\n[[plugins]]\nname = \"first\"\n[[plugins]]\nname = \"second\"\n"
    );
    let edited = edit_request(&raw, |v| v["serverPort"] = json!(8000), None).unwrap();
    assert_eq!(
        edited,
        raw.replace("serverPort = 7443", "serverPort = 8000")
    );
    assert!(
        !frpc_projection(&raw)
            .unwrap()
            .to_string()
            .contains("synthetic-plugin")
    );
}
#[test]
fn frpc_unsupported_and_ambiguous_toml_never_normalizes_or_commits() {
    for raw in [
        "serverAddr = \"x\"\nauth = { token = \"synthetic\" }\n",
        "serverAddr = \"x\"\ntransport.protocol = \"kcp\"\n",
        "serverAddr = \"x\"\n[[proxies]]\ntype = \"stcp\"\n",
        "serverAddr = \"x\"\nserverAddr = \"y\"\n",
        "serverAddr = \"unterminated",
        "serverAddr = \"x\"\nauth.token = \"a\"\n[auth]\ntoken = \"b\"\n",
    ] {
        assert_eq!(frpc_projection(raw).unwrap()["supported"], false);
        assert_eq!(edit_frpc(raw,br#"{"input":{"serverAddress":"x","serverPort":7000,"transport":"tcp","tls":true,"proxies":[]}}"#).unwrap_err().code,"invalid_frpc");
    }
}
#[test]
fn frpc_strict_fields_ids_ports_domains_and_unicode_escaping() {
    let edits: [fn(&mut Value); 5] = [
        |v| {
            v["serverPort"] = json!(0);
        },
        |v| {
            v["tls"] = json!("true");
        },
        |v| {
            v["unknown"] = json!(true);
        },
        |v| {
            v["proxies"][0]["sourceId"] = json!("accepted-99");
        },
        |v| {
            v["proxies"][1]["domains"] = json!(["bad_domain.test"]);
        },
    ];
    for modify in edits {
        assert_eq!(
            edit_request(TOML, modify, None).unwrap_err().code,
            "invalid_field"
        );
    }
    let control = "synthetic\"\\\n\r\t\u{0000}\u{001b}\u{0085}\u{2028}";
    let encoded = edit_request(
        TOML,
        |_| {},
        Some(json!({"mode":"replace","value":control})),
    )
    .unwrap();
    assert!(encoded.contains("\\u0000"));
    assert!(encoded.contains("\\u001B"));
    assert!(encoded.contains("\\u0085"));
    assert!(encoded.contains("\\u2028"));
    assert_eq!(frpc_projection(&encoded).unwrap()["hasToken"], true);
    let projection = frpc_projection(TOML).unwrap();
    let request = json!({"input":projection["input"]});
    let raw = serde_json::to_string(&request).unwrap();
    let duplicate = raw.replacen("\"tls\":false", "\"tls\":false,\"tls\":true", 1);
    assert_eq!(
        edit_frpc(TOML, duplicate.as_bytes()).unwrap_err().code,
        "invalid_field"
    );
}

#[test]
fn frpc_missing_fields_insert_into_existing_tables_and_new_mappings_append() {
    let raw = "serverAddr = \"saved.test\"\n[transport]\nprotocol = \"tcp\"\npoolCount = 4\n[[proxies]]\nname = \"svc\"\ntype = \"tcp\"\nlocalPort = 80\nremotePort = 8080\n";
    let edited = edit_request(
        raw,
        |v| {
            v["transport"] = json!("quic");
            v["tls"] = json!(false);
        },
        Some(json!({"mode":"replace","value":"synthetic"})),
    )
    .unwrap();
    assert!(edited.contains("poolCount = 4"));
    let projected = frpc_projection(&edited).unwrap();
    assert_eq!(projected["input"]["tls"], false);
    assert_eq!(projected["hasToken"], true);
    let added=edit_request(TOML,|v|v["proxies"].as_array_mut().unwrap().push(json!({"name":"new","type":"udp","localAddress":"127.0.0.1","localPort":53,"remotePort":1053})),None).unwrap();
    assert!(added.starts_with(TOML));
    assert_eq!(
        frpc_projection(&added).unwrap()["input"]["proxies"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        edit_request(TOML, |v| v["proxies"][0]["remotePort"] = json!(-1), None)
            .unwrap_err()
            .code,
        "invalid_field"
    );
}

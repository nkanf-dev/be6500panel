//! Synthetic filesystem and finite host loopback tests. No runtime owner is started.
use be6500_panel::auth::Auth;
use be6500_panel::policy_store::{FILE_NAME, Store};
use be6500_panel::rules_http::RulesState;
use be6500_panel::server::Service;
use serde_json::{Value, json};
use std::fs;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "be6500-rules-http-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }
    fn subscription(&self) {
        fs::write(
            self.0.join("subscription.yaml"),
            br#"proxies:
  - name: Synthetic source node
    type: vless
    server: 192.0.2.1
    port: 443
    uuid: 11111111-1111-4111-8111-111111111111
    tls: true
    udp: true
    network: tcp
    servername: example.com
    flow: xtls-rprx-vision
    client-fingerprint: chrome
    reality-opts:
      public-key: AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA
      short-id: '0123456789abcdef'
rules:
  - DOMAIN,source.example,DIRECT
  - PROCESS-NAME,synthetic-app,DIRECT
  - MATCH,DIRECT
"#,
        )
        .unwrap();
        fs::set_permissions(
            self.0.join("subscription.yaml"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
    }
    fn service(&self) -> Arc<Service> {
        Arc::new(Service::new(self.0.clone()).with_data_dir(&self.0))
    }
    fn names(&self) -> Vec<String> {
        let mut out: Vec<_> = fs::read_dir(&self.0)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_str().unwrap().to_owned())
            .collect();
        out.sort();
        out
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn exchange(service: &Arc<Service>, request: &[u8]) -> Vec<u8> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let service = Arc::clone(service);
    let server = thread::spawn(move || service.handle(listener.accept().unwrap().0).unwrap());
    let mut client = TcpStream::connect(address).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    client.write_all(request).unwrap();
    client.shutdown(Shutdown::Write).unwrap();
    let mut response = Vec::new();
    client.read_to_end(&mut response).unwrap();
    server.join().unwrap();
    response
}
fn request(method: &str, path: &str, body: &str, extra: &str) -> Vec<u8> {
    format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\n{extra}{}\r\n{body}",
        if method == "POST" {
            format!(
                "Content-Type: application/json\r\nContent-Length: {}\r\n",
                body.len()
            )
        } else {
            String::new()
        }
    )
    .into_bytes()
}
fn parts(response: &[u8]) -> (&str, &[u8]) {
    let end = response.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    (
        std::str::from_utf8(&response[..end]).unwrap(),
        &response[end..],
    )
}
fn value(response: &[u8], status: u16) -> Value {
    let (headers, body) = parts(response);
    assert!(
        headers.starts_with(&format!("HTTP/1.1 {status} ")),
        "{headers} {}",
        String::from_utf8_lossy(body)
    );
    assert!(headers.contains(&format!("Content-Length: {}\r\n", body.len())));
    serde_json::from_slice(body).unwrap()
}
fn policy() -> Value {
    json!({"rules":[{"id":"direct-example","enabled":true,"label":"Synthetic direct","note":"draft only","rule":{"kind":"domain","value":"gpt.kanglives.top","target":"direct","index":0}}],"subscriptionEdits":[]})
}
#[test]
fn constructor_and_get_never_save_or_read_runtime_artifacts() {
    let fixture = Fixture::new();
    fixture.subscription();
    // Poison runtime filenames: the slice must not open or interpret them.
    for name in [
        "manager.lock",
        "service.lock",
        "local-proxy-rules-applied.json",
        "config-1.json",
        "capture.json",
    ] {
        fs::write(fixture.0.join(name), b"synthetic-runtime-poison").unwrap();
    }
    let before = fixture.names();
    RulesState::open(&fixture.0).unwrap();
    let service = fixture.service();
    let get = value(
        &exchange(&service, &request("GET", "/api/proxy/local-rules", "", "")),
        200,
    );
    assert_eq!(
        get["draft"]["policy"],
        json!({"rules":[],"subscriptionEdits":[]})
    );
    assert_eq!(get["subscriptionRules"].as_array().unwrap().len(), 2);
    assert_eq!(get["preview"]["rules"].as_array().unwrap().len(), 2);
    assert!(
        get["preview"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["scope"] == "subscription")
    );
    assert_eq!(get["applied"], json!({"state":"unknown"}));
    assert_eq!(get["runtimeGeneration"], 0);
    let nodes = value(
        &exchange(&service, &request("GET", "/api/proxy/nodes", "", "")),
        200,
    );
    assert_eq!(nodes["nodes"].as_array().unwrap().len(), 1);
    assert_eq!(nodes["nodes"][0]["label"], "Synthetic source node");
    assert_eq!(nodes["selectedNodeId"], "");
    assert_eq!(nodes["revision"], get["subscriptionRevision"]);
    assert_eq!(fixture.names(), before);
}
#[test]
fn save_readback_reopen_revision_subscription_and_head_are_exact() {
    let fixture = Fixture::new();
    fixture.subscription();
    let source = fs::read(fixture.0.join("subscription.yaml")).unwrap();
    let service = fixture.service();
    let body = json!({"policy":policy()}).to_string();
    let saved = value(
        &exchange(
            &service,
            &request("POST", "/api/proxy/local-rules", &body, ""),
        ),
        200,
    );
    assert_eq!(saved["draft"]["policy"], policy());
    assert_eq!(
        Store::open(&fixture.0).unwrap().snapshot().revision,
        saved["draft"]["revision"]
    );
    let get_bytes = exchange(&service, &request("GET", "/api/proxy/local-rules", "", ""));
    assert_eq!(value(&get_bytes, 200), saved);
    assert_eq!(
        value(
            &exchange(
                &fixture.service(),
                &request("GET", "/api/proxy/local-rules", "", "")
            ),
            200
        ),
        saved
    );
    let head_bytes = exchange(&service, &request("HEAD", "/api/proxy/local-rules", "", ""));
    let (headers, body) = parts(&head_bytes);
    assert!(body.is_empty());
    assert!(headers.contains(&format!(
        "Content-Length: {}\r\n",
        parts(&get_bytes).1.len()
    )));
    assert_eq!(
        fs::read(fixture.0.join("subscription.yaml")).unwrap(),
        source
    );
    assert_eq!(saved["applied"]["state"], "unknown");
    assert_eq!(fixture.names(), vec![FILE_NAME, "subscription.yaml"]);
    assert_eq!(
        value(
            &exchange(&service, &request("GET", "/api/health", "", "")),
            200
        )["readOnly"],
        false
    );
}
#[test]
fn preview_never_saves_and_missing_subscription_allows_independent_draft() {
    let fixture = Fixture::new();
    let service = fixture.service();
    let initial = value(
        &exchange(&service, &request("GET", "/api/proxy/local-rules", "", "")),
        200,
    );
    let preview = value(
        &exchange(
            &service,
            &request(
                "POST",
                "/api/proxy/local-rules/preview",
                &json!({"policy":policy()}).to_string(),
                "",
            ),
        ),
        200,
    );
    assert_eq!(preview["rules"][0]["value"], "gpt.kanglives.top");
    assert!(fixture.names().is_empty());
    assert_eq!(
        value(
            &exchange(&service, &request("GET", "/api/proxy/local-rules", "", "")),
            200
        ),
        initial
    );
}
#[test]
fn strict_invalid_requests_leave_accepted_draft_unchanged() {
    let fixture = Fixture::new();
    let service = fixture.service();
    let saved = value(
        &exchange(
            &service,
            &request(
                "POST",
                "/api/proxy/local-rules",
                &json!({"policy":policy()}).to_string(),
                "",
            ),
        ),
        200,
    );
    let bytes = fs::read(fixture.0.join(FILE_NAME)).unwrap();
    for body in [
        "{}",
        "[[[],[]]]",
        "{\"policy\":[[],[]]}",
        "{\"policy\":{\"rules\":[[\"x\",true,\"\",\"\",{\"kind\":\"match\",\"target\":\"direct\",\"index\":0}]],\"subscriptionEdits\":[]}}",
        "{\"policy\":{\"rules\":[{\"id\":\"x\",\"rule\":[\"match\",\"\",\"direct\",false,0]}],\"subscriptionEdits\":[]}}",
        "{\"policy\":{\"rules\":[],\"subscriptionEdits\":[[\"x\",\"fingerprint\",true,{},\"\",\"\"]]}}",
        "{\"policy\":{\"rules\":[],\"subscriptionEdits\":[{\"id\":\"x\",\"replacement\":[\"match\",\"\",\"direct\",false,0]}]}}",
        "{\"policy\":null}",
        "{\"policy\":{}}",
        "{\"policy\":{\"rules\":[],\"subscriptionEdits\":null}}",
        "{\"policy\":{\"rules\":[],\"rules\":[],\"subscriptionEdits\":[]}}",
        "{\"policy\":{\"Rules\":[],\"subscriptionEdits\":[]}}",
        "{\"policy\":{\"rules\":[],\"subscriptionEdits\":[],\"unknown\":0}}",
        "{\"policy\":{\"rules\":[],\"subscriptionEdits\":[]},\"unknown\":0}",
        "{\"policy\":{\"rules\":[{\"id\":\"x\",\"enabled\":true,\"label\":\"\",\"note\":\"\",\"rule\":{\"kind\":\"match\",\"target\":\"direct\",\"index\":0,\"index\":1}}],\"subscriptionEdits\":[]}}",
    ] {
        let reply = exchange(
            &service,
            &request("POST", "/api/proxy/local-rules", body, ""),
        );
        assert!(
            parts(&reply).0.starts_with("HTTP/1.1 400 "),
            "{}",
            String::from_utf8_lossy(&reply)
        );
        assert_eq!(fs::read(fixture.0.join(FILE_NAME)).unwrap(), bytes);
    }
    let mut bad = policy();
    bad["rules"][0]["rule"]["value"] = json!("contains spaces");
    value(
        &exchange(
            &service,
            &request(
                "POST",
                "/api/proxy/local-rules",
                &json!({"policy":bad}).to_string(),
                "",
            ),
        ),
        422,
    );
    let oversized = format!(
        "POST /api/proxy/local-rules HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        (256 << 10) + 1
    );
    value(&exchange(&service, oversized.as_bytes()), 413);
    assert_eq!(
        value(
            &exchange(&service, &request("GET", "/api/proxy/local-rules", "", "")),
            200
        ),
        saved
    );
}
#[test]
fn authentication_origin_and_runtime_refusals_do_not_write() {
    let fixture = Fixture::new();
    let service = Arc::new(
        Service::new(fixture.0.clone())
            .with_data_dir(&fixture.0)
            .with_auth(Auth::new("synthetic-password")),
    );
    let body = json!({"policy":policy()}).to_string();
    value(
        &exchange(
            &service,
            &request("POST", "/api/proxy/local-rules", &body, ""),
        ),
        401,
    );
    value(
        &exchange(
            &service,
            &request(
                "POST",
                "/api/proxy/local-rules",
                &body,
                "Origin: http://evil.invalid\r\n",
            ),
        ),
        403,
    );
    let login = exchange(
        &service,
        &request(
            "POST",
            "/api/session/login",
            "{\"password\":\"synthetic-password\"}",
            "",
        ),
    );
    let cookie = parts(&login)
        .0
        .lines()
        .find_map(|l| l.strip_prefix("Set-Cookie: "))
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    let auth = format!("Cookie: {cookie}\r\n");
    for path in ["/api/proxy/local-rules/apply", "/api/proxy/select"] {
        assert_eq!(
            value(
                &exchange(&service, &request("POST", path, "{}", &auth)),
                503
            )["error"]["code"],
            "runtime_unavailable"
        );
    }
    value(
        &exchange(
            &service,
            &request(
                "POST",
                "/api/proxy/local-rules",
                &body,
                &format!("{auth}Sec-Fetch-Site: cross-site\r\n"),
            ),
        ),
        403,
    );
    assert!(fixture.names().is_empty());
    for path in ["/api/proxy/status", "/api/runtime", "/api/capture"] {
        value(&exchange(&service, &request("GET", path, "", &auth)), 404);
    }
}
#[test]
fn corrupt_unsafe_subscription_or_draft_is_feature_503_not_empty_reset() {
    for kind in ["yaml", "oversized", "directory", "symlink", "draft"] {
        let fixture = Fixture::new();
        match kind {
            "yaml" => {
                fs::write(fixture.0.join("subscription.yaml"), b"rules: [unterminated").unwrap()
            }
            "oversized" => fs::write(
                fixture.0.join("subscription.yaml"),
                vec![b' '; (2 << 20) + 1],
            )
            .unwrap(),
            "directory" => fs::create_dir(fixture.0.join("subscription.yaml")).unwrap(),
            "symlink" => symlink("missing-synthetic", fixture.0.join("subscription.yaml")).unwrap(),
            "draft" => fs::write(fixture.0.join(FILE_NAME), b"not a policy").unwrap(),
            _ => unreachable!(),
        }
        let before = fixture.names();
        let service = fixture.service();
        assert_eq!(
            value(
                &exchange(&service, &request("GET", "/api/proxy/local-rules", "", "")),
                503
            )["error"]["code"],
            "local_rules_unavailable"
        );
        value(
            &exchange(
                &service,
                &request(
                    "POST",
                    "/api/proxy/local-rules",
                    &json!({"policy":policy()}).to_string(),
                    "",
                ),
            ),
            503,
        );
        assert_eq!(
            value(
                &exchange(&service, &request("GET", "/api/health", "", "")),
                200
            )["readOnly"],
            true
        );
        assert_eq!(fixture.names(), before);
    }
}
#[test]
fn body_deadline_is_not_renewed_after_headers() {
    let fixture = Fixture::new();
    let service = fixture.service();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let begin = Instant::now();
        service
            .handle_with_deadlines(
                listener.accept().unwrap().0,
                Duration::from_millis(180),
                Duration::from_secs(1),
            )
            .unwrap();
        begin.elapsed()
    });
    let mut client = TcpStream::connect(address).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    client.write_all(b"POST /api/proxy/local-rules HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: 100\r\n\r\n{").unwrap();
    let mut sender = client.try_clone().unwrap();
    let trickle = thread::spawn(move || {
        for _ in 0..8 {
            thread::sleep(Duration::from_millis(60));
            if sender.write_all(b" ").is_err() {
                break;
            }
        }
    });
    let mut response = Vec::new();
    client.read_to_end(&mut response).unwrap();
    assert!(parts(&response).0.starts_with("HTTP/1.1 408 "));
    assert!(server.join().unwrap() < Duration::from_millis(450));
    trickle.join().unwrap();
    assert!(fixture.names().is_empty());
}

#[test]
fn subscription_fifo_refuses_without_waiting_for_a_writer() {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let fixture = Fixture::new();
    let path = CString::new(fixture.0.join("subscription.yaml").as_os_str().as_bytes()).unwrap();
    // SAFETY: a synthetic absolute path, terminated string, private tempdir.
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    let started = Instant::now();
    assert!(RulesState::open(&fixture.0).is_err());
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(fixture.names(), vec!["subscription.yaml"]);
}
#[test]
fn failed_store_save_preserves_accepted_draft_and_reports_not_committed() {
    let fixture = Fixture::new();
    let service = fixture.service();
    let saved = value(
        &exchange(
            &service,
            &request(
                "POST",
                "/api/proxy/local-rules",
                &json!({"policy":policy()}).to_string(),
                "",
            ),
        ),
        200,
    );
    let bytes = fs::read(fixture.0.join(FILE_NAME)).unwrap();
    fs::rename(
        fixture.0.join(FILE_NAME),
        fixture.0.join("accepted-synthetic.json"),
    )
    .unwrap();
    symlink("accepted-synthetic.json", fixture.0.join(FILE_NAME)).unwrap();
    let failure = value(
        &exchange(
            &service,
            &request(
                "POST",
                "/api/proxy/local-rules",
                r#"{"policy":{"rules":[],"subscriptionEdits":[]}}"#,
                "",
            ),
        ),
        500,
    );
    assert_eq!(failure["error"]["code"], "storage_failed");
    assert_eq!(failure["committed"], false);
    assert_eq!(failure["draft"], saved["draft"]);
    assert_eq!(
        fs::read(fixture.0.join("accepted-synthetic.json")).unwrap(),
        bytes
    );
    assert_eq!(
        value(
            &exchange(&service, &request("GET", "/api/proxy/local-rules", "", "")),
            200
        ),
        saved
    );
}

#[test]
fn content_type_allowlist_and_no_data_diagnostics_remain_honest() {
    let fixture = Fixture::new();
    let service = Arc::new(Service::new(fixture.0.clone()));
    assert_eq!(
        value(
            &exchange(&service, &request("GET", "/api/health", "", "")),
            200
        )["readOnly"],
        true
    );
    value(
        &exchange(&service, &request("GET", "/api/proxy/local-rules", "", "")),
        503,
    );
    let no_media =
        b"POST /api/proxy/local-rules HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n";
    assert_eq!(
        value(&exchange(&service, no_media), 415)["error"]["code"],
        "unsupported_media_type"
    );
    for path in [
        "/api/proxy/local-rules/preview",
        "/api/proxy/local-rules/apply",
        "/api/proxy/select",
    ] {
        value(&exchange(&service, &request("GET", path, "", "")), 405);
    }
    value(
        &exchange(&service, &request("POST", "/api/proxy/nodes", "{}", "")),
        405,
    );
    value(
        &exchange(
            &service,
            &request("POST", "/api/proxy/subscription", "{}", ""),
        ),
        405,
    );
    assert!(fixture.names().is_empty());
}
#[test]
fn subscription_is_stable_for_service_lifetime_not_reloaded_or_cloned_per_get() {
    let fixture = Fixture::new();
    fixture.subscription();
    let service = fixture.service();
    let first = value(
        &exchange(&service, &request("GET", "/api/proxy/local-rules", "", "")),
        200,
    );
    fs::write(
        fixture.0.join("subscription.yaml"),
        b"synthetic-corrupt-after-open: [",
    )
    .unwrap();
    let second = value(
        &exchange(&service, &request("GET", "/api/proxy/local-rules", "", "")),
        200,
    );
    assert_eq!(first, second);
    value(
        &exchange(
            &fixture.service(),
            &request("GET", "/api/proxy/local-rules", "", ""),
        ),
        503,
    );
}


#[test]
fn prepared_duplicate_source_preserves_preview_save_readback_and_omissions() {
    use be6500_panel::policy::{Policy, merge_effective_policy, subscription_fingerprints};
    use be6500_panel::subscription::{parse_clash_yaml, summarize_policy};

    let fixture = Fixture::new();
    let yaml = br#"rules:
  - DOMAIN,DUP.example,PROXY
  - DOMAIN,dup.example,PROXY
  - PROCESS-NAME,synthetic-omission,DIRECT
  - MATCH,DIRECT
  - DOMAIN,after.example,PROXY
"#;
    fs::write(fixture.0.join("subscription.yaml"), yaml).unwrap();
    let source = parse_clash_yaml(yaml).unwrap();
    let refs = subscription_fingerprints(&source.rules).unwrap();
    let draft = json!({
        "rules": [{
            "id": "disabled-local", "enabled": false, "label": "", "note": "",
            "rule": {"kind": "domain", "value": "local.example", "target": "direct", "index": -1}
        }],
        "subscriptionEdits": [
            {
                "id": "disable-first", "sourceFingerprint": refs[0],
                "disabled": true, "label": "", "note": ""
            },
            {
                "id": "replace-second", "sourceFingerprint": refs[1],
                "disabled": false, "label": "replacement", "note": "",
                "replacement": {
                    "kind": "domain", "value": "replacement.example", "target": "block", "index": -1
                }
            },
            {
                "id": "orphan", "sourceFingerprint": format!("{}:1", "a".repeat(64)),
                "disabled": true, "label": "", "note": ""
            }
        ]
    });
    let policy: Policy = serde_json::from_value(draft.clone()).unwrap();
    let mut expected =
        serde_json::to_value(merge_effective_policy(&source.rules, &policy).unwrap()).unwrap();
    let summary = summarize_policy(&source);
    for omission in &summary.omitted_rules {
        expected["diagnostics"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "scope": "subscription", "index": omission.index,
                "code": omission.code, "message": omission.message
            }));
    }
    let service = fixture.service();
    let body = json!({"policy": draft}).to_string();
    for _ in 0..2 {
        let preview = value(
            &exchange(
                &service,
                &request("POST", "/api/proxy/local-rules/preview", &body, ""),
            ),
            200,
        );
        assert_eq!(preview, expected);
        assert!(!fixture.0.join(FILE_NAME).exists());
    }
    // Disk changes cannot substitute a different source into an existing binding.
    fs::write(
        fixture.0.join("subscription.yaml"),
        b"rules: ['MATCH,BLOCK']",
    )
    .unwrap();
    let saved_bytes = exchange(
        &service,
        &request("POST", "/api/proxy/local-rules", &body, ""),
    );
    let saved = value(&saved_bytes, 200);
    assert_eq!(saved["preview"], expected);
    assert_eq!(saved["draft"]["policy"], draft);
    let expected_sources: Vec<_> = source
        .rules
        .iter()
        .zip(&refs)
        .map(|(rule, reference)| json!({"fingerprint": reference, "rule": rule}))
        .collect();
    assert_eq!(saved["subscriptionRules"], json!(expected_sources));
    assert_eq!(saved["subscriptionRevision"], summary.revision);
    let readback = exchange(&service, &request("GET", "/api/proxy/local-rules", "", ""));
    assert_eq!(parts(&saved_bytes).1, parts(&readback).1);
    assert_eq!(
        saved["draft"]["revision"],
        Store::open(&fixture.0).unwrap().snapshot().revision
    );
    let again = exchange(
        &service,
        &request("POST", "/api/proxy/local-rules", &body, ""),
    );
    assert_eq!(parts(&saved_bytes).1, parts(&again).1);
}

#[test]
fn authenticated_nodes_match_actual_public_parser_projection_without_private_fields() {
    use be6500_panel::subscription::parse_clash_yaml;
    let fixture = Fixture::new();
    let yaml = br#"proxies:
  - name: Synthetic public node
    type: vless
    server: 192.0.2.1
    port: 443
    uuid: 11111111-1111-4111-8111-111111111111
    tls: true
    udp: true
    network: tcp
    servername: example.com
    flow: xtls-rprx-vision
    client-fingerprint: chrome
    reality-opts:
      public-key: AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA
      short-id: '0123456789abcdef'
rules: ['MATCH,DIRECT']
"#;
    fs::write(fixture.0.join("subscription.yaml"), yaml).unwrap();
    let expected = serde_json::to_value(parse_clash_yaml(yaml).unwrap().public_nodes()).unwrap();
    let service = fixture.service();
    let response = exchange(&service, &request("GET", "/api/proxy/nodes", "", ""));
    let nodes = value(&response, 200);
    assert_eq!(nodes["nodes"], expected);
    assert_eq!(nodes["selectedNodeId"], "");
    let text = String::from_utf8(response).unwrap();
    for private in [
        "11111111-1111-4111-8111-111111111111",
        "0123456789abcdef",
        "public-key",
        "realityPublicKey",
        "uuid",
        "subscription.yaml",
    ] {
        assert!(!text.contains(private));
    }
    let rules = String::from_utf8(exchange(
        &service,
        &request("GET", "/api/proxy/local-rules", "", ""),
    ))
    .unwrap();
    assert!(!rules.contains("192.0.2.1"));
    assert!(!rules.contains("11111111-1111-4111-8111-111111111111"));
}

#![cfg(any(target_os = "linux", target_os = "macos"))]
use be6500_panel::{
    auth::Auth,
    runtime_http::RuntimeHttp,
    runtime_manager::{
        ArtifactBinding, ArtifactBindings, ArtifactProvenance, HookError, Hooks, Limits, Manager,
        ServiceId,
    },
    runtime_process::Limits as ProcessLimits,
    server::Service,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    cell::Cell,
    fs,
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::PathBuf,
    rc::Rc,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
static SERIAL: Mutex<()> = Mutex::new(());
static NEXT: AtomicU64 = AtomicU64::new(0);
const HELPER: &str = r#"#!/bin/sh
umask 077
config="$2"
case "$1" in run|check|verify) config="$3";; esac
IFS= read -r behavior < "$config" || :
case "$1" in check|verify) case "$behavior" in bad) exit 9;; *) exit 0;; esac;; esac
trap 'exit 0' TERM
printf '%s\n' "$$" > "$TMPDIR/started"
IFS= read -r value < "$TMPDIR/wait"
"#;
struct Fixture {
    root: PathBuf,
    reject: Rc<Cell<bool>>,
}
impl Fixture {
    fn new() -> Self {
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "b6p-runtime-http-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        for directory in ["artifacts", "run", "run/sing-box"] {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(root.join(directory))
                .unwrap();
        }
        fs::write(root.join("artifacts/.artifact-fake"), HELPER).unwrap();
        fs::set_permissions(
            root.join("artifacts/.artifact-fake"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        let fifo = std::ffi::CString::new(
            root.join("run/sing-box/wait")
                .as_os_str()
                .as_encoded_bytes(),
        )
        .unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        Self {
            root,
            reject: Rc::new(Cell::new(false)),
        }
    }
    fn runtime(&self) -> Guard {
        let binding = ArtifactBinding::trusted_local(
            ServiceId::SingBox,
            self.root.join("artifacts"),
            self.root.join("artifacts/.artifact-fake"),
            Sha256::digest(HELPER.as_bytes()).into(),
            ArtifactProvenance::TrustedLocalModule,
        );
        let root = self.root.clone();
        let reject = self.reject.clone();
        let hooks = Hooks::new(
            |_| Ok(()),
            move |context| {
                let pid = context.run.ok_or(HookError::Failed)?.pid();
                let marker = root.join("run/sing-box/started");
                while !fs::read_to_string(&marker)
                    .is_ok_and(|value| value.trim() == pid.to_string())
                {
                    if Instant::now() >= context.deadline {
                        return Err(HookError::Deadline);
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                Ok(())
            },
            move |_| {
                if reject.get() {
                    Err(HookError::Failed)
                } else {
                    Ok(())
                }
            },
            |_| Ok(()),
        );
        Guard {
            runtime: RuntimeHttp::new(
                Manager::open(
                    self.root.join("services"),
                    self.root.join("run"),
                    ArtifactBindings {
                        sing_box: Some(binding),
                        frpc: None,
                    },
                    hooks,
                    Limits {
                        process: ProcessLimits {
                            term_grace: Duration::from_millis(100),
                            kill_grace: Duration::from_secs(1),
                            check_timeout: Duration::from_secs(2),
                        },
                        readiness_timeout: Duration::from_secs(2),
                        resource_timeout: Duration::from_secs(1),
                    },
                )
                .unwrap(),
            ),
            reject: self.reject.clone(),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
struct Guard {
    runtime: RuntimeHttp,
    reject: Rc<Cell<bool>>,
}
impl Drop for Guard {
    fn drop(&mut self) {
        self.reject.set(false);
        self.runtime.close().expect("fake HTTP owner cleanup");
    }
}
fn request(method: &str, path: &str, body: Option<&Value>, cookie: &str, origin: &str) -> Vec<u8> {
    let body = body
        .map(|value| serde_json::to_string(value).unwrap())
        .unwrap_or_default();
    format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nCookie: {cookie}\r\n{}{}\r\n{body}",
        if method == "POST" {
            format!(
                "Content-Length: {}\r\nContent-Type: application/json\r\n",
                body.len()
            )
        } else {
            String::new()
        },
        if origin.is_empty() {
            String::new()
        } else {
            format!("Origin: {origin}\r\n")
        }
    )
    .into_bytes()
}
fn exchange(service: &Service, runtime: Option<&mut RuntimeHttp>, request: Vec<u8>) -> Vec<u8> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let client = thread::spawn(move || {
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(8)))
            .unwrap();
        stream.write_all(&request).unwrap();
        stream.shutdown(Shutdown::Write).unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).unwrap();
        response
    });
    let stream = listener.accept().unwrap().0;
    match runtime {
        Some(runtime) => service.handle_with_runtime(stream, runtime).unwrap(),
        None => service.handle(stream).unwrap(),
    };
    client.join().unwrap()
}
fn body(response: &[u8], status: u16) -> Value {
    let end = response
        .windows(4)
        .position(|bytes| bytes == b"\r\n\r\n")
        .unwrap()
        + 4;
    assert!(
        response.starts_with(format!("HTTP/1.1 {status} ").as_bytes()),
        "{}",
        String::from_utf8_lossy(response)
    );
    let headers = std::str::from_utf8(&response[..end]).unwrap();
    assert!(headers.contains(&format!("Content-Length: {}\r\n", response.len() - end)));
    serde_json::from_slice(&response[end..]).unwrap()
}
fn login(service: &Service) -> String {
    let response = exchange(
        service,
        None,
        request(
            "POST",
            "/api/session/login",
            Some(&json!({"password":"isolated-secret"})),
            "",
            "http://localhost",
        ),
    );
    body(&response, 200);
    std::str::from_utf8(&response)
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("Set-Cookie: "))
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .into()
}
fn call(
    service: &Service,
    runtime: &mut RuntimeHttp,
    path: &str,
    payload: Option<&Value>,
    cookie: &str,
    status: u16,
) -> Value {
    body(
        &exchange(
            service,
            Some(runtime),
            request(
                if payload.is_some() { "POST" } else { "GET" },
                path,
                payload,
                cookie,
                "http://localhost",
            ),
        ),
        status,
    )
}
#[test]
fn explicit_runtime_http_preserves_auth_generation_and_retained_process() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut owner = fixture.runtime();
    let service = Service::new(fixture.root.clone()).with_auth(Auth::new("isolated-secret"));
    assert!(!fixture.root.join("run/sing-box/started").exists());
    body(
        &exchange(
            &service,
            Some(&mut owner.runtime),
            request("GET", "/api/runtime", None, "", ""),
        ),
        401,
    );
    let cookie = login(&service);
    let state = call(
        &service,
        &mut owner.runtime,
        "/api/runtime",
        None,
        &cookie,
        200,
    );
    assert_eq!(state["enabled"], true);
    assert_eq!(state["services"].as_array().unwrap().len(), 2);
    assert_eq!(state["services"][0]["desired"], false);
    body(
        &exchange(
            &service,
            Some(&mut owner.runtime),
            request(
                "POST",
                "/api/runtime/configure",
                Some(&json!({"service":"sing-box","config":"good\n","generation":0})),
                &cookie,
                "http://foreign.test",
            ),
        ),
        403,
    );
    assert!(!fixture.root.join("services/sing-box/state.json").exists());
    for payload in [
        json!(["sing-box", "good\n", 0]),
        json!({"service":"sing-box", "config":"good\n", "generation":0, "extra":true}),
        json!({"service":"sing-box", "config":"good\n", "generation":null}),
        json!({"service":"sing-box", "config":"good\n", "generation":-1}),
        json!({"service":"foreign", "config":"good\n", "generation":0}),
    ] {
        let invalid = call(
            &service,
            &mut owner.runtime,
            "/api/runtime/configure",
            Some(&payload),
            &cookie,
            400,
        );
        assert_eq!(invalid["error"]["code"], "invalid_json");
        assert!(!fixture.root.join("services/sing-box/state.json").exists());
    }
    let duplicate_body = br#"{"service":"sing-box","config":"good","generation":0,"generation":0}"#;
    let duplicate = format!(
        "POST /api/runtime/configure HTTP/1.1\r\nHost: localhost\r\nCookie: {cookie}\r\nOrigin: http://localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        duplicate_body.len()
    );
    let mut duplicate = duplicate.into_bytes();
    duplicate.extend_from_slice(duplicate_body);
    let refused = body(
        &exchange(&service, Some(&mut owner.runtime), duplicate),
        400,
    );
    assert_eq!(refused["error"]["code"], "invalid_json");
    assert!(!fixture.root.join("services/sing-box/state.json").exists());
    let configured = call(
        &service,
        &mut owner.runtime,
        "/api/runtime/configure",
        Some(&json!({"service":"sing-box","config":"good\n","generation":0})),
        &cookie,
        200,
    );
    assert_eq!(configured["generation"], 1);
    assert_eq!(configured["desired"], false);
    assert!(configured.get("pid").is_none());
    let running = call(
        &service,
        &mut owner.runtime,
        "/api/runtime/start",
        Some(&json!({"service":"sing-box"})),
        &cookie,
        200,
    );
    let pid = running["pid"].as_u64().unwrap();
    assert_eq!(running["state"], "running");
    let stale = call(
        &service,
        &mut owner.runtime,
        "/api/runtime/configure",
        Some(&json!({"service":"sing-box","config":"new\n","generation":0})),
        &cookie,
        409,
    );
    assert_eq!(stale["error"]["code"], "generation_conflict");
    assert_eq!(stale["status"]["pid"], pid);
    let bad = call(
        &service,
        &mut owner.runtime,
        "/api/runtime/configure",
        Some(&json!({"service":"sing-box","config":"bad\n","generation":1})),
        &cookie,
        422,
    );
    assert_eq!(bad["error"]["code"], "config_check_failed");
    assert_eq!(bad["status"]["pid"], pid);
    fixture.reject.set(true);
    let failed = call(
        &service,
        &mut owner.runtime,
        "/api/runtime/stop",
        Some(&json!({"service":"sing-box"})),
        &cookie,
        503,
    );
    assert_eq!(failed["status"]["pid"], pid);
    assert_eq!(unsafe { libc::kill(pid as libc::pid_t, 0) }, 0);
    fixture.reject.set(false);
    let stopped = call(
        &service,
        &mut owner.runtime,
        "/api/runtime/stop",
        Some(&json!({"service":"sing-box"})),
        &cookie,
        200,
    );
    assert!(stopped.get("pid").is_none());
    let config = call(
        &service,
        &mut owner.runtime,
        "/api/runtime/config?service=sing-box",
        None,
        &cookie,
        200,
    );
    assert_eq!(config["config"], "good\n");
    assert_eq!(config["generation"], 1);
    let head = exchange(
        &service,
        Some(&mut owner.runtime),
        request("HEAD", "/api/runtime", None, &cookie, ""),
    );
    assert!(head.ends_with(b"\r\n\r\n"));
    call(
        &service,
        &mut owner.runtime,
        "/api/runtime/acquire",
        Some(&json!({"service":"sing-box"})),
        &cookie,
        503,
    );
    assert!(!format!("{:?}", owner.runtime).contains("isolated-secret"));
}
#[test]
fn runtime_without_owner_is_explicitly_unavailable() {
    let service = Service::new(std::env::temp_dir());
    let response = exchange(&service, None, request("GET", "/api/runtime", None, "", ""));
    let value = body(&response, 503);
    assert_eq!(value["error"]["code"], "runtime_unavailable");
}

#[test]
fn rule_apply_attests_exact_live_readback_and_keeps_draft_distinct() {
    use be6500_panel::native::{CompileInput, compile_native};
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let fixture = Fixture::new();
    fs::write(
        fixture.root.join("subscription.yaml"),
        r#"proxies:
  - name: Synthetic current
    type: vless
    server: node.example
    port: 443
    uuid: 00000000-0000-4000-8000-000000000001
    network: tcp
    tls: true
    udp: true
    servername: certificate.example
    flow: xtls-rprx-vision
    client-fingerprint: chrome
    reality-opts:
      public-key: AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA
      short-id: '01020304'
rules:
  - MATCH,PROXY
"#,
    )
    .unwrap();
    fs::set_permissions(
        fixture.root.join("subscription.yaml"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    fs::DirBuilder::new()
        .mode(0o700)
        .create(fixture.root.join("rules"))
        .unwrap();
    let mut refs = Vec::new();
    for (tag, kind, name) in [
        ("cn-domain", "domain", "domains.srs"),
        ("cn-ip", "ip", "ips.srs"),
    ] {
        let path = fixture.root.join("rules").join(name);
        fs::write(&path, b"synthetic binary rule set").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        refs.push(json!({"Tag":tag,"Kind":kind,"Path":path,"SHA256":format!("{:x}",Sha256::digest(b"synthetic binary rule set")),"SourceURL":"","MaxBytes":1024}));
    }
    fs::write(
        fixture.root.join("rule-sets.json"),
        serde_json::to_vec(&refs).unwrap(),
    )
    .unwrap();
    fs::set_permissions(
        fixture.root.join("rule-sets.json"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let reference: Value = serde_json::from_str(include_str!("fixtures/native-go.json")).unwrap();
    let mut input: CompileInput =
        serde_json::from_value(reference["cases"][0]["input"].clone()).unwrap();
    input.rule_sets=refs.iter().map(|value|serde_json::from_value(json!({"tag":value["Tag"],"kind":value["Kind"],"path":value["Path"],"sha256":value["SHA256"],"sourceURL":"","maxBytes":1024})).unwrap()).collect();
    let original = compile_native(&input).unwrap();
    let mut accepted: Value = serde_json::from_slice(&original.config).unwrap();
    accepted["experimental"] = json!({"clash_api":{"external_controller":"127.0.0.1:9090","secret":"synthetic-private-secret"}});
    accepted["log"]["level"] = "debug".into();
    accepted["outbounds"][0]["bind_interface"] = "wan-test".into();
    let accepted_text = serde_json::to_string(&accepted).unwrap();
    let mut owner = fixture.runtime();
    let service = Service::new(fixture.root.clone())
        .with_data_dir(&fixture.root)
        .with_auth(Auth::new("isolated-secret"));
    let cookie = login(&service);
    let first = call(
        &service,
        &mut owner.runtime,
        "/api/proxy/local-rules",
        None,
        &cookie,
        200,
    );
    assert_eq!(first["applied"]["state"], "unknown");
    let policy = json!({"rules":[{"id":"gpt-direct","enabled":true,"label":"GPT direct","note":"draft only","rule":{"kind":"domain","value":"gpt.kanglives.top","target":"direct","index":0}}],"subscriptionEdits":[]});
    let saved = call(
        &service,
        &mut owner.runtime,
        "/api/proxy/local-rules",
        Some(&json!({"policy":policy})),
        &cookie,
        200,
    );
    let revision = saved["draft"]["revision"].as_str().unwrap();
    assert_eq!(saved["applied"]["state"], "unknown");
    call(
        &service,
        &mut owner.runtime,
        "/api/runtime/configure",
        Some(&json!({"service":"sing-box","config":accepted_text,"generation":0})),
        &cookie,
        200,
    );
    let off = call(
        &service,
        &mut owner.runtime,
        "/api/proxy/local-rules/apply",
        Some(&json!({"revision":revision,"generation":1})),
        &cookie,
        409,
    );
    assert_eq!(off["error"]["code"], "runtime_not_ready");
    let old_pid = call(
        &service,
        &mut owner.runtime,
        "/api/runtime/start",
        Some(&json!({"service":"sing-box"})),
        &cookie,
        200,
    )["pid"]
        .as_u64()
        .unwrap();
    let stale = call(
        &service,
        &mut owner.runtime,
        "/api/proxy/local-rules/apply",
        Some(&json!({"revision":"changed","generation":1})),
        &cookie,
        409,
    );
    assert_eq!(stale["error"]["code"], "local_rules_revision_changed");
    let stale = call(
        &service,
        &mut owner.runtime,
        "/api/proxy/local-rules/apply",
        Some(&json!({"revision":revision,"generation":0})),
        &cookie,
        409,
    );
    assert_eq!(stale["error"]["code"], "generation_conflict");
    fixture.reject.set(true);
    let cleanup = call(
        &service,
        &mut owner.runtime,
        "/api/proxy/local-rules/apply",
        Some(&json!({"revision":revision,"generation":1})),
        &cookie,
        503,
    );
    assert_eq!(cleanup["error"]["code"], "cleanup_failed");
    assert_eq!(cleanup["status"]["pid"], old_pid);
    assert!(!fixture.root.join("local-proxy-rules-applied.json").exists());
    // Explicit retryable restart re-establishes ready resource state; Apply
    // itself must refuse while a previous recovery condition remains pending.
    fixture.reject.set(false);
    call(
        &service,
        &mut owner.runtime,
        "/api/runtime/restart",
        Some(&json!({"service":"sing-box"})),
        &cookie,
        200,
    );
    let applied = call(
        &service,
        &mut owner.runtime,
        "/api/proxy/local-rules/apply",
        Some(&json!({"revision":revision,"generation":1})),
        &cookie,
        200,
    );
    assert_eq!(applied["applied"], true);
    assert_eq!(applied["status"]["generation"], 2);
    assert_eq!(applied["draftRevision"], revision);
    let manifest: Value = serde_json::from_slice(
        &fs::read(fixture.root.join("local-proxy-rules-applied.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["nativeSHA256"], applied["configSHA256"]);
    assert_eq!(manifest["generation"], 2);
    assert!(manifest.get("nativeSha256").is_none());
    let actual = call(
        &service,
        &mut owner.runtime,
        "/api/runtime/config?service=sing-box",
        None,
        &cookie,
        200,
    );
    let text = actual["config"].as_str().unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(text.as_bytes())),
        applied["configSHA256"]
    );
    let doc: Value = serde_json::from_str(text).unwrap();
    for key in ["inbounds", "outbounds", "log", "experimental"] {
        assert_eq!(doc[key], accepted[key], "preserved {key}");
    }
    assert!(
        doc["route"]["rules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|rule| rule["domain"] == json!(["gpt.kanglives.top"])
                && rule["outbound"] == "direct")
    );
    let active = call(
        &service,
        &mut owner.runtime,
        "/api/proxy/local-rules",
        None,
        &cookie,
        200,
    );
    assert_eq!(
        active["applied"],
        json!({"state":"known","revision":revision,"generation":2})
    );
    let mut edited = policy.clone();
    edited["rules"][0]["label"] = "different draft".into();
    let saved = call(
        &service,
        &mut owner.runtime,
        "/api/proxy/local-rules",
        Some(&json!({"policy":edited})),
        &cookie,
        200,
    );
    assert_ne!(saved["draft"]["revision"], revision);
    assert_eq!(saved["applied"]["revision"], revision);
    assert_eq!(saved["runtimeGeneration"], 2);
    call(
        &service,
        &mut owner.runtime,
        "/api/runtime/stop",
        Some(&json!({"service":"sing-box"})),
        &cookie,
        200,
    );
    let stopped = call(
        &service,
        &mut owner.runtime,
        "/api/proxy/local-rules",
        None,
        &cookie,
        200,
    );
    assert_eq!(stopped["applied"]["state"], "unknown");
}

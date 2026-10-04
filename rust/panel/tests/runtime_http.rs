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
        self.runtime_mode(false)
    }
    fn saved_runtime(&self) -> Guard {
        self.runtime_mode(true)
    }
    fn runtime_mode(&self, saved: bool) -> Guard {
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
        let mut runtime = RuntimeHttp::new(
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
        );
        if saved {
            runtime.load_saved_intent(&self.root).unwrap();
        }
        Guard {
            runtime,
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

#[test]
fn saved_intent_requires_explicit_restore_and_shutdown_keeps_it() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let service = Service::new(fixture.root.clone()).with_auth(Auth::new("isolated-secret"));
    let cookie = login(&service);
    let action = json!({"service":"sing-box"});
    {
        let mut owner = fixture.saved_runtime();
        call(
            &service,
            &mut owner.runtime,
            "/api/runtime/configure",
            Some(&json!({"service":"sing-box","config":"good\n","generation":0})),
            &cookie,
            200,
        );
        call(
            &service,
            &mut owner.runtime,
            "/api/runtime/start",
            Some(&action),
            &cookie,
            200,
        );
        assert_eq!(
            serde_json::from_slice::<Value>(
                &fs::read(fixture.root.join("desired-services.json")).unwrap()
            )
            .unwrap()["sing-box"],
            true
        );
    }
    fs::remove_file(fixture.root.join("run/sing-box/started")).unwrap();
    let mut owner = fixture.saved_runtime();
    call(
        &service,
        &mut owner.runtime,
        "/api/runtime",
        None,
        &cookie,
        200,
    );
    assert!(
        owner
            .runtime
            .poll_recovery(Instant::now() + Duration::from_secs(600))
            .is_empty()
    );
    assert!(!fixture.root.join("run/sing-box/started").exists());
    let restored = owner.runtime.restore_saved();
    assert_eq!(restored.len(), 1);
    assert!(restored[0].1.as_ref().unwrap().active);
    let stopped = call(
        &service,
        &mut owner.runtime,
        "/api/runtime/stop",
        Some(&action),
        &cookie,
        200,
    );
    assert_eq!(stopped["desired"], false);
    assert!(
        owner
            .runtime
            .poll_recovery(Instant::now() + Duration::from_secs(600))
            .is_empty()
    );
    assert_eq!(
        serde_json::from_slice::<Value>(
            &fs::read(fixture.root.join("desired-services.json")).unwrap()
        )
        .unwrap()["sing-box"],
        false
    );
}
#[test]
fn off_latch_survives_persistence_failure_and_does_not_restart_retained_child() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut owner = fixture.saved_runtime();
    let service = Service::new(fixture.root.clone()).with_auth(Auth::new("isolated-secret"));
    let cookie = login(&service);
    let action = json!({"service":"sing-box"});
    call(
        &service,
        &mut owner.runtime,
        "/api/runtime/configure",
        Some(&json!({"service":"sing-box","config":"good\n","generation":0})),
        &cookie,
        200,
    );
    let live = call(
        &service,
        &mut owner.runtime,
        "/api/runtime/start",
        Some(&action),
        &cookie,
        200,
    );
    let path = fixture.root.join("desired-services.json");
    fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink("foreign-intent", &path).unwrap();
    fixture.reject.set(true);
    let refused = call(
        &service,
        &mut owner.runtime,
        "/api/runtime/stop",
        Some(&action),
        &cookie,
        503,
    );
    assert_eq!(refused["status"]["desired"], false);
    assert_eq!(refused["status"]["pid"], live["pid"]);
    assert_eq!(
        unsafe { libc::kill(live["pid"].as_u64().unwrap() as i32, 0) },
        0
    );
    fixture.reject.set(false);
    let stopped = call(
        &service,
        &mut owner.runtime,
        "/api/runtime/stop",
        Some(&action),
        &cookie,
        500,
    );
    assert_eq!(stopped["error"]["code"], "storage_failed");
    assert_eq!(stopped["status"]["intentDurabilityUncertain"], true);
    assert_eq!(stopped["status"]["desired"], false);
    assert!(stopped["status"].get("pid").is_none());
    assert!(owner.runtime.restore_saved().is_empty());
    assert!(
        owner
            .runtime
            .poll_recovery(Instant::now() + Duration::from_secs(600))
            .is_empty()
    );
    assert!(fs::symlink_metadata(path).unwrap().file_type().is_symlink());
}
#[test]
fn exit_cleanup_precedes_relaunch_and_eight_attempts_bound_crash_loop() {
    use be6500_panel::runtime_http::RecoveryResult;
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut owner = fixture.saved_runtime();
    let service = Service::new(fixture.root.clone()).with_auth(Auth::new("isolated-secret"));
    let cookie = login(&service);
    let action = json!({"service":"sing-box"});
    call(
        &service,
        &mut owner.runtime,
        "/api/runtime/configure",
        Some(&json!({"service":"sing-box","config":"good
","generation":0})),
        &cookie,
        200,
    );
    let running = call(
        &service,
        &mut owner.runtime,
        "/api/runtime/start",
        Some(&action),
        &cookie,
        200,
    );
    let mut pid = running["pid"].as_u64().unwrap();
    let mut now = Instant::now();
    for attempt in 0..8 {
        assert_eq!(unsafe { libc::kill(pid as i32, libc::SIGKILL) }, 0);
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let state = call(
                &service,
                &mut owner.runtime,
                "/api/runtime",
                None,
                &cookie,
                200,
            );
            if state["services"][0]["errorCode"] == "owned_run_exited" {
                break;
            }
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        if attempt == 0 {
            fixture.reject.set(true);
            assert_eq!(
                owner.runtime.poll_recovery(now),
                vec![(ServiceId::SingBox, RecoveryResult::Deferred)]
            );
            let held = call(
                &service,
                &mut owner.runtime,
                "/api/runtime",
                None,
                &cookie,
                200,
            );
            assert_eq!(held["services"][0]["pid"], pid);
            assert_eq!(held["services"][0]["restarts"], 0);
            fixture.reject.set(false);
            assert!(
                owner
                    .runtime
                    .poll_recovery(now + Duration::from_secs(1))
                    .is_empty()
            );
            now += Duration::from_secs(2);
            let results = owner.runtime.poll_recovery(now);
            assert_eq!(
                results,
                vec![
                    (ServiceId::SingBox, RecoveryResult::Withdrawn),
                    (ServiceId::SingBox, RecoveryResult::Started)
                ]
            );
        } else {
            let withdrawn = owner.runtime.poll_recovery(now);
            if attempt == 7 {
                assert_eq!(
                    withdrawn,
                    vec![
                        (ServiceId::SingBox, RecoveryResult::Withdrawn),
                        (ServiceId::SingBox, RecoveryResult::Exhausted)
                    ]
                );
                break;
            }
            assert_eq!(
                withdrawn,
                vec![(ServiceId::SingBox, RecoveryResult::Withdrawn)]
            );
            assert!(
                owner
                    .runtime
                    .poll_recovery(now + Duration::from_secs(1))
                    .is_empty()
            );
            now += Duration::from_secs(2);
            assert_eq!(
                owner.runtime.poll_recovery(now),
                vec![(ServiceId::SingBox, RecoveryResult::Started)]
            );
        }
        let state = call(
            &service,
            &mut owner.runtime,
            "/api/runtime",
            None,
            &cookie,
            200,
        );
        pid = state["services"][0]["pid"].as_u64().unwrap();
        assert_eq!(state["services"][0]["restarts"], attempt + 1);
        now += Duration::from_secs(1);
    }
    assert!(
        owner
            .runtime
            .poll_recovery(now + Duration::from_secs(600))
            .is_empty()
    );
    let state = call(
        &service,
        &mut owner.runtime,
        "/api/runtime",
        None,
        &cookie,
        200,
    );
    assert_eq!(state["services"][0]["restarts"], 7);
    assert_eq!(state["services"][0]["recoveryAttempts"], 8);
    assert_eq!(state["services"][0]["recoveryExhausted"], true);
    assert!(state["services"][0].get("pid").is_none());
    call(
        &service,
        &mut owner.runtime,
        "/api/runtime/stop",
        Some(&action),
        &cookie,
        200,
    );
}

#[test]
fn failed_close_cancels_recovery_but_retains_child_and_saved_intent() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut owner = fixture.saved_runtime();
    let service = Service::new(fixture.root.clone()).with_auth(Auth::new("isolated-secret"));
    let cookie = login(&service);
    let action = json!({"service":"sing-box"});
    call(
        &service,
        &mut owner.runtime,
        "/api/runtime/configure",
        Some(&json!({"service":"sing-box","config":"good
","generation":0})),
        &cookie,
        200,
    );
    let running = call(
        &service,
        &mut owner.runtime,
        "/api/runtime/start",
        Some(&action),
        &cookie,
        200,
    );
    fixture.reject.set(true);
    assert!(owner.runtime.close().is_err());
    assert!(owner.runtime.restore_saved().is_empty());
    assert!(
        owner
            .runtime
            .poll_recovery(Instant::now() + Duration::from_secs(600))
            .is_empty()
    );
    let state = call(
        &service,
        &mut owner.runtime,
        "/api/runtime",
        None,
        &cookie,
        200,
    );
    assert_eq!(state["services"][0]["pid"], running["pid"]);
    assert_eq!(state["services"][0]["restarts"], 0);
    assert_eq!(
        serde_json::from_slice::<Value>(
            &fs::read(fixture.root.join("desired-services.json")).unwrap()
        )
        .unwrap()["sing-box"],
        true
    );
    assert_eq!(
        call(
            &service,
            &mut owner.runtime,
            "/api/runtime/start",
            Some(&action),
            &cookie,
            503
        )["error"]["code"],
        "runtime_shutting_down"
    );
    fixture.reject.set(false);
    owner.runtime.close().unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(
            &fs::read(fixture.root.join("desired-services.json")).unwrap()
        )
        .unwrap()["sing-box"],
        true
    );
}

#[test]
fn rejected_intent_load_preserves_exclusive_owner_for_cleanup() {
    use be6500_panel::runtime_intent::IntentError;
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut owner = fixture.runtime();
    let service = Service::new(fixture.root.clone()).with_auth(Auth::new("isolated-secret"));
    let cookie = login(&service);
    let action = json!({"service":"sing-box"});
    let path = fixture.root.join("desired-services.json");
    fs::write(&path, br#"{"sing-box":true,"sing-box":false}"#).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        owner.runtime.load_saved_intent(&fixture.root),
        Err(IntentError::Invalid)
    );
    assert!(owner.runtime.restore_saved().is_empty());
    call(
        &service,
        &mut owner.runtime,
        "/api/runtime/configure",
        Some(&json!({"service":"sing-box","config":"good
","generation":0})),
        &cookie,
        200,
    );
    let running = call(
        &service,
        &mut owner.runtime,
        "/api/runtime/start",
        Some(&action),
        &cookie,
        200,
    );
    assert_eq!(
        owner.runtime.load_saved_intent(&fixture.root),
        Err(IntentError::Invalid)
    );
    let read = call(
        &service,
        &mut owner.runtime,
        "/api/runtime",
        None,
        &cookie,
        200,
    );
    assert_eq!(read["services"][0]["pid"], running["pid"]);
    assert_eq!(
        unsafe { libc::kill(running["pid"].as_u64().unwrap() as i32, 0) },
        0
    );
    call(
        &service,
        &mut owner.runtime,
        "/api/runtime/stop",
        Some(&action),
        &cookie,
        200,
    );
    owner.runtime.close().unwrap();
}

fn loop_request(address: std::net::SocketAddr, raw: Vec<u8>) -> Vec<u8> {
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream.write_all(&raw).unwrap();
    stream.shutdown(Shutdown::Write).unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();
    response
}
#[test]
fn owned_server_loop_borrows_one_manager_authenticates_and_cleans_on_cancel() {
    use be6500_panel::server_loop::{LoopError, serve};
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut owner = fixture.saved_runtime();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let diagnostic = Service::new(fixture.root.clone());
    assert_eq!(
        serve(&listener, &diagnostic, Some(&mut owner.runtime), &cancel),
        Err(LoopError::Authentication)
    );
    assert!(!fixture.root.join("run/sing-box/started").exists());
    let service = Service::new(fixture.root.clone()).with_auth(Auth::new("isolated-secret"));
    let stop = cancel.clone();
    let client_cancel = CancelOnDrop(stop.clone());
    let client = thread::spawn(move || {
        let _cancel = client_cancel;
        body(
            &loop_request(address, request("GET", "/api/runtime", None, "", "")),
            401,
        );
        let login = loop_request(
            address,
            request(
                "POST",
                "/api/session/login",
                Some(&json!({"password":"isolated-secret"})),
                "",
                "http://localhost",
            ),
        );
        body(&login, 200);
        let cookie = std::str::from_utf8(&login)
            .unwrap()
            .lines()
            .find_map(|line| line.strip_prefix("Set-Cookie: "))
            .unwrap()
            .split(';')
            .next()
            .unwrap();
        let health = body(
            &loop_request(address, request("GET", "/api/health", None, cookie, "")),
            200,
        );
        assert_eq!(health["mode"], "manager");
        assert_eq!(health["runtimeEnabled"], true);
        assert_eq!(health["readOnly"], false);
        let state = body(
            &loop_request(address, request("GET", "/api/runtime", None, cookie, "")),
            200,
        );
        assert!(state["services"][0].get("pid").is_none());
        assert_eq!(state["services"][0]["desired"], false);
        body(
            &loop_request(
                address,
                request(
                    "POST",
                    "/api/runtime/configure",
                    Some(&json!({"service":"sing-box","config":"good\n","generation":0})),
                    cookie,
                    "http://localhost",
                ),
            ),
            200,
        );
        let running = body(
            &loop_request(
                address,
                request(
                    "POST",
                    "/api/runtime/start",
                    Some(&json!({"service":"sing-box"})),
                    cookie,
                    "http://localhost",
                ),
            ),
            200,
        );
        assert_eq!(running["state"], "running");
        let pid = running["pid"].as_u64().unwrap();
        stop.store(true, Ordering::Release);
        pid
    });
    serve(&listener, &service, Some(&mut owner.runtime), &cancel).unwrap();
    let pid = client.join().unwrap();
    assert_eq!(unsafe { libc::kill(pid as i32, 0) }, -1);
    assert!(
        owner
            .runtime
            .poll_recovery(Instant::now() + Duration::from_secs(600))
            .is_empty()
    );
    let saved: Value =
        serde_json::from_slice(&fs::read(fixture.root.join("desired-services.json")).unwrap())
            .unwrap();
    assert_eq!(saved["sing-box"], true);
}
#[test]
fn owned_server_close_failure_returns_borrowed_live_handle_for_retry() {
    use be6500_panel::server_loop::{LoopError, serve};
    use std::sync::atomic::AtomicBool;
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut owner = fixture.saved_runtime();
    let service = Service::new(fixture.root.clone()).with_auth(Auth::new("isolated-secret"));
    let cookie = login(&service);
    call(
        &service,
        &mut owner.runtime,
        "/api/runtime/configure",
        Some(&json!({"service":"sing-box","config":"good\n","generation":0})),
        &cookie,
        200,
    );
    let running = call(
        &service,
        &mut owner.runtime,
        "/api/runtime/start",
        Some(&json!({"service":"sing-box"})),
        &cookie,
        200,
    );
    fixture.reject.set(true);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let cancel = AtomicBool::new(true);
    assert!(matches!(
        serve(&listener, &service, Some(&mut owner.runtime), &cancel),
        Err(LoopError::Shutdown(_))
    ));
    let pid = running["pid"].as_u64().unwrap();
    assert_eq!(unsafe { libc::kill(pid as i32, 0) }, 0);
    assert!(owner.runtime.restore_saved().is_empty());
    assert!(
        owner
            .runtime
            .poll_recovery(Instant::now() + Duration::from_secs(600))
            .is_empty()
    );
    let held = call(
        &service,
        &mut owner.runtime,
        "/api/runtime",
        None,
        &cookie,
        200,
    );
    assert_eq!(held["services"][0]["pid"], pid);
    fixture.reject.set(false);
    owner.runtime.close().unwrap();
    assert_eq!(unsafe { libc::kill(pid as i32, 0) }, -1);
}

#[test]
fn owned_loop_observes_exit_and_recovers_without_get_start_side_effects() {
    use be6500_panel::server_loop::serve;
    use std::sync::{Arc, atomic::AtomicBool};
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut owner = fixture.saved_runtime();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let service = Service::new(fixture.root.clone()).with_auth(Auth::new("isolated-secret"));
    let cancel = Arc::new(AtomicBool::new(false));
    let stop = cancel.clone();
    let client_failed = CancelOnDrop(stop.clone());
    let client = thread::spawn(move || {
        let _cancel = client_failed;
        let login = loop_request(
            address,
            request(
                "POST",
                "/api/session/login",
                Some(&json!({"password":"isolated-secret"})),
                "",
                "http://localhost",
            ),
        );
        body(&login, 200);
        let cookie = std::str::from_utf8(&login)
            .unwrap()
            .lines()
            .find_map(|line| line.strip_prefix("Set-Cookie: "))
            .unwrap()
            .split(';')
            .next()
            .unwrap();
        body(
            &loop_request(
                address,
                request(
                    "POST",
                    "/api/runtime/configure",
                    Some(&json!({"service":"sing-box","config":"good\n","generation":0})),
                    cookie,
                    "http://localhost",
                ),
            ),
            200,
        );
        let running = body(
            &loop_request(
                address,
                request(
                    "POST",
                    "/api/runtime/start",
                    Some(&json!({"service":"sing-box"})),
                    cookie,
                    "http://localhost",
                ),
            ),
            200,
        );
        let original = running["pid"].as_u64().unwrap();
        assert_eq!(unsafe { libc::kill(original as i32, libc::SIGKILL) }, 0);
        // Exit withdrawal/recovery is driven by the caller loop, not a
        // status GET. Polls return truthful
        // transient state and never start/check within the GET handler itself.
        let deadline = Instant::now() + Duration::from_secs(7);
        let mut replacement = None;
        while Instant::now() < deadline {
            let state = body(
                &loop_request(address, request("GET", "/api/runtime", None, cookie, "")),
                200,
            );
            let core = &state["services"][0];
            if core["state"] == "running" && core["restarts"] == 1 {
                let actual = core["pid"].as_u64().unwrap();
                assert_ne!(actual, original);
                assert_eq!(core["recoveryAttempts"], 1);
                replacement = Some(actual);
                break;
            }
            std::thread::park_timeout(Duration::from_millis(150));
        }
        let replacement = replacement.expect("fixed caller cadence must recover fake exited child");
        let stopped = body(
            &loop_request(
                address,
                request(
                    "POST",
                    "/api/runtime/stop",
                    Some(&json!({"service":"sing-box"})),
                    cookie,
                    "http://localhost",
                ),
            ),
            200,
        );
        assert_eq!(stopped["desired"], false);
        assert!(stopped.get("pid").is_none());
        stop.store(true, Ordering::Release);
        replacement
    });
    let result = serve(&listener, &service, Some(&mut owner.runtime), &cancel);
    let replacement = client.join().unwrap();
    result.unwrap();
    assert_eq!(unsafe { libc::kill(replacement as i32, 0) }, -1);
    let saved: Value =
        serde_json::from_slice(&fs::read(fixture.root.join("desired-services.json")).unwrap())
            .unwrap();
    assert_eq!(saved["sing-box"], false);
}
struct CancelOnDrop(std::sync::Arc<std::sync::atomic::AtomicBool>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

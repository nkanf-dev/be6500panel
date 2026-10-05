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
case "$1" in check|verify) if test -f "$TMPDIR/reject-check";then exit 9;fi;case "$behavior" in bad) exit 9;; *) exit 0;; esac;; esac
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
        self.runtime_bound_mode(saved, true)
    }
    fn unbound_runtime(&self) -> Guard {
        self.runtime_bound_mode(false, false)
    }
    fn runtime_bound_mode(&self, saved: bool, bound: bool) -> Guard {
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
                    sing_box: bound.then_some(binding),
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
    let reference: Value = serde_json::from_str(include_str!("fixtures/native.json")).unwrap();
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
        Some(&json!({"service":"sing-box","config":"good\n","generation":0})),
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
            now = now.max(Instant::now()) + Duration::from_secs(2);
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
            now = now.max(Instant::now()) + Duration::from_secs(2);
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
    let mut listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let diagnostic = Service::new(fixture.root.clone());
    assert_eq!(
        serve(
            &mut listener,
            &diagnostic,
            Some(&mut owner.runtime),
            &cancel
        ),
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
        assert_eq!(health["mode"], "host");
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
    serve(&mut listener, &service, Some(&mut owner.runtime), &cancel).unwrap();
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
    let mut listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let cancel = AtomicBool::new(true);
    assert!(matches!(
        serve(&mut listener, &service, Some(&mut owner.runtime), &cancel),
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
    let mut listener = TcpListener::bind("127.0.0.1:0").unwrap();
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
    let result = serve(&mut listener, &service, Some(&mut owner.runtime), &cancel);
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

fn artifact_download_fixture(raw: Vec<u8>) -> (String, thread::JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let peer = thread::spawn(move || {
        let mut stream = listener.accept().unwrap().0;
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut request = Vec::new();
        let mut bytes = [0u8; 1024];
        loop {
            let n = stream.read(&mut bytes).unwrap();
            assert!(n > 0);
            request.extend_from_slice(&bytes[..n]);
            if request.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
            assert!(request.len() <= 16384);
        }
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            raw.len()
        );
        stream.write_all(header.as_bytes()).unwrap();
        stream.write_all(&raw).unwrap();
        request
    });
    (format!("http://{address}/fake-core"), peer)
}
fn acquire_payload(url: &str, raw: &[u8], version: &str) -> Value {
    json!({"service":"sing-box","artifact":{"url":url,"sha256":format!("{:x}",Sha256::digest(raw)),"compression":"none","version":version}})
}
#[test]
fn authenticated_acquire_preserves_actual_owner_on_download_check_and_cleanup_failures() {
    use be6500_panel::artifact_source::SourcePolicy;
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut owner = fixture.saved_runtime();
    owner
        .runtime
        .load_artifact_source(
            SourcePolicy::loopback_fixture("127.0.0.1:53".parse().unwrap()).unwrap(),
            &fixture.root.join("artifacts"),
            &fixture.root.join("artifacts"),
        )
        .unwrap();
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
    let initial = call(
        &service,
        &mut owner.runtime,
        "/api/runtime/start",
        Some(&json!({"service":"sing-box"})),
        &cookie,
        200,
    );
    let pid = initial["pid"].as_u64().unwrap();
    let manifest = fs::read(fixture.root.join("services/sing-box/state.json")).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/fake-core", listener.local_addr().unwrap());
    let payload = acquire_payload(&url, HELPER.as_bytes(), "refused");
    body(
        &exchange(
            &service,
            Some(&mut owner.runtime),
            request(
                "POST",
                "/api/runtime/acquire",
                Some(&payload),
                "",
                "http://localhost",
            ),
        ),
        401,
    );
    body(
        &exchange(
            &service,
            Some(&mut owner.runtime),
            request(
                "POST",
                "/api/runtime/acquire",
                Some(&payload),
                &cookie,
                "http://foreign.test",
            ),
        ),
        403,
    );
    for invalid in [
        json!({"service":"sing-box","artifact":{"url":url}}),
        json!({"service":"sing-box","artifact":null}),
        json!({"service":"sing-box","artifact":{"url":url,"sha256":"x","compression":"none","version":"bad"}}),
        json!({"service":"sing-box","artifact":{"url":url,"sha256":"ab".repeat(32),"compression":"zip","version":"bad"}}),
        json!({"service":"sing-box","artifact":{"url":"file:///private/path","sha256":"ab".repeat(32),"compression":"none","version":"bad"}}),
    ] {
        let rejected = call(
            &service,
            &mut owner.runtime,
            "/api/runtime/acquire",
            Some(&invalid),
            &cookie,
            400,
        );
        assert!(rejected.get("error").is_some());
    }
    let mut stale = payload.clone();
    stale["generation"] = 0.into();
    assert_eq!(
        call(
            &service,
            &mut owner.runtime,
            "/api/runtime/acquire",
            Some(&stale),
            &cookie,
            409
        )["error"]["code"],
        "generation_conflict"
    );
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(
        fs::read(fixture.root.join("services/sing-box/state.json")).unwrap(),
        manifest
    );
    let (url, peer) = artifact_download_fixture(HELPER.as_bytes().to_vec());
    let mut bad_digest = acquire_payload(&url, HELPER.as_bytes(), "wrong-digest");
    bad_digest["artifact"]["sha256"] = "00".repeat(32).into();
    let failed = call(
        &service,
        &mut owner.runtime,
        "/api/runtime/acquire",
        Some(&bad_digest),
        &cookie,
        422,
    );
    assert_eq!(failed["error"]["code"], "artifact_acquire_failed");
    assert_eq!(failed["status"]["pid"], pid);
    peer.join().unwrap();
    let bad_core = b"#!/bin/sh\nexit 7\n";
    let (url, peer) = artifact_download_fixture(bad_core.to_vec());
    let rejected = call(
        &service,
        &mut owner.runtime,
        "/api/runtime/acquire",
        Some(&acquire_payload(&url, bad_core, "bad-checker")),
        &cookie,
        422,
    );
    assert_eq!(rejected["error"]["code"], "config_check_failed");
    assert_eq!(rejected["status"]["pid"], pid);
    peer.join().unwrap();
    fixture.reject.set(true);
    let (url, peer) = artifact_download_fixture(HELPER.as_bytes().to_vec());
    let failed = call(
        &service,
        &mut owner.runtime,
        "/api/runtime/acquire",
        Some(&acquire_payload(&url, HELPER.as_bytes(), "cleanup-failed")),
        &cookie,
        503,
    );
    assert_eq!(failed["error"]["code"], "cleanup_failed");
    assert_eq!(failed["status"]["pid"], pid);
    peer.join().unwrap();
    assert_eq!(
        fs::read(fixture.root.join("services/sing-box/state.json")).unwrap(),
        manifest
    );
    fixture.reject.set(false);
    call(
        &service,
        &mut owner.runtime,
        "/api/runtime/stop",
        Some(&json!({"service":"sing-box"})),
        &cookie,
        200,
    );
    call(
        &service,
        &mut owner.runtime,
        "/api/runtime/start",
        Some(&json!({"service":"sing-box"})),
        &cookie,
        200,
    );
    let (url, peer) = artifact_download_fixture(HELPER.as_bytes().to_vec());
    let acquired = call(
        &service,
        &mut owner.runtime,
        "/api/runtime/acquire",
        Some(&acquire_payload(&url, HELPER.as_bytes(), "qualified")),
        &cookie,
        200,
    );
    peer.join().unwrap();
    assert_eq!(acquired["generation"], 1);
    assert_eq!(acquired["state"], "running");
    assert_eq!(acquired["version"], "qualified");
    assert_eq!(acquired["ready"], true);
    assert_eq!(acquired["needsRecovery"], false);
    assert_eq!(acquired["desired"], true);
    assert_eq!(
        fs::read_dir(fixture.root.join("artifacts"))
            .unwrap()
            .count(),
        2
    );
}

#[test]
fn acquire_initial_and_no_config_replacement_stay_off_and_source_setup_retains_owner() {
    use be6500_panel::artifact_source::SourcePolicy;
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut owner = fixture.unbound_runtime();
    let service = Service::new(fixture.root.clone()).with_auth(Auth::new("isolated-secret"));
    let cookie = login(&service);
    let empty = call(
        &service,
        &mut owner.runtime,
        "/api/runtime",
        None,
        &cookie,
        200,
    );
    assert_eq!(empty["services"][0]["artifactAvailable"], false);
    let unavailable = call(
        &service,
        &mut owner.runtime,
        "/api/runtime/acquire",
        Some(&acquire_payload(
            "https://example.invalid/core",
            HELPER.as_bytes(),
            "not-fetched",
        )),
        &cookie,
        503,
    );
    assert_eq!(
        unavailable["error"]["code"],
        "artifact_acquisition_unavailable"
    );
    assert!(
        owner
            .runtime
            .load_artifact_source(
                SourcePolicy::loopback_fixture("127.0.0.1:53".parse().unwrap()).unwrap(),
                &fixture.root.join("missing-root"),
                &fixture.root.join("artifacts")
            )
            .is_err()
    );
    owner
        .runtime
        .load_artifact_source(
            SourcePolicy::loopback_fixture("127.0.0.1:53".parse().unwrap()).unwrap(),
            &fixture.root.join("artifacts"),
            &fixture.root.join("artifacts"),
        )
        .unwrap();
    let (url, peer) = artifact_download_fixture(HELPER.as_bytes().to_vec());
    let acquired = call(
        &service,
        &mut owner.runtime,
        "/api/runtime/acquire",
        Some(&acquire_payload(&url, HELPER.as_bytes(), "initial")),
        &cookie,
        200,
    );
    peer.join().unwrap();
    assert_eq!(acquired["state"], "notconfigured");
    assert_eq!(acquired["generation"], 0);
    assert_eq!(acquired["version"], "initial");
    assert_eq!(acquired["artifactAvailable"], true);
    assert_eq!(acquired["desired"], false);
    assert_eq!(acquired["ready"], false);
    assert!(acquired.get("pid").is_none());
    assert!(!fixture.root.join("run/sing-box/started").exists());
    let (url, peer) = artifact_download_fixture(HELPER.as_bytes().to_vec());
    let changed = call(
        &service,
        &mut owner.runtime,
        "/api/runtime/acquire",
        Some(&acquire_payload(&url, HELPER.as_bytes(), "replacement-off")),
        &cookie,
        200,
    );
    peer.join().unwrap();
    assert_eq!(changed["generation"], 0);
    assert_eq!(changed["state"], "notconfigured");
    assert_eq!(changed["version"], "replacement-off");
    assert_eq!(
        fs::read_dir(fixture.root.join("artifacts"))
            .unwrap()
            .count(),
        2
    );
    assert!(!fixture.root.join("run/sing-box/started").exists());
    call(
        &service,
        &mut owner.runtime,
        "/api/runtime/configure",
        Some(&json!({"service":"sing-box","config":"good\n","generation":0})),
        &cookie,
        200,
    );
    assert!(
        call(
            &service,
            &mut owner.runtime,
            "/api/runtime/start",
            Some(&json!({"service":"sing-box"})),
            &cookie,
            200
        )["pid"]
            .as_u64()
            .is_some()
    );
    assert!(
        owner
            .runtime
            .load_artifact_source(
                SourcePolicy::loopback_fixture("127.0.0.1:53".parse().unwrap()).unwrap(),
                &fixture.root.join("artifacts"),
                &fixture.root.join("artifacts")
            )
            .is_err()
    );
    assert_eq!(
        call(
            &service,
            &mut owner.runtime,
            "/api/runtime",
            None,
            &cookie,
            200
        )["services"][0]["state"],
        "running"
    );
}
#[test]
fn pinned_artifact_directory_replacement_and_bad_dtos_refuse_before_download() {
    use be6500_panel::artifact_source::SourcePolicy;
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut owner = fixture.unbound_runtime();
    owner
        .runtime
        .load_artifact_source(
            SourcePolicy::loopback_fixture("127.0.0.1:53".parse().unwrap()).unwrap(),
            &fixture.root.join("artifacts"),
            &fixture.root.join("artifacts"),
        )
        .unwrap();
    let service = Service::new(fixture.root.clone()).with_auth(Auth::new("isolated-secret"));
    let cookie = login(&service);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/core", listener.local_addr().unwrap());
    let good = acquire_payload(&url, HELPER.as_bytes(), "refused");
    for values in [
        json!([url, "ab".repeat(32), "none", "array"]),
        json!([url, "ab".repeat(32), "none"]),
        json!([url, "ab".repeat(32), "none", "array", true]),
    ] {
        let invalid = json!({"service":"sing-box","artifact":values});
        assert_eq!(
            call(
                &service,
                &mut owner.runtime,
                "/api/runtime/acquire",
                Some(&invalid),
                &cookie,
                400
            )["error"]["code"],
            "invalid_json"
        );
    }
    for change in 0..6 {
        let mut bad = good.clone();
        match change {
            0 => bad["generation"] = Value::Null,
            1 => bad["artifact"]["extra"] = true.into(),
            2 => {
                bad["artifact"].as_object_mut().unwrap().remove("version");
            }
            3 => bad["artifact"]["sha256"] = Value::Null,
            4 => bad["service"] = "foreign".into(),
            _ => bad["extra"] = true.into(),
        };
        assert_eq!(
            call(
                &service,
                &mut owner.runtime,
                "/api/runtime/acquire",
                Some(&bad),
                &cookie,
                400
            )["error"]["code"],
            "invalid_json"
        );
    }
    let digest = format!("{:x}", Sha256::digest(HELPER.as_bytes()));
    let duplicate_body = format!(
        r#"{{"service":"sing-box","artifact":{{"url":"{url}","sha256":"{digest}","compression":"none","version":"one","version":"two"}}}}"#
    );
    let head = format!(
        "POST /api/runtime/acquire HTTP/1.1\r\nHost: localhost\r\nCookie: {cookie}\r\nOrigin: http://localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        duplicate_body.len()
    );
    let mut request = head.into_bytes();
    request.extend_from_slice(duplicate_body.as_bytes());
    assert_eq!(
        body(&exchange(&service, Some(&mut owner.runtime), request), 400)["error"]["code"],
        "invalid_json"
    );
    let retained = fixture.root.join("artifacts-retained");
    fs::rename(fixture.root.join("artifacts"), &retained).unwrap();
    fs::DirBuilder::new()
        .mode(0o700)
        .create(fixture.root.join("artifacts"))
        .unwrap();
    assert_eq!(
        call(
            &service,
            &mut owner.runtime,
            "/api/runtime/acquire",
            Some(&good),
            &cookie,
            400
        )["error"]["code"],
        "invalid_input"
    );
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert!(!fixture.root.join("services/sing-box/state.json").exists());
}

#[test]
fn saved_on_missing_artifact_rebuilds_only_from_explicit_startup_call() {
    use be6500_panel::artifact_source::SourcePolicy;
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let service = Service::new(fixture.root.clone()).with_auth(Auth::new("isolated-secret"));
    let cookie = login(&service);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let requests = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let count = requests.clone();
    let peer = thread::spawn(move || {
        for _ in 0..2 {
            let mut stream = listener.accept().unwrap().0;
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut bytes = [0u8; 1024];
            let _ = stream.read(&mut bytes).unwrap();
            count.fetch_add(1, Ordering::Relaxed);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                HELPER.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
            stream.write_all(HELPER.as_bytes()).unwrap();
        }
    });
    {
        let mut owner = fixture.saved_runtime();
        owner
            .runtime
            .load_artifact_source(
                SourcePolicy::loopback_fixture("127.0.0.1:53".parse().unwrap()).unwrap(),
                &fixture.root.join("artifacts"),
                &fixture.root.join("artifacts"),
            )
            .unwrap();
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
            "/api/runtime/acquire",
            Some(&acquire_payload(
                &format!("http://{address}/core"),
                HELPER.as_bytes(),
                "restore-fixture",
            )),
            &cookie,
            200,
        );
        call(
            &service,
            &mut owner.runtime,
            "/api/runtime/start",
            Some(&json!({"service":"sing-box"})),
            &cookie,
            200,
        );
    }
    assert_eq!(
        fs::read_dir(fixture.root.join("artifacts"))
            .unwrap()
            .count(),
        1,
        "managed stage removed on closed owner, local fake fixture not adopted"
    );
    fs::remove_file(fixture.root.join("run/sing-box/started")).unwrap();
    let mut owner = fixture.unbound_runtime();
    owner.runtime.load_saved_intent(&fixture.root).unwrap();
    owner
        .runtime
        .load_artifact_source(
            SourcePolicy::loopback_fixture("127.0.0.1:53".parse().unwrap()).unwrap(),
            &fixture.root.join("artifacts"),
            &fixture.root.join("artifacts"),
        )
        .unwrap();
    let status = call(
        &service,
        &mut owner.runtime,
        "/api/runtime",
        None,
        &cookie,
        200,
    );
    assert_eq!(status["services"][0]["artifactAvailable"], false);
    assert_eq!(status["services"][0]["desired"], true);
    assert!(
        owner
            .runtime
            .poll_recovery(Instant::now() + Duration::from_secs(600))
            .is_empty()
    );
    assert_eq!(requests.load(Ordering::Relaxed), 1);
    assert!(!fixture.root.join("run/sing-box/started").exists());
    let restored = owner.runtime.restore_saved();
    assert_eq!(restored.len(), 1);
    assert!(restored[0].1.as_ref().unwrap().active);
    assert_eq!(requests.load(Ordering::Relaxed), 2);
    peer.join().unwrap();
    let status = call(
        &service,
        &mut owner.runtime,
        "/api/runtime",
        None,
        &cookie,
        200,
    );
    assert_eq!(status["services"][0]["version"], "restore-fixture");
    assert_eq!(status["services"][0]["generation"], 1);
    assert_eq!(status["services"][0]["state"], "running");
    call(
        &service,
        &mut owner.runtime,
        "/api/runtime/stop",
        Some(&json!({"service":"sing-box"})),
        &cookie,
        200,
    );
}
#[test]
fn saved_off_missing_artifact_never_fetches_and_saved_on_failures_retry_finitely() {
    use be6500_panel::artifact_source::SourcePolicy;
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let service = Service::new(fixture.root.clone()).with_auth(Auth::new("isolated-secret"));
    let cookie = login(&service);
    let missing = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
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
    }
    let path = fixture.root.join("services/sing-box/state.json");
    let mut metadata: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    metadata["artifact"] = json!({"url":format!("http://{missing}/core"),"sha256":format!("{:x}",Sha256::digest(HELPER.as_bytes())),"compression":"none","version":"retained"});
    fs::write(&path, serde_json::to_vec(&metadata).unwrap()).unwrap();
    let desired_path = fixture.root.join("desired-services.json");
    fs::write(&desired_path, b"{\"sing-box\":false,\"frpc\":false}").unwrap();
    fs::set_permissions(&desired_path, fs::Permissions::from_mode(0o600)).unwrap();
    {
        let mut owner = fixture.unbound_runtime();
        owner.runtime.load_saved_intent(&fixture.root).unwrap();
        owner
            .runtime
            .load_artifact_source(
                SourcePolicy::loopback_fixture("127.0.0.1:53".parse().unwrap()).unwrap(),
                &fixture.root.join("artifacts"),
                &fixture.root.join("artifacts"),
            )
            .unwrap();
        assert!(owner.runtime.restore_saved().is_empty());
        assert!(
            owner
                .runtime
                .poll_recovery(Instant::now() + Duration::from_secs(600))
                .is_empty()
        );
    }
    fs::write(&desired_path, b"{\"sing-box\":true,\"frpc\":false}").unwrap();
    let manifest = fs::read(&path).unwrap();
    let mut owner = fixture.unbound_runtime();
    owner.runtime.load_saved_intent(&fixture.root).unwrap();
    owner
        .runtime
        .load_artifact_source(
            SourcePolicy::loopback_fixture("127.0.0.1:53".parse().unwrap()).unwrap(),
            &fixture.root.join("artifacts"),
            &fixture.root.join("artifacts"),
        )
        .unwrap();
    let result = owner.runtime.restore_saved();
    assert_eq!(result.len(), 1);
    assert!(result[0].1.is_err());
    let base = Instant::now();
    for attempt in 1..=8 {
        let _ = owner
            .runtime
            .poll_recovery(base + Duration::from_secs(attempt * 61));
    }
    let status = call(
        &service,
        &mut owner.runtime,
        "/api/runtime",
        None,
        &cookie,
        200,
    );
    assert_eq!(status["services"][0]["recoveryAttempts"], 8);
    assert_eq!(status["services"][0]["recoveryExhausted"], true);
    assert_eq!(
        status["services"][0]["errorCode"],
        "artifact_acquire_failed"
    );
    assert_eq!(status["services"][0]["desired"], true);
    assert_eq!(fs::read(&path).unwrap(), manifest);
    assert!(!fixture.root.join("run/sing-box/started").exists());
    call(
        &service,
        &mut owner.runtime,
        "/api/runtime/stop",
        Some(&json!({"service":"sing-box"})),
        &cookie,
        200,
    );
    assert!(owner.runtime.restore_saved().is_empty());
    assert!(
        owner
            .runtime
            .poll_recovery(base + Duration::from_secs(10000))
            .is_empty()
    );
}

#[test]
fn saved_on_unconfigured_or_missing_source_returns_truthfully_without_download_or_config_reset() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let service = Service::new(fixture.root.clone()).with_auth(Auth::new("isolated-secret"));
    let cookie = login(&service);
    let desired = fixture.root.join("desired-services.json");
    fs::write(&desired, b"{\"sing-box\":true,\"frpc\":false}").unwrap();
    fs::set_permissions(&desired, fs::Permissions::from_mode(0o600)).unwrap();
    {
        let mut owner = fixture.unbound_runtime();
        owner.runtime.load_saved_intent(&fixture.root).unwrap();
        assert!(owner.runtime.restore_saved().is_empty());
        assert!(
            owner
                .runtime
                .poll_recovery(Instant::now() + Duration::from_secs(600))
                .is_empty()
        );
        assert!(!fixture.root.join("services/sing-box/state.json").exists());
    }
    {
        let mut owner = fixture.runtime();
        call(
            &service,
            &mut owner.runtime,
            "/api/runtime/configure",
            Some(&json!({"service":"sing-box","config":"good\n","generation":0})),
            &cookie,
            200,
        );
    }
    let metadata_path = fixture.root.join("services/sing-box/state.json");
    let mut metadata: Value = serde_json::from_slice(&fs::read(&metadata_path).unwrap()).unwrap();
    metadata["artifact"] = json!({"url":"https://example.invalid/unavailable-source","sha256":"ab".repeat(32),"compression":"none","version":"saved"});
    fs::write(&metadata_path, serde_json::to_vec(&metadata).unwrap()).unwrap();
    let previous = fs::read(&metadata_path).unwrap();
    let mut owner = fixture.unbound_runtime();
    owner.runtime.load_saved_intent(&fixture.root).unwrap();
    let failed = owner.runtime.restore_saved();
    assert_eq!(failed.len(), 1);
    assert_eq!(
        failed[0].1,
        Err(be6500_panel::runtime_http::RestoreError::SourceUnavailable)
    );
    let status = call(
        &service,
        &mut owner.runtime,
        "/api/runtime",
        None,
        &cookie,
        200,
    );
    assert_eq!(
        status["services"][0]["errorCode"],
        "artifact_acquisition_unavailable"
    );
    assert_eq!(status["services"][0]["configured"], true);
    assert_eq!(status["services"][0]["desired"], true);
    assert_eq!(status["services"][0]["needsRecovery"], true);
    assert_eq!(fs::read(&metadata_path).unwrap(), previous);
    assert!(!fixture.root.join("run/sing-box/started").exists());
    call(
        &service,
        &mut owner.runtime,
        "/api/runtime/stop",
        Some(&json!({"service":"sing-box"})),
        &cookie,
        200,
    );
}
#[test]
fn startup_rebuild_bad_digest_keeps_saved_metadata_and_accepted_bytes_then_off_cancels_retry() {
    use be6500_panel::artifact_source::{SourceError, SourcePolicy};
    use be6500_panel::artifact_stage::StageError;
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let service = Service::new(fixture.root.clone()).with_auth(Auth::new("isolated-secret"));
    let cookie = login(&service);
    {
        let mut owner = fixture.runtime();
        call(
            &service,
            &mut owner.runtime,
            "/api/runtime/configure",
            Some(&json!({"service":"sing-box","config":"good\n","generation":0})),
            &cookie,
            200,
        );
    }
    let (url, peer) = artifact_download_fixture(HELPER.as_bytes().to_vec());
    let path = fixture.root.join("services/sing-box/state.json");
    let mut metadata: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    metadata["artifact"] =
        json!({"url":url,"sha256":"00".repeat(32),"compression":"none","version":"corrupt-source"});
    fs::write(&path, serde_json::to_vec(&metadata).unwrap()).unwrap();
    let previous = fs::read(&path).unwrap();
    let desired = fixture.root.join("desired-services.json");
    fs::write(&desired, b"{\"sing-box\":true,\"frpc\":false}").unwrap();
    fs::set_permissions(&desired, fs::Permissions::from_mode(0o600)).unwrap();
    let mut owner = fixture.unbound_runtime();
    owner.runtime.load_saved_intent(&fixture.root).unwrap();
    owner
        .runtime
        .load_artifact_source(
            SourcePolicy::loopback_fixture("127.0.0.1:53".parse().unwrap()).unwrap(),
            &fixture.root.join("artifacts"),
            &fixture.root.join("artifacts"),
        )
        .unwrap();
    let failure = owner.runtime.restore_saved();
    peer.join().unwrap();
    assert_eq!(failure.len(), 1);
    assert_eq!(
        failure[0].1,
        Err(be6500_panel::runtime_http::RestoreError::Source(
            SourceError::Stage(StageError::Digest)
        ))
    );
    assert_eq!(fs::read(&path).unwrap(), previous);
    assert_eq!(
        fs::read(fixture.root.join("services/sing-box/config-1.json")).unwrap(),
        b"good\n"
    );
    assert_eq!(
        fs::read_dir(fixture.root.join("artifacts"))
            .unwrap()
            .count(),
        1
    );
    assert!(!fixture.root.join("run/sing-box/started").exists());
    call(
        &service,
        &mut owner.runtime,
        "/api/runtime/stop",
        Some(&json!({"service":"sing-box"})),
        &cookie,
        200,
    );
    assert!(
        owner
            .runtime
            .poll_recovery(Instant::now() + Duration::from_secs(1000))
            .is_empty()
    );
    assert!(owner.runtime.restore_saved().is_empty());
}

const IMPORT_YAML: &str = r#"proxies:
  - name: Import fixture node
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
  - DOMAIN,first.example,DIRECT
  - MATCH,DIRECT
"#;
#[test]
fn subscription_import_preserves_draft_and_runtime_and_orphans_changed_source_edits() {
    use be6500_panel::artifact_source::SourcePolicy;
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut owner = fixture.runtime();
    let service = Service::new(fixture.root.clone())
        .with_auth(Auth::new("isolated-secret"))
        .with_data_dir(&fixture.root);
    let cookie = login(&service);
    let policy = json!({"rules":[{"id":"local-direct","enabled":true,"label":"retained independent","note":"not a subscription edit","rule":{"kind":"domain","value":"gpt.kanglives.top","target":"direct","index":0}}],"subscriptionEdits":[]});
    let saved = call(
        &service,
        &mut owner.runtime,
        "/api/proxy/local-rules",
        Some(&json!({"policy":policy})),
        &cookie,
        200,
    );
    let draft = fs::read(fixture.root.join("local-proxy-rules.json")).unwrap();
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
    let pid = running["pid"].as_u64().unwrap();
    let accepted = fs::read(fixture.root.join("services/sing-box/state.json")).unwrap();
    let payload = json!({"content":IMPORT_YAML});
    body(
        &exchange(
            &service,
            Some(&mut owner.runtime),
            request(
                "POST",
                "/api/proxy/import",
                Some(&payload),
                "",
                "http://localhost",
            ),
        ),
        401,
    );
    body(
        &exchange(
            &service,
            Some(&mut owner.runtime),
            request(
                "POST",
                "/api/proxy/import",
                Some(&payload),
                &cookie,
                "http://foreign.test",
            ),
        ),
        403,
    );
    assert!(!fixture.root.join("subscription.yaml").exists());
    for invalid in [
        json!({}),
        json!({"url":"https://example.invalid/sub","content":IMPORT_YAML}),
        json!({"content":null}),
        json!({"content":IMPORT_YAML,"extra":true}),
        json!([IMPORT_YAML]),
        json!({"content":"rules: [broken"}),
        json!({"content":"rules:\n - MATCH,DIRECT\n"}),
    ] {
        let expected = if invalid["content"]
            .as_str()
            .is_some_and(|s| s == "rules: [broken" || s.starts_with("rules:\n"))
        {
            422
        } else {
            400
        };
        assert!(
            call(
                &service,
                &mut owner.runtime,
                "/api/proxy/import",
                Some(&invalid),
                &cookie,
                expected
            )
            .get("error")
            .is_some()
        );
        assert!(!fixture.root.join("subscription.yaml").exists());
    }
    let imported = call(
        &service,
        &mut owner.runtime,
        "/api/proxy/import",
        Some(&payload),
        &cookie,
        200,
    );
    assert_eq!(imported["nodes"].as_array().unwrap().len(), 1);
    assert_eq!(imported["selectedNodeId"], "");
    assert_eq!(
        fs::read(fixture.root.join("subscription.yaml")).unwrap(),
        IMPORT_YAML.as_bytes()
    );
    assert_eq!(
        fs::read(fixture.root.join("local-proxy-rules.json")).unwrap(),
        draft
    );
    assert_eq!(
        fs::read(fixture.root.join("services/sing-box/state.json")).unwrap(),
        accepted
    );
    assert_eq!(
        call(
            &service,
            &mut owner.runtime,
            "/api/runtime",
            None,
            &cookie,
            200
        )["services"][0]["pid"],
        pid
    );
    let read = call(
        &service,
        &mut owner.runtime,
        "/api/proxy/local-rules",
        None,
        &cookie,
        200,
    );
    assert_eq!(read["draft"]["revision"], saved["draft"]["revision"]);
    let reference = read["subscriptionRules"][0]["fingerprint"]
        .as_str()
        .unwrap();
    let mut edited = policy.clone();
    edited["subscriptionEdits"] = json!([{"id":"override-first","sourceFingerprint":reference,"disabled":true,"label":"changed source may orphan","note":""}]);
    call(
        &service,
        &mut owner.runtime,
        "/api/proxy/local-rules",
        Some(&json!({"policy":edited})),
        &cookie,
        200,
    );
    let retained = fs::read(fixture.root.join("local-proxy-rules.json")).unwrap();
    let changed = IMPORT_YAML.replace("first.example", "second.example");
    owner
        .runtime
        .load_artifact_source(
            SourcePolicy::loopback_fixture("127.0.0.1:53".parse().unwrap()).unwrap(),
            &fixture.root.join("artifacts"),
            &fixture.root.join("artifacts"),
        )
        .unwrap();
    let (url, peer) = artifact_download_fixture(changed.as_bytes().to_vec());
    let imported = call(
        &service,
        &mut owner.runtime,
        "/api/proxy/import",
        Some(&json!({"url":url})),
        &cookie,
        200,
    );
    peer.join().unwrap();
    assert_eq!(
        fs::read(fixture.root.join("subscription.yaml")).unwrap(),
        changed.as_bytes()
    );
    assert_ne!(imported["revision"], read["subscriptionRevision"]);
    assert_eq!(
        fs::read(fixture.root.join("local-proxy-rules.json")).unwrap(),
        retained
    );
    let changed_read = call(
        &service,
        &mut owner.runtime,
        "/api/proxy/local-rules",
        None,
        &cookie,
        200,
    );
    assert!(
        changed_read["preview"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "orphaned-edit")
    );
    assert_eq!(changed_read["applied"]["state"], "unknown");
    assert_eq!(
        fs::read(fixture.root.join("services/sing-box/state.json")).unwrap(),
        accepted
    );
}

struct SelectionLanObserver;
impl be6500_panel::readiness_tun::Observer for SelectionLanObserver {
    fn read_file(
        &mut self,
        _: &std::path::Path,
        _: usize,
        _: &be6500_panel::readiness_tun::Budget<'_>,
    ) -> Result<Vec<u8>, be6500_panel::readiness_tun::TunError> {
        Err(be6500_panel::readiness_tun::TunError::Unavailable)
    }
    fn read_link(
        &mut self,
        _: &std::path::Path,
        _: &be6500_panel::readiness_tun::Budget<'_>,
    ) -> Result<PathBuf, be6500_panel::readiness_tun::TunError> {
        Err(be6500_panel::readiness_tun::TunError::Unavailable)
    }
    fn metadata(
        &mut self,
        _: &std::path::Path,
        _: bool,
        _: &be6500_panel::readiness_tun::Budget<'_>,
    ) -> Result<be6500_panel::readiness_tun::FileIdentity, be6500_panel::readiness_tun::TunError>
    {
        Err(be6500_panel::readiness_tun::TunError::Unavailable)
    }
    fn list_dir(
        &mut self,
        _: &std::path::Path,
        _: usize,
        _: &be6500_panel::readiness_tun::Budget<'_>,
    ) -> Result<Vec<String>, be6500_panel::readiness_tun::TunError> {
        Err(be6500_panel::readiness_tun::TunError::Unavailable)
    }
    fn interfaces(
        &mut self,
        b: &be6500_panel::readiness_tun::Budget<'_>,
    ) -> Result<Vec<be6500_panel::readiness_tun::Interface>, be6500_panel::readiness_tun::TunError>
    {
        b.check()?;
        Ok(vec![be6500_panel::readiness_tun::Interface {
            name: "br-lan".into(),
            up: true,
            mtu: 1500,
            addresses: vec![be6500_panel::readiness_tun::InterfaceAddress {
                address: "192.168.50.1".parse().unwrap(),
                bits: 24,
            }],
        }])
    }
    fn ipv4_routes(
        &mut self,
        b: &be6500_panel::readiness_tun::Budget<'_>,
    ) -> Result<Vec<be6500_panel::readiness_tun::Ipv4Prefix>, be6500_panel::readiness_tun::TunError>
    {
        b.check()?;
        Ok(vec![])
    }
}
fn load_selection_sources(fixture: &Fixture, runtime: &mut RuntimeHttp) {
    use be6500_panel::{
        artifact_source::SourcePolicy,
        capture_executor::{Binaries, TrustedBinary},
        capture_kernel::table_names,
        capture_runtime::CaptureRuntime,
        capture_state::Controller,
        native_runtime::NativeReadiness,
    };
    for part in ["capture", "selection-exec"] {
        fs::DirBuilder::new()
            .mode(0o700)
            .create(fixture.root.join(part))
            .unwrap();
    }
    let command = fixture.root.join("artifacts/.artifact-fake");
    let hash = Sha256::digest(HELPER.as_bytes()).into();
    let capture = CaptureRuntime::new(
        Controller::open(fixture.root.join("capture")).unwrap(),
        Binaries {
            ip: TrustedBinary::admit(&command, hash).unwrap(),
            iptables: TrustedBinary::admit(&command, hash).unwrap(),
        },
        fixture.root.join("selection-exec"),
        table_names(b"").unwrap(),
        |_, _, _| panic!("selection must not Apply capture"),
    );
    let (_, handle) = capture.into_hooks_with_handle(NativeReadiness::with_observer(
        SelectionLanObserver,
        Rc::new(std::sync::atomic::AtomicBool::new(false)),
    ));
    runtime.load_capture(handle).unwrap();
    runtime
        .load_artifact_source(
            SourcePolicy::loopback_fixture("127.0.0.1:53".parse().unwrap()).unwrap(),
            &fixture.root.join("artifacts"),
            &fixture.root.join("artifacts"),
        )
        .unwrap();
}
fn stage_selection_refs(fixture: &Fixture) {
    let mut refs = Vec::new();
    for (tag, kind) in [("cn-domain", "domain"), ("cn-ip", "ip")] {
        let path = fixture.root.join(format!("{tag}.srs"));
        let bytes = format!("synthetic-{tag}").into_bytes();
        fs::write(&path, &bytes).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        refs.push(json!({"Tag":tag,"Kind":kind,"Path":path,"SHA256":format!("{:x}",Sha256::digest(&bytes)),"SourceURL":"https://example.invalid/fixture","MaxBytes":4096}));
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
}
#[test]
fn node_selection_first_current_lan_off_config_and_new_subscription_preserves_settings() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut owner = fixture.runtime();
    load_selection_sources(&fixture, &mut owner.runtime);
    stage_selection_refs(&fixture);
    let service = Service::new(fixture.root.clone())
        .with_auth(Auth::new("isolated-secret"))
        .with_data_dir(&fixture.root);
    let cookie = login(&service);
    let imported = call(
        &service,
        &mut owner.runtime,
        "/api/proxy/import",
        Some(&json!({"content":IMPORT_YAML})),
        &cookie,
        200,
    );
    let id = imported["nodes"][0]["id"].as_str().unwrap();
    let payload = json!({"nodeId":id,"ipv6":"direct","failure":"direct","ports":{"mixed":2080,"tproxy":7893,"dns":6450},"datapath":"routed-tun","routedTUN":{"interfaceName":"b6p-tun","address":"172.31.255.253/30"},"generation":0});
    body(
        &exchange(
            &service,
            Some(&mut owner.runtime),
            request(
                "POST",
                "/api/proxy/select",
                Some(&payload),
                "",
                "http://localhost",
            ),
        ),
        401,
    );
    body(
        &exchange(
            &service,
            Some(&mut owner.runtime),
            request(
                "POST",
                "/api/proxy/select",
                Some(&payload),
                &cookie,
                "http://foreign.test",
            ),
        ),
        403,
    );
    for invalid in [
        json!([id]),
        json!({"nodeId":id,"ipv6":"direct","failure":"direct","ports":[2080,7893,6450]}),
        json!({"nodeId":id,"ipv6":"direct","failure":"direct","ports":{"mixed":2080,"tproxy":7893,"dns":6450},"routedTUN":null}),
        json!({"nodeId":id,"ipv6":"direct","failure":"direct","ports":{"mixed":2080,"tproxy":7893,"dns":6450},"generation":null}),
    ] {
        call(
            &service,
            &mut owner.runtime,
            "/api/proxy/select",
            Some(&invalid),
            &cookie,
            400,
        );
    }
    let first = call(
        &service,
        &mut owner.runtime,
        "/api/proxy/select",
        Some(&payload),
        &cookie,
        200,
    );
    assert_eq!(first["status"]["generation"], 1);
    assert_eq!(first["status"]["desired"], false);
    assert_eq!(first["applied"], false);
    assert!(!fixture.root.join("run/sing-box/started").exists());
    assert!(!fixture.root.join("local-proxy-rules-applied.json").exists());
    let config = call(
        &service,
        &mut owner.runtime,
        "/api/runtime/config?service=sing-box",
        None,
        &cookie,
        200,
    );
    let mut accepted: Value = serde_json::from_str(config["config"].as_str().unwrap()).unwrap();
    for tag in ["mixed-in", "dns-in"] {
        assert_eq!(
            accepted["inbounds"]
                .as_array()
                .unwrap()
                .iter()
                .find(|inbound| inbound["tag"] == tag)
                .unwrap()["listen"],
            "192.168.50.1"
        );
    }
    assert_eq!(
        call(
            &service,
            &mut owner.runtime,
            "/api/proxy/nodes",
            None,
            &cookie,
            200
        )["selectedNodeId"],
        id
    );
    accepted["log"]["level"] = "debug".into();
    accepted["experimental"] =
        json!({"clash_api":{"secret":"synthetic-private","external_controller":"127.0.0.1:9090"}});
    accepted["dns"]["cache_capacity"] = 1234.into();
    accepted["outbounds"][0]["bind_interface"] = "wan0".into();
    call(
        &service,
        &mut owner.runtime,
        "/api/runtime/configure",
        Some(&json!({"service":"sing-box","config":accepted.to_string(),"generation":1})),
        &cookie,
        200,
    );
    let changed = IMPORT_YAML.replace("192.0.2.1", "192.0.2.2").replace(
        "11111111-1111-4111-8111-111111111111",
        "22222222-2222-4222-8222-222222222222",
    );
    let newsource = call(
        &service,
        &mut owner.runtime,
        "/api/proxy/import",
        Some(&json!({"content":changed})),
        &cookie,
        200,
    );
    let newid = newsource["nodes"][0]["id"].as_str().unwrap();
    assert_ne!(newid, id);
    assert_eq!(
        call(
            &service,
            &mut owner.runtime,
            "/api/proxy/nodes",
            None,
            &cookie,
            200
        )["selectedNodeId"],
        ""
    );
    let mut next = payload.clone();
    next["nodeId"] = newid.into();
    next["generation"] = 2.into();
    let selected = call(
        &service,
        &mut owner.runtime,
        "/api/proxy/select",
        Some(&next),
        &cookie,
        200,
    );
    assert_eq!(selected["status"]["generation"], 3);
    assert_eq!(selected["applied"], false);
    let config = call(
        &service,
        &mut owner.runtime,
        "/api/runtime/config?service=sing-box",
        None,
        &cookie,
        200,
    );
    let now: Value = serde_json::from_str(config["config"].as_str().unwrap()).unwrap();
    assert_eq!(now["log"], accepted["log"]);
    assert_eq!(now["experimental"], accepted["experimental"]);
    assert_eq!(now["dns"]["cache_capacity"], 1234);
    assert_eq!(now["outbounds"][0]["bind_interface"], "wan0");
    assert_eq!(
        now["outbounds"]
            .as_array()
            .unwrap()
            .iter()
            .find(|out| out["tag"] == "proxy")
            .unwrap()["server"],
        "192.0.2.2"
    );
    assert_eq!(
        call(
            &service,
            &mut owner.runtime,
            "/api/proxy/nodes",
            None,
            &cookie,
            200
        )["selectedNodeId"],
        newid
    );
    let stale = call(
        &service,
        &mut owner.runtime,
        "/api/proxy/select",
        Some(&next),
        &cookie,
        409,
    );
    assert_eq!(stale["error"]["code"], "generation_conflict");
}

#[test]
fn node_selection_running_checker_cleanup_readback_and_uncertain_evidence_are_truthful() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut owner = fixture.runtime();
    load_selection_sources(&fixture, &mut owner.runtime);
    stage_selection_refs(&fixture);
    let service = Service::new(fixture.root.clone())
        .with_auth(Auth::new("isolated-secret"))
        .with_data_dir(&fixture.root);
    let cookie = login(&service);
    let imported = call(
        &service,
        &mut owner.runtime,
        "/api/proxy/import",
        Some(&json!({"content":IMPORT_YAML})),
        &cookie,
        200,
    );
    let id = imported["nodes"][0]["id"].as_str().unwrap();
    let payload = json!({"nodeId":id,"ipv6":"direct","failure":"direct","ports":{"mixed":2080,"tproxy":7893,"dns":6450},"generation":0});
    call(
        &service,
        &mut owner.runtime,
        "/api/proxy/select",
        Some(&payload),
        &cookie,
        200,
    );
    let started = call(
        &service,
        &mut owner.runtime,
        "/api/runtime/start",
        Some(&json!({"service":"sing-box"})),
        &cookie,
        200,
    );
    let pid = started["pid"].as_u64().unwrap();
    let state = fs::read(fixture.root.join("services/sing-box/state.json")).unwrap();
    let mut next = payload.clone();
    next["generation"] = 1.into();
    next["ports"]["mixed"] = 2081.into();
    fs::write(fixture.root.join("run/sing-box/reject-check"), b"synthetic").unwrap();
    let failed = call(
        &service,
        &mut owner.runtime,
        "/api/proxy/select",
        Some(&next),
        &cookie,
        422,
    );
    assert_eq!(failed["error"]["code"], "config_check_failed");
    assert_eq!(failed["status"]["pid"], pid);
    assert_eq!(
        fs::read(fixture.root.join("services/sing-box/state.json")).unwrap(),
        state
    );
    fs::remove_file(fixture.root.join("run/sing-box/reject-check")).unwrap();
    fixture.reject.set(true);
    let failed = call(
        &service,
        &mut owner.runtime,
        "/api/proxy/select",
        Some(&next),
        &cookie,
        503,
    );
    assert_eq!(failed["status"]["pid"], pid);
    assert_eq!(
        fs::read(fixture.root.join("services/sing-box/state.json")).unwrap(),
        state
    );
    fixture.reject.set(false);
    // A failed safety withdrawal intentionally needs explicit same-owner recovery.
    call(
        &service,
        &mut owner.runtime,
        "/api/runtime/restart",
        Some(&json!({"service":"sing-box"})),
        &cookie,
        200,
    );
    let changed = call(
        &service,
        &mut owner.runtime,
        "/api/proxy/select",
        Some(&next),
        &cookie,
        200,
    );
    assert_eq!(changed["status"]["generation"], 2);
    assert_eq!(changed["applied"], true);
    assert!(fixture.root.join("local-proxy-rules-applied.json").exists());
    assert_eq!(
        call(
            &service,
            &mut owner.runtime,
            "/api/proxy/local-rules",
            None,
            &cookie,
            200
        )["applied"]["state"],
        "known"
    );
    // Foreign evidence inode is never repaired or overwritten after runtime commit.
    fs::rename(
        fixture.root.join("proxy-selection.json"),
        fixture.root.join("old-selection"),
    )
    .unwrap();
    fs::write(
        fixture.root.join("proxy-selection.json"),
        b"foreign-private",
    )
    .unwrap();
    fs::set_permissions(
        fixture.root.join("proxy-selection.json"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    next["generation"] = 2.into();
    next["ports"]["mixed"] = 2082.into();
    let uncertain = call(
        &service,
        &mut owner.runtime,
        "/api/proxy/select",
        Some(&next),
        &cookie,
        500,
    );
    assert_eq!(uncertain["committed"], true);
    assert_eq!(uncertain["selection"]["status"]["generation"], 3);
    assert_eq!(uncertain["selection"]["applied"], false);
    assert_eq!(
        fs::read(fixture.root.join("proxy-selection.json")).unwrap(),
        b"foreign-private"
    );
    assert_eq!(
        call(
            &service,
            &mut owner.runtime,
            "/api/proxy/nodes",
            None,
            &cookie,
            200
        )["selectedNodeId"],
        ""
    );
}
#[test]
fn node_selection_source_replacement_missing_refs_and_omission_ack_refuse_before_runtime() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut owner = fixture.runtime();
    load_selection_sources(&fixture, &mut owner.runtime);
    let service = Service::new(fixture.root.clone())
        .with_auth(Auth::new("isolated-secret"))
        .with_data_dir(&fixture.root);
    let cookie = login(&service);
    let imported = call(
        &service,
        &mut owner.runtime,
        "/api/proxy/import",
        Some(&json!({"content":IMPORT_YAML})),
        &cookie,
        200,
    );
    let id = imported["nodes"][0]["id"].as_str().unwrap();
    let mut payload = json!({"nodeId":id,"ipv6":"direct","failure":"direct","ports":{"mixed":2080,"tproxy":7893,"dns":6450},"generation":0});
    assert_eq!(
        call(
            &service,
            &mut owner.runtime,
            "/api/proxy/select",
            Some(&payload),
            &cookie,
            409
        )["error"]["code"],
        "rules_unavailable"
    );
    assert!(!fixture.root.join("services/sing-box/state.json").exists());
    stage_selection_refs(&fixture);
    let omitted = IMPORT_YAML.replace("rules:", "rules:\n  - GEOIP,US,PROXY");
    let imported = call(
        &service,
        &mut owner.runtime,
        "/api/proxy/import",
        Some(&json!({"content":omitted})),
        &cookie,
        200,
    );
    assert_eq!(
        call(
            &service,
            &mut owner.runtime,
            "/api/proxy/select",
            Some(&payload),
            &cookie,
            409
        )["error"]["code"],
        "policy_acknowledgment_required"
    );
    payload["acknowledgedRevision"] = "0".repeat(64).into();
    assert_eq!(
        call(
            &service,
            &mut owner.runtime,
            "/api/proxy/select",
            Some(&payload),
            &cookie,
            409
        )["error"]["code"],
        "policy_revision_changed"
    );
    payload["acknowledgedRevision"] = imported["revision"].clone();
    fs::rename(
        fixture.root.join("subscription.yaml"),
        fixture.root.join("old-source"),
    )
    .unwrap();
    fs::write(fixture.root.join("subscription.yaml"), IMPORT_YAML).unwrap();
    fs::set_permissions(
        fixture.root.join("subscription.yaml"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    assert_eq!(
        call(
            &service,
            &mut owner.runtime,
            "/api/proxy/select",
            Some(&payload),
            &cookie,
            409
        )["error"]["code"],
        "subscription_source_changed"
    );
    assert!(!fixture.root.join("services/sing-box/state.json").exists());
}

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

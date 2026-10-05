use std::process::Command;

#[test]
fn cli_rejects_bad_unknown_duplicate_and_nonloopback_arguments_before_listening() {
    for args in [
        vec!["--listen", "0.0.0.0:8790"],
        vec!["--listen", "192.168.1.1:8790"],
        vec!["--listen", "[::]:8790"],
        vec!["--listen", "localhost:8790"],
        vec!["--listen", "bad"],
        vec!["--listen"],
        vec!["--listen", "127.0.0.1:0", "--listen", "127.0.0.1:0"],
        vec!["--proc-root", "/proc", "--proc-root", "/proc"],
        vec!["--web-dir", "/tmp", "--web-dir", "/tmp"],
        vec!["--data-dir"],
        vec!["--data-dir", "--listen"],
        vec!["--data-dir", "/tmp", "--data-dir", "/tmp"],
        vec!["--unknown", "private-path"],
        vec!["--web-dir", "/path/that/does/not/exist"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_be6500-panel"))
            .args(&args)
            .output()
            .unwrap();
        assert!(!output.status.success(), "accepted {args:?}");
        assert!(output.stdout.is_empty());
        let stderr = std::str::from_utf8(&output.stderr).unwrap();
        assert!(!stderr.contains("private-path"));
        assert!(!stderr.contains("/path/that/does/not/exist"));
    }
}

#[test]
fn cli_help_is_explicit_diagnostic_slice() {
    let output = Command::new(env!("CARGO_BIN_EXE_be6500-panel"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = std::str::from_utf8(&output.stdout).unwrap();
    assert!(help.contains("loopback-only"));
    assert!(help.contains("--proc-root"));
    assert!(help.contains("--web-dir"));
    assert!(help.contains("--data-dir"));
    assert!(help.contains("never Apply"));
}

#[test]
fn cli_password_flag_is_not_accepted_or_echoed() {
    let output = Command::new(env!("CARGO_BIN_EXE_be6500-panel"))
        .args(["--password", "private-secret"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        !String::from_utf8(output.stderr)
            .unwrap()
            .contains("private-secret")
    );
}

#[cfg(unix)]
#[test]
fn cli_invalid_secret_environment_is_fixed_before_listening() {
    use std::os::unix::ffi::OsStringExt;
    let output = Command::new(env!("CARGO_BIN_EXE_be6500-panel"))
        .args(["--listen", "127.0.0.1:0"])
        .env(
            "BE6500PANEL_PASSWORD",
            std::ffi::OsString::from_vec(vec![0xff]),
        )
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(
        output.stderr,
        b"be6500-panel: invalid password environment\n"
    );
}

#[test]
fn cli_data_dir_opens_drafts_without_startup_save_or_runtime_owner() {
    use std::fs;
    use std::io::{Read, Write};
    use std::net::{Shutdown, TcpListener, TcpStream};
    use std::time::{Duration, Instant};
    let path = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!("be6500-cli-draft-{}", std::process::id()));
    fs::create_dir(&path).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let probe = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = probe.local_addr().unwrap();
    drop(probe);
    let mut child = Command::new(env!("CARGO_BIN_EXE_be6500-panel"))
        .args([
            "--listen",
            &address.to_string(),
            "--data-dir",
            path.to_str().unwrap(),
        ])
        .env_remove("BE6500PANEL_PASSWORD")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    // Finite test readiness uses actual child exit information, not a marker
    // that silently treats early failure and a slow native spawn as identical.
    let mut early_exit = None;
    let connection = loop {
        if let Some(status) = child.try_wait().unwrap() {
            early_exit = Some(status);
            break None;
        }
        match TcpStream::connect(address) {
            Ok(client) => break Some(client),
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => break None,
        }
    };
    let mut response = Vec::new();
    if let Some(mut client) = connection {
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        client
            .write_all(b"GET /api/proxy/local-rules HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .unwrap();
        client.shutdown(Shutdown::Write).unwrap();
        client.read_to_end(&mut response).unwrap();
    }
    if early_exit.is_none() {
        child.kill().unwrap();
    }
    let exit = child.wait().unwrap();
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    let names: Vec<_> = fs::read_dir(&path)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    fs::remove_dir_all(&path).unwrap();
    assert!(
        response.starts_with(b"HTTP/1.1 200 "),
        "CLI did not serve drafts: early_exit={early_exit:?}, exit={exit:?}, response={} bytes, fixed stderr={stderr}",
        response.len()
    );
    assert!(
        names.is_empty(),
        "startup created a draft/runtime file: {names:?}"
    );
}

#[test]
fn cli_native_mode_requires_complete_explicit_options_and_authentication() {
    for args in [
        vec!["--native-runtime"],
        vec!["--native-runtime", "--data-dir", "/private-test-data"],
        vec!["--run-dir", "/private-test-run"],
        vec!["--command-manifest", "/private-test-manifest"],
        vec!["--native-runtime", "--native-runtime"],
        vec![
            "--native-runtime",
            "--data-dir",
            "/private-test-data",
            "--run-dir",
            "/private-test-run",
            "--command-manifest",
            "/private-test-manifest",
            "--proc-root",
            "/synthetic-proc",
        ],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_be6500-panel"))
            .args(&args)
            .env_remove("BE6500PANEL_PASSWORD")
            .output()
            .unwrap();
        assert!(!output.status.success());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(!error.contains("private-test") && !error.contains("synthetic-proc"));
    }
    let output = Command::new(env!("CARGO_BIN_EXE_be6500-panel"))
        .args([
            "--native-runtime",
            "--data-dir",
            "/private-test-data",
            "--run-dir",
            "/private-test-run",
            "--command-manifest",
            "/private-test-manifest",
        ])
        .env_remove("BE6500PANEL_PASSWORD")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(
        output.stderr,
        b"be6500-panel: native mode requires authentication\n"
    );
}

#[cfg(unix)]
mod native_binding_tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::{
        fs,
        io::{Read, Write},
        net::{Shutdown, SocketAddr, TcpListener, TcpStream},
        os::unix::fs::{DirBuilderExt, PermissionsExt},
        path::PathBuf,
        process::{Child, Stdio},
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, Instant},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    const COMMAND: &str = r#"#!/bin/sh
umask 077
printf '%s\n' "$*" >> "$TMPDIR/commands"
IFS= read -r mode < "$TMPDIR/mode" || :
case "$mode" in fail) exit 8;; esac
exit 0
"#;
    struct Fixture {
        root: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let root = fs::canonicalize(std::env::temp_dir())
                .unwrap()
                .join(format!(
                    "b6p-native-cli-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
            fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
            for part in ["data", "run"] {
                fs::DirBuilder::new()
                    .mode(0o700)
                    .create(root.join(part))
                    .unwrap();
            }
            let path = root.join("fake-network");
            fs::write(&path, COMMAND).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            let hash = format!("{:x}", Sha256::digest(COMMAND.as_bytes()));
            let manifest = serde_json::json!({"ip":{"path":path,"sha256":hash},"iptables":{"path":path,"sha256":hash},"dnsBootstrap":"127.0.0.1:53"});
            fs::write(root.join("bindings.json"), manifest.to_string()).unwrap();
            fs::set_permissions(
                root.join("bindings.json"),
                fs::Permissions::from_mode(0o600),
            )
            .unwrap();
            Self { root }
        }
        fn args(&self, address: SocketAddr) -> Vec<String> {
            vec![
                "--native-runtime".into(),
                "--listen".into(),
                address.to_string(),
                "--data-dir".into(),
                self.root.join("data").to_str().unwrap().into(),
                "--run-dir".into(),
                self.root.join("run").to_str().unwrap().into(),
                "--command-manifest".into(),
                self.root.join("bindings.json").to_str().unwrap().into(),
            ]
        }
        fn spawn(&self, address: SocketAddr) -> Running {
            let child = Command::new(env!("CARGO_BIN_EXE_be6500-panel"))
                .args(self.args(address))
                .env("BE6500PANEL_PASSWORD", "native-cli-secret")
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            Running(child)
        }
        fn journal(&self) {
            use be6500_panel::{
                capture_plan::RulesPlanInput,
                capture_state::{CommandResult, Controller, Desired},
                native::Ports,
            };
            let mut c = Controller::open(self.root.join("data")).unwrap();
            c.set_desired(Desired {
                scope: "gateway".into(),
                lan_ipv4_prefixes: vec!["192.168.50.0/24".into()],
                desired: true,
                ..Desired::default()
            })
            .unwrap();
            c.apply(
                RulesPlanInput {
                    scope: "gateway".into(),
                    lan_ipv4_prefixes: vec!["192.168.50.0/24".into()],
                    datapath: "routed-tun".into(),
                    tun_interface: "b6p-test".into(),
                    tun_address: "172.30.0.1/30".into(),
                    lan_interface: "br-lan".into(),
                    ports: Ports {
                        mixed: 2080,
                        tproxy: 7893,
                        dns: 1053,
                    },
                    ipv6: "direct".into(),
                    failure: "direct".into(),
                    ..RulesPlanInput::default()
                },
                |_, _, _| Ok(()),
                |_, _| Ok(CommandResult::success()),
            )
            .unwrap();
            let mut d = c.desired();
            d.desired = false;
            c.set_desired(d).unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).unwrap();
        }
    }
    struct Running(Child);
    impl Drop for Running {
        fn drop(&mut self) {
            if self.0.try_wait().is_ok_and(|s| s.is_none()) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
    }
    fn address() -> SocketAddr {
        TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
    }
    fn ready(child: &mut Child, address: SocketAddr) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            assert!(
                child.try_wait().unwrap().is_none(),
                "native CLI exited before listener admission"
            );
            if TcpStream::connect(address).is_ok() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "native listener startup exceeded test deadline"
            );
            std::thread::park_timeout(Duration::from_millis(5));
        }
    }
    fn request(
        address: SocketAddr,
        method: &str,
        target: &str,
        body: Option<&serde_json::Value>,
        cookie: &str,
    ) -> Vec<u8> {
        let text = body.map(ToString::to_string).unwrap_or_default();
        let raw = format!(
            "{method} {target} HTTP/1.1\r\nHost: localhost\r\nCookie: {cookie}\r\n{}\r\n{text}",
            if method == "POST" {
                format!(
                    "Content-Type: application/json\r\nOrigin: http://localhost\r\nContent-Length: {}\r\n",
                    text.len()
                )
            } else {
                String::new()
            }
        );
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream.write_all(raw.as_bytes()).unwrap();
        stream.shutdown(Shutdown::Write).unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).unwrap();
        response
    }
    fn value(response: &[u8], code: u16) -> serde_json::Value {
        assert!(
            response.starts_with(format!("HTTP/1.1 {code} ").as_bytes()),
            "{}",
            String::from_utf8_lossy(response)
        );
        let body = response.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        serde_json::from_slice(&response[body..]).unwrap()
    }
    fn login(address: SocketAddr) -> String {
        let response = request(
            address,
            "POST",
            "/api/session/login",
            Some(&serde_json::json!({"password":"native-cli-secret"})),
            "",
        );
        value(&response, 200);
        std::str::from_utf8(&response)
            .unwrap()
            .lines()
            .find_map(|l| l.strip_prefix("Set-Cookie: "))
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .into()
    }
    fn terminate(child: &mut Child) -> std::process::ExitStatus {
        assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGTERM) }, 0);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "native CLI did not finish bounded normal shutdown"
            );
            std::thread::park_timeout(Duration::from_millis(5));
        }
    }
    #[test]
    fn explicit_native_cli_has_authenticated_one_entry_and_graceful_signal_exit() {
        let fixture = Fixture::new();
        let address = address();
        let mut child = fixture.spawn(address);
        ready(&mut child.0, address);
        value(&request(address, "GET", "/api/runtime", None, ""), 401);
        let cookie = login(address);
        let health = value(&request(address, "GET", "/api/health", None, &cookie), 200);
        assert_eq!(health["mode"], "manager");
        assert_eq!(health["runtimeEnabled"], true);
        let runtime = value(&request(address, "GET", "/api/runtime", None, &cookie), 200);
        assert_eq!(runtime["services"].as_array().unwrap().len(), 2);
        assert_eq!(runtime["services"][0]["desired"], false);
        assert_eq!(
            value(
                &request(address, "GET", "/api/proxy/capture", None, &cookie),
                200
            )["active"],
            false
        );
        assert!(!fixture.root.join("run/capture-exec/commands").exists());
        assert!(terminate(&mut child.0).success());
    }
    #[test]
    fn busy_listener_and_duplicate_manager_never_withdraw_startup_journal() {
        let fixture = Fixture::new();
        fixture.journal();
        let occupied = TcpListener::bind("127.0.0.1:0").unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_be6500-panel"))
            .args(fixture.args(occupied.local_addr().unwrap()))
            .env("BE6500PANEL_PASSWORD", "native-cli-secret")
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(fixture.root.join("data/capture-journal.json").exists());
        assert!(!fixture.root.join("run/capture-exec/commands").exists());
        drop(occupied);
        let first_address = address();
        let mut child = fixture.spawn(first_address);
        ready(&mut child.0, first_address);
        let cookie = login(first_address);
        let _ = value(
            &request(first_address, "GET", "/api/runtime", None, &cookie),
            200,
        );
        let before = fs::read(fixture.root.join("run/capture-exec/commands")).unwrap();
        fixture.journal();
        let journal = fs::read(fixture.root.join("data/capture-journal.json")).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_be6500-panel"))
            .args(fixture.args(address()))
            .env("BE6500PANEL_PASSWORD", "native-cli-secret")
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert_eq!(
            fs::read(fixture.root.join("data/capture-journal.json")).unwrap(),
            journal
        );
        assert_eq!(
            fs::read(fixture.root.join("run/capture-exec/commands")).unwrap(),
            before
        );
        assert!(terminate(&mut child.0).success());
    }
    #[test]
    fn malformed_or_public_binding_manifest_never_echoes_private_metadata_or_touches_capture() {
        let fixture = Fixture::new();
        fixture.journal();
        for raw in [
            b"{\"private-value\":\"private-credential\"}".as_slice(),
            b"[]",
        ] {
            fs::write(fixture.root.join("bindings.json"), raw).unwrap();
            let output = Command::new(env!("CARGO_BIN_EXE_be6500-panel"))
                .args(fixture.args(address()))
                .env("BE6500PANEL_PASSWORD", "native-cli-secret")
                .output()
                .unwrap();
            assert!(!output.status.success());
            let stderr = String::from_utf8(output.stderr).unwrap();
            assert!(
                !stderr.contains("private-credential")
                    && !stderr.contains(fixture.root.to_str().unwrap())
            );
            assert!(!fixture.root.join("run/capture-exec/commands").exists());
        }
        fs::set_permissions(
            fixture.root.join("bindings.json"),
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_be6500-panel"))
            .args(fixture.args(address()))
            .env("BE6500PANEL_PASSWORD", "native-cli-secret")
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(fixture.root.join("data/capture-journal.json").exists());
    }

    #[test]
    fn sigterm_with_failed_cleanup_retains_authenticated_owner_until_explicit_repair_and_delete() {
        use std::os::fd::AsRawFd;
        let fixture = Fixture::new();
        fixture.journal();
        fs::DirBuilder::new()
            .mode(0o700)
            .create(fixture.root.join("run/capture-exec"))
            .unwrap();
        fs::write(fixture.root.join("run/capture-exec/mode"), b"fail").unwrap();
        let address = address();
        let mut child = fixture.spawn(address);
        ready(&mut child.0, address);
        let cookie = login(address);
        assert!(fixture.root.join("data/capture-journal.json").exists());
        assert_eq!(unsafe { libc::kill(child.0.id() as i32, libc::SIGTERM) }, 0);
        let stderr = child.0.stderr.as_mut().unwrap();
        let fd = stderr.as_raw_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        assert!(flags >= 0);
        assert_eq!(
            unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) },
            0
        );
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut log = String::new();
        let mut bytes = [0u8; 1024];
        loop {
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "failed cleanup must retain owner process"
            );
            match child.0.stderr.as_mut().unwrap().read(&mut bytes) {
                Ok(n) => log.push_str(std::str::from_utf8(&bytes[..n]).unwrap()),
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(error) => panic!("fixture stderr read failed: {error}"),
            }
            assert!(log.len() < 4096, "pending cleanup log must be bounded");
            if log.contains("owner retained") {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "retained shutdown state not reached: {log}"
            );
            std::thread::park_timeout(Duration::from_millis(5));
        }
        let before = fs::read(fixture.root.join("run/capture-exec/commands")).unwrap();
        let retained = value(&request(address, "GET", "/api/runtime", None, &cookie), 200);
        assert!(retained["services"][0].get("pid").is_none());
        value(&request(address, "GET", "/api/runtime", None, ""), 401);
        assert_eq!(
            fs::read(fixture.root.join("run/capture-exec/commands")).unwrap(),
            before,
            "readback must not retry cleanup"
        );
        let refused = value(
            &request(
                address,
                "POST",
                "/api/runtime/start",
                Some(&serde_json::json!({"service":"sing-box"})),
                &cookie,
            ),
            503,
        );
        assert!(refused["error"].get("code").is_some());
        fs::write(fixture.root.join("run/capture-exec/mode"), b"success").unwrap();
        let off = value(
            &request(address, "DELETE", "/api/proxy/capture", None, &cookie),
            200,
        );
        assert_eq!(off["desired"], false);
        assert_eq!(off["cleanupPending"], false);
        let deadline = Instant::now() + Duration::from_secs(5);
        let status = loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "explicit repaired cleanup must allow exit"
            );
            std::thread::park_timeout(Duration::from_millis(5));
        };
        assert!(status.success());
        assert!(!fixture.root.join("data/capture-journal.json").exists());
    }
}

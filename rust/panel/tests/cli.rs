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
    let path = std::env::temp_dir().join(format!("be6500-cli-draft-{}", std::process::id()));
    fs::create_dir(&path).unwrap();
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
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    // This is a finite native host test, not production polling or an agent loop.
    let connection = loop {
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
    child.kill().unwrap();
    child.wait().unwrap();
    let names: Vec<_> = fs::read_dir(&path)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    fs::remove_dir_all(&path).unwrap();
    assert!(
        response.starts_with(b"HTTP/1.1 200 "),
        "CLI did not serve drafts"
    );
    assert!(
        names.is_empty(),
        "startup created a draft/runtime file: {names:?}"
    );
}

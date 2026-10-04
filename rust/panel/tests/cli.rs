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

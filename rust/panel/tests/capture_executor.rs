#![cfg(any(target_os = "linux", target_os = "macos"))]
use be6500_panel::capture_executor::{Binaries, Executor, TrustedBinary};
use be6500_panel::capture_plan::{RulesPlanInput, plan_owned_rules};
use be6500_panel::capture_state::CommandError;
use be6500_panel::native::Ports;
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::{Duration, Instant},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
const HELPER: &str = r#"#!/bin/sh
umask 077
printf '%s\n' "$@" > "$TMPDIR/argv"
printf '%s\n' "${BE6500PANEL_PASSWORD-unset}:${http_proxy-unset}" > "$TMPDIR/inherited"
IFS= read -r mode < "$TMPDIR/mode" || :
case "$mode" in
 success) printf 'bounded-output\n'; exit 0;;
 failure) printf 'fixed-fake-failure\n' >&2; exit 8;;
 noisy) dd if=/dev/zero bs=65536 count=4 2>/dev/null; exit 0;;
 hang) IFS= read -r value < "$TMPDIR/wait";;
 *) exit 9;;
esac
"#;
struct Fixture {
    root: PathBuf,
    artifact: PathBuf,
    run: PathBuf,
    input: RulesPlanInput,
}
impl Fixture {
    fn new() -> Self {
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "b6p-executor-fake-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        for name in ["artifacts", "run"] {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(root.join(name))
                .unwrap();
        }
        let artifact = root.join("artifacts/fake-command");
        fs::write(&artifact, HELPER).unwrap();
        fs::set_permissions(&artifact, fs::Permissions::from_mode(0o700)).unwrap();
        let run = root.join("run");
        fs::write(run.join("mode"), b"success").unwrap();
        let fifo = std::ffi::CString::new(run.join("wait").as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        let input = RulesPlanInput {
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
        };
        Self {
            root,
            artifact,
            run,
            input,
        }
    }
    fn executor(&self) -> Executor {
        let hash = Sha256::digest(HELPER.as_bytes()).into();
        Executor::admit(
            &self.input,
            Binaries {
                ip: TrustedBinary::admit(&self.artifact, hash).unwrap(),
                iptables: TrustedBinary::admit(&self.artifact, hash).unwrap(),
            },
            &self.run,
        )
        .unwrap()
    }
    fn command(&self) -> Vec<String> {
        plan_owned_rules(&self.input).unwrap().apply[0].clone()
    }
    fn mode(&self, mode: &str) {
        fs::write(self.run.join("mode"), mode).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
#[test]
fn only_exact_internal_plan_argv_are_executed() {
    let fixture = Fixture::new();
    let mut executor = fixture.executor();
    assert!(!fixture.run.join("argv").exists());
    let command = fixture.command();
    let result = executor
        .execute(&command, Instant::now() + Duration::from_secs(2), None)
        .unwrap();
    assert!(result.success);
    assert_eq!(result.output, b"bounded-output\n");
    assert_eq!(
        fs::read_to_string(fixture.run.join("argv")).unwrap(),
        command[1..].join("\n") + "\n"
    );
    assert_eq!(
        fs::read_to_string(fixture.run.join("inherited")).unwrap(),
        "unset:unset\n"
    );
    let mut changed = command.clone();
    changed.push("unapproved".into());
    assert_eq!(
        executor
            .execute(&changed, Instant::now() + Duration::from_secs(1), None)
            .unwrap_err(),
        CommandError::Failure
    );
    assert!(
        executor
            .execute(
                &["sh".into(), "-c".into(), "true".into()],
                Instant::now() + Duration::from_secs(1),
                None
            )
            .is_err()
    );
    assert!(!format!("{executor:?}").contains(fixture.root.to_str().unwrap()));
}
#[test]
fn nonzero_and_oversized_output_do_not_claim_success() {
    let fixture = Fixture::new();
    let mut executor = fixture.executor();
    fixture.mode("failure");
    let result = executor
        .execute(
            &fixture.command(),
            Instant::now() + Duration::from_secs(2),
            None,
        )
        .unwrap();
    assert!(!result.success);
    assert_eq!(result.output, b"fixed-fake-failure\n");
    fixture.mode("noisy");
    assert_eq!(
        executor
            .execute(
                &fixture.command(),
                Instant::now() + Duration::from_secs(2),
                None
            )
            .unwrap_err(),
        CommandError::Failure
    );
    executor.retry_abort().unwrap();
}
#[test]
fn deadline_cancel_and_followup_are_bounded() {
    let fixture = Fixture::new();
    let mut executor = fixture.executor();
    let cancel = AtomicBool::new(true);
    assert_eq!(
        executor
            .execute(
                &fixture.command(),
                Instant::now() + Duration::from_secs(1),
                Some(&cancel)
            )
            .unwrap_err(),
        CommandError::Cancelled
    );
    assert!(!fixture.run.join("argv").exists());
    fixture.mode("hang");
    let start = Instant::now();
    assert_eq!(
        executor
            .execute(&fixture.command(), start + Duration::from_millis(100), None)
            .unwrap_err(),
        CommandError::Timeout
    );
    assert!(start.elapsed() < Duration::from_secs(2));
    executor.retry_abort().unwrap();
    fixture.mode("success");
    assert!(
        executor
            .execute(
                &fixture.command(),
                Instant::now() + Duration::from_secs(2),
                None
            )
            .unwrap()
            .success
    );
}
#[test]
fn changed_executable_and_run_root_are_refused() {
    let fixture = Fixture::new();
    let mut executor = fixture.executor();
    fs::write(&fixture.artifact, b"changed").unwrap();
    assert!(
        executor
            .execute(
                &fixture.command(),
                Instant::now() + Duration::from_secs(1),
                None
            )
            .is_err()
    );
    assert!(
        TrustedBinary::admit(&fixture.artifact, Sha256::digest(HELPER.as_bytes()).into()).is_err()
    );
    let fixture = Fixture::new();
    let mut executor = fixture.executor();
    fs::set_permissions(&fixture.run, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        executor
            .execute(
                &fixture.command(),
                Instant::now() + Duration::from_secs(1),
                None
            )
            .is_err()
    );
}

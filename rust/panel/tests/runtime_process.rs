#![cfg(any(target_os = "linux", target_os = "macos"))]

use be6500_panel::runtime_process::{
    LaunchSpec, Limits, Phase, ProcessError, ProcessOwner, ServiceId, TrustedRoots,
};
use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt, symlink};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

static SERIAL: Mutex<()> = Mutex::new(());
static NEXT: AtomicU64 = AtomicU64::new(0);
// This fixture is the only executable used in these tests. No real core, router,
// network socket, capture operation, or production path is accessed.
const HELPER: &str = r#"#!/bin/sh
umask 077
printf '%s\n' "$@" > "$TMPDIR/argv"
env > "$TMPDIR/environment"
pwd > "$TMPDIR/cwd"
config="$2"
case "$1" in run|check|verify) config="$3" ;; esac
behavior=$(cat "$config")
case "$behavior" in
  good) exit 0 ;;
  bad) printf 'private verifier failure\n' >&2; exit 9 ;;
  rewrite) printf 'changed' > "$config"; exit 0 ;;
  noisy)
    dd if=/dev/zero bs=65536 count=24 2>/dev/null
    dd if=/dev/zero bs=65536 count=24 1>&2 2>/dev/null
    printf 'private-output-tail-end\n' >&2
    exit 0 ;;
  hang) trap '' TERM; while :; do sleep 1; done ;;
  descendant)
    sleep 15 &
    printf '%s\n' "$!" > "$TMPDIR/descendant.pid"
    exit 0 ;;
  running)
    trap 'printf "term\n" >> "$TMPDIR/events"; exit 0' TERM
    printf '%s\n' "$$" > "$TMPDIR/leader.pid"
    while :; do sleep 1; done ;;
  *) exit 11 ;;
esac
"#;
struct Fixture {
    base: PathBuf,
    artifact: PathBuf,
    config: PathBuf,
    run: PathBuf,
    service: ServiceId,
}
impl Fixture {
    fn new(service: ServiceId) -> Self {
        let parent = fs::canonicalize(std::env::temp_dir()).unwrap();
        let base = parent.join(format!(
            "be6500-owned-fake-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&base).unwrap();
        for name in ["artifacts", "configs", "run"] {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(base.join(name))
                .unwrap();
        }
        let artifact = base.join("artifacts/fake-core");
        fs::write(&artifact, HELPER).unwrap();
        fs::set_permissions(&artifact, fs::Permissions::from_mode(0o700)).unwrap();
        let config = base.join(match service {
            ServiceId::SingBox => "configs/config-1.json",
            ServiceId::Frpc => "configs/config-1.toml",
        });
        let run = base.join("run");
        Self {
            base,
            artifact,
            config,
            run,
            service,
        }
    }
    fn roots(&self) -> TrustedRoots {
        TrustedRoots::new(
            self.base.join("artifacts"),
            self.base.join("configs"),
            &self.run,
        )
    }
    fn spec(&self, behavior: &str) -> LaunchSpec {
        fs::write(&self.config, behavior).unwrap();
        fs::set_permissions(&self.config, fs::Permissions::from_mode(0o600)).unwrap();
        LaunchSpec::new(
            &self.artifact,
            Sha256::digest(fs::read(&self.artifact).unwrap()).into(),
            &self.config,
            Sha256::digest(behavior.as_bytes()).into(),
            behavior.len() as u64,
        )
    }
    fn owner(&self) -> Guard {
        Guard(ProcessOwner::new(self.service, self.roots(), limits()).unwrap())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.base).unwrap();
    }
}
// Unwind cleanup for fake fixtures only. Production owner Drop intentionally
// cannot substitute for the caller's network cleanup.
struct Guard(ProcessOwner);
impl std::ops::Deref for Guard {
    type Target = ProcessOwner;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for Guard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.abort_check();
        let _ = self.0.stop_with_cleanup(|| Ok::<_, ()>(()));
        self.0
            .close()
            .expect("fake process owner was not fully cleaned up");
    }
}
fn limits() -> Limits {
    Limits {
        term_grace: Duration::from_millis(100),
        kill_grace: Duration::from_secs(1),
        check_timeout: Duration::from_secs(1),
    }
}
fn until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "finite fake-process wait exceeded"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn exists(pid: u32) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}
fn pid_file(path: &Path) -> u32 {
    fs::read_to_string(path).unwrap().trim().parse().unwrap()
}
fn assert_fixed_environment(fixture: &Fixture) {
    let env = fs::read_to_string(fixture.run.join("environment")).unwrap();
    assert!(
        env.lines()
            .any(|s| s == "PATH=/usr/sbin:/usr/bin:/sbin:/bin")
    );
    for key in ["HOME", "TMPDIR"] {
        assert!(
            env.lines()
                .any(|s| s == format!("{key}={}", fixture.run.display()))
        );
    }
    // A fixture shell may introduce PWD; nothing else is inherited from panel.
    for line in env.lines() {
        let key = line.split('=').next().unwrap();
        assert!(
            ["PATH", "HOME", "TMPDIR", "PWD", "SHLVL", "_"].contains(&key),
            "unexpected inherited environment key"
        );
    }
    assert!(!env.contains("BE6500PANEL_PASSWORD="));
    assert!(!env.contains("proxy="));
    assert_eq!(
        fs::read_to_string(fixture.run.join("cwd")).unwrap().trim(),
        fixture.run.to_str().unwrap()
    );
}

#[test]
fn fixed_arguments_private_environment_and_owner_exclusivity() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    for service in [ServiceId::SingBox, ServiceId::Frpc] {
        let fixture = Fixture::new(service);
        let mut owner = fixture.owner();
        assert_eq!(owner.status().unwrap().phase, Phase::Idle);
        assert_eq!(
            ProcessOwner::new(service, fixture.roots(), limits()).unwrap_err(),
            ProcessError::Busy
        );
        let status = owner.start(fixture.spec("running")).unwrap();
        assert_eq!(status.phase, Phase::Running);
        assert!(status.pid.is_some_and(exists));
        assert_eq!(
            owner.start(fixture.spec("running")).unwrap_err(),
            ProcessError::Busy
        );
        assert_eq!(owner.close().unwrap_err(), ProcessError::Busy);
        until(|| fixture.run.join("leader.pid").exists());
        let args = fs::read_to_string(fixture.run.join("argv")).unwrap();
        let expected = match service {
            ServiceId::SingBox => format!("run\n-c\n{}\n", fixture.config.display()),
            ServiceId::Frpc => format!("-c\n{}\n", fixture.config.display()),
        };
        assert_eq!(args, expected);
        assert_fixed_environment(&fixture);
        let stopped = owner.stop_with_cleanup(|| Ok::<_, ()>(())).unwrap();
        assert_eq!(stopped.phase, Phase::Stopped);
        assert_eq!(stopped.pid, None);
        assert!(!exists(status.pid.unwrap()), "leader must be fully reaped");
        owner.verify(fixture.spec("good"), None).unwrap();
        let args = fs::read_to_string(fixture.run.join("argv")).unwrap();
        assert_eq!(
            args,
            format!(
                "{}\n-c\n{}\n",
                match service {
                    ServiceId::SingBox => "check",
                    ServiceId::Frpc => "verify",
                },
                fixture.config.display()
            )
        );
        assert_fixed_environment(&fixture);
        assert_eq!(owner.status().unwrap().pid, None);
        let private = format!(
            "{:?} {:?} {:?} {}",
            fixture.roots(),
            fixture.spec("good"),
            &*owner,
            ProcessError::CheckFailed
        );
        assert!(!private.contains(fixture.base.to_str().unwrap()));
        assert!(!private.contains("private verifier"));
    }
}

#[test]
fn cleanup_failure_retains_live_child_and_success_precedes_term() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new(ServiceId::SingBox);
    let mut owner = fixture.owner();
    let status = owner.start(fixture.spec("running")).unwrap();
    until(|| fixture.run.join("leader.pid").exists());
    assert_eq!(
        owner
            .stop_with_cleanup(|| Err::<(), _>("private failure"))
            .unwrap_err(),
        ProcessError::CleanupFailed
    );
    assert_eq!(owner.status().unwrap().pid, status.pid);
    assert_eq!(owner.status().unwrap().phase, Phase::Running);
    assert!(exists(status.pid.unwrap()));
    assert!(
        !fixture.run.join("events").exists(),
        "cleanup failure must send no TERM"
    );
    owner
        .stop_with_cleanup(|| {
            fs::write(fixture.run.join("events"), "cleanup\n").unwrap();
            Ok::<_, ()>(())
        })
        .unwrap();
    assert_eq!(
        fs::read_to_string(fixture.run.join("events")).unwrap(),
        "cleanup\nterm\n"
    );
    owner.stop_with_cleanup(|| Err::<(), _>(())).unwrap();
    assert_eq!(owner.status().unwrap().phase, Phase::Stopped);
    assert!(!exists(status.pid.unwrap()));
}

#[test]
fn verifier_failure_timeout_cancellation_and_changed_candidate_are_fixed() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new(ServiceId::SingBox);
    let mut owner = fixture.owner();
    assert_eq!(
        owner.verify(fixture.spec("bad"), None).unwrap_err(),
        ProcessError::CheckFailed
    );
    assert_eq!(owner.status().unwrap().exit.unwrap().code, Some(9));
    assert!(owner.status().unwrap().pid.is_none());
    let started = Instant::now();
    assert_eq!(
        owner.verify(fixture.spec("hang"), None).unwrap_err(),
        ProcessError::CheckDeadline
    );
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(owner.status().unwrap().pid.is_none());
    let cancel = Arc::new(AtomicBool::new(true));
    assert_eq!(
        owner
            .verify(fixture.spec("good"), Some(cancel))
            .unwrap_err(),
        ProcessError::Cancelled
    );
    let cancel = Arc::new(AtomicBool::new(false));
    let set = cancel.clone();
    let setter = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(40));
        set.store(true, Ordering::Release);
    });
    assert_eq!(
        owner
            .verify(fixture.spec("hang"), Some(cancel))
            .unwrap_err(),
        ProcessError::Cancelled
    );
    setter.join().unwrap();
    assert!(owner.status().unwrap().pid.is_none());
    assert_eq!(
        owner.verify(fixture.spec("rewrite"), None).unwrap_err(),
        ProcessError::Integrity
    );
    assert!(owner.status().unwrap().pid.is_none());
}

#[test]
fn noisy_both_pipes_are_drained_without_deadlock() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new(ServiceId::Frpc);
    let mut limits = limits();
    limits.check_timeout = Duration::from_secs(3);
    let mut owner = Guard(ProcessOwner::new(fixture.service, fixture.roots(), limits).unwrap());
    let started = Instant::now();
    owner.verify(fixture.spec("noisy"), None).unwrap();
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(owner.status().unwrap().pid.is_none());
}

#[test]
fn natural_exit_retains_leader_identity_until_cleanup_and_reaping() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new(ServiceId::Frpc);
    let mut owner = fixture.owner();
    let status = owner.start(fixture.spec("descendant")).unwrap();
    until(|| owner.poll_exited().unwrap().is_some());
    let child_pid = pid_file(&fixture.run.join("descendant.pid"));
    assert!(exists(child_pid));
    assert_eq!(owner.status().unwrap().phase, Phase::Exited);
    assert_eq!(owner.status().unwrap().pid, status.pid);
    assert!(
        exists(status.pid.unwrap()),
        "waitable leader identity remains reserved"
    );
    assert_eq!(
        owner.stop_with_cleanup(|| Err::<(), _>(())).unwrap_err(),
        ProcessError::CleanupFailed
    );
    assert!(exists(child_pid));
    owner.stop_with_cleanup(|| Ok::<_, ()>(())).unwrap();
    assert_eq!(owner.status().unwrap().pid, None);
    assert!(!exists(status.pid.unwrap()));
    // macOS reaps orphan descendants. On Linux a dead descendant can remain a
    // zombie under a container init, so attest it is dead rather than own/reap it.
    until(|| descendant_dead(child_pid));
    assert_eq!(owner.status().unwrap().phase, Phase::Stopped);
}
fn descendant_dead(pid: u32) -> bool {
    if !exists(pid) {
        return true;
    }
    #[cfg(target_os = "linux")]
    {
        return fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|s| {
            s.split_once(") ")
                .is_some_and(|(_, rest)| rest.starts_with('Z'))
        });
    }
    #[cfg(not(target_os = "linux"))]
    false
}

#[test]
fn verifier_success_kills_descendants_before_full_reap() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new(ServiceId::SingBox);
    let mut owner = fixture.owner();
    owner.verify(fixture.spec("descendant"), None).unwrap();
    let child_pid = pid_file(&fixture.run.join("descendant.pid"));
    until(|| descendant_dead(child_pid));
    assert!(owner.status().unwrap().pid.is_none());
    assert_eq!(owner.status().unwrap().exit.unwrap().code, Some(0));
}

#[test]
fn untrusted_paths_modes_hashes_lengths_and_changed_root_are_refused() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new(ServiceId::SingBox);
    let mut owner = fixture.owner();
    let spec = fixture.spec("good");
    fs::write(&fixture.config, "changed").unwrap();
    assert_eq!(owner.start(spec).unwrap_err(), ProcessError::Integrity);
    let _ = fixture.spec("good");
    let digest: [u8; 32] = Sha256::digest(fs::read(&fixture.artifact).unwrap()).into();
    let config_digest: [u8; 32] = Sha256::digest(b"good").into();
    assert_eq!(
        owner
            .start(LaunchSpec::new(
                &fixture.artifact,
                [0; 32],
                &fixture.config,
                config_digest,
                4
            ))
            .unwrap_err(),
        ProcessError::Integrity
    );
    assert_eq!(
        owner
            .start(LaunchSpec::new(
                &fixture.artifact,
                digest,
                &fixture.config,
                config_digest,
                3
            ))
            .unwrap_err(),
        ProcessError::Integrity
    );
    let spec = fixture.spec("good");
    fs::set_permissions(&fixture.artifact, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(owner.start(spec).unwrap_err(), ProcessError::UntrustedPath);
    fs::set_permissions(&fixture.artifact, fs::Permissions::from_mode(0o700)).unwrap();
    let spec = fixture.spec("good");
    let original = fixture.base.join("artifacts/original");
    fs::rename(&fixture.artifact, &original).unwrap();
    symlink(&original, &fixture.artifact).unwrap();
    assert_eq!(owner.start(spec).unwrap_err(), ProcessError::UntrustedPath);
    fs::remove_file(&fixture.artifact).unwrap();
    fs::rename(&original, &fixture.artifact).unwrap();
    let spec = fixture.spec("good");
    let original_config = fixture.base.join("configs/original");
    fs::rename(&fixture.config, &original_config).unwrap();
    symlink(&original_config, &fixture.config).unwrap();
    assert_eq!(owner.start(spec).unwrap_err(), ProcessError::UntrustedPath);
    fs::remove_file(&fixture.config).unwrap();
    fs::rename(&original_config, &fixture.config).unwrap();
    let arbitrary = fixture.base.join("configs/arbitrary.json");
    fs::write(&arbitrary, "good").unwrap();
    fs::set_permissions(&arbitrary, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        owner
            .start(LaunchSpec::new(
                &fixture.artifact,
                digest,
                &arbitrary,
                config_digest,
                4
            ))
            .unwrap_err(),
        ProcessError::UntrustedPath
    );
    let spec = fixture.spec("good");
    fs::rename(&fixture.run, fixture.base.join("old-run")).unwrap();
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&fixture.run)
        .unwrap();
    assert_eq!(owner.start(spec).unwrap_err(), ProcessError::UntrustedPath);
    assert!(owner.status().unwrap().pid.is_none());
    owner.close().unwrap();
    assert_eq!(
        ProcessOwner::new(
            ServiceId::SingBox,
            TrustedRoots::new("relative", fixture.base.join("configs"), &fixture.run),
            limits()
        )
        .unwrap_err(),
        ProcessError::UntrustedPath
    );
    assert_eq!(
        ProcessOwner::new(
            ServiceId::SingBox,
            TrustedRoots::new(&fixture.run, &fixture.run, &fixture.run),
            limits()
        )
        .unwrap_err(),
        ProcessError::UntrustedPath
    );
    let linked = fixture.base.join("linked-root");
    symlink(fixture.base.join("artifacts"), &linked).unwrap();
    assert_eq!(
        ProcessOwner::new(
            ServiceId::SingBox,
            TrustedRoots::new(linked, fixture.base.join("configs"), &fixture.run),
            limits()
        )
        .unwrap_err(),
        ProcessError::UntrustedPath
    );
}

#[test]
fn unrelated_child_is_not_adopted_or_signalled() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    // Another trusted fixture gets a different group through std Command only
    // to prove owned stop cannot affect arbitrary live PIDs.
    let other = Fixture::new(ServiceId::SingBox);
    let _ = other.spec("running");
    let mut unrelated = std::process::Command::new(&other.artifact)
        .args(["run", "-c"])
        .arg(&other.config)
        .env_clear()
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .env("HOME", &other.run)
        .env("TMPDIR", &other.run)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .spawn()
        .unwrap();
    let fixture = Fixture::new(ServiceId::SingBox);
    let mut owner = fixture.owner();
    let status = owner.start(fixture.spec("running")).unwrap();
    assert_ne!(status.pid, Some(unrelated.id()));
    owner.stop_with_cleanup(|| Ok::<_, ()>(())).unwrap();
    assert!(unrelated.try_wait().unwrap().is_none());
    assert_eq!(
        unsafe { libc::kill(-(unrelated.id() as libc::pid_t), libc::SIGKILL) },
        0
    );
    unrelated.wait().unwrap();
}

#[test]
fn injected_environment_child() {
    if std::env::var_os("BE6500_FIXTURE_ENV_SUBTEST").is_none() {
        return;
    }
    let fixture = Fixture::new(ServiceId::Frpc);
    let mut owner = fixture.owner();
    owner.verify(fixture.spec("good"), None).unwrap();
    assert_fixed_environment(&fixture);
}
#[test]
fn panel_password_and_proxy_are_not_inherited() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "injected_environment_child", "--nocapture"])
        .env("BE6500_FIXTURE_ENV_SUBTEST", "1")
        .env("BE6500PANEL_PASSWORD", "private-panel-password")
        .env("http_proxy", "private-proxy-value")
        .env("HTTPS_PROXY", "private-proxy-value")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "environment subtest failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn store_candidate_check_only_frpc_legacy_json_and_volatile_artifact_layout() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    for service in [ServiceId::SingBox, ServiceId::Frpc] {
        let fixture = Fixture::new(service);
        let mut owner = fixture.owner();
        let _ = fixture.spec("good");
        for suffix in match service {
            ServiceId::SingBox => vec!["json"],
            ServiceId::Frpc => vec!["json", "toml"],
        } {
            let candidate = fixture.base.join(format!(
                "configs/.candidate-0123456789abcdef0123456789abcdef.{suffix}"
            ));
            fs::write(&candidate, b"good").unwrap();
            fs::set_permissions(&candidate, fs::Permissions::from_mode(0o600)).unwrap();
            let spec = LaunchSpec::new(
                &fixture.artifact,
                Sha256::digest(fs::read(&fixture.artifact).unwrap()).into(),
                &candidate,
                Sha256::digest(b"good").into(),
                4,
            );
            assert_eq!(
                owner.start(spec.clone()).unwrap_err(),
                ProcessError::UntrustedPath
            );
            owner.verify(spec, None).unwrap();
        }
        let unowned = fixture.base.join("configs/.candidate-not-owned.json");
        fs::write(&unowned, b"good").unwrap();
        fs::set_permissions(&unowned, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            owner
                .verify(
                    LaunchSpec::new(
                        &fixture.artifact,
                        Sha256::digest(fs::read(&fixture.artifact).unwrap()).into(),
                        unowned,
                        Sha256::digest(b"good").into(),
                        4
                    ),
                    None
                )
                .unwrap_err(),
            ProcessError::UntrustedPath
        );
    }
    let mut fixture = Fixture::new(ServiceId::Frpc);
    fixture.config = fixture.base.join("configs/config-1.json");
    let mut owner = fixture.owner();
    owner.start(fixture.spec("running")).unwrap();
    owner.stop_with_cleanup(|| Ok::<_, ()>(())).unwrap();
    owner.close().unwrap();
    // Existing artifacts may be in RunDir, or a private subdirectory of it.
    for nested in [false, true] {
        let artifact_root = if nested {
            let p = fixture.run.join("cores");
            fs::DirBuilder::new().mode(0o700).create(&p).unwrap();
            p
        } else {
            fixture.run.clone()
        };
        fixture.artifact = artifact_root.join("fake-core");
        fs::write(&fixture.artifact, HELPER).unwrap();
        fs::set_permissions(&fixture.artifact, fs::Permissions::from_mode(0o700)).unwrap();
        let roots = TrustedRoots::new(artifact_root, fixture.base.join("configs"), &fixture.run);
        let mut owner = Guard(ProcessOwner::new(ServiceId::Frpc, roots, limits()).unwrap());
        owner.verify(fixture.spec("good"), None).unwrap();
    }
}

#[test]
fn non_regular_hardlinked_and_public_files_are_refused_before_spawn() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new(ServiceId::SingBox);
    let mut owner = fixture.owner();
    let spec = fixture.spec("good");
    let link = fixture.base.join("configs/hardlink");
    fs::hard_link(&fixture.config, &link).unwrap();
    assert_eq!(owner.start(spec).unwrap_err(), ProcessError::UntrustedPath);
    fs::remove_file(link).unwrap();
    let spec = fixture.spec("good");
    fs::set_permissions(&fixture.config, fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(owner.start(spec).unwrap_err(), ProcessError::UntrustedPath);
    let spec = fixture.spec("good");
    fs::remove_file(&fixture.config).unwrap();
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&fixture.config)
        .unwrap();
    assert_eq!(owner.start(spec).unwrap_err(), ProcessError::UntrustedPath);
    fs::remove_dir(&fixture.config).unwrap();
    let spec = fixture.spec("good");
    fs::remove_file(&fixture.config).unwrap();
    let path = std::ffi::CString::new(fixture.config.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    let started = Instant::now();
    assert_eq!(owner.start(spec).unwrap_err(), ProcessError::UntrustedPath);
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(owner.status().unwrap().pid.is_none());
}

#[test]
fn candidate_verification_keeps_existing_run_owned_and_untouched() {
    let _lock = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    for service in [ServiceId::SingBox, ServiceId::Frpc] {
        let fixture = Fixture::new(service);
        let mut owner = fixture.owner();
        let status = owner.start(fixture.spec("running")).unwrap();
        until(|| fixture.run.join("leader.pid").exists());
        for behavior in ["good", "bad", "noisy", "hang"] {
            let extension = match service {
                ServiceId::SingBox => "json",
                ServiceId::Frpc => "toml",
            };
            let candidate = fixture.base.join(format!(
                "configs/.candidate-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.{extension}"
            ));
            fs::write(&candidate, behavior).unwrap();
            fs::set_permissions(&candidate, fs::Permissions::from_mode(0o600)).unwrap();
            let spec = LaunchSpec::new(
                &fixture.artifact,
                Sha256::digest(fs::read(&fixture.artifact).unwrap()).into(),
                candidate,
                Sha256::digest(behavior.as_bytes()).into(),
                behavior.len() as u64,
            );
            let result = owner.verify(spec, None);
            match behavior {
                "bad" => assert_eq!(result.unwrap_err(), ProcessError::CheckFailed),
                "hang" => assert_eq!(result.unwrap_err(), ProcessError::CheckDeadline),
                _ => result.unwrap(),
            }
            assert_eq!(owner.status().unwrap().pid, status.pid);
            assert_eq!(owner.status().unwrap().phase, Phase::Running);
            assert!(exists(status.pid.unwrap()));
            assert!(
                !fixture.run.join("events").exists(),
                "Check must not TERM the Run group"
            );
        }
        owner
            .stop_with_cleanup(|| {
                fs::write(fixture.run.join("events"), "cleanup\n").unwrap();
                Ok::<_, ()>(())
            })
            .unwrap();
        assert_eq!(
            fs::read_to_string(fixture.run.join("events")).unwrap(),
            "cleanup\nterm\n"
        );
    }
}

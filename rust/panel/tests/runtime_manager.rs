#![cfg(any(target_os = "linux", target_os = "macos"))]

use be6500_panel::runtime_manager::{
    ArtifactBinding, ArtifactBindings, ArtifactProvenance, Failure, HookError, HookStage, Hooks,
    Limits, Manager, ServiceId, State,
};
use be6500_panel::runtime_process::{self as process, ProcessError};
use be6500_panel::runtime_store::{self as store, StoreError};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::fs;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

static SERIAL: Mutex<()> = Mutex::new(());
static NEXT: AtomicU64 = AtomicU64::new(0);
// No external command, real core, socket, capture, firewall or production path.
// Run waits on a private FIFO with shell builtins, without a busy loop. Checks
// inspect the actual staged candidate passed through the fixed -c argv.
const HELPER: &str = r#"#!/bin/sh
umask 077
mode=run
config="$2"
case "$1" in run|check|verify) mode="$1"; config="$3" ;; esac
IFS= read -r behavior < "$config" || :
case "$mode" in
  check|verify)
    printf '%s\n' "$config" >> "$TMPDIR/checks"
    if IFS= read -r oldpid < "$TMPDIR/expected.pid"; then
      kill -0 "$oldpid" || exit 13
      printf '%s\n' "$oldpid" >> "$TMPDIR/check.live"
    fi
    case "$behavior" in
      bad) printf 'private-password verifier output\n' >&2; exit 9 ;;
      rewrite) printf 'changed\n' > "$config"; exit 0 ;;
      hang) trap '' TERM; IFS= read -r value < "$TMPDIR/wait"; exit 0 ;;
      *) exit 0 ;;
    esac ;;
esac
trap 'printf "term %s\n" "$$" >> "$TMPDIR/events"; exit 0' TERM
printf '%s %s\n' "$$" "$config" >> "$TMPDIR/starts"
case "$behavior" in exit) exit 7 ;; esac
IFS= read -r value < "$TMPDIR/wait"
"#;
#[derive(Default)]
struct Control {
    events: Vec<(HookStage, ServiceId, u64, Option<u32>, String)>,
    fail_prestart_generation: Option<u64>,
    fail_ready_generation: Option<u64>,
    fail_cleanup: bool,
    fail_cleanup_generation: Option<u64>,
    fail_restore: bool,
    prestart_mutate_generation: Option<u64>,
    cleanup_mutate_candidate: bool,
    ready_expired: bool,
}
struct Fixture {
    base: PathBuf,
    control: Rc<RefCell<Control>>,
}
impl Fixture {
    fn new() -> Self {
        let parent = fs::canonicalize(std::env::temp_dir()).unwrap();
        let base = parent.join(format!(
            "be6500-manager-fake-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&base).unwrap();
        for name in ["artifacts", "run", "run/sing-box", "run/frpc"] {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(base.join(name))
                .unwrap();
        }
        fs::write(base.join("artifacts/fake-core"), HELPER).unwrap();
        fs::set_permissions(
            base.join("artifacts/fake-core"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        for service in [ServiceId::SingBox, ServiceId::Frpc] {
            let fifo = std::ffi::CString::new(
                base.join("run")
                    .join(service.as_str())
                    .join("wait")
                    .as_os_str()
                    .as_encoded_bytes(),
            )
            .unwrap();
            assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        }
        Self {
            base,
            control: Rc::new(RefCell::new(Control::default())),
        }
    }
    fn binding(&self, service: ServiceId) -> ArtifactBinding {
        ArtifactBinding::trusted_local(
            service,
            self.base.join("artifacts"),
            self.base.join("artifacts/fake-core"),
            Sha256::digest(HELPER.as_bytes()).into(),
            ArtifactProvenance::TrustedLocalModule,
        )
    }
    fn hooks(&self) -> Hooks {
        let pre_control = self.control.clone();
        let pre_base = self.base.clone();
        let ready_control = self.control.clone();
        let ready_base = self.base.clone();
        let cleanup_control = self.control.clone();
        let cleanup_base = self.base.clone();
        let restore_control = self.control.clone();
        Hooks::new(
            move |context| {
                assert_hook_config(context);
                assert!(context.run.is_none());
                let mut control = pre_control.borrow_mut();
                control.events.push((
                    HookStage::PreStart,
                    context.service,
                    context.config.generation,
                    None,
                    context.config.sha256.clone(),
                ));
                if control.prestart_mutate_generation == Some(context.config.generation) {
                    fs::write(
                        pre_base
                            .join("services")
                            .join(context.service.as_str())
                            .join(format!("config-{}.json", context.config.generation)),
                        b"mutated\n",
                    )
                    .unwrap();
                }
                if control.fail_prestart_generation == Some(context.config.generation) {
                    return Err(HookError::Failed);
                }
                Ok(())
            },
            move |context| {
                assert_hook_config(context);
                let identity = context.run.expect("readiness requires actual owned run");
                assert!(exists(identity.pid()));
                assert_eq!(identity.sha256(), context.config.sha256);
                assert_eq!(identity.launch_generation(), context.config.generation);
                ready_control.borrow_mut().events.push((
                    HookStage::Readiness,
                    context.service,
                    context.config.generation,
                    Some(identity.pid()),
                    context.config.sha256.clone(),
                ));
                if ready_control.borrow().ready_expired {
                    // Finite fake hook that explicitly honors/reports its deadline.
                    return Err(HookError::Deadline);
                }
                if ready_control.borrow().fail_ready_generation == Some(context.config.generation) {
                    return Err(HookError::Failed);
                }
                let starts = ready_base
                    .join("run")
                    .join(context.service.as_str())
                    .join("starts");
                finite_until(context.deadline, || {
                    fs::read_to_string(&starts).is_ok_and(|text| {
                        text.lines()
                            .any(|line| line.starts_with(&format!("{} ", identity.pid())))
                    })
                })?;
                Ok(())
            },
            move |context| {
                // A failed prestart may have mutated accepted bytes. Cleanup
                // still receives the retained trusted path and original SHA.
                assert!(!format!("{context:?}").contains(context.config_path.to_str().unwrap()));
                let identity = context.run.expect("cleanup retains exact run identity");
                let mut control = cleanup_control.borrow_mut();
                control.events.push((
                    HookStage::Cleanup,
                    context.service,
                    context.config.generation,
                    Some(identity.pid()),
                    context.config.sha256.clone(),
                ));
                if control.fail_cleanup
                    || control.fail_cleanup_generation == Some(context.config.generation)
                {
                    return Err(HookError::Failed);
                }
                if control.cleanup_mutate_candidate {
                    // Same-uid mutation after Check is still rejected by the
                    // store commit. Only touch this test's real staged file.
                    let root = cleanup_base.join("services").join(context.service.as_str());
                    for entry in fs::read_dir(root).unwrap() {
                        let entry = entry.unwrap();
                        if entry
                            .file_name()
                            .as_encoded_bytes()
                            .starts_with(b".candidate-")
                        {
                            fs::write(entry.path(), b"changed-after-check\n").unwrap();
                        }
                    }
                }
                let path = cleanup_base
                    .join("run")
                    .join(context.service.as_str())
                    .join("events");
                use std::io::Write;
                let mut file = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .unwrap();
                writeln!(file, "cleanup {}", identity.pid()).unwrap();
                Ok(())
            },
            move |context| {
                assert_hook_config(context);
                let identity = context.run.expect("restore requires real owned ready run");
                assert_eq!(identity.accepted_generation(), context.config.generation);
                assert!(exists(identity.pid()));
                restore_control.borrow_mut().events.push((
                    HookStage::Restore,
                    context.service,
                    context.config.generation,
                    Some(identity.pid()),
                    context.config.sha256.clone(),
                ));
                if restore_control.borrow().fail_restore {
                    return Err(HookError::Failed);
                }
                Ok(())
            },
        )
    }
    fn manager(&self) -> Guard {
        Guard {
            manager: Manager::open(
                self.base.join("services"),
                self.base.join("run"),
                ArtifactBindings {
                    sing_box: Some(self.binding(ServiceId::SingBox)),
                    frpc: Some(self.binding(ServiceId::Frpc)),
                },
                self.hooks(),
                limits(),
            )
            .unwrap(),
            control: self.control.clone(),
        }
    }
    fn run(&self, service: ServiceId) -> PathBuf {
        self.base.join("run").join(service.as_str())
    }
    fn expect_live(&self, service: ServiceId, pid: u32) {
        fs::write(self.run(service).join("expected.pid"), format!("{pid}\n")).unwrap();
    }
    fn manifest(&self, service: ServiceId) -> Vec<u8> {
        fs::read(
            self.base
                .join("services")
                .join(service.as_str())
                .join("state.json"),
        )
        .unwrap()
    }
    fn no_candidates(&self, service: ServiceId) {
        assert!(
            fs::read_dir(self.base.join("services").join(service.as_str()))
                .unwrap()
                .all(|entry| !entry
                    .unwrap()
                    .file_name()
                    .as_encoded_bytes()
                    .starts_with(b".candidate-"))
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.base).unwrap();
    }
}
struct Guard {
    manager: Manager,
    control: Rc<RefCell<Control>>,
}
impl std::ops::Deref for Guard {
    type Target = Manager;
    fn deref(&self) -> &Manager {
        &self.manager
    }
}
impl std::ops::DerefMut for Guard {
    fn deref_mut(&mut self) -> &mut Manager {
        &mut self.manager
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        // Fake test fixture only; production Manager has no Drop stop bypass.
        self.control.borrow_mut().fail_cleanup = false;
        self.control.borrow_mut().fail_cleanup_generation = None;
        self.manager
            .close()
            .expect("finite fake cleanup must leave no owned children");
    }
}
fn limits() -> Limits {
    Limits {
        process: process::Limits {
            term_grace: Duration::from_millis(50),
            kill_grace: Duration::from_secs(1),
            check_timeout: Duration::from_secs(1),
        },
        readiness_timeout: Duration::from_secs(2),
        resource_timeout: Duration::from_secs(1),
    }
}
fn assert_hook_config(context: &be6500_panel::runtime_manager::HookContext<'_>) {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(context.config_path)
        .unwrap();
    let metadata = file.metadata().unwrap();
    assert!(metadata.is_file() && metadata.len() <= store::MAX_STORED_CONFIG_BYTES as u64);
    let mut bytes = Vec::new();
    file.take(store::MAX_STORED_CONFIG_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .unwrap();
    assert!(bytes.len() <= store::MAX_STORED_CONFIG_BYTES);
    assert_eq!(hash(&bytes), context.config.sha256);
    assert_eq!(
        context.config_path.file_name().unwrap().to_str().unwrap(),
        format!(
            "config-{}.{}",
            context.config.generation,
            if context.service == ServiceId::Frpc {
                "toml"
            } else {
                "json"
            }
        )
    );
    assert!(!format!("{context:?}").contains(context.config_path.to_str().unwrap()));
}
fn exists(pid: u32) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}
fn finite_until(deadline: Instant, mut condition: impl FnMut() -> bool) -> Result<(), HookError> {
    while !condition() {
        if Instant::now() >= deadline {
            return Err(HookError::Deadline);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}
fn live(manager: &mut Manager, service: ServiceId, raw: &[u8]) -> u32 {
    manager.configure(service, 0, raw, None).unwrap();
    let status = manager.start(service).unwrap();
    assert!(status.ready && status.active && status.running_matches_accepted);
    status.pid.unwrap()
}
fn hash(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
}

#[test]
fn constructor_locks_without_process_hook_or_adoption_and_keeps_off_intent() {
    let _lock = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let fixture = Fixture::new();
    let mut manager = fixture.manager();
    for service in [ServiceId::SingBox, ServiceId::Frpc] {
        let status = manager.status(service).unwrap();
        assert_eq!(status.state, State::NotConfigured);
        assert!(!status.desired && !status.ready && !status.active && status.pid.is_none());
        assert!(!fixture.run(service).join("starts").exists());
        assert!(!fixture.run(service).join("checks").exists());
        assert!(!fixture.run(service).join("events").exists());
        manager.configure(service, 0, b"good\n", None).unwrap();
        assert!(!manager.status(service).unwrap().desired);
        assert!(!fixture.run(service).join("starts").exists());
    }
    assert!(fixture.control.borrow().events.is_empty());
    assert_eq!(
        store::RuntimeStore::open(
            fixture.base.join("services"),
            fixture.base.join("other-run")
        )
        .unwrap_err(),
        StoreError::Busy
    );
    manager.close().unwrap();
    drop(manager);
    let mut manager = fixture.manager();
    for service in [ServiceId::SingBox, ServiceId::Frpc] {
        assert_eq!(manager.status(service).unwrap().state, State::Stopped);
        assert!(!manager.status(service).unwrap().desired);
    }
}

#[test]
fn verifier_runs_exact_candidate_while_old_run_lives_and_rejects_bad_changed_and_stale() {
    let _lock = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let fixture = Fixture::new();
    let mut manager = fixture.manager();
    for service in [ServiceId::SingBox, ServiceId::Frpc] {
        let pid = live(&mut manager, service, b"good\n");
        fixture.expect_live(service, pid);
        let old_manifest = fixture.manifest(service);
        for (bytes, failure) in [
            (
                b"bad\n".as_slice(),
                Failure::Process(ProcessError::CheckFailed),
            ),
            (
                b"rewrite\n".as_slice(),
                Failure::Process(ProcessError::Integrity),
            ),
        ] {
            assert_eq!(
                manager
                    .configure(service, 1, bytes, None)
                    .unwrap_err()
                    .failure,
                failure
            );
            assert_eq!(fixture.manifest(service), old_manifest);
            assert_eq!(manager.status(service).unwrap().pid, Some(pid));
            assert!(manager.status(service).unwrap().active);
            assert_eq!(manager.config(service).unwrap().unwrap().bytes(), b"good\n");
            fixture.no_candidates(service);
        }
        let checks = fs::read_to_string(fixture.run(service).join("checks")).unwrap();
        assert!(checks.lines().all(|line| line.contains("/.candidate-")));
        assert_eq!(
            manager
                .configure(service, 0, b"new\n", None)
                .unwrap_err()
                .failure,
            Failure::Generation
        );
        assert_eq!(
            fs::read_to_string(fixture.run(service).join("checks")).unwrap(),
            checks
        );
        assert_eq!(fixture.manifest(service), old_manifest);
        assert_eq!(
            fs::read_to_string(fixture.run(service).join("check.live"))
                .unwrap()
                .lines()
                .count(),
            2
        );
        let status = manager.configure(service, 1, b"new\n", None).unwrap();
        assert_eq!(status.generation, 2);
        assert!(status.active && !status.restored && !status.needs_recovery);
        assert_ne!(status.pid, Some(pid));
        assert!(!exists(pid));
        let manifest: serde_json::Value =
            serde_json::from_slice(&fixture.manifest(service)).unwrap();
        assert_eq!(manifest["lastGood"]["generation"], 1);
        assert_eq!(manifest["lastGood"]["ready"], true);
        assert_eq!(manifest["current"]["ready"], true);
        let events = fixture
            .control
            .borrow()
            .events
            .iter()
            .filter(|event| event.1 == service)
            .map(|event| (event.0, event.2))
            .collect::<Vec<_>>();
        assert_eq!(
            events,
            vec![
                (HookStage::PreStart, 1),
                (HookStage::Readiness, 1),
                (HookStage::Restore, 1),
                (HookStage::Cleanup, 1),
                (HookStage::PreStart, 2),
                (HookStage::Readiness, 2),
                (HookStage::Restore, 2)
            ]
        );
        let process_events = fs::read_to_string(fixture.run(service).join("events")).unwrap();
        assert!(process_events.starts_with(&format!("cleanup {pid}\n")));
    }
}

#[test]
fn failed_cleanup_sends_no_term_keeps_generation_and_close_can_retry() {
    let _lock = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let fixture = Fixture::new();
    let mut manager = fixture.manager();
    let service = ServiceId::SingBox;
    let pid = live(&mut manager, service, b"old\n");
    let manifest = fixture.manifest(service);
    fixture.control.borrow_mut().fail_cleanup = true;
    assert_eq!(
        manager
            .configure(service, 1, b"new\n", None)
            .unwrap_err()
            .failure,
        Failure::Hook(HookStage::Cleanup, HookError::Failed)
    );
    let status = manager.status(service).unwrap();
    assert_eq!(status.pid, Some(pid));
    assert_eq!(status.generation, 1);
    assert!(status.needs_recovery && status.resource_suspended);
    assert_eq!(fixture.manifest(service), manifest);
    fixture.no_candidates(service);
    assert!(!fixture.run(service).join("events").exists());
    assert_eq!(
        manager.close().unwrap_err().failure,
        Failure::Hook(HookStage::Cleanup, HookError::Failed)
    );
    assert!(exists(pid));
    assert!(!manager.status(service).unwrap().desired);
    fixture.control.borrow_mut().fail_cleanup = false;
    manager.close().unwrap();
    assert!(!exists(pid));
    assert_eq!(manager.start(service).unwrap_err().failure, Failure::Closed);
}

#[test]
fn prestart_and_readiness_failure_recover_exact_proven_old_hash_with_monotonic_generation() {
    let _lock = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    for stage in [HookStage::PreStart, HookStage::Readiness] {
        let fixture = Fixture::new();
        let mut manager = fixture.manager();
        let service = ServiceId::SingBox;
        let old_pid = live(&mut manager, service, b"private-password-old\n");
        if stage == HookStage::PreStart {
            fixture.control.borrow_mut().fail_prestart_generation = Some(2);
        } else {
            fixture.control.borrow_mut().fail_ready_generation = Some(2);
        }
        let error = manager.configure(service, 1, b"new\n", None).unwrap_err();
        assert_eq!(error.failure, Failure::Hook(stage, HookError::Failed));
        assert_eq!(error.recovery_failure, None);
        let status = manager.status(service).unwrap();
        assert_eq!(status.generation, 3);
        assert!(
            status.restored
                && status.desired
                && status.ready
                && status.active
                && !status.needs_recovery
        );
        assert_ne!(status.pid, Some(old_pid));
        assert!(!exists(old_pid));
        let config = manager.config(service).unwrap().unwrap();
        assert_eq!(config.bytes(), b"private-password-old\n");
        assert_eq!(config.identity.sha256, hash(b"private-password-old\n"));
        let run = manager.current_run(service).unwrap();
        assert_eq!(run.launch_generation(), 1);
        assert_eq!(run.accepted_generation(), 3);
        assert_eq!(run.sha256(), config.identity.sha256);
        let manifest: serde_json::Value =
            serde_json::from_slice(&fixture.manifest(service)).unwrap();
        assert_eq!(manifest["lastGood"]["generation"], 1);
        assert_eq!(manifest["lastGood"]["ready"], true);
        assert_eq!(manifest["current"]["generation"], 3);
        assert_eq!(manifest["current"]["ready"], true);
        assert!(
            fixture
                .base
                .join("services/sing-box/config-1.json")
                .exists()
        );
        assert!(
            fixture
                .base
                .join("services/sing-box/config-2.json")
                .exists()
        );
        assert!(
            fixture
                .base
                .join("services/sing-box/config-3.json")
                .exists()
        );
        let public = format!(
            "{status:?} {error:?} {config:?} {}",
            serde_json::to_string(&status).unwrap()
        );
        assert!(
            !public.contains("private-password")
                && !public.contains(fixture.base.to_str().unwrap())
        );
        let events = fixture.control.borrow().events.clone();
        assert!(
            events.iter().any(|event| event.0 == HookStage::Readiness
                && event.2 == 1
                && event.3 == status.pid)
        );
        assert!(
            events.iter().any(|event| event.0 == HookStage::Restore
                && event.2 == 3
                && event.3 == status.pid)
        );
    }
}

#[test]
fn unready_new_child_cleanup_failure_retains_child_current_and_prevents_rollback() {
    let _lock = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let fixture = Fixture::new();
    let mut manager = fixture.manager();
    let service = ServiceId::SingBox;
    let old_pid = live(&mut manager, service, b"old\n");
    fixture.control.borrow_mut().fail_ready_generation = Some(2);
    fixture.control.borrow_mut().fail_cleanup_generation = Some(2);
    let error = manager.configure(service, 1, b"new\n", None).unwrap_err();
    assert_eq!(
        error.recovery_failure,
        Some(Failure::Hook(HookStage::Cleanup, HookError::Failed))
    );
    let status = manager.status(service).unwrap();
    assert_eq!(status.generation, 2);
    assert!(status.pid.is_some_and(exists));
    assert_ne!(status.pid, Some(old_pid));
    assert!(!status.ready && !status.active && !status.restored && status.needs_recovery);
    assert_eq!(manager.config(service).unwrap().unwrap().bytes(), b"new\n");
    assert!(!exists(old_pid));
    assert!(
        !fixture
            .base
            .join("services/sing-box/config-3.json")
            .exists()
    );
    assert_eq!(
        fixture
            .control
            .borrow()
            .events
            .iter()
            .filter(|event| event.0 == HookStage::Restore)
            .count(),
        1
    );
}

#[test]
fn restore_resource_failure_keeps_ready_core_but_never_claims_active() {
    let _lock = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let fixture = Fixture::new();
    let mut manager = fixture.manager();
    let service = ServiceId::Frpc;
    let old_pid = live(&mut manager, service, b"old\n");
    fixture.control.borrow_mut().fail_restore = true;
    assert_eq!(
        manager
            .configure(service, 1, b"new\n", None)
            .unwrap_err()
            .failure,
        Failure::Hook(HookStage::Restore, HookError::Failed)
    );
    let status = manager.status(service).unwrap();
    assert_eq!(status.generation, 2);
    assert_eq!(status.state, State::Running);
    assert!(status.pid.is_some_and(exists));
    assert_ne!(status.pid, Some(old_pid));
    assert!(status.ready && status.resource_suspended && status.needs_recovery && !status.active);
    assert!(!status.restored && !exists(old_pid));
    assert!(!fixture.base.join("services/frpc/config-3.toml").exists());
}

#[test]
fn explicit_restore_requires_ready_last_good_and_respects_stopped_intent() {
    let _lock = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let fixture = Fixture::new();
    let mut manager = fixture.manager();
    let service = ServiceId::SingBox;
    manager
        .configure(service, 0, b"never-ready\n", None)
        .unwrap();
    manager
        .configure(service, 1, b"also-not-ready\n", None)
        .unwrap();
    assert_eq!(
        manager.restore(service, 2).unwrap_err().failure,
        Failure::NotReady
    );
    assert!(fixture.control.borrow().events.is_empty());
    manager.start(service).unwrap();
    manager.configure(service, 2, b"new\n", None).unwrap();
    let before = manager.status(service).unwrap();
    assert_eq!(
        manager.restore(service, 1).unwrap_err().failure,
        Failure::Generation
    );
    assert_eq!(manager.status(service).unwrap().pid, before.pid);
    let status = manager.restore(service, 3).unwrap();
    assert!(status.active && status.restored);
    assert_eq!(status.generation, 4);
    assert_eq!(
        manager.config(service).unwrap().unwrap().bytes(),
        b"also-not-ready\n"
    );
    manager.stop(service).unwrap();
    let events = fixture.control.borrow().events.len();
    assert_eq!(
        manager.restore(service, 4).unwrap_err().failure,
        Failure::NotReady
    );
    assert_eq!(fixture.control.borrow().events.len(), events);
    assert!(!manager.status(service).unwrap().desired);
}

#[test]
fn finite_check_timeout_and_cancel_preserve_run_and_accepted_state() {
    let _lock = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let fixture = Fixture::new();
    let mut manager = fixture.manager();
    let service = ServiceId::SingBox;
    let pid = live(&mut manager, service, b"old\n");
    let manifest = fixture.manifest(service);
    for (cancel, expected) in [
        (None, ProcessError::CheckDeadline),
        (
            Some(Arc::new(AtomicBool::new(true))),
            ProcessError::Cancelled,
        ),
    ] {
        let started = Instant::now();
        assert_eq!(
            manager
                .configure(service, 1, b"hang\n", cancel)
                .unwrap_err()
                .failure,
            Failure::Process(expected)
        );
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(manager.status(service).unwrap().pid, Some(pid));
        assert_eq!(fixture.manifest(service), manifest);
        fixture.no_candidates(service);
    }
    let flag = Arc::new(AtomicBool::new(false));
    let set = flag.clone();
    let setter = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(30));
        set.store(true, Ordering::Release);
    });
    assert_eq!(
        manager
            .configure(service, 1, b"hang\n", Some(flag))
            .unwrap_err()
            .failure,
        Failure::Process(ProcessError::Cancelled)
    );
    setter.join().unwrap();
    assert_eq!(manager.status(service).unwrap().pid, Some(pid));
    assert_eq!(fixture.manifest(service), manifest);
}

#[test]
fn actual_owned_exit_is_observed_and_only_explicit_handler_withdraws_and_reaps() {
    let _lock = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let fixture = Fixture::new();
    let mut manager = fixture.manager();
    let service = ServiceId::Frpc;
    let pid = live(&mut manager, service, b"old\n");
    // Deliver finite fake input so the shell exits naturally, without signalling.
    use std::io::Write;
    let mut fifo = fs::OpenOptions::new()
        .write(true)
        .open(fixture.run(service).join("wait"))
        .unwrap();
    writeln!(fifo, "finish").unwrap();
    drop(fifo);
    finite_until(Instant::now() + Duration::from_secs(2), || {
        manager.status(service).unwrap().state == State::Error
    })
    .unwrap();
    let status = manager.status(service).unwrap();
    assert_eq!(status.pid, Some(pid));
    assert!(!status.ready && !status.active && status.needs_recovery);
    assert_eq!(
        fixture
            .control
            .borrow()
            .events
            .iter()
            .filter(|event| event.0 == HookStage::Cleanup)
            .count(),
        0
    );
    fixture.control.borrow_mut().fail_cleanup = true;
    assert_eq!(
        manager.handle_exit(service).unwrap_err().failure,
        Failure::Hook(HookStage::Cleanup, HookError::Failed)
    );
    assert_eq!(manager.status(service).unwrap().pid, Some(pid));
    fixture.control.borrow_mut().fail_cleanup = false;
    let status = manager.handle_exit(service).unwrap();
    assert!(status.pid.is_none() && status.needs_recovery && status.desired);
    assert!(!exists(pid));
}

#[test]
fn new_size_is_bounded_legacy_read_and_real_checker_prestart_integrity_are_preserved() {
    let _lock = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let fixture = Fixture::new();
    let mut manager = fixture.manager();
    let service = ServiceId::SingBox;
    assert_eq!(
        manager
            .configure(service, 0, &vec![b'x'; store::MAX_CONFIG_BYTES + 1], None)
            .unwrap_err()
            .failure,
        Failure::Store(StoreError::ConfigSize)
    );
    assert!(!fixture.run(service).join("checks").exists());
    manager.configure(service, 0, b"good\n", None).unwrap();
    fixture.control.borrow_mut().prestart_mutate_generation = Some(1);
    assert_eq!(
        manager.start(service).unwrap_err().failure,
        Failure::Process(ProcessError::Integrity)
    );
    assert!(!fixture.run(service).join("starts").exists());
    fixture.control.borrow_mut().prestart_mutate_generation = None;
    // Restore the trusted fixture bytes; no lifecycle operation for a real core.
    fs::write(
        fixture.base.join("services/sing-box/config-1.json"),
        b"good\n",
    )
    .unwrap();
    drop(manager);
    let legacy = vec![b'x'; store::MAX_CONFIG_BYTES + 9];
    fs::write(
        fixture.base.join("services/sing-box/config-1.json"),
        &legacy,
    )
    .unwrap();
    let state = serde_json::json!({"generation": 1, "current": {"generation": 1,
        "file": "config-1.json", "sha256": hash(&legacy), "ready": false}});
    fs::write(
        fixture.base.join("services/sing-box/state.json"),
        serde_json::to_vec(&state).unwrap(),
    )
    .unwrap();
    let mut manager = fixture.manager();
    assert_eq!(manager.config(service).unwrap().unwrap().bytes(), legacy);
    assert!(!manager.status(service).unwrap().desired);
    assert_eq!(
        manager.restore(service, 1).unwrap_err().failure,
        Failure::NotReady
    );
}

#[test]
fn restart_is_explicit_and_native_hook_failure_is_not_a_health_boolean() {
    let _lock = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let fixture = Fixture::new();
    let mut manager = fixture.manager();
    let service = ServiceId::Frpc;
    let pid = live(&mut manager, service, b"old\n");
    let status = manager.restart(service).unwrap();
    assert_eq!(status.generation, 1);
    assert!(status.active && status.desired);
    assert_ne!(status.pid, Some(pid));
    assert!(!exists(pid));
    manager.stop(service).unwrap();
    fixture.control.borrow_mut().ready_expired = true;
    let error = manager.start(service).unwrap_err();
    assert_eq!(
        error.failure,
        Failure::Hook(HookStage::Readiness, HookError::Deadline)
    );
    let status = manager.status(service).unwrap();
    assert!(!status.active && !status.desired && status.pid.is_none() && status.needs_recovery);
}

#[test]
fn changed_after_checker_commit_failure_recovers_unchanged_accepted_run_without_new_generation() {
    let _lock = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let fixture = Fixture::new();
    let mut manager = fixture.manager();
    let service = ServiceId::SingBox;
    let old_pid = live(&mut manager, service, b"old\n");
    let manifest = fixture.manifest(service);
    fixture.control.borrow_mut().cleanup_mutate_candidate = true;
    let error = manager.configure(service, 1, b"new\n", None).unwrap_err();
    assert_eq!(error.failure, Failure::Store(StoreError::Verification));
    assert_eq!(error.recovery_failure, None);
    let status = manager.status(service).unwrap();
    assert_eq!(status.generation, 1);
    assert!(status.active && status.restored && !status.needs_recovery);
    assert_ne!(status.pid, Some(old_pid));
    assert!(!exists(old_pid));
    assert_eq!(fixture.manifest(service), manifest);
    assert_eq!(manager.config(service).unwrap().unwrap().bytes(), b"old\n");
    fixture.no_candidates(service);
}

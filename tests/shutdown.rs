#![cfg(unix)]
use be6500_panel::{
    readiness_dns::{self, ListenerTarget, Network},
    runtime_manager::{
        ArtifactBinding, ArtifactBindings, ArtifactProvenance, HookError, Hooks, Limits, Manager,
        ServiceId,
    },
    runtime_process::Limits as ProcessLimits,
    shutdown::SignalGuard,
};
use sha2::{Digest, Sha256};
use std::{
    cell::Cell,
    fs,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::PathBuf,
    rc::Rc,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
const CORE: &str = r#"#!/bin/sh
umask 077
case "$1" in verify) exit 0;; esac
trap 'printf "term\n" >> "$TMPDIR/events"; exit 0' TERM
printf '%s\n' "$$" > "$TMPDIR/ready"
IFS= read -r value < "$TMPDIR/wait"
"#;
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!("b6p-owned-signal-{}", std::process::id()));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        for part in ["artifacts", "run", "run/frpc"] {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(path.join(part))
                .unwrap();
        }
        fs::write(path.join("artifacts/.artifact-fake"), CORE).unwrap();
        fs::set_permissions(
            path.join("artifacts/.artifact-fake"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        let fifo =
            std::ffi::CString::new(path.join("run/frpc/wait").as_os_str().as_encoded_bytes())
                .unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
struct OwnerGuard {
    manager: Manager,
    reject: Rc<Cell<bool>>,
}
impl std::ops::Deref for OwnerGuard {
    type Target = Manager;
    fn deref(&self) -> &Manager {
        &self.manager
    }
}
impl std::ops::DerefMut for OwnerGuard {
    fn deref_mut(&mut self) -> &mut Manager {
        &mut self.manager
    }
}
impl Drop for OwnerGuard {
    fn drop(&mut self) {
        self.reject.set(false);
        self.manager
            .close()
            .expect("synthetic signal fixture cleanup");
    }
}
#[test]
fn isolated_signal_owned_core_case() {
    if std::env::var_os("BE6500_SIGNAL_OWNED_CASE").is_none() {
        return;
    }
    let fixture = Fixture::new();
    let reject = Rc::new(Cell::new(false));
    let cleanup = reject.clone();
    let ready = fixture.0.join("run/frpc/ready");
    let events = fixture.0.join("run/frpc/events");
    let hooks = Hooks::new(
        |_| Ok(()),
        move |context| {
            let pid = context.run.ok_or(HookError::Failed)?.pid();
            while !fs::read_to_string(&ready).is_ok_and(|s| s.trim() == pid.to_string()) {
                if Instant::now() >= context.deadline {
                    return Err(HookError::Deadline);
                }
                std::thread::yield_now();
            }
            Ok(())
        },
        move |_| {
            if cleanup.get() {
                return Err(HookError::Failed);
            }
            use std::io::Write;
            let mut file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&events)
                .unwrap();
            writeln!(file, "cleanup").unwrap();
            Ok(())
        },
        |_| Ok(()),
    );
    let manager = Manager::open(
        fixture.0.join("services"),
        fixture.0.join("run"),
        ArtifactBindings {
            sing_box: None,
            frpc: Some(ArtifactBinding::trusted_local(
                ServiceId::Frpc,
                fixture.0.join("artifacts"),
                fixture.0.join("artifacts/.artifact-fake"),
                Sha256::digest(CORE.as_bytes()).into(),
                ArtifactProvenance::TrustedLocalModule,
            )),
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
    .unwrap();
    let mut manager = OwnerGuard {
        manager,
        reject: reject.clone(),
    };
    manager
        .configure(
            ServiceId::Frpc,
            0,
            b"serverAddr = \"example.invalid\"\n",
            None,
        )
        .unwrap();
    let pid = manager.start(ServiceId::Frpc).unwrap().pid.unwrap();
    let mut prior: libc::sigaction = unsafe { std::mem::zeroed() };
    assert_eq!(
        unsafe { libc::sigaction(libc::SIGTERM, std::ptr::null(), &mut prior) },
        0
    );
    let guard = SignalGuard::install().unwrap();
    assert!(!be6500_panel::shutdown::requested());
    assert!(SignalGuard::install().is_err());
    assert_eq!(
        unsafe { libc::kill(std::process::id() as i32, libc::SIGTERM) },
        0
    );
    let delivered = Instant::now() + Duration::from_secs(1);
    while !be6500_panel::shutdown::requested() || be6500_panel::shutdown::sequence() < 1 {
        assert!(
            Instant::now() < delivered,
            "test SIGTERM delivery exceeded deadline"
        );
        std::thread::yield_now();
    }
    assert_eq!(be6500_panel::shutdown::sequence(), 1);
    let target = ListenerTarget {
        network: Network::Tcp,
        address: "127.0.0.1:1".parse().unwrap(),
        domain: None,
    };
    assert_eq!(
        readiness_dns::probe_once(
            &target,
            Instant::now() + Duration::from_secs(1),
            Some(&AtomicBool::new(false))
        ),
        Err(readiness_dns::ReadinessError::Canceled)
    );
    reject.set(true);
    assert!(manager.close().is_err());
    assert_eq!(manager.status(ServiceId::Frpc).unwrap().pid, Some(pid));
    assert_eq!(unsafe { libc::kill(pid as i32, 0) }, 0);
    assert!(!fixture.0.join("run/frpc/events").exists());
    assert_eq!(
        manager.start(ServiceId::Frpc).unwrap_err().failure,
        be6500_panel::runtime_manager::Failure::Closed
    );
    reject.set(false);
    manager.close().unwrap();
    assert_eq!(unsafe { libc::kill(pid as i32, 0) }, -1);
    let events = fs::read_to_string(fixture.0.join("run/frpc/events")).unwrap();
    assert_eq!(events.lines().next(), Some("cleanup"));
    assert!(events.contains("term"));
    drop(guard);
    let mut restored: libc::sigaction = unsafe { std::mem::zeroed() };
    assert_eq!(
        unsafe { libc::sigaction(libc::SIGTERM, std::ptr::null(), &mut restored) },
        0
    );
    assert_eq!(restored.sa_sigaction, prior.sa_sigaction);
    assert_eq!(restored.sa_flags, prior.sa_flags);
}
#[test]
fn sigterm_cancels_admission_but_owned_cleanup_and_exact_reaping_remain_independent() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "isolated_signal_owned_core_case", "--nocapture"])
        .env("BE6500_SIGNAL_OWNED_CASE", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "isolated signal owned fixture failed: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

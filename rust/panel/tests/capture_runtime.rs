#![cfg(any(target_os = "linux", target_os = "macos"))]
use be6500_panel::capture_executor::{Binaries, TrustedBinary};
use be6500_panel::capture_kernel::table_names;
use be6500_panel::capture_plan::RulesPlanInput;
use be6500_panel::capture_runtime::CaptureRuntime;
use be6500_panel::capture_state::{CommandResult, Controller, Desired, Phase};
use be6500_panel::native::Ports;
use be6500_panel::runtime_manager::{
    ArtifactBinding, ArtifactBindings, ArtifactProvenance, HookError, Hooks, Limits, Manager,
    ServiceId,
};
use be6500_panel::runtime_process::Limits as ProcessLimits;
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, RefCell},
    fs,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::PathBuf,
    rc::Rc,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
static SERIAL: Mutex<()> = Mutex::new(());
static NEXT: AtomicU64 = AtomicU64::new(0);
const CORE: &str = r#"#!/bin/sh
umask 077
case "$1" in check|verify) exit 0;; esac
trap 'printf "term\n" >> "$TMPDIR/events"; exit 0' TERM
printf '%s\n' "$$" > "$TMPDIR/ready"
IFS= read -r value < "$TMPDIR/wait"
"#;
const COMMAND: &str = r#"#!/bin/sh
umask 077
printf '%s\n' "$*" >> "$TMPDIR/commands"
IFS= read -r mode < "$TMPDIR/mode" || :
case "$mode" in fail) exit 8;; *) exit 0;; esac
"#;
struct Fixture {
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "b6p-capture-runtime-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        for directory in ["capture", "artifacts", "exec", "run", "run/sing-box"] {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(root.join(directory))
                .unwrap();
        }
        for (name, source) in [(".artifact-core", CORE), ("fake-command", COMMAND)] {
            fs::write(root.join("artifacts").join(name), source).unwrap();
            fs::set_permissions(
                root.join("artifacts").join(name),
                fs::Permissions::from_mode(0o700),
            )
            .unwrap();
        }
        fs::write(root.join("exec/mode"), b"success").unwrap();
        let fifo = std::ffi::CString::new(
            root.join("run/sing-box/wait")
                .as_os_str()
                .as_encoded_bytes(),
        )
        .unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        Self { root }
    }
    fn capture(&self, controller: Controller, built: Rc<Cell<usize>>) -> CaptureRuntime {
        let command = self.root.join("artifacts/fake-command");
        let hash = Sha256::digest(COMMAND.as_bytes()).into();
        CaptureRuntime::new(
            controller,
            Binaries {
                ip: TrustedBinary::admit(&command, hash).unwrap(),
                iptables: TrustedBinary::admit(&command, hash).unwrap(),
            },
            self.root.join("exec"),
            table_names(b"").unwrap(),
            move |_, _, _| {
                built.set(built.get() + 1);
                Err(HookError::Failed)
            },
        )
    }
    fn manager(&self, capture: Rc<RefCell<CaptureRuntime>>) -> Guard {
        let cleanup = capture.clone();
        let restore = capture;
        let root = self.root.clone();
        let hooks = Hooks::new(
            |_| Ok(()),
            move |context| {
                let pid = context.run.ok_or(HookError::Failed)?.pid();
                let marker = root.join("run/sing-box/ready");
                loop {
                    if fs::read_to_string(&marker).is_ok_and(|text| text.trim() == pid.to_string())
                    {
                        return Ok(());
                    }
                    if Instant::now() >= context.deadline {
                        return Err(HookError::Deadline);
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            },
            move |context| cleanup.borrow_mut().cleanup(context),
            move |context| restore.borrow_mut().restore(context),
        );
        let binding = ArtifactBinding::trusted_local(
            ServiceId::SingBox,
            self.root.join("artifacts"),
            self.root.join("artifacts/.artifact-core"),
            Sha256::digest(CORE.as_bytes()).into(),
            ArtifactProvenance::TrustedLocalModule,
        );
        Guard {
            manager: Manager::open(
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
                    resource_timeout: Duration::from_secs(10),
                },
            )
            .unwrap(),
            mode: self.root.join("exec/mode"),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
struct Guard {
    manager: Manager,
    mode: PathBuf,
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
        fs::write(&self.mode, b"success").unwrap();
        self.manager.close().expect("fake capture runtime cleanup");
    }
}
fn input() -> RulesPlanInput {
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
    }
}
#[test]
fn off_capture_never_builds_or_executes_from_runtime_start() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let built = Rc::new(Cell::new(0));
    let capture = Rc::new(RefCell::new(fixture.capture(
        Controller::open(fixture.root.join("capture")).unwrap(),
        built.clone(),
    )));
    let mut manager = fixture.manager(capture.clone());
    assert!(!fixture.root.join("exec/commands").exists());
    manager
        .configure(ServiceId::SingBox, 0, b"synthetic fake core config\n", None)
        .unwrap();
    manager.start(ServiceId::SingBox).unwrap();
    manager.stop(ServiceId::SingBox).unwrap();
    assert_eq!(built.get(), 0);
    assert!(!fixture.root.join("exec/commands").exists());
    assert_eq!(capture.borrow().status().phase, Phase::Off);
}
#[test]
fn recovered_compiled_cleanup_uses_executor_and_blocks_stop_on_failure() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let directory = fixture.root.join("capture");
    let mut controller = Controller::open(&directory).unwrap();
    controller
        .set_desired(Desired {
            scope: "gateway".into(),
            lan_ipv4_prefixes: vec!["192.168.50.0/24".into()],
            desired: true,
            ..Desired::default()
        })
        .unwrap();
    controller
        .apply(
            input(),
            |_, _, _| Ok(()),
            |_, _| Ok(CommandResult::success()),
        )
        .unwrap();
    controller
        .set_desired(Desired {
            scope: "gateway".into(),
            lan_ipv4_prefixes: vec!["192.168.50.0/24".into()],
            desired: false,
            ..Desired::default()
        })
        .unwrap();
    drop(controller);
    let built = Rc::new(Cell::new(0));
    let capture = Rc::new(RefCell::new(
        fixture.capture(Controller::open(&directory).unwrap(), built.clone()),
    ));
    let mut manager = fixture.manager(capture.clone());
    manager
        .configure(ServiceId::SingBox, 0, b"synthetic fake config\n", None)
        .unwrap();
    let pid = manager.start(ServiceId::SingBox).unwrap().pid.unwrap();
    fs::write(fixture.root.join("exec/mode"), b"fail").unwrap();
    assert!(manager.stop(ServiceId::SingBox).is_err());
    assert_eq!(manager.status(ServiceId::SingBox).unwrap().pid, Some(pid));
    assert_eq!(unsafe { libc::kill(pid as libc::pid_t, 0) }, 0);
    assert!(directory.join("capture-journal.json").exists());
    assert!(!fixture.root.join("run/sing-box/events").exists());
    fs::write(fixture.root.join("exec/mode"), b"success").unwrap();
    manager.stop(ServiceId::SingBox).unwrap();
    assert!(manager.status(ServiceId::SingBox).unwrap().pid.is_none());
    assert_eq!(capture.borrow().status().phase, Phase::Off);
    assert!(!directory.join("capture-journal.json").exists());
    let commands = fs::read_to_string(fixture.root.join("exec/commands")).unwrap();
    assert!(commands.lines().all(|line| line.contains(" -D ")
        || line.contains(" -F ")
        || line.contains(" -X ")
        || line.contains(" del ")
        || line.starts_with("-4 rule del")));
    assert_eq!(built.get(), 0);
}

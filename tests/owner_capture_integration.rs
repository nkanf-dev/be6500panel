#![cfg(any(target_os = "linux", target_os = "macos"))]
use be6500_panel::capture_plan::{OwnedRulesPlan, RulesPlanInput};
use be6500_panel::capture_state::{
    CommandError, CommandResult, Controller, Desired, DeviceSelection, Phase,
};
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
    rc::Rc,
    time::{Duration, Instant},
};
struct FakeManagerGuard {
    manager: Manager,
    reject: Rc<Cell<bool>>,
}
impl std::ops::Deref for FakeManagerGuard {
    type Target = Manager;
    fn deref(&self) -> &Manager {
        &self.manager
    }
}
impl std::ops::DerefMut for FakeManagerGuard {
    fn deref_mut(&mut self) -> &mut Manager {
        &mut self.manager
    }
}
impl Drop for FakeManagerGuard {
    fn drop(&mut self) {
        self.reject.set(false);
        self.manager.close().expect("fake joint manager cleanup");
    }
}
const HELPER: &str = r#"#!/bin/sh
umask 077
config="$2"
case "$1" in run|check|verify) config="$3";; esac
case "$1" in check|verify) exit 0;; esac
trap 'printf "term\n" >> "$TMPDIR/events"; exit 0' TERM
printf '%s\n' "$$" > "$TMPDIR/ready"
IFS= read -r value < "$TMPDIR/wait"
"#;
#[test]
fn retained_capture_cleanup_gates_exact_owned_process_stop() {
    let root = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!("be6500-joint-owner-capture-{}", std::process::id()));
    assert!(!root.exists());
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    for directory in ["capture", "artifacts", "run", "run/sing-box"] {
        fs::DirBuilder::new()
            .mode(0o700)
            .create(root.join(directory))
            .unwrap();
    }
    let artifact = root.join("artifacts/.artifact-fake");
    fs::write(&artifact, HELPER).unwrap();
    fs::set_permissions(&artifact, fs::Permissions::from_mode(0o700)).unwrap();
    let fifo = std::ffi::CString::new(
        root.join("run/sing-box/wait")
            .as_os_str()
            .as_encoded_bytes(),
    )
    .unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    let controller = Rc::new(RefCell::new(
        Controller::open(root.join("capture")).unwrap(),
    ));
    controller
        .borrow_mut()
        .set_desired(Desired {
            desired: true,
            devices: vec![DeviceSelection {
                mac: "02:aa:bb:cc:dd:ee".into(),
            }],
            ..Desired::default()
        })
        .unwrap();
    let input = RulesPlanInput {
        datapath: "routed-tun".into(),
        tun_interface: "b6p-test".into(),
        tun_address: "172.30.0.1/30".into(),
        client_ipv4: "192.168.50.10".into(),
        client_macs: Some([("192.168.50.10".into(), "02:aa:bb:cc:dd:ee".into())].into()),
        lan_interface: "br-lan".into(),
        ipv6: "direct".into(),
        failure: "direct".into(),
        ports: Ports {
            mixed: 2080,
            tproxy: 7893,
            dns: 1053,
        },
        ..RulesPlanInput::default()
    };
    controller
        .borrow_mut()
        .apply(
            input,
            |_: &RulesPlanInput, _: &OwnedRulesPlan, _| Ok(()),
            |_, _| Ok(CommandResult::success()),
        )
        .unwrap();
    assert_eq!(controller.borrow().status().phase, Phase::ActiveByApply);
    let reject = Rc::new(Cell::new(true));
    let commands = Rc::new(RefCell::new(Vec::<Vec<String>>::new()));
    let cleanup_controller = controller.clone();
    let cleanup_reject = reject.clone();
    let cleanup_commands = commands.clone();
    let ready_root = root.clone();
    let hooks = Hooks::new(
        |_| Ok(()),
        move |context| {
            let identity = context.run.ok_or(HookError::Failed)?;
            let marker = ready_root.join("run/sing-box/ready");
            while !fs::read_to_string(&marker)
                .is_ok_and(|value| value.trim() == identity.pid().to_string())
            {
                if Instant::now() >= context.deadline {
                    return Err(HookError::Deadline);
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(())
        },
        move |_| {
            cleanup_controller
                .borrow_mut()
                .cleanup(|argv, _| {
                    cleanup_commands.borrow_mut().push(argv.to_vec());
                    if cleanup_reject.get() {
                        Err(CommandError::Failure)
                    } else {
                        Ok(CommandResult::success())
                    }
                })
                .map_err(|_| HookError::Failed)
        },
        |_| Ok(()),
    );
    let binding = ArtifactBinding::trusted_local(
        ServiceId::SingBox,
        root.join("artifacts"),
        &artifact,
        Sha256::digest(HELPER.as_bytes()).into(),
        ArtifactProvenance::TrustedLocalModule,
    );
    let limits = Limits {
        process: ProcessLimits {
            term_grace: Duration::from_millis(100),
            kill_grace: Duration::from_secs(1),
            check_timeout: Duration::from_secs(1),
        },
        readiness_timeout: Duration::from_secs(2),
        resource_timeout: Duration::from_secs(1),
    };
    let manager = Manager::open(
        root.join("services"),
        root.join("run"),
        ArtifactBindings {
            sing_box: Some(binding),
            frpc: None,
        },
        hooks,
        limits,
    )
    .unwrap();
    let mut manager = FakeManagerGuard {
        manager,
        reject: reject.clone(),
    };
    let status = manager
        .configure(ServiceId::SingBox, 0, b"synthetic-config\n", None)
        .unwrap();
    assert_eq!(status.generation, 1);
    let running = manager.start(ServiceId::SingBox).unwrap();
    let pid = running.pid.unwrap();
    assert!(manager.stop(ServiceId::SingBox).is_err());
    assert_eq!(manager.status(ServiceId::SingBox).unwrap().pid, Some(pid));
    assert_eq!(unsafe { libc::kill(pid as libc::pid_t, 0) }, 0);
    assert!(!root.join("run/sing-box/events").exists());
    assert!(root.join("capture/capture-journal.json").exists());
    assert_eq!(controller.borrow().status().phase, Phase::CleanupPending);
    reject.set(false);
    manager.stop(ServiceId::SingBox).unwrap();
    assert!(manager.status(ServiceId::SingBox).unwrap().pid.is_none());
    assert_eq!(controller.borrow().status().phase, Phase::Off);
    assert!(!root.join("capture/capture-journal.json").exists());
    assert!(!commands.borrow().is_empty());
    manager.close().unwrap();
    drop(manager);
    drop(controller);
    fs::remove_dir_all(root).unwrap();
}

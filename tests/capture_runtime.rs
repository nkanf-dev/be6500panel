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

#[test]
fn off_capture_native_observer_seam_has_no_constructor_or_lifecycle_actions() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let command = fixture.root.join("artifacts/fake-command");
    let hash = Sha256::digest(COMMAND.as_bytes()).into();
    let capture = Rc::new(RefCell::new(CaptureRuntime::with_observer(
        Controller::open(fixture.root.join("capture")).unwrap(),
        Binaries {
            ip: TrustedBinary::admit(&command, hash).unwrap(),
            iptables: TrustedBinary::admit(&command, hash).unwrap(),
        },
        fixture.root.join("exec"),
        table_names(b"").unwrap(),
        be6500_panel::readiness_tun::NativeObserver::with_proc_root(
            fixture.root.join("missing-native-source"),
        ),
    )));
    assert!(!fixture.root.join("exec/commands").exists());
    let mut manager = fixture.manager(capture.clone());
    manager
        .configure(
            ServiceId::SingBox,
            0,
            b"not native configuration: off means no parse\n",
            None,
        )
        .unwrap();
    assert!(manager.start(ServiceId::SingBox).unwrap().active);
    manager.stop(ServiceId::SingBox).unwrap();
    assert_eq!(capture.borrow().status().phase, Phase::Off);
    assert!(!fixture.root.join("exec/commands").exists());
    assert!(!fixture.root.join("missing-native-source").exists());
}

#[test]
fn explicit_startup_withdrawal_uses_only_validated_cleanup_without_core_or_builder() {
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
    drop(controller);
    let built = Rc::new(Cell::new(0));
    let mut capture = fixture.capture(Controller::open(&directory).unwrap(), built.clone());
    assert_eq!(capture.status().phase, Phase::Staged);
    assert!(!fixture.root.join("exec/commands").exists());
    assert!(directory.join("capture-journal.json").exists());
    fs::write(fixture.root.join("exec/mode"), b"fail").unwrap();
    assert!(
        capture
            .startup_withdraw(Instant::now() + Duration::from_secs(10))
            .is_err()
    );
    assert_eq!(capture.status().phase, Phase::CleanupPending);
    assert!(directory.join("capture-journal.json").exists());
    assert_eq!(built.get(), 0);
    fs::write(fixture.root.join("exec/mode"), b"success").unwrap();
    capture
        .startup_withdraw(Instant::now() + Duration::from_secs(10))
        .unwrap();
    assert_eq!(capture.status().phase, Phase::Off);
    assert!(capture.desired().desired);
    assert!(!directory.join("capture-journal.json").exists());
    assert!(!fixture.root.join("run/sing-box/ready").exists());
    assert_eq!(built.get(), 0);
    let commands = fs::read_to_string(fixture.root.join("exec/commands")).unwrap();
    assert!(commands.lines().all(|line| line.contains(" -D ")
        || line.contains(" -F ")
        || line.contains(" -X ")
        || line.contains(" del ")
        || line.starts_with("-4 rule del")));
    let before = commands.len();
    capture
        .startup_withdraw(Instant::now() + Duration::from_secs(1))
        .unwrap();
    assert_eq!(
        fs::read_to_string(fixture.root.join("exec/commands"))
            .unwrap()
            .len(),
        before
    );
}
#[test]
fn off_without_journal_startup_withdrawal_is_zero_commands_even_with_expired_budget() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let built = Rc::new(Cell::new(0));
    let mut capture = fixture.capture(
        Controller::open(fixture.root.join("capture")).unwrap(),
        built.clone(),
    );
    capture.startup_withdraw(Instant::now()).unwrap();
    assert_eq!(built.get(), 0);
    assert!(!fixture.root.join("exec/commands").exists());
    assert_eq!(capture.status().phase, Phase::Off);
}

fn proof_input() -> RulesPlanInput {
    let mut input = input();
    input.management_ips = vec!["192.168.50.1".into()];
    input
}
fn proof_config() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({"inbounds":[{"type":"mixed","tag":"mixed-in","listen":"127.0.0.1","listen_port":2080},{"type":"tun","tag":"tun-in","interface_name":"b6p-test","address":["172.30.0.1/30"],"mtu":1500,"stack":"system","dns_mode":"disabled","auto_route":false,"auto_redirect":false,"udp_timeout":"2m","udp_nat_max":1024},{"type":"direct","tag":"dns-in","listen":"192.168.50.1","listen_port":1053}],"route":{"rules":[{"inbound":["dns-in"],"action":"hijack-dns"},{"ip_version":6,"outbound":"direct"}]}})).unwrap()
}
fn quote_shell(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
fn kernel_proof_command(input: &RulesPlanInput) -> String {
    let plan = be6500_panel::capture_plan::plan_owned_rules(input).unwrap();
    let mut source = String::from(
        "#!/bin/sh\numask 077\nprintf '%s\\n' \"$*\" >> \"$TMPDIR/commands\"\nIFS= read -r mode < \"$TMPDIR/mode\" || :\ncase \"$mode\" in fail) exit 8;; esac\n",
    );
    let last = plan.apply.last().unwrap()[1..].join(" ");
    source.push_str(&format!(
        "case \"$*\" in {}) printf 'installed\\n' > \"$TMPDIR/mode\"; exit 0;; esac\n",
        quote_shell(&last)
    ));
    source.push_str("if [ \"$mode\" = preflight ]; then\ncase \"$*\" in\n");
    source.push_str("'-4 route show table all') printf '%s\\n' '172.30.0.0/30 dev b6p-test proto kernel scope link src 172.30.0.1' 'local 172.30.0.1 dev b6p-test table local proto kernel scope host'; exit 0;;\n'-4 route show table 16500') printf '%s\\n' 'Error: ipv4: FIB table does not exist.'; exit 1;;\n'-4 rule show') printf '%s\\n' '0: from all lookup local' '32766: from all lookup main'; exit 0;;\n'-w 5 -t mangle -S') printf '%s\\n' '-P PREROUTING ACCEPT'; exit 0;;\n");
    for chain in &plan.ownership.chains {
        source.push_str(&format!(
            "{}) printf '%s\\n' 'iptables: No chain/target/match by that name.'; exit 1;;\n",
            quote_shell(&format!("-w 5 -t {} -S {}", chain.table, chain.name))
        ));
    }
    source.push_str("esac\nfi\nif [ \"$mode\" = installed ]; then\ncase \"$*\" in\n'-4 route show table 16500') printf '%s\\n' 'default dev b6p-test proto static scope link'; exit 0;;\n'-4 rule show') printf '%s\\n' '0: from all lookup local' '16500: from 192.168.50.0/24 iif br-lan fwmark 0x4000/0x4000 lookup 16500' '32766: from all lookup main'; exit 0;;\n");
    let mut seen = std::collections::BTreeSet::new();
    for chain in &plan.ownership.chains {
        let mut rows = vec![format!("-N {}", chain.name)];
        for argv in &plan.apply {
            if argv.len() > 6
                && argv[0] == "iptables"
                && argv[4] == chain.table
                && argv[5] == "-A"
                && argv[6] == chain.name
            {
                rows.push(argv[5..].join(" "));
            }
        }
        source.push_str(&format!(
            "{}) printf '%s\\n' {}; exit 0;;\n",
            quote_shell(&format!("-w 5 -t {} -S {}", chain.table, chain.name)),
            rows.iter()
                .map(|row| quote_shell(row))
                .collect::<Vec<_>>()
                .join(" ")
        ));
        if seen.insert((chain.table.clone(), chain.hook.clone())) {
            let mut rows = vec![format!("-P {} ACCEPT", chain.hook)];
            for argv in plan.apply.iter().rev() {
                if argv.len() > 8
                    && argv[0] == "iptables"
                    && argv[4] == chain.table
                    && argv[5] == "-I"
                    && argv[6] == chain.hook
                {
                    let mut row = argv[5..].to_vec();
                    row[0] = "-A".into();
                    row.remove(2);
                    rows.push(row.join(" "));
                }
            }
            source.push_str(&format!(
                "{}) printf '%s\\n' {}; exit 0;;\n",
                quote_shell(&format!("-w 5 -t {} -S {}", chain.table, chain.hook)),
                rows.iter()
                    .map(|row| quote_shell(row))
                    .collect::<Vec<_>>()
                    .join(" ")
            ));
        }
    }
    source.push_str("esac\nfi\nexit 0\n");
    source
}
#[test]
fn current_capture_requires_live_origin_and_exact_readonly_queries_and_preserves_cleanup_failure() {
    use be6500_panel::capture_runtime::CurrentState;
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
    let input = proof_input();
    let source = kernel_proof_command(&input);
    let command = fixture.root.join("artifacts/proof-command");
    fs::write(&command, &source).unwrap();
    fs::set_permissions(&command, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(fixture.root.join("exec/mode"), b"preflight").unwrap();
    let digest = Sha256::digest(source.as_bytes()).into();
    let changed = Rc::new(Cell::new(false));
    let current_scope = changed.clone();
    let capture = Rc::new(RefCell::new(CaptureRuntime::new(
        controller,
        Binaries {
            ip: TrustedBinary::admit(&command, digest).unwrap(),
            iptables: TrustedBinary::admit(&command, digest).unwrap(),
        },
        fixture.root.join("exec"),
        table_names(b"").unwrap(),
        move |_, _, _| {
            let mut fresh = proof_input();
            if current_scope.get() {
                fresh.endpoint_ips.push("203.0.113.9".into());
            }
            Ok(fresh)
        },
    )));
    assert_eq!(
        capture.borrow().current_unobserved().state,
        CurrentState::Suspended
    );
    let mut manager = fixture.manager(capture.clone());
    manager
        .configure(ServiceId::SingBox, 0, &proof_config(), None)
        .unwrap();
    manager.start(ServiceId::SingBox).unwrap();
    assert_eq!(capture.borrow().status().phase, Phase::ActiveByApply);
    assert!(!capture.borrow().current_unobserved().active);
    let journal = fs::read(directory.join("capture-journal.json")).unwrap();
    let before = fs::read(fixture.root.join("exec/commands")).unwrap().len();
    // The callback seam supplies test-only current native proof; native Runtime
    // identity/DNS failure and success are separately tested with actual owner.
    let state = manager
        .observe_current(
            ServiceId::SingBox,
            Instant::now() + Duration::from_secs(2),
            |context| Ok(capture.borrow_mut().observe_current(context, |_| Ok(()))),
        )
        .unwrap();
    assert_eq!(state.state, CurrentState::Active);
    assert!(state.active);
    let text = fs::read_to_string(fixture.root.join("exec/commands")).unwrap();
    assert!(
        text[before..].lines().all(|line| line.contains(" -S ")
            || line.contains(" show ")
            || line.ends_with(" show"))
    );
    assert_eq!(
        fs::read(directory.join("capture-journal.json")).unwrap(),
        journal
    );
    let query_count = text.len();
    let state = manager
        .observe_current(
            ServiceId::SingBox,
            Instant::now() + Duration::from_secs(1),
            |context| {
                Ok(capture
                    .borrow_mut()
                    .observe_current(context, |_| Err(HookError::Failed)))
            },
        )
        .unwrap();
    assert_eq!(state.state, CurrentState::Unknown);
    assert_eq!(
        fs::read(fixture.root.join("exec/commands")).unwrap().len(),
        query_count
    );
    changed.set(true);
    let state = manager
        .observe_current(
            ServiceId::SingBox,
            Instant::now() + Duration::from_secs(1),
            |context| Ok(capture.borrow_mut().observe_current(context, |_| Ok(()))),
        )
        .unwrap();
    assert_eq!(state.state, CurrentState::ScopeChanged);
    assert_eq!(
        fs::read(fixture.root.join("exec/commands")).unwrap().len(),
        query_count
    );
    changed.set(false);
    fs::write(fixture.root.join("exec/mode"), b"fail").unwrap();
    let state = manager
        .observe_current(
            ServiceId::SingBox,
            Instant::now() + Duration::from_secs(1),
            |context| Ok(capture.borrow_mut().observe_current(context, |_| Ok(()))),
        )
        .unwrap();
    assert_eq!(state.state, CurrentState::Unknown);
    assert_eq!(capture.borrow().status().phase, Phase::ActiveByApply);
    assert_eq!(
        fs::read(directory.join("capture-journal.json")).unwrap(),
        journal
    );
    assert!(manager.stop(ServiceId::SingBox).is_err());
    let capture_status = capture.borrow().current_unobserved();
    assert_eq!(capture_status.state, CurrentState::CleanupPending);
    assert!(!capture_status.active);
    assert!(capture_status.intent.cleanup_pending);
    assert!(directory.join("capture-journal.json").exists());
    fs::write(fixture.root.join("exec/mode"), b"success").unwrap();
    manager.stop(ServiceId::SingBox).unwrap();
    assert_eq!(
        capture.borrow().current_unobserved().state,
        CurrentState::Suspended
    );
}

#[test]
fn shared_capture_handle_keeps_one_controller_and_explicit_cleanup_authority() {
    use be6500_panel::{capture_runtime::CurrentState, native_runtime::NativeReadiness};
    use std::sync::atomic::AtomicBool;
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
    drop(controller);
    let built = Rc::new(Cell::new(0));
    let capture = fixture.capture(Controller::open(&directory).unwrap(), built.clone());
    let readiness = NativeReadiness::with_observer(
        be6500_panel::readiness_tun::NativeObserver::with_proc_root(
            fixture.root.join("unavailable-native"),
        ),
        Rc::new(AtomicBool::new(false)),
    );
    let (hooks, handle) = capture.into_hooks_with_handle(readiness);
    let same = handle.clone();
    assert_eq!(
        handle.current_unobserved().unwrap().state,
        CurrentState::Staged
    );
    assert!(!fixture.root.join("exec/commands").exists());
    assert_eq!(built.get(), 0);
    fs::write(fixture.root.join("exec/mode"), b"fail").unwrap();
    assert!(
        same.startup_withdraw(Instant::now() + Duration::from_secs(5))
            .is_err()
    );
    assert_eq!(
        handle.current_unobserved().unwrap().state,
        CurrentState::CleanupPending
    );
    assert!(directory.join("capture-journal.json").exists());
    fs::write(fixture.root.join("exec/mode"), b"success").unwrap();
    handle
        .startup_withdraw(Instant::now() + Duration::from_secs(5))
        .unwrap();
    assert_eq!(
        same.current_unobserved().unwrap().state,
        CurrentState::Suspended
    );
    assert!(!directory.join("capture-journal.json").exists());
    assert_eq!(built.get(), 0);
    assert!(!fixture.root.join("unavailable-native").exists());
    drop(hooks);
}

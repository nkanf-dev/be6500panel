#![cfg(unix)]
use be6500_panel::capture_state::{Controller, Desired, DeviceSelection, Phase};
use std::fs::{self, DirBuilder, Permissions};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let p = std::fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "be6500-capture-state-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        DirBuilder::new().mode(0o700).create(&p).unwrap();
        Self(p)
    }
    fn open(&self) -> Controller {
        Controller::open(&self.0).unwrap()
    }
    fn write(&self, name: &str, raw: &[u8]) {
        private_write(&self.0.join(name), raw);
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn private_write(p: &Path, raw: &[u8]) {
    fs::write(p, raw).unwrap();
    fs::set_permissions(p, Permissions::from_mode(0o600)).unwrap();
}
#[test]
fn constructor_read_only_missing_is_off_and_preserves_unrelated_data() {
    let f = Fixture::new();
    f.write("emergency-reserve", b"untouched");
    let c = f.open();
    assert!(!c.desired().desired);
    assert_eq!(c.status().phase, Phase::Off);
    assert_eq!(fs::read_dir(&f.0).unwrap().count(), 1);
    assert_eq!(
        fs::read(f.0.join("emergency-reserve")).unwrap(),
        b"untouched"
    );
}
#[test]
fn durable_off_gateway_and_normalized_off_device_selection() {
    let f = Fixture::new();
    let mut c = f.open();
    let d = Desired {
        scope: "gateway".into(),
        lan_ipv4_prefixes: vec!["192.168.31.0/24".into(), "10.0.0.0/24".into()],
        ..Desired::default()
    };
    c.set_desired(d).unwrap();
    assert_eq!(
        c.desired().lan_ipv4_prefixes,
        ["10.0.0.0/24", "192.168.31.0/24"]
    );
    assert_eq!(f.open().desired(), c.desired());
    c.set_desired(Desired {
        devices: vec![DeviceSelection {
            mac: "02-AA-BB-CC-DD-EE".into(),
        }],
        ..Desired::default()
    })
    .unwrap();
    assert_eq!(f.open().desired().devices[0].mac, "02:aa:bb:cc:dd:ee");
    assert!(!f.open().desired().desired);
}
#[test]
fn malformed_duplicate_unsupported_and_oversized_desired_are_not_reset() {
    for raw in [
        b"{".as_slice(),
        br#"{"desired":false,"desired":true}"#,
        br#"{"desired":true,"devices":[{"mac":"02:aa:bb:cc:dd:ee"}],"ipv6":"block"}"#,
        br#"{"desired":false,"lanInterface":"br-lan"}"#,
    ] {
        let f = Fixture::new();
        f.write("capture-desired.json", raw);
        assert!(Controller::open(&f.0).is_err());
        assert_eq!(fs::read(f.0.join("capture-desired.json")).unwrap(), raw);
    }
    let f = Fixture::new();
    f.write("capture-desired.json", &vec![b' '; (64 << 10) + 1]);
    assert!(Controller::open(&f.0).is_err());
}

use be6500_panel::capture_plan::{OwnedRulesPlan, RulesPlanInput, plan_owned_rules};
use be6500_panel::capture_state::{
    CommandError, CommandResult, Error, MAX_JOURNAL_BYTES, Phase as CapturePhase, PreflightError,
};
use be6500_panel::native::Ports;
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::os::unix::fs::{MetadataExt, symlink};
fn input() -> RulesPlanInput {
    RulesPlanInput {
        datapath: "routed-tun".into(),
        tun_interface: "b6p-tun0".into(),
        tun_address: "172.30.0.1/30".into(),
        client_ipv4: "192.168.31.10".into(),
        client_macs: Some([("192.168.31.10".into(), "02:aa:bb:cc:dd:ee".into())].into()),
        lan_interface: "br-lan".into(),
        ipv6: "direct".into(),
        failure: "direct".into(),
        ports: Ports {
            mixed: 2080,
            tproxy: 7893,
            dns: 1053,
        },
        endpoint_ips: vec!["203.0.113.10".into()],
        management_ips: vec!["192.168.31.1".into()],
        ..RulesPlanInput::default()
    }
}
fn enable(c: &mut Controller) {
    c.set_desired(Desired {
        desired: true,
        devices: vec![DeviceSelection {
            mac: "02:aa:bb:cc:dd:ee".into(),
        }],
        ..Desired::default()
    })
    .unwrap();
}
fn success(_: &[String], _: std::time::Instant) -> Result<CommandResult, CommandError> {
    Ok(CommandResult::success())
}
fn preflight(
    _: &RulesPlanInput,
    _: &OwnedRulesPlan,
    _: std::time::Instant,
) -> Result<(), PreflightError> {
    Ok(())
}
fn journal(i: &RulesPlanInput, with_input: bool) -> Value {
    let mut j = serde_json::to_value(plan_owned_rules(i).unwrap()).unwrap();
    if with_input {
        j["input"] = serde_json::to_value(i).unwrap();
    }
    j
}
fn install_journal(f: &Fixture, j: &Value) {
    f.write("capture-journal.json", &serde_json::to_vec(j).unwrap());
}
#[test]
fn startup_journal_is_staged_pending_never_active_and_snapshot_owns_input() {
    let f = Fixture::new();
    let i = input();
    install_journal(&f, &journal(&i, true));
    let c = f.open();
    assert_eq!(c.status().phase, CapturePhase::Staged);
    assert!(c.status().cleanup_pending);
    assert!(!c.desired().desired);
    assert_eq!(c.snapshot().input, Some(i));
    let before = fs::read(f.0.join("capture-journal.json")).unwrap();
    let _ = c.status();
    let _ = c.snapshot();
    let _ = c.desired();
    drop(c);
    assert_eq!(fs::read(f.0.join("capture-journal.json")).unwrap(), before);
}
#[test]
fn unsafe_stored_apply_is_ignored_and_cleanup_is_exact_regenerated() {
    let f = Fixture::new();
    let i = input();
    let p = plan_owned_rules(&i).unwrap();
    let mut j = journal(&i, true);
    j["Apply"] = json!([["sh", "-c", "UNTRUSTED"], ["iptables", "-F"]]);
    j["OnFailure"] = json!({"any": "not execution authority"});
    install_journal(&f, &j);
    let mut c = f.open();
    let mut ran = vec![];
    c.cleanup(|argv, _| {
        ran.push(argv.to_vec());
        Ok(CommandResult::success())
    })
    .unwrap();
    assert_eq!(ran, p.cleanup);
    assert_eq!(c.status().phase, CapturePhase::Off);
    assert!(!f.0.join("capture-journal.json").exists());
}
#[test]
fn tampered_cleanup_ownership_or_duplicate_map_journal_refused_and_retained() {
    for key in ["cleanup", "chain", "mark", "input"] {
        let f = Fixture::new();
        let mut j = journal(&input(), true);
        match key {
            "cleanup" => j["Cleanup"][0][0] = json!("sh"),
            "chain" => j["Ownership"]["Chains"][0]["Name"] = json!("OTHER_USER_CHAIN"),
            "mark" => j["Ownership"]["Mark"] = json!(123),
            _ => j["input"]["lanInterface"] = json!("other-lan"),
        }
        install_journal(&f, &j);
        let before = fs::read(f.0.join("capture-journal.json")).unwrap();
        assert_eq!(Controller::open(&f.0).unwrap_err(), Error::InvalidJournal);
        assert_eq!(fs::read(f.0.join("capture-journal.json")).unwrap(), before);
    }
    let f = Fixture::new();
    let raw = serde_json::to_string(&journal(&input(), true)).unwrap().replace("\"clientMACs\":{\"192.168.31.10\":\"02:aa:bb:cc:dd:ee\"}", "\"clientMACs\":{\"192.168.31.10\":\"02:aa:bb:cc:dd:ee\",\"192.168.31.10\":\"02:aa:bb:cc:dd:ee\"}");
    f.write("capture-journal.json", raw.as_bytes());
    assert_eq!(Controller::open(&f.0).unwrap_err(), Error::InvalidJournal);
}
#[test]
fn ownership_only_device_and_gateway_cleanup_placeholder_compatibility() {
    let mut gateway = input();
    gateway.scope = "gateway".into();
    gateway.client_ipv4.clear();
    gateway.client_macs = None;
    gateway.lan_ipv4_prefixes = vec!["192.168.31.0/24".into()];
    gateway.ports.dns = 15353;
    for i in [input(), gateway] {
        let f = Fixture::new();
        let p = plan_owned_rules(&i).unwrap();
        install_journal(&f, &journal(&i, false));
        let mut c = f.open();
        assert!(c.snapshot().input.is_none());
        let mut ran = vec![];
        c.cleanup(|argv, _| {
            ran.push(argv.to_vec());
            Ok(CommandResult::success())
        })
        .unwrap();
        assert_eq!(ran, p.cleanup);
    }
}
#[test]
fn go_pascal_input_and_lower_camel_ownership_aliases_restore_without_execution() {
    let f = Fixture::new();
    let i = input();
    let mut j = journal(&i, true);
    let input_object = j["input"].as_object_mut().unwrap();
    let renames = [
        ("scope", "Scope"),
        ("lanIPv4Prefixes", "LANIPv4Prefixes"),
        ("datapath", "Datapath"),
        ("tunInterface", "TUNInterface"),
        ("tunAddress", "TUNAddress"),
        ("clientIPv4", "ClientIPv4"),
        ("clientIPv6", "ClientIPv6"),
        ("clientIPv4s", "ClientIPv4s"),
        ("clientIPv6s", "ClientIPv6s"),
        ("clientMACs", "ClientMACs"),
        ("lanInterface", "LANInterface"),
        ("ports", "Ports"),
        ("ipv6", "IPv6"),
        ("failure", "Failure"),
        ("endpointIPs", "EndpointIPs"),
        ("managementIPs", "ManagementIPs"),
        ("routerDNSAddresses", "RouterDNSAddresses"),
        ("fakeIP", "FakeIP"),
    ];
    for (lower, upper) in renames {
        let v = input_object.remove(lower).unwrap();
        input_object.insert(upper.into(), v);
    }
    j["input"]["Ports"] = json!({"Mixed":2080,"TProxy":7893,"DNS":1053});
    let ownership = j["Ownership"].as_object_mut().unwrap();
    for (upper, lower) in [
        ("Datapath", "datapath"),
        ("TUNInterface", "tunInterface"),
        ("TUNAddress", "tunAddress"),
        ("Mark", "mark"),
        ("Mask", "mask"),
        ("RouteTable", "routeTable"),
        ("RulePriority", "rulePriority"),
        ("LANInterface", "lanInterface"),
        ("ClientIPv4", "clientIPv4"),
        ("ClientIPv6", "clientIPv6"),
        ("ClientMACs", "clientMACs"),
        ("RouteFamilies", "routeFamilies"),
        ("Chains", "chains"),
    ] {
        let v = ownership.remove(upper).unwrap();
        ownership.insert(lower.into(), v);
    }
    install_journal(&f, &j);
    let c = f.open();
    assert_eq!(c.snapshot().input, Some(i));
    assert_eq!(c.status().phase, CapturePhase::Staged);
}
#[test]
fn tproxy_journal_is_fixed_refused_and_retained_for_migration() {
    let f = Fixture::new();
    let mut j = journal(&input(), true);
    j["Ownership"]["Datapath"] = json!("tproxy");
    j["input"]["datapath"] = json!("tproxy");
    install_journal(&f, &j);
    let before = fs::read(f.0.join("capture-journal.json")).unwrap();
    assert_eq!(Controller::open(&f.0).unwrap_err(), Error::LegacyJournal);
    assert_eq!(fs::read(f.0.join("capture-journal.json")).unwrap(), before);
}
#[test]
fn preflight_precedes_journal_and_every_exact_apply_and_constructor_runs_nothing() {
    let f = Fixture::new();
    let mut c = f.open();
    enable(&mut c);
    let seen = RefCell::new(vec![]);
    let p = plan_owned_rules(&input()).unwrap();
    c.apply(
        input(),
        |i, plan, deadline| {
            assert_eq!(i, &input());
            assert_eq!(plan, &p);
            assert!(deadline > std::time::Instant::now());
            assert!(!f.0.join("capture-journal.json").exists());
            seen.borrow_mut().push("preflight");
            Ok(())
        },
        |argv, deadline| {
            assert!(deadline > std::time::Instant::now());
            assert!(f.0.join("capture-journal.json").exists());
            let staged = f.open();
            assert_eq!(staged.status().phase, CapturePhase::Staged);
            let index = seen.borrow().len() - 1;
            assert_eq!(argv, &p.apply[index]);
            seen.borrow_mut().push("apply");
            Ok(CommandResult::success())
        },
    )
    .unwrap();
    assert_eq!(seen.borrow().len(), p.apply.len() + 1);
    assert_eq!(c.status().phase, CapturePhase::ActiveByApply);
    assert!(!c.status().cleanup_pending);
    assert_eq!(f.open().status().phase, CapturePhase::Staged);
}
#[test]
fn refused_preflight_and_off_intent_do_not_journal_or_run() {
    let f = Fixture::new();
    let mut c = f.open();
    assert_eq!(
        c.apply(
            input(),
            |_, _, _| panic!("preflight while off"),
            |_, _| panic!("runner while off")
        ),
        Err(Error::DesiredOff)
    );
    enable(&mut c);
    assert_eq!(
        c.apply(
            input(),
            |_, _, _| Err(PreflightError::Refused),
            |_, _| panic!("runner after refusal")
        ),
        Err(Error::Preflight)
    );
    assert!(!f.0.join("capture-journal.json").exists());
    let mut wrong = input();
    wrong
        .client_macs
        .as_mut()
        .unwrap()
        .insert("192.168.31.10".into(), "02:aa:bb:cc:dd:ef".into());
    assert_eq!(
        c.apply(
            wrong,
            |_, _, _| panic!("preflight wrong intent"),
            |_, _| panic!("runner wrong intent")
        ),
        Err(Error::IntentMismatch)
    );
}
#[test]
fn partial_apply_failure_or_cancel_stops_apply_but_attempts_all_cleanup_independently() {
    for failure in [
        CommandError::Failure,
        CommandError::Cancelled,
        CommandError::Timeout,
    ] {
        let f = Fixture::new();
        let mut c = f.open();
        enable(&mut c);
        let p = plan_owned_rules(&input()).unwrap();
        let mut ran = vec![];
        let failure_deadline = Cell::new(None);
        let error = c
            .apply(input(), preflight, |argv, deadline| {
                ran.push(argv.to_vec());
                if ran.len() == 3 {
                    failure_deadline.set(Some(deadline));
                    return Err(failure);
                }
                if ran.len() > 3 {
                    assert!(deadline > failure_deadline.get().unwrap());
                }
                Ok(CommandResult::success())
            })
            .unwrap_err();
        assert_eq!(
            error,
            Error::ApplyFailed {
                cleanup_failed: false
            }
        );
        let expected: Vec<_> = p.apply[..3].iter().chain(&p.cleanup).cloned().collect();
        assert_eq!(ran, expected);
        assert_eq!(c.status().phase, CapturePhase::Off);
        assert!(!f.0.join("capture-journal.json").exists());
    }
}
#[test]
fn cleanup_failure_retains_owned_journal_attempts_all_and_blocks_new_apply() {
    let f = Fixture::new();
    let mut c = f.open();
    enable(&mut c);
    c.apply(input(), preflight, success).unwrap();
    let p = plan_owned_rules(&input()).unwrap();
    let mut ran = vec![];
    assert_eq!(
        c.cleanup(|argv, _| {
            ran.push(argv.to_vec());
            if ran.len() == 1 {
                Err(CommandError::Failure)
            } else {
                Ok(CommandResult::success())
            }
        }),
        Err(Error::CleanupFailed)
    );
    assert_eq!(ran, p.cleanup);
    assert!(f.0.join("capture-journal.json").exists());
    assert_eq!(c.status().phase, CapturePhase::CleanupPending);
    assert_eq!(c.status().pending_cleanup_commands, 1);
    assert_eq!(
        c.apply(input(), preflight, success),
        Err(Error::AlreadyOwned)
    );
    assert!(c.snapshot().ownership.is_some());
    c.cleanup(success).unwrap();
    assert_eq!(c.status().phase, CapturePhase::Off);
}
#[test]
fn exact_absence_only_and_ambiguous_mac_extension_errors_are_not_absence() {
    let f = Fixture::new();
    install_journal(&f, &journal(&input(), true));
    let mut c = f.open();
    let calls = Cell::new(0);
    assert_eq!(
        c.cleanup(|argv, _| {
            calls.set(calls.get() + 1);
            let output = if argv[0] == "ip" {
                "RTNETLINK answers: No such process"
            } else {
                "iptables: No chain/target/match by that name."
            };
            Ok(CommandResult {
                success: false,
                output: output.as_bytes().to_vec(),
            })
        }),
        Err(Error::CleanupFailed)
    );
    assert!(c.status().pending_cleanup_commands > 0);
    assert_eq!(
        calls.get(),
        plan_owned_rules(&input()).unwrap().cleanup.len()
    );
    c.cleanup(|argv, _| {
        let output = if argv[0] == "ip" {
            "RTNETLINK answers: No such process"
        } else if argv[5] == "-D" {
            "iptables: Bad rule (does a matching rule exist in that chain?)."
        } else {
            "iptables: No chain/target/match by that name."
        };
        Ok(CommandResult {
            success: false,
            output: output.as_bytes().to_vec(),
        })
    })
    .unwrap();
    let f = Fixture::new();
    install_journal(&f, &journal(&input(), true));
    let mut c = f.open();
    let total = plan_owned_rules(&input()).unwrap().cleanup.len();
    assert_eq!(
        c.cleanup(|_, _| Ok(CommandResult {
            success: false,
            output: b"Permission denied".to_vec()
        })),
        Err(Error::CleanupFailed)
    );
    assert_eq!(c.status().pending_cleanup_commands, total);
}
#[test]
fn disable_write_failure_still_latches_off_cleans_all_and_get_never_restores() {
    let f = Fixture::new();
    let mut c = f.open();
    enable(&mut c);
    c.apply(input(), preflight, success).unwrap();
    let old_desired = fs::read(f.0.join("capture-desired.json")).unwrap();
    fs::rename(f.0.join("capture-desired.json"), f.0.join("old-desired")).unwrap();
    private_write(&f.0.join("capture-desired.json"), &old_desired); // replacement inode refuses save
    let mut ran = vec![];
    assert_eq!(
        c.disable(|argv, _| {
            ran.push(argv.to_vec());
            Ok(CommandResult::success())
        }),
        Err(Error::DisableFailed {
            persistence_failed: true,
            cleanup_failed: false
        })
    );
    assert_eq!(ran, plan_owned_rules(&input()).unwrap().cleanup);
    assert!(!c.desired().desired);
    assert!(c.status().disable_not_persisted);
    assert!(!c.snapshot().desired.desired);
    assert!(!f.0.join("capture-journal.json").exists());
    assert_eq!(c.apply(input(), preflight, success), Err(Error::DesiredOff));
    assert!(f.open().desired().desired); // truthful restart warning, not overwritten on GET
}
#[test]
fn successful_disable_durably_off_even_when_cleanup_fails() {
    let f = Fixture::new();
    let mut c = f.open();
    enable(&mut c);
    c.apply(input(), preflight, success).unwrap();
    assert_eq!(
        c.disable(|_, _| Err(CommandError::Failure)),
        Err(Error::DisableFailed {
            persistence_failed: false,
            cleanup_failed: true
        })
    );
    assert!(!c.desired().desired);
    assert!(!c.status().disable_not_persisted);
    assert!(!f.open().desired().desired);
    assert!(f.0.join("capture-journal.json").exists());
}
#[test]
fn private_modes_nofollow_hardlink_fifo_and_oversize_refused_without_reset() {
    for file in ["capture-desired.json", "capture-journal.json"] {
        let f = Fixture::new();
        f.write("target", b"{}");
        symlink(f.0.join("target"), f.0.join(file)).unwrap();
        assert!(Controller::open(&f.0).is_err());
        assert!(
            fs::symlink_metadata(f.0.join(file))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        let f = Fixture::new();
        f.write("target", b"{}");
        fs::hard_link(f.0.join("target"), f.0.join(file)).unwrap();
        assert_eq!(Controller::open(&f.0).unwrap_err(), Error::UnsafeFile);
        let f = Fixture::new();
        f.write(file, b"{}");
        fs::set_permissions(f.0.join(file), Permissions::from_mode(0o644)).unwrap();
        assert_eq!(Controller::open(&f.0).unwrap_err(), Error::UnsafeFile);
        let f = Fixture::new();
        use std::os::unix::ffi::OsStrExt;
        let fifo = std::ffi::CString::new(f.0.join(file).as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        assert_eq!(Controller::open(&f.0).unwrap_err(), Error::UnsafeFile);
    }
    let f = Fixture::new();
    f.write("capture-journal.json", &vec![b' '; MAX_JOURNAL_BYTES + 1]);
    assert_eq!(Controller::open(&f.0).unwrap_err(), Error::TooLarge);
    let f = Fixture::new();
    fs::set_permissions(&f.0, Permissions::from_mode(0o755)).unwrap();
    assert_eq!(Controller::open(&f.0).unwrap_err(), Error::UnsafeFile);
    let f = Fixture::new();
    let c = f.open();
    assert!(!f.0.join("capture-desired.json").exists());
    drop(c);
    assert!(Controller::open(f.0.join("missing")).is_err());
}
#[test]
fn saved_files_are_private_and_no_unexpected_owned_chain_removals() {
    let f = Fixture::new();
    let mut c = f.open();
    enable(&mut c);
    c.apply(input(), preflight, success).unwrap();
    for name in ["capture-desired.json", "capture-journal.json"] {
        assert_eq!(fs::metadata(f.0.join(name)).unwrap().mode() & 0o7777, 0o600);
    }
    let p = plan_owned_rules(&input()).unwrap();
    c.cleanup(|argv, _| {
        assert!(p.cleanup.contains(&argv.to_vec()));
        if argv[0] == "iptables" && matches!(argv[5].as_str(), "-F" | "-X") {
            assert!(
                p.ownership
                    .chains
                    .iter()
                    .any(|chain| chain.table == argv[4] && chain.name == argv[6])
            );
            assert_ne!(argv[6], "PREROUTING");
            assert_ne!(argv[6], "FORWARD");
        }
        Ok(CommandResult::success())
    })
    .unwrap();
}
#[test]
fn desired_mac_limits_duplicates_nonunicast_and_unknown_authority_are_refused() {
    let f = Fixture::new();
    let mut c = f.open();
    for mac in [
        "00:00:00:00:00:00",
        "01:aa:bb:cc:dd:ee",
        "02aabbccddee..",
        "not-a-mac",
    ] {
        assert_eq!(
            c.set_desired(Desired {
                devices: vec![DeviceSelection { mac: mac.into() }],
                ..Desired::default()
            }),
            Err(Error::InvalidDesired)
        );
    }
    let duplicate = Desired {
        devices: vec![
            DeviceSelection {
                mac: "02:aa:bb:cc:dd:ee".into(),
            },
            DeviceSelection {
                mac: "02-AA-BB-CC-DD-EE".into(),
            },
        ],
        ..Desired::default()
    };
    assert_eq!(c.set_desired(duplicate), Err(Error::InvalidDesired));
    let too_many = Desired {
        devices: (0..65)
            .map(|i| DeviceSelection {
                mac: format!("02:00:00:00:00:{i:02x}"),
            })
            .collect(),
        ..Desired::default()
    };
    assert_eq!(c.set_desired(too_many), Err(Error::InvalidDesired));
}
#[test]
fn normal_64_client_journal_is_measured_and_limited_before_execution() {
    let mut i = input();
    i.client_ipv4.clear();
    i.client_ipv4s = Some((1..=64).map(|n| format!("192.168.31.{n}")).collect());
    i.client_macs = Some(
        (1..=64)
            .map(|n| (format!("192.168.31.{n}"), format!("02:00:00:00:00:{n:02x}")))
            .collect(),
    );
    let bytes = serde_json::to_vec(&journal(&i, true)).unwrap();
    println!("64-client normal journal bytes={}", bytes.len());
    assert!(bytes.len() < MAX_JOURNAL_BYTES);
    let f = Fixture::new();
    let mut c = f.open();
    c.set_desired(Desired {
        desired: true,
        devices: (1..=64)
            .map(|n| DeviceSelection {
                mac: format!("02:00:00:00:00:{n:02x}"),
            })
            .collect(),
        ..Desired::default()
    })
    .unwrap();
    c.apply(i, preflight, success).unwrap();
    assert!(
        fs::metadata(f.0.join("capture-journal.json"))
            .unwrap()
            .len()
            <= MAX_JOURNAL_BYTES as u64
    );
}

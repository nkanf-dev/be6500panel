//! Synthetic paths and a recording Backend only. No router or shell execution.
use be6500_panel::features_maintenance::{
    CRON_BEGIN, CRON_END, ConfigPaths, MAX_CRON_BYTES, MAX_SCHEDULE_BYTES, REBOOT_SCRIPT,
    invoke_with_paths,
};
use be6500_panel::product_io::{Backend, Error as IoError, Output, Program};
use be6500_panel::readiness_tun::Budget;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const UNRELATED: &[u8] = b"# factory cron\n*/2 * * * * /data/root-rescue/keep-ssh.sh\n@reboot /data/panel/bootstrap.sh\n0 1 * * * /usr/sbin/factory-task\n# no final line feed";
struct Fake {
    reads: BTreeMap<PathBuf, Result<Vec<u8>, IoError>>,
    lists: BTreeMap<PathBuf, Result<Vec<String>, IoError>>,
    paths_read: Vec<PathBuf>,
    calls: Vec<(Program, Vec<String>)>,
    fail_restart: bool,
    expected_publication: Option<(PathBuf, PathBuf, String)>,
}
impl Fake {
    fn new() -> Self {
        Self {
            reads: BTreeMap::new(),
            lists: BTreeMap::new(),
            paths_read: Vec::new(),
            calls: Vec::new(),
            fail_restart: false,
            expected_publication: None,
        }
    }
}
impl Backend for Fake {
    fn read(&mut self, path: &Path, limit: usize, b: &Budget<'_>) -> Result<Vec<u8>, IoError> {
        b.check().map_err(|_| IoError::Deadline)?;
        assert!(limit <= 32);
        self.paths_read.push(path.into());
        let result = self
            .reads
            .get(path)
            .cloned()
            .unwrap_or(Err(IoError::Unavailable))?;
        if result.len() > limit {
            return Err(IoError::Limit);
        }
        Ok(result)
    }
    fn list(&mut self, path: &Path, limit: usize, b: &Budget<'_>) -> Result<Vec<String>, IoError> {
        b.check().map_err(|_| IoError::Deadline)?;
        assert!(limit <= 4096);
        self.paths_read.push(path.into());
        let result = self
            .lists
            .get(path)
            .cloned()
            .unwrap_or(Err(IoError::Unavailable))?;
        if result.len() > limit {
            return Err(IoError::Limit);
        }
        Ok(result)
    }
    fn run(
        &mut self,
        program: Program,
        args: &[String],
        stdin: Option<&[u8]>,
        limit: usize,
        b: &Budget<'_>,
    ) -> Result<Output, IoError> {
        b.check().map_err(|_| IoError::Deadline)?;
        assert_eq!(program, Program::Service);
        assert_eq!(args, &["cron".to_string(), "restart".to_string()]);
        assert!(stdin.is_none());
        assert_eq!(limit, 4096);
        if let Some((record, cron, line)) = &self.expected_publication {
            let state: Value = serde_json::from_slice(&fs::read(record).unwrap()).unwrap();
            assert_eq!(state["reloadPending"], true);
            assert!(fs::read_to_string(cron).unwrap().contains(line));
        }
        self.calls.push((program, args.to_vec()));
        Ok(Output {
            code: if self.fail_restart { 1 } else { 0 },
            stdout: b"password=private-runtime-token".to_vec(),
            stderr: b"account=private-runtime-token".to_vec(),
        })
    }
    fn now_unix(&self) -> u64 {
        1_800_000_000
    }
}
struct Fixture {
    root: PathBuf,
    data: PathBuf,
    cron: PathBuf,
    debug: PathBuf,
    modules: PathBuf,
    io: Fake,
}
impl Fixture {
    fn new() -> Self {
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).unwrap();
        let id = random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let root = std::env::temp_dir().join(format!("be6500-maintenance-{id}"));
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let data = root.join("private");
        let cron_dir = root.join("crontabs");
        fs::create_dir(&data).unwrap();
        fs::set_permissions(&data, fs::Permissions::from_mode(0o700)).unwrap();
        fs::create_dir(&cron_dir).unwrap();
        fs::set_permissions(&cron_dir, fs::Permissions::from_mode(0o755)).unwrap();
        let cron = cron_dir.join("root");
        fs::write(&cron, UNRELATED).unwrap();
        fs::set_permissions(&cron, fs::Permissions::from_mode(0o644)).unwrap();
        let debug = root.join("debug/ecm");
        let modules = root.join("sys/module");
        Self {
            root,
            data,
            cron,
            debug,
            modules,
            io: Fake::new(),
        }
    }
    fn paths(&self) -> ConfigPaths {
        ConfigPaths::with_system_paths(&self.data, &self.cron, &self.debug, &self.modules)
    }
    fn call(
        &mut self,
        handler: &str,
        input: Value,
    ) -> Result<Value, be6500_panel::features::Error> {
        let cancel = AtomicBool::new(false);
        let paths = self.paths();
        invoke_with_paths(handler, &input, &paths, &mut self.io, &budget(&cancel))
    }
    fn record(&self) -> PathBuf {
        self.data.join("maintenance-schedule.json")
    }
    fn script(&self) -> PathBuf {
        self.data.join("scheduled-reboot.sh")
    }
    fn no_stages(&self) {
        for directory in [&self.data, self.cron.parent().unwrap()] {
            assert!(
                fs::read_dir(directory).unwrap().all(|e| !e
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with('.'))
            );
        }
    }
    fn scalar(&mut self, name: &str, value: &[u8]) {
        self.io
            .reads
            .insert(self.debug.join(name), Ok(value.to_vec()));
    }
    fn frontend(&mut self, names: &[&str]) {
        self.io.lists.insert(
            self.debug.clone(),
            Ok(names.iter().map(|s| (*s).into()).collect()),
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn budget(cancel: &AtomicBool) -> Budget<'_> {
    Budget {
        deadline: Instant::now() + Duration::from_secs(30),
        cancel,
    }
}
fn on() -> Value {
    json!({"enabled": true, "time": "03:07", "weekdays": [6, 1, 0]})
}
fn owned(f: &Fixture, time: &str, days: &str) -> Vec<u8> {
    format!(
        "{CRON_BEGIN}\n{time} * * {days} {}\n{CRON_END}\n",
        f.script().display()
    )
    .into_bytes()
}

#[test]
fn one_owned_entry_preserves_every_unrelated_byte_and_fixed_script() {
    let mut f = Fixture::new();
    let line = format!("7 3 * * 0,1,6 {}", f.script().display());
    f.io.expected_publication = Some((f.record(), f.cron.clone(), line));
    let value = f.call("setSchedule", on()).unwrap();
    assert_eq!(
        value,
        json!({"enabled": true, "time": "03:07", "weekdays": [0, 1, 6],
        "timeBasis": "router", "reloadPending": false})
    );
    let mut expected = owned(&f, "7 3", "0,1,6");
    expected.extend_from_slice(UNRELATED);
    assert_eq!(fs::read(&f.cron).unwrap(), expected);
    assert_eq!(fs::read(f.script()).unwrap(), REBOOT_SCRIPT);
    assert_eq!(
        fs::metadata(&f.data).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(f.record()).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(f.script()).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(&f.cron).unwrap().permissions().mode() & 0o777,
        0o644
    );
    assert_eq!(f.io.calls.len(), 1);
    assert_eq!(f.call("getSchedule", json!({})).unwrap(), value);
    assert_eq!(f.call("setSchedule", on()).unwrap(), value);
    assert_eq!(f.io.calls.len(), 1, "unchanged bytes must not restart cron");
    f.no_stages();
}

#[test]
fn update_collapses_owned_duplicates_but_never_touches_rescue_or_bootstrap() {
    let mut f = Fixture::new();
    let old = owned(&f, "9 2", "2,3");
    let mut cron = old.clone();
    cron.extend_from_slice(b"# gap between entries\n");
    cron.extend_from_slice(&old);
    cron.extend_from_slice(UNRELATED);
    fs::write(&f.cron, cron).unwrap();
    f.call("setSchedule", on()).unwrap();
    let actual = fs::read(&f.cron).unwrap();
    let mut expected = owned(&f, "7 3", "0,1,6");
    expected.extend_from_slice(b"# gap between entries\n");
    expected.extend_from_slice(UNRELATED);
    assert_eq!(actual, expected);
    assert_eq!(
        String::from_utf8(actual)
            .unwrap()
            .matches(CRON_BEGIN)
            .count(),
        1
    );
}

#[test]
fn disabling_removes_only_owned_entry_and_keeps_off_record() {
    let mut f = Fixture::new();
    f.call("setSchedule", on()).unwrap();
    let off = json!({"enabled": false, "time": "23:59", "weekdays": []});
    let value = f.call("setSchedule", off.clone()).unwrap();
    assert_eq!(fs::read(&f.cron).unwrap(), UNRELATED);
    assert_eq!(fs::read(f.script()).unwrap(), REBOOT_SCRIPT);
    assert_eq!(value["enabled"], false);
    assert_eq!(value["time"], "23:59");
    assert_eq!(value["weekdays"], json!([]));
    assert_eq!(f.call("getSchedule", json!({})).unwrap(), value);
    assert_eq!(f.io.calls.len(), 2);
    f.call("setSchedule", off).unwrap();
    assert_eq!(f.io.calls.len(), 2);
    f.no_stages();
}

#[test]
fn omitted_days_are_daily_json_form_days_become_one_array_contract() {
    let mut f = Fixture::new();
    let daily = f
        .call("setSchedule", json!({"enabled": true, "time": "00:00"}))
        .unwrap();
    assert_eq!(daily["weekdays"], json!([0, 1, 2, 3, 4, 5, 6]));
    assert!(
        fs::read_to_string(&f.cron)
            .unwrap()
            .contains("0 0 * * 0,1,2,3,4,5,6")
    );
    let form = f
        .call(
            "setSchedule",
            json!({"enabled": true, "time": "23:59", "weekdays": "[5,2]"}),
        )
        .unwrap();
    assert_eq!(form["weekdays"], json!([2, 5]));
    assert!(
        fs::read_to_string(&f.cron)
            .unwrap()
            .contains("59 23 * * 2,5")
    );
    assert_eq!(form["timeBasis"], "router");
}

#[test]
fn invalid_and_injected_values_are_rejected_before_any_native_action_or_write() {
    let mut f = Fixture::new();
    f.call("setSchedule", on()).unwrap();
    let record = fs::read(f.record()).unwrap();
    let cron = fs::read(&f.cron).unwrap();
    let calls = f.io.calls.len();
    let inputs = [
        json!({"enabled": 1, "time": "03:00"}),
        json!({"enabled": true, "time": "3:00"}),
        json!({"enabled": true, "time": "24:00"}),
        json!({"enabled": true, "time": "23:60"}),
        json!({"enabled": false, "time": "24:00"}),
        json!({"enabled": true, "time": "03:00;reboot"}),
        json!({"enabled": true, "time": "03:00\n* * * * * evil"}),
        json!({"enabled": true, "time": "03:00", "weekdays": []}),
        json!({"enabled": true, "time": "03:00", "weekdays": [0,0]}),
        json!({"enabled": true, "time": "03:00", "weekdays": [7]}),
        json!({"enabled": true, "time": "03:00", "weekdays": [-1]}),
        json!({"enabled": true, "time": "03:00", "weekdays": [1.0]}),
        json!({"enabled": true, "time": "03:00", "weekdays": ["1"]}),
        json!({"enabled": true, "time": "03:00", "weekdays": "[1]; /sbin/reboot"}),
        json!({"enabled": true, "time": "03:00", "weekdays": "null"}),
        json!({"enabled": true, "time": "03:00", "password": "private-injected-token"}),
        json!({"enabled": true, "time": "03:00", "script": "/tmp/private-token"}),
        json!({"enabled": true}),
        json!([]),
    ];
    for input in inputs {
        let error = f.call("setSchedule", input).unwrap_err();
        assert_eq!(error.code, "invalid_feature_input");
        assert!(!format!("{error:?}").contains("private"));
        assert_eq!(fs::read(f.record()).unwrap(), record);
        assert_eq!(fs::read(&f.cron).unwrap(), cron);
        assert_eq!(f.io.calls.len(), calls);
    }
    assert!(
        f.call("getSchedule", json!({"password": "private-token"}))
            .is_err()
    );
    assert!(f.call("scheduled_reboot", json!({})).is_err());
    assert!(f.call("scheduled_reboot_set", on()).is_err());
    f.no_stages();
}

#[test]
fn malformed_ownership_markers_cannot_swallow_other_root_jobs() {
    for cron in [
        format!("{CRON_BEGIN}\n*/2 * * * * /data/root-rescue/keep-ssh.sh\n{CRON_END}\n"),
        format!("{CRON_BEGIN}\n@reboot /data/panel/bootstrap.sh\n"),
        format!("{CRON_END}\n"),
    ] {
        let mut f = Fixture::new();
        fs::write(&f.cron, &cron).unwrap();
        let error = f.call("setSchedule", on()).unwrap_err();
        assert_eq!(error.code, "maintenance_cron_conflict");
        assert_eq!(fs::read(&f.cron).unwrap(), cron.as_bytes());
        assert!(!f.record().exists());
        assert!(!f.script().exists());
        assert!(f.io.calls.is_empty());
        f.no_stages();
    }
}

#[test]
fn reload_failure_keeps_new_authoritative_state_and_can_retry_without_replacing_rows() {
    let mut f = Fixture::new();
    f.io.fail_restart = true;
    let error = f.call("setSchedule", on()).unwrap_err();
    assert_eq!(error.code, "schedule_saved_reload_failed");
    assert!(!format!("{error:?}").contains("private-runtime-token"));
    let state = f.call("getSchedule", json!({})).unwrap();
    assert_eq!(state["enabled"], true);
    assert_eq!(state["reloadPending"], true);
    let cron = fs::read(&f.cron).unwrap();
    assert!(cron.ends_with(UNRELATED));
    f.no_stages();
    f.io.fail_restart = false;
    let state = f.call("setSchedule", on()).unwrap();
    assert_eq!(state["reloadPending"], false);
    assert_eq!(fs::read(&f.cron).unwrap(), cron);
    assert_eq!(f.io.calls.len(), 2);
    f.no_stages();
}

#[test]
fn missing_or_loose_private_directory_never_initializes_or_resets_schedule() {
    let mut f = Fixture::new();
    fs::remove_dir(&f.data).unwrap();
    assert!(f.call("setSchedule", on()).is_err());
    assert!(f.call("getSchedule", json!({})).is_err());
    assert!(!f.data.exists());
    assert_eq!(fs::read(&f.cron).unwrap(), UNRELATED);
    assert!(f.io.calls.is_empty());
    fs::create_dir(&f.data).unwrap();
    fs::set_permissions(&f.data, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(f.call("setSchedule", on()).is_err());
    assert!(!f.record().exists());
}

#[test]
fn symlink_records_scripts_crontabs_and_invalid_stored_json_are_not_replaced() {
    for name in ["record", "script", "cron"] {
        let mut f = Fixture::new();
        let victim = f.root.join("private-victim");
        fs::write(&victim, b"private-secret-that-must-not-be-read").unwrap();
        let path = match name {
            "record" => f.record(),
            "script" => f.script(),
            _ => f.cron.clone(),
        };
        if path.exists() {
            fs::remove_file(&path).unwrap();
        }
        symlink(&victim, &path).unwrap();
        let error = f.call("setSchedule", on()).unwrap_err();
        assert!(!format!("{error:?}").contains("private-secret"));
        assert!(
            fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            fs::read(&victim).unwrap(),
            b"private-secret-that-must-not-be-read"
        );
        assert!(f.io.calls.is_empty());
    }
    let mut f = Fixture::new();
    fs::write(f.record(), b"{broken private-token").unwrap();
    fs::set_permissions(f.record(), fs::Permissions::from_mode(0o600)).unwrap();
    assert!(f.call("getSchedule", json!({})).is_err());
    assert!(f.call("setSchedule", on()).is_err());
    assert_eq!(fs::read(f.record()).unwrap(), b"{broken private-token");
    assert_eq!(fs::read(&f.cron).unwrap(), UNRELATED);
}

#[test]
fn bounded_documents_and_preexisting_stages_fail_without_reclaiming_reserves() {
    let mut f = Fixture::new();
    let reserve = f.data.join(".emergency-reserve");
    fs::write(&reserve, vec![0x5a; 4096]).unwrap();
    fs::write(&f.cron, vec![b'#'; MAX_CRON_BYTES + 1]).unwrap();
    assert!(f.call("setSchedule", on()).is_err());
    assert_eq!(
        fs::metadata(&f.cron).unwrap().len(),
        (MAX_CRON_BYTES + 1) as u64
    );
    assert_eq!(fs::read(&reserve).unwrap(), vec![0x5a; 4096]);
    fs::write(&f.cron, UNRELATED).unwrap();
    fs::write(f.record(), vec![b' '; MAX_SCHEDULE_BYTES + 1]).unwrap();
    fs::set_permissions(f.record(), fs::Permissions::from_mode(0o600)).unwrap();
    assert!(f.call("setSchedule", on()).is_err());
    fs::remove_file(f.record()).unwrap();
    let stale = f.data.join(".maintenance-schedule.pending");
    fs::write(&stale, b"private-stale-stage").unwrap();
    assert!(f.call("setSchedule", on()).is_err());
    assert_eq!(fs::read(&stale).unwrap(), b"private-stale-stage");
    assert!(!f.record().exists());
    assert!(!f.script().exists());
    assert_eq!(fs::read(&f.cron).unwrap(), UNRELATED);
    assert!(f.io.calls.is_empty());
}

#[test]
fn cancelled_or_expired_budget_prevents_publication_and_native_commands() {
    let mut f = Fixture::new();
    let cancel = AtomicBool::new(true);
    let paths = f.paths();
    let error =
        invoke_with_paths("setSchedule", &on(), &paths, &mut f.io, &budget(&cancel)).unwrap_err();
    assert_eq!(error.code, "operation_cancelled");
    cancel.store(false, Ordering::Relaxed);
    let expired = Budget {
        deadline: Instant::now() - Duration::from_millis(1),
        cancel: &cancel,
    };
    let error = invoke_with_paths("setSchedule", &on(), &paths, &mut f.io, &expired).unwrap_err();
    assert_eq!(error.code, "operation_timeout");
    assert!(!f.record().exists());
    assert_eq!(fs::read(&f.cron).unwrap(), UNRELATED);
    assert!(f.io.calls.is_empty());
}

#[test]
fn acceleration_missing_observations_are_unknown_not_active_or_forced_start() {
    let mut f = Fixture::new();
    let value = f.call("acceleration_status", json!({})).unwrap();
    assert_eq!(
        value,
        json!({"engine": "unknown", "state": "unknown", "frontend": "unknown",
        "ecm": null, "sfe": null, "ppe": null})
    );
    assert!(f.io.calls.is_empty());
    assert!(!f.record().exists());
    assert!(
        f.io.paths_read
            .iter()
            .all(|p| p.starts_with(&f.debug) || p == &f.modules)
    );
}

#[test]
fn loaded_modules_without_actual_counters_never_claim_active_engine() {
    let mut f = Fixture::new();
    f.io.lists.insert(
        f.modules.clone(),
        Ok(vec![
            "ecm".into(),
            "qca_nss_ppe".into(),
            "qca_nss_sfe".into(),
        ]),
    );
    f.frontend(&[
        "ecm_ppe_ipv4",
        "ecm_ppe_ipv6",
        "ecm_sfe_ipv4",
        "ecm_sfe_ipv6",
    ]);
    let value = f.call("acceleration_status", json!({})).unwrap();
    assert_eq!(value["ecm"], true);
    assert_eq!(value["ppe"], true);
    assert_eq!(value["sfe"], true);
    assert_eq!(value["frontend"], "ppe,sfe");
    assert_eq!(value["engine"], "unknown");
    assert_eq!(value["state"], "unknown");
    assert!(value.get("counters").is_none());
    assert!(f.io.calls.is_empty());
}

#[test]
fn actual_scalar_counters_identify_active_ppe_and_do_not_emit_raw_debug_text() {
    let mut f = Fixture::new();
    f.io.lists.insert(
        f.modules.clone(),
        Ok(vec!["ecm".into(), "qca_nss_ppe".into()]),
    );
    f.frontend(&["ecm_ppe_ipv4", "ecm_ppe_ipv6"]);
    f.scalar("ecm_ppe_ipv4/accelerated_count", b"17\n");
    f.scalar("ecm_ppe_ipv6/accelerated_count", b"2\n");
    f.scalar("ecm_ppe_ipv4/pending_accel_count", b"1\n");
    f.scalar("ecm_ppe_ipv6/pending_accel_count", b"0\n");
    f.scalar("ecm_db/connection_count", b"42\n");
    let value = f.call("acceleration_status", json!({})).unwrap();
    assert_eq!(
        value,
        json!({"engine": "ppe", "state": "active", "frontend": "ppe",
        "ecm": true, "sfe": false, "ppe": true,
        "counters": {"accelerated": 19, "pending": 1, "connections": 42}})
    );
    f.scalar("ecm_ppe_ipv4/accelerated_count", b"private-token=17");
    f.scalar("ecm_ppe_ipv6/accelerated_count", b"-1");
    f.scalar("ecm_db/connection_count", b"4294967296");
    let value = f.call("acceleration_status", json!({})).unwrap();
    assert_eq!(value["state"], "unknown");
    assert!(value["counters"].get("accelerated").is_none());
    assert!(value["counters"].get("connections").is_none());
    assert!(!value.to_string().contains("private-token"));
    assert!(f.io.calls.is_empty());
}

#[test]
fn mixed_frontends_observe_ecm_and_zero_flows_do_not_claim_unsupported_hardware() {
    let mut f = Fixture::new();
    f.io.lists.insert(f.modules.clone(), Ok(vec![]));
    f.frontend(&["ecm_ppe_ipv4", "ecm_sfe_ipv4"]);
    for directory in ["ecm_ppe_ipv4", "ecm_sfe_ipv4"] {
        f.scalar(&format!("{directory}/accelerated_count"), b"1");
        f.scalar(&format!("{directory}/pending_accel_count"), b"0");
    }
    let value = f.call("acceleration_status", json!({})).unwrap();
    assert_eq!(value["engine"], "ecm");
    assert_eq!(value["state"], "active");
    assert_eq!(value["frontend"], "ppe,sfe");
    assert_eq!(value["counters"]["accelerated"], 2);
    for directory in ["ecm_ppe_ipv4", "ecm_sfe_ipv4"] {
        f.scalar(&format!("{directory}/accelerated_count"), b"0");
    }
    let value = f.call("acceleration_status", json!({})).unwrap();
    assert_eq!(value["engine"], "unknown");
    assert_eq!(value["state"], "inactive");
    assert_eq!(value["counters"]["accelerated"], 0);
    assert!(f.io.calls.is_empty());
}

#[test]
fn acceleration_timeout_is_not_swallowed_as_unknown() {
    let mut f = Fixture::new();
    f.io.lists.insert(f.modules.clone(), Err(IoError::Deadline));
    let error = f.call("acceleration_status", json!({})).unwrap_err();
    assert_eq!(error.code, "operation_timeout");
    assert!(f.io.calls.is_empty());
    assert_eq!(fs::read(&f.cron).unwrap(), UNRELATED);
}

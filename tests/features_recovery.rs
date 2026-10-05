//! Synthetic recovery fixtures only. No router, vendor command or shell invocation.
use be6500_panel::{
    features_recovery::{MAX_DEVICE_DB_BYTES, MAX_UCI_BYTES, Recovery},
    product_io::{Backend, Error, Output, Program},
    readiness_tun::Budget,
};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt, symlink},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

struct Fake {
    fail_reads: bool,
}
impl Backend for Fake {
    fn read(&mut self, path: &Path, limit: usize, b: &Budget<'_>) -> Result<Vec<u8>, Error> {
        b.check().map_err(|_| Error::Deadline)?;
        if self.fail_reads {
            return Err(Error::Unavailable);
        }
        let raw = fs::read(path).map_err(|_| Error::Unavailable)?;
        if raw.len() > limit {
            return Err(Error::Limit);
        }
        Ok(raw)
    }
    fn run(
        &mut self,
        _: Program,
        _: &[String],
        _: Option<&[u8]>,
        _: usize,
        _: &Budget<'_>,
    ) -> Result<Output, Error> {
        panic!("recovery storage must never execute a native command")
    }
    fn now_unix(&self) -> u64 {
        1_800_000_000
    }
}
struct Fixture {
    base: PathBuf,
    root: PathBuf,
    native: PathBuf,
    db: PathBuf,
    io: Fake,
}
impl Fixture {
    fn new() -> Self {
        let mut random = [0; 16];
        getrandom::fill(&mut random).unwrap();
        let base = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "be6500-feature-recovery-{}",
                random
                    .iter()
                    .map(|n| format!("{n:02x}"))
                    .collect::<String>()
            ));
        let native = base.join("etc/config");
        fs::create_dir_all(&native).unwrap();
        let root = base.join("feature-operations");
        let db = base.join("etc/xqDb");
        Self {
            base,
            root,
            native,
            db,
            io: Fake { fail_reads: false },
        }
    }
    fn put(&self, name: &str, raw: &[u8], mode: u32) {
        fs::write(self.native.join(name), raw).unwrap();
        fs::set_permissions(self.native.join(name), fs::Permissions::from_mode(mode)).unwrap();
    }
    fn prepare(
        &mut self,
        names: &[&str],
        db: bool,
    ) -> Result<Recovery, be6500_panel::features::Error> {
        let cancel = AtomicBool::new(false);
        Recovery::prepare_with_paths(
            &self.root,
            "1800000000-1",
            names,
            db,
            &self.native,
            &self.db,
            &mut self.io,
            &budget(&cancel),
        )
    }
    fn recover(&mut self) -> Result<Vec<String>, be6500_panel::features::Error> {
        let cancel = AtomicBool::new(false);
        Recovery::recover_with_paths(
            &self.root,
            &self.native,
            &self.db,
            &mut self.io,
            &budget(&cancel),
        )
    }
    fn journal(&self) -> PathBuf {
        self.root.join("1800000000-1/journal.json")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}
fn budget(cancel: &AtomicBool) -> Budget<'_> {
    Budget {
        deadline: Instant::now() + Duration::from_secs(60),
        cancel,
    }
}
fn sqlite_bytes(tail: &[u8]) -> Vec<u8> {
    let mut raw = vec![0; 100];
    raw[..16].copy_from_slice(b"SQLite format 3\0");
    raw[18] = 1;
    raw[19] = 1;
    raw.extend_from_slice(tail);
    raw
}

#[test]
fn checkpoint_is_armed_private_and_restart_restores_exact_bytes_and_modes() {
    let mut f = Fixture::new();
    let original = b"# comments and binary-safe secret fixture\n\0\xff";
    f.put("network", original, 0o640);
    let recovery = f.prepare(&["network"], false).unwrap();
    assert!(f.journal().is_file());
    let journal = fs::read_to_string(f.journal()).unwrap();
    assert!(!journal.contains("secret fixture"));
    assert_eq!(fs::metadata(&f.root).unwrap().mode() & 0o777, 0o700);
    assert_eq!(
        fs::metadata(f.journal().parent().unwrap()).unwrap().mode() & 0o777,
        0o700
    );
    assert_eq!(fs::metadata(f.journal()).unwrap().mode() & 0o777, 0o600);
    assert_eq!(
        fs::metadata(f.journal().parent().unwrap().join("backup-00"))
            .unwrap()
            .mode()
            & 0o777,
        0o600
    );
    drop(recovery); // Simulate restart. Drop must not delete recovery data.
    f.put("network", b"changed", 0o600);
    assert_eq!(f.recover().unwrap(), vec!["network"]);
    assert_eq!(fs::read(f.native.join("network")).unwrap(), original);
    assert_eq!(
        fs::metadata(f.native.join("network")).unwrap().mode() & 0o777,
        0o640
    );
    assert!(f.journal().exists()); // Root still owes a fixed native reload.
    assert_eq!(f.recover().unwrap(), vec!["network"]); // Reload retry keeps full scope.
    Recovery::discard_recovered(&f.root).unwrap();
    assert!(!f.journal().exists());
}
#[test]
fn originally_missing_is_removed_and_existing_empty_file_is_not_missing() {
    let mut f = Fixture::new();
    f.put("network", b"", 0o644);
    let mut recovery = f.prepare(&["network", "dhcp"], false).unwrap();
    f.put("network", b"changed", 0o600);
    f.put("dhcp", b"new native file", 0o600);
    let cancel = AtomicBool::new(false);
    assert_eq!(
        recovery.restore(&mut f.io, &budget(&cancel)).unwrap(),
        vec!["network", "dhcp"]
    );
    assert!(f.native.join("network").is_file());
    assert_eq!(fs::read(f.native.join("network")).unwrap(), b"");
    assert_eq!(
        fs::metadata(f.native.join("network")).unwrap().mode() & 0o777,
        0o644
    );
    assert!(!f.native.join("dhcp").exists());
    recovery.discard().unwrap();
}
#[test]
fn refusal_of_source_symlink_hardlink_and_special_mode_does_not_mutate() {
    for kind in ["symlink", "hardlink", "special_mode"] {
        let mut f = Fixture::new();
        let outside = f.base.join("outside");
        fs::write(&outside, b"outside bytes").unwrap();
        match kind {
            "symlink" => symlink(&outside, f.native.join("network")).unwrap(),
            "hardlink" => fs::hard_link(&outside, f.native.join("network")).unwrap(),
            _ => f.put("network", b"mode", 0o4644),
        }
        assert_eq!(
            f.prepare(&["network"], false).err().unwrap().code,
            "feature_recovery_unsafe"
        );
        assert_eq!(fs::read(outside).unwrap(), b"outside bytes");
        assert!(!f.journal().exists());
    }
}
#[test]
fn destination_link_refusal_retains_armed_journal_and_can_retry() {
    let mut f = Fixture::new();
    f.put("network", b"original", 0o600);
    let mut recovery = f.prepare(&["network"], false).unwrap();
    let outside = f.base.join("outside");
    fs::write(&outside, b"do not replace").unwrap();
    fs::remove_file(f.native.join("network")).unwrap();
    symlink(&outside, f.native.join("network")).unwrap();
    let cancel = AtomicBool::new(false);
    assert_eq!(
        recovery
            .restore(&mut f.io, &budget(&cancel))
            .unwrap_err()
            .code,
        "feature_recovery_unsafe"
    );
    assert_eq!(fs::read(&outside).unwrap(), b"do not replace");
    assert!(f.journal().exists());
    fs::remove_file(f.native.join("network")).unwrap();
    recovery.restore(&mut f.io, &budget(&cancel)).unwrap();
    assert_eq!(fs::read(f.native.join("network")).unwrap(), b"original");
    recovery.discard().unwrap();
}
#[test]
fn corrupt_journal_or_backup_retains_live_bytes_and_recovery_artifacts() {
    for corruption in ["journal", "backup", "descriptor"] {
        let mut f = Fixture::new();
        f.put("network", b"original", 0o600);
        drop(f.prepare(&["network"], false).unwrap());
        f.put("network", b"current", 0o600);
        match corruption {
            "journal" => fs::write(f.journal(), b"{broken").unwrap(),
            "backup" => {
                fs::write(f.journal().parent().unwrap().join("backup-00"), b"tampered").unwrap()
            }
            _ => {
                let mut journal: Value =
                    serde_json::from_slice(&fs::read(f.journal()).unwrap()).unwrap();
                journal["configs"] = json!(["not_in_any_fixed_descriptor"]);
                fs::write(f.journal(), serde_json::to_vec(&journal).unwrap()).unwrap();
            }
        }
        assert!(f.recover().is_err());
        assert_eq!(fs::read(f.native.join("network")).unwrap(), b"current");
        assert!(f.journal().exists());
    }
}
#[test]
fn native_read_failure_is_not_treated_as_originally_missing() {
    let mut f = Fixture::new();
    f.put("network", b"original", 0o600);
    f.io.fail_reads = true;
    assert!(f.prepare(&["network"], false).is_err());
    assert!(!f.journal().exists());
    assert_eq!(fs::read(f.native.join("network")).unwrap(), b"original");
}
#[test]
fn uci_total_and_database_size_have_independent_hard_bounds() {
    let mut f = Fixture::new();
    f.put("network", &vec![b'n'; MAX_UCI_BYTES], 0o600);
    f.put("dhcp", b"x", 0o600);
    assert_eq!(
        f.prepare(&["network", "dhcp"], false).err().unwrap().code,
        "feature_checkpoint_too_large"
    );
    assert!(!f.journal().exists());
    let mut f = Fixture::new();
    fs::write(&f.db, vec![0; MAX_DEVICE_DB_BYTES + 1]).unwrap();
    assert_eq!(
        f.prepare(&[], true).err().unwrap().code,
        "feature_checkpoint_too_large"
    );
    assert!(!f.journal().exists());
}
#[test]
fn device_database_is_private_and_includes_devicelist_in_reload_scope() {
    let mut f = Fixture::new();
    let original = sqlite_bytes(b"fixture database bytes");
    fs::write(&f.db, &original).unwrap();
    fs::set_permissions(&f.db, fs::Permissions::from_mode(0o640)).unwrap();
    f.put("devicelist", b"device fixture", 0o644);
    let mut recovery = f.prepare(&["macbind"], true).unwrap();
    fs::write(&f.db, sqlite_bytes(b"changed bytes")).unwrap();
    f.put("devicelist", b"changed devices", 0o600);
    f.put("macbind", b"new binding", 0o600);
    let cancel = AtomicBool::new(false);
    assert_eq!(
        recovery.restore(&mut f.io, &budget(&cancel)).unwrap(),
        vec!["macbind", "devicelist"]
    );
    assert_eq!(fs::read(&f.db).unwrap(), original);
    assert_eq!(fs::metadata(&f.db).unwrap().mode() & 0o777, 0o640);
    assert_eq!(
        fs::read(f.native.join("devicelist")).unwrap(),
        b"device fixture"
    );
    assert!(!f.native.join("macbind").exists());
    recovery.discard().unwrap();
}
#[test]
fn hot_sqlite_wal_and_rollback_journal_are_unsupported_not_raw_copied() {
    for suffix in ["-wal", "-journal"] {
        let mut f = Fixture::new();
        fs::write(&f.db, sqlite_bytes(b"fixture")).unwrap();
        fs::write(
            f.base.join(format!("etc/xqDb{suffix}")),
            b"active transaction",
        )
        .unwrap();
        assert_eq!(
            f.prepare(&["macbind"], true).err().unwrap().code,
            "feature_sqlite_busy"
        );
        assert!(!f.journal().exists());
    }
    let mut f = Fixture::new();
    let mut raw = sqlite_bytes(b"wal header");
    raw[18] = 2;
    raw[19] = 2;
    fs::write(&f.db, raw).unwrap();
    assert_eq!(
        f.prepare(&[], true).err().unwrap().code,
        "feature_sqlite_busy"
    );
}
#[test]
fn store_symlink_or_nonprivate_mode_is_refused_and_unknown_files_survive_discard() {
    let mut f = Fixture::new();
    fs::create_dir(&f.root).unwrap();
    fs::set_permissions(&f.root, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        f.prepare(&[], false).err().unwrap().code,
        "feature_recovery_unsafe"
    );
    fs::set_permissions(&f.root, fs::Permissions::from_mode(0o700)).unwrap();
    let recovery = f.prepare(&[], false).unwrap();
    let unknown = f.journal().parent().unwrap().join("keep-unknown");
    fs::write(&unknown, b"not owned by recovery").unwrap();
    recovery.discard().unwrap();
    assert_eq!(fs::read(unknown).unwrap(), b"not owned by recovery");
    assert!(!f.journal().exists());
    let mut f = Fixture::new();
    let elsewhere = f.base.join("elsewhere");
    fs::create_dir(&elsewhere).unwrap();
    symlink(&elsewhere, &f.root).unwrap();
    assert_eq!(
        f.prepare(&[], false).err().unwrap().code,
        "feature_recovery_unsafe"
    );
}

#![cfg(unix)]
use be6500_panel::runtime_store::{
    Artifact, Candidate, ConfigRecord, DiskState, MAX_CONFIG_BYTES, MAX_STORED_CONFIG_BYTES,
    ReadinessProof, RuntimeStore, ServiceId, StoreError, VerificationProof,
};
use sha2::{Digest, Sha256};
use std::fs::{self, DirBuilder, Permissions};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
const SB: ServiceId = ServiceId::SingBox;
const FR: ServiceId = ServiceId::Frpc;
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "be6500-runtime-store-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        DirBuilder::new().mode(0o700).create(&path).unwrap();
        Self(path)
    }
    fn data(&self) -> PathBuf {
        self.0.join("services")
    }
    fn run(&self) -> PathBuf {
        self.0.join("run")
    }
    fn service(&self, id: ServiceId) -> PathBuf {
        self.data().join(id.as_str())
    }
    fn open(&self) -> RuntimeStore {
        RuntimeStore::open(self.data(), self.run()).unwrap()
    }
    fn install(&self, id: ServiceId, state: &DiskState, configs: &[(&str, &[u8])]) {
        fs::create_dir_all(self.service(id)).unwrap();
        for (name, raw) in configs {
            private_write(&self.service(id).join(name), raw);
        }
        private_write(
            &self.service(id).join("state.json"),
            &serde_json::to_vec(state).unwrap(),
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn private_write(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, Permissions::from_mode(0o600)).unwrap();
}
fn digest(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
}
fn record(generation: u64, name: &str, raw: &[u8], ready: bool) -> ConfigRecord {
    ConfigRecord {
        generation,
        file: name.into(),
        sha256: digest(raw),
        ready,
    }
}
fn proof(id: ServiceId, raw: &[u8]) -> VerificationProof {
    VerificationProof::checked(id, raw)
}
fn commit(store: &mut RuntimeStore, id: ServiceId, raw: &[u8]) -> DiskState {
    let candidate = store.stage_candidate(id, raw).unwrap();
    store
        .commit_verified_candidate(candidate, &proof(id, raw))
        .unwrap()
        .state
}
fn ready(store: &mut RuntimeStore, id: ServiceId, raw: &[u8]) {
    let generation = store.service_state(id).current.as_ref().unwrap().generation;
    store
        .mark_ready(id, &ReadinessProof::observed(id, generation, raw))
        .unwrap();
}
#[test]
fn exclusive_owner_and_private_layout_without_constructor_cleanup() {
    let f = Fixture::new();
    let store = f.open();
    assert_eq!(SB.as_str(), "sing-box");
    assert_eq!(FR.as_str(), "frpc");
    assert_eq!(store.snapshot(SB), DiskState::default());
    assert_eq!(
        RuntimeStore::open(f.data(), f.run()).unwrap_err(),
        StoreError::Busy
    );
    for dir in [f.data(), f.run(), f.service(SB), f.service(FR)] {
        assert_eq!(fs::metadata(dir).unwrap().mode() & 0o7777, 0o700);
    }
    assert_eq!(
        fs::metadata(f.data().join(".manager.lock")).unwrap().mode() & 0o7777,
        0o600
    );
    private_write(&f.service(SB).join("config-77.json"), b"orphan retained");
    private_write(&f.service(SB).join(".candidate-unknown.json"), b"not ours");
    private_write(&f.run().join("pid"), b"12345");
    drop(store);
    let _store = f.open();
    assert!(f.service(SB).join("config-77.json").exists());
    assert!(f.service(SB).join(".candidate-unknown.json").exists());
    assert_eq!(fs::read(f.run().join("pid")).unwrap(), b"12345");
}
#[test]
fn overlapping_real_roots_and_final_symlinks_refused() {
    let f = Fixture::new();
    assert_eq!(
        RuntimeStore::open(f.data(), f.data().join("run")).unwrap_err(),
        StoreError::OverlappingRoots
    );
    fs::create_dir_all(f.data()).unwrap();
    symlink(f.data(), f.0.join("alias")).unwrap();
    assert!(RuntimeStore::open(f.data(), f.0.join("alias")).is_err());
    assert_eq!(
        RuntimeStore::open(f.data(), f.0.join("alias/subrun")).unwrap_err(),
        StoreError::OverlappingRoots
    );
    assert!(RuntimeStore::open(f.0.join("alias"), f.run()).is_err());
}
#[test]
fn compatible_camel_case_metadata_and_legacy_unready_last_good() {
    let f = Fixture::new();
    let raw = b"{\"private\":\"password\"}";
    fs::create_dir_all(f.service(SB)).unwrap();
    private_write(&f.service(SB).join("config-3.json"), raw);
    let json = format!(
        r#"{{"generation":7,"current":{{"generation":3,"file":"config-3.json","sha256":"{}"}},"lastGood":{{"generation":999,"file":"../ignored","sha256":"invalid"}},"artifact":{{"url":"https://private.invalid/password","sha256":"{}","compression":"gzip","version":"v1"}}}}"#,
        digest(raw),
        "0".repeat(64)
    );
    private_write(&f.service(SB).join("state.json"), json.as_bytes());
    let store = f.open();
    let state = store.snapshot(SB);
    assert_eq!(state.generation, 7);
    assert!(state.last_good.is_none());
    assert_eq!(
        store
            .read_config(SB, state.current.as_ref().unwrap())
            .unwrap(),
        raw
    );
    let encoded = serde_json::to_value(&state).unwrap();
    assert!(encoded.get("last_good").is_none());
    assert!(encoded["current"].get("ready").is_none());
    assert_eq!(encoded["artifact"]["compression"], "gzip");
    let debug = format!("{store:?} {state:?}");
    assert!(!debug.contains("password"));
    assert!(!debug.contains("private.invalid"));
}
#[test]
fn malformed_metadata_references_names_and_hashes_fail_closed() {
    let raw = b"{}";
    for change in 0..9 {
        let f = Fixture::new();
        let mut state = DiskState {
            generation: 1,
            current: Some(record(1, "config-1.json", raw, false)),
            ..DiskState::default()
        };
        match change {
            0 => state.current.as_mut().unwrap().generation = 0,
            1 => state.current.as_mut().unwrap().generation = 2,
            2 => state.current.as_mut().unwrap().file = "../config-1.json".into(),
            3 => state.current.as_mut().unwrap().file = "config-01.json".into(),
            4 => state.current.as_mut().unwrap().file = "config-1.toml".into(),
            5 => state.current.as_mut().unwrap().sha256 = "f".repeat(64),
            6 => state.current.as_mut().unwrap().sha256 = "GG".repeat(32),
            7 => {
                state.last_good = state.current.take();
                state.last_good.as_mut().unwrap().ready = true;
            }
            8 => state.current.as_mut().unwrap().sha256 = "a".into(),
            _ => unreachable!(),
        }
        f.install(SB, &state, &[("config-1.json", raw)]);
        let before = fs::read(f.service(SB).join("state.json")).unwrap();
        assert!(RuntimeStore::open(f.data(), f.run()).is_err(), "{change}");
        assert_eq!(fs::read(f.service(SB).join("state.json")).unwrap(), before);
    }
    for malformed in [b"".as_slice(), b"{broken", b"null", b"{\"unknown\":123}"] {
        let f = Fixture::new();
        fs::create_dir_all(f.service(SB)).unwrap();
        private_write(&f.service(SB).join("state.json"), malformed);
        assert!(RuntimeStore::open(f.data(), f.run()).is_err());
    }
}
#[test]
fn symlinks_fifos_oversize_and_missing_accepted_configs_refused() {
    for kind in 0..7 {
        let f = Fixture::new();
        let raw = b"{}";
        let state = DiskState {
            generation: 1,
            current: Some(record(1, "config-1.json", raw, false)),
            ..DiskState::default()
        };
        f.install(SB, &state, &[]);
        let config = f.service(SB).join("config-1.json");
        match kind {
            0 => {
                private_write(&f.0.join("outside"), raw);
                symlink(f.0.join("outside"), &config).unwrap();
            }
            1 => fifo(&config),
            2 => private_write(&config, &vec![0; MAX_STORED_CONFIG_BYTES + 1]),
            3 => {
                private_write(&config, raw);
                let state = f.service(SB).join("state.json");
                fs::remove_file(&state).unwrap();
                symlink(&config, state).unwrap();
            }
            4 => {
                private_write(&config, raw);
                let state = f.service(SB).join("state.json");
                fs::remove_file(&state).unwrap();
                fifo(&state);
            }
            5 => {
                private_write(&config, raw);
                private_write(
                    &f.service(SB).join("state.json"),
                    &vec![b' '; (16 << 10) + 1],
                );
            }
            6 => (),
            _ => unreachable!(),
        }
        assert!(RuntimeStore::open(f.data(), f.run()).is_err(), "{kind}");
    }
    for name in [".manager.lock", "sing-box"] {
        let f = Fixture::new();
        fs::create_dir_all(f.data()).unwrap();
        symlink(&f.0, f.data().join(name)).unwrap();
        assert!(RuntimeStore::open(f.data(), f.run()).is_err());
    }
    let f = Fixture::new();
    fs::create_dir_all(f.data()).unwrap();
    fifo(&f.data().join(".manager.lock"));
    assert!(RuntimeStore::open(f.data(), f.run()).is_err());
}
fn fifo(path: &Path) {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
}
#[test]
fn stage_bounds_legacy_read_and_frpc_extension() {
    let f = Fixture::new();
    let mut store = f.open();
    assert_eq!(
        store
            .stage_candidate(SB, &vec![0; MAX_CONFIG_BYTES + 1])
            .unwrap_err(),
        StoreError::ConfigSize
    );
    let raw = vec![b'x'; MAX_CONFIG_BYTES];
    let candidate = store.stage_candidate(SB, &raw).unwrap();
    assert_eq!(
        fs::metadata(candidate.path()).unwrap().mode() & 0o7777,
        0o600
    );
    assert_eq!(candidate.len(), MAX_CONFIG_BYTES);
    assert_eq!(candidate.sha256(), digest(&raw));
    let path = candidate.path().to_owned();
    drop(candidate);
    assert!(!path.exists());
    let state = commit(&mut store, FR, b"serverAddr = 'example.invalid'\n");
    assert_eq!(state.current.unwrap().file, "config-1.toml");
    let state = commit(&mut store, FR, b"{}");
    assert_eq!(state.current.unwrap().file, "config-2.json");
    drop(store);
    let legacy = vec![b'a'; MAX_STORED_CONFIG_BYTES];
    let state = DiskState {
        generation: 1,
        current: Some(record(1, "config-1.json", &legacy, true)),
        ..DiskState::default()
    };
    f.install(SB, &state, &[("config-1.json", &legacy)]);
    let store = f.open();
    assert_eq!(
        store
            .read_config(SB, state.current.as_ref().unwrap())
            .unwrap()
            .len(),
        MAX_STORED_CONFIG_BYTES
    );
}
#[test]
fn unchanged_checked_candidate_required_and_precise_cleanup() {
    let f = Fixture::new();
    let mut store = f.open();
    let raw = b"{}";
    for kind in 0..4 {
        let candidate = store.stage_candidate(SB, raw).unwrap();
        let path = candidate.path().to_owned();
        let check = match kind {
            0 => proof(SB, b"changed"),
            1 => proof(FR, raw),
            _ => proof(SB, raw),
        };
        if kind == 2 {
            private_write(&path, b"modified by checker");
        }
        if kind == 3 {
            fs::remove_file(&path).unwrap();
            private_write(&path, b"replacement not owned");
        }
        assert_eq!(
            store
                .commit_verified_candidate(candidate, &check)
                .unwrap_err(),
            StoreError::Verification
        );
        assert_eq!(store.snapshot(SB), DiskState::default());
        if kind == 3 {
            assert_eq!(fs::read(path).unwrap(), b"replacement not owned");
        } else {
            assert!(!path.exists());
        }
    }
}
#[test]
fn monotonic_commits_ready_only_last_good_and_immutable_snapshots() {
    let f = Fixture::new();
    let mut store = f.open();
    let first = commit(&mut store, SB, b"{}");
    assert_eq!(first.generation, 1);
    assert!(!first.current.as_ref().unwrap().ready);
    ready(&mut store, SB, b"{}");
    assert!(!first.current.as_ref().unwrap().ready);
    let second = commit(&mut store, SB, b"{\"x\":2}");
    assert_eq!(second.last_good.as_ref().unwrap().generation, 1);
    let third = commit(&mut store, SB, b"{\"x\":3}");
    assert_eq!(third.last_good.as_ref().unwrap().generation, 1);
    assert_eq!(third.current.as_ref().unwrap().generation, 3);
    assert_eq!(
        store
            .read_config(SB, first.current.as_ref().unwrap())
            .unwrap(),
        b"{}"
    );
    assert!(f.service(SB).join("config-2.json").exists());
    let pending = store.stage_candidate(SB, b"{}").unwrap();
    commit(&mut store, SB, b"{\"x\":4}");
    assert_eq!(
        store
            .commit_verified_candidate(pending, &proof(SB, b"{}"))
            .unwrap_err(),
        StoreError::Generation
    );
    assert_eq!(
        store
            .mark_ready(SB, &ReadinessProof::observed(SB, 3, b"{\"x\":3}"))
            .unwrap_err(),
        StoreError::Readiness
    );
}
#[test]
fn proven_last_good_restore_requires_bound_check_and_readiness() {
    let f = Fixture::new();
    let mut store = f.open();
    assert_eq!(
        store.stage_last_good(SB).unwrap_err(),
        StoreError::NotConfigured
    );
    commit(&mut store, SB, b"{}");
    ready(&mut store, SB, b"{}");
    commit(&mut store, SB, b"{\"bad\":true}");
    let candidate = store.stage_last_good(SB).unwrap();
    assert_eq!(
        store
            .restore_proven_last_good(
                candidate,
                &proof(SB, b"{}"),
                &ReadinessProof::observed(SB, 2, b"{}")
            )
            .unwrap_err(),
        StoreError::Readiness
    );
    let candidate = store.stage_last_good(SB).unwrap();
    let outcome = store
        .restore_proven_last_good(
            candidate,
            &proof(SB, b"{}"),
            &ReadinessProof::observed(SB, 1, b"{}"),
        )
        .unwrap();
    assert_eq!(outcome.state.generation, 3);
    assert!(outcome.state.current.as_ref().unwrap().ready);
    assert_eq!(
        outcome.state.current.as_ref().unwrap().file,
        "config-3.json"
    );
    drop(store);
    assert_eq!(f.open().snapshot(SB), outcome.state);
}
#[test]
fn generation_exhaustion_and_unknown_destination_do_not_reset_or_overwrite() {
    let f = Fixture::new();
    f.install(
        SB,
        &DiskState {
            generation: u64::MAX,
            ..DiskState::default()
        },
        &[],
    );
    let mut store = f.open();
    let candidate = store.stage_candidate(SB, b"{}").unwrap();
    assert_eq!(
        store
            .commit_verified_candidate(candidate, &proof(SB, b"{}"))
            .unwrap_err(),
        StoreError::GenerationExhausted
    );
    assert_eq!(store.service_state(SB).generation, u64::MAX);
    drop(store);
    f.install(
        SB,
        &DiskState::default(),
        &[("config-1.json", b"retain unknown")],
    );
    let mut store = f.open();
    let candidate = store.stage_candidate(SB, b"{}").unwrap();
    assert!(
        store
            .commit_verified_candidate(candidate, &proof(SB, b"{}"))
            .is_err()
    );
    assert_eq!(
        fs::read(f.service(SB).join("config-1.json")).unwrap(),
        b"retain unknown"
    );
}
#[test]
fn harden_existing_modes_and_fixed_error_debug() {
    let f = Fixture::new();
    let raw = b"{}";
    let state = DiskState {
        generation: 1,
        current: Some(record(1, "config-1.json", raw, true)),
        ..DiskState::default()
    };
    f.install(SB, &state, &[("config-1.json", raw)]);
    for path in [f.data(), f.service(SB)] {
        fs::set_permissions(path, Permissions::from_mode(0o777)).unwrap();
    }
    for path in [
        f.service(SB).join("state.json"),
        f.service(SB).join("config-1.json"),
    ] {
        fs::set_permissions(path, Permissions::from_mode(0o644)).unwrap();
    }
    let _store = f.open();
    for path in [
        f.service(SB).join("state.json"),
        f.service(SB).join("config-1.json"),
    ] {
        assert_eq!(fs::metadata(path).unwrap().mode() & 0o7777, 0o600);
    }
    let artifact = Artifact {
        url: "private-password-url".into(),
        sha256: "private-password-sha".into(),
        compression: "private-password-compression".into(),
        version: "private-password-version".into(),
    };
    assert!(!format!("{artifact:?}").contains("password"));
    for e in [
        StoreError::Storage,
        StoreError::InvalidState,
        StoreError::Verification,
    ] {
        assert!(!format!("{e} {e:?}").contains("private"));
    }
}
#[test]
fn directory_replacement_is_not_authorized_by_retained_fd() {
    let f = Fixture::new();
    let mut store = f.open();
    let candidate = store.stage_candidate(SB, b"{}").unwrap();
    fs::rename(f.service(SB), f.0.join("old-service")).unwrap();
    fs::create_dir(f.service(SB)).unwrap();
    assert!(
        store
            .commit_verified_candidate(candidate, &proof(SB, b"{}"))
            .is_err()
    );
    assert!(!f.service(SB).join("state.json").exists());
}
// Compile-time proof that candidates are the only accepted commit input, not arbitrary paths.
fn _candidate_type(_: Candidate) {}

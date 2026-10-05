#![cfg(unix)]
use be6500_panel::subscription_store::{Store, StoreError, MAX_BYTES};
use be6500_panel::readiness_tun::Budget;
use sha2::{Digest, Sha256};
use std::{fs, os::unix::fs::{DirBuilderExt, PermissionsExt, symlink}, path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering}, time::{Duration, Instant}};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture { root: PathBuf }
impl Fixture {
    fn new() -> Self {
        let root = fs::canonicalize(std::env::temp_dir()).unwrap().join(format!("b6p-subscription-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        Self { root }
    }
    fn source(&self) -> PathBuf { self.root.join("subscription.yaml") }
    fn put(&self, raw: &[u8]) {
        fs::write(self.source(), raw).unwrap();
        fs::set_permissions(self.source(),fs::Permissions::from_mode(0o600)).unwrap();
    }
}
impl Drop for Fixture { fn drop(&mut self) { fs::remove_dir_all(&self.root).unwrap(); } }
fn budget(cancel: &AtomicBool) -> Budget<'_> { Budget { deadline: Instant::now()+Duration::from_secs(30), cancel } }
#[test]
fn source_open_is_read_only_and_save_preserves_independent_draft() {
    let f = Fixture::new();
    let draft = f.root.join("local-rules.json"); fs::write(&draft,b"private independent draft").unwrap();
    let mut store = Store::open(&f.root).unwrap();
    assert_eq!(store.load().unwrap(),None);
    assert!(!f.source().exists());
    let cancel = AtomicBool::new(false);
    let outcome = store.save(b"proxies: []\n# private synthetic source",&budget(&cancel)).unwrap();
    assert!(outcome.committed && outcome.durability_error.is_none());
    assert_eq!(outcome.sha256, <[u8;32]>::from(Sha256::digest(b"proxies: []\n# private synthetic source")));
    assert_eq!(store.load().unwrap().unwrap(),b"proxies: []\n# private synthetic source");
    assert_eq!(fs::read(&draft).unwrap(),b"private independent draft");
    assert_eq!(fs::read_dir(&f.root).unwrap().count(),2);
    assert_eq!(fs::metadata(f.source()).unwrap().permissions().mode() & 0o7777,0o600);
    assert!(!format!("{store:?} {outcome:?}").contains("private synthetic"));
    assert!(!format!("{store:?} {outcome:?}").contains(f.root.to_str().unwrap()));
    drop(store);
    assert!(Store::open(&f.root).unwrap().load().unwrap().is_some());
}
#[test]
fn source_is_opaque_unicode_and_empty_not_a_yaml_parser() {
    let f = Fixture::new(); let mut store = Store::open(&f.root).unwrap();
    let cancel = AtomicBool::new(false);
    for raw in [b"not valid YAML [ :".as_slice(), "订阅\nprivate: 凭据".as_bytes(), b""] {
        assert!(store.save(raw,&budget(&cancel)).unwrap().committed);
        assert_eq!(store.load().unwrap().unwrap(),raw);
    }
}
#[test]
fn source_exact_two_mib_and_precommit_limit_refusal_keep_old_hash() {
    let f = Fixture::new(); let mut store = Store::open(&f.root).unwrap(); let cancel=AtomicBool::new(false);
    let raw=vec![b'x';MAX_BYTES]; store.save(&raw,&budget(&cancel)).unwrap();
    assert_eq!(store.load().unwrap().unwrap().len(),MAX_BYTES);
    let oversized=vec![b'y';MAX_BYTES+1];
    assert_eq!(store.save(&oversized,&budget(&cancel)),Err(StoreError::Limit));
    assert_eq!(Sha256::digest(fs::read(f.source()).unwrap()),Sha256::digest(&raw));
    f.put(&oversized); assert_eq!(Store::open(&f.root).unwrap_err(),StoreError::Limit);
}
#[test]
fn source_cancel_and_deadline_refuse_without_touching_prior_source() {
    let f=Fixture::new(); f.put(b"old"); let mut store=Store::open(&f.root).unwrap();
    let cancel=AtomicBool::new(true);
    assert_eq!(store.save(b"new",&budget(&cancel)),Err(StoreError::Cancelled));
    assert_eq!(store.load_until(&budget(&cancel)),Err(StoreError::Cancelled));
    cancel.store(false,Ordering::Release);
    let expired=Budget { deadline:Instant::now(),cancel:&cancel };
    assert_eq!(store.save(b"new",&expired),Err(StoreError::Deadline));
    assert_eq!(store.load_until(&expired),Err(StoreError::Deadline));
    assert_eq!(fs::read(f.source()).unwrap(),b"old");
    assert_eq!(fs::read_dir(&f.root).unwrap().count(),1);
}
#[test]
fn source_refuses_unsafe_modes_links_symlinks_and_fifo_without_repair() {
    let f=Fixture::new(); f.put(b"old");
    fs::set_permissions(f.source(),fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(Store::open(&f.root).unwrap_err(),StoreError::Invalid);
    assert_eq!(fs::metadata(f.source()).unwrap().permissions().mode()&0o7777,0o644);
    fs::set_permissions(f.source(),fs::Permissions::from_mode(0o600)).unwrap();
    fs::hard_link(f.source(),f.root.join("alias")).unwrap(); assert_eq!(Store::open(&f.root).unwrap_err(),StoreError::Invalid);
    fs::remove_file(f.root.join("alias")).unwrap();
    fs::rename(f.source(),f.root.join("old")).unwrap(); symlink(f.root.join("old"),f.source()).unwrap();
    assert_eq!(Store::open(&f.root).unwrap_err(),StoreError::Storage);
    fs::remove_file(f.source()).unwrap();
    let name=std::ffi::CString::new(f.source().as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe {libc::mkfifo(name.as_ptr(),0o600)},0);
    assert_eq!(Store::open(&f.root).unwrap_err(),StoreError::Invalid);
}
#[test]
fn source_existing_root_absolute_private_and_component_no_follow() {
    let f=Fixture::new();
    assert_eq!(Store::open(Path::new("relative")).unwrap_err(),StoreError::Invalid);
    assert_eq!(Store::open(&f.root.join("missing")).unwrap_err(),StoreError::Storage);
    assert!(!f.root.join("missing").exists());
    assert_eq!(Store::open(&f.root.join("./")).unwrap_err(),StoreError::Invalid);
    assert_eq!(Store::open(&f.root.join("../")).unwrap_err(),StoreError::Invalid);
    fs::set_permissions(&f.root,fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(Store::open(&f.root).unwrap_err(),StoreError::Invalid);
    assert_eq!(fs::metadata(&f.root).unwrap().permissions().mode()&0o7777,0o755);
    fs::set_permissions(&f.root,fs::Permissions::from_mode(0o700)).unwrap();
    symlink(&f.root,f.root.join("parent-alias")).unwrap();
    assert_eq!(Store::open(&f.root.join("parent-alias")).unwrap_err(),StoreError::Storage);
}
#[test]
fn source_external_replacement_or_same_inode_edit_cannot_overwrite_new_authority() {
    let f=Fixture::new(); f.put(b"old"); let mut store=Store::open(&f.root).unwrap(); let cancel=AtomicBool::new(false);
    fs::rename(f.source(),f.root.join("retained" )).unwrap(); f.put(b"foreign");
    assert_eq!(store.load(),Err(StoreError::Storage));
    assert_eq!(store.save(b"new",&budget(&cancel)),Err(StoreError::Storage));
    assert_eq!(fs::read(f.source()).unwrap(),b"foreign");
    drop(store); let mut store=Store::open(&f.root).unwrap(); f.put(b"changed length");
    assert_eq!(store.load(),Err(StoreError::Storage));
    assert_eq!(store.save(b"new",&budget(&cancel)),Err(StoreError::Storage));
    assert_eq!(fs::read(f.source()).unwrap(),b"changed length");
}
#[test]
fn source_pinned_root_replacement_is_refused_and_foreign_tree_untouched() {
    let f=Fixture::new(); f.put(b"old"); let mut store=Store::open(&f.root).unwrap(); let cancel=AtomicBool::new(false);
    let previous=f.root.with_extension("retained"); fs::rename(&f.root,&previous).unwrap();
    fs::DirBuilder::new().mode(0o700).create(&f.root).unwrap(); f.put(b"foreign");
    assert_eq!(store.load(),Err(StoreError::Storage));
    assert_eq!(store.save(b"new",&budget(&cancel)),Err(StoreError::Storage));
    assert_eq!(fs::read(f.source()).unwrap(),b"foreign");
    drop(store); fs::remove_dir_all(previous).unwrap();
}

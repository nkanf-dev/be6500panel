#![cfg(unix)]
use be6500_panel::runtime_intent::{DesiredServices, RuntimeIntent};
use be6500_panel::runtime_manager::ServiceId;
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, PermissionsExt, symlink},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "b6p-intent-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        Self(root)
    }
    fn write(&self, raw: &[u8]) {
        fs::write(self.0.join("desired-services.json"), raw).unwrap();
        fs::set_permissions(
            self.0.join("desired-services.json"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn missing_is_off_and_explicit_save_reopen_keeps_both_fixed_flags() {
    let fixture = Fixture::new();
    let mut intent = RuntimeIntent::open(&fixture.0).unwrap();
    assert_eq!(intent.desired(), DesiredServices::default());
    assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 0);
    let result = intent.save(ServiceId::SingBox, true).unwrap();
    assert!(result.durable && result.desired.sing_box);
    intent.save(ServiceId::Frpc, true).unwrap();
    intent.save(ServiceId::SingBox, false).unwrap();
    let reopened = RuntimeIntent::open(&fixture.0).unwrap();
    assert_eq!(
        reopened.desired(),
        DesiredServices {
            sing_box: false,
            frpc: true
        }
    );
    assert_eq!(
        fs::metadata(fixture.0.join("desired-services.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777,
        0o600
    );
    assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 1);
}
#[test]
fn old_partial_map_is_compatible_but_malformed_authority_is_not_reset() {
    let fixture = Fixture::new();
    fixture.write(br#"{"sing-box":true}"#);
    assert_eq!(
        RuntimeIntent::open(&fixture.0).unwrap().desired(),
        DesiredServices {
            sing_box: true,
            frpc: false
        }
    );
    for raw in [
        br#"{"sing-box":true,"sing-box":false}"#.as_slice(),
        br#"{"foreign":true}"#,
        br#"{"frpc":null}"#,
        br#"{"sing-box":"true"}"#,
        br#"[true,false]"#,
        b"{",
    ] {
        fixture.write(raw);
        assert!(RuntimeIntent::open(&fixture.0).is_err());
        assert_eq!(
            fs::read(fixture.0.join("desired-services.json")).unwrap(),
            raw
        );
    }
}
#[test]
fn unsafe_replacement_and_size_fail_before_commit() {
    let fixture = Fixture::new();
    let mut intent = RuntimeIntent::open(&fixture.0).unwrap();
    intent.save(ServiceId::SingBox, true).unwrap();
    let target = fixture.0.join("desired-services.json");
    fs::remove_file(&target).unwrap();
    symlink("foreign", &target).unwrap();
    assert!(intent.save(ServiceId::SingBox, false).is_err());
    assert!(
        fs::symlink_metadata(&target)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    fs::remove_file(target).unwrap();
    fixture.write(&vec![b' '; 4097]);
    assert!(RuntimeIntent::open(&fixture.0).is_err());
}

#[test]
fn private_permissions_links_and_in_place_updates_are_not_admitted() {
    let fixture = Fixture::new();
    fixture.write(br#"{"sing-box":true,"frpc":false}"#);
    let target = fixture.0.join("desired-services.json");
    fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(RuntimeIntent::open(&fixture.0).is_err());
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
    fs::hard_link(&target, fixture.0.join("extra-link")).unwrap();
    assert!(RuntimeIntent::open(&fixture.0).is_err());
    fs::remove_file(fixture.0.join("extra-link")).unwrap();
    let mut intent = RuntimeIntent::open(&fixture.0).unwrap();
    fixture.write(br#"{"sing-box":false,"frpc":false}"#);
    assert!(intent.save(ServiceId::Frpc, true).is_err());
    assert_eq!(
        fs::read(&target).unwrap(),
        br#"{"sing-box":false,"frpc":false}"#
    );
    fs::set_permissions(&fixture.0, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(RuntimeIntent::open(&fixture.0).is_err());
    fs::set_permissions(&fixture.0, fs::Permissions::from_mode(0o700)).unwrap();
}

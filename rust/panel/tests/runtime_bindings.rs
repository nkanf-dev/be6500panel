#![cfg(unix)]
use be6500_panel::runtime_bindings::{BindingError, Bindings};
use sha2::{Digest, Sha256};
use std::{fs, os::unix::fs::{DirBuilderExt, PermissionsExt, symlink}, path::{Path, PathBuf}, sync::atomic::{AtomicU64, Ordering}};
static NEXT: AtomicU64 = AtomicU64::new(0);
// Never run these bytes. Admission is only a bounded hash/metadata read.
const IP: &[u8] = b"#!/bin/sh\nprintf 'must not execute ip'\n";
const IPTABLES: &[u8] = b"#!/bin/sh\nprintf 'must not execute iptables'\n";
struct Fixture { root: PathBuf, manifest: PathBuf, ip: PathBuf, iptables: PathBuf }
impl Fixture {
    fn new() -> Self {
        let root = fs::canonicalize(std::env::temp_dir()).unwrap().join(format!("b6p-bindings-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let ip = root.join("ip");
        let iptables = root.join("iptables");
        for (path, raw) in [(&ip, IP), (&iptables, IPTABLES)] {
            fs::write(path, raw).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let manifest = root.join("bindings.json");
        let f = Self { root, manifest, ip, iptables };
        f.write(&f.valid());
        f
    }
    fn valid(&self) -> serde_json::Value {
        serde_json::json!({"ip":{"path":self.ip,"sha256":format!("{:x}",Sha256::digest(IP))},
            "iptables":{"path":self.iptables,"sha256":format!("{:x}",Sha256::digest(IPTABLES))},
            "dnsBootstrap":"127.0.0.1:53"})
    }
    fn write(&self, value: &serde_json::Value) { self.raw(&serde_json::to_vec(value).unwrap()); }
    fn raw(&self, raw: &[u8]) {
        fs::write(&self.manifest, raw).unwrap();
        fs::set_permissions(&self.manifest, fs::Permissions::from_mode(0o600)).unwrap();
    }
    fn fail(&self, error: BindingError) { assert_eq!(Bindings::load(&self.manifest).unwrap_err(), error); }
}
impl Drop for Fixture { fn drop(&mut self) { fs::remove_dir_all(&self.root).unwrap(); } }
#[test]
fn bindings_admit_hash_bound_fixed_commands_and_route_aliases_without_actions() {
    let f = Fixture::new();
    let loaded = Bindings::load(&f.manifest).unwrap();
    assert_eq!(loaded.names.get("main"), Some(&254));
    let before = fs::read(&f.manifest).unwrap();
    assert!(!format!("{loaded:?}").contains(f.root.to_str().unwrap()));
    assert!(!format!("{:?}",loaded.binaries).contains("must not execute"));
    assert_eq!(fs::read(&f.manifest).unwrap(), before);
    assert_eq!(fs::read_dir(&f.root).unwrap().count(), 3);
    let mut value = f.valid();
    value["routeTables"] = serde_json::json!("# fixed aliases\n16500 capture\n254 main\n16500 b6p\n");
    value["ip"]["sha256"] = serde_json::json!(format!("{:X}",Sha256::digest(IP)));
    f.write(&value);
    let loaded = Bindings::load(&f.manifest).unwrap();
    assert_eq!(loaded.names.get("capture"), Some(&16500));
    assert_eq!(loaded.names.get("b6p"), Some(&16500));
}
#[test]
fn bindings_top_and_nested_objects_are_required_strict_maps() {
    let f = Fixture::new();
    let good = f.valid();
    for value in [serde_json::Value::Null, serde_json::json!([]), serde_json::json!([good["ip"],good["iptables"],"127.0.0.1:53"]), serde_json::json!({})] {
        f.write(&value); f.fail(BindingError::Manifest);
    }
    for field in ["ip", "iptables", "dnsBootstrap"] {
        let mut missing = good.clone(); missing.as_object_mut().unwrap().remove(field); f.write(&missing); f.fail(BindingError::Manifest);
        let mut null = good.clone(); null[field] = serde_json::Value::Null; f.write(&null); f.fail(BindingError::Manifest);
    }
    for field in ["ip", "iptables"] {
        for nested in [serde_json::json!([good[field]["path"],good[field]["sha256"]]), serde_json::json!({}),
            serde_json::json!({"path":good[field]["path"],"sha256":good[field]["sha256"],"argv":["-4"]})] {
            let mut value = good.clone(); value[field] = nested; f.write(&value); f.fail(BindingError::Manifest);
        }
        for key in ["path", "sha256"] {
            let mut value = good.clone(); value[field].as_object_mut().unwrap().remove(key); f.write(&value); f.fail(BindingError::Manifest);
            let mut value = good.clone(); value[field][key] = serde_json::Value::Null; f.write(&value); f.fail(BindingError::Manifest);
        }
    }
    for field in ["argv", "pid", "artifact", "ca", "IP", "dns_bootstrap"] {
        let mut value = good.clone(); value[field] = serde_json::json!("private"); f.write(&value); f.fail(BindingError::Manifest);
    }
    let mut value = good; value["routeTables"] = serde_json::Value::Null; f.write(&value); f.fail(BindingError::Manifest);
}
#[test]
fn bindings_duplicate_fields_trailing_data_and_invalid_json_are_refused() {
    let f = Fixture::new();
    let valid = serde_json::to_string(&f.valid()).unwrap();
    for raw in [valid.replacen("{", "{\"ip\":{},", 1), valid.replacen("{", "{\"dnsBootstrap\":\"127.0.0.1:53\",", 1),
        valid.replacen("\"path\":", "\"path\":\"private\",\"path\":", 1),
        valid.replacen("\"sha256\":", "\"sha256\":\"private\",\"sha256\":", 1),
        format!("{valid} {{}}"), "{".into(), "".into(), format!("{valid}\0"),
        valid.replacen("{", "{\"routeTables\":\"\",\"routeTables\":\"\",", 1)] {
        f.raw(raw.as_bytes()); f.fail(BindingError::Manifest);
    }
}
#[test]
fn bindings_sha_and_binary_admission_never_accept_unverified_commands() {
    let f = Fixture::new();
    for text in ["".to_string(), "a".repeat(63), "a".repeat(65), "z".repeat(64), "0".repeat(64)] {
        let mut value = f.valid(); value["ip"]["sha256"] = serde_json::json!(text); f.write(&value); f.fail(BindingError::Binary);
    }
    for path in ["ip", "/nonexistent/private-command"] {
        let mut value = f.valid(); value["ip"]["path"] = serde_json::json!(path); f.write(&value); f.fail(BindingError::Binary);
    }
    f.write(&f.valid());
    fs::set_permissions(&f.ip, fs::Permissions::from_mode(0o722)).unwrap(); f.fail(BindingError::Binary);
    fs::set_permissions(&f.ip, fs::Permissions::from_mode(0o600)).unwrap(); f.fail(BindingError::Binary);
    fs::set_permissions(&f.ip, fs::Permissions::from_mode(0o700)).unwrap();
    fs::hard_link(&f.ip, f.root.join("linked-command")).unwrap(); f.fail(BindingError::Binary);
}
#[test]
fn bindings_dns_bootstrap_is_literal_explicit_port_native_policy_only() {
    let f = Fixture::new();
    for bootstrap in ["127.0.0.1:53", "192.168.50.1:5353", "[::1]:53", "[2001:db8::1]:53"] {
        let mut value = f.valid(); value["dnsBootstrap"] = serde_json::json!(bootstrap); f.write(&value); Bindings::load(&f.manifest).unwrap();
    }
    for bootstrap in ["localhost:53", "127.0.0.1", "127.0.0.1:0", "127.0.0.1:65536", "0.0.0.0:53", "224.0.0.1:53", "[::]:53", "[ff02::1]:53",
        "[::ffff:127.0.0.1]:53", "[fe80::1%2]:53", "http://127.0.0.1:53", " 127.0.0.1:53", "127.0.0.1:53 "] {
        let mut value = f.valid(); value["dnsBootstrap"] = serde_json::json!(bootstrap); f.write(&value); f.fail(BindingError::Bootstrap);
    }
}
#[test]
fn bindings_manifest_path_private_regular_no_follow_and_no_mode_repair() {
    let f = Fixture::new();
    fs::set_permissions(&f.manifest, fs::Permissions::from_mode(0o644)).unwrap(); f.fail(BindingError::Manifest);
    assert_eq!(fs::metadata(&f.manifest).unwrap().permissions().mode() & 0o7777, 0o644);
    fs::set_permissions(&f.manifest, fs::Permissions::from_mode(0o600)).unwrap();
    let alias = f.root.join("alias.json"); symlink(&f.manifest, &alias).unwrap();
    assert_eq!(Bindings::load(&alias).unwrap_err(), BindingError::Manifest);
    let parent_alias = f.root.join("parent-alias"); symlink(&f.root, &parent_alias).unwrap();
    assert_eq!(Bindings::load(&parent_alias.join("bindings.json")).unwrap_err(), BindingError::Manifest);
    assert_eq!(Bindings::load(Path::new("bindings.json")).unwrap_err(), BindingError::Manifest);
    assert_eq!(Bindings::load(&f.root.join("../").join(f.root.file_name().unwrap()).join("bindings.json")).unwrap_err(), BindingError::Manifest);
    assert_eq!(Bindings::load(&f.root.join("./bindings.json")).unwrap_err(), BindingError::Manifest);
    fs::hard_link(&f.manifest, f.root.join("hardlink.json")).unwrap(); f.fail(BindingError::Manifest);
}
#[test]
fn bindings_regular_file_and_total_four_kib_limit_are_enforced() {
    let f = Fixture::new();
    let mut exact = serde_json::to_vec(&f.valid()).unwrap(); exact.resize(4096, b' '); f.raw(&exact);
    Bindings::load(&f.manifest).unwrap();
    exact.push(b' '); f.raw(&exact); f.fail(BindingError::Manifest);
    fs::remove_file(&f.manifest).unwrap(); fs::DirBuilder::new().mode(0o600).create(&f.manifest).unwrap(); f.fail(BindingError::Manifest);
    fs::set_permissions(&f.manifest, fs::Permissions::from_mode(0o700)).unwrap(); fs::remove_dir(&f.manifest).unwrap();
    let path = std::ffi::CString::new(f.manifest.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0); f.fail(BindingError::Manifest);
}
#[test]
fn bindings_route_table_parser_rejects_conflicts_and_bad_optional_types() {
    let f = Fixture::new();
    for route in ["16500 capture\n16501 capture\n", "1 main", "capture 16500", "16500 capture extra", "4294967296 cap", "1 abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmno"] {
        let mut value = f.valid(); value["routeTables"] = serde_json::json!(route); f.write(&value); f.fail(BindingError::RouteTables);
    }
    for route in [serde_json::json!(42), serde_json::json!([])] {
        let mut value = f.valid(); value["routeTables"] = route; f.write(&value); f.fail(BindingError::Manifest);
    }
}
#[test]
fn bindings_system_applet_symlink_alias_uses_existing_release_hash_admission() {
    let f = Fixture::new();
    let applet = f.root.join("ip-applet"); symlink(&f.ip, &applet).unwrap();
    let mut value = f.valid(); value["ip"]["path"] = serde_json::json!(applet); f.write(&value);
    Bindings::load(&f.manifest).unwrap();
    value["ip"]["sha256"] = serde_json::json!("0".repeat(64)); f.write(&value); f.fail(BindingError::Binary);
}
#[test]
fn bindings_errors_and_debug_do_not_disclose_manifest_paths_or_hashes() {
    let f = Fixture::new();
    let mut value = f.valid(); value["ip"]["sha256"] = serde_json::json!("private-hash-secret"); f.write(&value);
    let error = Bindings::load(&f.manifest).unwrap_err();
    let text = format!("{error:?} {error}");
    assert!(!text.contains("private-hash-secret")); assert!(!text.contains(f.root.to_str().unwrap()));
}

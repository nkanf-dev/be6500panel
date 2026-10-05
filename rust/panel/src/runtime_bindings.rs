//! Trusted local release command and optional core bindings. Loading reads the
//! private manifest and admits fixed command binaries; it never executes,
//! fetches, adopts a core or repairs paths.
use crate::{
    artifact_source::SourcePolicy,
    capture_executor::{Binaries, TrustedBinary},
    capture_kernel::{self, TableNames},
    readiness_tun::FileIdentity,
    runtime_manager::{ArtifactBinding, ArtifactBindings, ArtifactProvenance, ServiceId},
};
use serde::{
    Deserialize, Deserializer,
    de::{self, MapAccess, Visitor},
};
use std::{
    ffi::CString,
    fmt,
    fs::File,
    io::{self, Read},
    net::SocketAddr,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::{ffi::OsStrExt, fs::MetadataExt},
    },
    path::{Component, Path},
};
const MAX_MANIFEST_BYTES: usize = 4096;

pub struct Bindings {
    pub binaries: Binaries,
    pub names: TableNames,
    pub source: SourcePolicy,
    pub artifacts: ArtifactBindings,
}
impl fmt::Debug for Bindings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RuntimeBindings([private])")
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingError {
    Manifest,
    Binary,
    Bootstrap,
    RouteTables,
}
impl fmt::Display for BindingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Manifest => "native bindings manifest unavailable or invalid",
            Self::Binary => "native command binding admission failed",
            Self::Bootstrap => "native DNS bootstrap invalid",
            Self::RouteTables => "native route table bindings invalid",
        })
    }
}
impl std::error::Error for BindingError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Manifest {
    #[serde(deserialize_with = "command_object")]
    ip: Command,
    #[serde(deserialize_with = "command_object")]
    iptables: Command,
    dns_bootstrap: String,
    #[serde(default)]
    route_tables: String,
    #[serde(default, deserialize_with = "optional_command_object")]
    sing_box: Option<Command>,
    #[serde(default, deserialize_with = "optional_command_object")]
    frpc: Option<Command>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Command {
    path: String,
    sha256: String,
}
fn command_object<'de, D: Deserializer<'de>>(d: D) -> Result<Command, D::Error> {
    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = Command;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("command binding object")
        }
        fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<Command, M::Error> {
            Command::deserialize(de::value::MapAccessDeserializer::new(map))
        }
    }
    d.deserialize_map(V)
}
fn optional_command_object<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Option<Command>, D::Error> {
    command_object(d).map(Some)
}

// The private manifest supplies release trust. Manager admits the private root;
// its existing checker/start path verifies the exact executable against this SHA.
// Do not hash a core twice or execute it while loading bindings.
fn local_artifact(
    service: ServiceId,
    command: Option<Command>,
) -> Result<Option<ArtifactBinding>, BindingError> {
    let Some(command) = command else {
        return Ok(None);
    };
    let sha256 = digest(&command.sha256)?;
    let path = Path::new(&command.path);
    if !path.is_absolute()
        || path.as_os_str().as_bytes().contains(&0)
        || path.as_os_str().as_bytes().split(|b| *b == b'/')
            .any(|c| c == b"." || c == b"..")
        || path.file_name().is_none()
    {
        return Err(BindingError::Binary);
    }
    let root = path.parent().ok_or(BindingError::Binary)?;
    if root == Path::new("/") {
        return Err(BindingError::Binary);
    }
    Ok(Some(ArtifactBinding::trusted_local(
        service,
        root,
        path,
        sha256,
        ArtifactProvenance::TrustedLocalModule,
    )))
}

struct Object(Manifest);
impl<'de> Deserialize<'de> for Object {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Object;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("bindings manifest object")
            }
            fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<Object, M::Error> {
                Manifest::deserialize(de::value::MapAccessDeserializer::new(map)).map(Object)
            }
        }
        d.deserialize_map(V)
    }
}
fn digest(text: &str) -> Result<[u8; 32], BindingError> {
    if text.len() != 64 {
        return Err(BindingError::Binary);
    }
    let mut value = [0; 32];
    for (slot, pair) in value.iter_mut().zip(text.as_bytes().chunks_exact(2)) {
        let nibble = |b: u8| match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        };
        *slot = (nibble(pair[0]).ok_or(BindingError::Binary)? << 4)
            | nibble(pair[1]).ok_or(BindingError::Binary)?;
    }
    Ok(value)
}
struct PinnedManifest {
    file: File,
    directory: File,
    identity: FileIdentity,
    parent: FileIdentity,
}
impl PinnedManifest {
    fn open(path: &Path) -> Result<Self, BindingError> {
        if !path.is_absolute()
            || path
                .as_os_str()
                .as_bytes()
                .split(|b| *b == b'/')
                .any(|c| c == b"." || c == b"..")
        {
            return Err(BindingError::Manifest);
        }
        let mut components = path.components().peekable();
        if components.next() != Some(Component::RootDir) {
            return Err(BindingError::Manifest);
        }
        let fd = unsafe {
            libc::open(
                c"/".as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(BindingError::Manifest);
        }
        let mut directory = unsafe { OwnedFd::from_raw_fd(fd) };
        while let Some(component) = components.next() {
            let Component::Normal(name) = component else {
                return Err(BindingError::Manifest);
            };
            let name = CString::new(name.as_bytes()).map_err(|_| BindingError::Manifest)?;
            let last = components.peek().is_none();
            let flags = libc::O_RDONLY
                | libc::O_NOFOLLOW
                | libc::O_CLOEXEC
                | if last {
                    libc::O_NONBLOCK
                } else {
                    libc::O_DIRECTORY
                };
            let fd = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
            if fd < 0 {
                return Err(BindingError::Manifest);
            }
            let next = unsafe { OwnedFd::from_raw_fd(fd) };
            if last {
                let file = File::from(next);
                let metadata = file.metadata().map_err(|_| BindingError::Manifest)?;
                if !metadata.is_file()
                    || metadata.mode() & 0o7777 != 0o600
                    || metadata.uid() != unsafe { libc::geteuid() }
                    || metadata.nlink() != 1
                    || metadata.len() == 0
                    || metadata.len() > MAX_MANIFEST_BYTES as u64
                {
                    return Err(BindingError::Manifest);
                }
                let directory = File::from(directory);
                let parent = FileIdentity::from_metadata(
                    &directory.metadata().map_err(|_| BindingError::Manifest)?,
                );
                return Ok(Self {
                    file,
                    directory,
                    identity: FileIdentity::from_metadata(&metadata),
                    parent,
                });
            }
            directory = next;
        }
        Err(BindingError::Manifest)
    }
    fn checked(&self, path: &Path) -> Result<(), BindingError> {
        let current = Self::open(path)?;
        let retained =
            FileIdentity::from_metadata(&self.file.metadata().map_err(|_| BindingError::Manifest)?);
        let parent = FileIdentity::from_metadata(
            &self
                .directory
                .metadata()
                .map_err(|_| BindingError::Manifest)?,
        );
        if retained != self.identity
            || current.identity != self.identity
            || parent != self.parent
            || current.parent != self.parent
        {
            return Err(BindingError::Manifest);
        }
        Ok(())
    }
    fn read(&mut self) -> Result<Vec<u8>, BindingError> {
        let mut raw = Vec::with_capacity(MAX_MANIFEST_BYTES);
        let mut bytes = [0; 512];
        loop {
            let size = bytes
                .len()
                .min(MAX_MANIFEST_BYTES.saturating_sub(raw.len()) + 1);
            let n = match self.file.read(&mut bytes[..size]) {
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                result => result.map_err(|_| BindingError::Manifest)?,
            };
            if n == 0 {
                break;
            }
            if n > MAX_MANIFEST_BYTES.saturating_sub(raw.len()) {
                return Err(BindingError::Manifest);
            }
            raw.extend_from_slice(&bytes[..n]);
        }
        if raw.len() as u64 != self.identity.size {
            return Err(BindingError::Manifest);
        }
        Ok(raw)
    }
}
impl Bindings {
    pub fn load(path: &Path) -> Result<Self, BindingError> {
        let mut pinned = PinnedManifest::open(path)?;
        let raw = pinned.read()?;
        pinned.checked(path)?;
        let Object(manifest) = serde_json::from_slice(&raw).map_err(|_| BindingError::Manifest)?;
        // Validate all syntax before any potentially large executable hashing.
        let ip_sha = digest(&manifest.ip.sha256)?;
        let iptables_sha = digest(&manifest.iptables.sha256)?;
        let artifacts = ArtifactBindings {
            sing_box: local_artifact(ServiceId::SingBox, manifest.sing_box)?,
            frpc: local_artifact(ServiceId::Frpc, manifest.frpc)?,
        };
        let bootstrap: SocketAddr = manifest
            .dns_bootstrap
            .parse()
            .map_err(|_| BindingError::Bootstrap)?;
        let source = SourcePolicy::native(bootstrap).map_err(|_| BindingError::Bootstrap)?;
        let names = capture_kernel::table_names(manifest.route_tables.as_bytes())
            .map_err(|_| BindingError::RouteTables)?;
        let ip = TrustedBinary::admit(Path::new(&manifest.ip.path), ip_sha)
            .map_err(|_| BindingError::Binary)?;
        let iptables = TrustedBinary::admit(Path::new(&manifest.iptables.path), iptables_sha)
            .map_err(|_| BindingError::Binary)?;
        pinned.checked(path)?;
        Ok(Self {
            binaries: Binaries { ip, iptables },
            names,
            source,
            artifacts,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::{DirBuilderExt, PermissionsExt},
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture {
        root: PathBuf,
        path: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let root = fs::canonicalize(std::env::temp_dir())
                .unwrap()
                .join(format!(
                    "b6p-bind-pin-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
            fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
            let path = root.join("manifest.json");
            fs::write(&path, b"{}").unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            Self { root, path }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).unwrap();
        }
    }
    #[test]
    fn optional_local_release_bindings_default_none_and_load_without_execution() {
        use sha2::{Digest, Sha256};
        let f = Fixture::new();
        let command = f.root.join("command");
        let marker = f.root.join("executed");
        let source = format!("#!/bin/sh\nprintf executed > '{}'\nexit 9\n", marker.display());
        fs::write(&command, source.as_bytes()).unwrap();
        fs::set_permissions(&command, fs::Permissions::from_mode(0o700)).unwrap();
        let sha = format!("{:x}", Sha256::digest(source.as_bytes()));
        let core = f.root.join("core");
        fs::write(&core, source.as_bytes()).unwrap();
        fs::set_permissions(&core, fs::Permissions::from_mode(0o700)).unwrap();
        let mut manifest = serde_json::json!({
            "ip": {"path": command, "sha256": sha},
            "iptables": {"path": command, "sha256": sha},
            "dnsBootstrap": "127.0.0.1:53"
        });
        fs::write(&f.path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let defaults = Bindings::load(&f.path).unwrap();
        assert!(defaults.artifacts.sing_box.is_none());
        assert!(defaults.artifacts.frpc.is_none());
        manifest["singBox"] = serde_json::json!({"path": core, "sha256": sha});
        manifest["frpc"] = manifest["singBox"].clone();
        fs::write(&f.path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let bound = Bindings::load(&f.path).unwrap();
        assert!(bound.artifacts.sing_box.is_some());
        assert!(bound.artifacts.frpc.is_some());
        assert!(!marker.exists());
    }
    #[test]
    fn optional_core_commands_require_objects_and_valid_release_paths_and_sha() {
        let base = serde_json::json!({
            "ip": {"path": "/private/ip", "sha256": "00".repeat(32)},
            "iptables": {"path": "/private/iptables", "sha256": "00".repeat(32)},
            "dnsBootstrap": "127.0.0.1:53"
        });
        for invalid in [
            serde_json::Value::Null,
            serde_json::json!(["/private/core", "00".repeat(32)]),
            serde_json::json!({"path": "/private/core"}),
            serde_json::json!({"path": "/private/core", "sha256": "00".repeat(32), "argv": []}),
        ] {
            for field in ["singBox", "frpc"] {
                let mut raw = base.clone();
                raw[field] = invalid.clone();
                assert!(serde_json::from_value::<Object>(raw).is_err());
            }
        }
        for path in ["relative/core", "/private/../core", "/private/./core", "/core", "/"] {
            assert!(local_artifact(ServiceId::SingBox, Some(Command {
                path: path.into(), sha256: "00".repeat(32),
            })).is_err());
        }
        assert!(local_artifact(ServiceId::Frpc, Some(Command {
            path: "/private/core".into(), sha256: "invalid".into(),
        })).is_err());
        let duplicate = r#"{"ip":{"path":"/private/ip","sha256":""},"iptables":{"path":"/private/iptables","sha256":""},"dnsBootstrap":"127.0.0.1:53","singBox":{},"singBox":{}}"#;
        assert!(serde_json::from_str::<Object>(duplicate).is_err());
    }
    #[test]
    fn pinned_manifest_refuses_replacement_and_permission_change() {
        let f = Fixture::new();
        let mut pinned = PinnedManifest::open(&f.path).unwrap();
        assert_eq!(pinned.read().unwrap(), b"{}");
        pinned.checked(&f.path).unwrap();
        fs::rename(&f.path, f.root.join("retained.json")).unwrap();
        fs::write(&f.path, b"{}").unwrap();
        fs::set_permissions(&f.path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(pinned.checked(&f.path), Err(BindingError::Manifest));
        let pinned = PinnedManifest::open(&f.path).unwrap();
        fs::set_permissions(&f.path, fs::Permissions::from_mode(0o640)).unwrap();
        assert_eq!(pinned.checked(&f.path), Err(BindingError::Manifest));
    }
    #[test]
    fn pinned_manifest_refuses_growth_before_read_without_unbounded_allocation() {
        let f = Fixture::new();
        let mut pinned = PinnedManifest::open(&f.path).unwrap();
        fs::write(&f.path, [b' '; MAX_MANIFEST_BYTES + 1]).unwrap();
        assert_eq!(pinned.read(), Err(BindingError::Manifest));
    }
}

//! Private, bounded local policy drafts. Saving never compiles or applies rules.
//!
//! One owner uses ordinary `&mut Store`; no lock, CAS, runtime, or subscription
//! reader is involved. Only `local-proxy-rules.json` is loaded. A snapshot owns
//! its policy. Reads allocate at most 256 KiB + one sentinel byte; typed arrays
//! stop at the policy collection bounds. Save serialization is capped at 256
//! KiB, before a temporary file is created. Owned policy memory is additionally
//! bounded by the existing policy field and collection limits.
//!
//! Unix directory-relative operations pin the checked private directory.
//! Admission measures actual available space and charges the entire rounded
//! temporary document plus one allocation unit, while keeping 1 MiB free.
//! Existing allocated emergency reserves are already excluded from available
//! space. This store never creates, changes, or reclaims such reserves.
use crate::policy::{
    LocalRule, MAX_LOCAL_RULES, MAX_SUBSCRIPTION_EDITS, Policy, SubscriptionEdit, policy_revision,
    validate_policy,
};
use serde::{Deserialize, Deserializer, de};
use std::ffi::{CStr, CString};
use std::fmt;
use std::fs::{self, DirBuilder, File, OpenOptions, Permissions};
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};

pub const FILE_NAME: &str = "local-proxy-rules.json";
pub const MAX_FILE_BYTES: usize = 256 << 10;
pub const FREE_HEADROOM_BYTES: u64 = 1 << 20;
const WRITE_CHUNK_BYTES: usize = 32 << 10;
const DOCUMENT: &CStr = c"local-proxy-rules.json";

/// Fixed errors contain no paths, rejected input, or underlying OS messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreError {
    InvalidInput,
    InvalidPolicy,
    DocumentSize,
    Storage,
    InsufficientSpace,
    Measurement,
}
impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidInput => "invalid local proxy rule store input",
            Self::InvalidPolicy => "invalid local proxy policy",
            Self::DocumentSize => "local proxy policy document exceeds storage limit",
            Self::Storage => "local proxy rule storage unavailable",
            Self::InsufficientSpace => {
                "persistent storage needs free space for safe configuration recovery"
            }
            Self::Measurement => "persistent storage free space is unavailable",
        })
    }
}
impl std::error::Error for StoreError {}

#[derive(Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub policy: Policy,
    /// Semantic policy SHA256, not a runtime-config hash or generation counter.
    pub revision: String,
}
impl fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Snapshot")
            .field("policy", &"[redacted]")
            .field("revision", &self.revision)
            .finish()
    }
}

/// `save` returns Err only before rename, preserving the accepted snapshot.
/// After rename it returns Ok with `committed = true`, even when directory
/// fsync fails. In that case `durability_error = Some(StoreError::Storage)` and
/// both this snapshot and the store's snapshot describe the committed policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveOutcome {
    pub snapshot: Snapshot,
    pub committed: bool,
    pub durability_error: Option<StoreError>,
}

pub struct Store {
    directory: PathBuf,
    identity: (u64, u64),
    state: Snapshot,
    #[cfg(test)]
    fault: Option<Fault>,
}
impl fmt::Debug for Store {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Store")
            .field("snapshot", &self.state)
            .finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredPolicy {
    #[serde(deserialize_with = "local_rules")]
    rules: Vec<LocalRule>,
    #[serde(deserialize_with = "subscription_edits")]
    subscription_edits: Vec<StoredSubscriptionEdit>,
}

// Omitted replacement means None, as in Go. An explicitly present replacement
// must be a Rule object, never null. Keep this stricter disk contract private
// instead of changing the public Policy defaults or optional-field semantics.
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
struct StoredSubscriptionEdit {
    id: String,
    source_fingerprint: String,
    disabled: bool,
    #[serde(deserialize_with = "present_replacement")]
    replacement: Option<crate::policy::Rule>,
    label: String,
    note: String,
}
fn present_replacement<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<crate::policy::Rule>, D::Error> {
    crate::policy::Rule::deserialize(deserializer).map(Some)
}
impl From<StoredSubscriptionEdit> for SubscriptionEdit {
    fn from(edit: StoredSubscriptionEdit) -> Self {
        Self {
            id: edit.id,
            source_fingerprint: edit.source_fingerprint,
            disabled: edit.disabled,
            replacement: edit.replacement,
            label: edit.label,
            note: edit.note,
        }
    }
}

fn bounded_array<'de, D, T, const LIMIT: usize>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Array<T, const LIMIT: usize>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const LIMIT: usize> de::Visitor<'de> for Array<T, LIMIT> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a bounded policy array")
        }
        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut out = Vec::new();
            while out.len() < LIMIT {
                let Some(value) = seq.next_element()? else {
                    return Ok(out);
                };
                out.push(value);
            }
            if seq.next_element::<de::IgnoredAny>()?.is_some() {
                return Err(de::Error::custom("policy collection limit exceeded"));
            }
            Ok(out)
        }
    }
    deserializer.deserialize_seq(Array::<T, LIMIT>(std::marker::PhantomData))
}
fn local_rules<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<LocalRule>, D::Error> {
    bounded_array::<D, LocalRule, MAX_LOCAL_RULES>(d)
}
fn subscription_edits<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Vec<StoredSubscriptionEdit>, D::Error> {
    bounded_array::<D, StoredSubscriptionEdit, MAX_SUBSCRIPTION_EDITS>(d)
}

struct BoundedBytes(Vec<u8>);
impl Write for BoundedBytes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let length = self
            .0
            .len()
            .checked_add(bytes.len())
            .filter(|length| *length <= MAX_FILE_BYTES)
            .ok_or_else(|| io::Error::other("document limit"))?;
        if length > self.0.capacity() {
            let capacity = length
                .max(self.0.capacity().saturating_mul(2))
                .min(MAX_FILE_BYTES);
            self.0
                .try_reserve_exact(capacity - self.0.len())
                .map_err(|_| io::Error::other("document allocation"))?;
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn snapshot(policy: Policy) -> Result<Snapshot, StoreError> {
    let revision = policy_revision(&policy).map_err(|_| StoreError::InvalidPolicy)?;
    Ok(Snapshot { policy, revision })
}
fn normalized_path(path: &Path) -> Result<PathBuf, StoreError> {
    if path.as_os_str().is_empty() {
        return Err(StoreError::InvalidInput);
    }
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|_| StoreError::Storage)?
            .join(path)
    };
    // Remove final dot/separators so O_NOFOLLOW checks the actual final name.
    Ok(absolute
        .components()
        .filter(|part| *part != Component::CurDir)
        .collect())
}
fn open_directory(path: &Path) -> Result<File, StoreError> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| StoreError::Storage)
}
fn directory_identity(directory: &File) -> Result<(u64, u64), StoreError> {
    let metadata = directory.metadata().map_err(|_| StoreError::Storage)?;
    if !metadata.is_dir() || metadata.mode() & 0o7777 != 0o700 {
        return Err(StoreError::Storage);
    }
    Ok((metadata.dev(), metadata.ino()))
}
fn open_at(directory: &File, name: &CStr, flags: i32, mode: libc::mode_t) -> io::Result<File> {
    // SAFETY: the descriptor is live and the CStr is terminated; ownership of
    // the new descriptor moves into File only after a successful openat.
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            flags,
            libc::c_uint::from(mode),
        )
    };
    if fd < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}
fn document(directory: &File) -> Result<Option<File>, StoreError> {
    let file = match open_at(
        directory,
        DOCUMENT,
        libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
        0,
    ) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(StoreError::Storage),
    };
    let metadata = file.metadata().map_err(|_| StoreError::Storage)?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES as u64 {
        return Err(StoreError::Storage);
    }
    Ok(Some(file))
}
fn read_policy(directory: &File) -> Result<Policy, StoreError> {
    let Some(mut file) = document(directory)? else {
        return Ok(Policy::default());
    };
    // Allocate a fixed upper bound, not a growing buffer based on an untrusted
    // file length. The extra byte detects growth after metadata inspection.
    let mut bytes = vec![0; MAX_FILE_BYTES + 1];
    let mut length = 0;
    while length < bytes.len() {
        match file.read(&mut bytes[length..]) {
            Ok(0) => break,
            Ok(n) => length += n,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(StoreError::Storage),
        }
    }
    if length > MAX_FILE_BYTES {
        return Err(StoreError::Storage);
    }
    bytes.truncate(length);
    let stored: StoredPolicy = serde_json::from_slice(&bytes).map_err(|_| StoreError::Storage)?;
    let policy = Policy {
        rules: stored.rules,
        subscription_edits: stored
            .subscription_edits
            .into_iter()
            .map(Into::into)
            .collect(),
    };
    validate_policy(&policy).map_err(|_| StoreError::Storage)?;
    file.set_permissions(Permissions::from_mode(0o600))
        .map_err(|_| StoreError::Storage)?;
    Ok(policy)
}

impl Store {
    /// DataDir is required. Create/harden it to 0700, without following a final
    /// directory symlink. A malformed/unsafe draft fails closed, never resets.
    pub fn open(data_dir: impl AsRef<Path>) -> Result<Self, StoreError> {
        let directory = normalized_path(data_dir.as_ref())?;
        match fs::symlink_metadata(&directory) {
            Ok(metadata) if !metadata.is_dir() => return Err(StoreError::Storage),
            Ok(_) => (),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                DirBuilder::new()
                    .recursive(true)
                    .mode(0o700)
                    .create(&directory)
                    .map_err(|_| StoreError::Storage)?;
            }
            Err(_) => return Err(StoreError::Storage),
        }
        let opened = open_directory(&directory)?;
        opened
            .set_permissions(Permissions::from_mode(0o700))
            .map_err(|_| StoreError::Storage)?;
        let identity = directory_identity(&opened)?;
        let mut store = Self {
            directory,
            identity,
            state: snapshot(Policy::default())?,
            #[cfg(test)]
            fault: None,
        };
        store.load()?;
        Ok(store)
    }
    fn checked_directory(&self) -> Result<File, StoreError> {
        let directory = open_directory(&self.directory)?;
        if directory_identity(&directory)? != self.identity {
            return Err(StoreError::Storage);
        }
        Ok(directory)
    }
    /// Owned deep clone; no disk read and no effect on accepted state.
    pub fn snapshot(&self) -> Snapshot {
        self.state.clone()
    }
    /// Reload only this draft. Errors retain the previous accepted snapshot.
    pub fn load(&mut self) -> Result<Snapshot, StoreError> {
        let directory = self.checked_directory()?;
        let policy = read_policy(&directory)?;
        let candidate = snapshot(policy).map_err(|_| StoreError::Storage)?;
        self.checked_directory()?;
        self.state = candidate;
        Ok(self.snapshot())
    }
    /// Validate/serialize within bounds before any disk write. Precommit errors
    /// retain the accepted policy and clean the temporary file. See SaveOutcome
    /// for the separate committed-but-directory-sync-failed result.
    pub fn save(&mut self, policy: &Policy) -> Result<SaveOutcome, StoreError> {
        validate_policy(policy).map_err(|_| StoreError::InvalidPolicy)?;
        let mut bytes = BoundedBytes(Vec::with_capacity(4096));
        serde_json::to_writer(&mut bytes, policy).map_err(|_| StoreError::DocumentSize)?;
        let candidate = snapshot(policy.clone())?;
        let directory = self.checked_directory()?;
        document(&directory)?;
        self.admit(&directory, bytes.0.len())?;
        // Check directory fsync support before the commit boundary.
        self.sync_directory(&directory, false)?;
        let (mut file, temporary) = temporary_file(&directory)?;
        let write_result = (|| {
            file.set_permissions(Permissions::from_mode(0o600))
                .map_err(|_| StoreError::Storage)?;
            for (i, chunk) in bytes.0.chunks(WRITE_CHUNK_BYTES).enumerate() {
                let n = self.write_chunk(&mut file, chunk, i)?;
                if n != chunk.len() {
                    return Err(StoreError::Storage);
                }
            }
            self.sync_file(&file)?;
            close_file(file)?;
            #[cfg(test)]
            if self.fault == Some(Fault::DirectorySwap) {
                let previous = self.directory.with_extension("old-private-directory");
                fs::rename(&self.directory, &previous).map_err(|_| StoreError::Storage)?;
                DirBuilder::new()
                    .mode(0o700)
                    .create(&self.directory)
                    .map_err(|_| StoreError::Storage)?;
            }
            self.checked_directory()?;
            document(&directory)?;
            self.rename(&directory, &temporary)?;
            Ok(())
        })();
        if let Err(error) = write_result {
            remove_temporary(&directory, &temporary);
            return Err(error);
        }
        // Atomic rename is the commit boundary. Never report the old state
        // after this point, including when final durability cannot be proved.
        self.state = candidate;
        let durability_error = self.sync_directory(&directory, true).err();
        Ok(SaveOutcome {
            snapshot: self.snapshot(),
            committed: true,
            durability_error,
        })
    }
    fn admit(&self, directory: &File, length: usize) -> Result<(), StoreError> {
        #[cfg(test)]
        match self.fault {
            Some(Fault::Measurement) => return Err(StoreError::Measurement),
            Some(Fault::InsufficientSpace) => return admit_space(0, 4096, length),
            _ => (),
        }
        let mut measured = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        // SAFETY: fstatvfs writes the structure on success; fd remains live.
        if unsafe { libc::fstatvfs(directory.as_raw_fd(), measured.as_mut_ptr()) } != 0 {
            return Err(StoreError::Measurement);
        }
        let measured = unsafe { measured.assume_init() };
        // u64::from is needed on 32-bit targets, even though hosts use u64.
        #[allow(clippy::useless_conversion)]
        let unit = u64::from(if measured.f_frsize == 0 {
            measured.f_bsize
        } else {
            measured.f_frsize
        });
        #[allow(clippy::useless_conversion)]
        let available = u64::from(measured.f_bavail)
            .checked_mul(unit)
            .ok_or(StoreError::Measurement)?;
        admit_space(available, unit, length)
    }
    fn write_chunk(
        &self,
        file: &mut File,
        chunk: &[u8],
        _index: usize,
    ) -> Result<usize, StoreError> {
        #[cfg(test)]
        match self.fault {
            Some(Fault::Write) => return Err(StoreError::Storage),
            Some(Fault::LaterWrite) if _index > 0 => return Err(StoreError::Storage),
            Some(Fault::ShortWrite) => {
                return file
                    .write(&chunk[..chunk.len() / 2])
                    .map_err(|_| StoreError::Storage);
            }
            _ => (),
        }
        file.write(chunk).map_err(|_| StoreError::Storage)
    }
    fn sync_file(&self, file: &File) -> Result<(), StoreError> {
        #[cfg(test)]
        if self.fault == Some(Fault::FileSync) {
            return Err(StoreError::Storage);
        }
        file.sync_all().map_err(|_| StoreError::Storage)
    }
    fn sync_directory(&self, directory: &File, _committed: bool) -> Result<(), StoreError> {
        #[cfg(test)]
        if self.fault
            == Some(if _committed {
                Fault::CommittedDirectorySync
            } else {
                Fault::DirectorySync
            })
        {
            return Err(StoreError::Storage);
        }
        directory.sync_all().map_err(|_| StoreError::Storage)
    }
    fn rename(&self, directory: &File, temporary: &CStr) -> Result<(), StoreError> {
        #[cfg(test)]
        if self.fault == Some(Fault::Rename) {
            return Err(StoreError::Storage);
        }
        // SAFETY: both names are terminated and the same live fd pins their directory.
        if unsafe {
            libc::renameat(
                directory.as_raw_fd(),
                temporary.as_ptr(),
                directory.as_raw_fd(),
                DOCUMENT.as_ptr(),
            )
        } == 0
        {
            Ok(())
        } else {
            Err(StoreError::Storage)
        }
    }
}

fn admit_space(available: u64, unit: u64, length: usize) -> Result<(), StoreError> {
    if unit == 0 {
        return Err(StoreError::Measurement);
    }
    // Match the Go budget's conservative extra allocation unit for metadata.
    let unit = unit.max(4096);
    let charge = (length as u64)
        .checked_add(unit - 1)
        .and_then(|n| (n / unit).checked_add(1))
        .and_then(|n| n.checked_mul(unit))
        .ok_or(StoreError::Measurement)?;
    if available
        .checked_sub(charge)
        .is_none_or(|free| free < FREE_HEADROOM_BYTES)
    {
        Err(StoreError::InsufficientSpace)
    } else {
        Ok(())
    }
}
fn temporary_file(directory: &File) -> Result<(File, CString), StoreError> {
    for _ in 0..16 {
        let mut random = [0_u8; 16];
        getrandom::fill(&mut random).map_err(|_| StoreError::Storage)?;
        let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        let name = CString::new(format!(".local-proxy-rules-{suffix}"))
            .map_err(|_| StoreError::Storage)?;
        match open_at(
            directory,
            &name,
            libc::O_WRONLY
                | libc::O_CREAT
                | libc::O_EXCL
                | libc::O_NOFOLLOW
                | libc::O_NONBLOCK
                | libc::O_CLOEXEC,
            0o600,
        ) {
            Ok(file) => return Ok((file, name)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(StoreError::Storage),
        }
    }
    Err(StoreError::Storage)
}
fn close_file(file: File) -> Result<(), StoreError> {
    // SAFETY: into_raw_fd transfers ownership. Do not retry close on EINTR;
    // the fd may already have been closed and must not be reused by this code.
    if unsafe { libc::close(file.into_raw_fd()) } == 0 {
        Ok(())
    } else {
        Err(StoreError::Storage)
    }
}
fn remove_temporary(directory: &File, temporary: &CStr) {
    // SAFETY: this name identifies only our same-directory temporary file.
    // Cleanup is best effort when the filesystem itself refuses deletion.
    unsafe { libc::unlinkat(directory.as_raw_fd(), temporary.as_ptr(), 0) };
}

#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fault {
    Write,
    LaterWrite,
    ShortWrite,
    FileSync,
    Rename,
    DirectorySync,
    CommittedDirectorySync,
    Measurement,
    InsufficientSpace,
    DirectorySwap,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{Rule, RuleKind, Target};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let directory = std::env::temp_dir().join(format!(
                "be6500-store-fault-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            DirBuilder::new().mode(0o700).create(&directory).unwrap();
            Self(directory)
        }
        fn open(&self) -> Store {
            Store::open(&self.0).unwrap()
        }
        fn bytes(&self) -> Vec<u8> {
            fs::read(self.0.join(FILE_NAME)).unwrap()
        }
        fn no_temporaries(&self) {
            assert!(fs::read_dir(&self.0).unwrap().all(|entry| {
                !entry
                    .unwrap()
                    .file_name()
                    .as_encoded_bytes()
                    .starts_with(b".local-proxy-rules-")
            }));
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn draft(count: usize) -> Policy {
        Policy {
            rules: (0..count)
                .map(|i| LocalRule {
                    id: format!("local-{i}"),
                    enabled: true,
                    label: "private label".into(),
                    note: "n".repeat(256),
                    rule: Rule {
                        kind: RuleKind::Domain,
                        value: "example.com".into(),
                        target: Target::Proxy,
                        index: 0,
                        no_resolve: false,
                    },
                })
                .collect(),
            subscription_edits: vec![],
        }
    }
    #[test]
    fn all_precommit_failures_preserve_disk_memory_and_clean_tempfiles() {
        for fault in [
            Fault::Write,
            Fault::ShortWrite,
            Fault::LaterWrite,
            Fault::FileSync,
            Fault::Rename,
            Fault::DirectorySync,
            Fault::Measurement,
            Fault::InsufficientSpace,
        ] {
            let fixture = Fixture::new();
            let mut store = fixture.open();
            store.save(&draft(1)).unwrap();
            let accepted = store.snapshot();
            let bytes = fixture.bytes();
            store.fault = Some(fault);
            let error = store.save(&draft(100)).unwrap_err();
            let expected = match fault {
                Fault::Measurement => StoreError::Measurement,
                Fault::InsufficientSpace => StoreError::InsufficientSpace,
                _ => StoreError::Storage,
            };
            assert_eq!(error, expected);
            assert_eq!(store.snapshot(), accepted);
            assert_eq!(fixture.bytes(), bytes);
            assert_eq!(fixture.open().snapshot(), accepted);
            fixture.no_temporaries();
            store.fault = None;
            assert!(store.save(&draft(100)).unwrap().committed);
            fixture.no_temporaries();
        }
    }
    #[test]
    fn postrename_directory_sync_error_reports_committed_owned_snapshot() {
        let fixture = Fixture::new();
        let mut store = fixture.open();
        store.save(&draft(1)).unwrap();
        store.fault = Some(Fault::CommittedDirectorySync);
        let input = draft(100);
        let mut result = store.save(&input).unwrap();
        assert!(result.committed);
        assert_eq!(result.durability_error, Some(StoreError::Storage));
        assert_eq!(result.snapshot, store.snapshot());
        assert_eq!(fixture.open().snapshot(), store.snapshot());
        assert_eq!(fixture.bytes(), serde_json::to_vec(&input).unwrap());
        result.snapshot.policy.rules[0].note.clear();
        assert_eq!(store.snapshot().policy, input);
        assert!(!format!("{result:?}").contains("private label"));
        fixture.no_temporaries();
    }
    #[test]
    fn admission_charges_the_full_rounded_temporary_document_and_keeps_headroom() {
        for (length, unit, charge) in [
            (1, 4096, 8192),
            (4096, 4096, 8192),
            (4097, 4096, 12288),
            (MAX_FILE_BYTES, 4096, 266240),
            (MAX_FILE_BYTES, 8192, 270336),
            (1, 512, 8192),
        ] {
            assert_eq!(
                admit_space(FREE_HEADROOM_BYTES + charge, unit, length),
                Ok(())
            );
            assert_eq!(
                admit_space(FREE_HEADROOM_BYTES + charge - 1, unit, length),
                Err(StoreError::InsufficientSpace)
            );
        }
        assert_eq!(admit_space(u64::MAX, 0, 1), Err(StoreError::Measurement));
        assert_eq!(
            admit_space(u64::MAX, u64::MAX, 1),
            Err(StoreError::Measurement)
        );
    }
    #[test]
    fn admission_and_failures_never_change_an_existing_emergency_reserve() {
        let fixture = Fixture::new();
        let reserve = fixture.0.join(".emergency-reserve");
        let existing = vec![0x5a; 256 << 10];
        fs::write(&reserve, &existing).unwrap();
        let mut store = fixture.open();
        store.save(&draft(1)).unwrap();
        store.fault = Some(Fault::InsufficientSpace);
        assert_eq!(
            store.save(&draft(100)).unwrap_err(),
            StoreError::InsufficientSpace
        );
        assert_eq!(fs::read(&reserve).unwrap(), existing);
        fixture.no_temporaries();
    }
    #[test]
    fn serialization_does_not_grow_past_the_document_cap() {
        let mut writer = BoundedBytes(Vec::with_capacity(4096));
        let chunk = [b'x'; 4096];
        for _ in 0..MAX_FILE_BYTES / chunk.len() {
            writer.write_all(&chunk).unwrap();
        }
        assert_eq!(writer.0.len(), MAX_FILE_BYTES);
        assert_eq!(writer.0.capacity(), MAX_FILE_BYTES);
        assert!(writer.write_all(b"x").is_err());
        assert_eq!(writer.0.len(), MAX_FILE_BYTES);
    }

    #[test]
    fn swap_during_save_rejects_replacement_and_cleans_temp_in_pinned_old_directory() {
        let fixture = Fixture::new();
        let data = fixture.0.join("private");
        let mut store = Store::open(&data).unwrap();
        store.save(&draft(1)).unwrap();
        let accepted = store.snapshot();
        let accepted_bytes = fs::read(data.join(FILE_NAME)).unwrap();
        store.fault = Some(Fault::DirectorySwap);
        assert_eq!(store.save(&draft(100)).unwrap_err(), StoreError::Storage);
        assert_eq!(store.snapshot(), accepted);
        assert!(!data.join(FILE_NAME).exists());
        let old = data.with_extension("old-private-directory");
        assert_eq!(fs::read(old.join(FILE_NAME)).unwrap(), accepted_bytes);
        assert_eq!(fs::read_dir(&old).unwrap().count(), 1);
        assert_eq!(Store::open(&old).unwrap().snapshot(), accepted);
    }

    #[test]
    fn typed_arrays_stop_at_the_bound_without_deserializing_the_excess_object() {
        struct Counted;
        impl<'de> Deserialize<'de> for Counted {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                CALLS.fetch_add(1, Ordering::Relaxed);
                de::IgnoredAny::deserialize(deserializer).map(|_| Self)
            }
        }
        static CALLS: AtomicU64 = AtomicU64::new(0);
        let mut decoder = serde_json::Deserializer::from_slice(b"[{}, {}, {}, {}]");
        assert!(bounded_array::<_, Counted, 3>(&mut decoder).is_err());
        assert_eq!(CALLS.load(Ordering::Relaxed), 3);
    }
}

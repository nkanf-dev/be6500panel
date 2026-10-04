//! Exclusive, fixed-service runtime config storage. No process or network work.
//!
//! `VerificationProof` and `ReadinessProof` are explicit caller attestations,
//! not validators. Only the future manager, after a real checker/readiness
//! operation, should construct them. This module binds them to stored bytes.
//! Checking a config does not prove local readiness. Artifact metadata never
//! authorizes an executable, download, PID adoption, or run-directory contents.
//!
//! One owner holds the existing `.manager.lock` for this store's lifetime and
//! mutates through ordinary `&mut self`. Constructor loading never prunes files.
//! No snapshots are pruned here: a later manager must prove durable recovery
//! and successful owned withdrawal before deciding which snapshots to remove.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::ffi::{CStr, CString};
use std::fmt;
use std::fs::{self, DirBuilder, File, OpenOptions, Permissions};
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};

pub const MAX_METADATA_BYTES: usize = 16 << 10;
pub const MAX_CONFIG_BYTES: usize = 512 << 10;
pub const MAX_STORED_CONFIG_BYTES: usize = 4 << 20;
pub const FREE_HEADROOM_BYTES: u64 = 1 << 20;
const CHUNK: usize = 32 << 10;
const MANIFEST: &CStr = c"state.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceId {
    SingBox,
    Frpc,
}
impl ServiceId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SingBox => "sing-box",
            Self::Frpc => "frpc",
        }
    }
    const fn index(self) -> usize {
        match self {
            Self::SingBox => 0,
            Self::Frpc => 1,
        }
    }
}
/// Fixed errors never contain config bytes, URLs, paths, or OS error text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreError {
    InvalidInput,
    OverlappingRoots,
    Busy,
    Storage,
    InvalidState,
    ConfigSize,
    Verification,
    Readiness,
    Generation,
    GenerationExhausted,
    NotConfigured,
    InsufficientSpace,
    Measurement,
    Durability,
}
impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidInput => "invalid runtime store input",
            Self::OverlappingRoots => {
                "persistent and runtime directories must be physically separate"
            }
            Self::Busy => "runtime data directory is already in use",
            Self::Storage => "runtime storage unavailable",
            Self::InvalidState => "invalid runtime state",
            Self::ConfigSize => "runtime configuration exceeds storage limit",
            Self::Verification => "candidate verification does not match unchanged stored bytes",
            Self::Readiness => "readiness evidence does not match accepted configuration",
            Self::Generation => "staged configuration generation is no longer current",
            Self::GenerationExhausted => "configuration generation exhausted",
            Self::NotConfigured => "proven configuration unavailable",
            Self::InsufficientSpace => {
                "persistent storage needs free space for safe configuration recovery"
            }
            Self::Measurement => "persistent storage free space unavailable",
            Self::Durability => "runtime state committed without confirmed directory durability",
        })
    }
}
impl std::error::Error for StoreError {}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Artifact {
    pub url: String,
    pub sha256: String,
    pub compression: String,
    pub version: String,
}
impl fmt::Debug for Artifact {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Artifact([redacted])")
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct ConfigRecord {
    pub generation: u64,
    pub file: String,
    pub sha256: String,
    #[serde(skip_serializing_if = "is_false")]
    pub ready: bool,
}
fn is_false(value: &bool) -> bool {
    !*value
}
impl fmt::Debug for ConfigRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConfigRecord")
            .field("generation", &self.generation)
            .field("ready", &self.ready)
            .finish_non_exhaustive()
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Default, Debug)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct DiskState {
    pub generation: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<ConfigRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_good: Option<ConfigRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact: Option<Artifact>,
}
/// A caller attests a *successful real native checker* examined these exact
/// bytes. This constructor hashes the supplied bytes; it does not run a check.
#[derive(Clone)]
pub struct VerificationProof {
    service: ServiceId,
    sha256: String,
    length: usize,
}
impl VerificationProof {
    pub fn checked(service: ServiceId, checked_bytes: &[u8]) -> Self {
        Self {
            service,
            sha256: hash(checked_bytes),
            length: checked_bytes.len(),
        }
    }
}
impl fmt::Debug for VerificationProof {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VerificationProof")
            .field("service", &self.service)
            .finish_non_exhaustive()
    }
}
/// Caller attests real readiness for this exact accepted generation and bytes.
/// An HTTP success, artifact metadata, or a checker exit is not readiness.
#[derive(Clone)]
pub struct ReadinessProof {
    service: ServiceId,
    generation: u64,
    sha256: String,
    length: usize,
}
impl ReadinessProof {
    pub fn observed(service: ServiceId, generation: u64, ready_bytes: &[u8]) -> Self {
        Self {
            service,
            generation,
            sha256: hash(ready_bytes),
            length: ready_bytes.len(),
        }
    }
}
impl fmt::Debug for ReadinessProof {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReadinessProof")
            .field("service", &self.service)
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}
/// Err is always pre-manifest-rename and leaves the old accepted state.
/// Ok is authoritative even when durability_error is Some(Durability).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitOutcome {
    pub state: DiskState,
    pub durability_error: Option<StoreError>,
}

struct Directory {
    path: PathBuf,
    file: File,
    identity: (u64, u64),
}
impl Directory {
    fn checked(&self) -> Result<(), StoreError> {
        let current = open_directory(&self.path)?;
        let meta = current.metadata().map_err(|_| StoreError::Storage)?;
        if identity(&meta) != self.identity || meta.mode() & 0o7777 != 0o700 {
            return Err(StoreError::Storage);
        }
        Ok(())
    }
}
struct ServiceStore {
    dir: Directory,
    state: DiskState,
}
pub struct RuntimeStore {
    data: Directory,
    run: Directory,
    services: [ServiceStore; 2],
    _lock: File,
    owner: [u8; 16],
    #[cfg(test)]
    fault: Option<Fault>,
}
impl fmt::Debug for RuntimeStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RuntimeStore")
            .field("sing_box", &self.services[0].state)
            .field("frpc", &self.services[1].state)
            .finish_non_exhaustive()
    }
}
/// Exact owned temporary file. Drop cleans only the same owned inode, never a
/// replacement or an unknown `.candidate-*` path. Committed snapshots survive.
pub struct Candidate {
    // Retain the inode until cleanup/commit, preventing reuse from authorizing
    // deletion of a replacement file even if the old name was unlinked.
    file: File,
    directory: File,
    name: CString,
    path: PathBuf,
    identity: (u64, u64),
    directory_identity: (u64, u64),
    owner: [u8; 16],
    service: ServiceId,
    base_generation: u64,
    sha256: String,
    length: usize,
    extension: &'static str,
    restored: Option<ConfigRecord>,
    active: bool,
}
impl Candidate {
    /// Private path for the caller's fixed-service native checker only.
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
    pub fn len(&self) -> usize {
        self.length
    }
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }
    pub fn service(&self) -> ServiceId {
        self.service
    }
}
impl fmt::Debug for Candidate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Candidate")
            .field("service", &self.service)
            .field("length", &self.length)
            .finish_non_exhaustive()
    }
}
impl Drop for Candidate {
    fn drop(&mut self) {
        if self.active {
            remove_owned(&self.directory, &self.name, self.identity);
        }
    }
}

impl RuntimeStore {
    pub fn open(data_dir: impl AsRef<Path>, run_dir: impl AsRef<Path>) -> Result<Self, StoreError> {
        let data_path = normalize(data_dir.as_ref())?;
        let run_path = normalize(run_dir.as_ref())?;
        if overlap(&data_path, &run_path) {
            return Err(StoreError::OverlappingRoots);
        }
        let data = private_directory(&data_path)?;
        let run = private_directory(&run_path)?;
        if overlap(&data.path, &run.path) || data.identity == run.identity {
            return Err(StoreError::OverlappingRoots);
        }
        let lock = open_at(
            &data.file,
            c".manager.lock",
            libc::O_RDWR | libc::O_CREAT,
            0o600,
        )
        .map_err(storage)?;
        regular(&lock, u64::MAX)?;
        // SAFETY: lock is a live owned descriptor. Nonblocking exclusive flock
        // is held until File drops; it is compatible with the original Go lock.
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            let e = io::Error::last_os_error();
            return Err(if e.kind() == io::ErrorKind::WouldBlock {
                StoreError::Busy
            } else {
                StoreError::Storage
            });
        }
        let mut owner = [0; 16];
        getrandom::fill(&mut owner).map_err(|_| StoreError::Storage)?;
        let sing_box = service_store(&data, ServiceId::SingBox)?;
        let frpc = service_store(&data, ServiceId::Frpc)?;
        let store = Self {
            data,
            run,
            services: [sing_box, frpc],
            _lock: lock,
            owner,
            #[cfg(test)]
            fault: None,
        };
        store.checked(ServiceId::SingBox)?;
        store.checked(ServiceId::Frpc)?;
        Ok(store)
    }
    pub fn service_state(&self, service: ServiceId) -> &DiskState {
        &self.services[service.index()].state
    }
    pub fn snapshot(&self, service: ServiceId) -> DiskState {
        self.service_state(service).clone()
    }
    fn checked(&self, service: ServiceId) -> Result<&ServiceStore, StoreError> {
        self.data.checked()?;
        self.run.checked()?;
        let slot = &self.services[service.index()];
        slot.dir.checked()?;
        Ok(slot)
    }
    /// Reads exactly a controlled accepted config name and validates its hash.
    /// Read-only metadata carries no executable or PID trust.
    pub fn read_config(
        &self,
        service: ServiceId,
        record: &ConfigRecord,
    ) -> Result<Vec<u8>, StoreError> {
        let slot = self.checked(service)?;
        validate_record(service, &slot.state, record)?;
        let name = c_name(&record.file)?;
        let mut file = required_file(&slot.dir.file, &name, MAX_STORED_CONFIG_BYTES)?;
        let bytes = read_bounded(&mut file, MAX_STORED_CONFIG_BYTES)?;
        if hash(&bytes) != record.sha256 {
            return Err(StoreError::InvalidState);
        }
        self.checked(service)?;
        Ok(bytes)
    }
    pub fn stage_candidate(
        &mut self,
        service: ServiceId,
        raw: &[u8],
    ) -> Result<Candidate, StoreError> {
        self.stage(service, raw, None)
    }
    /// Copies one proven-ready lastGood config, including legacy accepted bytes
    /// up to 4 MiB. A caller must recheck and prove readiness before restore.
    pub fn stage_last_good(&mut self, service: ServiceId) -> Result<Candidate, StoreError> {
        let record = self
            .service_state(service)
            .last_good
            .clone()
            .filter(|r| r.ready)
            .ok_or(StoreError::NotConfigured)?;
        let raw = self.read_config(service, &record)?;
        self.stage(service, &raw, Some(record))
    }
    fn stage(
        &mut self,
        service: ServiceId,
        raw: &[u8],
        restored: Option<ConfigRecord>,
    ) -> Result<Candidate, StoreError> {
        let limit = if restored.is_some() {
            MAX_STORED_CONFIG_BYTES
        } else {
            MAX_CONFIG_BYTES
        };
        if raw.len() > limit {
            return Err(StoreError::ConfigSize);
        }
        let slot = self.checked(service)?;
        // Charge all prospective temporary growth (candidate + bounded manifest
        // and filesystem metadata), not merely net replacement size. No reserve
        // file is ever allocated, touched, or reclaimed here.
        self.admit(&slot.dir.file, &[raw.len(), MAX_METADATA_BYTES])?;
        let extension = extension(service, raw);
        let directory = slot.dir.file.try_clone().map_err(storage)?;
        let (file, name) = temporary(&slot.dir.file, ".candidate-", extension)?;
        let file_identity = match file.metadata() {
            Ok(metadata) => identity(&metadata),
            Err(error) => {
                // The just-created O_EXCL name is ours; no other name is touched.
                unlink(&slot.dir.file, &name);
                return Err(storage(error));
            }
        };
        let mut candidate = Candidate {
            file,
            directory,
            path: slot
                .dir
                .path
                .join(name.to_str().map_err(|_| StoreError::Storage)?),
            name,
            identity: file_identity,
            directory_identity: slot.dir.identity,
            owner: self.owner,
            service,
            base_generation: slot.state.generation,
            sha256: hash(raw),
            length: raw.len(),
            extension,
            restored,
            active: true,
        };
        self.write_bytes(&mut candidate.file, raw)?;
        self.sync_file(&candidate.file)?;
        self.checked(service)?;
        Ok(candidate)
    }
    pub fn commit_verified_candidate(
        &mut self,
        candidate: Candidate,
        proof: &VerificationProof,
    ) -> Result<CommitOutcome, StoreError> {
        if candidate.restored.is_some() {
            return Err(StoreError::Readiness);
        }
        self.commit(candidate, proof, None)
    }
    /// Restore advances the latest generation, not the older snapshot counter.
    /// Readiness must name the exact proven lastGood generation, not current.
    /// The caller must establish owned withdrawal and real readiness first.
    pub fn restore_proven_last_good(
        &mut self,
        candidate: Candidate,
        proof: &VerificationProof,
        readiness: &ReadinessProof,
    ) -> Result<CommitOutcome, StoreError> {
        let old = candidate
            .restored
            .as_ref()
            .ok_or(StoreError::NotConfigured)?;
        if self.service_state(candidate.service).last_good.as_ref() != Some(old)
            || !old.ready
            || !matches_ready(readiness, candidate.service, old, candidate.length)
        {
            return Err(StoreError::Readiness);
        }
        self.commit(candidate, proof, Some(readiness))
    }
    fn commit(
        &mut self,
        mut candidate: Candidate,
        proof: &VerificationProof,
        readiness: Option<&ReadinessProof>,
    ) -> Result<CommitOutcome, StoreError> {
        let service = candidate.service;
        let slot = self.checked(service)?;
        if candidate.owner != self.owner || candidate.directory_identity != slot.dir.identity {
            return Err(StoreError::Verification);
        }
        if candidate.base_generation != slot.state.generation {
            return Err(StoreError::Generation);
        }
        if proof.service != service
            || proof.sha256 != candidate.sha256
            || proof.length != candidate.length
        {
            return Err(StoreError::Verification);
        }
        let mut file = required_file(&slot.dir.file, &candidate.name, MAX_STORED_CONFIG_BYTES)
            .map_err(|_| StoreError::Verification)?;
        if identity(&file.metadata().map_err(|_| StoreError::Verification)?) != candidate.identity {
            return Err(StoreError::Verification);
        }
        let (digest, length) = stream_digest(&mut file, MAX_STORED_CONFIG_BYTES)
            .map_err(|_| StoreError::Verification)?;
        if digest != candidate.sha256 || length != candidate.length {
            return Err(StoreError::Verification);
        }
        self.sync_file(&file)?;
        drop(file);
        let generation = slot
            .state
            .generation
            .checked_add(1)
            .ok_or(StoreError::GenerationExhausted)?;
        let dest_string = format!("config-{generation}{}", candidate.extension);
        let dest = c_name(&dest_string)?;
        absent(&slot.dir.file, &dest)?;
        let mut next = slot.state.clone();
        next.generation = generation;
        if let Some(current) = next.current.as_ref().filter(|r| r.ready) {
            next.last_good = Some(current.clone());
        }
        next.current = Some(ConfigRecord {
            generation,
            file: dest_string,
            sha256: candidate.sha256.clone(),
            ready: readiness.is_some(),
        });
        let bytes = serialize_state(&next)?;
        // Candidate already occupies disk; charge the entire manifest temporary
        // plus metadata again immediately before committing, preserving headroom.
        self.admit(&slot.dir.file, &[bytes.len()])?;
        self.sync_directory(&slot.dir.file, SyncPoint::Before)?;
        self.checked(service)?;
        self.rename(&slot.dir.file, &candidate.name, &dest, RenamePoint::Config)?;
        candidate.active = false;
        let precommit = (|| {
            self.sync_directory(&slot.dir.file, SyncPoint::Config)?;
            self.persist(&slot.dir, &bytes)?;
            Ok(())
        })();
        if let Err(error) = precommit {
            remove_owned(&slot.dir.file, &dest, candidate.identity);
            return Err(error);
        }
        // persist returns only after the manifest rename. From now on next is
        // authoritative. No pruning or removal may follow uncertain durability.
        self.services[service.index()].state = next;
        let durability_error = self
            .sync_directory(
                &self.services[service.index()].dir.file,
                SyncPoint::Committed,
            )
            .err()
            .map(|_| StoreError::Durability);
        Ok(CommitOutcome {
            state: self.snapshot(service),
            durability_error,
        })
    }
    pub fn mark_ready(
        &mut self,
        service: ServiceId,
        proof: &ReadinessProof,
    ) -> Result<CommitOutcome, StoreError> {
        let slot = self.checked(service)?;
        let current = slot
            .state
            .current
            .as_ref()
            .ok_or(StoreError::NotConfigured)?;
        let mut file = required_file(
            &slot.dir.file,
            &c_name(&current.file)?,
            MAX_STORED_CONFIG_BYTES,
        )?;
        let (digest, length) = stream_digest(&mut file, MAX_STORED_CONFIG_BYTES)?;
        if digest != current.sha256 || !matches_ready(proof, service, current, length) {
            return Err(StoreError::Readiness);
        }
        let mut next = slot.state.clone();
        next.current
            .as_mut()
            .ok_or(StoreError::NotConfigured)?
            .ready = true;
        self.save_state(service, next)
    }
    fn save_state(
        &mut self,
        service: ServiceId,
        next: DiskState,
    ) -> Result<CommitOutcome, StoreError> {
        let bytes = serialize_state(&next)?;
        let slot = self.checked(service)?;
        self.admit(&slot.dir.file, &[bytes.len()])?;
        self.sync_directory(&slot.dir.file, SyncPoint::Before)?;
        self.persist(&slot.dir, &bytes)?;
        self.services[service.index()].state = next;
        let durability_error = self
            .sync_directory(
                &self.services[service.index()].dir.file,
                SyncPoint::Committed,
            )
            .err()
            .map(|_| StoreError::Durability);
        Ok(CommitOutcome {
            state: self.snapshot(service),
            durability_error,
        })
    }
    fn persist(&self, directory: &Directory, bytes: &[u8]) -> Result<(), StoreError> {
        let (mut file, name) = temporary(&directory.file, ".atomic-", "")?;
        let temporary_identity = match file.metadata() {
            Ok(metadata) => identity(&metadata),
            Err(error) => {
                unlink(&directory.file, &name);
                return Err(storage(error));
            }
        };
        // Keep the inode live through close/rename/cleanup. A failed rename may
        // not authorize removing another inode that reused this temporary name.
        let held = match file.try_clone() {
            Ok(held) => held,
            Err(error) => {
                remove_owned(&directory.file, &name, temporary_identity);
                return Err(storage(error));
            }
        };
        let result = (|| {
            self.write_bytes(&mut file, bytes)?;
            self.sync_file(&file)?;
            close_file(file)?;
            directory.checked()?;
            self.data.checked()?;
            if let Some(manifest) = optional_file(&directory.file, MANIFEST, MAX_METADATA_BYTES)? {
                drop(manifest);
            }
            self.rename(&directory.file, &name, MANIFEST, RenamePoint::Manifest)
        })();
        if result.is_err() {
            remove_owned(&directory.file, &name, temporary_identity);
        }
        drop(held);
        result
    }
    #[allow(clippy::unused_enumerate_index)] // Index is used only by the later-write test fault.
    fn write_bytes(&self, file: &mut File, bytes: &[u8]) -> Result<(), StoreError> {
        for (_index, chunk) in bytes.chunks(CHUNK).enumerate() {
            #[cfg(test)]
            match self.fault {
                Some(Fault::Write) => return Err(StoreError::Storage),
                Some(Fault::LaterWrite) if _index > 0 => return Err(StoreError::Storage),
                Some(Fault::ShortWrite) => {
                    file.write_all(&chunk[..chunk.len() / 2]).map_err(storage)?;
                    return Err(StoreError::Storage);
                }
                _ => (),
            }
            file.write_all(chunk).map_err(storage)?;
        }
        Ok(())
    }
    fn sync_file(&self, file: &File) -> Result<(), StoreError> {
        #[cfg(test)]
        if self.fault == Some(Fault::FileSync) {
            return Err(StoreError::Storage);
        }
        file.sync_all().map_err(storage)
    }
    fn sync_directory(&self, directory: &File, _point: SyncPoint) -> Result<(), StoreError> {
        #[cfg(test)]
        if self.fault
            == Some(match _point {
                SyncPoint::Before => Fault::BeforeSync,
                SyncPoint::Config => Fault::ConfigSync,
                SyncPoint::Committed => Fault::CommittedSync,
            })
        {
            return Err(StoreError::Storage);
        }
        directory.sync_all().map_err(storage)
    }
    fn rename(
        &self,
        dir: &File,
        from: &CStr,
        to: &CStr,
        _point: RenamePoint,
    ) -> Result<(), StoreError> {
        #[cfg(test)]
        if self.fault
            == Some(match _point {
                RenamePoint::Config => Fault::ConfigRename,
                RenamePoint::Manifest => Fault::ManifestRename,
            })
        {
            return Err(StoreError::Storage);
        }
        // SAFETY: names are terminated, controlled, single components. Both
        // paths are relative to the same live private directory descriptor.
        if unsafe { libc::renameat(dir.as_raw_fd(), from.as_ptr(), dir.as_raw_fd(), to.as_ptr()) }
            == 0
        {
            Ok(())
        } else {
            Err(storage(io::Error::last_os_error()))
        }
    }
    fn admit(&self, directory: &File, lengths: &[usize]) -> Result<(), StoreError> {
        #[cfg(test)]
        match self.fault {
            Some(Fault::Measurement) => return Err(StoreError::Measurement),
            Some(Fault::Space) => return admit_space(0, 4096, lengths),
            _ => (),
        }
        let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        // SAFETY: fd remains live; structure is initialized only on success.
        if unsafe { libc::fstatvfs(directory.as_raw_fd(), stat.as_mut_ptr()) } != 0 {
            return Err(StoreError::Measurement);
        }
        let stat = unsafe { stat.assume_init() };
        #[allow(clippy::useless_conversion)]
        let unit = u64::from(if stat.f_frsize == 0 {
            stat.f_bsize
        } else {
            stat.f_frsize
        });
        #[allow(clippy::useless_conversion)]
        let available = u64::from(stat.f_bavail)
            .checked_mul(unit)
            .ok_or(StoreError::Measurement)?;
        admit_space(available, unit, lengths)
    }
}
#[derive(Clone, Copy)]
enum SyncPoint {
    Before,
    Config,
    Committed,
}
#[derive(Clone, Copy)]
enum RenamePoint {
    Config,
    Manifest,
}
fn matches_ready(
    proof: &ReadinessProof,
    service: ServiceId,
    record: &ConfigRecord,
    length: usize,
) -> bool {
    proof.service == service
        && proof.generation == record.generation
        && proof.sha256 == record.sha256
        && proof.length == length
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn storage(error: io::Error) -> StoreError {
    if error.raw_os_error() == Some(libc::ENOSPC) {
        StoreError::InsufficientSpace
    } else {
        StoreError::Storage
    }
}
fn c_name(name: &str) -> Result<CString, StoreError> {
    CString::new(name).map_err(|_| StoreError::InvalidState)
}
fn identity(meta: &fs::Metadata) -> (u64, u64) {
    (meta.dev(), meta.ino())
}
fn overlap(a: &Path, b: &Path) -> bool {
    a.starts_with(b) || b.starts_with(a)
}
fn normalize(path: &Path) -> Result<PathBuf, StoreError> {
    if path.as_os_str().is_empty() {
        return Err(StoreError::InvalidInput);
    }
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir().map_err(storage)?.join(path)
    };
    if absolute.components().any(|c| c == Component::ParentDir) {
        return Err(StoreError::InvalidInput);
    }
    Ok(absolute
        .components()
        .filter(|c| *c != Component::CurDir)
        .collect())
}
fn open_directory(path: &Path) -> Result<File, StoreError> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
        .map_err(storage)
}
fn private_directory(path: &Path) -> Result<Directory, StoreError> {
    match fs::symlink_metadata(path) {
        Ok(meta) if !meta.is_dir() => return Err(StoreError::Storage),
        Ok(_) => (),
        Err(e) if e.kind() == io::ErrorKind::NotFound => DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)
            .map_err(storage)?,
        Err(e) => return Err(storage(e)),
    }
    let file = open_directory(path)?;
    file.set_permissions(Permissions::from_mode(0o700))
        .map_err(storage)?;
    let path = fs::canonicalize(path).map_err(storage)?;
    let dir = Directory {
        identity: identity(&file.metadata().map_err(storage)?),
        file,
        path,
    };
    dir.checked()?;
    Ok(dir)
}
fn open_at(dir: &File, name: &CStr, flags: i32, mode: libc::mode_t) -> io::Result<File> {
    // SAFETY: directory fd remains live; name is terminated. Ownership of a
    // successful descriptor transfers exactly once to File.
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
            libc::c_uint::from(mode),
        )
    };
    if fd < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}
fn regular(file: &File, limit: u64) -> Result<(), StoreError> {
    let meta = file.metadata().map_err(storage)?;
    if !meta.is_file() || meta.len() > limit || meta.nlink() != 1 {
        return Err(StoreError::InvalidState);
    }
    file.set_permissions(Permissions::from_mode(0o600))
        .map_err(storage)?;
    Ok(())
}
fn optional_file(dir: &File, name: &CStr, limit: usize) -> Result<Option<File>, StoreError> {
    match open_at(dir, name, libc::O_RDONLY, 0) {
        Ok(file) => {
            regular(&file, limit as u64)?;
            Ok(Some(file))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(storage(e)),
    }
}
fn required_file(dir: &File, name: &CStr, limit: usize) -> Result<File, StoreError> {
    optional_file(dir, name, limit)?.ok_or(StoreError::InvalidState)
}
fn read_bounded(file: &mut File, limit: usize) -> Result<Vec<u8>, StoreError> {
    let length = usize::try_from(file.metadata().map_err(storage)?.len())
        .map_err(|_| StoreError::InvalidState)?;
    if length > limit {
        return Err(StoreError::InvalidState);
    }
    // Allocate exactly the inspected length, not a doubling read_to_end buffer.
    // An extra scalar read detects growth without another full legacy buffer.
    let mut bytes = vec![0; length];
    file.read_exact(&mut bytes).map_err(storage)?;
    let mut extra = [0];
    loop {
        match file.read(&mut extra) {
            Ok(0) => return Ok(bytes),
            Ok(_) => return Err(StoreError::InvalidState),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(storage(error)),
        }
    }
}
fn stream_digest(file: &mut File, limit: usize) -> Result<(String, usize), StoreError> {
    let mut sha = Sha256::new();
    let mut length = 0usize;
    let mut bytes = [0; CHUNK];
    loop {
        let n = match file.read(&mut bytes) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(storage(e)),
        };
        length = length
            .checked_add(n)
            .filter(|n| *n <= limit)
            .ok_or(StoreError::InvalidState)?;
        sha.update(&bytes[..n]);
    }
    Ok((format!("{:x}", sha.finalize()), length))
}
fn validate_record(
    service: ServiceId,
    state: &DiskState,
    record: &ConfigRecord,
) -> Result<(), StoreError> {
    let prefix = format!("config-{}", record.generation);
    if record.generation == 0
        || record.generation > state.generation
        || !(record.file == format!("{prefix}.json")
            || service == ServiceId::Frpc && record.file == format!("{prefix}.toml"))
        || record.sha256.len() != 64
        || !record
            .sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(StoreError::InvalidState);
    }
    Ok(())
}
fn service_store(data: &Directory, service: ServiceId) -> Result<ServiceStore, StoreError> {
    let dir = private_directory(&data.path.join(service.as_str()))?;
    let mut state: DiskState = match optional_file(&dir.file, MANIFEST, MAX_METADATA_BYTES)? {
        None => DiskState::default(),
        Some(mut file) => serde_json::from_slice(&read_bounded(&mut file, MAX_METADATA_BYTES)?)
            .map_err(|_| StoreError::InvalidState)?,
    };
    // Go compatibility: legacy lastGood proves checking only and is excluded
    // before reference validation. It is not a rollback candidate.
    if state.last_good.as_ref().is_some_and(|r| !r.ready) {
        state.last_good = None;
    }
    if state.current.is_none() && state.last_good.is_some() {
        return Err(StoreError::InvalidState);
    }
    for record in [&state.current, &state.last_good].into_iter().flatten() {
        validate_record(service, &state, record)?;
        let mut file = required_file(&dir.file, &c_name(&record.file)?, MAX_STORED_CONFIG_BYTES)?;
        if stream_digest(&mut file, MAX_STORED_CONFIG_BYTES)?.0 != record.sha256 {
            return Err(StoreError::InvalidState);
        }
    }
    Ok(ServiceStore { dir, state })
}
fn extension(service: ServiceId, raw: &[u8]) -> &'static str {
    // IgnoredAny checks JSON validity without allocating a second config AST.
    if service == ServiceId::Frpc && serde_json::from_slice::<serde::de::IgnoredAny>(raw).is_err() {
        ".toml"
    } else {
        ".json"
    }
}
struct BoundedBytes(Vec<u8>);
impl Write for BoundedBytes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self
            .0
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > MAX_METADATA_BYTES)
        {
            return Err(io::Error::other("metadata limit"));
        }
        let length = self.0.len() + bytes.len();
        if length > self.0.capacity() {
            let capacity = length
                .max(self.0.capacity().saturating_mul(2))
                .min(MAX_METADATA_BYTES);
            self.0
                .try_reserve_exact(capacity - self.0.len())
                .map_err(|_| io::Error::other("metadata allocation"))?;
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn serialize_state(state: &DiskState) -> Result<Vec<u8>, StoreError> {
    let mut bytes = BoundedBytes(Vec::with_capacity(1024));
    serde_json::to_writer(&mut bytes, state).map_err(|_| StoreError::InvalidState)?;
    bytes
        .write_all(b"\n")
        .map_err(|_| StoreError::InvalidState)?;
    Ok(bytes.0)
}
fn admit_space(available: u64, unit: u64, lengths: &[usize]) -> Result<(), StoreError> {
    if unit == 0 {
        return Err(StoreError::Measurement);
    }
    let unit = unit.max(4096);
    let mut charge = unit;
    for &length in lengths {
        let rounded = (length as u64)
            .checked_add(unit - 1)
            .and_then(|n| (n / unit).checked_mul(unit))
            .ok_or(StoreError::Measurement)?;
        charge = charge.checked_add(rounded).ok_or(StoreError::Measurement)?;
    }
    if available
        .checked_sub(charge)
        .is_none_or(|n| n < FREE_HEADROOM_BYTES)
    {
        Err(StoreError::InsufficientSpace)
    } else {
        Ok(())
    }
}
fn temporary(dir: &File, prefix: &str, extension: &str) -> Result<(File, CString), StoreError> {
    for _ in 0..16 {
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).map_err(|_| StoreError::Storage)?;
        let suffix: String = random.iter().map(|b| format!("{b:02x}")).collect();
        let name = c_name(&format!("{prefix}{suffix}{extension}"))?;
        match open_at(
            dir,
            &name,
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
            0o600,
        ) {
            Ok(file) => {
                if let Err(error) = file.set_permissions(Permissions::from_mode(0o600)) {
                    unlink(dir, &name);
                    return Err(storage(error));
                }
                return Ok((file, name));
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(storage(e)),
        }
    }
    Err(StoreError::Storage)
}
fn absent(dir: &File, name: &CStr) -> Result<(), StoreError> {
    let mut meta = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: live fd, terminated name and valid output storage. No follow.
    if unsafe {
        libc::fstatat(
            dir.as_raw_fd(),
            name.as_ptr(),
            meta.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } == 0
    {
        return Err(StoreError::InvalidState);
    }
    let e = io::Error::last_os_error();
    if e.kind() == io::ErrorKind::NotFound {
        Ok(())
    } else {
        Err(storage(e))
    }
}
fn unlink(dir: &File, name: &CStr) {
    // SAFETY: only a terminated exact just-created owned name is passed.
    unsafe { libc::unlinkat(dir.as_raw_fd(), name.as_ptr(), 0) };
}
fn remove_owned(dir: &File, name: &CStr, expected: (u64, u64)) {
    let mut meta = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: live pinned fd and terminated owned name. Compare inode before
    // best-effort cleanup; replacements are not this store's responsibility.
    if unsafe {
        libc::fstatat(
            dir.as_raw_fd(),
            name.as_ptr(),
            meta.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } == 0
    {
        let meta = unsafe { meta.assume_init() };
        #[allow(clippy::unnecessary_cast)] // stat widths differ on ARM/Linux and macOS.
        let observed = (meta.st_dev as u64, meta.st_ino as u64);
        if observed == expected {
            unsafe { libc::unlinkat(dir.as_raw_fd(), name.as_ptr(), 0) };
        }
    }
}
fn close_file(file: File) -> Result<(), StoreError> {
    // SAFETY: into_raw_fd transfers ownership. Never retry close on EINTR.
    if unsafe { libc::close(file.into_raw_fd()) } == 0 {
        Ok(())
    } else {
        Err(storage(io::Error::last_os_error()))
    }
}
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fault {
    Write,
    LaterWrite,
    ShortWrite,
    FileSync,
    BeforeSync,
    ConfigSync,
    CommittedSync,
    ConfigRename,
    ManifestRename,
    Measurement,
    Space,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!(
                "be6500-runtime-fault-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            DirBuilder::new().mode(0o700).create(&dir).unwrap();
            Self(dir)
        }
        fn open(&self) -> RuntimeStore {
            RuntimeStore::open(self.0.join("services"), self.0.join("run")).unwrap()
        }
        fn dir(&self) -> PathBuf {
            self.0.join("services/sing-box")
        }
        fn manifest(&self) -> Vec<u8> {
            fs::read(self.dir().join("state.json")).unwrap()
        }
        fn no_temps(&self) {
            assert!(fs::read_dir(self.dir()).unwrap().all(|e| {
                let n = e.unwrap().file_name();
                !n.as_encoded_bytes().starts_with(b".candidate-")
                    && !n.as_encoded_bytes().starts_with(b".atomic-")
            }));
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn accept(store: &mut RuntimeStore, raw: &[u8]) -> CommitOutcome {
        let candidate = store.stage_candidate(ServiceId::SingBox, raw).unwrap();
        store
            .commit_verified_candidate(
                candidate,
                &VerificationProof::checked(ServiceId::SingBox, raw),
            )
            .unwrap()
    }
    #[test]
    fn stage_failures_clean_exact_temporary_and_keep_manifest() {
        for fault in [
            Fault::Write,
            Fault::LaterWrite,
            Fault::ShortWrite,
            Fault::FileSync,
            Fault::Measurement,
            Fault::Space,
        ] {
            let fixture = Fixture::new();
            let mut store = fixture.open();
            accept(&mut store, b"{}");
            let old = store.snapshot(ServiceId::SingBox);
            let manifest = fixture.manifest();
            fs::write(fixture.dir().join(".candidate-not-owned.json"), b"keep").unwrap();
            store.fault = Some(fault);
            assert!(
                store
                    .stage_candidate(ServiceId::SingBox, &vec![b'x'; CHUNK + 1])
                    .is_err()
            );
            assert_eq!(store.snapshot(ServiceId::SingBox), old);
            assert_eq!(fixture.manifest(), manifest);
            assert_eq!(
                fs::read(fixture.dir().join(".candidate-not-owned.json")).unwrap(),
                b"keep"
            );
            fs::remove_file(fixture.dir().join(".candidate-not-owned.json")).unwrap();
            fixture.no_temps();
        }
    }
    #[test]
    fn precommit_faults_keep_old_state_and_config_and_clean_new_files() {
        for fault in [
            Fault::Write,
            Fault::ShortWrite,
            Fault::FileSync,
            Fault::BeforeSync,
            Fault::ConfigSync,
            Fault::ConfigRename,
            Fault::ManifestRename,
            Fault::Measurement,
            Fault::Space,
        ] {
            let fixture = Fixture::new();
            let mut store = fixture.open();
            accept(&mut store, b"{}");
            let old = store.snapshot(ServiceId::SingBox);
            let manifest = fixture.manifest();
            let candidate = store
                .stage_candidate(ServiceId::SingBox, b"{\"private-password\":true}")
                .unwrap();
            store.fault = Some(fault);
            assert!(
                store
                    .commit_verified_candidate(
                        candidate,
                        &VerificationProof::checked(
                            ServiceId::SingBox,
                            b"{\"private-password\":true}"
                        )
                    )
                    .is_err()
            );
            assert_eq!(store.snapshot(ServiceId::SingBox), old);
            assert_eq!(fixture.manifest(), manifest);
            assert_eq!(
                fs::read(fixture.dir().join("config-1.json")).unwrap(),
                b"{}"
            );
            assert!(!fixture.dir().join("config-2.json").exists());
            fixture.no_temps();
            drop(store);
            assert_eq!(fixture.open().snapshot(ServiceId::SingBox), old);
        }
    }
    #[test]
    fn postmanifest_sync_failure_returns_authoritative_state_and_retains_snapshots() {
        let fixture = Fixture::new();
        let mut store = fixture.open();
        accept(&mut store, b"{}");
        store
            .mark_ready(
                ServiceId::SingBox,
                &ReadinessProof::observed(ServiceId::SingBox, 1, b"{}"),
            )
            .unwrap();
        let retained = store.snapshot(ServiceId::SingBox);
        let candidate = store
            .stage_candidate(ServiceId::SingBox, b"{\"next\":2}")
            .unwrap();
        store.fault = Some(Fault::CommittedSync);
        let outcome = store
            .commit_verified_candidate(
                candidate,
                &VerificationProof::checked(ServiceId::SingBox, b"{\"next\":2}"),
            )
            .unwrap();
        assert_eq!(outcome.durability_error, Some(StoreError::Durability));
        assert_eq!(outcome.state, store.snapshot(ServiceId::SingBox));
        assert_eq!(outcome.state.generation, 2);
        assert_eq!(outcome.state.last_good, retained.current);
        assert!(fixture.dir().join("config-1.json").exists());
        assert!(fixture.dir().join("config-2.json").exists());
        fixture.no_temps();
        drop(store);
        assert_eq!(fixture.open().snapshot(ServiceId::SingBox), outcome.state);
    }
    #[test]
    fn readiness_precommit_and_postcommit_faults_obey_same_boundary() {
        for fault in [
            Fault::Write,
            Fault::FileSync,
            Fault::BeforeSync,
            Fault::ManifestRename,
            Fault::Space,
            Fault::CommittedSync,
        ] {
            let fixture = Fixture::new();
            let mut store = fixture.open();
            accept(&mut store, b"{}");
            let old = store.snapshot(ServiceId::SingBox);
            let manifest = fixture.manifest();
            store.fault = Some(fault);
            let result = store.mark_ready(
                ServiceId::SingBox,
                &ReadinessProof::observed(ServiceId::SingBox, 1, b"{}"),
            );
            if fault == Fault::CommittedSync {
                let outcome = result.unwrap();
                assert!(outcome.state.current.unwrap().ready);
                assert_eq!(outcome.durability_error, Some(StoreError::Durability));
            } else {
                assert!(result.is_err());
                assert_eq!(store.snapshot(ServiceId::SingBox), old);
                assert_eq!(fixture.manifest(), manifest);
            }
            fixture.no_temps();
        }
    }
    #[test]
    fn admission_charges_complete_rounded_growth_and_reserve_is_untouched() {
        let charge = 4096 + 4096 + MAX_METADATA_BYTES as u64;
        assert_eq!(
            admit_space(
                FREE_HEADROOM_BYTES + charge - 1,
                4096,
                &[1, MAX_METADATA_BYTES]
            ),
            Err(StoreError::InsufficientSpace)
        );
        assert_eq!(
            admit_space(FREE_HEADROOM_BYTES + charge, 4096, &[1, MAX_METADATA_BYTES]),
            Ok(())
        );
        assert_eq!(admit_space(u64::MAX, 0, &[1]), Err(StoreError::Measurement));
        assert_eq!(
            admit_space(u64::MAX, 4096, &[usize::MAX]),
            Err(StoreError::Measurement)
        );
        let fixture = Fixture::new();
        let mut store = fixture.open();
        let reserve = fixture.0.join("services/.emergency-reserve");
        fs::write(&reserve, vec![7; 64 << 10]).unwrap();
        let old = fs::metadata(&reserve).unwrap();
        accept(&mut store, b"{}");
        store.fault = Some(Fault::Space);
        assert_eq!(
            store
                .stage_candidate(ServiceId::SingBox, b"{}")
                .unwrap_err(),
            StoreError::InsufficientSpace
        );
        assert_eq!(fs::read(&reserve).unwrap(), vec![7; 64 << 10]);
        let now = fs::metadata(reserve).unwrap();
        assert_eq!(identity(&now), identity(&old));
    }
    #[test]
    fn bounded_metadata_serialization_refuses_growth_before_write() {
        let fixture = Fixture::new();
        let mut store = fixture.open();
        accept(&mut store, b"{}");
        let old = fixture.manifest();
        let mut state = store.snapshot(ServiceId::SingBox);
        state.artifact = Some(Artifact {
            url: "private".repeat(MAX_METADATA_BYTES),
            ..Artifact::default()
        });
        assert_eq!(
            store.save_state(ServiceId::SingBox, state).unwrap_err(),
            StoreError::InvalidState
        );
        assert_eq!(fixture.manifest(), old);
        fixture.no_temps();
    }
}

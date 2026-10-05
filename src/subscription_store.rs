//! Private opaque subscription source. No YAML parsing, draft mutation, fetch,
//! command execution, directory creation or permission repair occurs here.
use crate::readiness_tun::{Budget, TunError};
use sha2::{Digest, Sha256};
use std::{
    ffi::{CStr, CString},
    fmt,
    fs::{File, Metadata},
    io::{self, Read, Seek, SeekFrom, Write},
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::{ffi::OsStrExt, fs::MetadataExt},
    },
    path::{Component, Path, PathBuf},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
pub const MAX_BYTES: usize = 2 << 20;
const HEADROOM: u128 = 1 << 20;
const SOURCE: &CStr = c"subscription.yaml";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreError {
    Storage,
    Invalid,
    Limit,
    Measurement,
    InsufficientSpace,
    Deadline,
    Cancelled,
    Durability,
}
impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Storage => "subscription source storage unavailable",
            Self::Invalid => "subscription source storage invalid",
            Self::Limit => "subscription source exceeds limit",
            Self::Measurement => "subscription source free space unavailable",
            Self::InsufficientSpace => "subscription source needs recovery headroom",
            Self::Deadline => "subscription source operation deadline exceeded",
            Self::Cancelled => "subscription source operation cancelled",
            Self::Durability => "subscription source durability uncertain",
        })
    }
}
impl std::error::Error for StoreError {}
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SaveOutcome {
    pub committed: bool,
    pub durability_error: Option<StoreError>,
    pub sha256: [u8; 32],
}
impl fmt::Debug for SaveOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SubscriptionSaveOutcome")
            .field("committed", &self.committed)
            .field("durability_error", &self.durability_error)
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct Stamp {
    id: (u64, u64),
    size: u64,
    mode: u32,
    uid: u32,
    links: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}
fn stamp(m: &Metadata) -> Stamp {
    Stamp {
        id: (m.dev(), m.ino()),
        size: m.len(),
        mode: m.mode(),
        uid: m.uid(),
        links: m.nlink(),
        modified: (m.mtime(), m.mtime_nsec()),
        changed: (m.ctime(), m.ctime_nsec()),
    }
}
fn private_file(m: &Metadata) -> Result<(), StoreError> {
    if !m.is_file()
        || m.mode() & 0o7777 != 0o600
        || m.uid() != unsafe { libc::geteuid() }
        || m.nlink() != 1
    {
        return Err(StoreError::Invalid);
    }
    if m.len() > MAX_BYTES as u64 {
        return Err(StoreError::Limit);
    }
    Ok(())
}
fn check(b: &Budget<'_>) -> Result<(), StoreError> {
    b.check().map_err(|e| match e {
        TunError::Deadline => StoreError::Deadline,
        TunError::Cancelled => StoreError::Cancelled,
        _ => StoreError::Storage,
    })
}
fn checked_io<T>(b: &Budget<'_>, action: impl FnOnce() -> io::Result<T>) -> Result<T, StoreError> {
    check(b)?;
    let result = action();
    check(b)?;
    result.map_err(|_| StoreError::Storage)
}
fn root_open(path: &Path, b: &Budget<'_>) -> Result<File, StoreError> {
    check(b)?;
    if !path.is_absolute()
        || path
            .as_os_str()
            .as_bytes()
            .split(|b| *b == b'/')
            .any(|c| c == b"." || c == b"..")
    {
        return Err(StoreError::Invalid);
    }
    let fd = unsafe {
        libc::open(
            c"/".as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        check(b)?;
        return Err(StoreError::Storage);
    }
    let mut directory = unsafe { OwnedFd::from_raw_fd(fd) };
    check(b)?;
    for part in path.components() {
        let name = match part {
            Component::RootDir => continue,
            Component::Normal(n) => n,
            _ => return Err(StoreError::Invalid),
        };
        let name = CString::new(name.as_bytes()).map_err(|_| StoreError::Invalid)?;
        check(b)?;
        let fd = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            )
        };
        if fd < 0 {
            check(b)?;
            return Err(StoreError::Storage);
        }
        directory = unsafe { OwnedFd::from_raw_fd(fd) };
        check(b)?;
    }
    let file = File::from(directory);
    let m = checked_io(b, || file.metadata())?;
    if !m.is_dir() || m.mode() & 0o7777 != 0o700 || m.uid() != unsafe { libc::geteuid() } {
        return Err(StoreError::Invalid);
    }
    Ok(file)
}
fn open_at(dir: &File, name: &CStr, flags: i32) -> io::Result<File> {
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
            0o600 as libc::c_uint,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}
fn optional(dir: &File, name: &CStr, b: &Budget<'_>) -> Result<Option<File>, StoreError> {
    check(b)?;
    let result = open_at(dir, name, libc::O_RDONLY);
    check(b)?;
    match result {
        Ok(f) => Ok(Some(f)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(StoreError::Storage),
    }
}
#[cfg(test)]
fn read_hash(
    file: &mut impl Read,
    b: &Budget<'_>,
    bytes: Option<&mut Vec<u8>>,
) -> Result<([u8; 32], u64), StoreError> {
    read_hash_limit(file, b, bytes, MAX_BYTES)
}
fn read_hash_limit(
    file: &mut impl Read,
    b: &Budget<'_>,
    mut bytes: Option<&mut Vec<u8>>,
    limit: usize,
) -> Result<([u8; 32], u64), StoreError> {
    let mut sha = Sha256::new();
    let mut scratch = [0; 8192];
    let mut total = 0usize;
    loop {
        check(b)?;
        let size = scratch.len().min(limit.saturating_sub(total) + 1);
        let result = file.read(&mut scratch[..size]);
        check(b)?;
        let n = match result {
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            result => result.map_err(|_| StoreError::Storage)?,
        };
        if n == 0 {
            break;
        }
        if n > limit.saturating_sub(total) {
            return Err(StoreError::Limit);
        }
        total += n;
        sha.update(&scratch[..n]);
        if let Some(v) = bytes.as_deref_mut() {
            // Reserve exactly the pinned size; growth is not a reason to keep
            // reallocating a credential-bearing buffer beyond admitted bytes.
            if n > v.capacity().saturating_sub(v.len()) {
                return Err(StoreError::Storage);
            }
            v.extend_from_slice(&scratch[..n]);
        }
    }
    check(b)?;
    let digest = sha.finalize().into();
    check(b)?;
    Ok((digest, total as u64))
}
fn write_hash(
    writer: &mut impl Write,
    raw: &[u8],
    b: &Budget<'_>,
    mut before_write: impl FnMut() -> Result<(), StoreError>,
) -> Result<[u8; 32], StoreError> {
    let mut sha = Sha256::new();
    for chunk in raw.chunks(8192) {
        let mut pending = chunk;
        while !pending.is_empty() {
            check(b)?;
            before_write()?;
            let result = writer.write(pending);
            check(b)?;
            let n = match result {
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                result => result.map_err(|_| StoreError::Storage)?,
            };
            if n == 0 || n > pending.len() {
                return Err(StoreError::Storage);
            }
            sha.update(&pending[..n]);
            pending = &pending[n..];
        }
    }
    check(b)?;
    let digest = sha.finalize().into();
    check(b)?;
    Ok(digest)
}
fn admit_space(unit: u128, available: u128, length: usize) -> Result<(), StoreError> {
    if unit == 0 {
        return Err(StoreError::Measurement);
    }
    let block = unit.max(4096);
    let rounded = (length as u128)
        .checked_add(block - 1)
        .and_then(|n| n.checked_div(block))
        .and_then(|n| n.checked_mul(block))
        .ok_or(StoreError::Measurement)?;
    let needed = HEADROOM
        .checked_add(rounded)
        .and_then(|n| n.checked_add(block))
        .ok_or(StoreError::Measurement)?;
    if available < needed {
        return Err(StoreError::InsufficientSpace);
    }
    Ok(())
}
pub struct Store {
    path: PathBuf,
    name: &'static CStr,
    limit: usize,
    directory: File,
    root_id: (u64, u64),
    file: Option<File>,
    saved: Option<Stamp>,
    sha256: Option<[u8; 32]>,
    #[cfg(test)]
    fault: Option<Fault>,
    #[cfg(test)]
    cancel_after_commit: Option<std::sync::Arc<AtomicBool>>,
}
impl fmt::Debug for Store {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SubscriptionStore([private])")
    }
}
impl Store {
    pub fn open(root: &Path) -> Result<Self, StoreError> {
        Self::open_fixed(root, SOURCE, MAX_BYTES)
    }
    pub(crate) fn open_selection(root: &Path) -> Result<Self, StoreError> {
        Self::open_fixed(root, c"proxy-selection.json", 4096)
    }
    fn open_fixed(root: &Path, name: &'static CStr, limit: usize) -> Result<Self, StoreError> {
        let cancel = AtomicBool::new(false);
        let budget = Budget {
            deadline: Instant::now() + Duration::from_secs(30),
            cancel: &cancel,
        };
        let directory = root_open(root, &budget)?;
        let m = checked_io(&budget, || directory.metadata())?;
        let mut value = Self {
            path: root.into(),
            name,
            limit,
            directory,
            root_id: (m.dev(), m.ino()),
            file: None,
            saved: None,
            sha256: None,
            #[cfg(test)]
            fault: None,
            #[cfg(test)]
            cancel_after_commit: None,
        };
        if let Some(mut file) = optional(&value.directory, value.name, &budget)? {
            let before = checked_io(&budget, || file.metadata())?;
            private_file(&before)?;
            if before.len() > value.limit as u64 {
                return Err(StoreError::Limit);
            }
            let (sha, size) = read_hash_limit(&mut file, &budget, None, value.limit)?;
            let after = checked_io(&budget, || file.metadata())?;
            if stamp(&before) != stamp(&after) || size != before.len() {
                return Err(StoreError::Storage);
            }
            value.saved = Some(stamp(&after));
            value.sha256 = Some(sha);
            value.file = Some(file);
        }
        drop(value.checked(&budget)?);
        Ok(value)
    }
    fn checked(&self, b: &Budget<'_>) -> Result<Option<File>, StoreError> {
        let current = root_open(&self.path, b)?;
        let current_meta = checked_io(b, || current.metadata())?;
        let retained = checked_io(b, || self.directory.metadata())?;
        if (current_meta.dev(), current_meta.ino()) != self.root_id
            || (retained.dev(), retained.ino()) != self.root_id
            || retained.mode() & 0o7777 != 0o700
            || retained.uid() != unsafe { libc::geteuid() }
        {
            return Err(StoreError::Storage);
        }
        let disk = optional(&self.directory, self.name, b)?;
        match (&self.file, &disk) {
            (None, None) => {}
            (Some(pin), Some(disk)) => {
                let pin = checked_io(b, || pin.metadata())?;
                let disk = checked_io(b, || disk.metadata())?;
                private_file(&disk)?;
                if self.saved != Some(stamp(&pin)) || stamp(&pin) != stamp(&disk) {
                    return Err(StoreError::Storage);
                }
            }
            _ => return Err(StoreError::Storage),
        }
        Ok(disk)
    }
    pub(crate) fn current_sha256(&self, b: &Budget<'_>) -> Result<Option<[u8; 32]>, StoreError> {
        drop(self.checked(b)?);
        Ok(self.sha256)
    }
    pub fn load(&self) -> Result<Option<Vec<u8>>, StoreError> {
        let cancel = AtomicBool::new(false);
        self.load_until(&Budget {
            deadline: Instant::now() + Duration::from_secs(30),
            cancel: &cancel,
        })
    }
    pub fn load_until(&self, b: &Budget<'_>) -> Result<Option<Vec<u8>>, StoreError> {
        let Some(mut disk) = self.checked(b)? else {
            return Ok(None);
        };
        let size = self.saved.ok_or(StoreError::Storage)?.size;
        let mut raw = Vec::new();
        raw.try_reserve_exact(size as usize)
            .map_err(|_| StoreError::Storage)?;
        let (sha, length) = read_hash_limit(&mut disk, b, Some(&mut raw), self.limit)?;
        drop(self.checked(b)?);
        if length != size || self.sha256 != Some(sha) {
            return Err(StoreError::Storage);
        }
        Ok(Some(raw))
    }
    pub fn save(&mut self, raw: &[u8], b: &Budget<'_>) -> Result<SaveOutcome, StoreError> {
        check(b)?;
        if raw.len() > self.limit {
            return Err(StoreError::Limit);
        }
        drop(self.checked(b)?);
        let mut space = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        check(b)?;
        let result = unsafe { libc::fstatvfs(self.directory.as_raw_fd(), space.as_mut_ptr()) };
        check(b)?;
        if result != 0 {
            return Err(StoreError::Measurement);
        }
        let space = unsafe { space.assume_init() };
        let unit = if space.f_frsize == 0 {
            space.f_bsize
        } else {
            space.f_frsize
        };
        let available = u128::from(space.f_bavail)
            .checked_mul(u128::from(unit))
            .ok_or(StoreError::Measurement)?;
        admit_space(u128::from(unit), available, raw.len())?;
        self.inject(Step::Admission)?;
        let mut random = [0; 16];
        check(b)?;
        let result = getrandom::fill(&mut random);
        check(b)?;
        result.map_err(|_| StoreError::Storage)?;
        let name = CString::new(format!(
            ".subscription-{}",
            random
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        ))
        .map_err(|_| StoreError::Storage)?;
        check(b)?;
        let opened = open_at(
            &self.directory,
            &name,
            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
        );
        // A successful creation must be retained before reporting budget expiry.
        let mut file = match opened {
            Ok(file) => file,
            Err(_) => {
                check(b)?;
                return Err(StoreError::Storage);
            }
        };
        let written = (|| {
            check(b)?;
            let metadata = checked_io(b, || file.metadata())?;
            private_file(&metadata)?;
            let expected = write_hash(&mut file, raw, b, || self.inject(Step::Write))?;
            self.inject(Step::FileSync)?;
            checked_io(b, || file.sync_all())?;
            let m = checked_io(b, || file.metadata())?;
            private_file(&m)?;
            if m.len() != raw.len() as u64 {
                return Err(StoreError::Storage);
            }
            checked_io(b, || file.seek(SeekFrom::Start(0)))?;
            let (reread, length) = read_hash_limit(&mut file, b, None, self.limit)?;
            if reread != expected || length != m.len() {
                return Err(StoreError::Storage);
            }
            drop(self.checked(b)?);
            let temp = checked_io(b, || open_at(&self.directory, &name, libc::O_RDONLY))?;
            let tm = checked_io(b, || temp.metadata())?;
            if stamp(&tm) != stamp(&m) {
                return Err(StoreError::Storage);
            }
            check(b)?;
            self.inject(Step::Rename)?;
            check(b)?;
            // Atomic commit point. Never run a post-call budget check until the
            // new authority has replaced the old pin.
            if unsafe {
                libc::renameat(
                    self.directory.as_raw_fd(),
                    name.as_ptr(),
                    self.directory.as_raw_fd(),
                    self.name.as_ptr(),
                )
            } != 0
            {
                check(b)?;
                return Err(StoreError::Storage);
            }
            Ok(expected)
        })();
        let sha = match written {
            Ok(sha) => sha,
            Err(error) => {
                cleanup_temp(&self.directory, &name, &file);
                return Err(error);
            }
        };
        self.file = Some(file);
        self.sha256 = Some(sha);
        self.saved = self
            .file
            .as_ref()
            .and_then(|file| file.metadata().ok())
            .map(|m| stamp(&m));
        #[cfg(test)]
        if let Some(flag) = self.cancel_after_commit.take() {
            flag.store(true, std::sync::atomic::Ordering::Release);
        }
        // Finish directory durability even after deadline/cancellation. Never
        // report precommit Err or roll back a successful rename.
        let mut uncertain = self.saved.is_none() || self.inject(Step::DirectorySync).is_err();
        if self.directory.sync_all().is_err() {
            uncertain = true;
        }
        if check(b).is_err() || self.checked(b).is_err() {
            uncertain = true;
        }
        Ok(SaveOutcome {
            committed: true,
            durability_error: uncertain.then_some(StoreError::Durability),
            sha256: sha,
        })
    }
    #[cfg(not(test))]
    fn inject(&mut self, _: Step) -> Result<(), StoreError> {
        Ok(())
    }
    #[cfg(test)]
    fn inject(&mut self, step: Step) -> Result<(), StoreError> {
        if self.fault.is_some_and(|fault| fault.step == step) {
            let fault = self.fault.take().expect("private fault");
            return Err(fault.error);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Admission,
    Write,
    FileSync,
    Rename,
    DirectorySync,
}
#[cfg(test)]
#[derive(Clone, Copy)]
struct Fault {
    step: Step,
    error: StoreError,
}
fn cleanup_temp(dir: &File, name: &CStr, file: &File) {
    let Ok(pin) = file.metadata() else {
        return;
    };
    let mut current = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe {
        libc::fstatat(
            dir.as_raw_fd(),
            name.as_ptr(),
            current.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } != 0
    {
        return;
    }
    let current = unsafe { current.assume_init() };
    // stat widths/sign differ on BSD and ARM Linux; compare the same native
    // representation as std's MetadataExt while the inode remains pinned.
    #[allow(clippy::unnecessary_cast)]
    let observed = (current.st_dev as u64, current.st_ino as u64);
    if observed != (pin.dev(), pin.ino()) {
        return;
    }
    if unsafe { libc::unlinkat(dir.as_raw_fd(), name.as_ptr(), 0) } == 0 {
        let _ = dir.sync_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        os::unix::fs::{DirBuilderExt, PermissionsExt},
        sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        },
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
                "b6p-source-fault-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(&path)
                .unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn budget(cancel: &AtomicBool) -> Budget<'_> {
        Budget {
            deadline: Instant::now() + Duration::from_secs(30),
            cancel,
        }
    }

    #[test]
    fn fixed_selection_evidence_keeps_private_cap_source_and_pin_authority() {
        let fixture = Fixture::new();
        let mut source = Store::open(&fixture.0).unwrap();
        let cancel = AtomicBool::new(false);
        source.save(b"opaque-source", &budget(&cancel)).unwrap();
        let mut selection = Store::open_selection(&fixture.0).unwrap();
        let accepted = vec![b'x'; 4096];
        let outcome = selection.save(&accepted, &budget(&cancel)).unwrap();
        assert_eq!(outcome.sha256, <[u8; 32]>::from(Sha256::digest(&accepted)));
        assert_eq!(
            selection.current_sha256(&budget(&cancel)).unwrap(),
            Some(outcome.sha256)
        );
        assert_eq!(
            selection
                .save(&vec![b'x'; 4097], &budget(&cancel))
                .unwrap_err(),
            StoreError::Limit
        );
        assert_eq!(selection.load().unwrap().unwrap(), accepted);
        assert_eq!(source.load().unwrap().unwrap(), b"opaque-source");
        selection.fault = Some(Fault {
            step: Step::DirectorySync,
            error: StoreError::Storage,
        });
        let changed = selection.save(b"new-evidence", &budget(&cancel)).unwrap();
        assert!(changed.committed);
        assert_eq!(changed.durability_error, Some(StoreError::Durability));
        assert_eq!(selection.load().unwrap().unwrap(), b"new-evidence");
        // Committed-new authority remains usable; a noncritical fsync report
        // does not introduce a permanent operation gate or affect the core.
        assert!(selection.current_sha256(&budget(&cancel)).is_ok());
    }
    #[test]
    fn each_precommit_fault_keeps_old_source_and_cleans_only_owned_temp() {
        for (step, error) in [
            (Step::Admission, StoreError::InsufficientSpace),
            (Step::Write, StoreError::Storage),
            (Step::FileSync, StoreError::Storage),
            (Step::Rename, StoreError::Storage),
            (Step::Write, StoreError::Deadline),
            (Step::Rename, StoreError::Cancelled),
        ] {
            let f = Fixture::new();
            let mut store = Store::open(&f.0).unwrap();
            let cancel = AtomicBool::new(false);
            store.save(b"synthetic old", &budget(&cancel)).unwrap();
            store.fault = Some(Fault { step, error });
            assert_eq!(store.save(b"synthetic new", &budget(&cancel)), Err(error));
            assert_eq!(store.load().unwrap().unwrap(), b"synthetic old");
            assert_eq!(std::fs::read_dir(&f.0).unwrap().count(), 1);
            assert!(
                store
                    .save(b"retry new", &budget(&cancel))
                    .unwrap()
                    .committed
            );
        }
    }
    #[test]
    fn postcommit_dir_failure_keeps_new_authority_and_is_retryable() {
        let f = Fixture::new();
        let mut store = Store::open(&f.0).unwrap();
        let cancel = AtomicBool::new(false);
        store.save(b"old", &budget(&cancel)).unwrap();
        store.fault = Some(Fault {
            step: Step::DirectorySync,
            error: StoreError::Durability,
        });
        let outcome = store.save(b"new", &budget(&cancel)).unwrap();
        assert!(outcome.committed);
        assert_eq!(outcome.durability_error, Some(StoreError::Durability));
        assert_eq!(store.load().unwrap().unwrap(), b"new");
        assert_eq!(Store::open(&f.0).unwrap().load().unwrap().unwrap(), b"new");
        assert!(
            store
                .save(b"retry", &budget(&cancel))
                .unwrap()
                .durability_error
                .is_none()
        );
    }
    #[test]
    fn cancellation_after_rename_cannot_report_uncommitted_old_authority() {
        let f = Fixture::new();
        let mut store = Store::open(&f.0).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        store.save(b"old", &budget(&cancel)).unwrap();
        store.cancel_after_commit = Some(cancel.clone());
        let outcome = store.save(b"committed", &budget(&cancel)).unwrap();
        assert!(outcome.committed && outcome.durability_error == Some(StoreError::Durability));
        cancel.store(false, Ordering::Release);
        assert_eq!(store.load().unwrap().unwrap(), b"committed");
        assert!(
            store
                .save(b"next", &budget(&cancel))
                .unwrap()
                .durability_error
                .is_none()
        );
    }
    #[test]
    fn space_rounds_full_growth_and_dir_entry_without_spending_headroom() {
        assert_eq!(
            admit_space(4096, HEADROOM + 8191, 1),
            Err(StoreError::InsufficientSpace)
        );
        assert_eq!(admit_space(4096, HEADROOM + 8192, 1), Ok(()));
        assert_eq!(
            admit_space(4096, HEADROOM + MAX_BYTES as u128 + 4096, MAX_BYTES),
            Ok(())
        );
        assert_eq!(
            admit_space(4096, HEADROOM + MAX_BYTES as u128 + 4095, MAX_BYTES),
            Err(StoreError::InsufficientSpace)
        );
        assert_eq!(
            admit_space(0, HEADROOM + 8192, 1),
            Err(StoreError::Measurement)
        );
        assert_eq!(
            admit_space(u128::MAX, u128::MAX, 1),
            Err(StoreError::Measurement)
        );
    }
    #[test]
    fn exact_temp_cleanup_does_not_remove_a_foreign_replacement() {
        let f = Fixture::new();
        let cancel = AtomicBool::new(false);
        let dir = root_open(&f.0, &budget(&cancel)).unwrap();
        let name = c".subscription-test";
        let file = open_at(&dir, name, libc::O_RDWR | libc::O_CREAT | libc::O_EXCL).unwrap();
        std::fs::rename(f.0.join(".subscription-test"), f.0.join("retained")).unwrap();
        std::fs::write(f.0.join(".subscription-test"), b"foreign").unwrap();
        std::fs::set_permissions(
            f.0.join(".subscription-test"),
            std::fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        cleanup_temp(&dir, name, &file);
        assert_eq!(
            std::fs::read(f.0.join(".subscription-test")).unwrap(),
            b"foreign"
        );
        let own = c".subscription-owned";
        let file = open_at(&dir, own, libc::O_RDWR | libc::O_CREAT | libc::O_EXCL).unwrap();
        cleanup_temp(&dir, own, &file);
        assert!(!f.0.join(".subscription-owned").exists());
    }
    struct ShortWriter {
        raw: Vec<u8>,
        calls: usize,
        stop: Option<usize>,
        cancel: Option<Arc<AtomicBool>>,
    }
    impl Write for ShortWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            assert!(bytes.len() <= 8192);
            self.calls += 1;
            if let Some(flag) = &self.cancel {
                flag.store(true, Ordering::Release);
            }
            if self.calls == 1 {
                return Err(io::Error::from(io::ErrorKind::Interrupted));
            }
            if self.stop == Some(self.calls) {
                return Ok(0);
            }
            let n = bytes.len().min(31);
            self.raw.extend_from_slice(&bytes[..n]);
            Ok(n)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn chunk_writer_handles_short_interrupted_zero_and_cancelled_io() {
        let cancel = AtomicBool::new(false);
        let raw = vec![7; 17000];
        let mut writer = ShortWriter {
            raw: Vec::new(),
            calls: 0,
            stop: None,
            cancel: None,
        };
        assert_eq!(
            write_hash(&mut writer, &raw, &budget(&cancel), || Ok(())).unwrap(),
            <[u8; 32]>::from(Sha256::digest(&raw))
        );
        assert_eq!(writer.raw, raw);
        let mut writer = ShortWriter {
            raw: Vec::new(),
            calls: 0,
            stop: Some(2),
            cancel: None,
        };
        assert_eq!(
            write_hash(&mut writer, &raw, &budget(&cancel), || Ok(())),
            Err(StoreError::Storage)
        );
        let flag = Arc::new(AtomicBool::new(false));
        let mut writer = ShortWriter {
            raw: Vec::new(),
            calls: 0,
            stop: None,
            cancel: Some(flag.clone()),
        };
        assert_eq!(
            write_hash(&mut writer, &raw, &budget(&flag), || Ok(())),
            Err(StoreError::Cancelled)
        );
        assert_eq!(writer.calls, 1);
    }
    #[test]
    fn load_hash_checks_cancel_after_read_even_on_interrupted_io() {
        struct Cancels(Arc<AtomicBool>);
        impl Read for Cancels {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                self.0.store(true, Ordering::Release);
                Err(io::Error::from(io::ErrorKind::Interrupted))
            }
        }
        let flag = Arc::new(AtomicBool::new(false));
        let mut read = Cancels(flag.clone());
        assert_eq!(
            read_hash(&mut read, &budget(&flag), None),
            Err(StoreError::Cancelled)
        );
    }
}

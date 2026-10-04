//! Verified bounded artifact staging, not transport, activation or executable trust.
//! Caller-supplied Read must cooperate with the absolute Budget; no thread hides
//! blocking. Encoded SHA covers all bytes before gzip decoding. One owned file.
use crate::readiness_tun::{Budget, TunError};
use crate::runtime_store::{Artifact, FREE_HEADROOM_BYTES};
use flate2::bufread::MultiGzDecoder;
use sha2::{Digest, Sha256};
use std::{
    ffi::CString,
    fmt,
    fs::{self, File, OpenOptions, Permissions},
    io::{self, BufReader, Read, Seek, SeekFrom, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Component, Path, PathBuf},
};

pub const MAX_ENCODED_BYTES: u64 = 16 << 20;
pub const MAX_DECODED_BYTES: u64 = 40 << 20;
pub const STREAM_BYTES: usize = 8192;
const MAX_URL_BYTES: usize = 4096;
const MAX_VERSION_BYTES: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageError {
    Metadata,
    Directory,
    Source,
    EncodedLimit,
    DecodedLimit,
    Empty,
    Gzip,
    Digest,
    Storage,
    Measurement,
    InsufficientSpace,
    Identity,
    Deadline,
    Cancelled,
}
impl fmt::Display for StageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Metadata => "artifact metadata invalid",
            Self::Directory => "artifact staging directory invalid",
            Self::Source => "artifact source unavailable",
            Self::EncodedLimit => "artifact encoded limit exceeded",
            Self::DecodedLimit => "artifact extracted limit exceeded",
            Self::Empty => "artifact extracted file is empty",
            Self::Gzip => "artifact gzip stream invalid",
            Self::Digest => "artifact source digest mismatch",
            Self::Storage => "artifact staging storage unavailable",
            Self::Measurement => "artifact staging resources unavailable",
            Self::InsufficientSpace => "artifact staging needs recovery headroom",
            Self::Identity => "artifact staging identity changed",
            Self::Deadline => "artifact staging deadline exceeded",
            Self::Cancelled => "artifact staging cancelled",
        })
    }
}
impl std::error::Error for StageError {}
fn check(b: &Budget<'_>) -> Result<(), StageError> {
    b.check().map_err(|e| match e {
        TunError::Deadline => StageError::Deadline,
        TunError::Cancelled => StageError::Cancelled,
        _ => StageError::Source,
    })
}
fn storage(e: io::Error) -> StageError {
    if e.raw_os_error() == Some(libc::ENOSPC) {
        StageError::InsufficientSpace
    } else {
        StageError::Storage
    }
}
fn read_error(e: io::Error, fallback: StageError) -> StageError {
    e.get_ref()
        .and_then(|e| e.downcast_ref::<StageError>())
        .copied()
        .unwrap_or(fallback)
}
fn sha(text: &str) -> Result<[u8; 32], StageError> {
    if text.len() != 64 {
        return Err(StageError::Metadata);
    }
    let mut out = [0; 32];
    for (pair, byte) in text.as_bytes().chunks_exact(2).zip(&mut out) {
        fn hex(b: u8) -> Result<u8, StageError> {
            match b {
                b'0'..=b'9' => Ok(b - b'0'),
                b'a'..=b'f' => Ok(b - b'a' + 10),
                b'A'..=b'F' => Ok(b - b'A' + 10),
                _ => Err(StageError::Metadata),
            }
        }
        *byte = hex(pair[0])? << 4 | hex(pair[1])?;
    }
    Ok(out)
}
fn metadata(artifact: &Artifact) -> Result<[u8; 32], StageError> {
    // Transport scheme/redirect authority belongs to the source provider.
    if artifact.url.is_empty()
        || artifact.url.len() > MAX_URL_BYTES
        || artifact.url.chars().any(char::is_control)
        || artifact.version.len() > MAX_VERSION_BYTES
        || artifact.version.chars().any(char::is_control)
        || !matches!(artifact.compression.as_str(), "none" | "gzip")
    {
        return Err(StageError::Metadata);
    }
    sha(&artifact.sha256)
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Identity {
    dev: u64,
    ino: u64,
    length: u64,
    mode: u32,
    uid: u32,
    links: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}
fn identity(m: &fs::Metadata) -> Identity {
    Identity {
        dev: m.dev(),
        ino: m.ino(),
        length: m.len(),
        mode: m.mode(),
        uid: m.uid(),
        links: m.nlink(),
        modified: (m.mtime(), m.mtime_nsec()),
        changed: (m.ctime(), m.ctime_nsec()),
    }
}
struct Owned {
    root: PathBuf,
    path: PathBuf,
    name: CString,
    directory: File,
    directory_id: (u64, u64),
    file: File,
    file_id: Identity,
    extracted_sha256: [u8; 32],
    length: u64,
    artifact: Artifact,
}
/// Checked borrowed admission. Metadata alone does not assert binary validity,
/// ABI, native checking or process ownership. File/directory remain pinned.
pub struct Admission<'a> {
    pub root: &'a Path,
    pub path: &'a Path,
    pub file: &'a File,
    pub directory: &'a File,
    pub extracted_sha256: [u8; 32],
    pub length: u64,
    pub artifact: &'a Artifact,
}
impl fmt::Debug for Admission<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ArtifactAdmission([private])")
    }
}
impl Owned {
    #[allow(clippy::unnecessary_cast)] // stat dev/ino differ across ARM and BSD.
    fn checked(&self) -> Result<Admission<'_>, StageError> {
        let root = fs::symlink_metadata(&self.root).map_err(|_| StageError::Identity)?;
        let directory = self
            .directory
            .metadata()
            .map_err(|_| StageError::Identity)?;
        if !root.is_dir()
            || root.mode() & 0o7777 != 0o700
            || root.uid() != unsafe { libc::geteuid() }
            || (root.dev(), root.ino()) != self.directory_id
            || (directory.dev(), directory.ino()) != self.directory_id
            || directory.mode() & 0o7777 != 0o700
            || directory.uid() != root.uid()
        {
            return Err(StageError::Identity);
        }
        let named = named_metadata(&self.directory, &self.name)?;
        let file = self.file.metadata().map_err(|_| StageError::Identity)?;
        if !file.is_file()
            || identity(&file) != self.file_id
            || named.st_dev as u64 != self.file_id.dev
            || named.st_ino as u64 != self.file_id.ino
            || named.st_mode & libc::S_IFMT != libc::S_IFREG
            || named.st_nlink != 1
            || named.st_mode & 0o7777 != 0o700
        {
            return Err(StageError::Identity);
        }
        Ok(Admission {
            root: &self.root,
            path: &self.path,
            file: &self.file,
            directory: &self.directory,
            extracted_sha256: self.extracted_sha256,
            length: self.length,
            artifact: &self.artifact,
        })
    }
    fn remove(&self) -> Result<(), StageError> {
        self.remove_with(|directory| directory.sync_all().map_err(storage))
    }
    #[allow(clippy::unnecessary_cast)] // stat dev/ino differ across ARM and BSD.
    fn remove_with(
        &self,
        mut sync: impl FnMut(&File) -> Result<(), StageError>,
    ) -> Result<(), StageError> {
        // Cleanup needs only ownership identity, not unchanged contents. Never
        // follow symlinks or remove a different inode occupying the owned name.
        let named = match named_metadata_io(&self.directory, &self.name) {
            Ok(named) => named,
            Err(error)
                if error.kind() == io::ErrorKind::NotFound
                    && self.file.metadata().is_ok_and(|m| m.nlink() == 0) =>
            {
                // A previous unlink may have succeeded before directory fsync
                // failed. Retry durability without deleting any new name.
                return sync(&self.directory);
            }
            Err(_) => return Err(StageError::Identity),
        };
        if named.st_dev as u64 != self.file_id.dev
            || named.st_ino as u64 != self.file_id.ino
            || named.st_mode & libc::S_IFMT != libc::S_IFREG
        {
            return Err(StageError::Identity);
        }
        // SAFETY: pinned directory fd and exact random owned terminated name.
        if unsafe { libc::unlinkat(self.directory.as_raw_fd(), self.name.as_ptr(), 0) } != 0 {
            return Err(StageError::Storage);
        }
        sync(&self.directory)
    }
}
fn named_metadata(directory: &File, name: &CString) -> Result<libc::stat, StageError> {
    named_metadata_io(directory, name).map_err(|_| StageError::Identity)
}
fn named_metadata_io(directory: &File, name: &CString) -> io::Result<libc::stat> {
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: live directory, terminated child name, initialized only on success.
    if unsafe {
        libc::fstatat(
            directory.as_raw_fd(),
            name.as_ptr(),
            stat.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { stat.assume_init() })
}

/// Drop releases only the exact still-owned temporary inode. Explicit transfer
/// is required before a manager can preserve it for native checking/use.
pub struct Stage {
    owned: Option<Owned>,
}
impl fmt::Debug for Stage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ArtifactStage([private])")
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        if let Some(owned) = &self.owned {
            let _ = owned.remove();
        }
    }
}
/// Deliberately does not remove the artifact on Drop. The manager must call
/// cleanup only after real owned-process withdrawal, including failed checks.
/// This handle is not Clone; dropping it preserves the file, never adopts it.
pub struct RetainedStage {
    owned: Option<Owned>,
}
impl fmt::Debug for RetainedStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RetainedArtifactStage([private])")
    }
}
impl RetainedStage {
    pub fn checked(&self, budget: &Budget<'_>) -> Result<Admission<'_>, StageError> {
        check(budget)?;
        let admission = self.admitted();
        check(budget)?;
        admission
    }
    pub fn admitted(&self) -> Result<Admission<'_>, StageError> {
        self.owned.as_ref().ok_or(StageError::Identity)?.checked()
    }
    /// On failure the handle stays retained so the manager can retry; this never
    /// deletes a replacement and never runs a process withdrawal itself.
    pub fn cleanup(&mut self) -> Result<(), StageError> {
        if let Some(owned) = &self.owned {
            owned.remove()?;
        }
        self.owned = None;
        Ok(())
    }
}
impl Stage {
    pub fn checked(&self, budget: &Budget<'_>) -> Result<Admission<'_>, StageError> {
        check(budget)?;
        let admission = self.admitted();
        check(budget)?;
        admission
    }
    pub fn admitted(&self) -> Result<Admission<'_>, StageError> {
        self.owned.as_ref().ok_or(StageError::Identity)?.checked()
    }
    pub fn into_retained(mut self) -> Result<RetainedStage, StageError> {
        self.admitted()?;
        Ok(RetainedStage {
            owned: self.owned.take(),
        })
    }
    pub fn from_reader(
        root: &Path,
        artifact: &Artifact,
        reader: impl Read,
        budget: &Budget<'_>,
    ) -> Result<Self, StageError> {
        check(budget)?;
        let expected = metadata(artifact)?;
        let (root, directory) = open_root(root)?;
        let directory_metadata = directory.metadata().map_err(|_| StageError::Directory)?;
        let max_growth = if artifact.compression == "none" {
            MAX_ENCODED_BYTES
        } else {
            MAX_DECODED_BYTES
        };
        admit_resources(&directory, max_growth, budget)?;
        check(budget)?;
        let (file, name) = temporary(&directory)?;
        let file_id = identity(&file.metadata().map_err(storage)?);
        let path = root.join(name.to_str().map_err(|_| StageError::Storage)?);
        let mut stage = Self {
            owned: Some(Owned {
                root,
                path,
                name,
                directory,
                directory_id: (directory_metadata.dev(), directory_metadata.ino()),
                file,
                file_id,
                extracted_sha256: [0; 32],
                length: 0,
                artifact: artifact.clone(),
            }),
        };
        let owned = stage.owned.as_mut().ok_or(StageError::Identity)?;
        let mut source = Encoded {
            reader,
            budget,
            digest: Sha256::new(),
            count: 0,
            eof: false,
        };
        let extracted = if artifact.compression == "gzip" {
            let mut decoder =
                MultiGzDecoder::new(BufReader::with_capacity(STREAM_BYTES, &mut source));
            let result = extract(
                &mut decoder,
                &mut owned.file,
                budget,
                MAX_DECODED_BYTES,
                StageError::Gzip,
            );
            drop(decoder);
            result?
        } else {
            extract(
                &mut source,
                &mut owned.file,
                budget,
                MAX_DECODED_BYTES,
                StageError::Source,
            )?
        };
        check(budget)?;
        if extracted.1 == 0 {
            return Err(StageError::Empty);
        }
        if !source.eof || <[u8; 32]>::from(source.digest.finalize()) != expected {
            return Err(StageError::Digest);
        }
        owned.extracted_sha256 = extracted.0;
        owned.length = extracted.1;
        owned
            .file
            .set_permissions(Permissions::from_mode(0o700))
            .map_err(storage)?;
        check(budget)?;
        owned.file.sync_all().map_err(storage)?;
        check(budget)?;
        owned.directory.sync_all().map_err(storage)?;
        check(budget)?;
        verify_written(owned, budget)?;
        owned.checked()?;
        check(budget)?;
        Ok(stage)
    }
}

fn verify_written(owned: &mut Owned, budget: &Budget<'_>) -> Result<(), StageError> {
    check(budget)?;
    let before = owned.file.metadata().map_err(storage)?;
    if !before.is_file()
        || before.nlink() != 1
        || before.len() != owned.length
        || before.mode() & 0o7777 != 0o700
        || before.uid() != unsafe { libc::geteuid() }
        || (before.dev(), before.ino()) != (owned.file_id.dev, owned.file_id.ino)
    {
        return Err(StageError::Identity);
    }
    owned.file.seek(SeekFrom::Start(0)).map_err(storage)?;
    let mut bytes = [0u8; STREAM_BYTES];
    let mut hash = Sha256::new();
    let mut length = 0u64;
    loop {
        check(budget)?;
        let result = owned.file.read(&mut bytes);
        check(budget)?;
        let n = match result {
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            result => result.map_err(storage)?,
        };
        if n == 0 {
            break;
        }
        length = length.checked_add(n as u64).ok_or(StageError::Identity)?;
        if length > owned.length {
            return Err(StageError::Identity);
        }
        hash.update(&bytes[..n]);
    }
    let after = owned.file.metadata().map_err(storage)?;
    if identity(&before) != identity(&after)
        || length != owned.length
        || <[u8; 32]>::from(hash.finalize()) != owned.extracted_sha256
    {
        return Err(StageError::Identity);
    }
    // Admission exposes a read-only pinned fd, never the staging writer.
    // SAFETY: exact owned child of the live pinned directory, no-follow open.
    let fd = unsafe {
        libc::openat(
            owned.directory.as_raw_fd(),
            owned.name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
            0,
        )
    };
    if fd < 0 {
        return Err(StageError::Identity);
    }
    let file = unsafe { File::from_raw_fd(fd) };
    if identity(&file.metadata().map_err(storage)?) != identity(&after) {
        return Err(StageError::Identity);
    }
    owned.file = file;
    owned.file_id = identity(&after);
    check(budget)
}

struct Encoded<'a, 'b, R> {
    reader: R,
    budget: &'a Budget<'b>,
    digest: Sha256,
    count: u64,
    eof: bool,
}
impl<R: Read> Read for Encoded<'_, '_, R> {
    fn read(&mut self, into: &mut [u8]) -> io::Result<usize> {
        check(self.budget).map_err(io::Error::other)?;
        if into.is_empty() {
            return Ok(0);
        }
        let remaining = MAX_ENCODED_BYTES - self.count;
        let size = into.len().min(STREAM_BYTES).min(remaining.max(1) as usize);
        let n = loop {
            check(self.budget).map_err(io::Error::other)?;
            let result = self.reader.read(&mut into[..size]);
            check(self.budget).map_err(io::Error::other)?;
            match result {
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(io::Error::other(StageError::Source)),
                Ok(n) => break n,
            }
        };
        if n > size {
            return Err(io::Error::other(StageError::Source));
        }
        if n as u64 > remaining {
            return Err(io::Error::other(StageError::EncodedLimit));
        }
        self.count += n as u64;
        self.digest.update(&into[..n]);
        self.eof = n == 0;
        Ok(n)
    }
}
fn extract(
    reader: &mut impl Read,
    file: &mut File,
    budget: &Budget<'_>,
    limit: u64,
    fallback: StageError,
) -> Result<([u8; 32], u64), StageError> {
    let mut bytes = [0; STREAM_BYTES];
    let mut count = 0u64;
    let mut hash = Sha256::new();
    loop {
        check(budget)?;
        let size = bytes.len().min((limit - count).max(1) as usize);
        let result = reader.read(&mut bytes[..size]);
        check(budget)?;
        let n = result.map_err(|e| read_error(e, fallback))?;
        if n == 0 {
            break;
        }
        if n as u64 > limit - count {
            return Err(StageError::DecodedLimit);
        }
        let mut written = 0;
        while written < n {
            check(budget)?;
            let result = file.write(&bytes[written..n]);
            check(budget)?;
            match result {
                Ok(0) => return Err(StageError::Storage),
                Ok(count) => written += count,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(storage(e)),
            }
        }
        hash.update(&bytes[..n]);
        count += n as u64;
    }
    Ok((hash.finalize().into(), count))
}
fn open_root(root: &Path) -> Result<(PathBuf, File), StageError> {
    if !root.is_absolute() || root.components().any(|p| p == Component::ParentDir) {
        return Err(StageError::Directory);
    }
    let root: PathBuf = root
        .components()
        .filter(|p| *p != Component::CurDir)
        .collect();
    let directory = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(&root)
        .map_err(|_| StageError::Directory)?;
    let m = directory.metadata().map_err(|_| StageError::Directory)?;
    let path = fs::symlink_metadata(&root).map_err(|_| StageError::Directory)?;
    if !m.is_dir()
        || m.mode() & 0o7777 != 0o700
        || m.uid() != unsafe { libc::geteuid() }
        || !path.is_dir()
        || (path.dev(), path.ino()) != (m.dev(), m.ino())
    {
        return Err(StageError::Directory);
    }
    Ok((root, directory))
}
fn temporary(directory: &File) -> Result<(File, CString), StageError> {
    for _ in 0..16 {
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).map_err(|_| StageError::Storage)?;
        let mut name = String::with_capacity(42);
        name.push_str(".artifact-");
        for b in random {
            use std::fmt::Write as _;
            write!(name, "{b:02x}").map_err(|_| StageError::Storage)?;
        }
        let name = CString::new(name).map_err(|_| StageError::Storage)?;
        // SAFETY: pinned private directory and exclusive no-follow fixed child.
        let fd = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDWR
                    | libc::O_CREAT
                    | libc::O_EXCL
                    | libc::O_NOFOLLOW
                    | libc::O_CLOEXEC
                    | libc::O_NONBLOCK,
                0o700,
            )
        };
        if fd >= 0 {
            return Ok((unsafe { File::from_raw_fd(fd) }, name));
        }
        if io::Error::last_os_error().kind() != io::ErrorKind::AlreadyExists {
            return Err(StageError::Storage);
        }
    }
    Err(StageError::Storage)
}
fn admit_space(available: u64, unit: u64, growth: u64) -> Result<(), StageError> {
    if unit == 0 {
        return Err(StageError::Measurement);
    }
    let unit = unit.max(4096);
    let charge = growth
        .checked_add(unit - 1)
        .and_then(|n| (n / unit).checked_mul(unit))
        .and_then(|n| n.checked_add(unit))
        .and_then(|n| n.checked_add(FREE_HEADROOM_BYTES))
        .ok_or(StageError::Measurement)?;
    if available < charge {
        Err(StageError::InsufficientSpace)
    } else {
        Ok(())
    }
}
fn admit_resources(directory: &File, growth: u64, budget: &Budget<'_>) -> Result<(), StageError> {
    check(budget)?;
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: live fd and stat initialized only on success.
    if unsafe { libc::fstatvfs(directory.as_raw_fd(), stat.as_mut_ptr()) } != 0 {
        return Err(StageError::Measurement);
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
        .ok_or(StageError::Measurement)?;
    admit_space(available, unit, growth)?;
    let memory = available_memory(budget)?;
    // Conservative: full potential tmpfs growth plus working buffers and reserve,
    // even when a host's staging filesystem is disk backed. No reserve is spent.
    admit_space(memory, 4096, growth + (512 << 10))?;
    check(budget)
}
#[cfg(target_os = "linux")]
fn available_memory(budget: &Budget<'_>) -> Result<u64, StageError> {
    check(budget)?;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open("/proc/meminfo")
        .map_err(|_| StageError::Measurement)?;
    let mut bytes = [0u8; STREAM_BYTES];
    let mut length = 0;
    loop {
        check(budget)?;
        let result = file.read(&mut bytes[length..]);
        check(budget)?;
        let n = result.map_err(|_| StageError::Measurement)?;
        if n == 0 {
            break;
        }
        length += n;
        if length == bytes.len() {
            return Err(StageError::Measurement);
        }
    }
    let text = std::str::from_utf8(&bytes[..length]).map_err(|_| StageError::Measurement)?;
    let mut found = None;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("MemAvailable:") {
            let mut words = value.split_ascii_whitespace();
            let kib = words
                .next()
                .ok_or(StageError::Measurement)?
                .parse::<u64>()
                .map_err(|_| StageError::Measurement)?;
            if words.next() != Some("kB") || words.next().is_some() || found.is_some() {
                return Err(StageError::Measurement);
            }
            found = Some(kib.checked_mul(1024).ok_or(StageError::Measurement)?);
        }
    }
    found.ok_or(StageError::Measurement)
}
#[cfg(target_os = "macos")]
fn available_memory(budget: &Budget<'_>) -> Result<u64, StageError> {
    check(budget)?;
    let mut pages = 0u32;
    let mut length = std::mem::size_of_val(&pages);
    // SAFETY: fixed terminated MIB name and correctly sized writable scalar.
    if unsafe {
        libc::sysctlbyname(
            c"vm.page_free_count".as_ptr(),
            &mut pages as *mut u32 as *mut libc::c_void,
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    } != 0
        || length != std::mem::size_of_val(&pages)
    {
        return Err(StageError::Measurement);
    }
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    check(budget)?;
    if page_size <= 0 {
        return Err(StageError::Measurement);
    }
    u64::from(pages)
        .checked_mul(page_size as u64)
        .ok_or(StageError::Measurement)
}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn available_memory(_: &Budget<'_>) -> Result<u64, StageError> {
    Err(StageError::Measurement)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_unlink_sync_failure_is_retryable_without_deleting_replacement() {
        use std::sync::atomic::AtomicBool;
        use std::time::{Duration, Instant};
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "b6p-stage-fault-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, Permissions::from_mode(0o700)).unwrap();
        let cancel = AtomicBool::new(false);
        let budget = Budget {
            deadline: Instant::now() + Duration::from_secs(10),
            cancel: &cancel,
        };
        let artifact = Artifact {
            url: "https://synthetic.invalid/artifact".into(),
            sha256: format!("{:x}", Sha256::digest(b"binary")),
            compression: "none".into(),
            version: String::new(),
        };
        for replacement in [false, true] {
            let stage = Stage::from_reader(&root, &artifact, &b"binary"[..], &budget).unwrap();
            let mut retained = stage.into_retained().unwrap();
            let owned = retained.owned.as_ref().unwrap();
            let path = owned.path.clone();
            assert_eq!(
                owned.remove_with(|_| Err(StageError::Storage)),
                Err(StageError::Storage)
            );
            assert!(!path.exists());
            if replacement {
                fs::write(&path, b"keep replacement").unwrap();
                assert_eq!(retained.cleanup(), Err(StageError::Identity));
                assert_eq!(fs::read(&path).unwrap(), b"keep replacement");
                fs::remove_file(path).unwrap();
            }
            retained.cleanup().unwrap();
            retained.cleanup().unwrap();
        }
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn full_rounded_growth_and_headroom_are_admitted_without_consuming_reserve() {
        let charge = MAX_DECODED_BYTES + 4096 + FREE_HEADROOM_BYTES;
        assert_eq!(
            admit_space(charge - 1, 4096, MAX_DECODED_BYTES),
            Err(StageError::InsufficientSpace)
        );
        assert_eq!(admit_space(charge, 4096, MAX_DECODED_BYTES), Ok(()));
        assert_eq!(admit_space(u64::MAX, 0, 1), Err(StageError::Measurement));
        assert_eq!(
            admit_space(u64::MAX, 4096, u64::MAX),
            Err(StageError::Measurement)
        );
    }
}

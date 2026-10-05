//! Private, bounded checkpoints for the caller-owned feature operation lane.
//!
//! Returning from prepare is the durable mutation boundary. Neither Drop nor a
//! failed restore removes an armed journal. The root owner reloads fixed native
//! services after restore and only then discards the checkpoint.
use crate::{features::Error, product_io::Backend, readiness_tun::Budget};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    ffi::CString,
    fs::{self, File, OpenOptions, Permissions},
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Component, Path, PathBuf},
};

pub const MAX_UCI_BYTES: usize = 512 << 10;
pub const MAX_DEVICE_DB_BYTES: usize = 4 << 20;
pub const FREE_HEADROOM_BYTES: u64 = 1 << 20;
const MAX_CONFIGS: usize = 64;
const MAX_JOURNAL_BYTES: usize = 32 << 10;
const MAX_PENDING: usize = 32;
const JOURNAL: &str = "journal.json";

fn error(code: &'static str) -> Error {
    let (status, message) = match code {
        "feature_storage_insufficient" => (507, "恢复存储空间不足。"),
        "feature_recovery_corrupt" => (500, "恢复记录无效，现有文件已保留。"),
        "feature_recovery_unsafe" => (500, "恢复文件的类型、权限或所有者不安全。"),
        "feature_checkpoint_too_large" => (413, "本次操作的恢复文件超出大小限制。"),
        "feature_sqlite_busy" => (409, "设备数据库正在使用不支持的事务模式，请稍后重试。"),
        "feature_recovery_pending" => (409, "已有待恢复操作，请先完成恢复。"),
        "operation_timeout" => (504, "操作超时。"),
        "operation_cancelled" => (409, "操作已取消。"),
        _ => (507, "无法保存或恢复本次操作的数据。"),
    };
    Error {
        status,
        code,
        message,
    }
}
fn check(b: &Budget<'_>) -> Result<(), Error> {
    b.check().map_err(|e| {
        error(if e == crate::readiness_tun::TunError::Cancelled {
            "operation_cancelled"
        } else {
            "operation_timeout"
        })
    })
}
fn io_error(e: crate::product_io::Error) -> Error {
    error(match e {
        crate::product_io::Error::Cancelled => "operation_cancelled",
        crate::product_io::Error::Deadline => "operation_timeout",
        crate::product_io::Error::Limit => "feature_checkpoint_too_large",
        _ => "feature_storage_unavailable",
    })
}
fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.bytes().any(|c| c.is_ascii_digit())
        && id.bytes().all(|c| c.is_ascii_digit() || c == b'-')
}
fn config_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
        && [
            &crate::features_network::DOMAIN,
            &crate::features_wireless::DOMAIN,
            &crate::features_services::DOMAIN,
        ]
        .iter()
        .any(|domain| {
            domain
                .actions
                .iter()
                .any(|action| action.configs.contains(&name))
        })
}
fn owner(uid: u32) -> bool {
    uid == unsafe { libc::geteuid() }
}
fn cname(name: &str) -> Result<CString, Error> {
    if name.is_empty() || name.contains('/') {
        return Err(error("feature_recovery_unsafe"));
    }
    CString::new(name).map_err(|_| error("feature_recovery_unsafe"))
}
fn rounded(bytes: usize) -> u64 {
    (bytes as u64).div_ceil(4096) * 4096 + 4096
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

// File handles and private snapshot records intentionally do not implement Debug.
struct Directory {
    path: PathBuf,
    file: File,
    private: bool,
    identity: (u64, u64),
}
impl Directory {
    /// Walk each parent without following symlinks. Only missing parents are
    /// created/chmodded; unrelated existing parents keep their original modes.
    fn open(path: &Path, create: bool, private: bool) -> Result<Self, Error> {
        if !path.is_absolute() {
            return Err(error("feature_recovery_unsafe"));
        }
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open("/")
            .map_err(|_| error("feature_recovery_unsafe"))?;
        let components = path.components().collect::<Vec<_>>();
        if components.len() < 2 {
            return Err(error("feature_recovery_unsafe"));
        }
        for (index, component) in components.iter().enumerate().skip(1) {
            let Component::Normal(name) = component else {
                return Err(error("feature_recovery_unsafe"));
            };
            let name =
                CString::new(name.as_bytes()).map_err(|_| error("feature_recovery_unsafe"))?;
            let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
            let mut fd = unsafe { libc::openat(file.as_raw_fd(), name.as_ptr(), flags) };
            let mut created = false;
            if fd < 0
                && create
                && std::io::Error::last_os_error().kind() == std::io::ErrorKind::NotFound
            {
                if unsafe { libc::mkdirat(file.as_raw_fd(), name.as_ptr(), 0o700) } != 0 {
                    return Err(error("feature_storage_unavailable"));
                }
                file.sync_all()
                    .map_err(|_| error("feature_storage_unavailable"))?;
                fd = unsafe { libc::openat(file.as_raw_fd(), name.as_ptr(), flags) };
                created = true;
            }
            if fd < 0 {
                return Err(error("feature_recovery_unsafe"));
            }
            file = unsafe { File::from_raw_fd(fd) };
            if created {
                file.set_permissions(Permissions::from_mode(0o700))
                    .map_err(|_| error("feature_storage_unavailable"))?;
            }
            let m = file
                .metadata()
                .map_err(|_| error("feature_recovery_unsafe"))?;
            let final_component = index + 1 == components.len();
            if !m.is_dir()
                || (!owner(m.uid()) && m.uid() != 0)
                || (m.mode() & 0o022 != 0 && m.mode() & 0o1000 == 0)
                || (final_component && (!owner(m.uid()) || (private && m.mode() & 0o7777 != 0o700)))
            {
                return Err(error("feature_recovery_unsafe"));
            }
        }
        let m = file
            .metadata()
            .map_err(|_| error("feature_recovery_unsafe"))?;
        Ok(Self {
            path: path.to_owned(),
            file,
            private,
            identity: (m.dev(), m.ino()),
        })
    }
    fn check(&self) -> Result<(), Error> {
        let fresh = Self::open(&self.path, false, self.private)?;
        if fresh.identity != self.identity {
            return Err(error("feature_recovery_unsafe"));
        }
        Ok(())
    }
    fn file(&self, name: &str) -> Result<Option<File>, Error> {
        self.check()?;
        let name = cname(name)?;
        let fd = unsafe {
            libc::openat(
                self.file.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::NotFound {
                return Ok(None);
            }
            return Err(error("feature_recovery_unsafe"));
        }
        let file = unsafe { File::from_raw_fd(fd) };
        let m = file
            .metadata()
            .map_err(|_| error("feature_storage_unavailable"))?;
        if !m.is_file()
            || m.nlink() != 1
            || !owner(m.uid())
            || m.mode() & 0o7000 != 0
            || (self.private && m.mode() & 0o777 != 0o600)
        {
            return Err(error("feature_recovery_unsafe"));
        }
        Ok(Some(file))
    }
    fn read(&self, name: &str, limit: usize, b: &Budget<'_>) -> Result<Option<Vec<u8>>, Error> {
        check(b)?;
        let Some(mut file) = self.file(name)? else {
            return Ok(None);
        };
        if file
            .metadata()
            .map_err(|_| error("feature_storage_unavailable"))?
            .len()
            > limit as u64
        {
            return Err(error("feature_checkpoint_too_large"));
        }
        let mut bytes = Vec::new();
        let mut chunk = [0; 8192];
        loop {
            check(b)?;
            let n = file
                .read(&mut chunk)
                .map_err(|_| error("feature_storage_unavailable"))?;
            if n == 0 {
                return Ok(Some(bytes));
            }
            if n > limit.saturating_sub(bytes.len()) {
                return Err(error("feature_checkpoint_too_large"));
            }
            bytes.extend_from_slice(&chunk[..n]);
        }
    }
    fn admit(&self, growth: u64) -> Result<(), Error> {
        self.check()?;
        let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        if unsafe { libc::fstatvfs(self.file.as_raw_fd(), stat.as_mut_ptr()) } != 0 {
            return Err(error("feature_storage_insufficient"));
        }
        let stat = unsafe { stat.assume_init() };
        #[allow(clippy::unnecessary_cast)]
        let available = (stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64);
        if available < FREE_HEADROOM_BYTES.saturating_add(growth) {
            return Err(error("feature_storage_insufficient"));
        }
        Ok(())
    }
    fn write(&self, name: &str, bytes: &[u8], mode: u32, b: &Budget<'_>) -> Result<(), Error> {
        check(b)?;
        if mode & !0o777 != 0 {
            return Err(error("feature_recovery_corrupt"));
        }
        self.admit(rounded(bytes.len()))?;
        // Refuse links, foreign owners and special files before replacement.
        let _ = self.file(name)?;
        let final_name = cname(name)?;
        let mut random = [0; 16];
        getrandom::fill(&mut random).map_err(|_| error("feature_storage_unavailable"))?;
        let temporary = format!(
            ".recovery-{}",
            random
                .iter()
                .map(|v| format!("{v:02x}"))
                .collect::<String>()
        );
        let temporary = cname(&temporary)?;
        let fd = unsafe {
            libc::openat(
                self.file.as_raw_fd(),
                temporary.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600 as libc::c_uint,
            )
        };
        if fd < 0 {
            return Err(error("feature_storage_unavailable"));
        }
        let result = (|| {
            let mut file = unsafe { File::from_raw_fd(fd) };
            file.set_permissions(Permissions::from_mode(0o600))
                .map_err(|_| error("feature_storage_unavailable"))?;
            for chunk in bytes.chunks(8192) {
                check(b)?;
                file.write_all(chunk)
                    .map_err(|_| error("feature_storage_unavailable"))?;
            }
            // Original native mode is applied only after all private bytes exist.
            file.set_permissions(Permissions::from_mode(mode))
                .map_err(|_| error("feature_storage_unavailable"))?;
            file.sync_all()
                .map_err(|_| error("feature_storage_unavailable"))?;
            check(b)?;
            self.check()?;
            let _ = self.file(name)?;
            if unsafe {
                libc::renameat(
                    self.file.as_raw_fd(),
                    temporary.as_ptr(),
                    self.file.as_raw_fd(),
                    final_name.as_ptr(),
                )
            } != 0
            {
                return Err(error("feature_storage_unavailable"));
            }
            self.file
                .sync_all()
                .map_err(|_| error("feature_storage_unavailable"))
        })();
        unsafe {
            libc::unlinkat(self.file.as_raw_fd(), temporary.as_ptr(), 0);
        }
        result
    }
    fn remove(&self, name: &str) -> Result<(), Error> {
        if self.file(name)?.is_none() {
            return Ok(());
        }
        let name = cname(name)?;
        if unsafe { libc::unlinkat(self.file.as_raw_fd(), name.as_ptr(), 0) } != 0 {
            return Err(error("feature_storage_unavailable"));
        }
        self.file
            .sync_all()
            .map_err(|_| error("feature_storage_unavailable"))
    }
}
use std::os::unix::ffi::OsStrExt;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    exists: bool,
    mode: u32,
    size: usize,
    sha256: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    version: u8,
    id: String,
    phase: String,
    configs: Vec<String>,
    include_device_db: bool,
    entries: Vec<Entry>,
}
impl Journal {
    fn validate(&self, id: &str) -> Result<(), Error> {
        if self.version != 1
            || self.id != id
            || !safe_id(id)
            || self.phase != "armed"
            || self.configs.len() > MAX_CONFIGS
            || self.entries.len() != self.configs.len() + usize::from(self.include_device_db)
            || (self.include_device_db && !self.configs.iter().any(|c| c == "devicelist"))
        {
            return Err(error("feature_recovery_corrupt"));
        }
        let mut names = BTreeSet::new();
        let mut uci_total = 0usize;
        for (index, entry) in self.entries.iter().enumerate() {
            if index < self.configs.len() {
                if !config_name(&self.configs[index]) || !names.insert(&self.configs[index]) {
                    return Err(error("feature_recovery_corrupt"));
                }
                uci_total = uci_total.saturating_add(entry.size);
            } else if entry.size > MAX_DEVICE_DB_BYTES {
                return Err(error("feature_recovery_corrupt"));
            }
            if entry.mode & !0o777 != 0
                || entry.sha256.len() != 64
                || !entry.sha256.bytes().all(|v| v.is_ascii_hexdigit())
                || (!entry.exists
                    && (entry.size != 0 || entry.mode != 0o600 || entry.sha256 != digest(&[])))
            {
                return Err(error("feature_recovery_corrupt"));
            }
        }
        if uci_total > MAX_UCI_BYTES {
            return Err(error("feature_recovery_corrupt"));
        }
        Ok(())
    }
}
fn backup_name(index: usize) -> String {
    format!("backup-{index:02}")
}

/// Backend reads are checked against a no-follow descriptor. This rejects a
/// changing source and does not confuse a missing file with a read failure.
fn snapshot<B: Backend>(
    dir: &Directory,
    name: &str,
    limit: usize,
    sqlite: bool,
    io: &mut B,
    b: &Budget<'_>,
) -> Result<(Entry, Option<Vec<u8>>), Error> {
    check(b)?;
    let Some(mut file) = dir.file(name)? else {
        return Ok((
            Entry {
                exists: false,
                mode: 0o600,
                size: 0,
                sha256: digest(&[]),
            },
            None,
        ));
    };
    let before = file
        .metadata()
        .map_err(|_| error("feature_storage_unavailable"))?;
    if before.len() > limit as u64 {
        return Err(error("feature_checkpoint_too_large"));
    }
    let bytes = io.read(&dir.path.join(name), limit, b).map_err(io_error)?;
    if bytes.len() > limit {
        return Err(error("feature_checkpoint_too_large"));
    }
    if sqlite {
        sqlite_guard(dir, name, &file, &bytes)?;
    }
    let mut offset = 0usize;
    let mut chunk = [0; 8192];
    loop {
        check(b)?;
        let n = file
            .read(&mut chunk)
            .map_err(|_| error("feature_storage_unavailable"))?;
        if n == 0 {
            break;
        }
        if n > bytes.len().saturating_sub(offset) || chunk[..n] != bytes[offset..offset + n] {
            return Err(error("feature_storage_unavailable"));
        }
        offset += n;
    }
    let after = file
        .metadata()
        .map_err(|_| error("feature_storage_unavailable"))?;
    let Some(current) = dir.file(name)? else {
        return Err(error("feature_storage_unavailable"));
    };
    let current = current
        .metadata()
        .map_err(|_| error("feature_storage_unavailable"))?;
    if offset != bytes.len()
        || before.len() != bytes.len() as u64
        || before.mtime() != after.mtime()
        || before.mtime_nsec() != after.mtime_nsec()
        || before.ctime() != after.ctime()
        || before.ctime_nsec() != after.ctime_nsec()
        || before.mode() != after.mode()
        || (before.dev(), before.ino()) != (current.dev(), current.ino())
    {
        return Err(error("feature_storage_unavailable"));
    }
    if sqlite {
        sqlite_sidecars(dir, name)?;
    }
    Ok((
        Entry {
            exists: true,
            mode: before.mode() & 0o777,
            size: bytes.len(),
            sha256: digest(&bytes),
        },
        Some(bytes),
    ))
}
fn sqlite_sidecars(dir: &Directory, name: &str) -> Result<(), Error> {
    for suffix in ["-wal", "-journal", "-shm"] {
        if let Some(file) = dir.file(&format!("{name}{suffix}"))? {
            let length = file
                .metadata()
                .map_err(|_| error("feature_storage_unavailable"))?
                .len();
            if suffix == "-journal" || (suffix != "-shm" && length != 0) {
                return Err(error("feature_sqlite_busy"));
            }
        }
    }
    Ok(())
}
fn sqlite_guard(dir: &Directory, name: &str, file: &File, bytes: &[u8]) -> Result<(), Error> {
    sqlite_sidecars(dir, name)?;
    if bytes.len() < 100 || &bytes[..16] != b"SQLite format 3\0" || bytes[18] != 1 || bytes[19] != 1
    {
        return Err(error("feature_sqlite_busy"));
    }
    // SQLite's rollback-mode pending/reserved/shared byte range. A read lock
    // here refuses active writers and prevents a writer starting during copy.
    let mut lock: libc::flock = unsafe { std::mem::zeroed() };
    lock.l_type = libc::F_RDLCK as _;
    lock.l_whence = libc::SEEK_SET as _;
    lock.l_start = 0x4000_0000;
    lock.l_len = 512;
    if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETLK, &lock) } != 0 {
        return Err(error("feature_sqlite_busy"));
    }
    Ok(())
}

pub struct Recovery {
    root: Directory,
    data: Directory,
    config_dir: PathBuf,
    device_db: PathBuf,
    journal: Journal,
}
impl Recovery {
    pub fn prepare<B: Backend>(
        root: &Path,
        id: &str,
        configs: &[&str],
        include_device_db: bool,
        io: &mut B,
        b: &Budget<'_>,
    ) -> Result<Self, Error> {
        Self::prepare_with_paths(
            root,
            id,
            configs,
            include_device_db,
            Path::new("/etc/config"),
            Path::new("/etc/xqDb"),
            io,
            b,
        )
    }
    /// Administrator-only fixture seam. These paths never come from a client or journal.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_with_paths<B: Backend>(
        root: &Path,
        id: &str,
        configs: &[&str],
        include_device_db: bool,
        config_dir: &Path,
        device_db: &Path,
        io: &mut B,
        b: &Budget<'_>,
    ) -> Result<Self, Error> {
        check(b)?;
        if !safe_id(id)
            || configs.len() > MAX_CONFIGS
            || configs.iter().any(|name| !config_name(name))
        {
            return Err(crate::features::invalid());
        }
        let mut names = BTreeSet::new();
        let mut configs = configs
            .iter()
            .map(|name| name.to_string())
            .collect::<Vec<_>>();
        if configs.iter().any(|name| !names.insert(name.clone())) {
            return Err(crate::features::invalid());
        }
        if include_device_db && !configs.iter().any(|c| c == "devicelist") {
            configs.push("devicelist".into());
        }
        if configs.len() > MAX_CONFIGS {
            return Err(crate::features::invalid());
        }
        let native = Directory::open(config_dir, false, false)?;
        let (db_dir, db_name) = db_location(device_db)?;
        let root_dir = Directory::open(root, true, true)?;
        if root.starts_with(config_dir) || root.starts_with(&db_dir.path) {
            return Err(error("feature_recovery_unsafe"));
        }
        if !pending_ids(&root_dir)?.is_empty() {
            return Err(error("feature_recovery_pending"));
        }
        root_dir.admit(rounded(MAX_JOURNAL_BYTES))?;
        let id_name = cname(id)?;
        if unsafe { libc::mkdirat(root_dir.file.as_raw_fd(), id_name.as_ptr(), 0o700) } != 0 {
            return Err(error("feature_storage_unavailable"));
        }
        root_dir
            .file
            .sync_all()
            .map_err(|_| error("feature_storage_unavailable"))?;
        let data = Directory::open(&root.join(id), false, true)?;
        let mut journal = Journal {
            version: 1,
            id: id.into(),
            phase: "armed".into(),
            configs,
            include_device_db,
            entries: Vec::new(),
        };
        let mut owned = Vec::new();
        let result = (|| {
            let mut total = 0usize;
            for index in 0..journal.configs.len() + usize::from(include_device_db) {
                check(b)?;
                let is_db = index == journal.configs.len();
                let (dir, name, limit) = if is_db {
                    (&db_dir, db_name.as_str(), MAX_DEVICE_DB_BYTES)
                } else {
                    (
                        &native,
                        journal.configs[index].as_str(),
                        MAX_UCI_BYTES.saturating_sub(total),
                    )
                };
                if is_db {
                    sqlite_sidecars(dir, name)?;
                }
                let (entry, bytes) = snapshot(dir, name, limit, is_db, io, b)?;
                if !is_db {
                    total += entry.size;
                }
                if let Some(bytes) = bytes {
                    let name = backup_name(index);
                    // Each write reserves its own temporary growth plus the full,
                    // independent 1 MiB free-space headroom, including rename.
                    data.write(&name, &bytes, 0o600, b)?;
                    owned.push(name);
                }
                journal.entries.push(entry);
            }
            journal.validate(id)?;
            let encoded =
                serde_json::to_vec(&journal).map_err(|_| error("feature_storage_unavailable"))?;
            if encoded.len() > MAX_JOURNAL_BYTES {
                return Err(error("feature_checkpoint_too_large"));
            }
            data.write(JOURNAL, &encoded, 0o600, b)
        })();
        if let Err(e) = result {
            // Never clean a journal that may have reached its rename boundary.
            if data.file(JOURNAL).ok().flatten().is_none() {
                for name in owned {
                    let _ = data.remove(&name);
                }
                let _ = remove_operation_dir(&root_dir, id, &data);
            }
            return Err(e);
        }
        Ok(Self {
            root: root_dir,
            data,
            config_dir: config_dir.to_owned(),
            device_db: device_db.to_owned(),
            journal,
        })
    }
    pub fn restore<B: Backend>(
        &mut self,
        io: &mut B,
        b: &Budget<'_>,
    ) -> Result<Vec<String>, Error> {
        check(b)?;
        self.journal.validate(&self.journal.id)?;
        let native = Directory::open(&self.config_dir, false, false)?;
        let (db_dir, db_name) = db_location(&self.device_db)?;
        // Validate all backups and all live destinations before the first write.
        let mut backups = Vec::new();
        for (index, entry) in self.journal.entries.iter().enumerate() {
            check(b)?;
            let saved = self.data.read(&backup_name(index), entry.size, b)?;
            if (entry.exists
                && saved
                    .as_ref()
                    .is_none_or(|raw| raw.len() != entry.size || digest(raw) != entry.sha256))
                || (!entry.exists && saved.is_some())
            {
                return Err(error("feature_recovery_corrupt"));
            }
            let is_db = index == self.journal.configs.len();
            let (dir, name, limit) = if is_db {
                (&db_dir, db_name.as_str(), MAX_DEVICE_DB_BYTES)
            } else {
                (&native, self.journal.configs[index].as_str(), MAX_UCI_BYTES)
            };
            if is_db {
                sqlite_sidecars(dir, name)?;
            }
            let _ = snapshot(dir, name, limit, is_db, io, b)?;
            if let Some(raw) = &saved {
                dir.admit(rounded(raw.len()))?;
            }
            backups.push(saved);
        }
        for (index, raw) in backups.iter().enumerate() {
            check(b)?;
            let is_db = index == self.journal.configs.len();
            let (dir, name) = if is_db {
                (&db_dir, db_name.as_str())
            } else {
                (&native, self.journal.configs[index].as_str())
            };
            if is_db {
                sqlite_sidecars(dir, name)?;
            }
            if let Some(raw) = raw {
                dir.write(name, raw, self.journal.entries[index].mode, b)?;
            } else {
                dir.remove(name)?;
            }
        }
        // Return the armed descriptor scope even for an idempotent retry. A
        // previous reload may have failed after all original bytes were restored.
        for (index, entry) in self.journal.entries.iter().enumerate() {
            check(b)?;
            let is_db = index == self.journal.configs.len();
            let (dir, name, limit) = if is_db {
                (&db_dir, db_name.as_str(), MAX_DEVICE_DB_BYTES)
            } else {
                (&native, self.journal.configs[index].as_str(), MAX_UCI_BYTES)
            };
            let (observed, _) = snapshot(dir, name, limit, is_db, io, b)?;
            if observed.exists != entry.exists
                || observed.mode != entry.mode
                || observed.size != entry.size
                || observed.sha256 != entry.sha256
            {
                return Err(error("feature_storage_unavailable"));
            }
        }
        Ok(self.journal.configs.clone())
    }
    pub fn verify_restored<B: Backend>(&self, io: &mut B, b: &Budget<'_>) -> Result<(), Error> {
        check(b)?;
        self.journal.validate(&self.journal.id)?;
        let native = Directory::open(&self.config_dir, false, false)?;
        let (db_dir, db_name) = db_location(&self.device_db)?;
        for (index, entry) in self.journal.entries.iter().enumerate() {
            let is_db = index == self.journal.configs.len();
            let (dir, name, limit) = if is_db {
                (&db_dir, db_name.as_str(), MAX_DEVICE_DB_BYTES)
            } else {
                (&native, self.journal.configs[index].as_str(), MAX_UCI_BYTES)
            };
            let (observed, _) = snapshot(dir, name, limit, is_db, io, b)?;
            if observed.exists != entry.exists
                || observed.mode != entry.mode
                || observed.size != entry.size
                || observed.sha256 != entry.sha256
            {
                return Err(error("feature_storage_unavailable"));
            }
        }
        Ok(())
    }
    pub fn verify_recovered<B: Backend>(
        root: &Path,
        io: &mut B,
        b: &Budget<'_>,
    ) -> Result<(), Error> {
        Self::verify_recovered_with_paths(
            root,
            Path::new("/etc/config"),
            Path::new("/etc/xqDb"),
            io,
            b,
        )
    }
    pub fn verify_recovered_with_paths<B: Backend>(
        root: &Path,
        config_dir: &Path,
        device_db: &Path,
        io: &mut B,
        b: &Budget<'_>,
    ) -> Result<(), Error> {
        if missing_root(root)? {
            return Ok(());
        }
        let data = Directory::open(root, false, true)?;
        let ids = pending_ids(&data)?;
        if ids.len() > 1 {
            return Err(error("feature_recovery_corrupt"));
        }
        if let Some(id) = ids.first() {
            Self::load(root, id, config_dir, device_db, b)?.verify_restored(io, b)?;
        }
        Ok(())
    }

    pub fn discard(self) -> Result<(), Error> {
        self.clean()
    }
    fn clean(&self) -> Result<(), Error> {
        self.journal.validate(&self.journal.id)?;
        self.root.check()?;
        self.data.check()?;
        // Remove the journal first: from this durable boundary the operation is
        // accepted. Interrupted cleanup leaves only inert private backup files.
        self.data.remove(JOURNAL)?;
        for (index, entry) in self.journal.entries.iter().enumerate() {
            if entry.exists {
                self.data.remove(&backup_name(index))?;
            }
        }
        remove_operation_dir(&self.root, &self.journal.id, &self.data)
    }
    pub fn recover<B: Backend>(
        root: &Path,
        io: &mut B,
        b: &Budget<'_>,
    ) -> Result<Vec<String>, Error> {
        Self::recover_with_paths(
            root,
            Path::new("/etc/config"),
            Path::new("/etc/xqDb"),
            io,
            b,
        )
    }
    pub fn recover_with_paths<B: Backend>(
        root: &Path,
        config_dir: &Path,
        device_db: &Path,
        io: &mut B,
        b: &Budget<'_>,
    ) -> Result<Vec<String>, Error> {
        check(b)?;
        if missing_root(root)? {
            return Ok(Vec::new());
        }
        let data = Directory::open(root, false, true)?;
        let ids = pending_ids(&data)?;
        if ids.len() > 1 {
            return Err(error("feature_recovery_corrupt"));
        }
        let Some(id) = ids.first() else {
            return Ok(Vec::new());
        };
        let mut recovery = Self::load(root, id, config_dir, device_db, b)?;
        recovery.restore(io, b)
    }
    /// Root calls this only after its fixed recovery reload/readback succeeds.
    pub fn discard_recovered(root: &Path) -> Result<(), Error> {
        if missing_root(root)? {
            return Ok(());
        }
        let data = Directory::open(root, false, true)?;
        let ids = pending_ids(&data)?;
        if ids.len() > 1 {
            return Err(error("feature_recovery_corrupt"));
        }
        let Some(id) = ids.first() else {
            return Ok(());
        };
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let b = Budget {
            deadline: std::time::Instant::now() + std::time::Duration::from_secs(5),
            cancel: &cancel,
        };
        Self::load(
            root,
            id,
            Path::new("/etc/config"),
            Path::new("/etc/xqDb"),
            &b,
        )?
        .clean()
    }
    fn load(
        root: &Path,
        id: &str,
        config_dir: &Path,
        device_db: &Path,
        b: &Budget<'_>,
    ) -> Result<Self, Error> {
        let root_dir = Directory::open(root, false, true)?;
        let data = Directory::open(&root.join(id), false, true)?;
        let raw = data
            .read(JOURNAL, MAX_JOURNAL_BYTES, b)?
            .ok_or_else(|| error("feature_recovery_corrupt"))?;
        let journal: Journal =
            serde_json::from_slice(&raw).map_err(|_| error("feature_recovery_corrupt"))?;
        journal.validate(id)?;
        Ok(Self {
            root: root_dir,
            data,
            config_dir: config_dir.to_owned(),
            device_db: device_db.to_owned(),
            journal,
        })
    }
}
fn db_location(path: &Path) -> Result<(Directory, String), Error> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| error("feature_recovery_unsafe"))?;
    let _ = cname(name)?;
    Ok((
        Directory::open(
            path.parent()
                .ok_or_else(|| error("feature_recovery_unsafe"))?,
            false,
            false,
        )?,
        name.into(),
    ))
}
fn missing_root(path: &Path) -> Result<bool, Error> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(_) => Err(error("feature_recovery_unsafe")),
        Ok(_) => Ok(false),
    }
}
fn pending_ids(root: &Directory) -> Result<Vec<String>, Error> {
    root.check()?;
    let mut ids = Vec::new();
    let mut count = 0usize;
    for entry in fs::read_dir(&root.path).map_err(|_| error("feature_storage_unavailable"))? {
        count += 1;
        if count > MAX_PENDING {
            return Err(error("feature_recovery_corrupt"));
        }
        let entry = entry.map_err(|_| error("feature_storage_unavailable"))?;
        let name = entry.file_name();
        let Some(id) = name.to_str() else {
            return Err(error("feature_recovery_unsafe"));
        };
        // Generation and other root-owned public records are not this module's
        // artifacts. Never delete them or accept their paths as recovery targets.
        if !safe_id(id) {
            continue;
        }
        let op = Directory::open(&root.path.join(id), false, true)?;
        if op.file(JOURNAL)?.is_some() {
            ids.push(id.to_owned());
        }
    }
    ids.sort();
    Ok(ids)
}
fn remove_operation_dir(root: &Directory, id: &str, data: &Directory) -> Result<(), Error> {
    root.check()?;
    data.check()?;
    let name = cname(id)?;
    // No recursive removal: unknown files remain, even after successful discard.
    if unsafe { libc::unlinkat(root.file.as_raw_fd(), name.as_ptr(), libc::AT_REMOVEDIR) } != 0 {
        if std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOTEMPTY) {
            return Ok(());
        }
        return Err(error("feature_storage_unavailable"));
    }
    root.file
        .sync_all()
        .map_err(|_| error("feature_storage_unavailable"))
}

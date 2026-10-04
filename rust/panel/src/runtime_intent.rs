//! Two fixed saved service flags. Reads never launch processes; writes never
//! change accepted config, core PID or capture. Off may be latched by the caller
//! even when its private persistence fails.
use crate::runtime_manager::ServiceId;
use serde::{Deserialize, Serialize};
use std::{
    ffi::CString,
    fmt,
    fs::{File, Metadata, OpenOptions},
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
};
pub const MAX_BYTES: usize = 4096;
const FILE: &std::ffi::CStr = c"desired-services.json";
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DesiredServices {
    #[serde(rename = "sing-box")]
    pub sing_box: bool,
    pub frpc: bool,
}
impl DesiredServices {
    pub fn get(self, service: ServiceId) -> bool {
        match service {
            ServiceId::SingBox => self.sing_box,
            ServiceId::Frpc => self.frpc,
        }
    }
    fn set(&mut self, service: ServiceId, value: bool) {
        match service {
            ServiceId::SingBox => self.sing_box = value,
            ServiceId::Frpc => self.frpc = value,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntentError {
    Storage,
    Invalid,
    InsufficientSpace,
    Measurement,
}
impl fmt::Display for IntentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Storage => "runtime intent storage unavailable",
            Self::Invalid => "saved runtime intent invalid",
            Self::InsufficientSpace => "runtime intent needs recovery headroom",
            Self::Measurement => "runtime intent free space unavailable",
        })
    }
}
impl std::error::Error for IntentError {}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaveOutcome {
    pub desired: DesiredServices,
    pub durable: bool,
}
pub struct RuntimeIntent {
    path: PathBuf,
    directory: File,
    identity: (u64, u64),
    file: Option<File>,
    stamp: Option<(u64, i64, i64)>,
    desired: DesiredServices,
}
impl fmt::Debug for RuntimeIntent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RuntimeIntent")
            .field("desired", &self.desired)
            .finish_non_exhaustive()
    }
}
fn stamp(metadata: &Metadata) -> (u64, i64, i64) {
    (metadata.len(), metadata.mtime(), metadata.mtime_nsec())
}
fn id(metadata: &Metadata) -> (u64, u64) {
    (metadata.dev(), metadata.ino())
}
fn open_at(
    directory: &File,
    name: &std::ffi::CStr,
    flags: i32,
    mode: libc::mode_t,
) -> std::io::Result<File> {
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
            libc::c_uint::from(mode),
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}
impl RuntimeIntent {
    pub fn open(path: &Path) -> Result<Self, IntentError> {
        if !path.is_absolute() {
            return Err(IntentError::Invalid);
        }
        let directory = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)
            .map_err(|_| IntentError::Storage)?;
        let metadata = directory.metadata().map_err(|_| IntentError::Storage)?;
        if !metadata.is_dir()
            || metadata.mode() & 0o7777 != 0o700
            || metadata.uid() != unsafe { libc::geteuid() }
        {
            return Err(IntentError::Storage);
        }
        let mut file = match open_at(&directory, FILE, libc::O_RDONLY, 0) {
            Ok(file) => Some(file),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => return Err(IntentError::Storage),
        };
        let mut saved_stamp = None;
        let desired = if let Some(file) = &mut file {
            let metadata = file.metadata().map_err(|_| IntentError::Storage)?;
            if !metadata.is_file()
                || metadata.mode() & 0o7777 != 0o600
                || metadata.uid() != unsafe { libc::geteuid() }
                || metadata.nlink() != 1
                || metadata.len() > MAX_BYTES as u64
            {
                return Err(IntentError::Invalid);
            }
            let mut bytes = Vec::with_capacity(metadata.len() as usize);
            let mut buffer = [0u8; 1024];
            loop {
                let count = file.read(&mut buffer).map_err(|_| IntentError::Storage)?;
                if count == 0 {
                    break;
                }
                if count > MAX_BYTES.saturating_sub(bytes.len()) {
                    return Err(IntentError::Invalid);
                }
                bytes.extend_from_slice(&buffer[..count]);
            }
            if bytes
                .iter()
                .find(|b| !b.is_ascii_whitespace())
                .is_none_or(|b| *b != b'{')
            {
                return Err(IntentError::Invalid);
            }
            let after = file.metadata().map_err(|_| IntentError::Storage)?;
            if id(&after) != id(&metadata) || stamp(&after) != stamp(&metadata) {
                return Err(IntentError::Storage);
            }
            saved_stamp = Some(stamp(&after));
            serde_json::from_slice(&bytes).map_err(|_| IntentError::Invalid)?
        } else {
            DesiredServices::default()
        };
        let value = Self {
            path: path.into(),
            directory,
            identity: id(&metadata),
            file,
            stamp: saved_stamp,
            desired,
        };
        value.check()?;
        Ok(value)
    }
    pub fn desired(&self) -> DesiredServices {
        self.desired
    }
    fn check(&self) -> Result<(), IntentError> {
        let directory = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&self.path)
            .map_err(|_| IntentError::Storage)?;
        let metadata = directory.metadata().map_err(|_| IntentError::Storage)?;
        if id(&metadata) != self.identity
            || metadata.mode() & 0o7777 != 0o700
            || metadata.uid() != unsafe { libc::geteuid() }
        {
            return Err(IntentError::Storage);
        }
        let current = match open_at(&self.directory, FILE, libc::O_RDONLY, 0) {
            Ok(file) => Some(file),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => return Err(IntentError::Storage),
        };
        match (&self.file, current) {
            (None, None) => Ok(()),
            (Some(pin), Some(current)) => {
                let p = pin.metadata().map_err(|_| IntentError::Storage)?;
                let c = current.metadata().map_err(|_| IntentError::Storage)?;
                if id(&p) == id(&c)
                    && c.is_file()
                    && c.mode() & 0o7777 == 0o600
                    && c.nlink() == 1
                    && c.uid() == unsafe { libc::geteuid() }
                    && self.stamp == Some(stamp(&c))
                    && c.len() <= MAX_BYTES as u64
                {
                    Ok(())
                } else {
                    Err(IntentError::Storage)
                }
            }
            _ => Err(IntentError::Storage),
        }
    }
    pub fn save(&mut self, service: ServiceId, value: bool) -> Result<SaveOutcome, IntentError> {
        let mut desired = self.desired;
        desired.set(service, value);
        self.save_desired(desired)
    }
    pub fn save_desired(&mut self, desired: DesiredServices) -> Result<SaveOutcome, IntentError> {
        self.check()?;
        let bytes = serde_json::to_vec(&desired).map_err(|_| IntentError::Invalid)?;
        let mut space = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        if unsafe { libc::fstatvfs(self.directory.as_raw_fd(), space.as_mut_ptr()) } != 0 {
            return Err(IntentError::Measurement);
        }
        let space = unsafe { space.assume_init() };
        let unit = if space.f_frsize == 0 {
            space.f_bsize
        } else {
            space.f_frsize
        };
        let available = u128::from(space.f_bavail)
            .checked_mul(u128::from(unit))
            .ok_or(IntentError::Measurement)?;
        let growth = 2u128
            .checked_mul(u128::from(unit).max(4096))
            .ok_or(IntentError::Measurement)?;
        if available < u128::from(crate::runtime_store::FREE_HEADROOM_BYTES) + growth {
            return Err(IntentError::InsufficientSpace);
        }
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).map_err(|_| IntentError::Storage)?;
        let name = CString::new(format!(
            ".runtime-intent-{}",
            random
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        ))
        .map_err(|_| IntentError::Storage)?;
        let mut file = open_at(
            &self.directory,
            &name,
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
            0o600,
        )
        .map_err(|_| IntentError::Storage)?;
        let written = (|| {
            file.set_permissions(std::fs::Permissions::from_mode(0o600))
                .map_err(|_| IntentError::Storage)?;
            file.write_all(&bytes).map_err(|_| IntentError::Storage)?;
            file.sync_all().map_err(|_| IntentError::Storage)?;
            self.check()?;
            if unsafe {
                libc::renameat(
                    self.directory.as_raw_fd(),
                    name.as_ptr(),
                    self.directory.as_raw_fd(),
                    FILE.as_ptr(),
                )
            } != 0
            {
                return Err(IntentError::Storage);
            }
            Ok(())
        })();
        if let Err(error) = written {
            unsafe {
                libc::unlinkat(self.directory.as_raw_fd(), name.as_ptr(), 0);
            }
            return Err(error);
        }
        self.stamp = file.metadata().ok().map(|meta| stamp(&meta));
        self.file = Some(file);
        self.desired = desired;
        let durable = self.directory.sync_all().is_ok() && self.check().is_ok();
        Ok(SaveOutcome { desired, durable })
    }
}

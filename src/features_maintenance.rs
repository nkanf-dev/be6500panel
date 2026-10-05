//! Root-owned maintenance on the existing authenticated serial management lane.
//! No scheduler thread, shell-input endpoint, vendor reply, or offload write exists.
//! The private record is the desired state. `reloadPending` remains true across
//! cron publication/restart failure; a later identical set can retry that work.
use crate::features::{Error, invalid};
use crate::product_io::{Backend, Error as IoError, Program};
use crate::readiness_tun::Budget;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::ffi::CStr;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};

pub const MAX_SCHEDULE_BYTES: usize = 4 << 10;
pub const MAX_CRON_BYTES: usize = 32 << 10;
pub const FREE_HEADROOM_BYTES: u64 = 1 << 20;
pub const CRON_BEGIN: &str = "# BEGIN be6500-panel scheduled-reboot";
pub const CRON_END: &str = "# END be6500-panel scheduled-reboot";
pub const REBOOT_SCRIPT: &[u8] = b"#!/bin/sh\nexec /sbin/reboot\n";
const RECORD: &CStr = c"maintenance-schedule.json";
const SCRIPT: &CStr = c"scheduled-reboot.sh";
const ROOT_CRON: &CStr = c"root";
const RECORD_STAGE: &CStr = c".maintenance-schedule.pending";
const SETTLED_STAGE: &CStr = c".maintenance-schedule.settled";
const SCRIPT_STAGE: &CStr = c".scheduled-reboot.stage";
const CRON_STAGE: &CStr = c".be6500-panel-reboot.stage";

/// Paths come only from trusted startup configuration, never from feature input.
/// The alternate constructor supports synthetic native roots without router IO.
#[derive(Clone)]
pub struct ConfigPaths {
    data: PathBuf,
    root_crontab: PathBuf,
    ecm_debugfs: PathBuf,
    modules: PathBuf,
}
impl std::fmt::Debug for ConfigPaths {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MaintenanceConfigPaths([private])")
    }
}
impl ConfigPaths {
    pub fn new(data: &Path) -> Self {
        Self::with_system_paths(
            data,
            Path::new("/etc/crontabs/root"),
            Path::new("/sys/kernel/debug/ecm"),
            Path::new("/sys/module"),
        )
    }
    pub fn with_system_paths(
        data: &Path,
        root_crontab: &Path,
        ecm_debugfs: &Path,
        modules: &Path,
    ) -> Self {
        Self {
            data: data.into(),
            root_crontab: root_crontab.into(),
            ecm_debugfs: ecm_debugfs.into(),
            modules: modules.into(),
        }
    }
}

fn failure(code: &'static str) -> Error {
    let (status, message) = match code {
        "operation_timeout" => (504, "维护操作超时。"),
        "operation_cancelled" => (409, "维护操作已取消。"),
        "maintenance_storage_insufficient" => (507, "存储空间不足，需保留恢复空间。"),
        "maintenance_cron_conflict" => (409, "计划重启条目发生冲突，未修改现有任务。"),
        "schedule_saved_cron_unavailable" => (503, "计划已保存，定时任务尚未完成更新。"),
        "schedule_saved_reload_failed" => (503, "计划已保存，定时服务尚未完成重启。"),
        "schedule_saved_durability_unconfirmed" => (503, "计划已保存，持久化确认尚未完成。"),
        "schedule_saved_operation_interrupted" => (503, "计划已保存，定时任务更新被中断。"),
        _ => (503, "维护配置暂不可用，现有配置未被重置。"),
    };
    Error {
        status,
        code,
        message,
    }
}
fn check(b: &Budget<'_>) -> Result<(), Error> {
    b.check().map_err(|e| {
        failure(if e == crate::readiness_tun::TunError::Cancelled {
            "operation_cancelled"
        } else {
            "operation_timeout"
        })
    })
}
fn storage() -> Error {
    failure("maintenance_storage_unavailable")
}
fn saved(error: Error) -> Error {
    failure(match error.code {
        "operation_timeout" | "operation_cancelled" => "schedule_saved_operation_interrupted",
        _ => "schedule_saved_cron_unavailable",
    })
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Schedule {
    enabled: bool,
    time: String,
    weekdays: Vec<u8>,
    #[serde(default)]
    reload_pending: bool,
}
impl Default for Schedule {
    fn default() -> Self {
        Self {
            enabled: false,
            time: "03:00".into(),
            weekdays: (0..7).collect(),
            reload_pending: false,
        }
    }
}
impl Schedule {
    fn public(&self) -> Value {
        json!({"enabled": self.enabled, "time": self.time, "weekdays": self.weekdays,
            "timeBasis": "router", "reloadPending": self.reload_pending})
    }
    fn valid(&self) -> bool {
        hhmm(&self.time)
            && self.weekdays.len() <= 7
            && self.weekdays.iter().all(|d| *d <= 6)
            && self.weekdays.windows(2).all(|d| d[0] < d[1])
            && (!self.enabled || !self.weekdays.is_empty())
    }
    fn bytes(&self) -> Result<Vec<u8>, Error> {
        let bytes = serde_json::to_vec(self).map_err(|_| storage())?;
        if bytes.len() > MAX_SCHEDULE_BYTES {
            return Err(storage());
        }
        Ok(bytes)
    }
}
fn hhmm(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes.len() == 5
        && bytes[2] == b':'
        && [0, 1, 3, 4].iter().all(|i| bytes[*i].is_ascii_digit())
        && (bytes[0] - b'0') * 10 + bytes[1] - b'0' < 24
        && (bytes[3] - b'0') * 10 + bytes[4] - b'0' < 60
}
fn input_schedule(input: &Value) -> Result<Schedule, Error> {
    let map = input.as_object().ok_or_else(invalid)?;
    if map
        .keys()
        .any(|k| !["enabled", "time", "weekdays"].contains(&k.as_str()))
    {
        return Err(invalid());
    }
    let enabled = input
        .get("enabled")
        .and_then(Value::as_bool)
        .ok_or_else(invalid)?;
    let time = input
        .get("time")
        .and_then(Value::as_str)
        .filter(|s| hhmm(s))
        .ok_or_else(invalid)?;
    // JSON fields from a LuCI-style form can be encoded strings. Disk/public
    // contracts always use an array, never an executable template or raw text.
    let decoded;
    let weekdays = match input.get("weekdays") {
        None => None,
        Some(Value::String(s)) if s.len() <= 64 => {
            decoded = serde_json::from_str::<Value>(s).map_err(|_| invalid())?;
            Some(&decoded)
        }
        Some(v) => Some(v),
    };
    let mut days = if let Some(value) = weekdays {
        let list = value
            .as_array()
            .filter(|a| a.len() <= 7)
            .ok_or_else(invalid)?;
        list.iter()
            .map(|d| {
                d.as_u64()
                    .filter(|d| *d <= 6)
                    .map(|d| d as u8)
                    .ok_or_else(invalid)
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        (0..7).collect()
    };
    days.sort_unstable();
    let schedule = Schedule {
        enabled,
        time: time.into(),
        weekdays: days,
        reload_pending: false,
    };
    if !schedule.valid() {
        return Err(invalid());
    }
    Ok(schedule)
}
fn no_input(input: &Value) -> Result<(), Error> {
    if input.as_object().is_some_and(|m| m.is_empty()) {
        Ok(())
    } else {
        Err(invalid())
    }
}
fn safe_absolute(path: &Path) -> bool {
    let Some(text) = path.to_str() else {
        return false;
    };
    path.is_absolute()
        && text.len() <= 1024
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'_' | b'-'))
        && path
            .components()
            .all(|c| matches!(c, Component::RootDir | Component::Normal(_)))
}

/// Pinned directory-relative writes reject symlinks, special files, other owners
/// and loose private modes. Missing private directories are never initialized.
struct Directory {
    path: PathBuf,
    file: File,
    identity: (u64, u64),
    private: bool,
}
impl Directory {
    fn open(path: &Path, private: bool, b: &Budget<'_>) -> Result<Self, Error> {
        check(b)?;
        if !safe_absolute(path) {
            return Err(storage());
        }
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)
            .map_err(|_| storage())?;
        let m = file.metadata().map_err(|_| storage())?;
        let uid = unsafe { libc::geteuid() };
        if !m.is_dir()
            || m.uid() != uid
            || m.mode() & 0o7000 != 0
            || (private && m.mode() & 0o777 != 0o700)
            || (!private && m.mode() & 0o022 != 0)
        {
            return Err(storage());
        }
        let directory = Self {
            path: path.into(),
            file,
            identity: (m.dev(), m.ino()),
            private,
        };
        directory.check(b)?;
        Ok(directory)
    }
    fn check(&self, b: &Budget<'_>) -> Result<(), Error> {
        check(b)?;
        let m = fs::symlink_metadata(&self.path).map_err(|_| storage())?;
        if !m.is_dir()
            || (m.dev(), m.ino()) != self.identity
            || m.uid() != unsafe { libc::geteuid() }
            || m.mode() & 0o7000 != 0
            || (self.private && m.mode() & 0o777 != 0o700)
            || (!self.private && m.mode() & 0o022 != 0)
        {
            return Err(storage());
        }
        Ok(())
    }
    fn read(
        &self,
        name: &CStr,
        limit: usize,
        exact_mode: Option<u32>,
        b: &Budget<'_>,
    ) -> Result<Option<Document>, Error> {
        self.check(b)?;
        let fd = unsafe {
            libc::openat(
                self.file.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
                0,
            )
        };
        if fd < 0 {
            return if std::io::Error::last_os_error().kind() == std::io::ErrorKind::NotFound {
                Ok(None)
            } else {
                Err(storage())
            };
        }
        let mut file = unsafe { File::from_raw_fd(fd) };
        let m = file.metadata().map_err(|_| storage())?;
        if !m.is_file()
            || m.nlink() != 1
            || m.uid() != unsafe { libc::geteuid() }
            || m.len() > limit as u64
            || m.mode() & 0o7022 != 0
            || exact_mode.is_some_and(|mode| m.mode() & 0o777 != mode)
        {
            return Err(storage());
        }
        let mut bytes = Vec::with_capacity(limit.min(m.len() as usize + 1));
        let mut chunk = [0; 1024];
        loop {
            check(b)?;
            let size = chunk.len().min(limit.saturating_sub(bytes.len()) + 1);
            let n = match file.read(&mut chunk[..size]) {
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                result => result.map_err(|_| storage())?,
            };
            if n == 0 {
                break;
            }
            if n > limit.saturating_sub(bytes.len()) {
                return Err(storage());
            }
            bytes.extend_from_slice(&chunk[..n]);
        }
        self.check(b)?;
        Ok(Some(Document {
            bytes,
            identity: (m.dev(), m.ino()),
            mode: m.mode() & 0o777,
        }))
    }
    fn measurement(&self, b: &Budget<'_>) -> Result<(u64, u64), Error> {
        self.check(b)?;
        let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        if unsafe { libc::fstatvfs(self.file.as_raw_fd(), stat.as_mut_ptr()) } != 0 {
            return Err(failure("maintenance_storage_insufficient"));
        }
        let stat = unsafe { stat.assume_init() };
        #[allow(clippy::unnecessary_cast)]
        let unit = if stat.f_frsize == 0 {
            stat.f_bsize as u64
        } else {
            stat.f_frsize as u64
        };
        #[allow(clippy::unnecessary_cast)]
        let available = (stat.f_bavail as u64)
            .checked_mul(unit)
            .ok_or_else(|| failure("maintenance_storage_insufficient"))?;
        if unit == 0 {
            return Err(failure("maintenance_storage_insufficient"));
        }
        Ok((available, unit.max(4096)))
    }
    fn stage<'a>(
        &'a self,
        name: &'static CStr,
        final_name: &'static CStr,
        bytes: &[u8],
        mode: u32,
        b: &Budget<'_>,
    ) -> Result<Staged<'a>, Error> {
        self.check(b)?;
        // Four fixed names bound leftovers after a killed caller. Never delete
        // or overwrite a pre-existing temporary or any emergency reserve.
        let fd = unsafe {
            libc::openat(
                self.file.as_raw_fd(),
                name.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                mode as libc::c_uint,
            )
        };
        if fd < 0 {
            return Err(storage());
        }
        let staged = Staged {
            directory: self,
            name,
            final_name,
            live: true,
        };
        let mut file = unsafe { File::from_raw_fd(fd) };
        if unsafe { libc::fchmod(file.as_raw_fd(), mode as libc::mode_t) } != 0 {
            return Err(storage());
        }
        for chunk in bytes.chunks(1024) {
            check(b)?;
            file.write_all(chunk).map_err(|_| storage())?;
        }
        check(b)?;
        file.sync_all().map_err(|_| storage())?;
        self.check(b)?;
        Ok(staged)
    }
}
#[derive(Clone, PartialEq, Eq)]
struct Document {
    bytes: Vec<u8>,
    identity: (u64, u64),
    mode: u32,
}
struct Staged<'a> {
    directory: &'a Directory,
    name: &'static CStr,
    final_name: &'static CStr,
    live: bool,
}
impl Staged<'_> {
    /// `Ok(false)` means rename committed but directory durability is uncertain.
    /// Never convert that result to a pre-commit failure or restore old cron bytes.
    fn publish(
        mut self,
        previous: &Option<Document>,
        limit: usize,
        mode: Option<u32>,
        b: &Budget<'_>,
    ) -> Result<bool, Error> {
        if self.directory.read(self.final_name, limit, mode, b)? != *previous {
            return Err(failure("maintenance_cron_conflict"));
        }
        self.directory.check(b)?;
        if unsafe {
            libc::renameat(
                self.directory.file.as_raw_fd(),
                self.name.as_ptr(),
                self.directory.file.as_raw_fd(),
                self.final_name.as_ptr(),
            )
        } != 0
        {
            return Err(storage());
        }
        self.live = false;
        Ok(self.directory.file.sync_all().is_ok())
    }
}
impl Drop for Staged<'_> {
    fn drop(&mut self) {
        if self.live {
            unsafe {
                libc::unlinkat(self.directory.file.as_raw_fd(), self.name.as_ptr(), 0);
            }
        }
    }
}
fn charge(unit: u64, sizes: &[usize]) -> Result<u64, Error> {
    sizes.iter().try_fold(0u64, |sum, size| {
        let bytes = (*size as u64)
            .checked_add(unit - 1)
            .and_then(|n| (n / unit).checked_add(1))
            .and_then(|n| n.checked_mul(unit));
        bytes
            .and_then(|n| sum.checked_add(n))
            .ok_or_else(|| failure("maintenance_storage_insufficient"))
    })
}
fn admit(
    data: &Directory,
    data_sizes: &[usize],
    cron: &Directory,
    cron_sizes: &[usize],
    b: &Budget<'_>,
) -> Result<(), Error> {
    let (data_free, data_unit) = data.measurement(b)?;
    let (cron_free, cron_unit) = cron.measurement(b)?;
    let data_need = charge(data_unit, data_sizes)?;
    let cron_need = charge(cron_unit, cron_sizes)?;
    let enough = |free: u64, need: u64| {
        free.checked_sub(need)
            .is_some_and(|n| n >= FREE_HEADROOM_BYTES)
    };
    if data.identity.0 == cron.identity.0 {
        let need = data_need
            .checked_add(cron_need)
            .ok_or_else(|| failure("maintenance_storage_insufficient"))?;
        if !enough(data_free.min(cron_free), need) {
            return Err(failure("maintenance_storage_insufficient"));
        }
    } else if !enough(data_free, data_need) || !enough(cron_free, cron_need) {
        return Err(failure("maintenance_storage_insufficient"));
    }
    Ok(())
}
fn load(record: &Option<Document>) -> Result<Schedule, Error> {
    let Some(record) = record else {
        return Ok(Schedule::default());
    };
    let schedule: Schedule = serde_json::from_slice(&record.bytes).map_err(|_| storage())?;
    if !schedule.valid() {
        return Err(storage());
    }
    Ok(schedule)
}
fn cron_line(schedule: &Schedule, script: &str) -> String {
    let hour = (schedule.time.as_bytes()[0] - b'0') * 10 + schedule.time.as_bytes()[1] - b'0';
    let minute = (schedule.time.as_bytes()[3] - b'0') * 10 + schedule.time.as_bytes()[4] - b'0';
    let days = schedule
        .weekdays
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(",");
    format!("{minute} {hour} * * {days} {script}\n")
}
fn owned_line(line: &[u8], script: &str) -> bool {
    let Ok(text) = std::str::from_utf8(line) else {
        return false;
    };
    let parts = text.trim_end_matches('\n').split(' ').collect::<Vec<_>>();
    if parts.len() != 6 || parts[2] != "*" || parts[3] != "*" || parts[5] != script {
        return false;
    }
    let number = |s: &str, limit: u8| {
        !s.is_empty()
            && s.bytes().all(|c| c.is_ascii_digit())
            && s.parse::<u8>().is_ok_and(|n| n < limit)
    };
    number(parts[0], 60)
        && number(parts[1], 24)
        && !parts[4].is_empty()
        && parts[4].split(',').count() <= 7
        && parts[4].split(',').all(|s| number(s, 7))
}
fn regenerate_cron(previous: &[u8], schedule: &Schedule, script: &str) -> Result<Vec<u8>, Error> {
    if previous.len() > MAX_CRON_BYTES {
        return Err(storage());
    }
    let lines = previous
        .split_inclusive(|c| *c == b'\n')
        .collect::<Vec<_>>();
    let marker =
        |line: &[u8], text: &str| line.strip_suffix(b"\n").unwrap_or(line) == text.as_bytes();
    let mut unrelated = Vec::with_capacity(previous.len());
    let mut i = 0;
    while i < lines.len() {
        if marker(lines[i], CRON_BEGIN) {
            // Delete only a complete block containing exactly our fixed command.
            // Malformed markers must not swallow SSH rescue or bootstrap rows.
            if i + 2 >= lines.len()
                || !owned_line(lines[i + 1], script)
                || !marker(lines[i + 2], CRON_END)
            {
                return Err(failure("maintenance_cron_conflict"));
            }
            i += 3;
        } else {
            if marker(lines[i], CRON_END) {
                return Err(failure("maintenance_cron_conflict"));
            }
            unrelated.extend_from_slice(lines[i]);
            i += 1;
        }
    }
    let block = if schedule.enabled {
        format!("{CRON_BEGIN}\n{}{CRON_END}\n", cron_line(schedule, script)).into_bytes()
    } else {
        Vec::new()
    };
    if block.len().saturating_add(unrelated.len()) > MAX_CRON_BYTES {
        return Err(storage());
    }
    let mut result = Vec::with_capacity(block.len() + unrelated.len());
    // Prepending leaves every unrelated byte intact, even a missing final LF.
    result.extend_from_slice(&block);
    result.extend_from_slice(&unrelated);
    Ok(result)
}

pub fn invoke<B: Backend>(
    handler: &str,
    input: &Value,
    data: &Path,
    io: &mut B,
    b: &Budget<'_>,
) -> Result<Value, Error> {
    invoke_with_paths(handler, input, &ConfigPaths::new(data), io, b)
}
pub fn invoke_with_paths<B: Backend>(
    handler: &str,
    input: &Value,
    paths: &ConfigPaths,
    io: &mut B,
    b: &Budget<'_>,
) -> Result<Value, Error> {
    check(b)?;
    match handler {
        "getSchedule" => {
            no_input(input)?;
            let data = Directory::open(&paths.data, true, b)?;
            Ok(load(&data.read(RECORD, MAX_SCHEDULE_BYTES, Some(0o600), b)?)?.public())
        }
        "setSchedule" => set_schedule(input_schedule(input)?, paths, io, b),
        "acceleration_status" => {
            no_input(input)?;
            acceleration(paths, io, b)
        }
        _ => Err(invalid()),
    }
}
fn set_schedule<B: Backend>(
    mut schedule: Schedule,
    paths: &ConfigPaths,
    io: &mut B,
    b: &Budget<'_>,
) -> Result<Value, Error> {
    let script_path = paths.data.join("scheduled-reboot.sh");
    if !safe_absolute(&script_path) || paths.root_crontab.file_name().is_none_or(|n| n != "root") {
        return Err(storage());
    }
    let data = Directory::open(&paths.data, true, b)?;
    let cron = Directory::open(paths.root_crontab.parent().ok_or_else(storage)?, false, b)?;
    let previous_record = data.read(RECORD, MAX_SCHEDULE_BYTES, Some(0o600), b)?;
    let current = load(&previous_record)?;
    let previous_script = data.read(SCRIPT, 128, Some(0o700), b)?;
    let previous_cron = cron.read(ROOT_CRON, MAX_CRON_BYTES, None, b)?;
    let old_cron = previous_cron
        .as_ref()
        .map_or(&[][..], |d| d.bytes.as_slice());
    let script = script_path.to_str().ok_or_else(storage)?;
    let new_cron = regenerate_cron(old_cron, &schedule, script)?;
    let cron_changed = new_cron != old_cron;
    let script_changed = schedule.enabled
        && previous_script
            .as_ref()
            .is_none_or(|d| d.bytes != REBOOT_SCRIPT);
    let restart = cron_changed || current.reload_pending;
    schedule.reload_pending = restart;
    let record_bytes = schedule.bytes()?;
    let record_changed = previous_record
        .as_ref()
        .is_none_or(|d| d.bytes != record_bytes);
    if !record_changed && !cron_changed && !script_changed && !restart {
        return Ok(schedule.public());
    }
    let mut settled = schedule.clone();
    settled.reload_pending = false;
    let settled_bytes = settled.bytes()?;
    let mut data_sizes = Vec::with_capacity(3);
    if script_changed {
        data_sizes.push(REBOOT_SCRIPT.len());
    }
    if record_changed {
        data_sizes.push(record_bytes.len());
    }
    if restart {
        data_sizes.push(settled_bytes.len());
    }
    let cron_sizes = if cron_changed {
        vec![new_cron.len()]
    } else {
        Vec::new()
    };
    admit(&data, &data_sizes, &cron, &cron_sizes, b)?;
    let script_stage = if script_changed {
        Some(data.stage(SCRIPT_STAGE, SCRIPT, REBOOT_SCRIPT, 0o700, b)?)
    } else {
        None
    };
    let record_stage = if record_changed {
        Some(data.stage(RECORD_STAGE, RECORD, &record_bytes, 0o600, b)?)
    } else {
        None
    };
    let settled_stage = if restart {
        Some(data.stage(SETTLED_STAGE, RECORD, &settled_bytes, 0o600, b)?)
    } else {
        None
    };
    let cron_mode = previous_cron.as_ref().map_or(0o600, |d| d.mode);
    let cron_stage = if cron_changed {
        Some(cron.stage(CRON_STAGE, ROOT_CRON, &new_cron, cron_mode, b)?)
    } else {
        None
    };
    // All validation, parsing, modes, capacity admission and file fsyncs precede
    // the authoritative desired-state commit and the native service action.
    if let Some(staged) = script_stage
        && !staged.publish(&previous_script, 128, Some(0o700), b)?
    {
        return Err(storage());
    }
    if let Some(staged) = record_stage
        && !staged.publish(&previous_record, MAX_SCHEDULE_BYTES, Some(0o600), b)?
    {
        return Err(failure("schedule_saved_durability_unconfirmed"));
    }
    let committed_record = data
        .read(RECORD, MAX_SCHEDULE_BYTES, Some(0o600), b)
        .map_err(saved)?;
    if let Some(staged) = cron_stage
        && !staged
            .publish(&previous_cron, MAX_CRON_BYTES, None, b)
            .map_err(saved)?
    {
        return Err(failure("schedule_saved_durability_unconfirmed"));
    }
    if restart {
        check(b).map_err(saved)?;
        // RN02 /etc/init.d/cron has no custom reload; rc.common reload would
        // restart it. Use the explicit existing service restart, never a daemon.
        let out = io
            .run(
                Program::Service,
                &["cron".into(), "restart".into()],
                None,
                4096,
                b,
            )
            .map_err(|_| failure("schedule_saved_reload_failed"))?;
        if out.code != 0 {
            return Err(failure("schedule_saved_reload_failed"));
        }
        if let Some(staged) = settled_stage
            && !staged
                .publish(&committed_record, MAX_SCHEDULE_BYTES, Some(0o600), b)
                .map_err(saved)?
        {
            return Err(failure("schedule_saved_durability_unconfirmed"));
        }
    }
    Ok(settled.public())
}

fn observe<T>(result: Result<T, IoError>, b: &Budget<'_>) -> Result<Option<T>, Error> {
    match result {
        Ok(value) => {
            check(b)?;
            Ok(Some(value))
        }
        Err(IoError::Deadline) => Err(failure("operation_timeout")),
        Err(IoError::Cancelled) => Err(failure("operation_cancelled")),
        Err(_) => {
            check(b)?;
            Ok(None)
        }
    }
}
fn counter<B: Backend>(
    io: &mut B,
    root: &Path,
    name: &str,
    b: &Budget<'_>,
) -> Result<Option<u64>, Error> {
    let Some(raw) = observe(io.read(&root.join(name), 32, b), b)? else {
        return Ok(None);
    };
    // Only decimal uint32 driver scalars are projected. Debug text, free-form
    // frontend values and credentials never reach the public DTO.
    let Ok(text) = std::str::from_utf8(&raw) else {
        return Ok(None);
    };
    let text = text.trim();
    if text.is_empty() || text.len() > 10 || !text.bytes().all(|c| c.is_ascii_digit()) {
        return Ok(None);
    }
    Ok(text.parse::<u32>().ok().map(u64::from))
}
fn acceleration<B: Backend>(
    paths: &ConfigPaths,
    io: &mut B,
    b: &Budget<'_>,
) -> Result<Value, Error> {
    if !safe_absolute(&paths.ecm_debugfs) || !safe_absolute(&paths.modules) {
        return Err(storage());
    }
    let modules = observe(io.list(&paths.modules, 4096, b), b)?;
    let module = |names: &[&str]| {
        modules
            .as_ref()
            .map(|entries| entries.iter().any(|entry| names.contains(&entry.as_str())))
    };
    let ecm = module(&["ecm"]);
    let sfe = module(&["sfe", "qca_nss_sfe"]);
    let ppe = module(&["ppe", "qca_nss_ppe"]);
    let entries = observe(io.list(&paths.ecm_debugfs, 128, b), b)?;
    let mut observed = Vec::with_capacity(4);
    for (engine, directory) in [
        ("sfe", "ecm_sfe_ipv4"),
        ("sfe", "ecm_sfe_ipv6"),
        ("ppe", "ecm_ppe_ipv4"),
        ("ppe", "ecm_ppe_ipv6"),
    ] {
        let accelerated = counter(
            io,
            &paths.ecm_debugfs,
            &format!("{directory}/accelerated_count"),
            b,
        )?;
        let pending = counter(
            io,
            &paths.ecm_debugfs,
            &format!("{directory}/pending_accel_count"),
            b,
        )?;
        let present = entries
            .as_ref()
            .is_some_and(|e| e.iter().any(|n| n == directory))
            || accelerated.is_some()
            || pending.is_some();
        if present {
            observed.push((engine, accelerated, pending));
        }
    }
    let sfe_frontend = observed.iter().any(|(e, _, _)| *e == "sfe");
    let ppe_frontend = observed.iter().any(|(e, _, _)| *e == "ppe");
    let frontend = match (sfe_frontend, ppe_frontend) {
        (true, true) => "ppe,sfe",
        (true, false) => "sfe",
        (false, true) => "ppe",
        _ => "unknown",
    };
    let sfe_active = observed
        .iter()
        .any(|(e, n, _)| *e == "sfe" && n.is_some_and(|n| n > 0));
    let ppe_active = observed
        .iter()
        .any(|(e, n, _)| *e == "ppe" && n.is_some_and(|n| n > 0));
    let complete =
        entries.is_some() && !observed.is_empty() && observed.iter().all(|(_, n, _)| n.is_some());
    let engine = match (sfe_active, ppe_active) {
        (true, true) => "ecm",
        (true, false) => "sfe",
        (false, true) => "ppe",
        _ => "unknown",
    };
    let state = if sfe_active || ppe_active {
        "active"
    } else if complete {
        "inactive"
    } else {
        "unknown"
    };
    let mut counters = serde_json::Map::new();
    if complete {
        counters.insert(
            "accelerated".into(),
            json!(observed.iter().map(|(_, n, _)| n.unwrap_or(0)).sum::<u64>()),
        );
    }
    if !observed.is_empty() && observed.iter().all(|(_, _, n)| n.is_some()) {
        counters.insert(
            "pending".into(),
            json!(observed.iter().map(|(_, _, n)| n.unwrap_or(0)).sum::<u64>()),
        );
    }
    if let Some(n) = counter(io, &paths.ecm_debugfs, "ecm_db/connection_count", b)? {
        counters.insert("connections".into(), json!(n));
    }
    let mut result = json!({"engine": engine, "state": state, "frontend": frontend,
        "ecm": ecm, "sfe": sfe, "ppe": ppe});
    if !counters.is_empty() {
        result["counters"] = Value::Object(counters);
    }
    Ok(result)
}

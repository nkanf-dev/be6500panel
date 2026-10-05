//! Full router feature catalog and finite, caller-driven native operations.
use crate::{
    features::{self, Action, Domain, Error, Field, FieldKind, Impact, Read},
    product_io::{Backend, Program, timestamp},
    readiness_tun::Budget,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    fs::{self, File, OpenOptions},
    io::{Read as IoRead, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
const MAX_INPUT: usize = 64 << 10;
const MAX_OPERATIONS: usize = 32;
fn domains() -> [&'static Domain; 3] {
    [
        &crate::features_network::DOMAIN,
        &crate::features_wireless::DOMAIN,
        &crate::features_services::DOMAIN,
    ]
}
fn domain(id: &str) -> Result<&'static Domain, Error> {
    domains().into_iter().find(|d| d.id == id).ok_or(Error {
        status: 404,
        code: "feature_not_found",
        message: "功能不存在。",
    })
}
fn project(d: &str, id: &str, v: Value) -> Result<Value, Error> {
    match d {
        "network" => crate::features_network::project(id, v),
        "wireless" => crate::features_wireless::project(id, v),
        "services" => crate::features_services::project(id, v),
        _ => Err(features::invalid()),
    }
}
fn validate(d: &str, id: &str, v: &Value) -> Result<(), Error> {
    match d {
        "network" => crate::features_network::validate(id, v),
        "wireless" => crate::features_wireless::validate(id, v),
        "services" => crate::features_services::validate(id, v),
        _ => Err(features::invalid()),
    }
}
fn verified(d: &str, id: &str, input: &Value, after: &Value) -> bool {
    match d {
        "network" => crate::features_network::verify(id, input, after),
        "wireless" => crate::features_wireless::verify(id, input, after),
        "services" => crate::features_services::verify(id, input, after),
        _ => false,
    }
}

fn query_value(value: &str) -> Result<String, Error> {
    let value = value.replace('+', " ");
    let bytes = value.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            if at + 2 >= bytes.len() {
                return Err(features::invalid());
            }
            let word =
                std::str::from_utf8(&bytes[at + 1..at + 3]).map_err(|_| features::invalid())?;
            result.push(u8::from_str_radix(word, 16).map_err(|_| features::invalid())?);
            at += 3;
        } else {
            result.push(bytes[at]);
            at += 1;
        }
    }
    let text = String::from_utf8(result).map_err(|_| features::invalid())?;
    if text.len() > 4096 || text.chars().any(|c| c.is_control()) {
        return Err(features::invalid());
    }
    Ok(text)
}
fn read_parameters(read: &Read, input: &Value) -> Value {
    // macUnbind accepts a comma-list, but the optional getter filter accepts one MAC.
    // Read the complete binding table so absence can be proved for every MAC.
    if read.id == "macbind_info" {
        return json!({});
    }
    let mut values = serde_json::Map::new();
    for f in read.fields {
        if let Some(v) = input.get(f.key) {
            values.insert(f.key.into(), v.clone());
        }
    }
    if read.fields.iter().any(|f| f.key == "user_id")
        && !values.contains_key("user_id")
        && let Some(ops) = input.get("opt_list")
    {
        let decoded = if let Some(s) = ops.as_str() {
            serde_json::from_str::<Value>(s).ok()
        } else {
            Some(ops.clone())
        };
        if let Some(id) = decoded
            .as_ref()
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|o| o.get("user_id"))
        {
            values.insert("user_id".into(), id.clone());
        }
    }
    Value::Object(values)
}
fn prepare(d: &str, id: &str, input: Value, current: &Value) -> Result<Value, Error> {
    match d {
        "network" => crate::features_network::prepare_input(id, input, current),
        "wireless" => crate::features_wireless::prepare_input(id, input, current),
        "services" => crate::features_services::prepare_input(id, input, current),
        _ => Err(features::invalid()),
    }
}

fn check(b: &Budget<'_>) -> Result<(), Error> {
    b.check().map_err(|_| Error {
        status: 504,
        code: "operation_timeout",
        message: "操作超时。",
    })
}
fn fields_valid(fields: &[Field], input: &Value) -> Result<(), Error> {
    let object = input.as_object().ok_or_else(features::invalid)?;
    if object.len() > 64
        || object
            .keys()
            .any(|key| !fields.iter().any(|f| f.key == key))
    {
        return Err(features::invalid());
    }
    for f in fields {
        let Some(value) = object.get(f.key) else {
            if f.required {
                return Err(features::invalid());
            } else {
                continue;
            }
        };
        let valid = match f.kind {
            FieldKind::Boolean => {
                value.is_boolean() || value.as_str().is_some_and(|v| matches!(v, "0" | "1"))
            }
            FieldKind::Integer => value
                .as_i64()
                .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
                .is_some_and(|n| {
                    f.min.is_none_or(|min| n >= min) && f.max.is_none_or(|max| n <= max)
                }),
            FieldKind::Select => value.as_str().is_some_and(|s| f.options.contains(&s)),
            FieldKind::Ipv4 => value.as_str().is_some_and(|s| {
                s.is_empty() && !f.required || s.parse::<std::net::Ipv4Addr>().is_ok()
            }),
            FieldKind::Ipv6 => value.as_str().is_some_and(|s| {
                s.is_empty() && !f.required || s.parse::<std::net::Ipv6Addr>().is_ok()
            }),
            FieldKind::Mac => value.as_str().is_some_and(|s| {
                s.len() == 17
                    && s.split(':').count() == 6
                    && s.split(':')
                        .all(|part| part.len() == 2 && part.bytes().all(|b| b.is_ascii_hexdigit()))
            }),
            FieldKind::Text | FieldKind::Secret => value.as_str().is_some_and(|s| {
                s.len() <= 4096
                    && !s.chars().any(|c| c.is_control())
                    && (!f.required || !s.is_empty())
            }),
            FieldKind::Json => serde_json::to_vec(value).is_ok_and(|v| v.len() <= 32 << 10),
        };
        if !valid {
            return Err(features::invalid());
        }
    }
    Ok(())
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Apply {
    action_id: String,
    input: Value,
    generation: u64,
    #[serde(default)]
    acknowledge_impact: bool,
}
fn decode_apply(raw: &[u8]) -> Result<Apply, Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields, rename_all = "camelCase")]
    struct Input {
        action_id: String,
        #[serde(deserialize_with = "unique_input")]
        input: Value,
        generation: u64,
        #[serde(default)]
        acknowledge_impact: bool,
    }
    fn unique_input<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Value, D::Error> {
        struct Map;
        impl<'de> serde::de::Visitor<'de> for Map {
            type Value = Value;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("unique bounded feature input")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(self, mut m: M) -> Result<Value, M::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = m.next_key::<String>()? {
                    if values.len() >= 64 || values.contains_key(&key) {
                        return Err(serde::de::Error::custom("invalid input fields"));
                    }
                    values.insert(key, m.next_value()?);
                }
                Ok(Value::Object(values))
            }
        }
        d.deserialize_map(Map)
    }
    let i: Input = serde_json::from_slice(raw).map_err(|_| features::invalid())?;
    Ok(Apply {
        action_id: i.action_id,
        input: i.input,
        generation: i.generation,
        acknowledge_impact: i.acknowledge_impact,
    })
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Operation {
    id: String,
    domain: String,
    action_id: String,
    state: String,
    generation: u64,
    started_at: String,
    updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reconnect_address: Option<String>,
    #[serde(default)]
    can_confirm: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Reconnect {
    boot_id: String,
    target_ip: Option<String>,
    target_mask: Option<String>,
    previous_version: Option<String>,
    expected_mode: Option<String>,
    destructive: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Task {
    operation: Operation,
    phase: String,
    reserved_generation: u64,
    native_attempted: bool,
    deadline_unix: u64,
    reconnect: Option<Reconnect>,
}
struct Pending {
    task: Task,
    // Prepared secrets exist only on the caller-owned lane, never on disk.
    input: Value,
    action: &'static Action,
    next: Instant,
    recovery: Option<crate::features_recovery::Recovery>,
    sequence_target: Option<u64>,
}
pub struct Features {
    data: PathBuf,
    config_dir: PathBuf,
    device_db: PathBuf,
    generation: u64,
    load_error: Option<Error>,
    startup_ready: bool,
    startup_error: Option<Error>,
    startup_next: Instant,
    operations: VecDeque<Operation>,
    pending: Option<Pending>,
    cache: BTreeMap<String, (Instant, Value)>,
    catalog_metadata: Value,
    management_listener: Option<std::net::SocketAddr>,
    // Only a pending tick can publish a native + br-lan kernel proven target.
    rebind_address: Option<std::net::SocketAddr>,
}
const MAX_TASK_BYTES: usize = 64 << 10;
fn busy_error() -> Error {
    Error {
        status: 409,
        code: "operation_busy",
        message: "已有操作正在生效或恢复，请稍候。",
    }
}
fn reconnect_action(id: &str) -> bool {
    matches!(
        id,
        "set_lan_ip"
            | "set_lan_ap"
            | "disable_lan_ap"
            | "set_wifi_ap"
            | "disable_wifi_ap"
            | "reboot"
            | "factory_reset"
            | "official_upgrade"
    )
}
fn successful(v: &Value) -> bool {
    v.get("code").and_then(Value::as_i64) == Some(0)
}
fn private_verified(d: &str, id: &str, input: &Value, native: &Value) -> bool {
    match d {
        "network" => crate::features_network::verify_private(id, input, native),
        "wireless" => crate::features_wireless::verify_private(id, input, native),
        "services" => crate::features_services::verify_private(id, input, native),
        _ => false,
    }
}
/// This is only root-owned generation/task storage. Recovery owns its own journal.
/// Read and write through no-follow descriptors; never reset a bad record.
fn private_dir(path: &Path, create: bool) -> Result<File, Error> {
    if !path.is_absolute() {
        return Err(storage());
    }
    let mut parent = PathBuf::from("/");
    for c in path.components().skip(1) {
        let std::path::Component::Normal(name) = c else {
            return Err(storage());
        };
        parent.push(name);
        match fs::symlink_metadata(&parent) {
            Err(e) if create && e.kind() == std::io::ErrorKind::NotFound => {
                fs::DirBuilder::new()
                    .mode(0o700)
                    .create(&parent)
                    .map_err(|_| storage())?;
            }
            Err(_) => return Err(storage()),
            Ok(m) if !m.is_dir() || m.mode() & 0o022 != 0 && m.mode() & 0o1000 == 0 => {
                return Err(storage());
            }
            _ => {}
        }
    }
    let dir = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| storage())?;
    let m = dir.metadata().map_err(|_| storage())?;
    if m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o7777 != 0o700 {
        return Err(storage());
    }
    Ok(dir)
}
use std::os::{
    fd::{AsRawFd, FromRawFd},
    unix::fs::DirBuilderExt,
};
fn private_read(root: &Path, name: &str, limit: usize) -> Result<Option<Vec<u8>>, Error> {
    if fs::symlink_metadata(root).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound) {
        return Ok(None);
    }
    let dir = private_dir(root, false)?;
    let name = std::ffi::CString::new(name).map_err(|_| storage())?;
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return if std::io::Error::last_os_error().kind() == std::io::ErrorKind::NotFound {
            Ok(None)
        } else {
            Err(storage())
        };
    }
    let file = unsafe { File::from_raw_fd(fd) };
    let m = file.metadata().map_err(|_| storage())?;
    if !m.is_file()
        || m.nlink() != 1
        || m.uid() != unsafe { libc::geteuid() }
        || m.mode() & 0o7777 != 0o600
        || m.len() > limit as u64
    {
        return Err(storage());
    }
    let mut raw = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut raw)
        .map_err(|_| storage())?;
    if raw.len() > limit {
        return Err(storage());
    }
    Ok(Some(raw))
}
/// Returns false only AFTER rename: published content is authoritative even if
/// directory fsync failed. Callers must retain the operation and retry durability.
fn private_write(root: &Path, name: &str, bytes: &[u8]) -> Result<bool, Error> {
    if bytes.len() > MAX_TASK_BYTES {
        return Err(storage());
    }
    let dir = private_dir(root, true)?;
    let _ = private_read(root, name, MAX_TASK_BYTES)?;
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    if unsafe { libc::fstatvfs(dir.as_raw_fd(), stat.as_mut_ptr()) } != 0 {
        return Err(storage());
    }
    let stat = unsafe { stat.assume_init() };
    #[allow(clippy::unnecessary_cast)]
    let free = (stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64);
    if free < (1 << 20) + (bytes.len() as u64).div_ceil(4096) * 4096 + 4096 {
        return Err(storage());
    }
    let mut random = [0u8; 12];
    getrandom::fill(&mut random).map_err(|_| storage())?;
    let temp = std::ffi::CString::new(format!(
        ".feature-{}",
        random
            .iter()
            .map(|v| format!("{v:02x}"))
            .collect::<String>()
    ))
    .map_err(|_| storage())?;
    let target = std::ffi::CString::new(name).map_err(|_| storage())?;
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            temp.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600 as libc::c_uint,
        )
    };
    if fd < 0 {
        return Err(storage());
    }
    let result = (|| {
        let mut file = unsafe { File::from_raw_fd(fd) };
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|_| storage())?;
        file.write_all(bytes).map_err(|_| storage())?;
        file.sync_all().map_err(|_| storage())?;
        let _ = private_read(root, name, MAX_TASK_BYTES)?;
        let fresh = private_dir(root, false)?
            .metadata()
            .map_err(|_| storage())?;
        let original = dir.metadata().map_err(|_| storage())?;
        if (fresh.dev(), fresh.ino()) != (original.dev(), original.ino()) {
            return Err(storage());
        }
        if unsafe {
            libc::renameat(
                dir.as_raw_fd(),
                temp.as_ptr(),
                dir.as_raw_fd(),
                target.as_ptr(),
            )
        } != 0
        {
            return Err(storage());
        }
        Ok(dir.sync_all().is_ok())
    })();
    unsafe {
        libc::unlinkat(dir.as_raw_fd(), temp.as_ptr(), 0);
    }
    result
}
impl Features {
    pub fn open(data: &Path) -> Self {
        // RN02 binds /data/etc/config over /etc/config and links xqDb into
        // /data/etc. Bind the fixed persistent native roots, not a client path.
        let persistent = Path::new("/data/etc/config");
        if persistent.is_dir() {
            Self::open_with_paths(data, persistent, Path::new("/data/etc/xqDb"))
        } else {
            Self::open_with_paths(data, Path::new("/etc/config"), Path::new("/etc/xqDb"))
        }
    }
    /// Trusted fixture/startup paths, never client input.
    pub fn open_with_paths(data: &Path, config_dir: &Path, device_db: &Path) -> Self {
        let root = data.join("feature-operations");
        let mut load_error = None;
        let generation = match private_read(&root, "generation", 32) {
            Ok(None) => 1,
            Ok(Some(raw)) => match std::str::from_utf8(&raw)
                .ok()
                .and_then(|s| s.trim().parse::<u64>().ok())
                .filter(|n| *n > 0)
            {
                Some(n) => n,
                None => {
                    load_error = Some(storage());
                    0
                }
            },
            Err(e) => {
                load_error = Some(e);
                0
            }
        };
        let mut operations = VecDeque::new();
        let mut pending = None;
        match private_read(&root, "current.json", MAX_TASK_BYTES) {
            Ok(Some(raw)) => match serde_json::from_slice::<Task>(&raw) {
                Ok(task) => {
                    let action = domain(&task.operation.domain)
                        .ok()
                        .and_then(|d| d.actions.iter().find(|a| a.id == task.operation.action_id));
                    let valid = action.is_some()
                        && task.operation.id.len() <= 64
                        && task
                            .operation
                            .id
                            .bytes()
                            .all(|b| b.is_ascii_digit() || b == b'-')
                        && task.operation.generation <= task.reserved_generation
                        && task.reserved_generation <= generation.saturating_add(1)
                        && task.reserved_generation >= generation
                        && ["admitted", "pending", "restoring", "completing", "settled"]
                            .contains(&task.phase.as_str())
                        && task.reconnect.as_ref().is_none_or(|r| {
                            reconnect_action(&task.operation.action_id)
                                && r.boot_id.len() == 36
                                && r.target_ip.as_ref().is_none_or(|ip| safe_ip(ip).is_some())
                                && r.target_mask
                                    .as_ref()
                                    .is_none_or(|ip| ip.parse::<std::net::Ipv4Addr>().is_ok())
                                && r.previous_version.as_ref().is_none_or(|s| s.len() <= 128)
                                && r.expected_mode.as_ref().is_none_or(|s| {
                                    ["lanapmode", "wifiapmode", "router"].contains(&s.as_str())
                                })
                        });
                    if !valid {
                        load_error = Some(storage());
                    } else if task.phase == "settled" {
                        operations.push_front(task.operation);
                    } else {
                        pending = Some(Pending {
                            task,
                            input: json!({}),
                            action: action.unwrap(),
                            next: Instant::now(),
                            recovery: None,
                            sequence_target: None,
                        });
                    }
                }
                Err(_) => load_error = Some(storage()),
            },
            Ok(None) => {}
            Err(e) => load_error = Some(e),
        }
        let catalog_metadata = json!({"domains":domains().iter().map(|d|json!({"id":d.id,"title":d.title,
            "reads":d.reads.iter().map(|r|json!({"id":r.id,"title":r.title,"fields":r.fields})).collect::<Vec<_>>(),
            "actions":d.actions.iter().map(|a|json!({"id":a.id,"title":a.title,"fields":a.fields,
                "impact":a.impact,"configs":a.configs,"readback":a.readback})).collect::<Vec<_>>()})).collect::<Vec<_>>()});
        Self {
            data: root,
            config_dir: config_dir.into(),
            device_db: device_db.into(),
            generation,
            load_error,
            startup_ready: false,
            startup_error: None,
            startup_next: Instant::now(),
            operations,
            pending,
            cache: BTreeMap::new(),
            catalog_metadata,
            management_listener: None,
            rebind_address: None,
        }
    }
    /// Trusted, already-bound startup listener. Never set from client headers.
    pub fn set_management_listener(&mut self, address: std::net::SocketAddr) {
        self.management_listener = Some(address);
        self.rebind_address = None;
    }
    /// Cached pending-tick proof, not a client-selected address or a GET collector.
    pub fn management_rebind_address(&self) -> Option<std::net::SocketAddr> {
        self.rebind_address
    }
    fn save(&self, task: &Task) -> Result<(), Error> {
        let mut metadata = task.clone();
        // Persist only the bounded current lifecycle, not payloads or read history.
        metadata.operation.data = None;
        let raw = serde_json::to_vec(&metadata).map_err(|_| storage())?;
        if private_write(&self.data, "current.json", &raw)? {
            Ok(())
        } else {
            Err(storage())
        }
    }
    fn sequence(&mut self, target: u64) -> Result<(), Error> {
        let durable = private_write(&self.data, "generation", target.to_string().as_bytes())?;
        // Atomic rename has committed, even when the following fsync failed.
        self.generation = target;
        if durable { Ok(()) } else { Err(storage()) }
    }
    fn invoke<B: Backend>(
        &self,
        io: &mut B,
        controller: &str,
        handler: &str,
        input: &Value,
        b: &Budget<'_>,
    ) -> Result<Value, Error> {
        if controller == "maintenance" {
            let mut reply = crate::features_maintenance::invoke(
                handler,
                input,
                self.data.parent().ok_or_else(storage)?,
                io,
                b,
            )?;
            reply["code"] = json!(0);
            Ok(reply)
        } else {
            features::invoke(io, controller, handler, input, b)
        }
    }
    fn restore<B: Backend>(
        &self,
        recovery: &mut crate::features_recovery::Recovery,
        io: &mut B,
        action_id: &str,
    ) -> Result<(), Error> {
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let b = Budget {
            deadline: Instant::now() + Duration::from_secs(45),
            cancel: &cancel,
        };
        let configs = recovery.restore(io, &b)?;
        Self::reload(&configs, Some(action_id), io, &b)?;
        recovery.verify_restored(io, &b)
    }
    fn recover<B: Backend>(&self, io: &mut B, action_id: Option<&str>) -> Result<(), Error> {
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let b = Budget {
            deadline: Instant::now() + Duration::from_secs(45),
            cancel: &cancel,
        };
        let configs = crate::features_recovery::Recovery::recover_with_paths(
            &self.data,
            &self.config_dir,
            &self.device_db,
            io,
            &b,
        )?;
        Self::reload(&configs, action_id, io, &b)?;
        crate::features_recovery::Recovery::verify_recovered_with_paths(
            &self.data,
            &self.config_dir,
            &self.device_db,
            io,
            &b,
        )?;
        crate::features_recovery::Recovery::discard_recovered(&self.data)
    }
    fn reload_failed() -> Error {
        Error {
            status: 503,
            code: "restore_reload_failed",
            message: "配置已恢复，服务重新载入需要重试。",
        }
    }
    fn reload_call<B: Backend>(
        io: &mut B,
        program: Program,
        args: &[String],
        b: &Budget<'_>,
    ) -> Result<(), Error> {
        io.run(program, args, None, 64 << 10, b)
            .is_ok_and(|out| out.code == 0)
            .then_some(())
            .ok_or_else(Self::reload_failed)
    }
    fn restored_scalar<B: Backend>(
        io: &mut B,
        key: &str,
        default: &str,
        b: &Budget<'_>,
    ) -> Result<String, Error> {
        let out = io
            .run(
                Program::Uci,
                &["-q".into(), "get".into(), key.into()],
                None,
                64,
                b,
            )
            .map_err(|_| Self::reload_failed())?;
        if out.code == 1 && out.stdout.is_empty() {
            return Ok(default.into()); // Current native getters' absent-option default.
        }
        if out.code != 0 {
            return Err(Self::reload_failed());
        }
        let value = std::str::from_utf8(&out.stdout)
            .map_err(|_| Self::reload_failed())?
            .trim();
        if value.is_empty() {
            return Err(Self::reload_failed());
        }
        Ok(value.into())
    }
    fn reload_led<B: Backend>(
        io: &mut B,
        option: &str,
        target: Option<&str>,
        timer: bool,
        b: &Budget<'_>,
    ) -> Result<(), Error> {
        let value = Self::restored_scalar(io, &format!("xiaoqiang.common.{option}"), "1", b)?;
        let verb = match value.as_str() {
            "0" => "led_off",
            "1" => "led_on",
            _ => return Err(Self::reload_failed()),
        };
        let mut args = vec!["led_ctl".into(), verb.into()];
        args.extend(target.map(str::to_owned));
        Self::reload_call(io, Program::Service, &args, b)?;
        if !timer {
            return Ok(());
        }
        let enabled =
            Self::restored_scalar(io, &format!("xiaoqiang.common.{option}_TIMER"), "0", b)?;
        let mut args = vec!["led_ctl".into()];
        match enabled.as_str() {
            "0" => args.push("timer_off".into()),
            "1" => {
                args.push("timer_on".into());
                for suffix in ["OPEN", "CLOSE"] {
                    let time = Self::restored_scalar(
                        io,
                        &format!("xiaoqiang.common.{option}_TIMER_{suffix}"),
                        "00:00",
                        b,
                    )?;
                    let bytes = time.as_bytes();
                    if bytes.len() != 5
                        || bytes[2] != b':'
                        || ![bytes[0], bytes[1], bytes[3], bytes[4]]
                            .iter()
                            .all(u8::is_ascii_digit)
                    {
                        return Err(Self::reload_failed());
                    }
                    let hour = (bytes[0] - b'0') * 10 + bytes[1] - b'0';
                    let minute = (bytes[3] - b'0') * 10 + bytes[4] - b'0';
                    if hour > 23 || minute > 59 {
                        return Err(Self::reload_failed());
                    }
                    args.extend([time[..2].into(), time[3..].into()]);
                }
            }
            _ => return Err(Self::reload_failed()),
        }
        args.extend(target.map(str::to_owned));
        Self::reload_call(io, Program::Service, &args, b)
    }
    fn reload<B: Backend>(
        configs: &[String],
        action_id: Option<&str>,
        io: &mut B,
        b: &Budget<'_>,
    ) -> Result<(), Error> {
        let mut needed = std::collections::BTreeSet::new();
        for c in configs {
            let services: &[&str] = match c.as_str() {
                "network" | "ipv6" => &["network"],
                "port_service" | "port_map" => &["port_service", "network"],
                "mwan3" => &["mwan3"],
                "wireless" | "misc" => &["wifi"],
                "dhcp" | "macbind" | "devicelist" => &["dnsmasq"],
                "firewall" | "firewall_cpp" => &["firewall"],
                "macfilter" | "wifiblist" | "wifiwlist" => &["wifi", "firewall"],
                "ddns" => &["ddns"],
                "miqos" | "hwnat" => &["miqos"],
                "upnpd" => &["miniupnpd"],
                "nginx" => &["nginx"],
                "local_gw_security" => &["firewall"],
                "system" => &["system"],
                // LuCI reads webfilter on each request; backup stores mode history.
                // xiaoqiang consumers are action-specific, not a network restart.
                "webfilter" | "mipctl_user" | "miscan" | "xiaoqiang" | "backup" => &[],
                "otapred" => &[], // Persisted policy read on the next official OTA check.
                "parentalctl" => &[], // Fixed native apply below, not a network reload.
                _ => return Err(storage()),
            };
            needed.extend(services.iter().copied());
        }
        let mut failed = false;
        // Port-service's native helper precedes the generated network consumers.
        for service in [
            "port_service",
            "network",
            "wifi",
            "dnsmasq",
            "firewall",
            "mwan3",
            "ddns",
            "miqos",
            "miniupnpd",
            "nginx",
            "system",
        ] {
            let verb = if service == "port_service" {
                "restart"
            } else {
                "reload"
            };
            if needed.contains(service)
                && Self::reload_call(io, Program::Service, &[service.into(), verb.into()], b)
                    .is_err()
            {
                failed = true;
            }
        }
        if configs.iter().any(|c| c == "miscan") {
            let result =
                Self::restored_scalar(io, "miscan.config.enabled", "0", b).and_then(|enabled| {
                    let verb = match enabled.as_str() {
                        "0" => "stop",
                        "1" => "start",
                        _ => return Err(Self::reload_failed()),
                    };
                    Self::reload_call(io, Program::Service, &["scan".into(), verb.into()], b)
                });
            failed |= result.is_err();
        }
        if configs.iter().any(|c| c == "parentalctl") {
            failed |= Self::reload_call(
                io,
                Program::Service,
                &["parentalctl".into(), "apply".into()],
                b,
            )
            .is_err();
        }
        if configs.iter().any(|c| c == "mipctl_user") {
            // XQParentControlV2.set notifies UCI's actual native config consumer.
            failed |= Self::reload_call(
                io,
                Program::Ubus,
                &[
                    "call".into(),
                    "uci".into(),
                    "commit".into(),
                    r#"{"config":"mipctl_user"}"#.into(),
                ],
                b,
            )
            .is_err();
        }
        if configs.iter().any(|c| c == "xiaoqiang") {
            if action_id.is_none() || action_id == Some("router_name") {
                // Current RN02 trafficd has no reload method. Names are read
                // from native configuration; topology's actual consumer is MQTT.
                failed |= Self::reload_call(
                    io,
                    Program::Ubus,
                    &[
                        "call".into(),
                        "xq_info_sync_mqtt".into(),
                        "topo_changed".into(),
                        "{}".into(),
                    ],
                    b,
                )
                .is_err();
            }
            // Direct finite primitives avoid setters that skip an unchanged saved value.
            if action_id.is_none() || matches!(action_id, Some("led_set" | "all_led_set")) {
                failed |=
                    Self::reload_led(io, "BLUE_LED", None, action_id != Some("all_led_set"), b)
                        .is_err();
            }
            if action_id.is_none() || matches!(action_id, Some("eth_led_set" | "all_led_set")) {
                failed |= Self::reload_led(
                    io,
                    "ETHLED",
                    Some("ethled"),
                    action_id != Some("all_led_set"),
                    b,
                )
                .is_err();
            }
            if action_id.is_none() || action_id == Some("all_led_set") {
                failed |=
                    Self::reload_led(io, "XLED", Some("xled"), action_id.is_none(), b).is_err();
            }
        }
        if failed {
            Err(Self::reload_failed())
        } else {
            Ok(())
        }
    }
    pub fn busy(&self) -> bool {
        !self.startup_ready || self.pending.is_some() || self.load_error.is_some()
    }
    pub fn catalog(&self) -> Value {
        let mut catalog = self.catalog_metadata.clone();
        catalog["generation"] = json!(self.generation);
        catalog["writeReady"] = json!(!self.busy());
        // Public lifecycle only. Never expose prepared input or private reconnect proof.
        if let Some(pending) = &self.pending {
            catalog["pendingOperation"] = json!(&pending.task.operation);
        }
        if let Some(e) = self.load_error.or(self.startup_error) {
            catalog["writeError"] = json!(e.code);
        }
        catalog
    }
    fn read<B: Backend>(
        &mut self,
        d: &Domain,
        r: &Read,
        input: &Value,
        io: &mut B,
        b: &Budget<'_>,
    ) -> Result<Value, Error> {
        fields_valid(r.fields, input)?;
        let key = format!(
            "{}:{}:{}",
            d.id,
            r.id,
            serde_json::to_string(input).map_err(|_| features::invalid())?
        );
        if let Some((at, value)) = self.cache.get(&key)
            && at.elapsed() < Duration::from_secs(5)
        {
            check(b)?;
            return Ok(value.clone());
        }
        let native = self.invoke(io, r.controller, r.handler, input, b)?;
        let mut public = project(d.id, r.id, native)?;
        features::public_secrets(&mut public);
        if serde_json::to_vec(&public)
            .map_err(|_| features::invalid())?
            .len()
            > MAX_INPUT
        {
            return Err(features::invalid());
        }
        if self.cache.len() >= 32
            && let Some(oldest) = self
                .cache
                .iter()
                .min_by_key(|(_, (at, _))| *at)
                .map(|(key, _)| key.clone())
        {
            self.cache.remove(&oldest);
        }
        self.cache.insert(key, (Instant::now(), public.clone()));
        Ok(public)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn handle<B: Backend>(
        &mut self,
        path: &str,
        method: crate::http::Method,
        query: &str,
        body: &[u8],
        io: &mut B,
        b: &Budget<'_>,
        before: &mut impl FnMut(Impact) -> Result<(), Error>,
    ) -> Result<Value, Error> {
        check(b)?;
        if body.len() > MAX_INPUT || query.len() > MAX_INPUT {
            return Err(features::invalid());
        }
        if path == "/api/features/catalog" && method == crate::http::Method::Get {
            return Ok(self.catalog());
        }
        if path == "/api/features/operations" && method == crate::http::Method::Get {
            let id = query
                .strip_prefix("id=")
                .map(query_value)
                .transpose()?
                .ok_or_else(features::invalid)?;
            if id.is_empty() || id.len() > 64 || id.contains('&') {
                return Err(features::invalid());
            }
            let op = self
                .pending
                .as_ref()
                .map(|p| &p.task.operation)
                .filter(|o| o.id == id)
                .or_else(|| self.operations.iter().find(|o| o.id == id))
                .ok_or(Error {
                    status: 404,
                    code: "operation_not_found",
                    message: "操作记录不存在。",
                })?;
            return Ok(json!({"operation":op}));
        }
        if path == "/api/features/confirm" && method == crate::http::Method::Post {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Confirm {
                id: String,
            }
            let confirm: Confirm = serde_json::from_slice(body).map_err(|_| features::invalid())?;
            return self.confirm(&confirm.id, io, b);
        }
        let rest = path
            .strip_prefix("/api/features/")
            .ok_or_else(features::invalid)?;
        let (id, tail) = rest.split_once('/').ok_or_else(features::invalid)?;
        let d = domain(id)?;
        if tail == "state" && method == crate::http::Method::Get {
            let mut values = serde_json::Map::new();
            for part in query.split('&').filter(|s| !s.is_empty()) {
                let (k, v) = part.split_once('=').ok_or_else(features::invalid)?;
                if values
                    .insert(query_value(k)?, Value::String(query_value(v)?))
                    .is_some()
                {
                    return Err(features::invalid());
                }
            }
            let read_id = values
                .remove("read")
                .and_then(|v| v.as_str().map(str::to_owned))
                .ok_or_else(features::invalid)?;
            let r = d
                .reads
                .iter()
                .find(|r| r.id == read_id)
                .ok_or_else(features::invalid)?;
            let data = self.read(d, r, &Value::Object(values), io, b)?;
            return Ok(
                json!({"available":true,"readId":r.id,"generation":self.generation,"sampledAt":timestamp(io.now_unix()),"data":data}),
            );
        }
        if tail != "apply" || method != crate::http::Method::Post {
            return Err(features::invalid());
        }
        let mut input = decode_apply(body)?;
        if input.generation != self.generation {
            return Err(Error {
                status: 409,
                code: "generation_conflict",
                message: "设置已变化，请刷新后再保存。",
            });
        }
        if let Some(error) = self.load_error {
            return Err(error);
        }
        if self.busy() {
            return Err(busy_error());
        }
        let target = self.generation.checked_add(1).ok_or_else(storage)?;
        let action = d
            .actions
            .iter()
            .find(|a| a.id == input.action_id)
            .ok_or_else(features::invalid)?;
        fields_valid(action.fields, &input.input)?;
        if action.impact != Impact::Local && !input.acknowledge_impact {
            return Err(Error {
                status: 409,
                code: "impact_confirmation_required",
                message: "请确认本次操作对连接的影响。",
            });
        }
        // Only private preparation getters. DDNS add must not fetch a missing section.
        let needs_current = match d.id {
            "network" => matches!(action.id, "set_wan" | "set_wan6"),
            "wireless" => matches!(
                action.id,
                "set_wifi" | "set_all_wifi" | "set_guest_wifi" | "set_hostap_mlo" | "set_twt"
            ),
            _ => false,
        };
        let mut current = json!({});
        if needs_current {
            let read = d
                .reads
                .iter()
                .find(|r| r.id == action.readback)
                .ok_or_else(features::invalid)?;
            let params = read_parameters(read, &input.input);
            fields_valid(read.fields, &params)?;
            current = self.invoke(io, read.controller, read.handler, &params, b)?;
        }
        input.input = prepare(d.id, action.id, input.input, &current)?;
        fields_valid(action.fields, &input.input)?;
        validate(d.id, action.id, &input.input)?;
        if d.id == "wireless"
            && matches!(
                action.id,
                "set_wifi" | "set_all_wifi" | "set_hostap_mlo" | "set_twt"
            )
        {
            let wifi = if action.readback == "wifi_detail_all" {
                current
            } else {
                self.invoke(io, "xqnetwork", "getAllWifiInfo", &json!({}), b)?
            };
            crate::features_wireless::validate_environment(action.id, &input.input, &wifi)?;
        }
        let reconnect = if reconnect_action(action.id) {
            Some(self.reconnect_before(action.id, &input.input, io, b)?)
        } else {
            None
        };
        let operation_id = format!("{}-{}", io.now_unix(), target);
        let include_db = d.id == "network" && matches!(action.id, "mac_bind" | "mac_unbind");
        let recovery = if !action.configs.is_empty() || include_db {
            Some(crate::features_recovery::Recovery::prepare_with_paths(
                &self.data,
                &operation_id,
                action.configs,
                include_db,
                &self.config_dir,
                &self.device_db,
                io,
                b,
            )?)
        } else {
            None
        };
        let now = timestamp(io.now_unix());
        let operation = Operation {
            id: operation_id,
            domain: d.id.into(),
            action_id: action.id.into(),
            state: "pending".into(),
            generation: self.generation,
            started_at: now.clone(),
            updated_at: now,
            error: None,
            data: None,
            reconnect_address: reconnect.as_ref().and_then(|r| r.target_ip.clone()),
            can_confirm: false,
        };
        let task = Task {
            operation,
            phase: "admitted".into(),
            reserved_generation: target,
            native_attempted: false,
            deadline_unix: io.now_unix().saturating_add(if reconnect.is_some() {
                600
            } else {
                120
            }),
            reconnect,
        };
        let mut pending = Pending {
            task,
            input: input.input,
            action,
            next: Instant::now(),
            recovery,
            sequence_target: None,
        };
        if let Err(error) = self.save(&pending.task) {
            // No vendor action yet. The journal is inert, but retain lane if cleanup fails.
            pending.task.phase = "restoring".into();
            pending.task.operation.error = Some(error.code.into());
            self.pending = Some(pending);
            return Err(error);
        }
        if let Err(error) = before(action.impact) {
            // Root has not invoked the vendor. Restore/reload is caller-owned and retryable.
            pending.task.phase = "restoring".into();
            pending.task.operation.error = Some(error.code.into());
            self.pending = Some(pending);
            return Err(error);
        }
        pending.task.native_attempted = true;
        pending.task.phase = "pending".into();
        if let Err(error) = self.save(&pending.task) {
            pending.task.native_attempted = false;
            pending.task.phase = "restoring".into();
            pending.task.operation.error = Some(error.code.into());
            self.pending = Some(pending);
            return Err(error);
        }
        let result = self.invoke(io, action.controller, action.handler, &pending.input, b);
        self.cache.clear();
        pending.sequence_target = Some(target);
        pending.task.phase = "pending".into();
        if self.sequence(target).is_err() {
            pending.task.operation.error = Some("feature_storage_unavailable".into());
        } else {
            pending.sequence_target = None;
        }
        pending.task.operation.generation = self.generation;
        match result {
            Ok(native) if successful(&native) => {
                // Scan and other immediate result actions must verify the exact projected result.
                if action.readback.is_empty()
                    && let Ok(mut data) = project(d.id, action.id, native.clone())
                {
                    features::public_secrets(&mut data);
                    if private_verified(d.id, action.id, &pending.input, &native)
                        && verified(d.id, action.id, &pending.input, &data)
                    {
                        pending.task.operation.data = Some(data);
                        pending.task.phase = "completing".into();
                    }
                }
                if let Some(ip) = native
                    .get("ip")
                    .or_else(|| native.get("hostip"))
                    .and_then(Value::as_str)
                    .and_then(safe_ip)
                {
                    if let Some(r) = &mut pending.task.reconnect
                        && r.target_ip.is_none()
                    {
                        r.target_ip = Some(ip.clone());
                    }
                    pending.task.operation.reconnect_address = Some(ip);
                }
            }
            Ok(native)
                if matches!(action.id, "reboot" | "factory_reset" | "official_upgrade")
                    && native
                        .get("code")
                        .and_then(Value::as_i64)
                        .is_some_and(|code| code != 0) =>
            {
                // A structured native rejection means maintenance never started.
                // Do not restore/reload UCI or wait for a reboot that cannot happen.
                pending.task.phase = "settled".into();
                pending.task.operation.state = "failed".into();
                pending.task.operation.can_confirm = false;
                pending.task.operation.error = Some("vendor_rejected".into());
            }
            Ok(_) => {
                if !matches!(action.id, "reboot" | "factory_reset" | "official_upgrade")
                    && !pending
                        .task
                        .reconnect
                        .as_ref()
                        .is_some_and(|r| r.destructive)
                {
                    pending.task.phase = "restoring".into();
                }
                // Unknown maintenance output is not proof that it never started.
                pending.task.operation.error = Some(
                    if matches!(action.id, "reboot" | "factory_reset" | "official_upgrade") {
                        "vendor_reply_unverified"
                    } else {
                        "vendor_rejected"
                    }
                    .into(),
                );
            }
            Err(error) => {
                // A lost reboot/mode reply can follow a successful mutation. Prove it, do not replay it.
                if pending.task.reconnect.is_none() {
                    pending.task.phase = "restoring".into();
                }
                pending.task.operation.error = Some(error.code.into());
            }
        }
        if self.save(&pending.task).is_err() {
            pending.task.operation.error = Some("feature_storage_unavailable".into());
        }
        let operation = pending.task.operation.clone();
        if pending.task.phase == "settled"
            && pending.sequence_target.is_none()
            && self.save(&pending.task).is_ok()
        {
            let discarded = if let Some(recovery) = pending.recovery.take() {
                recovery.discard()
            } else {
                crate::features_recovery::Recovery::discard_recovered(&self.data)
            };
            if discarded.is_ok() {
                self.record(operation.clone());
                return Ok(json!({"operation":operation}));
            }
        }
        self.pending = Some(pending);
        Ok(json!({"operation":operation}))
    }
    fn reconnect_before<B: Backend>(
        &self,
        id: &str,
        input: &Value,
        io: &mut B,
        b: &Budget<'_>,
    ) -> Result<Reconnect, Error> {
        let boot_id = boot_id(io, b).ok_or(Error {
            status: 503,
            code: "reconnect_proof_unavailable",
            message: "无法读取重连任务的启动标识。",
        })?;
        let target_ip = if id == "set_lan_ip" {
            input.get("ip").and_then(Value::as_str).and_then(safe_ip)
        } else if id == "factory_reset" {
            Some("192.168.31.1".into())
        } else {
            uci(io, "network.lan.ipaddr", b).and_then(|s| safe_ip(&s))
        };
        let previous_version = if id == "official_upgrade" {
            let v = self.invoke(io, "xqsystem", "getInitInfo", &json!({}), b)?;
            v.get("romversion")
                .and_then(Value::as_str)
                .filter(|s| s.len() <= 128)
                .map(str::to_owned)
        } else {
            None
        };
        if id == "official_upgrade" && previous_version.is_none() {
            return Err(features::invalid());
        }
        Ok(Reconnect {
            boot_id,
            target_ip,
            target_mask: input.get("mask").and_then(Value::as_str).map(str::to_owned),
            previous_version,
            expected_mode: match id {
                "set_lan_ap" => Some("lanapmode".into()),
                "set_wifi_ap" => Some("wifiapmode".into()),
                "disable_lan_ap" | "disable_wifi_ap" => Some("router".into()),
                _ => None,
            },
            destructive: matches!(id, "factory_reset" | "official_upgrade"),
        })
    }
    fn listener_matches(&self, task: &Task) -> bool {
        self.management_listener.is_some_and(|listener| {
            !listener.ip().is_unspecified()
                && task
                    .reconnect
                    .as_ref()
                    .and_then(|r| r.target_ip.as_deref())
                    .is_some_and(|target| target == listener.ip().to_string())
        })
    }
    fn reconnect_proof<B: Backend>(&self, task: &mut Task, io: &mut B, b: &Budget<'_>) -> bool {
        self.reconnect_native_proof(task, io, b) && self.listener_matches(task)
    }
    fn reconnect_native_proof<B: Backend>(
        &self,
        task: &mut Task,
        io: &mut B,
        b: &Budget<'_>,
    ) -> bool {
        let Some(reconnect) = &mut task.reconnect else {
            return false;
        };
        let Some(current_boot) = boot_id(io, b) else {
            return false;
        };
        let id = task.operation.action_id.as_str();
        if matches!(id, "reboot" | "factory_reset" | "official_upgrade")
            && current_boot == reconnect.boot_id
        {
            return false;
        }
        if let Some(want) = &reconnect.expected_mode {
            let Some(actual) = uci(io, "xiaoqiang.common.NETMODE", b) else {
                return false;
            };
            if if want == "router" {
                matches!(actual.as_str(), "lanapmode" | "wifiapmode")
            } else {
                &actual != want
            } {
                return false;
            }
            // Exact public native mode DTO must also be available. UCI alone is not enough.
            let Ok(native) = self.invoke(io, "xqnetwork", "getMode", &json!({}), b) else {
                return false;
            };
            let Ok(data) = project("network", "mode", native) else {
                return false;
            };
            let expected = match want.as_str() {
                "lanapmode" => 2,
                "wifiapmode" => 1,
                _ => 0,
            };
            if data.get("mode").and_then(numeric) != Some(expected) {
                return false;
            }
            // AP addresses can be assigned by the upstream DHCP server.
            reconnect.target_ip = data
                .get("hostip")
                .and_then(Value::as_str)
                .and_then(safe_ip)
                .or_else(|| uci(io, "network.lan.ipaddr", b).and_then(|s| safe_ip(&s)));
        }
        let Some(target) = reconnect.target_ip.as_ref() else {
            return false;
        };
        // AP DHCP may leave the static UCI LAN address unchanged. Its UCI mode
        // and public native mode were checked above; the current hostip still
        // needs both kernel br-lan and native LAN proof below.
        let ap = matches!(
            reconnect.expected_mode.as_deref(),
            Some("lanapmode" | "wifiapmode")
        );
        if !ap && uci(io, "network.lan.ipaddr", b).as_ref() != Some(target) {
            return false;
        }
        // Candidate binding never substitutes for native + kernel proof.
        // Explicit authenticated confirm additionally checks the trusted listener.
        let Ok(out) = io.run(
            Program::Ip,
            &[
                "-j".into(),
                "address".into(),
                "show".into(),
                "dev".into(),
                "br-lan".into(),
            ],
            None,
            64 << 10,
            b,
        ) else {
            return false;
        };
        if out.code != 0 {
            return false;
        }
        let Ok(addresses) = serde_json::from_slice::<Value>(&out.stdout) else {
            return false;
        };
        if !addresses.as_array().is_some_and(|a| {
            a.iter().any(|i| {
                i["addr_info"].as_array().is_some_and(|a| {
                    a.iter()
                        .any(|v| v["local"].as_str() == Some(target.as_str()))
                })
            })
        }) {
            return false;
        }
        let Ok(lan) = self.invoke(io, "xqnetwork", "getLanInfo", &json!({}), b) else {
            return false;
        };
        let Ok(data) = project("network", "lan_info", lan) else {
            return false;
        };
        if !data
            .pointer("/info/ipv4")
            .and_then(Value::as_array)
            .is_some_and(|a| {
                a.iter().any(|v| {
                    v["ip"].as_str() == Some(target.as_str())
                        && reconnect
                            .target_mask
                            .as_ref()
                            .is_none_or(|m| v["mask"].as_str() == Some(m))
                })
            })
        {
            return false;
        }
        if id == "official_upgrade" {
            let Ok(version) = self.invoke(io, "xqsystem", "getInitInfo", &json!({}), b) else {
                return false;
            };
            let Ok(public) = project("services", "version", version) else {
                return false;
            };
            if !public
                .get("romversion")
                .and_then(Value::as_str)
                .is_some_and(|v| !v.is_empty() && reconnect.previous_version.as_deref() != Some(v))
            {
                return false;
            }
        }
        if id == "factory_reset" {
            // The native initialization marker is reset, not an unchanged old getter.
            if uci(io, "xiaoqiang.common.INITTED", b).is_none_or(|v| v == "YES") {
                return false;
            }
        }
        task.operation.reconnect_address = Some(target.clone());
        task.operation.data = Some(data);
        true
    }
    fn confirm<B: Backend>(
        &mut self,
        id: &str,
        io: &mut B,
        b: &Budget<'_>,
    ) -> Result<Value, Error> {
        if let Some(e) = self.load_error {
            return Err(e);
        }
        let Some(mut pending) = self.pending.take() else {
            return Err(features::invalid());
        };
        if pending.task.operation.id != id
            || pending.task.reconnect.is_none()
            || !self.startup_ready
            || pending.task.phase != "pending"
            || !self.reconnect_proof(&mut pending.task, io, b)
        {
            self.pending = Some(pending);
            return Err(Error {
                status: 409,
                code: "reconnect_not_verified",
                message: "新管理地址或重启结果尚未确认，请重连后重试。",
            });
        }
        pending.task.operation.can_confirm = true;
        pending.task.phase = "completing".into();
        let result = self.complete(&mut pending, io);
        if let Err(error) = result {
            self.pending = Some(pending);
            return Err(error);
        }
        let operation = pending.task.operation.clone();
        self.record(operation.clone());
        Ok(json!({"operation":operation}))
    }
    fn complete<B: Backend>(&mut self, pending: &mut Pending, io: &mut B) -> Result<(), Error> {
        if let Some(target) = pending.sequence_target {
            self.sequence(target)?;
            pending.sequence_target = None;
        }
        let mut accepted = pending.task.clone();
        accepted.operation.generation = self.generation;
        accepted.operation.state = "completed".into();
        accepted.operation.error = None;
        accepted.operation.can_confirm = false;
        accepted.operation.updated_at = timestamp(io.now_unix());
        accepted.phase = "settled".into();
        // Publish the verified outcome before checkpoint acceptance. A crash here
        // leaves a settled task; startup discards it instead of rolling it back.
        self.save(&accepted)?;
        if let Some(recovery) = pending.recovery.take() {
            recovery.discard()?;
        } else {
            crate::features_recovery::Recovery::discard_recovered(&self.data)?;
        }
        pending.task = accepted;
        Ok(())
    }
    fn fail_maintenance<B: Backend>(
        &mut self,
        pending: &mut Pending,
        code: &str,
        io: &mut B,
    ) -> Result<(), Error> {
        pending.task.operation.state = "failed".into();
        pending.task.operation.can_confirm = false;
        pending.task.operation.error = Some(code.into());
        pending.task.operation.updated_at = timestamp(io.now_unix());
        pending.task.phase = "settled".into();
        // Publish the rejection before discarding. Crash recovery must never
        // restore pre-maintenance UCI for a proven never-started flash.
        self.save(&pending.task)?;
        if let Some(recovery) = pending.recovery.take() {
            recovery.discard()?;
        } else {
            crate::features_recovery::Recovery::discard_recovered(&self.data)?;
        }
        Ok(())
    }
    fn restore_pending<B: Backend>(
        &mut self,
        pending: &mut Pending,
        io: &mut B,
    ) -> Result<(), Error> {
        if pending.task.native_attempted
            && pending
                .task
                .reconnect
                .as_ref()
                .is_some_and(|r| r.destructive)
        {
            pending.task.operation.state = "pending".into();
            pending.task.operation.error = Some("maintenance_reconnect_required".into());
            // UCI rollback cannot undo a factory reset or firmware flash.
            self.save(&pending.task)?;
            return Err(busy_error());
        }
        if let Some(recovery) = &mut pending.recovery {
            self.restore(recovery, io, pending.action.id)?;
        } else {
            self.recover(io, Some(pending.action.id))?;
        }
        if let Some(recovery) = pending.recovery.take() {
            recovery.discard()?;
        }
        pending.task.operation.state = "failed".into();
        pending.task.operation.can_confirm = false;
        pending.task.operation.updated_at = timestamp(io.now_unix());
        pending.task.phase = "settled".into();
        self.save(&pending.task)
    }
    pub fn tick<B: Backend>(&mut self, io: &mut B, b: &Budget<'_>) {
        self.rebind_address = None;
        if check(b).is_err() {
            return;
        }
        if !self.startup_ready {
            if Instant::now() < self.startup_next {
                return;
            }
            self.startup_next = Instant::now() + Duration::from_secs(5);
            if self.load_error.is_some() {
                return;
            }
            // Settled tasks were already verified and durably accepted.
            let accepted = self.operations.front().is_some();
            let reconnect = self.pending.as_ref().is_some_and(|p| {
                p.task.native_attempted
                    && p.task.reconnect.is_some()
                    && matches!(p.task.phase.as_str(), "pending" | "completing")
            });
            let result = if accepted {
                crate::features_recovery::Recovery::discard_recovered(&self.data)
            } else if reconnect {
                Ok(())
            } else {
                self.recover(io, self.pending.as_ref().map(|p| p.action.id))
            };
            if let Err(error) = result {
                self.startup_error = Some(error);
                return;
            }
            if let Some(target) = self.pending.as_ref().map(|p| p.task.reserved_generation)
                && self.generation < target
                && let Err(error) = self.sequence(target)
            {
                self.startup_error = Some(error);
                return;
            }
            self.startup_error = None;
            self.startup_ready = true;
            if !reconnect && let Some(mut pending) = self.pending.take() {
                pending.task.operation.state = "failed".into();
                pending.task.operation.error = Some("operation_interrupted".into());
                pending.task.operation.updated_at = timestamp(io.now_unix());
                pending.task.phase = "settled".into();
                if self.save(&pending.task).is_err() {
                    self.pending = Some(pending);
                    self.startup_ready = false;
                    return;
                }
                self.record(pending.task.operation);
            }
            return; // Never recover and invoke an action in the same startup tick.
        }
        let Some(mut pending) = self.pending.take() else {
            return;
        };
        if Instant::now() < pending.next {
            self.pending = Some(pending);
            return;
        }
        pending.next = Instant::now() + Duration::from_secs(2);
        if let Some(target) = pending.sequence_target {
            if self.sequence(target).is_err() {
                self.pending = Some(pending);
                return;
            }
            pending.sequence_target = None;
            pending.task.operation.generation = self.generation;
            if self.save(&pending.task).is_err() {
                self.pending = Some(pending);
                return;
            }
        }
        if pending.task.phase == "settled" {
            if self.save(&pending.task).is_ok() {
                if let Some(recovery) = pending.recovery.take() {
                    if recovery.discard().is_err() {
                        self.pending = Some(pending);
                        return;
                    }
                } else if crate::features_recovery::Recovery::discard_recovered(&self.data).is_err()
                {
                    self.pending = Some(pending);
                    return;
                }
                self.record(pending.task.operation);
                return;
            }
            self.pending = Some(pending);
            return;
        }
        if pending.task.phase == "completing" {
            if self.complete(&mut pending, io).is_ok() {
                self.record(pending.task.operation);
                return;
            }
            pending.task.operation.state = "pending".into();
            pending.task.operation.error = Some("feature_storage_unavailable".into());
            self.pending = Some(pending);
            return;
        }
        if pending.task.phase == "restoring" {
            if self.restore_pending(&mut pending, io).is_ok() {
                self.record(pending.task.operation);
                return;
            }
            pending.task.operation.error = Some("restore_needs_retry".into());
            self.pending = Some(pending);
            return;
        }
        if pending.task.operation.action_id == "official_upgrade"
            && pending
                .task
                .reconnect
                .as_ref()
                .is_some_and(|r| boot_id(io, b).as_deref() == Some(r.boot_id.as_str()))
            && let Ok(native) = self.invoke(io, "xqsystem", "upgradeStatus", &json!({}), b)
            && successful(&native)
            && let Ok(public) = project("services", "upgrade_status", native)
            && let Some(code) = public.get("status").and_then(numeric)
            && let Some(error) = (match code {
                6 => Some("official_upgrade_no_update"),
                7 => Some("official_upgrade_missing_metadata"),
                8 => Some("official_upgrade_download_failed"),
                9 => Some("official_upgrade_image_invalid"),
                10 => Some("official_upgrade_secboot_failed"),
                _ => None, // Progress, flash started, unknown or absent: keep proof pending.
            })
        {
            if self.fail_maintenance(&mut pending, error, io).is_ok() {
                self.record(pending.task.operation);
            } else {
                self.pending = Some(pending);
            }
            return;
        }
        if pending.task.reconnect.is_some() {
            let native_proved = self.reconnect_native_proof(&mut pending.task, io, b);
            let proved = native_proved && self.listener_matches(&pending.task);
            if native_proved
                && !proved
                && let Some(listener) = self.management_listener
                && !listener.ip().is_unspecified()
                && let Some(ip) = pending
                    .task
                    .reconnect
                    .as_ref()
                    .and_then(|r| r.target_ip.as_ref())
                    .and_then(|ip| ip.parse::<std::net::IpAddr>().ok())
            {
                self.rebind_address = Some(std::net::SocketAddr::new(ip, listener.port()));
            }
            pending.task.operation.state = "pending".into();
            pending.task.operation.can_confirm = proved;
            if proved {
                pending.task.operation.error = None;
            }
            if io.now_unix() >= pending.task.deadline_unix {
                if pending
                    .task
                    .reconnect
                    .as_ref()
                    .is_some_and(|r| r.destructive)
                {
                    pending.task.operation.error = Some("maintenance_reconnect_required".into());
                } else if !proved {
                    pending.task.phase = "restoring".into();
                    pending.task.operation.error = Some("reconnect_timeout".into());
                    self.rebind_address = None;
                }
            }
            pending.task.operation.updated_at = timestamp(io.now_unix());
            if self.save(&pending.task).is_err() {
                pending.task.operation.error = Some("feature_storage_unavailable".into());
            }
            self.pending = Some(pending);
            return;
        }
        let d = match domain(&pending.task.operation.domain) {
            Ok(d) => d,
            Err(_) => {
                self.pending = Some(pending);
                return;
            }
        };
        if let Some(read) = d.reads.iter().find(|r| r.id == pending.action.readback) {
            let params = read_parameters(read, &pending.input);
            if fields_valid(read.fields, &params).is_ok()
                && let Ok(native) = self.invoke(io, read.controller, read.handler, &params, b)
            {
                let private_ok = private_verified(d.id, pending.action.id, &pending.input, &native);
                if let Ok(mut data) = project(d.id, read.id, native) {
                    features::public_secrets(&mut data);
                    let runtime_ok = if matches!(
                        pending.action.id,
                        "forward_apply" | "dmz_reload" | "ddns_reload"
                    ) {
                        runtime_verified(pending.action.id, &data, io, b)
                    } else {
                        verified(d.id, pending.action.id, &pending.input, &data)
                    };
                    let extra_ok = if pending.action.id == "ddns_add" {
                        self.invoke(io, "xqnetwork", "ddnsStatus", &json!({}), b)
                            .ok()
                            .and_then(|v| project("services", "ddns", v).ok())
                            .and_then(|v| v["list"].as_array().cloned())
                            .is_some_and(|rows| {
                                rows.iter().any(|row| {
                                    numeric(&row["id"]) == numeric(&pending.input["id"])
                                        && numeric(&row["enabled"])
                                            == numeric(&pending.input["enable"])
                                })
                            })
                    } else {
                        true
                    };
                    if private_ok && runtime_ok && extra_ok {
                        pending.task.operation.data = Some(data);
                        pending.task.phase = "completing".into();
                        if self.complete(&mut pending, io).is_ok() {
                            self.cache.clear();
                            self.record(pending.task.operation);
                            return;
                        }
                        pending.task.operation.state = "pending".into();
                        pending.task.operation.error = Some("feature_storage_unavailable".into());
                        self.pending = Some(pending);
                        return;
                    }
                }
            }
        }
        if io.now_unix() >= pending.task.deadline_unix {
            pending.task.phase = "restoring".into();
            pending.task.operation.error = Some("readback_failed".into());
            if self.save(&pending.task).is_err() {
                pending.task.operation.error = Some("feature_storage_unavailable".into());
            }
            if self.restore_pending(&mut pending, io).is_ok() {
                self.record(pending.task.operation);
                return;
            }
            pending.task.operation.error = Some("restore_needs_retry".into());
        }
        self.pending = Some(pending);
    }
    fn record(&mut self, o: Operation) {
        self.operations.push_front(o);
        while self.operations.len() > MAX_OPERATIONS {
            self.operations.pop_back();
        }
    }
}
fn safe_ip(s: &str) -> Option<String> {
    s.parse::<std::net::Ipv4Addr>()
        .ok()
        .filter(|ip| {
            !ip.is_unspecified() && !ip.is_loopback() && !ip.is_multicast() && !ip.is_broadcast()
        })
        .filter(|ip| ip.to_string() == s)
        .map(|ip| ip.to_string())
}
fn boot_id<B: Backend>(io: &mut B, b: &Budget<'_>) -> Option<String> {
    let raw = io
        .read(Path::new("/proc/sys/kernel/random/boot_id"), 64, b)
        .ok()?;
    let s = std::str::from_utf8(&raw).ok()?.trim();
    (s.len() == 36
        && s.bytes().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        }))
    .then(|| s.to_owned())
}
fn uci<B: Backend>(io: &mut B, key: &str, b: &Budget<'_>) -> Option<String> {
    let out = io
        .run(
            Program::Uci,
            &["-q".into(), "get".into(), key.into()],
            None,
            4096,
            b,
        )
        .ok()?;
    if out.code != 0 {
        return None;
    }
    let s = std::str::from_utf8(&out.stdout).ok()?.trim();
    (s.len() <= 4096 && !s.chars().any(char::is_control)).then(|| s.to_owned())
}
fn runtime_verified<B: Backend>(id: &str, data: &Value, io: &mut B, b: &Budget<'_>) -> bool {
    if id == "ddns_reload" {
        let Some(rows) = data.get("list").and_then(Value::as_array) else {
            return false;
        };
        let enabled = rows
            .iter()
            .any(|r| r["enabled"].as_i64() == Some(1) || r["enabled"].as_str() == Some("1"));
        let status = io.run(
            Program::Service,
            &["ddns".into(), "status".into()],
            None,
            4096,
            b,
        );
        return match status {
            Ok(out) => {
                if enabled {
                    out.code == 0
                } else {
                    matches!(out.code, 0 | 3)
                }
            }
            Err(_) => false,
        };
    }
    let Ok(out) = io.run(
        Program::Iptables,
        &["-t".into(), "nat".into(), "-S".into()],
        None,
        64 << 10,
        b,
    ) else {
        return false;
    };
    if out.code != 0 {
        return false;
    }
    let Ok(text) = std::str::from_utf8(&out.stdout) else {
        return false;
    };
    let rules = text
        .lines()
        .filter(|l| l.starts_with("-A "))
        .map(|l| l.split_whitespace().collect::<Vec<_>>())
        .collect::<Vec<_>>();
    if id == "dmz_reload" {
        let Some(status) = data["status"]
            .as_i64()
            .or_else(|| data["status"].as_str().and_then(|s| s.parse().ok()))
        else {
            return false;
        };
        if !matches!(status, 0 | 1) {
            return false;
        }
        let enabled = status == 1;
        let target = data.get("ip").and_then(Value::as_str);
        let dmz = rules
            .iter()
            .filter(|r| {
                r.get(1)
                    .is_some_and(|c| c.to_ascii_lowercase().contains("dmz"))
            })
            .collect::<Vec<_>>();
        return if enabled {
            target.is_some_and(|ip| dmz.iter().any(|r| rule_target(r, ip, None, None)))
        } else {
            dmz.iter()
                .all(|r| !r.windows(2).any(|w| w == ["-j", "DNAT"]))
        };
    }
    let Some(rows) = data.get("list").and_then(Value::as_array) else {
        return false;
    };
    // The stock nat graph and forwarding chain must exist even for an empty rule list.
    let chain_present = text.lines().any(|l| {
        l.starts_with("-N ")
            && (l.to_ascii_lowercase().contains("redirect") || l.contains("zone_wan_prerouting"))
    });
    chain_present
        && rows.iter().all(|row| {
            let Some(ip) = row["destip"].as_str().and_then(safe_ip) else {
                return false;
            };
            let proto = row["proto"]
                .as_i64()
                .or_else(|| row["proto"].as_str().and_then(|s| s.parse().ok()));
            let protocols: &[&str] = match proto {
                Some(1) => &["tcp"],
                Some(2) => &["udp"],
                Some(3) => &["tcp", "udp"],
                _ => return false,
            };
            let src = if let Some(port) = value_string(&row["srcport"]) {
                port
            } else {
                let (Some(f), Some(t)) = (
                    value_string(&row["srcport"]["f"]),
                    value_string(&row["srcport"]["t"]),
                ) else {
                    return false;
                };
                format!("{f}:{t}")
            };
            let dest = value_string(&row["destport"]);
            protocols.iter().all(|p| {
                rules.iter().any(|r| {
                    r.windows(2).any(|w| w == ["-p", *p])
                        && r.windows(2).any(|w| w == ["--dport", src.as_str()])
                        && rule_target(r, &ip, dest.as_deref(), None)
                })
            })
        })
}
fn numeric(v: &Value) -> Option<i64> {
    v.as_i64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}
fn value_string(v: &Value) -> Option<String> {
    v.as_u64().map(|n| n.to_string()).or_else(|| {
        v.as_str()
            .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            .map(str::to_owned)
    })
}
fn rule_target(words: &[&str], ip: &str, port: Option<&str>, _unused: Option<&str>) -> bool {
    words.windows(2).any(|w| w == ["-j", "DNAT"])
        && words.windows(2).any(|w| {
            matches!(w[0], "--to-destination" | "--to")
                && w[1]
                    == port
                        .map(|p| format!("{ip}:{p}"))
                        .unwrap_or_else(|| ip.into())
        })
}
fn storage() -> Error {
    Error {
        status: 507,
        code: "feature_storage_unavailable",
        message: "无法保存本次操作的恢复数据。",
    }
}

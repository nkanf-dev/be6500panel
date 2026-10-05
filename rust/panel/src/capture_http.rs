//! Fixed authenticated capture projection. Trusted shared handle only; no
//! callback/command/PID/filesystem authority can be supplied by a request.
use crate::{
    capture_runtime::{CaptureHandle, CurrentState, CurrentStatus, Selection},
    capture_state::{self, Snapshot},
    http::{self, Method},
    readiness_tun::Observer,
    runtime_manager::{HookContext, HookError, Manager, ServiceId},
};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, MapAccess, SeqAccess, Visitor},
};
use std::{
    fmt,
    io::{self, Write},
    time::{Duration, Instant},
};
const MAX_RESPONSE: usize = 64 << 10;
pub(crate) trait CaptureControl {
    fn selection_scope(
        &self,
        budget: &crate::readiness_tun::Budget<'_>,
    ) -> Result<crate::capture_lan::Snapshot, HookError>;
    fn snapshot(&self) -> Result<Snapshot, HookError>;
    fn commands(&self) -> Result<usize, HookError>;
    fn unobserved(&self) -> Result<CurrentStatus, HookError>;
    fn observe(&self, context: &HookContext<'_>) -> Result<CurrentStatus, HookError>;
    fn select(
        &self,
        context: &HookContext<'_>,
        selection: &Selection,
    ) -> Result<CurrentStatus, HookError>;
    fn disable(&self, deadline: Instant) -> Result<(), capture_state::Error>;
    fn withdraw(&self, deadline: Instant) -> Result<(), HookError>;
}
impl<O: Observer + 'static> CaptureControl for CaptureHandle<O> {
    fn selection_scope(
        &self,
        budget: &crate::readiness_tun::Budget<'_>,
    ) -> Result<crate::capture_lan::Snapshot, HookError> {
        self.selection_scope(budget)
    }
    fn commands(&self) -> Result<usize, HookError> {
        self.owned_command_count()
    }
    fn snapshot(&self) -> Result<Snapshot, HookError> {
        self.snapshot()
    }
    fn unobserved(&self) -> Result<CurrentStatus, HookError> {
        self.current_unobserved()
    }
    fn observe(&self, c: &HookContext<'_>) -> Result<CurrentStatus, HookError> {
        self.observe_current(c)
    }
    fn select(&self, c: &HookContext<'_>, s: &Selection) -> Result<CurrentStatus, HookError> {
        self.select(c, s)
    }
    fn disable(&self, d: Instant) -> Result<(), capture_state::Error> {
        self.disable(d)
    }
    fn withdraw(&self, d: Instant) -> Result<(), HookError> {
        self.startup_withdraw(d)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Input {
    #[serde(default)]
    scope: String,
    #[serde(default, deserialize_with = "devices")]
    devices: Vec<Device>,
    #[serde(default, rename = "clientIPv4")]
    client_ipv4: String,
    #[serde(default, rename = "clientIPv6")]
    client_ipv6: String,
    // Required even though the other omission-compatible fields are optional.
    #[serde(deserialize_with = "direct")]
    ipv6: String,
}
fn direct<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let value = String::deserialize(d)?;
    if value != "direct" {
        return Err(de::Error::custom("direct only"));
    }
    Ok(value)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Device {
    mac: String,
}
fn device_object<'de, D: Deserializer<'de>>(d: D) -> Result<Device, D::Error> {
    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = Device;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("device object")
        }
        fn visit_map<M: MapAccess<'de>>(self, m: M) -> Result<Device, M::Error> {
            Device::deserialize(de::value::MapAccessDeserializer::new(m))
        }
    }
    d.deserialize_map(V)
}
struct Item(Device);
impl<'de> Deserialize<'de> for Item {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        device_object(d).map(Self)
    }
}
fn devices<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Device>, D::Error> {
    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = Vec<Device>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("bounded selected devices")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Vec<Device>, A::Error> {
            let mut out = Vec::new();
            while out.len() < 64 {
                match a.next_element::<Item>()? {
                    Some(Item(item)) => {
                        if item.mac.len() > 64 {
                            return Err(de::Error::custom("MAC bound"));
                        }
                        out.push(item)
                    }
                    None => return Ok(out),
                }
            }
            if a.next_element::<de::IgnoredAny>()?.is_some() {
                return Err(de::Error::custom("device limit"));
            }
            Ok(out)
        }
    }
    d.deserialize_seq(V)
}
fn decode(body: &[u8]) -> Result<Selection, ()> {
    if body.len() > 64 << 10
        || body
            .iter()
            .find(|b| !b.is_ascii_whitespace())
            .is_none_or(|b| *b != b'{')
    {
        return Err(());
    }
    let input: Input = serde_json::from_slice(body).map_err(|_| ())?;
    if input.ipv6 != "direct"
        || !input.client_ipv6.is_empty()
        || input.scope.len() > 16
        || input.client_ipv4.len() > 64
    {
        return Err(());
    }
    if input.scope == "gateway" {
        if !input.devices.is_empty() || !input.client_ipv4.is_empty() {
            return Err(());
        }
    } else if !matches!(input.scope.as_str(), "" | "devices")
        || input.devices.is_empty() == input.client_ipv4.is_empty()
    {
        return Err(());
    }
    Ok(Selection {
        scope: input.scope,
        devices: input
            .devices
            .into_iter()
            .map(|d| capture_state::DeviceSelection { mac: d.mac })
            .collect(),
        client_ipv4: input.client_ipv4,
        client_ipv6: input.client_ipv6,
        ipv6: input.ipv6,
    })
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Wire {
    active: bool,
    scope: String,
    #[serde(rename = "lanIPv4Prefixes")]
    lan_ipv4_prefixes: Vec<String>,
    #[serde(rename = "installedLanIPv4Prefixes")]
    installed_lan_ipv4_prefixes: Vec<String>,
    desired: bool,
    clients: Vec<Client>,
    installed_clients: Vec<Client>,
    ipv6: &'static str,
    state: &'static str,
    cleanup_pending: bool,
    scope_state: &'static str,
    commands: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'static str>,
}
#[derive(Serialize)]
struct Client {
    mac: String,
    ip: String,
    hostname: &'static str,
}
fn wire(snapshot: Snapshot, current: CurrentStatus, commands: usize) -> Wire {
    let d = snapshot.desired;
    let o = snapshot.ownership;
    let mut installed = Vec::new();
    if let Some(owned) = &o {
        for (ip, mac) in &owned.client_macs {
            installed.push(Client {
                mac: mac.clone(),
                ip: ip.clone(),
                hostname: "",
            });
        }
    }
    // Saved selections do not acquire an old journal IP as current identity.
    let clients = d
        .devices
        .iter()
        .map(|device| Client {
            mac: device.mac.clone(),
            ip: if current.active {
                o.as_ref()
                    .and_then(|owned| {
                        owned
                            .client_macs
                            .iter()
                            .find(|(_, mac)| *mac == &device.mac)
                    })
                    .map_or_else(String::new, |(ip, _)| ip.clone())
            } else {
                String::new()
            },
            hostname: "",
        })
        .collect();
    let state = match current.state {
        CurrentState::Inactive => "inactive",
        CurrentState::Suspended => "suspended",
        CurrentState::Staged => "staged",
        CurrentState::CleanupPending => "cleanup-pending",
        CurrentState::ScopeChanged => "scope-changed",
        CurrentState::Unknown => "unknown",
        CurrentState::Active => "active",
    };
    let error = if current.intent.disable_not_persisted {
        Some("capture_disable_not_persisted")
    } else if current.intent.storage_uncertain {
        Some("capture_storage_uncertain")
    } else {
        match current.state {
            CurrentState::CleanupPending => Some("capture_cleanup_pending"),
            CurrentState::ScopeChanged => Some("capture_scope_changed_apply_required"),
            CurrentState::Unknown => Some("capture_observation_failed"),
            _ => None,
        }
    };
    Wire {
        active: current.active,
        scope: if d.scope == "gateway" {
            "gateway".into()
        } else {
            "devices".into()
        },
        lan_ipv4_prefixes: d.lan_ipv4_prefixes,
        installed_lan_ipv4_prefixes: o.map_or_else(Vec::new, |o| o.lan_ipv4_prefixes),
        desired: d.desired,
        clients,
        installed_clients: installed,
        ipv6: "direct",
        state,
        cleanup_pending: current.intent.cleanup_pending,
        scope_state: if current.state == CurrentState::ScopeChanged {
            "changed"
        } else if current.active {
            "current"
        } else {
            "unresolved"
        },
        commands,
        error,
    }
}
fn observed(
    manager: &mut Manager,
    capture: &dyn CaptureControl,
    head: bool,
) -> Result<CurrentStatus, HookError> {
    let prior = capture.unobserved()?;
    if head || prior.state != CurrentState::Unknown {
        return Ok(prior);
    }
    match manager.observe_current(
        ServiceId::SingBox,
        Instant::now() + Duration::from_secs(3),
        |context| capture.observe(context),
    ) {
        Ok(status) => Ok(status),
        Err(_) => Ok(prior),
    }
}
pub(crate) fn respond(
    writer: &mut impl Write,
    manager: &mut Manager,
    capture: Option<&dyn CaptureControl>,
    target: &str,
    method: Method,
    body: &[u8],
) -> io::Result<()> {
    let head = method == Method::Head;
    let Some(capture) = capture else {
        return error(writer, 503, "capture_unavailable", None, head);
    };
    if target != "/api/proxy/capture" {
        return error(writer, 400, "invalid_input", None, head);
    }
    let failed = match method {
        Method::Get | Method::Head => None,
        Method::Delete => {
            if !body.is_empty() {
                return error(writer, 400, "invalid_input", None, false);
            }
            capture
                .disable(Instant::now() + Duration::from_secs(30))
                .err()
                .map(|error| match error {
                    capture_state::Error::DisableFailed {
                        persistence_failed: true,
                        ..
                    } => "capture_disable_not_persisted",
                    _ => "cleanup_failed",
                })
        }
        Method::Post => {
            let selection = match decode(body) {
                Ok(selection) => selection,
                Err(()) => return error(writer, 400, "invalid_json", None, false),
            };
            let ready = manager
                .status(ServiceId::SingBox)
                .is_ok_and(|status| status.active && status.desired && !status.needs_recovery);
            if !ready {
                return error(writer, 409, "proxy_not_running", None, false);
            }
            let mut applied = false;
            let result =
                manager.capture_operation(Instant::now() + Duration::from_secs(30), |context| {
                    let current = capture.select(context, &selection)?;
                    applied = current.active;
                    Ok(current)
                });
            if result.is_err() && applied {
                // Actual owned status invalidated the completed Apply. Only
                // this explicit mutation withdraws; GET never repairs it.
                let _ = capture.withdraw(Instant::now() + Duration::from_secs(30));
            }
            result.err().map(|error| {
                if error == HookError::Deadline {
                    "operation_timeout"
                } else {
                    "capture_failed"
                }
            })
        }
    };
    let snapshot = match capture.snapshot() {
        Ok(snapshot) => snapshot,
        Err(_) => return error(writer, 503, "capture_unavailable", None, head),
    };
    let current = match observed(manager, capture, head) {
        Ok(current) => current,
        Err(_) => return error(writer, 503, "capture_unavailable", None, head),
    };
    let commands = match capture.commands() {
        Ok(count) => count,
        Err(_) => return error(writer, 503, "capture_unavailable", None, head),
    };
    let output = wire(snapshot, current, commands);
    if let Some(code) = failed {
        return error(
            writer,
            if method == Method::Delete { 500 } else { 409 },
            code,
            Some(output),
            false,
        );
    }
    json(writer, 200, &output, head)
}
#[derive(Serialize)]
struct Error {
    code: &'static str,
    message: &'static str,
}
#[derive(Serialize)]
struct Failure {
    error: Error,
    #[serde(skip_serializing_if = "Option::is_none")]
    capture: Option<Wire>,
}
fn error(
    writer: &mut impl Write,
    status: u16,
    code: &'static str,
    capture: Option<Wire>,
    head: bool,
) -> io::Result<()> {
    json(
        writer,
        status,
        &Failure {
            error: Error {
                code,
                message: "Capture operation did not complete; read the current state before retrying.",
            },
            capture,
        },
        head,
    )
}
struct Count(usize);
impl Write for Count {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .filter(|n| *n < MAX_RESPONSE)
            .ok_or_else(|| io::Error::other("capture response bound"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn json(
    writer: &mut impl Write,
    status: u16,
    value: &impl Serialize,
    head: bool,
) -> io::Result<()> {
    let mut count = Count(0);
    serde_json::to_writer(&mut count, value).map_err(io::Error::other)?;
    let mut writer = io::BufWriter::with_capacity(8192, writer);
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        409 => "Conflict",
        500 => "Internal Server Error",
        _ => "Service Unavailable",
    };
    http::write_headers(
        &mut writer,
        status,
        reason,
        "application/json; charset=utf-8",
        (count.0 + 1) as u64,
    )?;
    if !head {
        serde_json::to_writer(&mut writer, value).map_err(io::Error::other)?;
        writer.write_all(b"\n")?;
    }
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_input_is_strict_map_direct_and_bounded_not_command_authority() {
        assert!(decode(br#"{"scope":"gateway","ipv6":"direct"}"#).is_ok());
        assert!(decode(br#"{"devices":[{"mac":"02:aa:bb:cc:dd:ee"}],"ipv6":"direct"}"#).is_ok());
        assert!(decode(br#"{"clientIPv4":"192.168.50.2","ipv6":"direct"}"#).is_ok());
        for raw in [br#"{"scope":"gateway"}"#.as_slice(),br#"{"scope":"gateway","ipv6":null}"#,br#"{"scope":"gateway","ipv6":"block"}"#,br#"{"scope":"gateway","ipv6":"follow"}"#,br#"{"scope":"gateway","devices":[{"mac":"02:aa:bb:cc:dd:ee"}],"ipv6":"direct"}"#,br#"{"scope":"gateway","lanIPv4Prefixes":["10.0.0.0/8"],"ipv6":"direct"}"#,br#"{"devices":[["02:aa:bb:cc:dd:ee"]],"ipv6":"direct"}"#,br#"{"devices":[{"mac":"02:aa:bb:cc:dd:ee","mac":"02:aa:bb:cc:dd:ff"}],"ipv6":"direct"}"#,br#"{"devices":null,"ipv6":"direct"}"#,br#"{"devices":[],"ipv6":"direct"}"#,br#"{"devices":[{"mac":"02:aa:bb:cc:dd:ee"}],"clientIPv4":"192.168.50.2","ipv6":"direct"}"#,br#"{"devices":[{"mac":"02:aa:bb:cc:dd:ee"}],"clientIPv6":"2001:db8::1","ipv6":"direct"}"#]{assert!(decode(raw).is_err());}
        let devices = serde_json::json!({"devices":(0..65).map(|i|serde_json::json!({"mac":format!("02:00:00:00:00:{i:02x}")})).collect::<Vec<_>>(),"ipv6":"direct"});
        assert!(decode(&serde_json::to_vec(&devices).unwrap()).is_err());
    }
}

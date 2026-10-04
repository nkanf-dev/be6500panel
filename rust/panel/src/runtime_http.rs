//! Authenticated fixed-service runtime HTTP projection. The existing service
//! borrows an actual exclusive Manager; no constructor/startup activation,
//! arbitrary argv/path/PID, artifact fetch or extra thread is introduced.
use crate::{
    http::{self, Method},
    runtime_manager::{Failure, Manager, ManagerError, ServiceId, Status},
    runtime_process::ProcessError,
    runtime_store::StoreError,
};
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
const MAX_RESPONSE: usize = 8 << 20;
const JSON_TYPE: &str = "application/json; charset=utf-8";
pub struct RuntimeHttp {
    manager: Manager,
}
impl std::fmt::Debug for RuntimeHttp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RuntimeHttp([owned])")
    }
}
impl RuntimeHttp {
    pub fn new(manager: Manager) -> Self {
        Self { manager }
    }
    pub fn close(&mut self) -> Result<(), ManagerError> {
        self.manager.close()
    }
    pub(crate) fn rule_status(&mut self) -> Result<Status, ManagerError> {
        self.manager.status(ServiceId::SingBox)
    }
    pub(crate) fn rule_config(
        &self,
    ) -> Result<Option<crate::runtime_manager::ConfigSnapshot>, ManagerError> {
        self.manager.config(ServiceId::SingBox)
    }
    pub(crate) fn configure_rules(
        &mut self,
        generation: u64,
        bytes: &[u8],
    ) -> Result<Status, ManagerError> {
        self.manager
            .configure(ServiceId::SingBox, generation, bytes, None)
    }
    pub(crate) fn write_failure(
        &mut self,
        writer: &mut impl Write,
        error: ManagerError,
    ) -> io::Result<()> {
        failure(writer, &mut self.manager, error, false)
    }

    pub(crate) fn respond(
        &mut self,
        writer: &mut impl Write,
        target: &str,
        method: Method,
        body: &[u8],
    ) -> io::Result<()> {
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        let head = method == Method::Head;
        if path == "/api/runtime" {
            if method == Method::Post {
                return method_error(writer, head, "GET, HEAD");
            }
            let mut services = Vec::with_capacity(2);
            for service in [ServiceId::SingBox, ServiceId::Frpc] {
                match self.manager.status(service) {
                    Ok(status) => services.push(WireStatus::from(status)),
                    Err(error) => return failure(writer, &mut self.manager, error, head),
                }
            }
            return write_json(
                writer,
                200,
                "OK",
                &RuntimeList {
                    enabled: true,
                    services,
                },
                head,
            );
        }
        if path == "/api/runtime/config" {
            if method == Method::Post {
                return method_error(writer, head, "GET, HEAD");
            }
            let Some(service) = query_service(query) else {
                return error(
                    writer,
                    400,
                    "Bad Request",
                    "invalid_service",
                    "Service type is invalid.",
                    head,
                );
            };
            return match self.manager.config(service) {
                Ok(Some(config)) => {
                    let Ok(text) = std::str::from_utf8(config.bytes()) else {
                        return error(
                            writer,
                            500,
                            "Internal Server Error",
                            "runtime_operation_failed",
                            "Runtime configuration is unavailable.",
                            head,
                        );
                    };
                    write_json(
                        writer,
                        200,
                        "OK",
                        &ConfigResponse {
                            service,
                            config: text,
                            generation: config.identity.generation,
                        },
                        head,
                    )
                }
                Ok(None) => error(
                    writer,
                    409,
                    "Conflict",
                    "not_configured",
                    "Service is not configured.",
                    head,
                ),
                Err(failed) => failure(writer, &mut self.manager, failed, head),
            };
        }
        if method != Method::Post {
            return method_error(writer, head, "POST");
        }
        if !query.is_empty() {
            return error(
                writer,
                400,
                "Bad Request",
                "invalid_input",
                "Request parameters are invalid.",
                false,
            );
        }
        let result = match path {
            "/api/runtime/configure" => {
                let input: Configure = match decode(body) {
                    Ok(input) => input,
                    Err(()) => return invalid_json(writer),
                };
                if input.config.len() > crate::runtime_store::MAX_CONFIG_BYTES {
                    return error(
                        writer,
                        413,
                        "Payload Too Large",
                        "body_too_large",
                        "Runtime configuration exceeds its limit.",
                        false,
                    );
                }
                self.manager.configure(
                    input.service,
                    input.generation,
                    input.config.as_bytes(),
                    None,
                )
            }
            "/api/runtime/restore" => {
                let input: Restore = match decode(body) {
                    Ok(input) => input,
                    Err(()) => return invalid_json(writer),
                };
                self.manager.restore(input.service, input.generation)
            }
            "/api/runtime/start" | "/api/runtime/stop" | "/api/runtime/restart" => {
                let input: Action = match decode(body) {
                    Ok(input) => input,
                    Err(()) => return invalid_json(writer),
                };
                match path {
                    "/api/runtime/start" => self.manager.start(input.service),
                    "/api/runtime/stop" => self.manager.stop(input.service),
                    _ => self.manager.restart(input.service),
                }
            }
            "/api/runtime/acquire" => {
                return error(
                    writer,
                    503,
                    "Service Unavailable",
                    "artifact_acquisition_unavailable",
                    "Artifact acquisition is not implemented.",
                    false,
                );
            }
            _ => {
                return error(
                    writer,
                    404,
                    "Not Found",
                    "not_found",
                    "Runtime route is unavailable.",
                    head,
                );
            }
        };
        match result {
            Ok(status) => write_json(writer, 200, "OK", &WireStatus::from(status), false),
            Err(error) => failure(writer, &mut self.manager, error, false),
        }
    }
}
pub(crate) fn is_runtime_path(path: &str) -> bool {
    matches!(
        path,
        "/api/runtime"
            | "/api/runtime/config"
            | "/api/runtime/configure"
            | "/api/runtime/start"
            | "/api/runtime/stop"
            | "/api/runtime/restart"
            | "/api/runtime/restore"
            | "/api/runtime/acquire"
    )
}
pub(crate) fn unavailable(writer: &mut impl Write, head: bool) -> io::Result<()> {
    error(
        writer,
        503,
        "Service Unavailable",
        "runtime_unavailable",
        "Runtime management is unavailable.",
        head,
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Action {
    service: ServiceId,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Configure {
    service: ServiceId,
    config: String,
    generation: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Restore {
    service: ServiceId,
    generation: u64,
}
fn decode<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T, ()> {
    if body.len() > http::MAX_RUNTIME_BODY_BYTES
        || body
            .iter()
            .find(|byte| !byte.is_ascii_whitespace())
            .is_none_or(|byte| *byte != b'{')
    {
        return Err(());
    }
    serde_json::from_slice(body).map_err(|_| ())
}
fn query_service(query: &str) -> Option<ServiceId> {
    let value = query.strip_prefix("service=")?;
    if value.contains(['&', '%', '+']) {
        return None;
    }
    match value {
        "sing-box" => Some(ServiceId::SingBox),
        "frpc" => Some(ServiceId::Frpc),
        _ => None,
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WireStatus {
    service: ServiceId,
    state: crate::runtime_manager::State,
    generation: u64,
    configured: bool,
    artifact_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pid: Option<u32>,
    rss_bytes: u64,
    rss_available: bool,
    desired: bool,
    restarts: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_code: Option<&'static str>,
    restored: bool,
    needs_recovery: bool,
    ready: bool,
    resource_suspended: bool,
    durability_uncertain: bool,
}
impl From<Status> for WireStatus {
    fn from(status: Status) -> Self {
        Self {
            service: status.service,
            state: status.state,
            generation: status.generation,
            configured: status.configured,
            artifact_available: status.artifact_available,
            pid: status.pid,
            rss_bytes: 0,
            rss_available: false,
            desired: status.desired,
            restarts: 0,
            error_code: status.error_code,
            restored: status.restored,
            needs_recovery: status.needs_recovery,
            ready: status.ready,
            resource_suspended: status.resource_suspended,
            durability_uncertain: status.durability_uncertain,
        }
    }
}
#[derive(Serialize)]
struct RuntimeList {
    enabled: bool,
    services: Vec<WireStatus>,
}
#[derive(Serialize)]
struct ConfigResponse<'a> {
    service: ServiceId,
    config: &'a str,
    generation: u64,
}
#[derive(Serialize)]
struct ApiError {
    code: &'static str,
    message: &'static str,
}
#[derive(Serialize)]
struct ErrorResponse {
    error: ApiError,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<WireStatus>,
}
fn failure(
    writer: &mut impl Write,
    manager: &mut Manager,
    failed: ManagerError,
    head: bool,
) -> io::Result<()> {
    let (status, code) = match failed.failure {
        Failure::Generation => (409, "generation_conflict"),
        Failure::NotConfigured => (409, "not_configured"),
        Failure::ArtifactUnavailable => (409, "artifact_unavailable"),
        Failure::Store(StoreError::InsufficientSpace) | Failure::Store(StoreError::Measurement) => {
            (409, "storage_insufficient")
        }
        Failure::Process(ProcessError::CheckFailed) => (422, "config_check_failed"),
        Failure::Process(ProcessError::CheckDeadline)
        | Failure::Process(ProcessError::StopDeadline) => (504, "operation_timeout"),
        Failure::Process(ProcessError::Cancelled) => (409, "operation_cancelled"),
        Failure::Hook(crate::runtime_manager::HookStage::Readiness, _) | Failure::NotReady => {
            (503, "readiness_failed")
        }
        Failure::Hook(crate::runtime_manager::HookStage::Cleanup, _) => (503, "cleanup_failed"),
        _ => (500, "runtime_operation_failed"),
    };
    let observed = failed
        .service
        .and_then(|service| manager.status(service).ok())
        .map(WireStatus::from);
    write_json(
        writer,
        status,
        reason(status),
        &ErrorResponse {
            error: ApiError {
                code,
                message: "Runtime operation did not complete; read the returned state before retrying.",
            },
            status: observed,
        },
        head,
    )
}
fn reason(status: u16) -> &'static str {
    match status {
        400 => "Bad Request",
        409 => "Conflict",
        413 => "Payload Too Large",
        422 => "Unprocessable Entity",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "Internal Server Error",
    }
}
fn invalid_json(writer: &mut impl Write) -> io::Result<()> {
    error(
        writer,
        400,
        "Bad Request",
        "invalid_json",
        "JSON fields or types do not match the request contract.",
        false,
    )
}
fn error(
    writer: &mut impl Write,
    status: u16,
    reason: &str,
    code: &'static str,
    message: &'static str,
    head: bool,
) -> io::Result<()> {
    write_json(
        writer,
        status,
        reason,
        &ErrorResponse {
            error: ApiError { code, message },
            status: None,
        },
        head,
    )
}
fn method_error(writer: &mut impl Write, head: bool, allow: &str) -> io::Result<()> {
    http::write_response_extra(writer,405,"Method Not Allowed",b"{\"error\":{\"code\":\"method_not_allowed\",\"message\":\"Method is not allowed for this endpoint.\"}}\n",head,JSON_TYPE,&[("Allow",allow)])
}
struct Counter(usize);
impl Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .filter(|length| *length < MAX_RESPONSE)
            .ok_or_else(|| io::Error::other("response bound"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn write_json(
    writer: &mut impl Write,
    status: u16,
    reason: &str,
    value: &impl Serialize,
    head: bool,
) -> io::Result<()> {
    let mut count = Counter(0);
    if serde_json::to_writer(&mut count, value).is_err() {
        return error(
            writer,
            503,
            "Service Unavailable",
            "response_too_large",
            "Runtime response exceeds its limit.",
            head,
        );
    }
    let mut output = io::BufWriter::with_capacity(8192, writer);
    http::write_headers(&mut output, status, reason, JSON_TYPE, (count.0 + 1) as u64)?;
    if !head {
        serde_json::to_writer(&mut output, value).map_err(io::Error::other)?;
        output.write_all(b"\n")?;
    }
    output.flush()
}

//! Authenticated fixed-service runtime HTTP projection. The existing service
//! borrows an actual exclusive Manager; no constructor/startup activation,
//! arbitrary argv/path/PID, artifact fetch or extra thread is introduced.
use crate::artifact_source::{SourceError, SourcePolicy};
use crate::artifact_stage::StageError;
use crate::runtime_intent::{DesiredServices, IntentError, RuntimeIntent};
use crate::{
    http::{self, Method},
    runtime_manager::{Failure, HookError, Manager, ManagerError, ServiceId, Status},
    runtime_process::ProcessError,
    runtime_store::StoreError,
};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};
const MAX_RESPONSE: usize = 8 << 20;
const JSON_TYPE: &str = "application/json; charset=utf-8";
#[derive(Default)]
struct Retry {
    attempts: u32,
    next: Option<Instant>,
    exhausted: bool,
    failure: Option<&'static str>,
}
impl Retry {
    fn defer_after(&mut self, entered: Instant, completed: Instant, delay: Duration) {
        self.next = Some(entered.max(completed) + delay);
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryResult {
    Withdrawn,
    Started,
    Deferred,
    Exhausted,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestoreError {
    Runtime(ManagerError),
    Source(SourceError),
    SourceUnavailable,
}
impl std::fmt::Display for RestoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for RestoreError {}
impl RestoreError {
    fn code(self) -> &'static str {
        match self {
            Self::Runtime(error) => match error.failure {
                Failure::OperationDeadline
                | Failure::Store(StoreError::OperationDeadline)
                | Failure::Process(ProcessError::OperationDeadline)
                | Failure::Process(ProcessError::CheckDeadline)
                | Failure::Process(ProcessError::StopDeadline) => "operation_timeout",
                Failure::Cancelled
                | Failure::Store(StoreError::Cancelled)
                | Failure::Process(ProcessError::Cancelled) => "operation_cancelled",
                _ => error.failure.code(),
            },
            Self::SourceUnavailable => "artifact_acquisition_unavailable",
            Self::Source(SourceError::Deadline | SourceError::Stage(StageError::Deadline)) => {
                "operation_timeout"
            }
            Self::Source(SourceError::Cancelled | SourceError::Stage(StageError::Cancelled)) => {
                "operation_cancelled"
            }
            Self::Source(_) => "artifact_acquire_failed",
        }
    }
}
struct ArtifactRoot {
    path: PathBuf,
    file: File,
    identity: (u64, u64),
}
impl ArtifactRoot {
    fn open(path: &Path) -> Result<Self, SourceError> {
        if !path.is_absolute()
            || path.components().any(|part| {
                matches!(
                    part,
                    std::path::Component::ParentDir | std::path::Component::CurDir
                )
            })
        {
            return Err(SourceError::Input);
        }
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)
            .map_err(|_| SourceError::Input)?;
        let metadata = file.metadata().map_err(|_| SourceError::Input)?;
        if !metadata.is_dir()
            || metadata.mode() & 0o7777 != 0o700
            || metadata.uid() != unsafe { libc::geteuid() }
        {
            return Err(SourceError::Input);
        }
        Ok(Self {
            path: path.into(),
            file,
            identity: (metadata.dev(), metadata.ino()),
        })
    }
    fn checked(&self) -> Result<(), SourceError> {
        let current = Self::open(&self.path)?;
        let retained = self.file.metadata().map_err(|_| SourceError::Input)?;
        if current.identity != self.identity || (retained.dev(), retained.ino()) != self.identity {
            return Err(SourceError::Input);
        }
        Ok(())
    }
}
struct Acquisition {
    source: SourcePolicy,
    roots: [ArtifactRoot; 2],
}
pub struct RuntimeHttp {
    manager: Manager,
    intent: Option<RuntimeIntent>,
    desired: [bool; 2],
    retry: [Retry; 2],
    restarts: [u32; 2],
    closing: bool,
    startup_blocked: bool,
    recovery_enabled: bool,
    intent_uncertain: bool,
    acquisition: Option<Acquisition>,
    capture: Option<Box<dyn crate::capture_http::CaptureControl>>,
    cleanup_requests: u64,
}
fn index(service: ServiceId) -> usize {
    match service {
        ServiceId::SingBox => 0,
        ServiceId::Frpc => 1,
    }
}
const SERVICES: [ServiceId; 2] = [ServiceId::SingBox, ServiceId::Frpc];
const MAX_RECOVERY_ATTEMPTS: u32 = 8;
impl std::fmt::Debug for RuntimeHttp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RuntimeHttp([owned])")
    }
}
impl RuntimeHttp {
    pub(crate) fn set_startup_blocked(&mut self, blocked: bool) {
        self.startup_blocked = blocked;
    }
    pub(crate) fn cleanup_sequence(&self) -> u64 {
        self.cleanup_requests
    }
    pub fn new(manager: Manager) -> Self {
        Self {
            manager,
            intent: None,
            desired: [false; 2],
            retry: [Retry::default(), Retry::default()],
            restarts: [0; 2],
            closing: false,
            startup_blocked: false,
            recovery_enabled: false,
            intent_uncertain: false,
            acquisition: None,
            capture: None,
            cleanup_requests: 0,
        }
    }
    pub fn load_capture<O: crate::readiness_tun::Observer + 'static>(
        &mut self,
        handle: crate::capture_runtime::CaptureHandle<O>,
    ) -> Result<(), HookError> {
        if self.closing || self.capture.is_some() {
            return Err(HookError::Failed);
        }
        self.capture = Some(Box::new(handle));
        Ok(())
    }
    /// Trusted startup wiring only. Borrows this owner, performs no fetch/start
    /// and cannot be configured by an HTTP path/CA/bootstrap request.
    pub fn load_artifact_source(
        &mut self,
        source: SourcePolicy,
        sing_box_root: &Path,
        frpc_root: &Path,
    ) -> Result<(), SourceError> {
        if self.closing || self.acquisition.is_some() {
            return Err(SourceError::Input);
        }
        let roots = [
            ArtifactRoot::open(sing_box_root)?,
            ArtifactRoot::open(frpc_root)?,
        ];
        for service in SERVICES {
            if self
                .manager
                .artifact_root(service)
                .is_some_and(|root| root != roots[index(service)].path)
            {
                return Err(SourceError::Input);
            }
        }
        self.acquisition = Some(Acquisition { source, roots });
        Ok(())
    }
    fn acquire(&mut self, writer: &mut impl Write, input: Acquire) -> io::Result<()> {
        match self.acquire_artifact(
            input.service,
            input.artifact.into_artifact(),
            input.generation,
        ) {
            Ok(status) => write_json(writer, 200, "OK", &self.wire(status), false),
            Err(RestoreError::Runtime(error)) => failure(writer, self, error, false),
            Err(RestoreError::Source(error)) => source_failure(writer, self, input.service, error),
            Err(RestoreError::SourceUnavailable) => error(
                writer,
                503,
                "Service Unavailable",
                "artifact_acquisition_unavailable",
                "Artifact acquisition is not configured.",
                false,
            ),
        }
    }
    /// Ordinary in-process acquisition used by explicit HTTP and startup. The
    /// saved request remains intent; only this actual fetch creates Stage trust.
    fn acquire_artifact(
        &mut self,
        service: ServiceId,
        artifact: crate::runtime_store::Artifact,
        generation: Option<u64>,
    ) -> Result<Status, RestoreError> {
        let operation_deadline = Instant::now() + Duration::from_secs(90);
        let acquisition = self
            .acquisition
            .as_ref()
            .ok_or(RestoreError::SourceUnavailable)?;
        acquisition
            .source
            .validate_artifact(&artifact)
            .map_err(RestoreError::Source)?;
        let root = &acquisition.roots[index(service)];
        root.checked().map_err(RestoreError::Source)?;
        let status = self
            .manager
            .status(service)
            .map_err(RestoreError::Runtime)?;
        let expected = generation.unwrap_or(status.generation);
        let admitted = self
            .manager
            .admit_artifact_acquisition(service, expected, &root.path)
            .map_err(RestoreError::Runtime)?;
        let fetch_budget = self.manager.acquisition_fetch_budget().map_err(|failure| {
            RestoreError::Runtime(ManagerError {
                service: Some(service),
                failure,
                recovery_failure: None,
                generation: expected,
                owned_pid: admitted.pid,
            })
        })?;
        let cancel = Arc::new(AtomicBool::new(false));
        let budget = crate::readiness_tun::Budget {
            deadline: operation_deadline.min(Instant::now() + fetch_budget),
            cancel: &cancel,
        };
        let stage = acquisition
            .source
            .fetch(&root.path, &artifact, &budget)
            .map_err(RestoreError::Source)?;
        root.checked().map_err(RestoreError::Source)?;
        self.manager
            .acquire_verified_stage(service, expected, stage, operation_deadline, cancel)
            .map_err(RestoreError::Runtime)
    }
    fn restore_service(&mut self, service: ServiceId) -> Result<Status, RestoreError> {
        let status = self
            .manager
            .status(service)
            .map_err(RestoreError::Runtime)?;
        if !status.configured {
            return Err(RestoreError::Runtime(ManagerError {
                service: Some(service),
                failure: Failure::NotConfigured,
                recovery_failure: None,
                generation: status.generation,
                owned_pid: status.pid,
            }));
        }
        if !status.artifact_available {
            let metadata =
                self.manager
                    .saved_artifact_request(service)
                    .ok_or(RestoreError::Runtime(ManagerError {
                        service: Some(service),
                        failure: Failure::ArtifactUnavailable,
                        recovery_failure: None,
                        generation: status.generation,
                        owned_pid: status.pid,
                    }))?;
            self.acquire_artifact(service, metadata, Some(status.generation))?;
        }
        self.manager.start(service).map_err(RestoreError::Runtime)
    }
    /// Load only. Even saved true intent executes no child/check/hook until
    /// the owner explicitly calls restore_saved or poll_recovery.
    pub fn load_saved_intent(&mut self, data_dir: &Path) -> Result<(), IntentError> {
        if self.closing || self.recovery_enabled || self.intent.is_some() {
            return Err(IntentError::Invalid);
        }
        for service in SERVICES {
            let status = self
                .manager
                .status(service)
                .map_err(|_| IntentError::Storage)?;
            if status.desired || status.pid.is_some() {
                return Err(IntentError::Invalid);
            }
        }
        let store = RuntimeIntent::open(data_dir)?;
        self.desired = SERVICES.map(|service| store.desired().get(service));
        self.intent = Some(store);
        Ok(())
    }
    fn save_intent(&mut self, service: ServiceId, value: bool) -> Result<(), IntentError> {
        // An explicit off intent is effective even if private storage fails.
        self.desired[index(service)] = value;
        if value {
            self.recovery_enabled = true;
        }
        self.retry[index(service)] = Retry::default();
        if let Some(store) = &mut self.intent {
            let desired = DesiredServices {
                sing_box: self.desired[0],
                frpc: self.desired[1],
            };
            let result = store.save_desired(desired);
            self.intent_uncertain = !matches!(&result,Ok(outcome) if outcome.durable);
            let outcome = result?;
            if !outcome.durable {
                return Err(IntentError::Storage);
            }
        }
        Ok(())
    }
    /// Explicit startup integration call, not a constructor or GET side effect.
    pub fn restore_saved(&mut self) -> Vec<(ServiceId, Result<Status, RestoreError>)> {
        let mut outcomes = Vec::with_capacity(2);
        if self.closing || self.startup_blocked {
            return outcomes;
        }
        self.recovery_enabled = true;
        for service in SERVICES {
            if self.desired[index(service)] {
                self.retry[index(service)] = Retry::default();
                if self
                    .manager
                    .status(service)
                    .is_ok_and(|status| !status.configured)
                {
                    continue;
                }
                let result = self.restore_service(service);
                if let Err(error) = &result {
                    self.retry[index(service)].failure = Some(error.code());
                    self.retry[index(service)].attempts = 1;
                    self.retry[index(service)].next = Some(Instant::now() + Duration::from_secs(2));
                }
                outcomes.push((service, result));
            }
        }
        outcomes
    }
    /// Caller-driven finite exit recovery. No timer thread, busy loop or GET
    /// action. Cleanup must complete before the old child is reaped/restarted.
    pub fn poll_recovery(&mut self, now: Instant) -> Vec<(ServiceId, RecoveryResult)> {
        let mut results = Vec::with_capacity(2);
        if self.closing || self.startup_blocked || !self.recovery_enabled {
            return results;
        }
        for service in SERVICES {
            let slot = index(service);
            let Ok(status) = self.manager.status(service) else {
                continue;
            };
            let exited = status.pid.is_some() && status.error_code == Some(Failure::Exited.code());
            let off_cleanup = !self.desired[slot] && status.pid.is_some() && status.needs_recovery;
            if !exited && !off_cleanup && (!self.desired[slot] || status.pid.is_some()) {
                continue;
            }
            if !status.configured && self.desired[slot] && !exited && !off_cleanup {
                continue;
            }
            let retry = &mut self.retry[slot];
            if retry.exhausted {
                continue;
            }
            if retry.next.is_some_and(|deadline| deadline > now) {
                continue;
            }
            if retry.next.is_none() {
                // Withdraw dead-core traffic immediately. Only launch attempts
                // wait for backoff; failed withdrawal is also finite/backed off.
                if exited || off_cleanup {
                    let cleaned = if exited {
                        self.manager.handle_exit(service)
                    } else {
                        self.manager.stop(service)
                    };
                    match cleaned {
                        Ok(_) => {
                            results.push((service, RecoveryResult::Withdrawn));
                        }
                        Err(_) => {
                            retry.attempts += 1;
                            retry.defer_after(now, Instant::now(), Duration::from_secs(2));
                            results.push((service, RecoveryResult::Deferred));
                            continue;
                        }
                    }
                    if !self.desired[slot] {
                        continue;
                    }
                }
                if retry.attempts >= MAX_RECOVERY_ATTEMPTS {
                    retry.exhausted = true;
                    results.push((service, RecoveryResult::Exhausted));
                    continue;
                }
                retry.defer_after(now, Instant::now(), Duration::from_secs(2));
                continue;
            }
            if retry.attempts >= MAX_RECOVERY_ATTEMPTS {
                retry.exhausted = true;
                retry.next = None;
                results.push((service, RecoveryResult::Exhausted));
                continue;
            }
            retry.attempts += 1;
            let seconds = (2u64 << retry.attempts.saturating_sub(1).min(5)).min(60);
            let cleaned = if exited {
                self.manager.handle_exit(service)
            } else if off_cleanup {
                self.manager.stop(service)
            } else {
                Ok(status)
            };
            let Ok(cleaned) = cleaned else {
                retry.defer_after(now, Instant::now(), Duration::from_secs(seconds));
                results.push((service, RecoveryResult::Deferred));
                continue;
            };
            if exited || off_cleanup {
                results.push((service, RecoveryResult::Withdrawn));
            }
            if !self.desired[slot] {
                retry.next = None;
                continue;
            }
            if cleaned.pid.is_some() {
                retry.defer_after(now, Instant::now(), Duration::from_secs(seconds));
                results.push((service, RecoveryResult::Deferred));
                continue;
            }
            let restored = self.restore_service(service);
            let retry = &mut self.retry[slot];
            match restored {
                Ok(_) => {
                    retry.next = None;
                    retry.failure = None;
                    self.restarts[slot] = self.restarts[slot].saturating_add(1);
                    results.push((service, RecoveryResult::Started));
                }
                Err(error) => {
                    retry.defer_after(now, Instant::now(), Duration::from_secs(seconds));
                    retry.failure = Some(error.code());
                    results.push((service, RecoveryResult::Deferred));
                }
            }
        }
        results
    }
    fn wire(&self, status: Status) -> WireStatus {
        let slot = index(status.service);
        let mut wire = WireStatus::from(status);
        if !self.closing && (self.intent.is_some() || self.recovery_enabled) {
            wire.desired = self.desired[slot];
        }
        wire.version = self
            .manager
            .artifact_version(wire.service)
            .map(str::to_owned);
        wire.restarts = self.restarts[slot];
        wire.recovery_attempts = self.retry[slot].attempts;
        wire.recovery_exhausted = self.retry[slot].exhausted;
        if self.retry[slot].failure.is_some() {
            wire.error_code = self.retry[slot].failure;
        }
        wire.intent_durability_uncertain = self.intent_uncertain;
        wire.needs_recovery |=
            self.retry[slot].exhausted || self.retry[slot].next.is_some() || self.intent_uncertain;
        wire
    }
    pub fn close(&mut self) -> Result<(), ManagerError> {
        // Shutdown cancels retries but preserves saved startup intent.
        self.closing = true;
        self.manager.close()
    }
    pub(crate) fn subscription_import_allowed(&self) -> bool {
        !self.closing && !self.startup_blocked && !crate::shutdown::requested()
    }
    pub(crate) fn fetch_subscription(
        &self,
        source: &str,
        deadline: Instant,
    ) -> Result<Vec<u8>, SourceError> {
        if self.closing || self.startup_blocked {
            return Err(SourceError::Cancelled);
        }
        let acquisition = self.acquisition.as_ref().ok_or(SourceError::Input)?;
        let cancel = AtomicBool::new(false);
        let budget = crate::readiness_tun::Budget {
            deadline,
            cancel: &cancel,
        };
        acquisition.source.fetch_subscription(source, &budget)
    }
    pub(crate) fn selection_scope(
        &self,
        budget: &crate::readiness_tun::Budget<'_>,
    ) -> Result<crate::capture_lan::Snapshot, HookError> {
        if !self.subscription_import_allowed() {
            return Err(HookError::Cancelled);
        }
        self.capture
            .as_deref()
            .ok_or(HookError::Failed)?
            .selection_scope(budget)
    }
    pub(crate) fn selection_endpoints(
        &self,
        host: &str,
        budget: &crate::readiness_tun::Budget<'_>,
    ) -> Result<Vec<String>, SourceError> {
        if !self.subscription_import_allowed() {
            return Err(SourceError::Cancelled);
        }
        self.acquisition
            .as_ref()
            .ok_or(SourceError::Input)?
            .source
            .selection_endpoints(host, budget)
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
        if self.startup_blocked {
            return Err(ManagerError {
                service: Some(ServiceId::SingBox),
                failure: Failure::CheckPending,
                recovery_failure: None,
                generation,
                owned_pid: None,
            });
        }
        self.manager
            .configure(ServiceId::SingBox, generation, bytes, None)
    }
    pub(crate) fn write_failure(
        &mut self,
        writer: &mut impl Write,
        error: ManagerError,
    ) -> io::Result<()> {
        failure(writer, self, error, false)
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
        if self.startup_blocked && method == Method::Post && path != "/api/runtime/stop" {
            return error(
                writer,
                503,
                "Service Unavailable",
                "startup_cleanup_pending",
                "Startup capture withdrawal must complete before activation.",
                false,
            );
        }
        if self.closing && method == Method::Post && path != "/api/runtime/stop" {
            return error(
                writer,
                503,
                "Service Unavailable",
                "runtime_shutting_down",
                "Runtime shutdown is in progress.",
                false,
            );
        }
        if path == "/api/proxy/capture" {
            if method == Method::Delete
                && query.is_empty()
                && body.is_empty()
                && self.capture.is_some()
            {
                self.cleanup_requests = self.cleanup_requests.wrapping_add(1);
            }
            return crate::capture_http::respond(
                writer,
                &mut self.manager,
                self.capture.as_deref(),
                target,
                method,
                body,
            );
        }
        if path == "/api/runtime" {
            if method == Method::Post {
                return method_error(writer, head, "GET, HEAD");
            }
            let mut services = Vec::with_capacity(2);
            for service in [ServiceId::SingBox, ServiceId::Frpc] {
                match self.manager.status(service) {
                    Ok(status) => {
                        services.push(self.wire(status));
                    }
                    Err(error) => return failure(writer, self, error, head),
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
                Err(failed) => failure(writer, self, failed, head),
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
                if path == "/api/runtime/stop" {
                    self.cleanup_requests = self.cleanup_requests.wrapping_add(1);
                    let saved = self.save_intent(input.service, false);
                    let stopped = self.manager.stop(input.service);
                    if let Err(failed) = stopped {
                        return failure(writer, self, failed, false);
                    }
                    if saved.is_err() {
                        return intent_failure(writer, self, input.service);
                    }
                    stopped
                } else {
                    let result = if path == "/api/runtime/start" {
                        self.manager.start(input.service)
                    } else {
                        self.manager.restart(input.service)
                    };
                    if result.is_ok() && self.save_intent(input.service, true).is_err() {
                        return intent_failure(writer, self, input.service);
                    }
                    result
                }
            }
            "/api/runtime/acquire" => {
                if self.acquisition.is_none() {
                    return error(
                        writer,
                        503,
                        "Service Unavailable",
                        "artifact_acquisition_unavailable",
                        "Artifact acquisition is not configured.",
                        false,
                    );
                }
                let input: Acquire = match decode(body) {
                    Ok(input) => input,
                    Err(()) => return invalid_json(writer),
                };
                return self.acquire(writer, input);
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
            Ok(status) => write_json(writer, 200, "OK", &self.wire(status), false),
            Err(error) => failure(writer, self, error, false),
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
struct Acquire {
    service: ServiceId,
    #[serde(deserialize_with = "artifact_object")]
    artifact: ArtifactInput,
    #[serde(default, deserialize_with = "optional_generation")]
    generation: Option<u64>,
}
fn artifact_object<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<ArtifactInput, D::Error> {
    struct Object;
    impl<'de> serde::de::Visitor<'de> for Object {
        type Value = ArtifactInput;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("artifact object with required unique fields")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(self, map: M) -> Result<Self::Value, M::Error> {
            ArtifactInput::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(Object)
}
fn optional_generation<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
    u64::deserialize(d).map(Some)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactInput {
    url: String,
    sha256: String,
    compression: String,
    version: String,
}
impl ArtifactInput {
    fn into_artifact(self) -> crate::runtime_store::Artifact {
        crate::runtime_store::Artifact {
            url: self.url,
            sha256: self.sha256,
            compression: self.compression,
            version: self.version,
        }
    }
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
    version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pid: Option<u32>,
    rss_bytes: u64,
    rss_available: bool,
    desired: bool,
    restarts: u32,
    recovery_attempts: u32,
    recovery_exhausted: bool,
    intent_durability_uncertain: bool,
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
            version: None,
            pid: status.pid,
            rss_bytes: 0,
            rss_available: false,
            desired: status.desired,
            restarts: 0,
            recovery_attempts: 0,
            recovery_exhausted: false,
            intent_durability_uncertain: false,
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
    runtime: &mut RuntimeHttp,
    failed: ManagerError,
    head: bool,
) -> io::Result<()> {
    let (status, code) = match failed.failure {
        Failure::Generation => (409, "generation_conflict"),
        Failure::InvalidInput => (400, "invalid_input"),
        Failure::NotConfigured => (409, "not_configured"),
        Failure::ArtifactUnavailable => (409, "artifact_unavailable"),
        Failure::CheckPending | Failure::Process(ProcessError::Busy) => (409, "operation_busy"),
        Failure::ArtifactStage(StageError::EncodedLimit) => (422, "artifact_compressed_limit"),
        Failure::ArtifactStage(StageError::DecodedLimit) => (422, "artifact_uncompressed_limit"),
        Failure::ArtifactStage(StageError::Measurement)
        | Failure::ArtifactStage(StageError::InsufficientSpace) => (409, "storage_insufficient"),
        Failure::Store(StoreError::InsufficientSpace) | Failure::Store(StoreError::Measurement) => {
            (409, "storage_insufficient")
        }
        Failure::Process(ProcessError::CheckFailed) => (422, "config_check_failed"),
        Failure::OperationDeadline
        | Failure::Store(StoreError::OperationDeadline)
        | Failure::Process(ProcessError::OperationDeadline)
        | Failure::Process(ProcessError::CheckDeadline)
        | Failure::Process(ProcessError::StopDeadline) => (504, "operation_timeout"),
        Failure::Cancelled
        | Failure::Store(StoreError::Cancelled)
        | Failure::Process(ProcessError::Cancelled) => (409, "operation_cancelled"),
        Failure::Hook(crate::runtime_manager::HookStage::Readiness, _) | Failure::NotReady => {
            (503, "readiness_failed")
        }
        Failure::Hook(crate::runtime_manager::HookStage::Cleanup, _) => (503, "cleanup_failed"),
        _ => (500, "runtime_operation_failed"),
    };
    let observed = failed
        .service
        .and_then(|service| runtime.manager.status(service).ok())
        .map(|status| runtime.wire(status));
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
fn source_failure(
    writer: &mut impl Write,
    runtime: &mut RuntimeHttp,
    service: ServiceId,
    failed: SourceError,
) -> io::Result<()> {
    let (status, code) = match failed {
        SourceError::Input => (400, "invalid_input"),
        SourceError::Deadline | SourceError::Stage(StageError::Deadline) => {
            (504, "operation_timeout")
        }
        SourceError::Cancelled | SourceError::Stage(StageError::Cancelled) => {
            (409, "operation_cancelled")
        }
        SourceError::Stage(StageError::EncodedLimit) => (422, "artifact_compressed_limit"),
        SourceError::Stage(StageError::DecodedLimit) => (422, "artifact_uncompressed_limit"),
        SourceError::Stage(StageError::InsufficientSpace)
        | SourceError::Stage(StageError::Measurement) => (409, "storage_insufficient"),
        _ => (422, "artifact_acquire_failed"),
    };
    let observed = runtime
        .manager
        .status(service)
        .ok()
        .map(|status| runtime.wire(status));
    write_json(
        writer,
        status,
        reason(status),
        &ErrorResponse {
            error: ApiError {
                code,
                message: "Artifact acquisition did not complete; current runtime state is returned.",
            },
            status: observed,
        },
        false,
    )
}
fn intent_failure(
    writer: &mut impl Write,
    runtime: &mut RuntimeHttp,
    service: ServiceId,
) -> io::Result<()> {
    let status = runtime
        .manager
        .status(service)
        .ok()
        .map(|status| runtime.wire(status));
    write_json(
        writer,
        500,
        "Internal Server Error",
        &ErrorResponse {
            error: ApiError {
                code: "storage_failed",
                message: "Runtime intent persistence is unconfirmed; current runtime state is returned.",
            },
            status,
        },
        false,
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

#[cfg(test)]
mod startup_retry_tests {
    use super::*;
    #[test]
    fn slow_failure_gets_full_backoff_from_completion_not_entry() {
        let entered = Instant::now();
        let completed = entered + Duration::from_secs(45);
        let mut retry = Retry::default();
        retry.defer_after(entered, completed, Duration::from_secs(4));
        assert_eq!(retry.next, Some(completed + Duration::from_secs(4)));
        assert!(retry.next.unwrap() > completed);
        // Synthetic caller time can be later than the real clock in fixed
        // recovery fixtures. Never make that caller's next attempt immediate.
        retry.defer_after(completed, entered, Duration::from_secs(8));
        assert_eq!(retry.next, Some(completed + Duration::from_secs(8)));
    }
    #[test]
    fn saved_restore_error_keeps_nested_stage_timeout_and_cancellation() {
        for (error, code) in [
            (SourceError::Deadline, "operation_timeout"),
            (
                SourceError::Stage(StageError::Deadline),
                "operation_timeout",
            ),
            (SourceError::Cancelled, "operation_cancelled"),
            (
                SourceError::Stage(StageError::Cancelled),
                "operation_cancelled",
            ),
        ] {
            assert_eq!(RestoreError::Source(error).code(), code);
        }
        for failure in [
            Failure::OperationDeadline,
            Failure::Store(StoreError::OperationDeadline),
            Failure::Process(ProcessError::OperationDeadline),
        ] {
            let error = ManagerError {
                service: Some(ServiceId::SingBox),
                failure,
                recovery_failure: None,
                generation: 1,
                owned_pid: None,
            };
            assert_eq!(RestoreError::Runtime(error).code(), "operation_timeout");
        }
        for failure in [
            Failure::Cancelled,
            Failure::Store(StoreError::Cancelled),
            Failure::Process(ProcessError::Cancelled),
        ] {
            let error = ManagerError {
                service: Some(ServiceId::SingBox),
                failure,
                recovery_failure: None,
                generation: 1,
                owned_pid: None,
            };
            assert_eq!(RestoreError::Runtime(error).code(), "operation_cancelled");
        }
    }
}

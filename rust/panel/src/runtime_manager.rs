//! Fixed-service runtime composition. This library has no HTTP, scheduler, download,
//! persisted PID adoption, or production readiness/capture executor.
//!
//! Local trusted code supplies artifact bindings and four finite native hooks.
//! Hooks must honor their deadline. Their success is an attestation, not a native
//! validator supplied by this module. The manager also observes its exact owned
//! child before and after readiness and binds all store proofs to checked bytes.
//! One `&mut Manager` is the mutation lane. Desired intent is in memory only;
//! loading a ready configuration never starts it. Call `handle_exit` explicitly
//! after observing exit; no automatic restart/watch/backoff is implemented.
//!
//! Explicit `close` can fail and must be retried on the same manager. Drop never
//! bypasses withdrawal: ProcessOwner retains an unclosed child rather than killing
//! it. Config snapshots and uncertain durable state are never pruned here.
use crate::artifact_stage::{RetainedStage, Stage, StageError};
use crate::runtime_process::{
    self as process, LaunchSpec, ProcessError, ProcessOwner, TrustedRoots,
};
use crate::runtime_store::{
    self as store, Candidate, ConfigRecord, ReadinessProof, RuntimeStore, StoreError,
    VerificationProof,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServiceId {
    #[serde(rename = "sing-box")]
    SingBox,
    #[serde(rename = "frpc")]
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
impl From<ServiceId> for store::ServiceId {
    fn from(service: ServiceId) -> Self {
        match service {
            ServiceId::SingBox => Self::SingBox,
            ServiceId::Frpc => Self::Frpc,
        }
    }
}
impl From<ServiceId> for process::ServiceId {
    fn from(service: ServiceId) -> Self {
        match service {
            ServiceId::SingBox => Self::SingBox,
            ServiceId::Frpc => Self::Frpc,
        }
    }
}
const SERVICES: [ServiceId; 2] = [ServiceId::SingBox, ServiceId::Frpc];

/// Provenance supplied by a trusted local artifact admission module, never by
/// a browser. This declaration itself does not validate an executable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactProvenance {
    TrustedLocalModule,
}
#[derive(Clone)]
pub struct ArtifactBinding {
    service: ServiceId,
    root: PathBuf,
    path: PathBuf,
    sha256: [u8; 32],
    provenance: ArtifactProvenance,
}
impl ArtifactBinding {
    pub fn trusted_local(
        service: ServiceId,
        root: impl Into<PathBuf>,
        path: impl Into<PathBuf>,
        sha256: [u8; 32],
        provenance: ArtifactProvenance,
    ) -> Self {
        Self {
            service,
            root: root.into(),
            path: path.into(),
            sha256,
            provenance,
        }
    }
}
impl fmt::Debug for ArtifactBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ArtifactBinding")
            .field("service", &self.service)
            .field("provenance", &self.provenance)
            .finish_non_exhaustive()
    }
}
#[derive(Debug, Default)]
pub struct ArtifactBindings {
    pub sing_box: Option<ArtifactBinding>,
    pub frpc: Option<ArtifactBinding>,
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub process: process::Limits,
    pub readiness_timeout: Duration,
    pub resource_timeout: Duration,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            process: process::Limits::default(),
            readiness_timeout: Duration::from_secs(5),
            resource_timeout: Duration::from_secs(3),
        }
    }
}
impl Limits {
    fn validate(self) -> Result<(), Failure> {
        if self.readiness_timeout.is_zero()
            || self.readiness_timeout > Duration::from_secs(30)
            || self.resource_timeout.is_zero()
            || self.resource_timeout > Duration::from_secs(30)
        {
            return Err(Failure::InvalidInput);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookStage {
    PreStart,
    Readiness,
    Cleanup,
    Restore,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookError {
    Failed,
    Deadline,
    Cancelled,
}
/// Fixed diagnostics; no OS text, checker tail, config, artifact path or URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    InvalidInput,
    Closed,
    ArtifactUnavailable,
    ArtifactStage(StageError),
    NotConfigured,
    NotReady,
    Generation,
    CheckPending,
    Exited,
    OperationDeadline,
    Cancelled,
    Store(StoreError),
    Process(ProcessError),
    Hook(HookStage, HookError),
}
impl Failure {
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidInput => "invalid_input",
            Self::Closed => "closed",
            Self::ArtifactUnavailable => "artifact_unavailable",
            Self::ArtifactStage(_) => "artifact_stage_failed",
            Self::NotConfigured => "not_configured",
            Self::NotReady => "not_ready",
            Self::Generation => "generation_conflict",
            Self::CheckPending => "check_pending",
            Self::Exited => "owned_run_exited",
            Self::OperationDeadline => "operation_timeout",
            Self::Cancelled => "operation_cancelled",
            Self::Store(StoreError::Durability) => "durability_uncertain",
            Self::Store(_) => "storage_failed",
            Self::Process(ProcessError::CheckFailed) => "check_failed",
            Self::Process(ProcessError::CheckDeadline) => "check_deadline",
            Self::Process(ProcessError::Cancelled) => "check_cancelled",
            Self::Process(_) => "process_failed",
            Self::Hook(HookStage::PreStart, _) => "prestart_failed",
            Self::Hook(HookStage::Readiness, _) => "readiness_failed",
            Self::Hook(HookStage::Cleanup, _) => "cleanup_failed",
            Self::Hook(HookStage::Restore, _) => "restore_failed",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManagerError {
    pub service: Option<ServiceId>,
    pub failure: Failure,
    pub recovery_failure: Option<Failure>,
    pub generation: u64,
    /// Only the manager's retained owned Run, never an adopted PID.
    pub owned_pid: Option<u32>,
}
impl fmt::Display for ManagerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.failure.code())
    }
}
impl std::error::Error for ManagerError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigIdentity {
    pub generation: u64,
    pub sha256: String,
}
/// Constructed only from the exact ProcessOwner's observed Run child.
/// Recovery may run an old config-N file while accepting the same hash under a
/// new monotonic generation. Both counters are exposed, never silently crossed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedRunIdentity {
    pid: u32,
    launch_generation: u64,
    accepted_generation: u64,
    sha256: String,
}
impl OwnedRunIdentity {
    pub fn pid(&self) -> u32 {
        self.pid
    }
    pub fn launch_generation(&self) -> u64 {
        self.launch_generation
    }
    pub fn accepted_generation(&self) -> u64 {
        self.accepted_generation
    }
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}
pub struct HookContext<'a> {
    pub service: ServiceId,
    pub config: &'a ConfigIdentity,
    /// Private fixed-store path, not a browser-supplied path. Native hooks can
    /// perform a bounded no-follow read and match config.sha256. Never log it.
    /// Cleanup uses the exact retained launch record; restore uses the accepted
    /// same-hash monotonic record after readiness and durable acceptance.
    pub config_path: &'a Path,
    pub run: Option<&'a OwnedRunIdentity>,
    /// Fixed admitted artifact location, private and never serialized/logged.
    pub artifact_root: Option<&'a Path>,
    pub artifact_path: Option<&'a Path>,
    pub artifact_file: Option<crate::readiness_tun::FileIdentity>,
    pub artifact_directory: Option<crate::readiness_tun::FileIdentity>,
    /// Samples the actual retained ProcessOwner, not a client/disk PID.
    pub owned_status: Option<&'a dyn Fn() -> Result<process::Status, ProcessError>>,
    pub deadline: Instant,
}
impl fmt::Debug for HookContext<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HookContext")
            .field("service", &self.service)
            .field("config", &self.config)
            .field("run", &self.run)
            .field("deadline", &self.deadline)
            .finish_non_exhaustive()
    }
}
type NativeHook = Box<dyn FnMut(&HookContext<'_>) -> Result<(), HookError>>;
/// Required native seams. There is deliberately no default healthy hook.
/// Callbacks are synchronous, must finish by `deadline`, and cannot call manager
/// mutations. A returned success after the deadline is rejected.
pub struct Hooks {
    pre_start: NativeHook,
    readiness: NativeHook,
    cleanup: NativeHook,
    restore: NativeHook,
}
impl Hooks {
    pub fn new(
        pre_start: impl FnMut(&HookContext<'_>) -> Result<(), HookError> + 'static,
        readiness: impl FnMut(&HookContext<'_>) -> Result<(), HookError> + 'static,
        cleanup: impl FnMut(&HookContext<'_>) -> Result<(), HookError> + 'static,
        restore: impl FnMut(&HookContext<'_>) -> Result<(), HookError> + 'static,
    ) -> Self {
        Self {
            pre_start: Box::new(pre_start),
            readiness: Box::new(readiness),
            cleanup: Box::new(cleanup),
            restore: Box::new(restore),
        }
    }
    fn call(&mut self, stage: HookStage, context: &HookContext<'_>) -> Result<(), Failure> {
        let callback = match stage {
            HookStage::PreStart => &mut self.pre_start,
            HookStage::Readiness => &mut self.readiness,
            HookStage::Cleanup => &mut self.cleanup,
            HookStage::Restore => &mut self.restore,
        };
        callback(context).map_err(|error| Failure::Hook(stage, error))?;
        if Instant::now() >= context.deadline {
            return Err(Failure::Hook(stage, HookError::Deadline));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    NotConfigured,
    Starting,
    Running,
    Stopped,
    Error,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub service: ServiceId,
    pub state: State,
    pub generation: u64,
    pub configured: bool,
    pub artifact_available: bool,
    pub pid: Option<u32>,
    pub desired: bool,
    pub restored: bool,
    pub needs_recovery: bool,
    pub resource_suspended: bool,
    pub ready: bool,
    pub running_matches_accepted: bool,
    /// True only for a live, ready matching Run and successful resource restore.
    pub active: bool,
    pub durability_uncertain: bool,
    pub error_code: Option<&'static str>,
}
/// Explicit private config read, unlike public Status. Debug is redacted.
pub struct ConfigSnapshot {
    pub identity: ConfigIdentity,
    bytes: Vec<u8>,
}
impl ConfigSnapshot {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
impl fmt::Debug for ConfigSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConfigSnapshot")
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}
struct Run {
    identity: OwnedRunIdentity,
    record: ConfigRecord,
    binding: ArtifactBinding,
    artifact_file: Option<crate::readiness_tun::FileIdentity>,
    artifact_directory: Option<crate::readiness_tun::FileIdentity>,
    ready: bool,
}
struct PendingArtifact {
    stage: RetainedStage,
    generation: u64,
    checked: bool,
    has_config: bool,
    previous_metadata: Option<store::Artifact>,
    metadata_committed: bool,
    initial_owner: bool,
}
struct ArtifactRecovery {
    binding: ArtifactBinding,
    metadata: Option<store::Artifact>,
    proven: bool,
    switched_back: bool,
}
struct Service {
    binding: Option<ArtifactBinding>,
    artifact_file: Option<crate::readiness_tun::FileIdentity>,
    artifact_directory: Option<crate::readiness_tun::FileIdentity>,
    config_root: PathBuf,
    run_root: PathBuf,
    owner: Option<ProcessOwner>,
    run: Option<Run>,
    pending_check: Option<Candidate>,
    pending_artifact: Option<PendingArtifact>,
    active_artifact: Option<RetainedStage>,
    retired_artifact: Option<RetainedStage>,
    artifact_recovery: Option<ArtifactRecovery>,
    desired: bool,
    restored: bool,
    needs_recovery: bool,
    resource_suspended: bool,
    durability_uncertain: bool,
    artifact_durability_uncertain: bool,
    failure: Option<Failure>,
}
struct FrozenReady {
    record: ConfigRecord,
    bytes: Vec<u8>,
    binding: ArtifactBinding,
}
#[must_use = "explicitly close this manager; retry cleanup failures on the same owner"]
pub struct Manager {
    store: RuntimeStore,
    services: [Service; 2],
    hooks: Hooks,
    limits: Limits,
    closed: bool,
    closing: bool,
    operation: Option<(Instant, Arc<AtomicBool>)>,
    #[cfg(test)]
    artifact_cancel_step: Option<bool>,
}
impl fmt::Debug for Manager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Manager")
            .field("closed", &self.closed)
            .finish_non_exhaustive()
    }
}
impl Manager {
    /// Opens the exclusive store and pins fixed process roots. Executes zero
    /// child starts, checks, cleanup, readiness or resource commands.
    pub fn open(
        data_dir: impl AsRef<Path>,
        run_dir: impl AsRef<Path>,
        artifacts: ArtifactBindings,
        hooks: Hooks,
        limits: Limits,
    ) -> Result<Self, ManagerError> {
        let constructor_error = |failure| ManagerError {
            service: None,
            failure,
            recovery_failure: None,
            generation: 0,
            owned_pid: None,
        };
        limits.validate().map_err(constructor_error)?;
        let store = RuntimeStore::open(data_dir.as_ref(), run_dir.as_ref())
            .map_err(|error| constructor_error(Failure::Store(error)))?;
        let data = fs::canonicalize(data_dir.as_ref())
            .map_err(|_| constructor_error(Failure::Store(StoreError::Storage)))?;
        let run = fs::canonicalize(run_dir.as_ref())
            .map_err(|_| constructor_error(Failure::Store(StoreError::Storage)))?;
        let sing_box = Self::service(ServiceId::SingBox, &data, &run, artifacts.sing_box, limits)
            .map_err(constructor_error)?;
        let frpc = match Self::service(ServiceId::Frpc, &data, &run, artifacts.frpc, limits) {
            Ok(service) => service,
            Err(failure) => {
                let mut sing_box = sing_box;
                if let Some(owner) = &mut sing_box.owner {
                    let _ = owner.close();
                }
                return Err(constructor_error(failure));
            }
        };
        Ok(Self {
            store,
            services: [sing_box, frpc],
            hooks,
            limits,
            closed: false,
            closing: false,
            operation: None,
            #[cfg(test)]
            artifact_cancel_step: None,
        })
    }
    fn service(
        service: ServiceId,
        data: &Path,
        run: &Path,
        binding: Option<ArtifactBinding>,
        limits: Limits,
    ) -> Result<Service, Failure> {
        let config_root = data.join(service.as_str());
        let owner = if let Some(binding) = &binding {
            if binding.service != service || binding.path.parent() != Some(binding.root.as_path()) {
                return Err(Failure::InvalidInput);
            }
            Some(Self::create_process_owner(
                service,
                &binding.root,
                &config_root,
                &run.join(service.as_str()),
                limits,
            )?)
        } else {
            None
        };
        let artifact_file = binding
            .as_ref()
            .map(|binding| {
                fs::symlink_metadata(&binding.path)
                    .map(|metadata| crate::readiness_tun::FileIdentity::from_metadata(&metadata))
                    .map_err(|_| Failure::ArtifactUnavailable)
            })
            .transpose()?;
        let artifact_directory = binding
            .as_ref()
            .map(|binding| {
                fs::symlink_metadata(&binding.root)
                    .map(|metadata| crate::readiness_tun::FileIdentity::from_metadata(&metadata))
                    .map_err(|_| Failure::ArtifactUnavailable)
            })
            .transpose()?;
        Ok(Service {
            binding,
            artifact_file,
            artifact_directory,
            config_root,
            run_root: run.join(service.as_str()),
            owner,
            run: None,
            pending_check: None,
            pending_artifact: None,
            active_artifact: None,
            retired_artifact: None,
            artifact_recovery: None,
            desired: false,
            restored: false,
            needs_recovery: false,
            resource_suspended: true,
            durability_uncertain: false,
            artifact_durability_uncertain: false,
            failure: None,
        })
    }
    fn create_process_owner(
        service: ServiceId,
        artifact_root: &Path,
        config_root: &Path,
        run_root: &Path,
        limits: Limits,
    ) -> Result<ProcessOwner, Failure> {
        match fs::symlink_metadata(run_root) {
            Ok(meta) if !meta.is_dir() || meta.file_type().is_symlink() => {
                return Err(Failure::Process(ProcessError::UntrustedPath));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::DirBuilder::new()
                    .mode(0o700)
                    .create(run_root)
                    .map_err(|_| Failure::Store(StoreError::Storage))?;
            }
            Err(_) => return Err(Failure::Store(StoreError::Storage)),
        }
        fs::set_permissions(run_root, fs::Permissions::from_mode(0o700))
            .map_err(|_| Failure::Store(StoreError::Storage))?;
        ProcessOwner::new(
            service.into(),
            TrustedRoots::new(artifact_root, config_root, run_root),
            limits.process,
        )
        .map_err(Failure::Process)
    }
    fn error(
        &self,
        service: ServiceId,
        failure: Failure,
        recovery_failure: Option<Failure>,
    ) -> ManagerError {
        ManagerError {
            service: Some(service),
            failure,
            recovery_failure,
            generation: self.store.service_state(service.into()).generation,
            owned_pid: self.services[service.index()]
                .run
                .as_ref()
                .map(|run| run.identity.pid),
        }
    }
    fn operation_check(&self) -> Result<(), Failure> {
        if let Some((deadline, cancel)) = &self.operation {
            if cancel.load(std::sync::atomic::Ordering::Acquire) {
                return Err(Failure::Cancelled);
            }
            if Instant::now() >= *deadline {
                return Err(Failure::OperationDeadline);
            }
        }
        Ok(())
    }
    fn set_operation(&mut self, budget: Option<(Instant, Arc<AtomicBool>)>) {
        self.store.set_operation_budget(budget.clone());
        self.operation = budget;
    }
    fn independent_recovery<T>(&mut self, action: impl FnOnce(&mut Self) -> T) -> T {
        let operation = self.operation.take();
        self.store.set_operation_budget(None);
        let result = action(self);
        self.set_operation(operation);
        result
    }
    pub(crate) fn acquire_verified_stage(
        &mut self,
        service: ServiceId,
        expected: u64,
        stage: Stage,
        deadline: Instant,
        cancel: Arc<AtomicBool>,
    ) -> Result<Status, ManagerError> {
        if self.operation.is_some() {
            return Err(self.error(service, Failure::CheckPending, None));
        }
        self.set_operation(Some((deadline, cancel.clone())));
        let result = (|| {
            self.operation_check()
                .map_err(|failure| self.error(service, failure, None))?;
            let status = self.status(service)?;
            if !status.artifact_available {
                self.initialize_staged_artifact(service, expected, stage, Some(cancel))
            } else {
                self.check_staged_artifact(service, expected, stage, Some(cancel))?;
                if let Err(failure) = self.operation_check() {
                    let recovery = self
                        .abort_staged_artifact(service)
                        .err()
                        .map(|error| error.failure);
                    return Err(self.error(service, failure, recovery));
                }
                self.activate_staged_artifact(service, expected)
            }
        })();
        let result = match result {
            Ok(status) => match self.operation_check() {
                Ok(()) => Ok(status),
                Err(failure) => Err(self.error(service, failure, None)),
            },
            Err(error) => Err(error),
        };
        let result = if result.as_ref().is_err_and(|error| {
            matches!(
                error.failure,
                Failure::OperationDeadline
                    | Failure::Cancelled
                    | Failure::Store(StoreError::OperationDeadline)
                    | Failure::Store(StoreError::Cancelled)
                    | Failure::Process(ProcessError::OperationDeadline)
                    | Failure::Process(ProcessError::Cancelled)
            )
        }) && self.services[service.index()].pending_artifact.is_some()
        {
            match self.abort_staged_artifact(service) {
                Ok(()) => result,
                Err(cleanup) => result.map_err(|mut error| {
                    error.recovery_failure = Some(cleanup.failure);
                    error
                }),
            }
        } else {
            result
        };
        self.set_operation(None);
        result
    }
    fn verify_process(
        &mut self,
        service: ServiceId,
        spec: LaunchSpec,
        cancel: Option<Arc<AtomicBool>>,
    ) -> Result<(), ProcessError> {
        let operation = self.operation.clone();
        let owner = self.services[service.index()]
            .owner
            .as_mut()
            .ok_or(ProcessError::Closed)?;
        if let Some((deadline, flag)) = operation {
            owner.verify_until(spec, deadline, Some(flag))
        } else {
            owner.verify(spec, cancel)
        }
    }
    fn ensure(&self, service: ServiceId) -> Result<(), Failure> {
        self.operation_check()?;
        if self.closed || self.closing {
            return Err(Failure::Closed);
        }
        if self.services[service.index()].pending_check.is_some()
            || self.services[service.index()].pending_artifact.is_some()
            || self.services[service.index()].retired_artifact.is_some()
            || self.services[service.index()].artifact_recovery.is_some()
        {
            return Err(Failure::CheckPending);
        }
        Ok(())
    }
    fn generation(&self, service: ServiceId, expected: u64) -> Result<(), Failure> {
        self.ensure(service)?;
        if self.store.service_state(service.into()).generation != expected {
            return Err(Failure::Generation);
        }
        Ok(())
    }
    pub fn config(&self, service: ServiceId) -> Result<Option<ConfigSnapshot>, ManagerError> {
        let record = self.store.service_state(service.into()).current.as_ref();
        record
            .map(|record| {
                self.store
                    .read_config(service.into(), record)
                    .map(|bytes| ConfigSnapshot {
                        identity: config_identity(record),
                        bytes,
                    })
                    .map_err(|error| self.error(service, Failure::Store(error), None))
            })
            .transpose()
    }
    /// Stored bounded request only. Does not adopt/hash any boot executable.
    pub(crate) fn saved_artifact_request(&self, service: ServiceId) -> Option<store::Artifact> {
        self.store.service_state(service.into()).artifact.clone()
    }
    pub(crate) fn artifact_root(&self, service: ServiceId) -> Option<&Path> {
        self.services[service.index()]
            .binding
            .as_ref()
            .map(|binding| binding.root.as_path())
    }
    pub(crate) fn artifact_version(&self, service: ServiceId) -> Option<&str> {
        let slot = &self.services[service.index()];
        if slot.binding.is_none()
            || slot.pending_artifact.is_some()
            || slot.artifact_recovery.is_some()
            || slot.artifact_durability_uncertain
        {
            return None;
        }
        self.store
            .service_state(service.into())
            .artifact
            .as_ref()
            .map(|metadata| metadata.version.as_str())
    }
    pub(crate) fn acquisition_fetch_budget(&self) -> Result<Duration, Failure> {
        // Reserve configured child/hook wait limits for checker/abort, three
        // withdrawals and new+old readiness. Hash/syscall time is separately
        // guarded by the absolute operation cutoff, not this arithmetic.
        // Safety recovery keeps independent limits after expiry.
        let process = self.limits.process;
        let stop = process
            .term_grace
            .checked_add(process.kill_grace)
            .ok_or(Failure::InvalidInput)?;
        let checker = process
            .check_timeout
            .checked_add(stop.checked_mul(2).ok_or(Failure::InvalidInput)?)
            .ok_or(Failure::InvalidInput)?;
        let withdrawals = self
            .limits
            .resource_timeout
            .checked_add(stop)
            .and_then(|n| n.checked_mul(3))
            .ok_or(Failure::InvalidInput)?;
        let readiness = self
            .limits
            .readiness_timeout
            .checked_mul(2)
            .and_then(|n| n.checked_add(self.limits.resource_timeout))
            .and_then(|n| n.checked_mul(2))
            .ok_or(Failure::InvalidInput)?;
        let reserved = checker
            .checked_add(withdrawals)
            .and_then(|n| n.checked_add(readiness))
            .and_then(|n| n.checked_add(Duration::from_secs(1)))
            .ok_or(Failure::InvalidInput)?;
        let remaining = Duration::from_secs(90)
            .checked_sub(reserved)
            .ok_or(Failure::InvalidInput)?;
        if remaining < Duration::from_secs(1) {
            return Err(Failure::InvalidInput);
        }
        Ok(remaining.min(Duration::from_secs(45)))
    }
    pub(crate) fn admit_artifact_acquisition(
        &mut self,
        service: ServiceId,
        expected: u64,
        root: &Path,
    ) -> Result<Status, ManagerError> {
        self.generation(service, expected)
            .map_err(|failure| self.error(service, failure, None))?;
        let status = self.status(service)?;
        if status.durability_uncertain
            || status.needs_recovery
            || status.pid.is_some() && !status.active
        {
            return Err(self.error(service, Failure::NotReady, None));
        }
        if self
            .artifact_root(service)
            .is_some_and(|fixed| fixed != root)
        {
            return Err(self.error(service, Failure::InvalidInput, None));
        }
        Ok(status)
    }
    /// The borrowed identity is only this manager's current owned handle.
    /// `status` observes exit; this accessor does not assert liveness/readiness.
    pub fn current_run(&self, service: ServiceId) -> Option<&OwnedRunIdentity> {
        self.services[service.index()]
            .run
            .as_ref()
            .map(|run| &run.identity)
    }
    /// Read-only native observation context from THIS retained live ready Run.
    /// No checker/start/withdrawal, arbitrary path or persisted PID is exposed.
    pub fn observe_current<T>(
        &mut self,
        service: ServiceId,
        deadline: Instant,
        mut observe: impl FnMut(&HookContext<'_>) -> Result<T, HookError>,
    ) -> Result<T, HookError> {
        self.ready_context(service, deadline, &mut observe)
    }
    pub(crate) fn capture_operation<T>(
        &mut self,
        deadline: Instant,
        mut action: impl FnMut(&HookContext<'_>) -> Result<T, HookError>,
    ) -> Result<T, HookError> {
        self.ensure(ServiceId::SingBox)
            .map_err(|_| HookError::Failed)?;
        self.ready_context(ServiceId::SingBox, deadline, &mut action)
    }
    fn ready_context<T>(
        &mut self,
        service: ServiceId,
        deadline: Instant,
        mut observe: impl FnMut(&HookContext<'_>) -> Result<T, HookError>,
    ) -> Result<T, HookError> {
        let deadline = deadline.min(Instant::now() + Duration::from_secs(30));
        if Instant::now() >= deadline {
            return Err(HookError::Deadline);
        }
        let initial_sample = self.status(service);
        let initial = current_status_after_io(initial_sample, deadline, Instant::now())?;
        if !initial.active || !initial.desired || initial.durability_uncertain {
            return Err(HookError::Failed);
        }
        let slot = &self.services[service.index()];
        let run = slot.run.as_ref().ok_or(HookError::Failed)?;
        let identity = run.identity.clone();
        let config = config_identity(&run.record);
        let config_path = slot.config_root.join(&run.record.file);
        let status = || {
            slot.owner
                .as_ref()
                .ok_or(ProcessError::Closed)?
                .observe_retained()
        };
        let context = HookContext {
            service,
            config: &config,
            config_path: &config_path,
            run: Some(&run.identity),
            artifact_root: Some(&run.binding.root),
            artifact_path: Some(&run.binding.path),
            artifact_file: run.artifact_file,
            artifact_directory: run.artifact_directory,
            owned_status: Some(&status),
            deadline,
        };
        let result = observe(&context);
        if Instant::now() >= deadline {
            return Err(HookError::Deadline);
        }
        let final_sample = self.status(service);
        let final_status = current_status_after_io(final_sample, deadline, Instant::now())?;
        if !final_status.active || self.current_run(service) != Some(&identity) {
            return Err(HookError::Failed);
        }
        result
    }
    fn observe(&mut self, service: ServiceId) -> Result<Option<process::Status>, Failure> {
        self.services[service.index()]
            .owner
            .as_mut()
            .map(|owner| owner.status())
            .transpose()
            .map_err(Failure::Process)
    }
    pub fn status(&mut self, service: ServiceId) -> Result<Status, ManagerError> {
        let observed = if self.closed {
            None
        } else {
            self.observe(service)
                .map_err(|failure| self.error(service, failure, None))?
        };
        let record = self.store.service_state(service.into()).current.as_ref();
        let slot = &mut self.services[service.index()];
        let run_live = observed.is_some_and(|status| {
            status.phase == process::Phase::Running && status.mode == Some(process::LaunchMode::Run)
        });
        let exited = observed.is_some_and(|status| {
            status.phase == process::Phase::Exited && status.mode == Some(process::LaunchMode::Run)
        });
        if exited {
            slot.needs_recovery = true;
            slot.resource_suspended = true;
            slot.failure = Some(Failure::Exited);
        }
        let matches = run_live
            && slot.run.as_ref().is_some_and(|run| {
                observed.is_some_and(|status| status.pid == Some(run.identity.pid))
                    && slot.binding.as_ref().is_some_and(|binding| {
                        binding.path == run.binding.path && binding.sha256 == run.binding.sha256
                    })
                    && record.is_some_and(|record| {
                        record.generation == run.identity.accepted_generation
                            && record.sha256 == run.identity.sha256
                    })
            });
        let ready = matches
            && slot.run.as_ref().is_some_and(|run| run.ready)
            && record.is_some_and(|record| record.ready);
        let state = if ready {
            State::Running
        } else if run_live {
            if slot.needs_recovery {
                State::Error
            } else {
                State::Starting
            }
        } else if exited || slot.needs_recovery {
            State::Error
        } else if record.is_none() {
            State::NotConfigured
        } else {
            State::Stopped
        };
        Ok(Status {
            service,
            state,
            generation: self.store.service_state(service.into()).generation,
            configured: record.is_some(),
            artifact_available: slot.binding.is_some(),
            pid: observed
                .filter(|status| status.mode == Some(process::LaunchMode::Run))
                .and_then(|status| status.pid),
            desired: slot.desired,
            restored: slot.restored,
            needs_recovery: slot.needs_recovery
                || slot.artifact_recovery.is_some()
                || slot.retired_artifact.is_some(),
            resource_suspended: slot.resource_suspended,
            ready,
            running_matches_accepted: matches,
            active: ready
                && !slot.resource_suspended
                && !slot.needs_recovery
                && !slot.durability_uncertain
                && !slot.artifact_durability_uncertain
                && slot.artifact_recovery.is_none()
                && slot.retired_artifact.is_none(),
            durability_uncertain: slot.durability_uncertain || slot.artifact_durability_uncertain,
            error_code: slot.failure.map(Failure::code),
        })
    }
    fn spec(&self, service: ServiceId, path: &Path, bytes: &[u8]) -> Result<LaunchSpec, Failure> {
        let binding = self.services[service.index()]
            .binding
            .as_ref()
            .ok_or(Failure::ArtifactUnavailable)?;
        Ok(LaunchSpec::new(
            &binding.path,
            binding.sha256,
            path,
            Sha256::digest(bytes).into(),
            bytes.len() as u64,
        ))
    }
    fn check(
        &mut self,
        service: ServiceId,
        candidate: Candidate,
        bytes: &[u8],
        cancel: Option<Arc<AtomicBool>>,
    ) -> Result<Candidate, Failure> {
        let spec = self.spec(service, candidate.path(), bytes)?;
        let slot = &mut self.services[service.index()];
        let owner = slot.owner.as_mut().ok_or(Failure::ArtifactUnavailable)?;
        if let Err(error) = owner.verify(spec, cancel) {
            // Normally verify has fully reaped the finite checker. If finishing
            // failed, retain its candidate inode until explicit abort succeeds.
            if owner.abort_check().is_err() {
                slot.pending_check = Some(candidate);
            }
            return Err(Failure::Process(error));
        }
        Ok(candidate)
    }
    /// First artifact for a missing fixed owner. Typed Stage trust is required;
    /// saved request metadata or boot files cannot authorize an executable.
    /// Optional accepted config is checked; no Run or readiness is created.
    pub fn initialize_staged_artifact(
        &mut self,
        service: ServiceId,
        expected_generation: u64,
        stage: Stage,
        cancel: Option<Arc<AtomicBool>>,
    ) -> Result<Status, ManagerError> {
        self.generation(service, expected_generation)
            .map_err(|failure| self.error(service, failure, None))?;
        if cancel
            .as_ref()
            .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Acquire))
        {
            return Err(self.error(service, Failure::Process(ProcessError::Cancelled), None));
        }
        let slot = &self.services[service.index()];
        if slot.binding.is_some()
            || slot.owner.is_some()
            || slot.run.is_some()
            || slot.active_artifact.is_some()
        {
            return Err(self.error(service, Failure::Process(ProcessError::Busy), None));
        }
        let admitted = stage
            .admitted()
            .map_err(|error| self.error(service, Failure::ArtifactStage(error), None))?;
        let binding = ArtifactBinding::trusted_local(
            service,
            admitted.root,
            admitted.path,
            admitted.extracted_sha256,
            ArtifactProvenance::TrustedLocalModule,
        );
        let metadata = admitted.artifact.clone();
        let previous_metadata = self.store.service_state(service.into()).artifact.clone();
        let record = self.store.service_state(service.into()).current.clone();
        let bytes = record
            .as_ref()
            .map(|record| self.store.read_config(service.into(), record))
            .transpose()
            .map_err(|error| self.error(service, Failure::Store(error), None))?;
        self.store
            .admit_artifact_intent(service.into(), expected_generation, Some(metadata.clone()))
            .map_err(|error| self.error(service, Failure::Store(error), None))?;
        let retained = stage
            .into_retained()
            .map_err(|error| self.error(service, Failure::ArtifactStage(error), None))?;
        self.services[service.index()].pending_artifact = Some(PendingArtifact {
            stage: retained,
            generation: expected_generation,
            checked: false,
            has_config: record.is_some(),
            previous_metadata,
            metadata_committed: false,
            initial_owner: true,
        });
        if let Err(failure) = self.operation_check() {
            let recovery = self
                .abort_staged_artifact(service)
                .err()
                .map(|error| error.failure);
            return Err(self.error(service, failure, recovery));
        }
        let owner = Self::create_process_owner(
            service,
            &binding.root,
            &self.services[service.index()].config_root,
            &self.services[service.index()].run_root,
            self.limits,
        );
        match owner {
            Ok(owner) => self.services[service.index()].owner = Some(owner),
            Err(failure) => {
                let recovery = self
                    .abort_staged_artifact(service)
                    .err()
                    .map(|error| error.failure);
                return Err(self.error(service, failure, recovery));
            }
        }
        if let (Some(record), Some(bytes)) = (&record, &bytes) {
            let spec = LaunchSpec::new(
                &binding.path,
                binding.sha256,
                self.services[service.index()]
                    .config_root
                    .join(&record.file),
                Sha256::digest(bytes).into(),
                bytes.len() as u64,
            );
            let result = self.verify_process(service, spec, cancel.clone());
            if let Err(error) = result {
                let recovery = self
                    .abort_staged_artifact(service)
                    .err()
                    .map(|error| error.failure);
                return Err(self.error(service, Failure::Process(error), recovery));
            }
        }
        let verification = (|| {
            self.operation_check()?;
            if cancel
                .as_ref()
                .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Acquire))
            {
                return Err(Failure::Process(ProcessError::Cancelled));
            }
            if self.store.service_state(service.into()).generation != expected_generation {
                return Err(Failure::Generation);
            }
            let pending = self.services[service.index()]
                .pending_artifact
                .as_ref()
                .ok_or(Failure::CheckPending)?;
            pending.stage.admitted().map_err(Failure::ArtifactStage)?;
            if let (Some(record), Some(bytes)) = (&record, &bytes)
                && self
                    .store
                    .read_config(service.into(), record)
                    .map_err(Failure::Store)?
                    != *bytes
            {
                return Err(Failure::Store(StoreError::Verification));
            }
            Ok(())
        })();
        if let Err(failure) = verification {
            let recovery = self
                .abort_staged_artifact(service)
                .err()
                .map(|error| error.failure);
            return Err(self.error(service, failure, recovery));
        }
        if let Some(pending) = self.services[service.index()].pending_artifact.as_mut() {
            pending.checked = record.is_some();
        }
        drop(bytes);
        let committed =
            self.store
                .set_artifact_intent(service.into(), expected_generation, Some(metadata));
        let outcome = match committed {
            Ok(outcome) => outcome,
            Err(error) => {
                let recovery = self
                    .abort_staged_artifact(service)
                    .err()
                    .map(|error| error.failure);
                return Err(self.error(service, Failure::Store(error), recovery));
            }
        };
        if let Some(pending) = self.services[service.index()].pending_artifact.as_mut() {
            pending.metadata_committed = true;
        }
        self.artifact_outcome(service, outcome.durability_error)
            .map_err(|failure| self.error(service, failure, None))?;
        let admitted = self.services[service.index()]
            .pending_artifact
            .as_ref()
            .ok_or_else(|| self.error(service, Failure::CheckPending, None))?
            .stage
            .admitted();
        if let Err(error) = admitted {
            let recovery = self
                .abort_staged_artifact(service)
                .err()
                .map(|error| error.failure);
            return Err(self.error(service, Failure::ArtifactStage(error), recovery));
        }
        if let Err(failure) = self.bind_artifact(service, binding) {
            let recovery = self
                .abort_staged_artifact(service)
                .err()
                .map(|error| error.failure);
            return Err(self.error(service, failure, recovery));
        }
        let pending = self.services[service.index()]
            .pending_artifact
            .take()
            .ok_or_else(|| self.error(service, Failure::CheckPending, None))?;
        let slot = &mut self.services[service.index()];
        slot.active_artifact = Some(pending.stage);
        slot.desired = false;
        slot.failure = None;
        slot.needs_recovery = false;
        slot.resource_suspended = true;
        self.status(service)
    }
    /// Check typed verified stage with the SAME fixed service owner while its
    /// old Run remains live. No metadata/binding/Run activation is performed.
    /// A finite checker or cleanup failure retains the stage in this manager;
    /// the caller must abort it before another mutation.
    pub fn check_staged_artifact(
        &mut self,
        service: ServiceId,
        expected_generation: u64,
        stage: Stage,
        cancel: Option<Arc<AtomicBool>>,
    ) -> Result<Status, ManagerError> {
        self.generation(service, expected_generation)
            .map_err(|failure| self.error(service, failure, None))?;
        let admitted = stage
            .admitted()
            .map_err(|error| self.error(service, Failure::ArtifactStage(error), None))?;
        let binding = self.services[service.index()]
            .binding
            .as_ref()
            .ok_or_else(|| self.error(service, Failure::ArtifactUnavailable, None))?;
        if admitted.root != binding.root || admitted.path.parent() != Some(binding.root.as_path()) {
            return Err(self.error(service, Failure::InvalidInput, None));
        }
        let record = self.store.service_state(service.into()).current.clone();
        let bytes = record
            .as_ref()
            .map(|record| self.store.read_config(service.into(), record))
            .transpose()
            .map_err(|error| self.error(service, Failure::Store(error), None))?;
        let spec = record.as_ref().zip(bytes.as_ref()).map(|(record, bytes)| {
            LaunchSpec::new(
                admitted.path,
                admitted.extracted_sha256,
                self.services[service.index()]
                    .config_root
                    .join(&record.file),
                Sha256::digest(bytes).into(),
                bytes.len() as u64,
            )
        });
        let retained = stage
            .into_retained()
            .map_err(|error| self.error(service, Failure::ArtifactStage(error), None))?;
        self.services[service.index()].pending_artifact = Some(PendingArtifact {
            stage: retained,
            generation: expected_generation,
            checked: false,
            has_config: record.is_some(),
            previous_metadata: None,
            metadata_committed: false,
            initial_owner: false,
        });
        let checked = if cancel
            .as_ref()
            .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Acquire))
        {
            Err(ProcessError::Cancelled)
        } else if let Some(spec) = spec {
            self.verify_process(service, spec, cancel)
        } else {
            Ok(())
        };
        if let Err(error) = checked {
            let recovery = self
                .abort_staged_artifact(service)
                .err()
                .map(|error| error.failure);
            return Err(self.error(service, Failure::Process(error), recovery));
        }
        // Native checker success is not enough if accepted config or exact
        // staged inode changed during check. Never create a readiness proof.
        let verified = (|| {
            self.operation_check()?;
            if self.store.service_state(service.into()).generation != expected_generation {
                return Err(Failure::Generation);
            }
            let pending = self.services[service.index()]
                .pending_artifact
                .as_ref()
                .ok_or(Failure::CheckPending)?;
            pending.stage.admitted().map_err(Failure::ArtifactStage)?;
            if let (Some(record), Some(bytes)) = (&record, &bytes)
                && self
                    .store
                    .read_config(service.into(), record)
                    .map_err(Failure::Store)?
                    != *bytes
            {
                return Err(Failure::Store(StoreError::Verification));
            }
            Ok(())
        })();
        if let Err(failure) = verified {
            let recovery = self
                .abort_staged_artifact(service)
                .err()
                .map(|error| error.failure);
            return Err(self.error(service, failure, recovery));
        }
        let Some(pending) = self.services[service.index()].pending_artifact.as_mut() else {
            return Err(self.error(service, Failure::CheckPending, None));
        };
        pending.checked = true;
        self.status(service)
    }
    /// In-process staging observation, not accepted metadata or active runtime.
    pub fn staged_artifact_checked(&self, service: ServiceId) -> bool {
        self.services[service.index()]
            .pending_artifact
            .as_ref()
            .is_some_and(|pending| {
                pending.checked
                    && pending.has_config
                    && pending.generation == self.store.service_state(service.into()).generation
                    && pending.stage.admitted().is_ok()
            })
    }
    fn bind_artifact(
        &mut self,
        service: ServiceId,
        binding: ArtifactBinding,
    ) -> Result<(), Failure> {
        self.operation_check()?;
        let file = fs::symlink_metadata(&binding.path).map_err(|_| Failure::ArtifactUnavailable)?;
        let directory =
            fs::symlink_metadata(&binding.root).map_err(|_| Failure::ArtifactUnavailable)?;
        self.operation_check()?;
        let slot = &mut self.services[service.index()];
        slot.artifact_file = Some(crate::readiness_tun::FileIdentity::from_metadata(&file));
        slot.artifact_directory = Some(crate::readiness_tun::FileIdentity::from_metadata(
            &directory,
        ));
        slot.binding = Some(binding);
        Ok(())
    }
    fn retire_artifact(&mut self, service: ServiceId) -> Result<(), Failure> {
        let slot = &self.services[service.index()];
        if let (Some(retired), Some(run)) = (&slot.retired_artifact, &slot.run)
            && retired.owns_path(&run.binding.path)
        {
            return Err(Failure::Process(ProcessError::Busy));
        }
        if let Some(retired) = self.services[service.index()].retired_artifact.as_mut() {
            retired.cleanup().map_err(Failure::ArtifactStage)?;
        }
        self.services[service.index()].retired_artifact.take();
        Ok(())
    }
    pub fn retry_artifact_retirement(&mut self, service: ServiceId) -> Result<(), ManagerError> {
        if self.services[service.index()].artifact_recovery.is_some() {
            return Err(self.error(service, Failure::CheckPending, None));
        }
        self.retire_artifact(service)
            .map_err(|failure| self.error(service, failure, None))?;
        self.services[service.index()].failure = None;
        Ok(())
    }
    /// Activate only the typed stage already checked with current accepted
    /// bytes. No URL fetch or arbitrary executable/PID enters this operation.
    pub fn activate_staged_artifact(
        &mut self,
        service: ServiceId,
        expected_generation: u64,
    ) -> Result<Status, ManagerError> {
        let result = self.activate_artifact_inner(service, expected_generation);
        match result {
            Ok(()) => self.status(service),
            Err((failure, recovery)) => {
                self.services[service.index()].failure = Some(failure);
                Err(self.error(service, failure, recovery))
            }
        }
    }
    fn restart_proven_artifact_after_failure(
        &mut self,
        service: ServiceId,
        proven: bool,
    ) -> Option<Failure> {
        self.independent_recovery(|manager| manager.restart_proven_artifact_inner(service, proven))
    }
    fn restart_proven_artifact_inner(
        &mut self,
        service: ServiceId,
        proven: bool,
    ) -> Option<Failure> {
        if !proven {
            return None;
        }
        match self.start_accepted(service) {
            Ok(()) => {
                self.services[service.index()].restored = true;
                None
            }
            Err(failure) => {
                slot_suspended(&mut self.services[service.index()]);
                if !matches!(
                    failure,
                    Failure::Hook(HookStage::Restore, _) | Failure::Store(StoreError::Durability)
                ) && let Err(cleanup) = self.withdraw_stop(service)
                {
                    return Some(cleanup);
                }
                Some(failure)
            }
        }
    }
    fn activate_artifact_inner(
        &mut self,
        service: ServiceId,
        expected: u64,
    ) -> Result<(), (Failure, Option<Failure>)> {
        self.operation_check().map_err(|failure| (failure, None))?;
        if self.closed || self.closing {
            return Err((Failure::Closed, None));
        }
        if self.store.service_state(service.into()).generation != expected {
            return Err((Failure::Generation, None));
        }
        let slot = &self.services[service.index()];
        if slot.pending_check.is_some()
            || slot.artifact_recovery.is_some()
            || slot.retired_artifact.is_some()
        {
            return Err((Failure::CheckPending, None));
        }
        if slot.durability_uncertain || slot.artifact_durability_uncertain {
            return Err((Failure::Store(StoreError::Durability), None));
        }
        if slot.needs_recovery {
            return Err((Failure::NotReady, None));
        }
        let pending = slot
            .pending_artifact
            .as_ref()
            .ok_or((Failure::CheckPending, None))?;
        if !pending.checked
            || pending.generation != expected
            || pending.has_config != self.store.service_state(service.into()).current.is_some()
        {
            return Err((Failure::CheckPending, None));
        }
        let admitted = pending
            .stage
            .admitted()
            .map_err(|error| (Failure::ArtifactStage(error), None))?;
        let old_binding = slot
            .binding
            .clone()
            .ok_or((Failure::ArtifactUnavailable, None))?;
        let new_binding = ArtifactBinding::trusted_local(
            service,
            admitted.root,
            admitted.path,
            admitted.extracted_sha256,
            ArtifactProvenance::TrustedLocalModule,
        );
        let new_metadata = admitted.artifact.clone();
        let old_metadata = self.store.service_state(service.into()).artifact.clone();
        let should_run = slot.desired;
        if let Some(record) = self.store.service_state(service.into()).current.as_ref() {
            let accepted = self
                .store
                .read_config(service.into(), record)
                .map_err(|error| (Failure::Store(error), None))?;
            drop(accepted);
        }
        let proven = self
            .freeze(service)
            .map_err(|failure| (failure, None))?
            .is_some();
        if should_run && !proven {
            return Err((Failure::NotReady, None));
        }
        // Admission reserves full manifest growth before any old Run stop.
        self.store
            .admit_artifact_intent(service.into(), expected, Some(new_metadata.clone()))
            .map_err(|error| (Failure::Store(error), None))?;
        #[cfg(test)]
        if self.artifact_cancel_step == Some(false) {
            self.artifact_cancel_step = None;
            if let Some((_, flag)) = &self.operation {
                flag.store(true, std::sync::atomic::Ordering::Release);
            }
        }
        self.operation_check().map_err(|failure| (failure, None))?;
        self.withdraw_stop(service)
            .map_err(|failure| (failure, None))?;
        #[cfg(test)]
        if self.artifact_cancel_step == Some(true) {
            self.artifact_cancel_step = None;
            if let Some((_, flag)) = &self.operation {
                flag.store(true, std::sync::atomic::Ordering::Release);
            }
        }
        let readmission = self.services[service.index()]
            .pending_artifact
            .as_ref()
            .ok_or(Failure::CheckPending)
            .and_then(|pending| {
                pending
                    .stage
                    .admitted()
                    .map(|_| ())
                    .map_err(Failure::ArtifactStage)
            });
        if let Err(failure) = self.operation_check() {
            let recovery = self.restart_proven_artifact_after_failure(service, proven);
            return Err((failure, recovery));
        }
        if let Err(failure) = readmission {
            let recovery = self.restart_proven_artifact_after_failure(service, proven);
            return Err((failure, recovery));
        }
        if let Some(record) = self.store.service_state(service.into()).current.as_ref()
            && let Err(error) = self.store.read_config(service.into(), record)
        {
            let recovery = self.restart_proven_artifact_after_failure(service, proven);
            return Err((Failure::Store(error), recovery));
        }
        let commit = self
            .store
            .set_artifact_intent(service.into(), expected, Some(new_metadata));
        let outcome = match commit {
            Ok(outcome) => outcome,
            Err(error) => {
                let recovery = self.restart_proven_artifact_after_failure(service, proven);
                return Err((Failure::Store(error), recovery));
            }
        };
        if let Some(pending) = self.services[service.index()].pending_artifact.as_mut() {
            pending.previous_metadata = old_metadata.clone();
            pending.metadata_committed = true;
        }
        self.artifact_outcome(service, outcome.durability_error)
            .map_err(|failure| (failure, None))?;
        // No owned Run remains. New binding stamps must still be admitted.
        if let Err(failure) = self.bind_artifact(service, new_binding) {
            let mut recovery = self
                .abort_staged_artifact(service)
                .err()
                .map(|error| error.failure);
            if recovery.is_none() {
                recovery = self.restart_proven_artifact_after_failure(service, proven);
            }
            if recovery.is_some() {
                slot_suspended(&mut self.services[service.index()]);
            }
            return Err((failure, recovery));
        }
        let stage = self.services[service.index()]
            .pending_artifact
            .take()
            .ok_or((Failure::CheckPending, None))?
            .stage;
        let slot = &mut self.services[service.index()];
        slot.retired_artifact = slot.active_artifact.take();
        slot.active_artifact = Some(stage);
        slot.artifact_recovery = Some(ArtifactRecovery {
            binding: old_binding,
            metadata: old_metadata,
            proven,
            switched_back: false,
        });
        slot.restored = false;
        slot.failure = None;
        if should_run && let Err(failure) = self.start_accepted(service) {
            slot_suspended(&mut self.services[service.index()]);
            if matches!(
                failure,
                Failure::Hook(HookStage::Restore, _) | Failure::Store(StoreError::Durability)
            ) {
                return Err((failure, None));
            }
            let recovery = self
                .recover_staged_artifact(service)
                .err()
                .map(|error| error.failure);
            return Err((failure, recovery));
        }
        self.services[service.index()].artifact_recovery.take();
        self.retire_artifact(service)
            .map_err(|failure| (failure, None))?;
        Ok(())
    }
    /// Explicit same-owner recovery after failed artifact activation. A failed
    /// withdrawal keeps the exact new child/binding and both verified artifacts.
    pub fn recover_staged_artifact(&mut self, service: ServiceId) -> Result<Status, ManagerError> {
        self.independent_recovery(|manager| manager.recover_staged_artifact_inner(service))
    }
    fn recover_staged_artifact_inner(
        &mut self,
        service: ServiceId,
    ) -> Result<Status, ManagerError> {
        if self.closed || self.closing {
            return Err(self.error(service, Failure::Closed, None));
        }
        let recovery = self.services[service.index()]
            .artifact_recovery
            .as_ref()
            .ok_or_else(|| self.error(service, Failure::NotReady, None))?;
        if !recovery.switched_back {
            let binding = recovery.binding.clone();
            let metadata = recovery.metadata.clone();
            self.withdraw_stop(service)
                .map_err(|failure| self.error(service, failure, None))?;
            let generation = self.store.service_state(service.into()).generation;
            let outcome = self
                .store
                .set_artifact_intent(service.into(), generation, metadata)
                .map_err(|error| self.error(service, Failure::Store(error), None))?;
            self.artifact_outcome(service, outcome.durability_error)
                .map_err(|failure| self.error(service, failure, None))?;
            self.bind_artifact(service, binding)
                .map_err(|failure| self.error(service, failure, None))?;
            let slot = &mut self.services[service.index()];
            std::mem::swap(&mut slot.active_artifact, &mut slot.retired_artifact);
            if let Some(recovery) = slot.artifact_recovery.as_mut() {
                recovery.switched_back = true;
            } else {
                return Err(self.error(service, Failure::NotReady, None));
            }
        }
        let proven = self.services[service.index()]
            .artifact_recovery
            .as_ref()
            .is_some_and(|r| r.proven);
        if self.services[service.index()].desired && proven {
            let observed = self
                .observe(service)
                .map_err(|failure| self.error(service, failure, None))?;
            let launch = if observed.is_some_and(|status| status.pid.is_some()) {
                let status = self.status(service)?;
                if status.durability_uncertain {
                    Err(Failure::Store(StoreError::Durability))
                } else if status.ready && status.running_matches_accepted {
                    let record = self
                        .store
                        .service_state(service.into())
                        .current
                        .clone()
                        .ok_or_else(|| self.error(service, Failure::NotConfigured, None))?;
                    self.restore_resources(service, &record)
                } else {
                    Err(Failure::NotReady)
                }
            } else {
                self.start_accepted(service)
            };
            if let Err(failure) = launch {
                slot_suspended(&mut self.services[service.index()]);
                let cleanup = if matches!(
                    failure,
                    Failure::Hook(HookStage::Restore, _) | Failure::Store(StoreError::Durability)
                ) {
                    None
                } else {
                    self.withdraw_stop(service).err()
                };
                return Err(self.error(service, failure, cleanup));
            }
            self.services[service.index()].restored = true;
        } else {
            self.services[service.index()].desired = false;
            self.services[service.index()].needs_recovery = false;
            self.services[service.index()].resource_suspended = true;
        }
        self.services[service.index()].artifact_recovery.take();
        self.retire_artifact(service)
            .map_err(|failure| self.error(service, failure, None))?;
        self.services[service.index()].failure = None;
        self.status(service)
    }
    pub fn abort_staged_artifact(&mut self, service: ServiceId) -> Result<(), ManagerError> {
        self.abort_check(service)
    }
    pub fn abort_check(&mut self, service: ServiceId) -> Result<(), ManagerError> {
        self.independent_recovery(|manager| manager.abort_check_inner(service))
    }
    fn abort_check_inner(&mut self, service: ServiceId) -> Result<(), ManagerError> {
        if self.closed {
            return Err(self.error(service, Failure::Closed, None));
        }
        let slot = &mut self.services[service.index()];
        if let Some(owner) = &mut slot.owner {
            owner
                .abort_check()
                .map_err(|error| self.error(service, Failure::Process(error), None))?;
        }
        self.services[service.index()].pending_check.take();
        let rollback = self.services[service.index()]
            .pending_artifact
            .as_ref()
            .filter(|pending| pending.metadata_committed)
            .map(|pending| (pending.generation, pending.previous_metadata.clone()));
        if let Some((generation, metadata)) = rollback {
            let outcome = self
                .store
                .set_artifact_intent(service.into(), generation, metadata)
                .map_err(|error| self.error(service, Failure::Store(error), None))?;
            self.artifact_outcome(service, outcome.durability_error)
                .map_err(|failure| self.error(service, failure, None))?;
            if let Some(pending) = self.services[service.index()].pending_artifact.as_mut() {
                pending.metadata_committed = false;
            }
        }
        let initial = self.services[service.index()]
            .pending_artifact
            .as_ref()
            .is_some_and(|pending| pending.initial_owner);
        if initial {
            if let Some(owner) = self.services[service.index()].owner.as_mut() {
                owner
                    .close()
                    .map_err(|error| self.error(service, Failure::Process(error), None))?;
            }
            self.services[service.index()].owner.take();
        }
        if let Some(pending) = self.services[service.index()].pending_artifact.as_mut() {
            pending
                .stage
                .cleanup()
                .map_err(|error| self.error(service, Failure::ArtifactStage(error), None))?;
        }
        self.services[service.index()].pending_artifact.take();
        Ok(())
    }
    fn hook(
        &mut self,
        service: ServiceId,
        stage: HookStage,
        record: &ConfigRecord,
    ) -> Result<(), Failure> {
        if stage != HookStage::Cleanup {
            self.operation_check()?;
        }
        let config = config_identity(record);
        let duration = match stage {
            HookStage::PreStart | HookStage::Readiness => self.limits.readiness_timeout,
            HookStage::Cleanup | HookStage::Restore => self.limits.resource_timeout,
        };
        let config_path = self.services[service.index()]
            .config_root
            .join(&record.file);
        let slot = &self.services[service.index()];
        let observe = || {
            slot.owner
                .as_ref()
                .ok_or(ProcessError::Closed)?
                .observe_retained()
        };
        let context = HookContext {
            service,
            config: &config,
            config_path: &config_path,
            run: slot.run.as_ref().map(|run| &run.identity),
            artifact_root: slot
                .run
                .as_ref()
                .map(|run| run.binding.root.as_path())
                .or_else(|| slot.binding.as_ref().map(|binding| binding.root.as_path())),
            artifact_path: slot
                .run
                .as_ref()
                .map(|run| run.binding.path.as_path())
                .or_else(|| slot.binding.as_ref().map(|binding| binding.path.as_path())),
            artifact_file: slot
                .run
                .as_ref()
                .map_or(slot.artifact_file, |run| run.artifact_file),
            artifact_directory: slot
                .run
                .as_ref()
                .map_or(slot.artifact_directory, |run| run.artifact_directory),
            owned_status: slot
                .owner
                .as_ref()
                .map(|_| &observe as &dyn Fn() -> Result<process::Status, ProcessError>),
            deadline: if stage == HookStage::Cleanup {
                Instant::now() + duration
            } else {
                self.operation
                    .as_ref()
                    .map_or(Instant::now() + duration, |(deadline, _)| {
                        (*deadline).min(Instant::now() + duration)
                    })
            },
        };
        self.hooks.call(stage, &context)?;
        if stage != HookStage::Cleanup {
            self.operation_check()?;
        }
        Ok(())
    }
    fn live_record(&self, service: ServiceId) -> Result<ConfigRecord, Failure> {
        let slot = &self.services[service.index()];
        // Retain the exact launch record even when restore advances current or
        // recovery fails before rebinding. Cleanup always has real child context.
        if let Some(run) = &slot.run {
            return Ok(run.record.clone());
        }
        self.store
            .service_state(service.into())
            .current
            .clone()
            .ok_or(Failure::NotConfigured)
    }
    fn withdraw_stop(&mut self, service: ServiceId) -> Result<(), Failure> {
        let observed = self.observe(service)?;
        if !observed.is_some_and(|status| {
            status.pid.is_some() && status.mode == Some(process::LaunchMode::Run)
        }) {
            self.services[service.index()].run = None;
            return Ok(());
        }
        let record = self.live_record(service)?;
        // Independent resource deadline; no TERM is sent if this hook fails.
        let result = self.hook(service, HookStage::Cleanup, &record);
        if let Err(failure) = result {
            let slot = &mut self.services[service.index()];
            slot.needs_recovery = true;
            slot.resource_suspended = true;
            return Err(failure);
        }
        self.services[service.index()].resource_suspended = true;
        self.services[service.index()]
            .owner
            .as_mut()
            .ok_or(Failure::ArtifactUnavailable)?
            .stop_with_cleanup(|| Ok::<(), HookError>(()))
            .map_err(Failure::Process)?;
        self.services[service.index()].run = None;
        Ok(())
    }
    fn launch_observe(
        &mut self,
        service: ServiceId,
        record: &ConfigRecord,
        bytes: &[u8],
    ) -> Result<(), Failure> {
        let readback = self
            .store
            .read_config(service.into(), record)
            .map_err(Failure::Store)?;
        if readback != bytes {
            return Err(Failure::Store(StoreError::Verification));
        }
        drop(readback);
        self.hook(service, HookStage::PreStart, record)?;
        let path = self.services[service.index()]
            .config_root
            .join(&record.file);
        let spec = self.spec(service, &path, bytes)?;
        self.operation_check()?;
        let operation = self.operation.clone();
        let owner = self.services[service.index()]
            .owner
            .as_mut()
            .ok_or(Failure::ArtifactUnavailable)?;
        let launch = if let Some((deadline, cancel)) = operation {
            owner.start_until(spec, deadline, Some(cancel))
        } else {
            owner.start(spec)
        };
        // A launch/pipe observation error can still retain a child. Capture its
        // owned identity so failed-launch cleanup cannot lose the handle.
        let observed = match launch {
            Ok(status) => status,
            Err(error) => {
                if let Ok(Some(status)) = self.observe(service) {
                    self.bind_run(service, record, status);
                }
                return Err(Failure::Process(error));
            }
        };
        self.bind_run(service, record, observed);
        self.require_live(service)?;
        self.hook(service, HookStage::Readiness, record)?;
        self.require_live(service)?;
        // ProcessOwner rehashes at launch; also require unchanged exact stored
        // bytes after the real readiness callback before constructing its proof.
        if self
            .store
            .read_config(service.into(), record)
            .map_err(Failure::Store)?
            != bytes
        {
            return Err(Failure::Store(StoreError::Readiness));
        }
        Ok(())
    }
    fn bind_run(&mut self, service: ServiceId, record: &ConfigRecord, status: process::Status) {
        if status.mode == Some(process::LaunchMode::Run)
            && let Some(pid) = status.pid
        {
            self.services[service.index()].run = Some(Run {
                identity: OwnedRunIdentity {
                    pid,
                    launch_generation: record.generation,
                    accepted_generation: record.generation,
                    sha256: record.sha256.clone(),
                },
                record: record.clone(),
                binding: self.services[service.index()]
                    .binding
                    .as_ref()
                    .expect("launched artifact binding")
                    .clone(),
                artifact_file: self.services[service.index()].artifact_file,
                artifact_directory: self.services[service.index()].artifact_directory,
                ready: false,
            });
        }
    }
    fn require_live(&mut self, service: ServiceId) -> Result<(), Failure> {
        let status = self.observe(service)?.ok_or(Failure::ArtifactUnavailable)?;
        let identity = self.services[service.index()]
            .run
            .as_ref()
            .ok_or(Failure::Exited)?;
        if status.phase != process::Phase::Running
            || status.mode != Some(process::LaunchMode::Run)
            || status.pid != Some(identity.identity.pid)
            || !self.services[service.index()]
                .binding
                .as_ref()
                .is_some_and(|binding| {
                    binding.path == identity.binding.path
                        && binding.sha256 == identity.binding.sha256
                })
        {
            return Err(Failure::Exited);
        }
        Ok(())
    }
    fn outcome(
        &mut self,
        service: ServiceId,
        durability: Option<StoreError>,
    ) -> Result<(), Failure> {
        if durability.is_some() {
            let slot = &mut self.services[service.index()];
            slot.durability_uncertain = true;
            slot.needs_recovery = true;
            slot.resource_suspended = true;
            return Err(Failure::Store(StoreError::Durability));
        }
        Ok(())
    }
    fn artifact_outcome(
        &mut self,
        service: ServiceId,
        durability: Option<StoreError>,
    ) -> Result<(), Failure> {
        let slot = &mut self.services[service.index()];
        slot.artifact_durability_uncertain = durability.is_some();
        if durability.is_some() {
            slot_suspended(slot);
            return Err(Failure::Store(StoreError::Durability));
        }
        Ok(())
    }
    fn start_accepted(&mut self, service: ServiceId) -> Result<(), Failure> {
        self.operation_check()?;
        let record = self
            .store
            .service_state(service.into())
            .current
            .clone()
            .ok_or(Failure::NotConfigured)?;
        let bytes = self
            .store
            .read_config(service.into(), &record)
            .map_err(Failure::Store)?;
        self.launch_observe(service, &record, &bytes)?;
        let proof = ReadinessProof::observed(service.into(), record.generation, &bytes);
        let result = self
            .store
            .mark_ready(service.into(), &proof)
            .map_err(Failure::Store)?;
        self.services[service.index()]
            .run
            .as_mut()
            .ok_or(Failure::Exited)?
            .ready = true;
        self.outcome(service, result.durability_error)?;
        self.restore_resources(service, &record)
    }
    fn restore_resources(
        &mut self,
        service: ServiceId,
        record: &ConfigRecord,
    ) -> Result<(), Failure> {
        self.require_live(service)?;
        if let Err(failure) = self.hook(service, HookStage::Restore, record) {
            let slot = &mut self.services[service.index()];
            slot.resource_suspended = true;
            slot.needs_recovery = true;
            return Err(failure);
        }
        self.require_live(service)?;
        let slot = &mut self.services[service.index()];
        slot.resource_suspended = false;
        slot.needs_recovery = slot.durability_uncertain || slot.artifact_durability_uncertain;
        Ok(())
    }
    fn freeze(&mut self, service: ServiceId) -> Result<Option<FrozenReady>, Failure> {
        let status = self.status(service).map_err(|error| error.failure)?;
        if !status.ready || !status.desired || status.needs_recovery {
            return Ok(None);
        }
        let record = self
            .store
            .service_state(service.into())
            .current
            .clone()
            .ok_or(Failure::NotConfigured)?;
        let bytes = self
            .store
            .read_config(service.into(), &record)
            .map_err(Failure::Store)?;
        let binding = self.services[service.index()]
            .binding
            .clone()
            .ok_or(Failure::ArtifactUnavailable)?;
        Ok(Some(FrozenReady {
            record,
            bytes,
            binding,
        }))
    }
    /// Validates while the old Run lives. Bad checker/conflicting generation
    /// leaves accepted bytes and the Run unchanged. Off services remain off.
    pub fn configure(
        &mut self,
        service: ServiceId,
        expected_generation: u64,
        bytes: &[u8],
        cancel: Option<Arc<AtomicBool>>,
    ) -> Result<Status, ManagerError> {
        let result = self.configure_inner(service, expected_generation, bytes, cancel);
        match result {
            Ok(()) => self.status(service),
            Err((failure, recovery)) => {
                self.services[service.index()].failure = Some(failure);
                Err(self.error(service, failure, recovery))
            }
        }
    }
    fn configure_inner(
        &mut self,
        service: ServiceId,
        expected: u64,
        bytes: &[u8],
        cancel: Option<Arc<AtomicBool>>,
    ) -> Result<(), (Failure, Option<Failure>)> {
        self.generation(service, expected)
            .map_err(|failure| (failure, None))?;
        let candidate = self
            .store
            .stage_candidate(service.into(), bytes)
            .map_err(|error| (Failure::Store(error), None))?;
        let candidate = self
            .check(service, candidate, bytes, cancel)
            .map_err(|failure| (failure, None))?;
        self.generation(service, expected)
            .map_err(|failure| (failure, None))?;
        let frozen = self.freeze(service).map_err(|failure| (failure, None))?;
        let should_run = self.services[service.index()].desired;
        self.withdraw_stop(service)
            .map_err(|failure| (failure, None))?;
        let proof = VerificationProof::checked(service.into(), bytes);
        let commit = self.store.commit_verified_candidate(candidate, &proof);
        let outcome = match commit {
            Ok(outcome) => outcome,
            Err(error) => {
                let failure = Failure::Store(error);
                let recovery = if frozen.is_some() {
                    match self.start_accepted(service) {
                        Ok(()) => {
                            self.services[service.index()].restored = true;
                            None
                        }
                        Err(recovery) => {
                            self.services[service.index()].needs_recovery = true;
                            self.services[service.index()].resource_suspended = true;
                            if !matches!(
                                recovery,
                                Failure::Hook(HookStage::Restore, _)
                                    | Failure::Store(StoreError::Durability)
                            ) && let Err(cleanup) = self.withdraw_stop(service)
                            {
                                return Err((failure, Some(cleanup)));
                            }
                            Some(recovery)
                        }
                    }
                } else {
                    self.services[service.index()].needs_recovery = should_run;
                    None
                };
                return Err((failure, recovery));
            }
        };
        self.services[service.index()].restored = false;
        self.outcome(service, outcome.durability_error)
            .map_err(|failure| (failure, None))?;
        if !should_run {
            let slot = &mut self.services[service.index()];
            slot.failure = None;
            slot.needs_recovery = slot.durability_uncertain;
            slot.resource_suspended = true;
            return Ok(());
        }
        match self.start_accepted(service) {
            Ok(()) => {
                self.services[service.index()].failure = None;
                Ok(())
            }
            Err(failure) => Err((failure, self.failed_change(service, failure, frozen))),
        }
    }
    fn failed_change(
        &mut self,
        service: ServiceId,
        failure: Failure,
        frozen: Option<FrozenReady>,
    ) -> Option<Failure> {
        // Resource restore failure keeps a genuinely ready core alive. An
        // authoritative durability error also must not trigger blind rollback.
        if matches!(
            failure,
            Failure::Hook(HookStage::Restore, _) | Failure::Store(StoreError::Durability)
        ) {
            self.services[service.index()].needs_recovery = true;
            self.services[service.index()].resource_suspended = true;
            return None;
        }
        if let Err(cleanup) = self.withdraw_stop(service) {
            self.services[service.index()].needs_recovery = true;
            return Some(cleanup);
        }
        if let Some(frozen) = frozen {
            if let Err(recovery) = self.recover(service, frozen) {
                self.services[service.index()].needs_recovery = true;
                self.services[service.index()].resource_suspended = true;
                // Recovery readiness can fail with its own child retained.
                if !matches!(
                    recovery,
                    Failure::Hook(HookStage::Restore, _) | Failure::Store(StoreError::Durability)
                ) && let Err(cleanup) = self.withdraw_stop(service)
                {
                    return Some(cleanup);
                }
                return Some(recovery);
            }
        } else {
            let slot = &mut self.services[service.index()];
            slot.desired = false;
            slot.needs_recovery = true;
            slot.resource_suspended = true;
        }
        None
    }
    fn recover(&mut self, service: ServiceId, frozen: FrozenReady) -> Result<(), Failure> {
        let last_good = self.store.service_state(service.into()).last_good.as_ref();
        if last_good != Some(&frozen.record) || !frozen.record.ready {
            return Err(Failure::NotReady);
        }
        // No arbitrary replacement artifact is admitted through configure.
        // Keep the exact trusted binding that owned the proven old Run.
        self.services[service.index()].binding = Some(frozen.binding.clone());
        let candidate = self
            .store
            .stage_last_good(service.into())
            .map_err(Failure::Store)?;
        let candidate = self.check(service, candidate, &frozen.bytes, None)?;
        self.recover_checked(service, frozen, candidate)
    }
    fn recover_checked(
        &mut self,
        service: ServiceId,
        frozen: FrozenReady,
        candidate: Candidate,
    ) -> Result<(), Failure> {
        self.launch_observe(service, &frozen.record, &frozen.bytes)?;
        let verification = VerificationProof::checked(service.into(), &frozen.bytes);
        let readiness =
            ReadinessProof::observed(service.into(), frozen.record.generation, &frozen.bytes);
        let outcome = self
            .store
            .restore_proven_last_good(candidate, &verification, &readiness)
            .map_err(Failure::Store)?;
        let record = outcome
            .state
            .current
            .as_ref()
            .ok_or(Failure::NotConfigured)?;
        let slot = &mut self.services[service.index()];
        let run = slot.run.as_mut().ok_or(Failure::Exited)?;
        run.identity.accepted_generation = record.generation;
        run.ready = true;
        slot.restored = true;
        self.outcome(service, outcome.durability_error)?;
        self.restore_resources(service, record)
    }
    pub fn start(&mut self, service: ServiceId) -> Result<Status, ManagerError> {
        self.ensure(service)
            .map_err(|failure| self.error(service, failure, None))?;
        let observed = self
            .observe(service)
            .map_err(|failure| self.error(service, failure, None))?;
        if observed.is_some_and(|status| status.pid.is_some()) {
            // Idempotent only for a healthy matching owned Run.
            let status = self.status(service)?;
            if status.active {
                self.services[service.index()].desired = true;
                return self.status(service);
            }
            return Err(self.error(service, Failure::Process(ProcessError::Busy), None));
        }
        self.services[service.index()].desired = true;
        self.services[service.index()].restored = false;
        match self.start_accepted(service) {
            Ok(()) => {
                self.services[service.index()].failure = None;
                self.status(service)
            }
            Err(failure) => {
                let recovery = self.failed_change(service, failure, None);
                self.services[service.index()].failure = Some(failure);
                Err(self.error(service, failure, recovery))
            }
        }
    }
    pub fn stop(&mut self, service: ServiceId) -> Result<Status, ManagerError> {
        // Off intent is latched even when checker/staged-file abort fails. A
        // retained checker must not block withdrawal of the old Run.
        self.services[service.index()].desired = false;
        if self.closed {
            return Err(self.error(service, Failure::Closed, None));
        }
        let abort_failure = self.abort_check(service).err();
        match self.withdraw_stop(service) {
            Ok(()) => {
                let slot = &mut self.services[service.index()];
                slot.resource_suspended = true;
                slot.needs_recovery = slot.durability_uncertain
                    || slot.artifact_durability_uncertain
                    || abort_failure.is_some()
                    || slot.artifact_recovery.is_some()
                    || slot.retired_artifact.is_some();
                slot.failure = abort_failure.map(|error| error.failure);
                if let Some(error) = abort_failure {
                    return Err(error);
                }
                self.status(service)
            }
            Err(failure) => {
                self.services[service.index()].failure = Some(failure);
                Err(self.error(service, failure, abort_failure.map(|error| error.failure)))
            }
        }
    }
    /// Explicit user restart, not an automatic retry policy.
    pub fn restart(&mut self, service: ServiceId) -> Result<Status, ManagerError> {
        self.ensure(service)
            .map_err(|failure| self.error(service, failure, None))?;
        let frozen = self
            .freeze(service)
            .map_err(|failure| self.error(service, failure, None))?;
        self.withdraw_stop(service)
            .map_err(|failure| self.error(service, failure, None))?;
        self.services[service.index()].desired = true;
        match self.start_accepted(service) {
            Ok(()) => {
                self.services[service.index()].failure = None;
                self.status(service)
            }
            Err(failure) => {
                // No config commit occurred. Restart the unchanged proven bytes
                // after withdrawal, without manufacturing a new generation.
                let recovery = self.failed_restart(service, failure, frozen.is_some());
                self.services[service.index()].failure = Some(failure);
                Err(self.error(service, failure, recovery))
            }
        }
    }
    fn failed_restart(
        &mut self,
        service: ServiceId,
        failure: Failure,
        proven: bool,
    ) -> Option<Failure> {
        if matches!(
            failure,
            Failure::Hook(HookStage::Restore, _) | Failure::Store(StoreError::Durability)
        ) {
            return self.failed_change(service, failure, None);
        }
        if let Err(error) = self.withdraw_stop(service) {
            return Some(error);
        }
        if proven {
            if let Err(error) = self.start_accepted(service) {
                self.services[service.index()].needs_recovery = true;
                if !matches!(
                    error,
                    Failure::Hook(HookStage::Restore, _) | Failure::Store(StoreError::Durability)
                ) && let Err(cleanup) = self.withdraw_stop(service)
                {
                    return Some(cleanup);
                }
                return Some(error);
            }
            self.services[service.index()].restored = true;
            return None;
        }
        self.services[service.index()].desired = false;
        self.services[service.index()].needs_recovery = true;
        None
    }
    /// Explicit restore is refused while off: native readiness cannot be
    /// manufactured for a stopped service. Call explicit start first.
    pub fn restore(
        &mut self,
        service: ServiceId,
        expected_generation: u64,
    ) -> Result<Status, ManagerError> {
        let result = self.restore_inner(service, expected_generation);
        match result {
            Ok(()) => {
                self.services[service.index()].failure = None;
                self.status(service)
            }
            Err((failure, recovery)) => {
                self.services[service.index()].failure = Some(failure);
                Err(self.error(service, failure, recovery))
            }
        }
    }
    fn restore_inner(
        &mut self,
        service: ServiceId,
        expected: u64,
    ) -> Result<(), (Failure, Option<Failure>)> {
        self.generation(service, expected)
            .map_err(|failure| (failure, None))?;
        if !self.services[service.index()].desired {
            return Err((Failure::NotReady, None));
        }
        let record = self
            .store
            .service_state(service.into())
            .last_good
            .clone()
            .filter(|record| record.ready)
            .ok_or((Failure::NotReady, None))?;
        let bytes = self
            .store
            .read_config(service.into(), &record)
            .map_err(|error| (Failure::Store(error), None))?;
        let binding = self.services[service.index()]
            .binding
            .clone()
            .ok_or((Failure::ArtifactUnavailable, None))?;
        let candidate = self
            .store
            .stage_last_good(service.into())
            .map_err(|error| (Failure::Store(error), None))?;
        let candidate = self
            .check(service, candidate, &bytes, None)
            .map_err(|failure| (failure, None))?;
        // Retain the already checked exact candidate across withdrawal. There
        // is no second verifier after stopping the working Run.
        self.generation(service, expected)
            .map_err(|failure| (failure, None))?;
        self.withdraw_stop(service)
            .map_err(|failure| (failure, None))?;
        let frozen = FrozenReady {
            record,
            bytes,
            binding,
        };
        if let Err(failure) = self.recover_checked(service, frozen, candidate) {
            let recovery = if matches!(
                failure,
                Failure::Hook(HookStage::Restore, _) | Failure::Store(StoreError::Durability)
            ) {
                None
            } else {
                self.withdraw_stop(service).err()
            };
            self.services[service.index()].needs_recovery = true;
            self.services[service.index()].resource_suspended = true;
            return Err((failure, recovery));
        }
        Ok(())
    }
    /// Required explicit exit handling. Status alone observes but never reaps or
    /// withdraws. This method does not schedule or automatically restart.
    pub fn handle_exit(&mut self, service: ServiceId) -> Result<Status, ManagerError> {
        if self.closed {
            return Err(self.error(service, Failure::Closed, None));
        }
        let observed = self
            .observe(service)
            .map_err(|failure| self.error(service, failure, None))?;
        if observed.is_some_and(|status| {
            status.phase == process::Phase::Exited && status.mode == Some(process::LaunchMode::Run)
        }) {
            // Staged checker/file cleanup must not leave dead-core hooks active.
            // Attempt both and retain any unresolved exact handle for retry.
            let abort_failure = self.abort_check(service).err();
            let withdrawn = self.withdraw_stop(service);
            let slot = &mut self.services[service.index()];
            slot.needs_recovery = true;
            slot.resource_suspended = true;
            slot.failure = Some(Failure::Exited);
            if let Err(failure) = withdrawn {
                return Err(self.error(service, failure, abort_failure.map(|error| error.failure)));
            }
            if let Some(error) = abort_failure {
                return Err(error);
            }
        }
        self.status(service)
    }
    /// Attempts both services even if one fails. Any failed withdrawal retains
    /// that service's owner and child; the manager remains open for retry.
    pub fn close(&mut self) -> Result<(), ManagerError> {
        if self.closed {
            return Ok(());
        }
        self.closing = true;
        let mut first = None;
        for service in SERVICES {
            self.services[service.index()].desired = false;
            if let Err(error) = self.abort_check(service)
                && first.is_none()
            {
                first = Some(error);
            }
            if let Err(error) = self.stop(service)
                && first.is_none()
            {
                first = Some(error);
            }
        }
        if let Some(error) = first {
            return Err(error);
        }
        for service in SERVICES {
            if let Some(owner) = &mut self.services[service.index()].owner {
                owner
                    .close()
                    .map_err(|error| self.error(service, Failure::Process(error), None))?;
            }
            self.services[service.index()].owner.take();
        }
        for service in SERVICES {
            if let Some(stage) = self.services[service.index()].active_artifact.as_mut() {
                stage
                    .cleanup()
                    .map_err(|error| self.error(service, Failure::ArtifactStage(error), None))?;
            }
            self.services[service.index()].active_artifact.take();
            if let Some(stage) = self.services[service.index()].retired_artifact.as_mut() {
                stage
                    .cleanup()
                    .map_err(|error| self.error(service, Failure::ArtifactStage(error), None))?;
            }
            self.services[service.index()].retired_artifact.take();
            self.services[service.index()].artifact_recovery.take();
        }
        self.closed = true;
        Ok(())
    }
}
fn current_status_after_io(
    result: Result<Status, ManagerError>,
    deadline: Instant,
    completed: Instant,
) -> Result<Status, HookError> {
    if completed >= deadline {
        return Err(HookError::Deadline);
    }
    result.map_err(|_| HookError::Failed)
}
fn slot_suspended(slot: &mut Service) {
    slot.needs_recovery = true;
    slot.resource_suspended = true;
}
fn config_identity(record: &ConfigRecord) -> ConfigIdentity {
    ConfigIdentity {
        generation: record.generation,
        sha256: record.sha256.clone(),
    }
}

#[cfg(test)]
mod artifact_fault_tests {
    use super::*;
    use std::{cell::Cell, rc::Rc, sync::Mutex, sync::atomic::AtomicU64};
    static SERIAL: Mutex<()> = Mutex::new(());
    static NEXT: AtomicU64 = AtomicU64::new(0);
    const CORE: &str = r#"#!/bin/sh
case "$1" in check|verify) exit 0;; esac
trap 'exit 0' TERM
printf '%s\n' "$$" > "$TMPDIR/marker"
IFS= read -r value < "$TMPDIR/wait"
"#;
    struct Fixture {
        root: PathBuf,
        cleanup: Rc<Cell<u32>>,
    }
    impl Fixture {
        fn new() -> Self {
            let root = fs::canonicalize(std::env::temp_dir())
                .unwrap()
                .join(format!(
                    "b6p-artifact-manager-fault-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                ));
            fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
            for part in ["artifacts", "run", "run/sing-box"] {
                fs::DirBuilder::new()
                    .mode(0o700)
                    .create(root.join(part))
                    .unwrap();
            }
            let core = CORE.to_owned();
            fs::write(root.join("artifacts/fake-core"), core.as_bytes()).unwrap();
            fs::set_permissions(
                root.join("artifacts/fake-core"),
                fs::Permissions::from_mode(0o700),
            )
            .unwrap();
            let name = std::ffi::CString::new(
                root.join("run/sing-box/wait")
                    .as_os_str()
                    .as_encoded_bytes(),
            )
            .unwrap();
            assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
            Self {
                root,
                cleanup: Rc::new(Cell::new(0)),
            }
        }
        fn manager(&self) -> Manager {
            self.manager_mode(true)
        }
        fn unbound(&self) -> Manager {
            self.manager_mode(false)
        }
        fn manager_mode(&self, bound: bool) -> Manager {
            let count = self.cleanup.clone();
            let marker = self.root.join("run/sing-box/marker");
            let hooks = Hooks::new(
                |_| Ok(()),
                move |context| {
                    let expected = context.run.ok_or(HookError::Failed)?.pid();
                    while !fs::read_to_string(&marker)
                        .is_ok_and(|text| text.trim() == expected.to_string())
                    {
                        if Instant::now() >= context.deadline {
                            return Err(HookError::Deadline);
                        }
                        std::thread::yield_now();
                    }
                    Ok(())
                },
                move |_| {
                    count.set(count.get() + 1);
                    Ok(())
                },
                |_| Ok(()),
            );
            let core = CORE.to_owned();
            Manager::open(
                self.root.join("services"),
                self.root.join("run"),
                ArtifactBindings {
                    sing_box: bound.then(|| {
                        ArtifactBinding::trusted_local(
                            ServiceId::SingBox,
                            self.root.join("artifacts"),
                            self.root.join("artifacts/fake-core"),
                            Sha256::digest(core.as_bytes()).into(),
                            ArtifactProvenance::TrustedLocalModule,
                        )
                    }),
                    frpc: None,
                },
                hooks,
                Limits {
                    process: process::Limits {
                        term_grace: std::time::Duration::from_millis(100),
                        kill_grace: std::time::Duration::from_secs(1),
                        check_timeout: std::time::Duration::from_secs(2),
                    },
                    readiness_timeout: std::time::Duration::from_secs(2),
                    resource_timeout: std::time::Duration::from_secs(1),
                },
            )
            .unwrap()
        }
        fn stage(&self) -> Stage {
            let core = CORE.to_owned();
            let metadata = store::Artifact {
                url: "https://example.invalid/fault-core".into(),
                sha256: format!("{:x}", Sha256::digest(core.as_bytes())),
                compression: "none".into(),
                version: "new".into(),
            };
            let cancel = AtomicBool::new(false);
            let budget = crate::readiness_tun::Budget {
                deadline: Instant::now() + std::time::Duration::from_secs(2),
                cancel: &cancel,
            };
            Stage::from_reader(
                &self.root.join("artifacts"),
                &metadata,
                core.as_bytes(),
                &budget,
            )
            .unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).unwrap();
        }
    }
    #[test]
    fn metadata_precommit_failure_preserves_old_binding_and_restarts_proven_config() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let fixture = Fixture::new();
        let mut manager = fixture.manager();
        let service = ServiceId::SingBox;
        manager.configure(service, 0, b"good\n", None).unwrap();
        let old = manager.start(service).unwrap();
        let manifest = fs::read(fixture.root.join("services/sing-box/state.json")).unwrap();
        manager
            .check_staged_artifact(service, 1, fixture.stage(), None)
            .unwrap();
        manager.store.inject_artifact_fault(false);
        let error = manager.activate_staged_artifact(service, 1).unwrap_err();
        assert_eq!(error.failure, Failure::Store(StoreError::Storage));
        assert_eq!(error.recovery_failure, None);
        let status = manager.status(service).unwrap();
        assert!(status.active && status.ready);
        assert_ne!(status.pid, old.pid);
        assert_eq!(
            fs::read(fixture.root.join("services/sing-box/state.json")).unwrap(),
            manifest
        );
        assert!(manager.staged_artifact_checked(service));
        manager.abort_staged_artifact(service).unwrap();
        manager.close().unwrap();
    }
    #[test]
    fn metadata_postrename_uncertainty_retains_new_intent_and_old_binding_until_explicit_abort() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let fixture = Fixture::new();
        let mut manager = fixture.manager();
        let service = ServiceId::SingBox;
        manager.configure(service, 0, b"good\n", None).unwrap();
        manager.start(service).unwrap();
        let old = manager.services[service.index()]
            .binding
            .as_ref()
            .unwrap()
            .path
            .clone();
        manager
            .check_staged_artifact(service, 1, fixture.stage(), None)
            .unwrap();
        manager.store.inject_artifact_fault(true);
        let error = manager.activate_staged_artifact(service, 1).unwrap_err();
        assert_eq!(error.failure, Failure::Store(StoreError::Durability));
        let status = manager.status(service).unwrap();
        assert!(
            status.pid.is_none()
                && !status.active
                && status.durability_uncertain
                && status.needs_recovery
        );
        assert_eq!(
            manager.services[service.index()]
                .binding
                .as_ref()
                .unwrap()
                .path,
            old
        );
        assert_eq!(
            manager
                .store
                .snapshot(service.into())
                .artifact
                .as_ref()
                .unwrap()
                .version,
            "new"
        );
        assert!(manager.staged_artifact_checked(service));
        manager.abort_staged_artifact(service).unwrap();
        assert!(manager.store.snapshot(service.into()).artifact.is_none());
        assert!(!manager.status(service).unwrap().durability_uncertain);
        assert!(manager.start(service).unwrap().active);
        manager.close().unwrap();
    }
    #[test]
    fn initial_metadata_precommit_failure_cleans_stage_and_owner_then_allows_same_manager_retry() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let fixture = Fixture::new();
        let mut manager = fixture.unbound();
        let service = ServiceId::SingBox;
        let stage = fixture.stage();
        let path = stage.admitted().unwrap().path.to_path_buf();
        manager.store.inject_artifact_fault(false);
        let failure = manager
            .initialize_staged_artifact(service, 0, stage, None)
            .unwrap_err();
        assert_eq!(failure.failure, Failure::Store(StoreError::Storage));
        assert_eq!(failure.recovery_failure, None);
        assert!(!path.exists());
        assert!(manager.services[service.index()].owner.is_none());
        assert!(manager.services[service.index()].pending_artifact.is_none());
        assert!(manager.store.snapshot(service.into()).artifact.is_none());
        assert!(!manager.status(service).unwrap().artifact_available);
        assert!(
            manager
                .initialize_staged_artifact(service, 0, fixture.stage(), None)
                .unwrap()
                .artifact_available
        );
        manager.close().unwrap();
    }
    #[test]
    fn initial_metadata_postrename_uncertainty_keeps_unavailable_stage_until_explicit_abort() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let fixture = Fixture::new();
        let mut manager = fixture.unbound();
        let service = ServiceId::SingBox;
        let stage = fixture.stage();
        let path = stage.admitted().unwrap().path.to_path_buf();
        manager.store.inject_artifact_fault(true);
        let failure = manager
            .initialize_staged_artifact(service, 0, stage, None)
            .unwrap_err();
        assert_eq!(failure.failure, Failure::Store(StoreError::Durability));
        let status = manager.status(service).unwrap();
        assert!(
            !status.artifact_available
                && !status.active
                && !status.desired
                && status.pid.is_none()
                && status.durability_uncertain
        );
        assert!(path.exists());
        assert!(manager.services[service.index()].owner.is_some());
        assert!(manager.store.snapshot(service.into()).artifact.is_some());
        assert_eq!(
            manager.start(service).unwrap_err().failure,
            Failure::CheckPending
        );
        manager.abort_staged_artifact(service).unwrap();
        assert!(!path.exists());
        assert!(manager.services[service.index()].owner.is_none());
        assert!(manager.store.snapshot(service.into()).artifact.is_none());
        assert!(!manager.status(service).unwrap().durability_uncertain);
        assert!(
            manager
                .initialize_staged_artifact(service, 0, fixture.stage(), None)
                .unwrap()
                .artifact_available
        );
        manager.close().unwrap();
    }
    #[test]
    fn acquire_reserves_configured_recovery_budgets_without_extending_limits() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let fixture = Fixture::new();
        let mut manager = fixture.unbound();
        assert_eq!(
            manager.acquisition_fetch_budget().unwrap(),
            Duration::from_secs(45)
        );
        manager.limits = Limits::default();
        assert_eq!(
            manager.acquisition_fetch_budget().unwrap(),
            Duration::from_secs(43)
        );
        manager.limits.readiness_timeout = Duration::from_secs(30);
        manager.limits.resource_timeout = Duration::from_secs(30);
        assert_eq!(
            manager.acquisition_fetch_budget(),
            Err(Failure::InvalidInput)
        );
        manager.close().unwrap();
    }
    #[test]
    fn expired_stage_acquisition_has_zero_withdrawal_or_metadata_and_preserves_old_run() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let fixture = Fixture::new();
        let mut manager = fixture.manager();
        let service = ServiceId::SingBox;
        manager.configure(service, 0, b"good\n", None).unwrap();
        let old = manager.start(service).unwrap();
        let manifest = fs::read(fixture.root.join("services/sing-box/state.json")).unwrap();
        let stage = fixture.stage();
        let path = stage.admitted().unwrap().path.to_path_buf();
        let error = manager
            .acquire_verified_stage(
                service,
                1,
                stage,
                Instant::now(),
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap_err();
        assert_eq!(error.failure, Failure::OperationDeadline);
        assert_eq!(manager.status(service).unwrap().pid, old.pid);
        assert_eq!(fixture.cleanup.get(), 0);
        assert_eq!(
            fs::read(fixture.root.join("services/sing-box/state.json")).unwrap(),
            manifest
        );
        assert!(!path.exists());
        assert!(manager.operation.is_none());
        manager.close().unwrap();
    }
    #[test]
    fn cancelled_stage_acquisition_cannot_initialize_missing_owner_or_commit_metadata() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let fixture = Fixture::new();
        let mut manager = fixture.unbound();
        let service = ServiceId::SingBox;
        let stage = fixture.stage();
        let path = stage.admitted().unwrap().path.to_path_buf();
        let error = manager
            .acquire_verified_stage(
                service,
                0,
                stage,
                Instant::now() + Duration::from_secs(1),
                Arc::new(AtomicBool::new(true)),
            )
            .unwrap_err();
        assert_eq!(error.failure, Failure::Cancelled);
        assert!(manager.services[service.index()].owner.is_none());
        assert!(manager.store.snapshot(service.into()).artifact.is_none());
        assert!(!path.exists());
        assert!(manager.operation.is_none());
        manager.close().unwrap();
    }
    #[test]
    fn cancellation_after_verified_stage_before_withdrawal_keeps_old_run_and_manifest() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let fixture = Fixture::new();
        let mut manager = fixture.manager();
        let service = ServiceId::SingBox;
        manager.configure(service, 0, b"good\n", None).unwrap();
        let old = manager.start(service).unwrap();
        let manifest = fs::read(fixture.root.join("services/sing-box/state.json")).unwrap();
        let stage = fixture.stage();
        let path = stage.admitted().unwrap().path.to_path_buf();
        manager.artifact_cancel_step = Some(false);
        let error = manager
            .acquire_verified_stage(
                service,
                1,
                stage,
                Instant::now() + Duration::from_secs(3),
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap_err();
        assert_eq!(error.failure, Failure::Cancelled);
        assert_eq!(error.recovery_failure, None);
        let actual = manager.status(service).unwrap();
        assert_eq!(actual.pid, old.pid);
        assert!(actual.active);
        assert_eq!(fixture.cleanup.get(), 0);
        assert_eq!(
            fs::read(fixture.root.join("services/sing-box/state.json")).unwrap(),
            manifest
        );
        assert!(!path.exists());
        assert!(manager.operation.is_none());
        manager.close().unwrap();
    }
    #[test]
    fn cancellation_after_withdrawal_restores_old_proven_run_with_independent_budget() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let fixture = Fixture::new();
        let mut manager = fixture.manager();
        let service = ServiceId::SingBox;
        manager.configure(service, 0, b"good\n", None).unwrap();
        let old = manager.start(service).unwrap();
        let manifest = fs::read(fixture.root.join("services/sing-box/state.json")).unwrap();
        let stage = fixture.stage();
        let path = stage.admitted().unwrap().path.to_path_buf();
        manager.artifact_cancel_step = Some(true);
        let error = manager
            .acquire_verified_stage(
                service,
                1,
                stage,
                Instant::now() + Duration::from_secs(3),
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap_err();
        assert_eq!(error.failure, Failure::Cancelled);
        assert_eq!(error.recovery_failure, None);
        let actual = manager.status(service).unwrap();
        assert_ne!(actual.pid, old.pid);
        assert!(actual.active && actual.restored);
        assert_eq!(fixture.cleanup.get(), 1);
        assert_eq!(
            fs::read(fixture.root.join("services/sing-box/state.json")).unwrap(),
            manifest
        );
        assert!(!path.exists());
        assert!(manager.operation.is_none());
        manager.close().unwrap();
    }
    #[test]
    fn current_status_sample_completed_after_deadline_cannot_attest_success() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let fixture = Fixture::new();
        let mut manager = fixture.manager();
        let service = ServiceId::SingBox;
        manager.configure(service, 0, b"good\n", None).unwrap();
        manager.start(service).unwrap();
        let sample = manager.status(service);
        let deadline = Instant::now() + Duration::from_secs(1);
        assert!(sample.as_ref().unwrap().active);
        assert_eq!(
            current_status_after_io(sample.clone(), deadline, deadline),
            Err(HookError::Deadline)
        );
        assert_eq!(
            current_status_after_io(sample, deadline, deadline - Duration::from_nanos(1))
                .unwrap()
                .state,
            State::Running
        );
        let error = manager.error(service, Failure::Exited, None);
        assert_eq!(
            current_status_after_io(Err(error), deadline, deadline),
            Err(HookError::Deadline)
        );
        assert_eq!(
            current_status_after_io(Err(error), deadline, deadline - Duration::from_nanos(1)),
            Err(HookError::Failed)
        );
        manager.close().unwrap();
    }
}

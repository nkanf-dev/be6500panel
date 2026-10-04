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
use crate::runtime_process::{
    self as process, LaunchSpec, ProcessError, ProcessOwner, TrustedRoots,
};
use crate::runtime_store::{
    self as store, Candidate, ConfigRecord, ReadinessProof, RuntimeStore, StoreError,
    VerificationProof,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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
    NotConfigured,
    NotReady,
    Generation,
    CheckPending,
    Exited,
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
            Self::NotConfigured => "not_configured",
            Self::NotReady => "not_ready",
            Self::Generation => "generation_conflict",
            Self::CheckPending => "check_pending",
            Self::Exited => "owned_run_exited",
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
    ready: bool,
}
struct Service {
    binding: Option<ArtifactBinding>,
    artifact_file: Option<crate::readiness_tun::FileIdentity>,
    artifact_directory: Option<crate::readiness_tun::FileIdentity>,
    config_root: PathBuf,
    owner: Option<ProcessOwner>,
    run: Option<Run>,
    pending_check: Option<Candidate>,
    desired: bool,
    restored: bool,
    needs_recovery: bool,
    resource_suspended: bool,
    durability_uncertain: bool,
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
            let run_root = run.join(service.as_str());
            match fs::symlink_metadata(&run_root) {
                Ok(meta) if !meta.is_dir() || meta.file_type().is_symlink() => {
                    return Err(Failure::Process(ProcessError::UntrustedPath));
                }
                Ok(_) => (),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    fs::DirBuilder::new()
                        .mode(0o700)
                        .create(&run_root)
                        .map_err(|_| Failure::Store(StoreError::Storage))?;
                }
                Err(_) => return Err(Failure::Store(StoreError::Storage)),
            }
            fs::set_permissions(&run_root, fs::Permissions::from_mode(0o700))
                .map_err(|_| Failure::Store(StoreError::Storage))?;
            Some(
                ProcessOwner::new(
                    service.into(),
                    TrustedRoots::new(&binding.root, &config_root, run_root),
                    limits.process,
                )
                .map_err(Failure::Process)?,
            )
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
            owner,
            run: None,
            pending_check: None,
            desired: false,
            restored: false,
            needs_recovery: false,
            resource_suspended: true,
            durability_uncertain: false,
            failure: None,
        })
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
    fn ensure(&self, service: ServiceId) -> Result<(), Failure> {
        if self.closed {
            return Err(Failure::Closed);
        }
        if self.services[service.index()].pending_check.is_some() {
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
    /// The borrowed identity is only this manager's current owned handle.
    /// `status` observes exit; this accessor does not assert liveness/readiness.
    pub fn current_run(&self, service: ServiceId) -> Option<&OwnedRunIdentity> {
        self.services[service.index()]
            .run
            .as_ref()
            .map(|run| &run.identity)
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
            needs_recovery: slot.needs_recovery,
            resource_suspended: slot.resource_suspended,
            ready,
            running_matches_accepted: matches,
            active: ready
                && !slot.resource_suspended
                && !slot.needs_recovery
                && !slot.durability_uncertain,
            durability_uncertain: slot.durability_uncertain,
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
    pub fn abort_check(&mut self, service: ServiceId) -> Result<(), ManagerError> {
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
        Ok(())
    }
    fn hook(
        &mut self,
        service: ServiceId,
        stage: HookStage,
        record: &ConfigRecord,
    ) -> Result<(), Failure> {
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
            artifact_root: slot.binding.as_ref().map(|binding| binding.root.as_path()),
            artifact_path: slot.binding.as_ref().map(|binding| binding.path.as_path()),
            artifact_file: slot.artifact_file,
            artifact_directory: slot.artifact_directory,
            owned_status: slot
                .owner
                .as_ref()
                .map(|_| &observe as &dyn Fn() -> Result<process::Status, ProcessError>),
            deadline: Instant::now() + duration,
        };
        self.hooks.call(stage, &context)
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
        self.hook(service, HookStage::PreStart, record)?;
        let path = self.services[service.index()]
            .config_root
            .join(&record.file);
        let spec = self.spec(service, &path, bytes)?;
        let launch = self.services[service.index()]
            .owner
            .as_mut()
            .ok_or(Failure::ArtifactUnavailable)?
            .start(spec);
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
    fn start_accepted(&mut self, service: ServiceId) -> Result<(), Failure> {
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
        slot.needs_recovery = slot.durability_uncertain;
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
        // Off intent is latched before even an error or cleanup attempt.
        self.services[service.index()].desired = false;
        self.ensure(service)
            .map_err(|failure| self.error(service, failure, None))?;
        match self.withdraw_stop(service) {
            Ok(()) => {
                let slot = &mut self.services[service.index()];
                slot.resource_suspended = true;
                slot.needs_recovery = slot.durability_uncertain;
                slot.failure = None;
                self.status(service)
            }
            Err(failure) => {
                self.services[service.index()].failure = Some(failure);
                Err(self.error(service, failure, None))
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
        self.ensure(service)
            .map_err(|failure| self.error(service, failure, None))?;
        let observed = self
            .observe(service)
            .map_err(|failure| self.error(service, failure, None))?;
        if observed.is_some_and(|status| {
            status.phase == process::Phase::Exited && status.mode == Some(process::LaunchMode::Run)
        }) {
            self.withdraw_stop(service)
                .map_err(|failure| self.error(service, failure, None))?;
            let slot = &mut self.services[service.index()];
            slot.needs_recovery = true;
            slot.resource_suspended = true;
            slot.failure = Some(Failure::Exited);
        }
        self.status(service)
    }
    /// Attempts both services even if one fails. Any failed withdrawal retains
    /// that service's owner and child; the manager remains open for retry.
    pub fn close(&mut self) -> Result<(), ManagerError> {
        if self.closed {
            return Ok(());
        }
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
            if let Some(owner) = &mut self.services[service.index()].owner
                && let Err(error) = owner.close()
            {
                return Err(self.error(service, Failure::Process(error), None));
            }
        }
        self.closed = true;
        Ok(())
    }
}
fn config_identity(record: &ConfigRecord) -> ConfigIdentity {
    ConfigIdentity {
        generation: record.generation,
        sha256: record.sha256.clone(),
    }
}

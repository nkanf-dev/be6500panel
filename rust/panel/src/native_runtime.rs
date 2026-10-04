//! Fixed native readiness glue. No HTTP action, artifact fetch, capture executor,
//! process adoption or automatic activation is performed by this module.
use crate::readiness_dns::{self, ReadinessError};
use crate::readiness_tun::{self, NativeObserver, Observer, OwnedIdentity, OwnedStatus, TunError};
use crate::runtime_manager::{HookContext, HookError, Hooks, ServiceId};
use crate::runtime_process::{self, LaunchMode, Phase};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    fmt,
    fs::{self, OpenOptions},
    io::Read,
    os::unix::fs::OpenOptionsExt,
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
};

pub const MAX_ACCEPTED_BYTES: usize = 4 << 20;
/// One observer and one cancellation flag, reused by caller-invoked checks.
/// Constructing it performs no observation or command.
pub struct NativeReadiness<O = NativeObserver> {
    observer: O,
    cancel: Rc<AtomicBool>,
}
impl<O> fmt::Debug for NativeReadiness<O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("NativeReadiness([private])")
    }
}
impl NativeReadiness<NativeObserver> {
    pub fn native(cancel: Rc<AtomicBool>) -> Self {
        Self::with_observer(NativeObserver::new(), cancel)
    }
}
impl<O: Observer + 'static> NativeReadiness<O> {
    pub fn with_observer(observer: O, cancel: Rc<AtomicBool>) -> Self {
        Self { observer, cancel }
    }
    pub fn pre_start(&mut self, context: &HookContext<'_>) -> Result<(), HookError> {
        self.check(context)?;
        if context.service == ServiceId::Frpc {
            return Ok(());
        }
        let raw = read_accepted(context)?;
        readiness_tun::check_prestart_once(&raw, &mut self.observer, context.deadline, &self.cancel)
            .map_err(tun_error)
    }
    pub fn readiness(&mut self, context: &HookContext<'_>) -> Result<(), HookError> {
        self.check(context)?;
        let initial = owned_status(context)?;
        if context.service == ServiceId::Frpc {
            // Only retained process liveness. No external tunnel-connectivity claim.
            if owned_status(context)? != initial {
                return Err(HookError::Failed);
            }
            return self.check(context);
        }
        let raw = read_accepted(context)?;
        let mut status = || {
            owned_status(context).map_err(|error| match error {
                HookError::Deadline => TunError::Deadline,
                HookError::Cancelled => TunError::Cancelled,
                _ => TunError::IdentityChanged,
            })
        };
        if readiness_tun::native_target(&raw)
            .map_err(tun_error)?
            .is_some()
        {
            let root = context
                .artifact_root
                .ok_or(HookError::Failed)?
                .to_path_buf();
            let path = context
                .artifact_path
                .ok_or(HookError::Failed)?
                .to_path_buf();
            let file = context.artifact_file.ok_or(HookError::Failed)?;
            let directory = context.artifact_directory.ok_or(HookError::Failed)?;
            let identity = OwnedIdentity::bind(
                initial,
                root,
                path,
                file,
                directory,
                &mut self.observer,
                context.deadline,
                &self.cancel,
            )
            .map_err(tun_error)?;
            let mut listeners = |accepted: &[u8], budget: &readiness_tun::Budget<'_>| {
                readiness_dns::wait_readiness(accepted, budget.deadline, Some(budget.cancel))
                    .map_err(|error| match error {
                        ReadinessError::Deadline => TunError::Deadline,
                        ReadinessError::Canceled => TunError::Cancelled,
                        _ => TunError::Observation,
                    })
            };
            readiness_tun::check_startup_once(
                &raw,
                &identity,
                &mut self.observer,
                context.deadline,
                &self.cancel,
                &mut status,
                Some(&mut listeners),
            )
            .map_err(tun_error)?;
        } else {
            readiness_dns::wait_readiness(&raw, context.deadline, Some(&self.cancel))
                .map_err(dns_error)?;
        }
        if owned_status(context)? != initial {
            return Err(HookError::Failed);
        }
        self.check(context)
    }
    fn check(&self, context: &HookContext<'_>) -> Result<(), HookError> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(HookError::Cancelled);
        }
        if std::time::Instant::now() >= context.deadline {
            return Err(HookError::Deadline);
        }
        Ok(())
    }
    /// Readiness is real; cleanup/restore are mandatory caller-owned capture hooks.
    /// No default success is provided for network withdrawal or restoration.
    pub fn into_hooks(
        self,
        cleanup: impl FnMut(&HookContext<'_>) -> Result<(), HookError> + 'static,
        restore: impl FnMut(&HookContext<'_>) -> Result<(), HookError> + 'static,
    ) -> Hooks {
        let shared = Rc::new(RefCell::new(self));
        let pre = shared.clone();
        Hooks::new(
            move |context| {
                pre.try_borrow_mut()
                    .map_err(|_| HookError::Failed)?
                    .pre_start(context)
            },
            move |context| {
                shared
                    .try_borrow_mut()
                    .map_err(|_| HookError::Failed)?
                    .readiness(context)
            },
            cleanup,
            restore,
        )
    }
}
fn owned_status(context: &HookContext<'_>) -> Result<OwnedStatus, HookError> {
    let expected = context.run.ok_or(HookError::Failed)?;
    let actual = context.owned_status.ok_or(HookError::Failed)?().map_err(|_| HookError::Failed)?;
    if actual.service
        != match context.service {
            ServiceId::SingBox => runtime_process::ServiceId::SingBox,
            ServiceId::Frpc => runtime_process::ServiceId::Frpc,
        }
        || actual.phase != Phase::Running
        || actual.mode != Some(LaunchMode::Run)
        || actual.pid != Some(expected.pid())
        || expected.sha256() != context.config.sha256
    {
        return Err(HookError::Failed);
    }
    Ok(OwnedStatus {
        service: actual.service,
        pid: expected.pid(),
        generation: context.config.generation,
        running: true,
    })
}
fn read_accepted(context: &HookContext<'_>) -> Result<Vec<u8>, HookError> {
    use std::os::unix::fs::MetadataExt;
    if !context.config_path.is_absolute() {
        return Err(HookError::Failed);
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(context.config_path)
        .map_err(|_| HookError::Failed)?;
    let before = file.metadata().map_err(|_| HookError::Failed)?;
    if !before.is_file()
        || before.mode() & 0o7777 != 0o600
        || before.nlink() != 1
        || before.uid() != unsafe { libc::geteuid() }
        || before.len() == 0
        || before.len() > MAX_ACCEPTED_BYTES as u64
    {
        return Err(HookError::Failed);
    }
    let mut raw = Vec::new();
    raw.try_reserve_exact(before.len() as usize)
        .map_err(|_| HookError::Failed)?;
    let mut buffer = [0u8; 8192];
    loop {
        if std::time::Instant::now() >= context.deadline {
            return Err(HookError::Deadline);
        }
        let n = file.read(&mut buffer).map_err(|_| HookError::Failed)?;
        if n == 0 {
            break;
        }
        if n > MAX_ACCEPTED_BYTES.saturating_sub(raw.len()) {
            return Err(HookError::Failed);
        }
        raw.extend_from_slice(&buffer[..n]);
    }
    let after = file.metadata().map_err(|_| HookError::Failed)?;
    let disk = fs::symlink_metadata(context.config_path).map_err(|_| HookError::Failed)?;
    if before.dev() != after.dev()
        || before.ino() != after.ino()
        || before.len() != after.len()
        || before.mtime() != after.mtime()
        || before.mtime_nsec() != after.mtime_nsec()
        || before.ctime() != after.ctime()
        || before.ctime_nsec() != after.ctime_nsec()
        || disk.dev() != before.dev()
        || disk.ino() != before.ino()
        || raw.len() as u64 != before.len()
    {
        return Err(HookError::Failed);
    }
    if format!("{:x}", Sha256::digest(&raw)) != context.config.sha256 {
        return Err(HookError::Failed);
    }
    Ok(raw)
}
fn tun_error(error: TunError) -> HookError {
    match error {
        TunError::Deadline => HookError::Deadline,
        TunError::Cancelled => HookError::Cancelled,
        _ => HookError::Failed,
    }
}
fn dns_error(error: ReadinessError) -> HookError {
    match error {
        ReadinessError::Deadline => HookError::Deadline,
        ReadinessError::Canceled => HookError::Cancelled,
        _ => HookError::Failed,
    }
}

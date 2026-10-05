//! In-process capture cleanup/restore hooks. One controller and retained binary
//! bindings; no daemon, constructor command or journal-based auto-restoration.
use crate::capture_executor::{Binaries, Executor};
use crate::capture_kernel::{self, TableNames};
use crate::capture_plan::RulesPlanInput;
use crate::capture_state::{Controller, Desired, Phase, PreflightError};
use crate::native_runtime::{self, NativeReadiness};
use crate::readiness_tun::Observer;
use crate::runtime_manager::{HookContext, HookError, Hooks, OwnedRunIdentity, ServiceId};
use serde::Deserialize;
use std::{cell::RefCell, fmt, path::PathBuf, rc::Rc, time::Instant};
type FreshInput = Box<dyn FnMut(&Desired, &[u8], Instant) -> Result<RulesPlanInput, HookError>>;
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Selection {
    pub scope: String,
    pub devices: Vec<crate::capture_state::DeviceSelection>,
    pub client_ipv4: String,
    pub client_ipv6: String,
    pub ipv6: String,
}
impl fmt::Debug for Selection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CaptureSelection([private])")
    }
}
type SelectDesired = Box<dyn FnMut(&Selection, Instant) -> Result<Desired, HookError>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurrentState {
    Inactive,
    Suspended,
    Staged,
    CleanupPending,
    ScopeChanged,
    Unknown,
    Active,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurrentStatus {
    pub state: CurrentState,
    pub active: bool,
    pub intent: crate::capture_state::Status,
}
pub struct CaptureRuntime {
    controller: Controller,
    binaries: Binaries,
    run_dir: PathBuf,
    names: TableNames,
    fresh_input: FreshInput,
    executor: Option<Executor>,
    applied_run: Option<OwnedRunIdentity>,
    select_desired: Option<SelectDesired>,
}
impl fmt::Debug for CaptureRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CaptureRuntime([private])")
    }
}
impl CaptureRuntime {
    /// Caller supplies an internal fresh accepted-config/LAN observation builder.
    /// It cannot be omitted or replaced by the Input saved in a cleanup journal.
    pub fn new(
        controller: Controller,
        binaries: Binaries,
        run_dir: PathBuf,
        names: TableNames,
        fresh_input: impl FnMut(&Desired, &[u8], Instant) -> Result<RulesPlanInput, HookError> + 'static,
    ) -> Self {
        Self {
            controller,
            binaries,
            run_dir,
            names,
            fresh_input: Box::new(fresh_input),
            executor: None,
            applied_run: None,
            select_desired: None,
        }
    }
    /// Native current-source builder using the same bounded Observer seam as
    /// readiness. Does not observe, resolve, execute or start on construction.
    pub fn with_observer<O: Observer + 'static>(
        controller: Controller,
        binaries: Binaries,
        run_dir: PathBuf,
        names: TableNames,
        observer: O,
    ) -> Self {
        Self::with_observer_cancel(
            controller,
            binaries,
            run_dir,
            names,
            observer,
            Rc::new(std::sync::atomic::AtomicBool::new(false)),
        )
    }
    pub fn with_observer_cancel<O: Observer + 'static>(
        controller: Controller,
        binaries: Binaries,
        run_dir: PathBuf,
        names: TableNames,
        observer: O,
        cancel: Rc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        let build_cancel = cancel.clone();
        let shared = Rc::new(RefCell::new(observer));
        let build = shared.clone();
        let mut runtime = Self::new(
            controller,
            binaries,
            run_dir,
            names,
            move |desired, accepted, deadline| {
                let budget = crate::readiness_tun::Budget {
                    deadline,
                    cancel: &build_cancel,
                };
                crate::capture_input::observe_and_build(
                    &mut *build.try_borrow_mut().map_err(|_| HookError::Failed)?,
                    desired,
                    accepted,
                    &budget,
                )
                .map_err(|error| match error {
                    crate::capture_input::InputError::Deadline => HookError::Deadline,
                    crate::capture_input::InputError::Canceled => HookError::Cancelled,
                    _ => HookError::Failed,
                })
            },
        );
        runtime.select_desired = Some(Box::new(move |selection, deadline| {
            let budget = crate::readiness_tun::Budget {
                deadline,
                cancel: &cancel,
            };
            validate_selection(selection)?;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .map_err(|_| HookError::Failed)?
                .as_secs();
            let observed = crate::capture_lan::observe(
                &mut *shared.try_borrow_mut().map_err(|_| HookError::Failed)?,
                selection.scope != "gateway",
                now,
                &budget,
            )
            .map_err(|error| match error {
                crate::capture_lan::LanError::Deadline => HookError::Deadline,
                crate::capture_lan::LanError::Cancelled => HookError::Cancelled,
                _ => HookError::Failed,
            })?;
            selection_from_lan(selection, &observed)
        }));
        runtime
    }
    /// Trusted local selection observer adapter. It returns desired identity
    /// only, never command arrays, ports, core path or proof of installation.
    pub fn with_selection_observer(
        mut self,
        observer: impl FnMut(&Selection, Instant) -> Result<Desired, HookError> + 'static,
    ) -> Self {
        self.select_desired = Some(Box::new(observer));
        self
    }
    pub fn desired(&self) -> Desired {
        self.controller.desired()
    }
    pub fn status(&self) -> crate::capture_state::Status {
        self.controller.status()
    }
    fn admit_executor(&mut self, input: &RulesPlanInput) -> Result<(), HookError> {
        if let Some(executor) = &mut self.executor {
            // Finish the retained exact one-shot child before replacing its
            // plan admission. No next command starts while it is unresolved.
            executor.retry_abort().map_err(|_| HookError::Failed)?;
        }
        self.executor = Some(
            Executor::admit(
                input,
                self.binaries
                    .retained_copy()
                    .map_err(|_| HookError::Failed)?,
                &self.run_dir,
            )
            .map_err(|_| HookError::Failed)?,
        );
        Ok(())
    }
    fn release_finished_executor(&mut self) {
        if self
            .executor
            .as_ref()
            .is_some_and(|executor| !executor.has_pending_child())
        {
            self.executor.take();
        }
    }
    /// Explicit startup withdrawal, independent of any current core or desired
    /// Apply intent. Constructor/status never execute this. Only exact cleanup
    /// regenerated from the validated journal is admitted.
    pub fn startup_withdraw(&mut self, deadline: Instant) -> Result<(), HookError> {
        self.cleanup_until(deadline.min(Instant::now() + std::time::Duration::from_secs(30)))
    }
    fn cleanup_until(&mut self, deadline: Instant) -> Result<(), HookError> {
        self.applied_run = None;
        let Some(input) = self.controller.cleanup_input() else {
            return Ok(());
        };
        self.admit_executor(&input)?;
        let executor = self.executor.as_mut().ok_or(HookError::Failed)?;
        let result = self.controller.cleanup_until(
            |argv, budget| executor.execute(argv, budget, None),
            deadline,
        );
        self.release_finished_executor();
        result.map_err(|error| match error {
            crate::capture_state::Error::Deadline => HookError::Deadline,
            _ => HookError::Failed,
        })
    }
    pub fn cleanup(&mut self, context: &HookContext<'_>) -> Result<(), HookError> {
        if context.service != ServiceId::SingBox {
            return Ok(());
        }
        self.cleanup_until(context.deadline)
    }
    pub fn restore(&mut self, context: &HookContext<'_>) -> Result<(), HookError> {
        if context.service != ServiceId::SingBox {
            return Ok(());
        }
        let desired = self.controller.desired();
        if !desired.desired {
            return Ok(());
        }
        if context.run.is_none() || self.controller.status().phase != Phase::Off {
            return Err(HookError::Failed);
        }
        let raw = native_runtime::read_accepted(context)?;
        let input = (self.fresh_input)(&desired, &raw, context.deadline)?;
        validate_accepted_input(&raw, &input)?;
        drop(raw);
        self.apply_input(context, input)
    }
    fn apply_input(
        &mut self,
        context: &HookContext<'_>,
        input: RulesPlanInput,
    ) -> Result<(), HookError> {
        self.admit_executor(&input)?;
        let checked_input = input.clone();
        let result = {
            let executor = RefCell::new(self.executor.as_mut().ok_or(HookError::Failed)?);
            self.controller.apply_until(
                input,
                |fresh, _plan, deadline| {
                    capture_kernel::preflight(
                        fresh,
                        &self.names,
                        |argv, budget| {
                            executor
                                .borrow_mut()
                                .execute(argv, budget.min(context.deadline), None)
                        },
                        deadline.min(context.deadline),
                    )
                    .map_err(|_| PreflightError::Refused)
                },
                // Controller supplies an independent cleanup deadline on failure.
                |argv, deadline| executor.borrow_mut().execute(argv, deadline, None),
                context.deadline,
            )
        };
        if result.is_err() {
            self.release_finished_executor();
            return Err(HookError::Failed);
        }
        let executor = self.executor.as_mut().ok_or(HookError::Failed)?;
        let observed = capture_kernel::observe_installed(
            &checked_input,
            &self.names,
            |argv, budget| executor.execute(argv, budget, None),
            context.deadline,
        );
        if observed.is_err() {
            // A failed read is not successful activation. Withdrawal receives
            // the controller's independent bounded cleanup budget.
            let _ = self
                .controller
                .cleanup(|argv, deadline| executor.execute(argv, deadline, None));
        }
        if observed.is_ok() {
            self.applied_run = context.run.cloned();
        } else {
            self.applied_run = None;
        }
        self.release_finished_executor();
        observed.map_err(|_| HookError::Failed)
    }
    /// Explicit selection, not a current-read repair. Fresh LAN/config
    /// preparation must succeed before stored intent or old resources change.
    pub fn select(
        &mut self,
        context: &HookContext<'_>,
        selection: &Selection,
        mut native: impl FnMut(&HookContext<'_>) -> Result<(), HookError>,
    ) -> Result<CurrentStatus, HookError> {
        validate_selection(selection)?;
        if context.service != ServiceId::SingBox
            || context.run.is_none()
            || Instant::now() >= context.deadline
        {
            return Err(HookError::Failed);
        }
        native(context)?;
        let desired =
            (self.select_desired.as_mut().ok_or(HookError::Failed)?)(selection, context.deadline)?;
        let desired =
            crate::capture_state::normalize_desired(desired).map_err(|_| HookError::Failed)?;
        if !desired.desired {
            return Err(HookError::Failed);
        }
        // Trusted selection adapters cannot turn device request into gateway.
        if (selection.scope == "gateway") != (desired.scope == "gateway") {
            return Err(HookError::Failed);
        }
        if !selection.devices.is_empty() {
            let requested = crate::capture_state::normalize_desired(Desired {
                scope: "devices".into(),
                devices: selection.devices.clone(),
                desired: true,
                ..Desired::default()
            })
            .map_err(|_| HookError::Failed)?;
            if requested.devices != desired.devices {
                return Err(HookError::Failed);
            }
        }
        let raw = native_runtime::read_accepted(context)?;
        let input = (self.fresh_input)(&desired, &raw, context.deadline)?;
        validate_accepted_input(&raw, &input)?;
        let plan = crate::capture_plan::plan_owned_rules(&input).map_err(|_| HookError::Failed)?;
        crate::capture_state::desired_matches(&desired, &plan.ownership)
            .map_err(|_| HookError::Failed)?;
        drop(plan);
        drop(raw);
        drop(input);
        if Instant::now() >= context.deadline {
            return Err(HookError::Deadline);
        }
        self.controller
            .set_desired(desired)
            .map_err(|_| HookError::Failed)?;
        // Fresh scope/config preparation and durability may consume time or
        // observe a lost retained core. Recheck before withdrawing old rules.
        native(context)?;
        self.cleanup_until(context.deadline)?;
        if Instant::now() >= context.deadline {
            return Err(HookError::Deadline);
        }
        let raw = native_runtime::read_accepted(context)?;
        let desired = self.controller.desired();
        let input = (self.fresh_input)(&desired, &raw, context.deadline)?;
        validate_accepted_input(&raw, &input)?;
        drop(raw);
        // Old withdrawal and this second fresh builder cannot authorize new
        // interception if the actual retained identity or DNS proof was lost.
        native(context)?;
        self.apply_input(context, input)?;
        let current = self.observe_current(context, &mut native);
        if current.active {
            Ok(current)
        } else {
            // Query uncertainty after explicit Apply cannot become success.
            let _ = self.cleanup_until(Instant::now() + std::time::Duration::from_secs(30));
            Err(HookError::Failed)
        }
    }
    /// Effective off intent first, including binary/executor admission failure.
    /// Persistence and best-effort cleanup remain attempted with independent
    /// fixed deadline and all unresolved owned commands retained.
    pub fn disable(&mut self, deadline: Instant) -> Result<(), crate::capture_state::Error> {
        self.applied_run = None;
        let input = self.controller.cleanup_input();
        self.controller.latch_off();
        let admitted = if let Some(input) = input {
            self.admit_executor(&input).is_ok()
        } else {
            true
        };
        let executor = &mut self.executor;
        let result = self.controller.disable_until(
            |argv, budget| {
                if !admitted {
                    return Err(crate::capture_state::CommandError::Failure);
                }
                executor
                    .as_mut()
                    .ok_or(crate::capture_state::CommandError::Failure)?
                    .execute(argv, budget, None)
            },
            deadline.min(Instant::now() + std::time::Duration::from_secs(30)),
        );
        self.release_finished_executor();
        result
    }
    pub fn snapshot(&self) -> crate::capture_state::Snapshot {
        self.controller.snapshot()
    }
    pub fn owned_command_count(&self) -> usize {
        self.controller.owned_command_count()
    }
    fn current(&self, state: CurrentState) -> CurrentStatus {
        CurrentStatus {
            state,
            active: state == CurrentState::Active,
            intent: self.controller.status(),
        }
    }
    /// Pure phase projection. Presence of saved ownership is never current
    /// proof. GET may choose observation, but never withdrawal/Apply/reaping.
    pub fn current_unobserved(&self) -> CurrentStatus {
        let status = self.controller.status();
        let state = match status.phase {
            Phase::Off => {
                if status.desired {
                    CurrentState::Suspended
                } else {
                    CurrentState::Inactive
                }
            }
            Phase::Staged => CurrentState::Staged,
            Phase::CleanupPending => CurrentState::CleanupPending,
            Phase::ActiveByApply => CurrentState::Unknown,
        };
        self.current(state)
    }
    /// Query-only current proof. No stored Apply replay, automatic repair,
    /// journal writes or core action. Each finite query child is reaped; an
    /// exceptional pending query is retained and never retried by observation.
    pub fn observe_current(
        &mut self,
        context: &HookContext<'_>,
        mut native: impl FnMut(&HookContext<'_>) -> Result<(), HookError>,
    ) -> CurrentStatus {
        let intent = self.controller.status();
        if intent.phase != Phase::ActiveByApply {
            return self.current_unobserved();
        }
        if !intent.desired
            || intent.cleanup_pending
            || intent.storage_uncertain
            || intent.disable_not_persisted
        {
            return self.current(CurrentState::CleanupPending);
        }
        if context.service != ServiceId::SingBox
            || context.run.is_none()
            || self.applied_run.as_ref() != context.run
            || Instant::now() >= context.deadline
        {
            return self.current(CurrentState::Unknown);
        }
        if self
            .executor
            .as_ref()
            .is_some_and(|executor| executor.has_pending_child())
        {
            return self.current(CurrentState::Unknown);
        }
        if native(context).is_err() {
            return self.current(CurrentState::Unknown);
        }
        let raw = match native_runtime::read_accepted(context) {
            Ok(raw) => raw,
            Err(_) => return self.current(CurrentState::Unknown),
        };
        let snapshot = self.controller.snapshot();
        let input = match snapshot.input {
            Some(input) => input,
            None => return self.current(CurrentState::Unknown),
        };
        let fresh = match (self.fresh_input)(&snapshot.desired, &raw, context.deadline) {
            Ok(fresh) => fresh,
            Err(_) => return self.current(CurrentState::Unknown),
        };
        if validate_accepted_input(&raw, &fresh).is_err() {
            return self.current(CurrentState::Unknown);
        }
        // The selected input is owned. Do not keep another legacy4MiB config
        // buffer live over kernel queries or the subsequent native reread.
        drop(raw);
        let same = match (
            crate::capture_plan::plan_owned_rules(&input),
            crate::capture_plan::plan_owned_rules(&fresh),
        ) {
            (Ok(installed), Ok(current)) => {
                installed.apply == current.apply && installed.ownership == current.ownership
            }
            _ => return self.current(CurrentState::Unknown),
        };
        if !same {
            return self.current(CurrentState::ScopeChanged);
        }
        let binaries = match self.binaries.retained_copy() {
            Ok(binaries) => binaries,
            Err(_) => return self.current(CurrentState::Unknown),
        };
        self.executor = match Executor::admit_readonly(&input, binaries, &self.run_dir) {
            Ok(executor) => Some(executor),
            Err(_) => return self.current(CurrentState::Unknown),
        };
        let executor = self.executor.as_mut().expect("admitted query executor");
        let installed = capture_kernel::observe_installed(
            &input,
            &self.names,
            |argv, budget| executor.execute(argv, budget, None),
            context.deadline,
        );
        self.release_finished_executor();
        if installed.is_err() || native(context).is_err() {
            return self.current(CurrentState::Unknown);
        }
        if native_runtime::read_accepted(context).is_err() || Instant::now() >= context.deadline {
            return self.current(CurrentState::Unknown);
        }
        self.current(CurrentState::Active)
    }
    pub fn into_hooks<O: Observer + 'static>(self, readiness: NativeReadiness<O>) -> Hooks {
        self.into_hooks_with_handle(readiness).0
    }
    pub fn into_hooks_with_handle<O: Observer + 'static>(
        self,
        readiness: NativeReadiness<O>,
    ) -> (Hooks, CaptureHandle<O>) {
        let capture = Rc::new(RefCell::new(self));
        let native = Rc::new(RefCell::new(readiness));
        let pre = native.clone();
        let ready = native.clone();
        let cleanup = capture.clone();
        let restore = capture.clone();
        let hooks = Hooks::new(
            move |context| {
                pre.try_borrow_mut()
                    .map_err(|_| HookError::Failed)?
                    .pre_start(context)
            },
            move |context| {
                ready
                    .try_borrow_mut()
                    .map_err(|_| HookError::Failed)?
                    .readiness(context)
            },
            move |context| {
                cleanup
                    .try_borrow_mut()
                    .map_err(|_| HookError::Failed)?
                    .cleanup(context)
            },
            move |context| {
                restore
                    .try_borrow_mut()
                    .map_err(|_| HookError::Failed)?
                    .restore(context)
            },
        );
        (hooks, CaptureHandle { capture, native })
    }
}
/// Shared in-process access to the same capture/readiness hooks. Cloning this
/// handle clones only Rc pointers, never a Manager/observer/executor thread.
pub struct CaptureHandle<O> {
    capture: Rc<RefCell<CaptureRuntime>>,
    native: Rc<RefCell<NativeReadiness<O>>>,
}
impl<O> Clone for CaptureHandle<O> {
    fn clone(&self) -> Self {
        Self {
            capture: self.capture.clone(),
            native: self.native.clone(),
        }
    }
}
impl<O> fmt::Debug for CaptureHandle<O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CaptureHandle([owned])")
    }
}
impl<O: Observer + 'static> CaptureHandle<O> {
    pub fn owned_command_count(&self) -> Result<usize, HookError> {
        Ok(self
            .capture
            .try_borrow()
            .map_err(|_| HookError::Failed)?
            .owned_command_count())
    }
    pub fn snapshot(&self) -> Result<crate::capture_state::Snapshot, HookError> {
        Ok(self
            .capture
            .try_borrow()
            .map_err(|_| HookError::Failed)?
            .snapshot())
    }
    pub fn disable(&self, deadline: Instant) -> Result<(), crate::capture_state::Error> {
        self.capture
            .try_borrow_mut()
            .map_err(|_| crate::capture_state::Error::AlreadyOwned)?
            .disable(deadline)
    }
    pub fn select(
        &self,
        context: &HookContext<'_>,
        selection: &Selection,
    ) -> Result<CurrentStatus, HookError> {
        self.capture
            .try_borrow_mut()
            .map_err(|_| HookError::Failed)?
            .select(context, selection, |context| {
                self.native
                    .try_borrow_mut()
                    .map_err(|_| HookError::Failed)?
                    .observe_current(context)
            })
    }
    pub fn startup_withdraw(&self, deadline: Instant) -> Result<(), HookError> {
        self.capture
            .try_borrow_mut()
            .map_err(|_| HookError::Failed)?
            .startup_withdraw(deadline)
    }
    pub fn current_unobserved(&self) -> Result<CurrentStatus, HookError> {
        Ok(self
            .capture
            .try_borrow()
            .map_err(|_| HookError::Failed)?
            .current_unobserved())
    }
    pub fn observe_current(&self, context: &HookContext<'_>) -> Result<CurrentStatus, HookError> {
        let mut capture = self
            .capture
            .try_borrow_mut()
            .map_err(|_| HookError::Failed)?;
        Ok(capture.observe_current(context, |context| {
            self.native
                .try_borrow_mut()
                .map_err(|_| HookError::Failed)?
                .observe_current(context)
        }))
    }
}
fn validate_selection(selection: &Selection) -> Result<(), HookError> {
    if selection.ipv6 != "direct"
        || !selection.client_ipv6.is_empty()
        || selection.devices.len() > 64
    {
        return Err(HookError::Failed);
    }
    match selection.scope.as_str() {
        "gateway" if selection.devices.is_empty() && selection.client_ipv4.is_empty() => Ok(()),
        "" | "devices" if selection.client_ipv4.is_empty() != selection.devices.is_empty() => {
            Ok(())
        }
        _ => Err(HookError::Failed),
    }
}
fn selection_from_lan(
    selection: &Selection,
    observed: &crate::capture_lan::Snapshot,
) -> Result<Desired, HookError> {
    validate_selection(selection)?;
    if selection.scope == "gateway" {
        return crate::capture_state::normalize_desired(Desired {
            scope: "gateway".into(),
            lan_ipv4_prefixes: observed.lan_ipv4_prefixes.clone(),
            desired: true,
            ..Desired::default()
        })
        .map_err(|_| HookError::Failed);
    }
    let devices = if !selection.client_ipv4.is_empty() {
        let address = selection
            .client_ipv4
            .parse::<std::net::Ipv4Addr>()
            .map_err(|_| HookError::Failed)?;
        let text = address.to_string();
        let mut found = observed.devices.iter().filter(|device| device.ip == text);
        let device = found.next().ok_or(HookError::Failed)?;
        if found.next().is_some() {
            return Err(HookError::Failed);
        }
        vec![crate::capture_state::DeviceSelection {
            mac: device.mac.clone(),
        }]
    } else {
        selection.devices.clone()
    };
    let desired = crate::capture_state::normalize_desired(Desired {
        scope: "devices".into(),
        devices,
        desired: true,
        ..Desired::default()
    })
    .map_err(|_| HookError::Failed)?;
    for selected in &desired.devices {
        let mut found = observed
            .devices
            .iter()
            .filter(|device| device.mac == selected.mac);
        if found.next().is_none() || found.next().is_some() {
            return Err(HookError::Failed);
        }
    }
    Ok(desired)
}
#[derive(Deserialize)]
struct Listeners {
    inbounds: Vec<Inbound>,
}
#[derive(Default, Deserialize)]
#[serde(default)]
struct Inbound {
    #[serde(rename = "type")]
    kind: String,
    tag: String,
    listen: String,
    listen_port: u16,
    interface_name: String,
    address: Option<Vec<String>>,
}
fn validate_accepted_input(raw: &[u8], input: &RulesPlanInput) -> Result<(), HookError> {
    let target = crate::readiness_tun::native_target(raw)
        .map_err(|_| HookError::Failed)?
        .ok_or(HookError::Failed)?;
    let config: Listeners = serde_json::from_slice(raw).map_err(|_| HookError::Failed)?;
    if config.inbounds.len() > 64
        || input.datapath != "routed-tun"
        || input.tun_interface != target.interface_name()
        || input.tun_address != target.address().to_string()
        || input.ipv6 != "direct"
        || input.failure != "direct"
    {
        return Err(HookError::Failed);
    }
    let mut mixed = None;
    let mut dns = None;
    for inbound in &config.inbounds {
        match inbound.tag.as_str() {
            "mixed-in" if inbound.kind == "mixed" => {
                if mixed.replace(inbound.listen_port).is_some() {
                    return Err(HookError::Failed);
                }
            }
            "dns-in" if inbound.kind == "direct" => {
                if dns.replace(inbound.listen_port).is_some() {
                    return Err(HookError::Failed);
                }
                let listen = inbound
                    .listen
                    .parse::<std::net::IpAddr>()
                    .map_err(|_| HookError::Failed)?;
                if listen.is_loopback()
                    || !listen.is_ipv4()
                    || (!listen.is_unspecified()
                        && !input.management_ips.contains(&listen.to_string()))
                {
                    return Err(HookError::Failed);
                }
            }
            "tun-in" => {
                if inbound.interface_name != input.tun_interface
                    || inbound
                        .address
                        .as_ref()
                        .is_none_or(|a| a != std::slice::from_ref(&input.tun_address))
                {
                    return Err(HookError::Failed);
                }
            }
            _ => {}
        }
    }
    if mixed != Some(input.ports.mixed)
        || dns != Some(input.ports.dns)
        || input.ports.mixed == 0
        || input.ports.dns == 0
    {
        return Err(HookError::Failed);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fresh_input_must_match_actual_native_lane_and_listener_ports() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/native-go.json")).unwrap();
        let raw = fixture["cases"][0]["config"].as_str().unwrap();
        let mut input = RulesPlanInput {
            datapath: "routed-tun".into(),
            tun_interface: "b6p-tun".into(),
            tun_address: "172.31.255.253/30".into(),
            ipv6: "direct".into(),
            failure: "direct".into(),
            ports: crate::native::Ports {
                mixed: 2080,
                tproxy: 7893,
                dns: 1053,
            },
            management_ips: vec!["192.168.31.1".into()],
            ..RulesPlanInput::default()
        };
        // Default native diagnostic DNS is loopback and cannot be LAN capture.
        assert!(validate_accepted_input(raw.as_bytes(), &input).is_err());
        let mut doc: serde_json::Value = serde_json::from_str(raw).unwrap();
        doc["inbounds"][2]["listen"] = "192.168.31.1".into();
        let raw = serde_json::to_vec(&doc).unwrap();
        assert!(validate_accepted_input(&raw, &input).is_ok());
        input.ports.dns = 6450;
        assert!(validate_accepted_input(&raw, &input).is_err());
        input.ports.dns = 1053;
        input.tun_interface = "b6p-wrong".into();
        assert!(validate_accepted_input(&raw, &input).is_err());
    }

    fn observed_lan() -> crate::capture_lan::Snapshot {
        crate::capture_lan::Snapshot {
            lan_ipv4_prefixes: vec!["192.168.50.0/24".into()],
            lan_addresses: vec!["192.168.50.1".into()],
            management_ips: vec!["192.168.50.1".into()],
            interface_addresses: vec![],
            devices: vec![crate::capture_lan::Device {
                mac: "02:aa:bb:cc:dd:ee".into(),
                ip: "192.168.50.2".into(),
            }],
        }
    }
    #[test]
    fn selection_uses_fresh_gateway_or_unique_current_mac_not_supplied_network() {
        let observed = observed_lan();
        let gateway = selection_from_lan(
            &Selection {
                scope: "gateway".into(),
                ipv6: "direct".into(),
                ..Selection::default()
            },
            &observed,
        )
        .unwrap();
        assert_eq!(gateway.lan_ipv4_prefixes, observed.lan_ipv4_prefixes);
        assert!(gateway.desired && gateway.devices.is_empty());
        let literal = Selection {
            client_ipv4: "192.168.50.2".into(),
            ipv6: "direct".into(),
            ..Selection::default()
        };
        let device = selection_from_lan(&literal, &observed).unwrap();
        assert_eq!(
            device.devices,
            vec![crate::capture_state::DeviceSelection {
                mac: "02:aa:bb:cc:dd:ee".into()
            }]
        );
        assert!(device.client_ipv4.is_empty());
        let stable = Selection {
            devices: device.devices.clone(),
            ipv6: "direct".into(),
            ..Selection::default()
        };
        assert_eq!(selection_from_lan(&stable, &observed).unwrap(), device);
        let mut stale = observed_lan();
        stale.devices.clear();
        assert!(selection_from_lan(&literal, &stale).is_err());
        assert!(selection_from_lan(&stable, &stale).is_err());
        let mut conflicted = observed_lan();
        conflicted.devices.push(crate::capture_lan::Device {
            mac: "02:aa:bb:cc:dd:ff".into(),
            ip: "192.168.50.2".into(),
        });
        assert!(selection_from_lan(&literal, &conflicted).is_err());
        let duplicate = Selection {
            devices: vec![device.devices[0].clone(), device.devices[0].clone()],
            ipv6: "direct".into(),
            ..Selection::default()
        };
        assert!(selection_from_lan(&duplicate, &observed).is_err());
        assert!(
            selection_from_lan(
                &Selection {
                    scope: "gateway".into(),
                    client_ipv4: "192.168.50.2".into(),
                    ipv6: "direct".into(),
                    ..Selection::default()
                },
                &observed
            )
            .is_err()
        );
    }
}

//! In-process capture cleanup/restore hooks. One controller and retained binary
//! bindings; no daemon, constructor command or journal-based auto-restoration.
use crate::capture_executor::{Binaries, Executor};
use crate::capture_kernel::{self, TableNames};
use crate::capture_plan::RulesPlanInput;
use crate::capture_state::{Controller, Desired, Phase, PreflightError};
use crate::native_runtime::{self, NativeReadiness};
use crate::readiness_tun::Observer;
use crate::runtime_manager::{HookContext, HookError, Hooks, ServiceId};
use serde::Deserialize;
use std::{cell::RefCell, fmt, path::PathBuf, rc::Rc, time::Instant};
type FreshInput = Box<dyn FnMut(&Desired, &[u8], Instant) -> Result<RulesPlanInput, HookError>>;
pub struct CaptureRuntime {
    controller: Controller,
    binaries: Binaries,
    run_dir: PathBuf,
    names: TableNames,
    fresh_input: FreshInput,
    executor: Option<Executor>,
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
        }
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
    pub fn cleanup(&mut self, context: &HookContext<'_>) -> Result<(), HookError> {
        if context.service != ServiceId::SingBox {
            return Ok(());
        }
        let Some(input) = self.controller.cleanup_input() else {
            return Ok(());
        };
        self.admit_executor(&input)?;
        let executor = self.executor.as_mut().ok_or(HookError::Failed)?;
        let result = self
            .controller
            .cleanup(|argv, deadline| executor.execute(argv, deadline.min(context.deadline), None));
        self.release_finished_executor();
        result.map_err(|_| HookError::Failed)
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
        self.admit_executor(&input)?;
        let checked_input = input.clone();
        let result = {
            let executor = RefCell::new(self.executor.as_mut().ok_or(HookError::Failed)?);
            self.controller.apply(
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
        self.release_finished_executor();
        observed.map_err(|_| HookError::Failed)
    }
    pub fn into_hooks<O: Observer + 'static>(self, readiness: NativeReadiness<O>) -> Hooks {
        let shared = Rc::new(RefCell::new(self));
        let cleanup = shared.clone();
        readiness.into_hooks(
            move |context| {
                cleanup
                    .try_borrow_mut()
                    .map_err(|_| HookError::Failed)?
                    .cleanup(context)
            },
            move |context| {
                shared
                    .try_borrow_mut()
                    .map_err(|_| HookError::Failed)?
                    .restore(context)
            },
        )
    }
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
}

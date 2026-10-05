//! Complete product API assembly on the existing authenticated owner lane.
use crate::{
    http::Method,
    product_configuration::Configuration,
    product_diagnostics::Diagnostics,
    product_io::{Backend, Native},
    product_maintenance::Maintenance,
    product_observations::Observations,
    product_plans::Plans,
    product_support::Logs,
    product_telemetry::Telemetry,
    readiness_tun::Budget,
    runtime_http::RuntimeHttp,
    runtime_manager::ServiceId,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    io::{self, Write},
    net::IpAddr,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApiError {
    pub status: u16,
    pub code: &'static str,
    pub message: &'static str,
}
fn invalid() -> ApiError {
    ApiError {
        status: 400,
        code: "invalid_json",
        message: "Request fields do not match the product contract.",
    }
}
fn unavailable(code: &'static str) -> ApiError {
    ApiError {
        status: 503,
        code,
        message: "Native source is unavailable.",
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Backup {
    scopes: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ImportStage {
    preview_id: String,
    generation: u64,
    modules: Vec<String>,
    #[serde(default)]
    acknowledge_model_mismatch: bool,
}
pub struct Product<B = Native> {
    data: PathBuf,
    diagnostic_root: PathBuf,
    io: B,
    observations: Observations,
    configuration: Result<Configuration, ApiError>,
    telemetry: Result<Telemetry, ApiError>,
    maintenance: Result<Maintenance, ApiError>,
    diagnostics: Diagnostics,
    diagnostic_candidate: Option<(PathBuf, [u8; 32])>,
    diagnostic_revision: String,
    diagnostic_source_identity: Option<[u8; 32]>,
    diagnostic_accepted: Option<(u64, u32)>,
    plans: Plans,
    logs: Logs,
}
impl Product<Native> {
    pub fn open(path: &Path) -> Self {
        Self::with_backend(path, Native::new())
    }
    pub fn open_with_run(path: &Path, run: &Path) -> Self {
        let mut value = Self::open(path);
        value.diagnostic_root = run.join("diagnostics");
        value
    }
}
impl<B: Backend> Product<B> {
    pub fn with_backend(path: &Path, io: B) -> Self {
        Self::with_backend_native_dir(path, io, Path::new("/etc/config"))
    }
    pub fn with_backend_native_dir(path: &Path, io: B, native: &Path) -> Self {
        Self {
            data: path.into(),
            diagnostic_root: std::env::temp_dir()
                .join(format!("be6500-native-diagnostics-{}", std::process::id())),
            io,
            observations: Observations::new().with_cache(),
            configuration: Configuration::open_with_native_dir(&path.join("configuration"), native)
                .map_err(|e| ApiError {
                    status: e.status,
                    code: e.code,
                    message: e.message,
                }),
            telemetry: Telemetry::open(path).map_err(|e| ApiError {
                status: e.status,
                code: e.code,
                message: e.message,
            }),
            maintenance: Maintenance::open(path).map_err(|e| ApiError {
                status: e.status,
                code: e.code,
                message: e.message,
            }),
            diagnostics: Diagnostics::new(),
            diagnostic_candidate: None,
            diagnostic_revision: String::new(),
            diagnostic_source_identity: None,
            diagnostic_accepted: None,
            plans: Plans::new(),
            logs: Logs::new(),
        }
    }
    pub fn close(&mut self) -> bool {
        self.clear_previews();
        let diagnostics_done = self.diagnostics.close(&mut self.io).unwrap_or(false);
        if let Ok(telemetry) = &mut self.telemetry
            && let Err(error) = telemetry.close()
        {
            self.logs.push(
                self.io.now_unix(),
                "WARN",
                error.code,
                "traffic",
                "Traffic final durability needs retry.",
            );
        }
        diagnostics_done
    }
    pub fn replace_configuration(&mut self, configuration: Configuration) {
        self.configuration = Ok(configuration);
    }
    pub fn clear_previews(&self) {
        if let Ok(maintenance) = &self.maintenance {
            maintenance.clear();
        }
    }
    pub fn system(&mut self) -> Result<Value, ApiError> {
        let cancel = AtomicBool::new(false);
        let budget = Budget {
            deadline: Instant::now() + Duration::from_secs(2),
            cancel: &cancel,
        };
        self.observations
            .get("/api/system", "", None, &mut self.io, &budget)
            .map_err(|e| ApiError {
                status: e.status,
                code: e.code,
                message: e.message,
            })
    }

    pub fn diagnostic_bindings(
        &mut self,
        runtime: &mut RuntimeHttp,
        revision: &str,
        source_identity: Option<[u8; 32]>,
        nodes: &[crate::native::Node],
        budget: &Budget<'_>,
    ) {
        if let Some(source) = runtime.product_source() {
            self.diagnostics.bind_source(source);
        }
        if self.diagnostic_revision != revision
            || self.diagnostic_source_identity != source_identity
        {
            match self.diagnostics.bind_nodes(revision, nodes) {
                Ok(()) => {
                    self.diagnostic_revision = revision.into();
                    self.diagnostic_source_identity = source_identity;
                }
                Err(_) => self.diagnostics.revoke_nodes(),
            }
        }
        let candidate = runtime.product_artifact();
        if self.diagnostic_candidate != candidate {
            self.diagnostics.revoke_candidate();
            self.diagnostic_candidate = None;
            if let Some((path, hash)) = candidate {
                let temporary = self.diagnostic_root.clone();
                let opened = std::fs::DirBuilder::new();
                use std::os::unix::fs::DirBuilderExt;
                let mut opened = opened;
                opened.mode(0o700);
                if (temporary.exists() || opened.create(&temporary).is_ok())
                    && self
                        .diagnostics
                        .bind_candidate(&path, hash, &temporary, budget)
                        .is_ok()
                {
                    self.diagnostic_candidate = Some((path, hash));
                }
            }
        }
        let current = runtime
            .product_status(ServiceId::SingBox)
            .ok()
            .filter(|status| status.ready && status.running_matches_accepted)
            .and_then(|status| status.pid.map(|pid| (status.generation, pid)));
        if self.diagnostic_accepted != current {
            self.diagnostics.revoke_accepted();
            self.diagnostic_accepted = None;
            if let (Some((_, pid)), Some(config), Some(run), Some((path, _))) = (
                current,
                runtime.product_config(ServiceId::SingBox).ok().flatten(),
                runtime.product_run(),
                runtime.product_artifact(),
            ) {
                let _ = pid;
                if let Ok(scope) = runtime.selection_scope(budget) {
                    let lan = scope
                        .lan_addresses
                        .iter()
                        .filter_map(|address| address.parse().ok())
                        .collect::<Vec<_>>();
                    if self
                        .diagnostics
                        .bind_accepted(config.bytes(), run, &path, &lan, &mut self.io, budget)
                        .is_ok()
                    {
                        self.diagnostic_accepted = current;
                    }
                }
            }
        }
    }
    pub fn now(&self) -> u64 {
        self.io.now_unix()
    }
    pub fn tick(&mut self, runtime: Option<&mut RuntimeHttp>, nodes: &[crate::native::Node]) {
        let cancel = AtomicBool::new(false);
        let budget = Budget {
            deadline: Instant::now() + Duration::from_secs(1),
            cancel: &cancel,
        };
        let mut runtime = runtime;
        if let Ok(configuration) = &mut self.configuration {
            let mut before = |_: &mut B, network: bool| {
                if network && let Some(owner) = runtime.as_deref() {
                    owner
                        .product_withdraw(Instant::now() + Duration::from_secs(30))
                        .map_err(|_| crate::product_configuration::ApiError {
                            status: 409,
                            code: "capture_cleanup_failed",
                            message: "Capture withdrawal needs retry before native changes.",
                        })?;
                }
                Ok(())
            };
            if let Err(e) =
                configuration.tick_with_before_mutation(&mut self.io, &budget, &mut before)
            {
                self.logs.push(
                    self.io.now_unix(),
                    "WARN",
                    e.code,
                    "configuration",
                    "Native configuration recovery needs retry.",
                );
            }
        }
        let owner_epoch = runtime
            .as_deref_mut()
            .and_then(|owner| owner.product_status(ServiceId::SingBox).ok())
            .filter(|status| status.ready && status.running_matches_accepted)
            .and_then(|status| status.pid.map(|pid| (status.generation, pid)));
        let native = runtime
            .as_deref()
            .and_then(|owner| owner.product_config(ServiceId::SingBox).ok().flatten());
        if let Ok(telemetry) = &mut self.telemetry {
            telemetry.set_owner_epoch(owner_epoch);
            if let Err(e) = telemetry.tick(
                native.as_ref().map(|config| config.bytes()),
                &mut self.io,
                &budget,
            ) {
                self.logs.push(
                    self.io.now_unix(),
                    "WARN",
                    e.code,
                    "telemetry",
                    "Native telemetry sample unavailable.",
                );
            }
        }
        if let Err(e) = self.diagnostics.tick(
            native.as_ref().map(|config| config.bytes()),
            nodes,
            &mut self.io,
            &budget,
        ) {
            self.logs.push(
                self.io.now_unix(),
                "WARN",
                e.code,
                "diagnostics",
                "Diagnostic operation needs retry.",
            );
        }
    }
    fn documents(&mut self, budget: &Budget<'_>) -> Result<Value, ApiError> {
        self.configuration
            .as_mut()
            .map_err(|e| *e)?
            .documents(&mut self.io, budget)
            .map_err(|e| ApiError {
                status: e.status,
                code: e.code,
                message: e.message,
            })
    }
    fn metadata(&mut self, documents: &Value, budget: &Budget<'_>) -> Result<Value, ApiError> {
        let router = self
            .observations
            .get("/api/router", "", None, &mut self.io, budget)
            .map_err(|e| ApiError {
                status: e.status,
                code: e.code,
                message: e.message,
            })?;
        Ok(
            json!({"model":router["platform"]["model"],"build":router["platform"]["firmware"],"documents":documents["documents"],"pendingCommit":documents.get("pendingCommit"),"runtimeGenerations":{}}),
        )
    }
    #[allow(clippy::too_many_arguments)] // Fixed authenticated boundary preserves separate authority inputs.
    pub fn handle(
        &mut self,
        path: &str,
        method: Method,
        query: &str,
        body: &[u8],
        peer: Option<IpAddr>,
        runtime: Option<&mut RuntimeHttp>,
        nodes: &[crate::native::Node],
        budget: &Budget<'_>,
    ) -> Result<Response, ApiError> {
        let method = if method == Method::Head {
            Method::Get
        } else {
            method
        };
        let mut runtime = runtime;
        if !matches!(method, Method::Get | Method::Head)
            && runtime
                .as_deref()
                .is_some_and(|owner| !owner.subscription_import_allowed())
        {
            return Err(unavailable("runtime_shutting_down"));
        }
        let value = match path {
            "/api/system"
            | "/api/router"
            | "/api/network"
            | "/api/devices"
            | "/api/frpc"
            | "/api/modules"
            | "/api/system/services" => {
                if !matches!(method, Method::Get | Method::Head) {
                    return Err(ApiError {
                        status: 405,
                        code: "method_not_allowed",
                        message: "Method is not allowed for this endpoint.",
                    });
                }
                let mut value = self
                    .observations
                    .get(path, query, peer, &mut self.io, budget)
                    .map_err(|e| ApiError {
                        status: e.status,
                        code: e.code,
                        message: e.message,
                    })?;
                if path == "/api/frpc"
                    && let Some(owner) = runtime.as_deref_mut()
                    && let Ok(Some(config)) = owner.product_config(ServiceId::Frpc)
                {
                    let raw = std::str::from_utf8(config.bytes())
                        .map_err(|_| unavailable("frpc_configuration_unavailable"))?;
                    let projection =
                        crate::product_maintenance::frpc_projection(raw).map_err(|e| ApiError {
                            status: e.status,
                            code: e.code,
                            message: e.message,
                        })?;
                    value["supported"] = true.into();
                    value["proxies"] = projection
                        .get("proxies")
                        .cloned()
                        .unwrap_or_else(|| json!([]));
                    value["running"] = owner
                        .product_status(ServiceId::Frpc)
                        .is_ok_and(|status| status.ready && status.running_matches_accepted)
                        .into();
                    value["reason"] = "".into();
                }
                if path == "/api/modules"
                    && let Some(modules) = value["modules"].as_array_mut()
                {
                    for module in modules {
                        if let Some(capabilities) = module["capabilities"].as_array_mut() {
                            for capability in capabilities {
                                let kind = capability["id"].as_str().unwrap_or("");
                                let supported = match kind {
                                    "configure" | "apply" | "stage" | "rollback" => {
                                        self.configuration.is_ok()
                                    }
                                    "runtime" | "acquire" | "start" | "stop" => runtime.is_some(),
                                    "backup" | "import" => self.maintenance.is_ok(),
                                    _ => capability["supported"].as_bool().unwrap_or(false),
                                };
                                if supported {
                                    capability["supported"] = true.into();
                                    capability
                                        .as_object_mut()
                                        .map(|fields| fields.remove("reason"));
                                }
                            }
                        }
                    }
                }
                value
            }
            p if p.starts_with("/api/configuration") => {
                if method == Method::Post {
                    self.observations.invalidate();
                }
                let mut before = |_: &mut B, network: bool| {
                    if network && let Some(owner) = runtime.as_deref() {
                        owner
                            .product_withdraw(Instant::now() + Duration::from_secs(30))
                            .map_err(|_| crate::product_configuration::ApiError {
                                status: 409,
                                code: "capture_cleanup_failed",
                                message: "Capture withdrawal needs retry before native changes.",
                            })?;
                    }
                    Ok(())
                };
                self.configuration
                    .as_mut()
                    .map_err(|e| *e)?
                    .handle_with_before_mutation(
                        path,
                        method,
                        query,
                        body,
                        &mut self.io,
                        budget,
                        &mut before,
                    )
                    .map_err(|e| ApiError {
                        status: e.status,
                        code: e.code,
                        message: e.message,
                    })?
            }
            "/api/proxy/metrics"
            | "/api/proxy/probe"
            | "/api/traffic/history"
            | "/api/devices/activity"
            | "/api/devices/annotations" => {
                let config = runtime
                    .as_deref()
                    .and_then(|owner| owner.product_config(ServiceId::SingBox).ok().flatten());
                self.telemetry
                    .as_mut()
                    .map_err(|e| *e)?
                    .handle(
                        path,
                        method,
                        query,
                        body,
                        config.as_ref().map(|snapshot| snapshot.bytes()),
                        &mut self.io,
                        budget,
                    )
                    .map_err(|e| ApiError {
                        status: e.status,
                        code: e.code,
                        message: e.message,
                    })?
            }
            "/api/proxy/request-traces"
            | "/api/proxy/node-probes"
            | "/api/proxy/node-probes/history" => {
                let config = runtime
                    .as_deref()
                    .and_then(|owner| owner.product_config(ServiceId::SingBox).ok().flatten());
                self.diagnostics
                    .handle(
                        path,
                        method,
                        query,
                        body,
                        config.as_ref().map(|snapshot| snapshot.bytes()),
                        nodes,
                        &mut self.io,
                        budget,
                    )
                    .map_err(|e| ApiError {
                        status: e.status,
                        code: e.code,
                        message: e.message,
                    })?
            }
            "/api/proxy/plan" | "/api/frpc/plan" => {
                if method != Method::Post {
                    return Err(ApiError {
                        status: 405,
                        code: "method_not_allowed",
                        message: "Method is not allowed for this endpoint.",
                    });
                }
                self.plans.plan(path, body).map_err(|e| ApiError {
                    status: e.status,
                    code: e.code,
                    message: e.message,
                })?
            }
            "/api/operations/apply" => {
                return Err(ApiError {
                    status: 501,
                    code: "not_implemented",
                    message: "Applying advisory plans is not implemented; no system changes were made.",
                });
            }
            "/api/logs" => {
                if !matches!(method, Method::Get | Method::Head) {
                    return Err(ApiError {
                        status: 405,
                        code: "method_not_allowed",
                        message: "Method is not allowed for this endpoint.",
                    });
                }
                self.logs.response(query).map_err(|e| ApiError {
                    status: e.status,
                    code: e.code,
                    message: e.message,
                })?
            }
            "/api/system/services/action" => {
                self.observations.invalidate();
                if method != Method::Post {
                    return Err(invalid());
                }
                let config = self
                    .configuration
                    .as_mut()
                    .map_err(|e| *e)?
                    .handle(
                        "/api/configuration/status",
                        Method::Get,
                        "",
                        &[],
                        &mut self.io,
                        budget,
                    )
                    .map_err(|e| ApiError {
                        status: e.status,
                        code: e.code,
                        message: e.message,
                    })?;
                if config
                    .get("pendingCommit")
                    .is_some_and(|pending| !pending.is_null())
                {
                    return Err(ApiError {
                        status: 409,
                        code: "pending_confirmation",
                        message: "Confirm or roll back native configuration first.",
                    });
                }
                let snapshot = self
                    .observations
                    .get("/api/system/services", "", peer, &mut self.io, budget)
                    .map_err(|e| ApiError {
                        status: e.status,
                        code: e.code,
                        message: e.message,
                    })?;
                let (mut action, accepted) =
                    crate::product_support::service_action(body, &snapshot, &mut self.io, budget)
                        .map_err(|e| ApiError {
                        status: e.status,
                        code: e.code,
                        message: e.message,
                    })?;
                self.observations.invalidate();
                action["snapshot"] = self
                    .observations
                    .get("/api/system/services", "", peer, &mut self.io, budget)
                    .map_err(|e| ApiError {
                        status: e.status,
                        code: e.code,
                        message: e.message,
                    })?;
                if !accepted {
                    return Ok(Response::Json {
                        status: 503,
                        value: action,
                    });
                }
                action
            }
            "/api/maintenance/backup" => {
                if method != Method::Post {
                    return Err(invalid());
                }
                let input: Backup = decode(body)?;
                let documents = self.documents(budget)?;
                let mut metadata = self.metadata(&documents, budget)?;
                let mut snapshots = Vec::new();
                if let Some(owner) = runtime.as_deref() {
                    for service in [ServiceId::SingBox, ServiceId::Frpc] {
                        if let Ok(Some(snapshot)) = owner.product_config(service) {
                            metadata["runtimeGenerations"][service.as_str()] =
                                snapshot.identity.generation.into();
                            snapshots
                                .push((service.as_str().to_owned(), snapshot.bytes().to_vec()));
                        }
                    }
                }
                let annotations = self
                    .io
                    .read(&self.data.join("device-names.json"), 128 << 10, budget)
                    .ok();
                let raw = self
                    .maintenance
                    .as_ref()
                    .map_err(|e| *e)?
                    .backup(
                        &input.scopes,
                        metadata,
                        documents,
                        &snapshots,
                        annotations.as_deref(),
                        &mut self.io,
                        budget,
                    )
                    .map_err(|e| ApiError {
                        status: e.status,
                        code: e.code,
                        message: e.message,
                    })?;
                return Ok(Response::Backup(raw));
            }
            "/api/maintenance/import/preview" => {
                if method == Method::Delete {
                    let id = query.strip_prefix("id=").ok_or_else(invalid)?;
                    json!({"discarded":self.maintenance.as_ref().map_err(|e|*e)?.discard(id)})
                } else if method == Method::Post {
                    let documents = self.documents(budget)?;
                    let metadata = self.metadata(&documents, budget)?;
                    let generation = documents["generation"].as_u64().ok_or_else(invalid)?;
                    self.maintenance
                        .as_ref()
                        .map_err(|e| *e)?
                        .preview(body, metadata, generation, budget)
                        .map_err(|e| ApiError {
                            status: e.status,
                            code: e.code,
                            message: e.message,
                        })?
                } else {
                    return Err(invalid());
                }
            }
            "/api/maintenance/import/stage" => {
                if method != Method::Post {
                    return Err(invalid());
                }
                let input: ImportStage = decode(body)?;
                let current = self.documents(budget)?;
                if current["generation"].as_u64() != Some(input.generation) {
                    return Err(ApiError {
                        status: 409,
                        code: "generation_conflict",
                        message: "Native configuration changed; refresh before importing.",
                    });
                }
                let candidates = self
                    .maintenance
                    .as_ref()
                    .map_err(|e| *e)?
                    .candidates(
                        &input.preview_id,
                        input.generation,
                        &input.modules,
                        input.acknowledge_model_mismatch,
                        budget,
                    )
                    .map_err(|e| ApiError {
                        status: e.status,
                        code: e.code,
                        message: e.message,
                    })?;
                let configuration = self.configuration.as_mut().map_err(|e| *e)?;
                let mut drafts = Vec::new();
                for candidate in candidates {
                    match configuration.stage_for_import(
                        &candidate.module,
                        &candidate.content,
                        input.generation,
                        &mut self.io,
                        budget,
                    ) {
                        Ok(draft) => drafts.push(draft),
                        Err(error) => {
                            let mut retained = Vec::new();
                            for draft in &drafts {
                                if let Some(id) = draft["id"].as_str()
                                    && configuration.discard_import_draft(id).is_err()
                                {
                                    retained.push(id.to_owned());
                                }
                            }
                            if !retained.is_empty() {
                                return Ok(Response::Json {
                                    status: 500,
                                    value: json!({"error":{"code":"import_cleanup_failed","message":"Some staged drafts remain available for explicit retry."},"retainedDraftIds":retained,"cleanupPending":true,"causeCode":error.code}),
                                });
                            }
                            return Err(ApiError {
                                status: error.status,
                                code: error.code,
                                message: error.message,
                            });
                        }
                    }
                }
                self.maintenance
                    .as_ref()
                    .map_err(|e| *e)?
                    .discard(&input.preview_id);
                json!({"drafts":drafts,"generation":input.generation})
            }
            _ => {
                return Err(ApiError {
                    status: 404,
                    code: "not_found",
                    message: "Product endpoint is unavailable.",
                });
            }
        };
        if method == Method::Post {
            self.logs.push(
                self.io.now_unix(),
                "INFO",
                "operation_completed",
                "product",
                "Native product operation completed.",
            );
        }
        Ok(Response::Json { status: 200, value })
    }
}
fn decode<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T, ApiError> {
    if body.len() > 64 << 10
        || body
            .iter()
            .find(|b| !b.is_ascii_whitespace())
            .is_none_or(|b| *b != b'{')
    {
        return Err(invalid());
    }
    serde_json::from_slice(body).map_err(|_| invalid())
}
pub enum Response {
    Json { status: u16, value: Value },
    Backup(Vec<u8>),
}
pub fn is_product_path(path: &str) -> bool {
    matches!(
        path,
        "/api/system"
            | "/api/router"
            | "/api/network"
            | "/api/devices"
            | "/api/frpc"
            | "/api/modules"
            | "/api/system/services"
            | "/api/system/services/action"
            | "/api/proxy/metrics"
            | "/api/proxy/probe"
            | "/api/traffic/history"
            | "/api/devices/activity"
            | "/api/devices/annotations"
            | "/api/proxy/request-traces"
            | "/api/proxy/node-probes"
            | "/api/proxy/node-probes/history"
            | "/api/proxy/plan"
            | "/api/frpc/plan"
            | "/api/operations/apply"
            | "/api/logs"
            | "/api/maintenance/backup"
            | "/api/maintenance/import/preview"
            | "/api/maintenance/import/stage"
            | "/api/configuration"
            | "/api/configuration/stage"
            | "/api/configuration/commit"
            | "/api/configuration/drafts"
            | "/api/configuration/confirm"
            | "/api/configuration/rollback"
            | "/api/configuration/status"
    )
}

struct Count(usize);
impl Write for Count {
    fn write(&mut self, raw: &[u8]) -> io::Result<usize> {
        self.0 = self
            .0
            .checked_add(raw.len())
            .filter(|size| *size <= 8 << 20)
            .ok_or_else(|| io::Error::other("product response bound"))?;
        Ok(raw.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub fn write(
    writer: &mut impl Write,
    response: Result<Response, ApiError>,
    head: bool,
) -> io::Result<()> {
    let (status, value) = match response {
        Ok(Response::Backup(raw)) => {
            return crate::http::write_response_extra(
                writer,
                200,
                "OK",
                &raw,
                head,
                "application/json; charset=utf-8",
                &[(
                    "Content-Disposition",
                    "attachment; filename=\"be6500panel-backup.json\"",
                )],
            );
        }
        Ok(Response::Json { status, value }) => (status, value),
        Err(error) => (
            error.status,
            json!({"error":{"code":error.code,"message":error.message}}),
        ),
    };
    let mut count = Count(0);
    serde_json::to_writer(&mut count, &value).map_err(io::Error::other)?;
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        413 => "Payload Too Large",
        422 => "Unprocessable Entity",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "Error",
    };
    let mut buffered = io::BufWriter::with_capacity(8192, writer);
    crate::http::write_headers(
        &mut buffered,
        status,
        reason,
        "application/json; charset=utf-8",
        (count.0 + 1) as u64,
    )?;
    if !head {
        serde_json::to_writer(&mut buffered, &value).map_err(io::Error::other)?;
        buffered.write_all(b"\n")?;
    }
    buffered.flush()
}

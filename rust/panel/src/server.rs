//! One synchronous bounded connection handler. No runtime or per-connection threads.
use crate::auth::Auth;
use crate::http::{self, DeadlineWriter, MAX_HEADER_BYTES, Method};
use crate::memory::read_memory;
use crate::rules_http::{self, RulesState};
use crate::static_files::StaticFiles;
use std::io;
use std::net::{Shutdown, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const HEALTH_BODY: &[u8] = b"{\"status\":\"ok\",\"mode\":\"host\",\"readOnly\":true}";
const JSON_TYPE: &str = "application/json; charset=utf-8";

pub struct Service {
    pub proc_root: PathBuf,
    static_files: Option<StaticFiles>,
    auth: Mutex<Auth>,
    // None means no data directory; Err isolates a failed rule feature.
    rules: Option<Mutex<Result<RulesState, rules_http::RulesError>>>,
}

#[derive(Clone, Copy)]
struct JsonDepth(u8);

impl<'de> serde::de::DeserializeSeed<'de> for JsonDepth {
    type Value = serde_json::Value;
    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        if self.0 > 32 {
            return Err(serde::de::Error::custom("JSON depth limit"));
        }
        deserializer.deserialize_any(self)
    }
}
impl<'de> serde::de::Visitor<'de> for JsonDepth {
    type Value = serde_json::Value;
    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("JSON value")
    }
    fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
        Ok(serde_json::Value::Null)
    }
    fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<Self::Value, E> {
        Ok(value.into())
    }
    fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Self::Value, E> {
        Ok(value.into())
    }
    fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Self::Value, E> {
        Ok(value.into())
    }
    fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value, E> {
        serde_json::Number::from_f64(value)
            .map(serde_json::Value::Number)
            .ok_or_else(|| E::custom("invalid number"))
    }
    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
        Ok(value.into())
    }
    fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Self::Value, E> {
        Ok(value.into())
    }
    fn visit_seq<A: serde::de::SeqAccess<'de>>(
        self,
        mut values: A,
    ) -> Result<Self::Value, A::Error> {
        let mut list = Vec::new();
        while let Some(value) = values.next_element_seed(JsonDepth(self.0 + 1))? {
            list.push(value);
        }
        Ok(serde_json::Value::Array(list))
    }
    fn visit_map<A: serde::de::MapAccess<'de>>(
        self,
        mut fields: A,
    ) -> Result<Self::Value, A::Error> {
        let mut object = serde_json::Map::new();
        while let Some(name) = fields.next_key::<String>()? {
            if object.contains_key(&name) {
                return Err(serde::de::Error::custom("duplicate field"));
            }
            let value = fields.next_value_seed(JsonDepth(self.0 + 1))?;
            object.insert(name, value);
        }
        Ok(serde_json::Value::Object(object))
    }
}

fn login_password(body: &[u8]) -> Result<String, (&'static str, &'static str)> {
    use serde::de::DeserializeSeed;
    let mut decoder = serde_json::Deserializer::from_slice(body);
    let value = JsonDepth(0).deserialize(&mut decoder).map_err(|_| {
        (
            "invalid_json",
            "Provide one JSON object with unique field names.",
        )
    })?;
    decoder
        .end()
        .map_err(|_| ("invalid_json", "Provide only one JSON object."))?;
    let serde_json::Value::Object(mut object) = value else {
        return Err(("invalid_json", "JSON body must be an object."));
    };
    if object.get("password").is_none_or(|value| value.is_null()) {
        return Err((
            "invalid_input",
            "Required fields must be present and non-null.",
        ));
    }
    if object.len() != 1 {
        return Err((
            "invalid_json",
            "JSON fields or types do not match the request contract.",
        ));
    }
    match object.remove("password") {
        Some(serde_json::Value::String(password)) => Ok(password),
        _ => Err((
            "invalid_json",
            "JSON fields or types do not match the request contract.",
        )),
    }
}

impl Service {
    /// An unconfigured service is the explicit loopback diagnostic mode.
    pub fn new(proc_root: PathBuf) -> Self {
        Self {
            proc_root,
            static_files: None,
            auth: Mutex::new(Auth::new("")),
            rules: None,
        }
    }

    pub(crate) fn authentication_required(&self) -> bool {
        self.auth.lock().is_ok_and(|auth| auth.required())
    }
    pub fn with_auth(mut self, auth: Auth) -> Self {
        self.auth = Mutex::new(auth);
        self
    }

    pub fn with_static_files(mut self, files: StaticFiles) -> Self {
        self.static_files = Some(files);
        self
    }

    /// Opens the draft feature once. A bad draft/source does not stop health,
    /// sessions or static files, and cannot be mistaken for an empty draft.
    pub fn with_data_dir(mut self, path: impl AsRef<Path>) -> Self {
        self.rules = Some(Mutex::new(RulesState::open(path)));
        self
    }

    fn draft_writes_available(&self) -> bool {
        self.rules
            .as_ref()
            .is_some_and(|rules| rules.lock().is_ok_and(|state| state.is_ok()))
    }

    pub fn handle(&self, stream: TcpStream) -> io::Result<()> {
        self.handle_with_deadlines(stream, http::REQUEST_DEADLINE, http::WRITE_DEADLINE)
    }

    /// Explicit budgets also let host tests exercise absolute deadlines quickly.
    pub fn handle_with_deadlines(
        &self,
        mut stream: TcpStream,
        read_budget: Duration,
        write_budget: Duration,
    ) -> io::Result<()> {
        let result = self.respond(&mut stream, read_budget, write_budget, None);
        let _ = stream.shutdown(Shutdown::Both);
        result
    }

    /// Borrows the one exclusive fixed-service owner for an explicit request.
    /// Main does not activate this until production ownership is qualified.
    pub fn handle_with_runtime(
        &self,
        mut stream: TcpStream,
        runtime: &mut crate::runtime_http::RuntimeHttp,
    ) -> io::Result<()> {
        let result = self.respond(
            &mut stream,
            http::REQUEST_DEADLINE,
            http::WRITE_DEADLINE,
            Some(runtime),
        );
        let _ = stream.shutdown(Shutdown::Both);
        result
    }

    fn respond(
        &self,
        stream: &mut TcpStream,
        read_budget: Duration,
        write_budget: Duration,
        runtime: Option<&mut crate::runtime_http::RuntimeHttp>,
    ) -> io::Result<()> {
        let deadline = Instant::now() + read_budget;
        let mut buffer = [0_u8; MAX_HEADER_BYTES + 1];
        let parsed = http::read_headers(stream, &mut buffer, deadline).and_then(|read| {
            http::parse_request(&buffer[..read.length]).map(|request| (read, request))
        });
        let (read, request) = match parsed {
            Ok(parsed) => parsed,
            Err(error) => return write_http_error(stream, write_budget, error),
        };
        let head_only = request.method == Method::Head;
        // Same-origin precedes rate accounting, session creation, or logout.
        if matches!(request.method, Method::Post | Method::Delete) && !request.same_origin() {
            return api_error(
                &mut DeadlineWriter::new(stream, write_budget),
                403,
                "Forbidden",
                "origin_rejected",
                "Unsafe requests must be same-origin.",
                head_only,
                &[],
            );
        }
        if matches!(
            request.path(),
            "/api/session/login"
                | "/api/proxy/local-rules"
                | "/api/proxy/local-rules/preview"
                | "/api/proxy/capture"
                | "/api/proxy/import"
        ) && request.method == Method::Post
            && !http::json_content_type(request.content_type)
        {
            return api_error(
                &mut DeadlineWriter::new(stream, write_budget),
                415,
                "Unsupported Media Type",
                "unsupported_media_type",
                "Content-Type must be application/json.",
                false,
                &[],
            );
        }
        // Logout usually has no body (current browser contract); any submitted
        // body still needs an unambiguous length and JSON media type.
        if matches!(request.method, Method::Post | Method::Delete)
            && (request.content_length > 0 || request.content_type.is_some())
            && !http::json_content_type(request.content_type)
        {
            return api_error(
                &mut DeadlineWriter::new(stream, write_budget),
                415,
                "Unsupported Media Type",
                "unsupported_media_type",
                "Content-Type must be application/json.",
                false,
                &[],
            );
        }
        let body =
            match http::read_body(stream, &buffer[read.length..read.used], &request, deadline) {
                Ok(body) => body,
                Err(error) => return write_http_error(stream, write_budget, error),
            };
        let peer = stream.peer_addr()?.ip();
        let response_budget = if (crate::runtime_http::is_runtime_path(request.path())
            || matches!(request.path(), "/api/proxy/import" | "/api/proxy/select"))
            && request.method == Method::Post
        {
            write_budget.max(Duration::from_secs(90))
        } else if request.path() == "/api/proxy/capture"
            && matches!(request.method, Method::Post | Method::Delete)
        {
            write_budget.max(Duration::from_secs(65))
        } else {
            write_budget
        };
        let mut writer = DeadlineWriter::new(stream, response_budget);
        let api = request.path() == "/api" || request.path().starts_with("/api/");
        let public = matches!(
            request.path(),
            "/api/session" | "/api/session/login" | "/api/session/logout"
        );
        let mut auth = self
            .auth
            .lock()
            .map_err(|_| io::Error::other("session state unavailable"))?;
        if api && !public && !auth.authenticated(request.cookie) {
            drop(auth);
            return api_error(
                &mut writer,
                401,
                "Unauthorized",
                "unauthenticated",
                "Authentication required.",
                head_only,
                &[],
            );
        }
        if crate::runtime_http::is_runtime_path(request.path())
            || request.path() == "/api/proxy/capture"
        {
            drop(auth);
            let Some(runtime) = runtime else {
                return crate::runtime_http::unavailable(&mut writer, head_only);
            };
            return runtime.respond(&mut writer, request.target, request.method, &body);
        }
        if rules_http::is_rules_path(request.path()) {
            // Session lock never covers draft filesystem I/O or streaming.
            drop(auth);
            if matches!(
                request.path(),
                "/api/proxy/local-rules/preview"
                    | "/api/proxy/local-rules/apply"
                    | "/api/proxy/select"
                    | "/api/proxy/import"
            ) && request.method != Method::Post
            {
                return rules_http::method_not_allowed(&mut writer, head_only, "POST");
            }
            if matches!(
                request.path(),
                "/api/proxy/local-rules/apply" | "/api/proxy/select" | "/api/proxy/import"
            ) && runtime.is_none()
            {
                return rules_http::runtime_unavailable(&mut writer, head_only);
            }
            let Some(rules) = &self.rules else {
                return rules_http::unavailable(&mut writer, head_only);
            };
            let Ok(mut state) = rules.lock() else {
                return rules_http::unavailable(&mut writer, head_only);
            };
            return match state.as_mut() {
                Ok(state) => {
                    state.respond(&mut writer, request.path(), request.method, &body, runtime)
                }
                Err(_) => rules_http::unavailable(&mut writer, head_only),
            };
        }
        match request.path() {
            "/api/session" if request.method != Method::Post => {
                session_response(&mut writer, &mut auth, request.cookie, head_only, &[])
            }
            "/api/session/login" if request.method == Method::Post => {
                if auth.required() && !auth.allow_attempt(peer) {
                    return api_error(
                        &mut writer,
                        429,
                        "Too Many Requests",
                        "rate_limited",
                        "Too many login attempts; try again later.",
                        false,
                        &[("Retry-After", "60")],
                    );
                }
                let password = match login_password(&body) {
                    Ok(password) => password,
                    Err((code, message)) => {
                        return api_error(
                            &mut writer,
                            400,
                            "Bad Request",
                            code,
                            message,
                            false,
                            &[],
                        );
                    }
                };
                if !auth.required() {
                    return session_response(&mut writer, &mut auth, request.cookie, false, &[]);
                }
                if !auth.password_matches(&password) {
                    return api_error(
                        &mut writer,
                        401,
                        "Unauthorized",
                        "invalid_password",
                        "Invalid password.",
                        false,
                        &[],
                    );
                }
                let cookie = match auth.create_session() {
                    Ok(cookie) => cookie,
                    Err(_) => {
                        return api_error(
                            &mut writer,
                            500,
                            "Internal Server Error",
                            "session_unavailable",
                            "Cannot create session.",
                            false,
                            &[],
                        );
                    }
                };
                auth.reset_attempts(peer);
                http::write_response_extra(
                    &mut writer,
                    200,
                    "OK",
                    b"{\"authenticated\":true,\"authRequired\":true}\n",
                    false,
                    JSON_TYPE,
                    &[("Set-Cookie", cookie.header_value())],
                )
            }
            "/api/session/logout" if request.method == Method::Post => {
                auth.logout(request.cookie);
                session_response(
                    &mut writer,
                    &mut auth,
                    request.cookie,
                    false,
                    &[("Set-Cookie", Auth::clear_cookie())],
                )
            }
            "/api/session/login" | "/api/session/logout" | "/api/session" => {
                let allowed = if request.path() == "/api/session" {
                    "GET, HEAD"
                } else {
                    "POST"
                };
                api_error(
                    &mut writer,
                    405,
                    "Method Not Allowed",
                    "method_not_allowed",
                    "Method is not allowed for this endpoint.",
                    head_only,
                    &[("Allow", allowed)],
                )
            }
            "/api/health" if request.method != Method::Post => {
                drop(auth);
                let body = if runtime.is_some() {
                    b"{\"status\":\"ok\",\"mode\":\"manager\",\"readOnly\":false,\"runtimeEnabled\":true}".as_slice()
                } else if self.draft_writes_available() {
                    b"{\"status\":\"ok\",\"mode\":\"host\",\"readOnly\":false}".as_slice()
                } else {
                    HEALTH_BODY
                };
                http::write_response(&mut writer, 200, "OK", body, head_only)
            }
            "/api/system/memory" if request.method != Method::Post => {
                match read_memory(&self.proc_root) {
                    Ok(memory) => {
                        let body = format!(
                            "{{\"source\":\"procfs\",\"totalBytes\":{},\"availableBytes\":{}}}",
                            memory.total_bytes, memory.available_bytes
                        );
                        http::write_response(&mut writer, 200, "OK", body.as_bytes(), head_only)
                    }
                    Err(_) => http::write_response(
                        &mut writer,
                        503,
                        "Service Unavailable",
                        b"{\"error\":\"memory unavailable\"}",
                        head_only,
                    ),
                }
            }
            path if !api && request.method != Method::Post => {
                drop(auth);
                if let Some(files) = &self.static_files
                    && let Ok(mut asset) = files.open(path)
                {
                    http::write_headers(&mut writer, 200, "OK", asset.content_type, asset.length)?;
                    if !head_only {
                        asset.stream(&mut writer)?;
                    }
                    return Ok(());
                }
                http::write_response(
                    &mut writer,
                    404,
                    "Not Found",
                    b"{\"error\":\"not found\"}",
                    head_only,
                )
            }
            _ if request.method == Method::Post => http::write_response(
                &mut writer,
                405,
                "Method Not Allowed",
                b"{\"error\":\"method not allowed\"}",
                false,
            ),
            _ => http::write_response(
                &mut writer,
                404,
                "Not Found",
                b"{\"error\":\"not found\"}",
                head_only,
            ),
        }
    }
}

fn write_http_error(
    stream: &mut TcpStream,
    budget: Duration,
    error: http::HttpError,
) -> io::Result<()> {
    let (status, reason, body) = error.kind.response();
    http::write_response(
        &mut DeadlineWriter::new(stream, budget),
        status,
        reason,
        body,
        error.head_only,
    )
}
fn session_response(
    writer: &mut impl io::Write,
    auth: &mut Auth,
    cookie: Option<&str>,
    head: bool,
    headers: &[(&str, &str)],
) -> io::Result<()> {
    let body = format!(
        "{{\"authenticated\":{},\"authRequired\":{}}}\n",
        auth.authenticated(cookie),
        auth.required()
    );
    http::write_response_extra(writer, 200, "OK", body.as_bytes(), head, JSON_TYPE, headers)
}
// Every call supplies fixed code/message constants. No password, cookie, body,
// parser detail, target or filesystem input is included in public errors.
fn api_error(
    writer: &mut impl io::Write,
    status: u16,
    reason: &str,
    code: &str,
    message: &str,
    head: bool,
    headers: &[(&str, &str)],
) -> io::Result<()> {
    let body = format!("{{\"error\":{{\"code\":\"{code}\",\"message\":\"{message}\"}}}}\n");
    http::write_response_extra(
        writer,
        status,
        reason,
        body.as_bytes(),
        head,
        JSON_TYPE,
        headers,
    )
}

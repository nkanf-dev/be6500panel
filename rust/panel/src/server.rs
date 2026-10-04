//! One synchronous connection handler. No background collectors or runtime.
use crate::http::{self, DeadlineWriter, MAX_HEADER_BYTES, Method};
use crate::memory::read_memory;
use crate::static_files::StaticFiles;
use std::io;
use std::net::{Shutdown, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

pub const HEALTH_BODY: &[u8] = b"{\"status\":\"ok\",\"mode\":\"host\",\"readOnly\":true}";

pub struct Service {
    pub proc_root: PathBuf,
    static_files: Option<StaticFiles>,
}

impl Service {
    pub fn new(proc_root: PathBuf) -> Self {
        Self {
            proc_root,
            static_files: None,
        }
    }

    pub fn with_static_files(mut self, files: StaticFiles) -> Self {
        self.static_files = Some(files);
        self
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
        let result = self.respond(&mut stream, read_budget, write_budget);
        let _ = stream.shutdown(Shutdown::Both);
        result
    }

    fn respond(
        &self,
        stream: &mut TcpStream,
        read_budget: Duration,
        write_budget: Duration,
    ) -> io::Result<()> {
        let mut buffer = [0_u8; MAX_HEADER_BYTES + 1];
        let request = http::read_headers(stream, &mut buffer, read_budget)
            .and_then(|length| http::parse_request(&buffer[..length]));
        let request = match request {
            Ok(request) => request,
            Err(error) => {
                let (status, reason, body) = error.kind.response();
                return http::write_response(
                    &mut DeadlineWriter::new(stream, write_budget),
                    status,
                    reason,
                    body,
                    error.head_only,
                );
            }
        };
        let head_only = request.method == Method::Head;
        let mut writer = DeadlineWriter::new(stream, write_budget);
        match request.path() {
            "/api/health" => http::write_response(&mut writer, 200, "OK", HEALTH_BODY, head_only),
            "/api/system/memory" => match read_memory(&self.proc_root) {
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
            },
            path if path != "/api" && !path.starts_with("/api/") => {
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

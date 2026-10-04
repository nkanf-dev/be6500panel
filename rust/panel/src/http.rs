//! Bounded HTTP/1.x: one diagnostic or session request per connection.
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

pub const MAX_HEADER_BYTES: usize = 16 * 1024;
pub const MAX_HEADERS: usize = 64;
pub const MAX_TARGET_BYTES: usize = 2048;
pub const MAX_BODY_BYTES: usize = 64 * 1024;
pub const MAX_RUNTIME_BODY_BYTES: usize = 2 << 20;
pub const MAX_LOGIN_BODY_BYTES: usize = 4 * 1024;
pub const MAX_RULES_BODY_BYTES: usize = 256 * 1024;
pub const MAX_HOST_BYTES: usize = 512;
pub const MAX_ORIGIN_BYTES: usize = 1024;
pub const MAX_COOKIE_BYTES: usize = 4 * 1024;
pub const REQUEST_DEADLINE: Duration = Duration::from_secs(5);
pub const WRITE_DEADLINE: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Head,
    Post,
}

#[derive(PartialEq, Eq)]
pub struct Request<'a> {
    pub method: Method,
    pub target: &'a str,
    pub host: Option<&'a str>,
    pub origin: Option<&'a str>,
    pub cookie: Option<&'a str>,
    pub content_type: Option<&'a str>,
    pub content_length: usize,
    pub fetch_site: Option<&'a str>,
}

impl std::fmt::Debug for Request<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Request")
            .field("method", &self.method)
            .finish_non_exhaustive()
    }
}

impl Request<'_> {
    pub fn path(&self) -> &str {
        self.target
            .split_once('?')
            .map_or(self.target, |(path, _)| path)
    }

    /// Plain TCP is the only transport here. X-Forwarded-Proto is not trusted.
    /// As in Go, requests without Origin are allowed unless Fetch Metadata says cross-site.
    pub fn same_origin(&self) -> bool {
        if self.fetch_site == Some("cross-site") {
            return false;
        }
        match self.origin {
            None => true,
            Some(origin) => origin
                .strip_prefix("http://")
                .is_some_and(|authority| self.host == Some(authority) && valid_host(authority)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    BadRequest,
    MethodNotAllowed,
    Timeout,
    HeadersTooLarge,
    TargetTooLong,
    BodyTooLarge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HttpError {
    pub kind: ErrorKind,
    pub head_only: bool,
}

impl ErrorKind {
    pub fn response(self) -> (u16, &'static str, &'static [u8]) {
        match self {
            Self::BadRequest => (400, "Bad Request", b"{\"error\":\"bad request\"}"),
            Self::MethodNotAllowed => (
                405,
                "Method Not Allowed",
                b"{\"error\":\"method not allowed\"}",
            ),
            Self::Timeout => (408, "Request Timeout", b"{\"error\":\"request timeout\"}"),
            Self::HeadersTooLarge => (
                431,
                "Request Header Fields Too Large",
                b"{\"error\":\"headers too large\"}",
            ),
            Self::TargetTooLong => (414, "URI Too Long", b"{\"error\":\"target too long\"}"),
            Self::BodyTooLarge => (413, "Payload Too Large", b"{\"error\":{\"code\":\"body_too_large\",\"message\":\"Request body exceeds the endpoint limit.\"}}\n"),
        }
    }
}

fn token(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}

fn error(kind: ErrorKind, bytes: &[u8]) -> HttpError {
    HttpError {
        kind,
        head_only: bytes.starts_with(b"HEAD "),
    }
}

pub fn parse_request(bytes: &[u8]) -> Result<Request<'_>, HttpError> {
    let bad = || error(ErrorKind::BadRequest, bytes);
    if bytes.len() > MAX_HEADER_BYTES {
        return Err(error(ErrorKind::HeadersTooLarge, bytes));
    }
    if !bytes.ends_with(b"\r\n\r\n") || !bytes.is_ascii() {
        return Err(bad());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| bad())?;
    let mut lines = text[..text.len() - 4].split("\r\n");
    let first = lines.next().ok_or_else(bad)?;
    let mut parts = first.split(' ');
    let method_text = parts.next().ok_or_else(bad)?;
    let target = parts.next().ok_or_else(bad)?;
    let version = parts.next().ok_or_else(bad)?;
    if parts.next().is_some() || !matches!(version, "HTTP/1.0" | "HTTP/1.1") {
        return Err(bad());
    }
    if target.len() > MAX_TARGET_BYTES {
        return Err(error(ErrorKind::TargetTooLong, bytes));
    }
    if !target.starts_with('/') || target.starts_with("//") || target.contains('#') {
        return Err(bad());
    }
    decode_target(target).map_err(|_| bad())?;
    let method = match method_text {
        "GET" => Method::Get,
        "HEAD" => Method::Head,
        "POST" => Method::Post,
        _ if !method_text.is_empty() && method_text.bytes().all(token) => {
            return Err(error(ErrorKind::MethodNotAllowed, bytes));
        }
        _ => return Err(bad()),
    };
    let mut request = Request {
        method,
        target,
        host: None,
        origin: None,
        cookie: None,
        content_type: None,
        content_length: 0,
        fetch_site: None,
    };
    let mut content_length = None;
    for (index, line) in lines.enumerate() {
        if index >= MAX_HEADERS {
            return Err(error(ErrorKind::HeadersTooLarge, bytes));
        }
        let (name, value) = line.split_once(':').ok_or_else(bad)?;
        if name.is_empty()
            || !name.bytes().all(token)
            || value.bytes().any(|b| (b < 32 && b != b'\t') || b == 127)
        {
            return Err(bad());
        }
        let value = value.trim_matches([' ', '\t']);
        if name.eq_ignore_ascii_case("host") {
            if request.host.is_some() || !valid_host(value) {
                return Err(bad());
            }
            request.host = Some(value);
        } else if name.eq_ignore_ascii_case("content-length") {
            if content_length.is_some()
                || value.is_empty()
                || !value.bytes().all(|b| b.is_ascii_digit())
            {
                return Err(bad());
            }
            content_length = Some(value.parse::<usize>().map_err(|_| bad())?);
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err(bad());
        } else {
            let (slot, limit) = if name.eq_ignore_ascii_case("origin") {
                (&mut request.origin, MAX_ORIGIN_BYTES)
            } else if name.eq_ignore_ascii_case("cookie") {
                (&mut request.cookie, MAX_COOKIE_BYTES)
            } else if name.eq_ignore_ascii_case("content-type") {
                (&mut request.content_type, 256)
            } else if name.eq_ignore_ascii_case("sec-fetch-site") {
                (&mut request.fetch_site, 64)
            } else {
                continue;
            };
            if slot.is_some() || value.len() > limit {
                return Err(bad());
            }
            *slot = Some(value);
        }
    }
    if version == "HTTP/1.1" && request.host.is_none() {
        return Err(bad());
    }
    if method == Method::Post
        && !matches!(
            request.path(),
            "/api/session/login"
                | "/api/session/logout"
                | "/api/proxy/local-rules"
                | "/api/proxy/local-rules/preview"
                | "/api/proxy/local-rules/apply"
                | "/api/proxy/select"
                | "/api/runtime/configure"
                | "/api/runtime/start"
                | "/api/runtime/stop"
                | "/api/runtime/restart"
                | "/api/runtime/restore"
                | "/api/runtime/acquire"
        )
    {
        return Err(error(ErrorKind::MethodNotAllowed, bytes));
    }
    request.content_length = match (method, content_length) {
        (Method::Post, None) => return Err(bad()),
        (_, Some(length)) => length,
        (_, None) => 0,
    };
    if method != Method::Post && request.content_length != 0 {
        return Err(bad());
    }
    let limit = match request.path() {
        "/api/session/login" => MAX_LOGIN_BODY_BYTES,
        "/api/proxy/local-rules" | "/api/proxy/local-rules/preview" => MAX_RULES_BODY_BYTES,
        "/api/runtime/configure" => MAX_RUNTIME_BODY_BYTES,
        _ => MAX_BODY_BYTES,
    };
    if request.content_length > limit {
        return Err(error(ErrorKind::BodyTooLarge, bytes));
    }
    Ok(request)
}

fn valid_host(value: &str) -> bool {
    if value.is_empty() || value.len() > MAX_HOST_BYTES {
        return false;
    }
    let port = if let Some(bracketed) = value.strip_prefix('[') {
        let Some((address, remainder)) = bracketed.split_once(']') else {
            return false;
        };
        if address.parse::<std::net::Ipv6Addr>().is_err() {
            return false;
        }
        if remainder.is_empty() {
            return true;
        }
        let Some(port) = remainder.strip_prefix(':') else {
            return false;
        };
        port
    } else {
        let (host, port) = value
            .split_once(':')
            .map_or((value, None), |(h, p)| (h, Some(p)));
        if host.is_empty()
            || !host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-._".contains(&b))
        {
            return false;
        }
        let Some(port) = port else { return true };
        port
    };
    !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) && port.parse::<u16>().is_ok()
}

/// Validate the small application/json media type contract without accepting
/// broken parameter syntax or duplicate names. No input reaches public errors.
pub fn json_content_type(value: Option<&str>) -> bool {
    let Some(value) = value else { return false };
    let mut fields = value.split(';');
    if !fields
        .next()
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("application/json"))
    {
        return false;
    }
    let mut names = Vec::new();
    for parameter in fields {
        let Some((name, value)) = parameter.trim().split_once('=') else {
            return false;
        };
        let name = name.trim();
        let value = value.trim();
        if name.is_empty()
            || !name.bytes().all(token)
            || names
                .iter()
                .any(|prior: &&str| prior.eq_ignore_ascii_case(name))
        {
            return false;
        }
        let valid_value =
            if let Some(quoted) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
                !quoted
                    .bytes()
                    .any(|b| b < 32 || b == 127 || matches!(b, b'"' | b'\\'))
            } else {
                !value.is_empty() && value.bytes().all(token)
            };
        if !valid_value {
            return false;
        }
        names.push(name);
    }
    true
}

/// Percent decode only a bounded target; reject control bytes, invalid UTF-8 and backslashes.
pub(crate) fn decode_target(target: &str) -> Result<String, ()> {
    if target.len() > MAX_TARGET_BYTES {
        return Err(());
    }
    let bytes = target.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let byte = if bytes[index] == b'%' {
            let hi = hex(*bytes.get(index + 1).ok_or(())?).ok_or(())?;
            let lo = hex(*bytes.get(index + 2).ok_or(())?).ok_or(())?;
            index += 3;
            hi * 16 + lo
        } else {
            let byte = bytes[index];
            index += 1;
            byte
        };
        if byte < 32 || byte == 127 || byte == b'\\' {
            return Err(());
        }
        decoded.push(byte);
    }
    let decoded = String::from_utf8(decoded).map_err(|_| ())?;
    if decoded.chars().any(char::is_control) {
        return Err(());
    }
    Ok(decoded)
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub(crate) struct HeaderRead {
    pub length: usize,
    pub used: usize,
}

fn timed_read(
    stream: &mut TcpStream,
    buffer: &mut [u8],
    deadline: Instant,
    head_only: bool,
) -> Result<usize, HttpError> {
    let failure = |kind| HttpError { kind, head_only };
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(|| failure(ErrorKind::Timeout))?;
        stream
            .set_read_timeout(Some(remaining))
            .map_err(|_| failure(ErrorKind::BadRequest))?;
        let count = match stream.read(buffer) {
            Ok(0) => return Err(failure(ErrorKind::BadRequest)),
            Ok(count) => count,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                return Err(failure(ErrorKind::Timeout));
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(failure(ErrorKind::BadRequest)),
        };
        if Instant::now() >= deadline {
            return Err(failure(ErrorKind::Timeout));
        }
        return Ok(count);
    }
}

pub(crate) fn read_headers(
    stream: &mut TcpStream,
    buffer: &mut [u8; MAX_HEADER_BYTES + 1],
    deadline: Instant,
) -> Result<HeaderRead, HttpError> {
    let mut used = 0;
    loop {
        let head_only = buffer.starts_with(b"HEAD ");
        used += timed_read(stream, &mut buffer[used..], deadline, head_only)?;
        if let Some(end) = buffer[..used].windows(4).position(|w| w == b"\r\n\r\n") {
            let length = end + 4;
            if length > MAX_HEADER_BYTES {
                return Err(error(ErrorKind::HeadersTooLarge, &buffer[..used]));
            }
            return Ok(HeaderRead { length, used });
        }
        if used >= MAX_HEADER_BYTES {
            return Err(error(ErrorKind::HeadersTooLarge, &buffer[..used]));
        }
    }
}

/// The same absolute deadline is used for header and body reads. The body
/// allocation occurs only after the parser validates Content-Length and caps.
pub(crate) fn read_body(
    stream: &mut TcpStream,
    prefix: &[u8],
    request: &Request<'_>,
    deadline: Instant,
) -> Result<Vec<u8>, HttpError> {
    let failure = |kind| HttpError {
        kind,
        head_only: request.method == Method::Head,
    };
    if prefix.len() > request.content_length {
        return Err(failure(ErrorKind::BadRequest));
    }
    let mut body = vec![0; request.content_length];
    body[..prefix.len()].copy_from_slice(prefix);
    let mut used = prefix.len();
    while used < body.len() {
        used += timed_read(
            stream,
            &mut body[used..],
            deadline,
            request.method == Method::Head,
        )?;
    }
    if Instant::now() >= deadline {
        return Err(failure(ErrorKind::Timeout));
    }
    Ok(body)
}

pub(crate) struct DeadlineWriter<'a> {
    stream: &'a mut TcpStream,
    deadline: Instant,
}

impl<'a> DeadlineWriter<'a> {
    pub(crate) fn new(stream: &'a mut TcpStream, budget: Duration) -> Self {
        Self {
            stream,
            deadline: Instant::now() + budget,
        }
    }
}

impl Write for DeadlineWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "write deadline"))?;
        self.stream.set_write_timeout(Some(remaining))?;
        self.stream.write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) fn write_headers(
    writer: &mut impl Write,
    status: u16,
    reason: &str,
    content_type: &str,
    length: u64,
) -> io::Result<()> {
    write_headers_extra(writer, status, reason, content_type, length, &[])
}

fn write_headers_extra(
    writer: &mut impl Write,
    status: u16,
    reason: &str,
    content_type: &str,
    length: u64,
    extra: &[(&str, &str)],
) -> io::Result<()> {
    write!(
        writer,
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {length}\r\nContent-Type: {content_type}\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: same-origin\r\nCache-Control: no-store\r\n"
    )?;
    if status == 405 && !extra.iter().any(|(name, _)| *name == "Allow") {
        writer.write_all(b"Allow: GET, HEAD\r\n")?;
    }
    for (name, value) in extra {
        write!(writer, "{name}: {value}\r\n")?;
    }
    writer.write_all(b"\r\n")
}

pub(crate) fn write_response(
    writer: &mut impl Write,
    status: u16,
    reason: &str,
    body: &[u8],
    head_only: bool,
) -> io::Result<()> {
    write_response_extra(
        writer,
        status,
        reason,
        body,
        head_only,
        "application/json",
        &[],
    )
}

pub(crate) fn write_response_extra(
    writer: &mut impl Write,
    status: u16,
    reason: &str,
    body: &[u8],
    head_only: bool,
    content_type: &str,
    extra: &[(&str, &str)],
) -> io::Result<()> {
    write_headers_extra(
        writer,
        status,
        reason,
        content_type,
        body.len() as u64,
        extra,
    )?;
    if !head_only {
        writer.write_all(body)?;
    }
    Ok(())
}

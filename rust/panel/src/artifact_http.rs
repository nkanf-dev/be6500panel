//! Pure bounded HTTP artifact URL and response framing. No socket/TLS/DNS.
//! Read callbacks must cooperate with the same absolute Budget; no hidden worker.
//! Transport sends Connection: close. Even length/chunk completion requires EOF
//! under that deadline: extra bytes are refused, not silently pooled or ignored.
use crate::readiness_tun::{Budget, TunError};
use http::Uri;
use std::{fmt, io::{self, Read}, net::IpAddr};

pub const MAX_URL_BYTES: usize = 4096;
pub const MAX_HEAD_BYTES: usize = 16 << 10;
pub const MAX_HEADERS: usize = 64;
pub const MAX_BODY_BYTES: u64 = 16 << 20;
pub const STREAM_BYTES: usize = 8192;
const MAX_CHUNK_LINE: usize = 1024;
const MAX_CHUNKS: usize = 65536;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpSourceError {
    Url,
    Downgrade,
    Source,
    Head,
    Encoding,
    Framing,
    Truncated,
    Limit,
    Deadline,
    Cancelled,
}
impl fmt::Display for HttpSourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Url => "artifact URL invalid",
            Self::Downgrade => "artifact redirect scheme refused",
            Self::Source => "artifact HTTP source unavailable",
            Self::Head => "artifact HTTP response header invalid",
            Self::Encoding => "artifact HTTP content encoding refused",
            Self::Framing => "artifact HTTP framing invalid",
            Self::Truncated => "artifact HTTP body truncated",
            Self::Limit => "artifact HTTP resource limit exceeded",
            Self::Deadline => "artifact HTTP deadline exceeded",
            Self::Cancelled => "artifact HTTP cancelled",
        })
    }
}
impl std::error::Error for HttpSourceError {}
fn check(b: &Budget<'_>) -> Result<(), HttpSourceError> {
    b.check().map_err(|e| match e {
        TunError::Deadline => HttpSourceError::Deadline,
        TunError::Cancelled => HttpSourceError::Cancelled,
        _ => HttpSourceError::Source,
    })
}

/// Bounded metadata, never a filesystem path or command. HTTP is permitted only
/// for an explicit numeric-loopback test policy; HTTPS redirects cannot downgrade.
#[derive(Clone)]
pub struct Url {
    scheme: String,
    host: String,
    port: u16,
    authority: String,
    path_and_query: String,
    allow_loopback_http: bool,
}
impl fmt::Debug for Url {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ArtifactUrl([private])")
    }
}
fn safe_url(text: &str) -> bool {
    if text.is_empty() || text.len() > MAX_URL_BYTES
        || !text.bytes().all(|b| b > 0x20 && b < 0x7f && b != b'#' && b != b'\\')
    {
        return false;
    }
    let bytes = text.as_bytes();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes[cursor] == b'%' {
            let Some(pair) = bytes.get(cursor + 1..cursor + 3) else { return false; };
            let Ok(pair) = std::str::from_utf8(pair) else { return false; };
            let Ok(byte) = u8::from_str_radix(pair, 16) else { return false; };
            if byte <= 0x20 || byte == 0x7f { return false; }
            cursor += 3;
        } else { cursor += 1; }
    }
    true
}
impl Url {
    pub fn parse(text: &str, allow_loopback_http: bool) -> Result<Self, HttpSourceError> {
        if !safe_url(text) { return Err(HttpSourceError::Url); }
        let uri: Uri = text.parse().map_err(|_| HttpSourceError::Url)?;
        let scheme = uri.scheme_str().ok_or(HttpSourceError::Url)?;
        if !matches!(scheme, "https" | "http") { return Err(HttpSourceError::Url); }
        let authority = uri.authority().ok_or(HttpSourceError::Url)?;
        if authority.as_str().contains(['@', '%']) { return Err(HttpSourceError::Url); }
        let host = authority.host();
        let host = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')).unwrap_or(host);
        if host.is_empty() { return Err(HttpSourceError::Url); }
        let literal = host.parse::<IpAddr>().ok();
        if let Some(IpAddr::V6(ip)) = literal {
            if ip.to_ipv4_mapped().is_some() { return Err(HttpSourceError::Url); }
        } else if literal.is_none()
            && (host.len() > 253 || host.split('.').any(|label| {
                label.is_empty() || label.len() > 63 || label.starts_with('-') || label.ends_with('-')
                    || !label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            }))
        {
            return Err(HttpSourceError::Url);
        }
        // A syntactically present invalid port must never silently become 443.
        let authority_text = authority.as_str();
        let explicit_port = if authority_text.starts_with('[') {
            let close = authority_text.find(']').ok_or(HttpSourceError::Url)?;
            let suffix = &authority_text[close + 1..];
            if suffix.is_empty() { None } else {
                Some(suffix.strip_prefix(':').ok_or(HttpSourceError::Url)?)
            }
        } else {
            authority_text.split_once(':').map(|(_, port)| port)
        };
        let port = match explicit_port {
            Some(port) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) =>
                port.parse::<u16>().map_err(|_| HttpSourceError::Url)?,
            Some(_) => return Err(HttpSourceError::Url),
            None => if scheme == "https" { 443 } else { 80 },
        };
        if port == 0 { return Err(HttpSourceError::Url); }
        if scheme == "http" && (!allow_loopback_http || !literal.is_some_and(|ip| ip.is_loopback())) {
            return Err(HttpSourceError::Url);
        }
        let path = uri.path_and_query().map(|p| p.as_str()).unwrap_or("/");
        if !path.starts_with('/') { return Err(HttpSourceError::Url); }
        Ok(Self {
            scheme: scheme.into(), host: host.to_ascii_lowercase(), port,
            authority: authority.as_str().into(), path_and_query: path.into(), allow_loopback_http,
        })
    }
    pub fn scheme(&self) -> &str { &self.scheme }
    pub fn host(&self) -> &str { &self.host }
    pub fn port(&self) -> u16 { self.port }
    pub fn authority(&self) -> &str { &self.authority }
    pub fn path_and_query(&self) -> &str { &self.path_and_query }
    pub fn resolve_location(&self, location: &str) -> Result<Self, HttpSourceError> {
        if !safe_url(location) { return Err(HttpSourceError::Url); }
        let absolute = if location.starts_with("//") {
            format!("{}:{location}", self.scheme)
        } else if location.split(['/', '?']).next().is_some_and(|p| p.contains(':')) {
            location.to_owned()
        } else {
            let base_path = self.path_and_query.split('?').next().ok_or(HttpSourceError::Url)?;
            let combined = if location.starts_with('?') {
                format!("{base_path}{location}")
            } else if location.starts_with('/') {
                location.to_owned()
            } else {
                let directory = base_path.rsplit_once('/').map_or("/", |(p, _)| p);
                format!("{directory}/{location}")
            };
            let path = normalize_path(&combined);
            format!("{}://{}{path}", self.scheme, self.authority)
        };
        let next = Self::parse(&absolute, self.allow_loopback_http)?;
        if self.scheme == "https" && next.scheme != "https" { return Err(HttpSourceError::Downgrade); }
        Ok(next)
    }
}
fn normalize_path(text: &str) -> String {
    let (path, query) = text.split_once('?').map_or((text, None), |(p, q)| (p, Some(q)));
    let mut parts = Vec::new();
    for part in path.split('/').skip(1) {
        match part {
            "." => {},
            ".." => { parts.pop(); },
            _ => parts.push(part),
        }
    }
    let mut out = format!("/{}", parts.join("/"));
    if (path.ends_with("/.") || path.ends_with("/..")) && !out.ends_with('/') { out.push('/'); }
    if let Some(q) = query { out.push('?'); out.push_str(q); }
    out
}

// One fixed prefetch buffer; head parsing cannot discard already read body bytes.
struct Wire<'a, R> {
    reader: R,
    budget: Budget<'a>,
    bytes: [u8; STREAM_BYTES],
    start: usize,
    end: usize,
    eof: bool,
}
impl<R: Read> Wire<'_, R> {
    fn fill(&mut self) -> Result<(), HttpSourceError> {
        check(&self.budget)?;
        if self.start < self.end || self.eof { return Ok(()); }
        loop {
            check(&self.budget)?;
            let result = self.reader.read(&mut self.bytes);
            check(&self.budget)?;
            match result {
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(HttpSourceError::Source),
                Ok(n) if n > self.bytes.len() => return Err(HttpSourceError::Source),
                Ok(n) => { self.start = 0; self.end = n; self.eof = n == 0; return Ok(()); }
            }
        }
    }
    fn byte(&mut self) -> Result<Option<u8>, HttpSourceError> {
        self.fill()?;
        if self.start == self.end { return Ok(None); }
        let byte = self.bytes[self.start];
        self.start += 1;
        Ok(Some(byte))
    }
    fn read(&mut self, into: &mut [u8]) -> Result<usize, HttpSourceError> {
        check(&self.budget)?;
        if into.is_empty() { return Ok(0); }
        self.fill()?;
        let n = into.len().min(self.end - self.start).min(STREAM_BYTES);
        into[..n].copy_from_slice(&self.bytes[self.start..self.start + n]);
        self.start += n;
        check(&self.budget)?;
        Ok(n)
    }
    fn line(&mut self, limit: usize) -> Result<Vec<u8>, HttpSourceError> {
        let mut line = Vec::new();
        loop {
            let byte = self.byte()?.ok_or(HttpSourceError::Truncated)?;
            if line.len() == limit { return Err(HttpSourceError::Limit); }
            if line.len() == line.capacity() {
                let capacity = line.capacity().saturating_mul(2).max(64).min(limit);
                line.try_reserve_exact(capacity - line.len()).map_err(|_| HttpSourceError::Limit)?;
            }
            line.push(byte);
            if byte == b'\n' {
                if !line.ends_with(b"\r\n") { return Err(HttpSourceError::Framing); }
                line.truncate(line.len() - 2);
                return Ok(line);
            }
        }
    }
    fn terminal(&mut self) -> Result<(), HttpSourceError> {
        if self.byte()?.is_some() { Err(HttpSourceError::Framing) } else { Ok(()) }
    }
}

enum Framing {
    Length(u64),
    Chunked { remaining: u64, chunks: usize, delimiter: bool },
    Close,
    Done,
}
pub struct Body<'a, R> {
    wire: Wire<'a, R>,
    framing: Framing,
    count: u64,
    failed: Option<HttpSourceError>,
}
impl<R> fmt::Debug for Body<'_, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str("ArtifactHttpBody([private])") }
}
pub struct Response<'a, R> {
    status: u16,
    location: Option<String>,
    body: Body<'a, R>,
}
impl<R> fmt::Debug for Response<'_, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str("ArtifactHttpResponse([private])") }
}
impl<'a, R> Response<'a, R> {
    pub fn status(&self) -> u16 { self.status }
    pub fn location(&self) -> Option<&str> { self.location.as_deref() }
    pub fn body_mut(&mut self) -> &mut Body<'a, R> { &mut self.body }
    pub fn into_body(self) -> Body<'a, R> { self.body }
}

pub fn parse_response<'a, R: Read>(reader: R, budget: &Budget<'a>) -> Result<Response<'a, R>, HttpSourceError> {
    check(budget)?;
    let mut wire = Wire {
        reader,
        budget: Budget { deadline: budget.deadline, cancel: budget.cancel },
        bytes: [0; STREAM_BYTES], start: 0, end: 0, eof: false,
    };
    let mut head = Vec::new();
    loop {
        let byte = wire.byte()?.ok_or(HttpSourceError::Truncated)?;
        if head.len() == MAX_HEAD_BYTES { return Err(HttpSourceError::Limit); }
        if head.len() == head.capacity() {
            let capacity = head.capacity().saturating_mul(2).max(1024).min(MAX_HEAD_BYTES);
            head.try_reserve_exact(capacity - head.len()).map_err(|_| HttpSourceError::Limit)?;
        }
        head.push(byte);
        if head.ends_with(b"\r\n\r\n") { break; }
    }
    // httparse accepts bare LF: tighten line boundaries before header parsing.
    for (i, byte) in head.iter().enumerate() {
        if (*byte == b'\n' && (i == 0 || head[i - 1] != b'\r'))
            || (*byte == b'\r' && head.get(i + 1) != Some(&b'\n'))
        { return Err(HttpSourceError::Head); }
    }
    let mut headers = [httparse::EMPTY_HEADER; MAX_HEADERS];
    let mut response = httparse::Response::new(&mut headers);
    match response.parse(&head).map_err(|_| HttpSourceError::Head)? {
        httparse::Status::Complete(length) if length == head.len() => {},
        _ => return Err(HttpSourceError::Head),
    }
    if !matches!(response.version, Some(0 | 1)) { return Err(HttpSourceError::Head); }
    let status = response.code.ok_or(HttpSourceError::Head)?;
    if !(100..=599).contains(&status) { return Err(HttpSourceError::Head); }
    let (mut length, mut transfer, mut encoding, mut location) = (None, false, false, None);
    for header in response.headers.iter() {
        let value = std::str::from_utf8(header.value).map_err(|_| HttpSourceError::Head)?;
        if value.bytes().any(|b| b < 0x20 && b != b'\t' || b == 0x7f) { return Err(HttpSourceError::Head); }
        let value = value.trim_matches([' ', '\t']);
        if header.name.eq_ignore_ascii_case("content-length") {
            if length.is_some() || value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                return Err(HttpSourceError::Framing);
            }
            length = Some(value.parse::<u64>().map_err(|_| HttpSourceError::Framing)?);
        } else if header.name.eq_ignore_ascii_case("transfer-encoding") {
            if transfer || !value.eq_ignore_ascii_case("chunked") { return Err(HttpSourceError::Framing); }
            transfer = true;
        } else if header.name.eq_ignore_ascii_case("content-encoding") {
            if encoding || !value.eq_ignore_ascii_case("identity") { return Err(HttpSourceError::Encoding); }
            encoding = true;
        } else if header.name.eq_ignore_ascii_case("location") {
            if location.is_some() || !safe_url(value) { return Err(HttpSourceError::Url); }
            location = Some(value.to_owned());
        }
    }
    if transfer && length.is_some() { return Err(HttpSourceError::Framing); }
    if length.is_some_and(|n| n > MAX_BODY_BYTES) { return Err(HttpSourceError::Limit); }
    let framing = if status < 200 || status == 204 || status == 304 {
        if transfer || length.is_some_and(|n| n != 0) { return Err(HttpSourceError::Framing); }
        Framing::Length(0)
    } else if transfer {
        Framing::Chunked { remaining: 0, chunks: 0, delimiter: false }
    } else if let Some(n) = length {
        Framing::Length(n)
    } else { Framing::Close };
    check(budget)?;
    Ok(Response { status, location, body: Body { wire, framing, count: 0, failed: None } })
}

fn token(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}
fn chunk_size(line: &[u8]) -> Result<u64, HttpSourceError> {
    let size_end = line.iter().position(|b| *b == b';').unwrap_or(line.len());
    let size = &line[..size_end];
    if size.is_empty() || size.len() > 16 || !size.iter().all(u8::is_ascii_hexdigit) {
        return Err(HttpSourceError::Framing);
    }
    let size = u64::from_str_radix(std::str::from_utf8(size).map_err(|_| HttpSourceError::Framing)?, 16)
        .map_err(|_| HttpSourceError::Framing)?;
    let mut cursor = size_end;
    while cursor < line.len() {
        if line[cursor] != b';' { return Err(HttpSourceError::Framing); }
        cursor += 1;
        let start = cursor;
        while cursor < line.len() && token(line[cursor]) { cursor += 1; }
        if start == cursor { return Err(HttpSourceError::Framing); }
        if line.get(cursor) == Some(&b'=') {
            cursor += 1;
            if line.get(cursor) == Some(&b'"') {
                cursor += 1;
                loop {
                    match line.get(cursor).copied() {
                        Some(b'"') => { cursor += 1; break; },
                        Some(b'\\') => {
                            cursor += 1;
                            if !line.get(cursor).is_some_and(|b| (0x20..=0x7e).contains(b)) { return Err(HttpSourceError::Framing); }
                            cursor += 1;
                        },
                        Some(b) if (0x20..=0x7e).contains(&b) => cursor += 1,
                        _ => return Err(HttpSourceError::Framing),
                    }
                }
            } else {
                let start = cursor;
                while cursor < line.len() && token(line[cursor]) { cursor += 1; }
                if start == cursor { return Err(HttpSourceError::Framing); }
            }
        }
    }
    Ok(size)
}
fn trailers<R: Read>(wire: &mut Wire<'_, R>) -> Result<(), HttpSourceError> {
    let mut total = 0usize;
    let mut count = 0usize;
    loop {
        let line = wire.line(MAX_HEAD_BYTES - total)?;
        total = total.checked_add(line.len() + 2).ok_or(HttpSourceError::Limit)?;
        if line.is_empty() { return wire.terminal(); }
        count += 1;
        if count > MAX_HEADERS || total >= MAX_HEAD_BYTES { return Err(HttpSourceError::Limit); }
        let colon = line.iter().position(|b| *b == b':').ok_or(HttpSourceError::Framing)?;
        if colon == 0 || !line[..colon].iter().copied().all(token)
            || line[colon + 1..].iter().any(|b| *b < 0x20 && *b != b'\t' || *b == 0x7f)
        { return Err(HttpSourceError::Framing); }
        // Artifacts have no negotiated trailer metadata. Reject security/framing
        // trailer fields rather than allowing them to change the header contract.
        let name = std::str::from_utf8(&line[..colon]).map_err(|_| HttpSourceError::Framing)?;
        if ["content-length", "transfer-encoding", "content-encoding", "location", "host", "trailer"]
            .iter().any(|n| name.eq_ignore_ascii_case(n)) { return Err(HttpSourceError::Framing); }
    }
}
impl<R: Read> Body<'_, R> {
    fn read_body(&mut self, into: &mut [u8]) -> Result<usize, HttpSourceError> {
        check(&self.wire.budget)?;
        if into.is_empty() { return Ok(0); }
        loop {
            match &mut self.framing {
                Framing::Done => return Ok(0),
                Framing::Length(remaining) => {
                    if *remaining == 0 {
                        self.wire.terminal()?;
                        self.framing = Framing::Done;
                        return Ok(0);
                    }
                    let size = into.len().min(STREAM_BYTES).min(*remaining as usize);
                    let n = self.wire.read(&mut into[..size])?;
                    if n == 0 { return Err(HttpSourceError::Truncated); }
                    *remaining -= n as u64;
                    self.count += n as u64;
                    return Ok(n);
                },
                Framing::Close => {
                    let size = into.len().min(STREAM_BYTES).min((MAX_BODY_BYTES - self.count).max(1) as usize);
                    let n = self.wire.read(&mut into[..size])?;
                    if n as u64 > MAX_BODY_BYTES - self.count { return Err(HttpSourceError::Limit); }
                    self.count += n as u64;
                    if n == 0 { self.framing = Framing::Done; }
                    return Ok(n);
                },
                Framing::Chunked { remaining, chunks, delimiter } => {
                    if *remaining == 0 {
                        if *delimiter {
                            if self.wire.byte()? != Some(b'\r') || self.wire.byte()? != Some(b'\n') {
                                return Err(HttpSourceError::Framing);
                            }
                            *delimiter = false;
                        }
                        let line = self.wire.line(MAX_CHUNK_LINE)?;
                        let size = chunk_size(&line)?;
                        if size == 0 {
                            trailers(&mut self.wire)?;
                            self.framing = Framing::Done;
                            return Ok(0);
                        }
                        *chunks += 1;
                        if *chunks > MAX_CHUNKS || size > MAX_BODY_BYTES - self.count {
                            return Err(HttpSourceError::Limit);
                        }
                        *remaining = size;
                    }
                    let size = into.len().min(STREAM_BYTES).min(*remaining as usize);
                    let n = self.wire.read(&mut into[..size])?;
                    if n == 0 { return Err(HttpSourceError::Truncated); }
                    *remaining -= n as u64;
                    self.count += n as u64;
                    *delimiter = *remaining == 0;
                    return Ok(n);
                },
            }
        }
    }
}
impl<R: Read> Read for Body<'_, R> {
    fn read(&mut self, into: &mut [u8]) -> io::Result<usize> {
        if let Some(error) = self.failed { return Err(io::Error::other(error)); }
        match self.read_body(into) {
            Ok(n) => Ok(n),
            Err(error) => { self.failed = Some(error); Err(io::Error::other(error)) },
        }
    }
}

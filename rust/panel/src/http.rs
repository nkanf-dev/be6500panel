//! Deliberately small HTTP/1.x subset: one GET/HEAD request per connection.
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

pub const MAX_HEADER_BYTES: usize = 16 * 1024;
pub const MAX_HEADERS: usize = 64;
pub const MAX_TARGET_BYTES: usize = 2048;
pub const REQUEST_DEADLINE: Duration = Duration::from_secs(5);
pub const WRITE_DEADLINE: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Head,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Request<'a> {
    pub method: Method,
    pub target: &'a str,
}

impl Request<'_> {
    pub fn path(&self) -> &str {
        self.target
            .split_once('?')
            .map_or(self.target, |(path, _)| path)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    BadRequest,
    MethodNotAllowed,
    Timeout,
    HeadersTooLarge,
    TargetTooLong,
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
    let mut hosts = 0;
    let mut content_length = false;
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
            hosts += 1;
            if hosts != 1
                || value.is_empty()
                || value.bytes().any(|b| b <= 32 || b"/\\@,?#".contains(&b))
            {
                return Err(bad());
            }
        } else if name.eq_ignore_ascii_case("content-length") {
            if content_length
                || value.is_empty()
                || !value.bytes().all(|b| b.is_ascii_digit())
                || value.parse::<u64>().ok() != Some(0)
            {
                return Err(bad());
            }
            content_length = true;
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err(bad());
        }
    }
    if version == "HTTP/1.1" && hosts != 1 {
        return Err(bad());
    }
    let method = match method_text {
        "GET" => Method::Get,
        "HEAD" => Method::Head,
        _ if !method_text.is_empty() && method_text.bytes().all(token) => {
            return Err(error(ErrorKind::MethodNotAllowed, bytes));
        }
        _ => return Err(bad()),
    };
    Ok(Request { method, target })
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

pub(crate) fn read_headers(
    stream: &mut TcpStream,
    buffer: &mut [u8; MAX_HEADER_BYTES + 1],
    budget: Duration,
) -> Result<usize, HttpError> {
    let deadline = Instant::now() + budget;
    let mut used = 0;
    loop {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            return Err(error(ErrorKind::Timeout, &buffer[..used]));
        };
        if remaining.is_zero() {
            return Err(error(ErrorKind::Timeout, &buffer[..used]));
        }
        stream
            .set_read_timeout(Some(remaining))
            .map_err(|_| error(ErrorKind::BadRequest, &buffer[..used]))?;
        match stream.read(&mut buffer[used..]) {
            Ok(0) => return Err(error(ErrorKind::BadRequest, &buffer[..used])),
            Ok(count) => used += count,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                return Err(error(ErrorKind::Timeout, &buffer[..used]));
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(error(ErrorKind::BadRequest, &buffer[..used])),
        }
        if Instant::now() >= deadline {
            return Err(error(ErrorKind::Timeout, &buffer[..used]));
        }
        if used > MAX_HEADER_BYTES {
            return Err(error(ErrorKind::HeadersTooLarge, &buffer[..used]));
        }
        if buffer[..used]
            .windows(4)
            .any(|window| window == b"\r\n\r\n")
        {
            return Ok(used);
        }
        if used == MAX_HEADER_BYTES {
            return Err(error(ErrorKind::HeadersTooLarge, &buffer[..used]));
        }
    }
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
    write!(
        writer,
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {length}\r\nContent-Type: {content_type}\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\nCache-Control: no-store\r\n"
    )?;
    if status == 405 {
        writer.write_all(b"Allow: GET, HEAD\r\n")?;
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
    write_headers(
        writer,
        status,
        reason,
        "application/json",
        body.len() as u64,
    )?;
    if !head_only {
        writer.write_all(body)?;
    }
    Ok(())
}

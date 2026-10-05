//! Certificate-verified, caller-driven artifact fetch. One absolute deadline,
//! one socket, bounded framing and direct literal-bootstrap DNS. No resolver
//! thread, environment proxy, HTTP decompression, arbitrary path or activation.
use crate::{
    artifact_http::{self, Url},
    artifact_stage::{Stage, StageError},
    endpoint_dns, readiness_dns,
    readiness_tun::{Budget, TunError},
    runtime_store::Artifact,
};
use std::{
    cell::Cell,
    fmt,
    io::{self, Read, Write},
    net::{SocketAddr, TcpStream},
    os::fd::AsRawFd,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
pub const MAX_FETCH_TIME: Duration = Duration::from_secs(90);
const MAX_REDIRECTS: usize = 10;
const IO_SLICE: Duration = Duration::from_millis(50);
const HANDSHAKE_BYTES: usize = 256 << 10;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceError {
    Input,
    Dns,
    Connect,
    Tls,
    Http,
    Status,
    Redirect,
    Limit,
    Deadline,
    Cancelled,
    Stage(StageError),
}
impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Input => "artifact source input invalid",
            Self::Dns => "artifact source DNS unavailable",
            Self::Connect => "artifact source connection unavailable",
            Self::Tls => "artifact TLS validation or transport failed",
            Self::Http => "artifact HTTP response invalid",
            Self::Status => "artifact HTTP status refused",
            Self::Redirect => "artifact redirect refused",
            Self::Limit => "private HTTPS source size limit exceeded",
            Self::Deadline => "artifact source deadline exceeded",
            Self::Cancelled => "artifact source cancelled",
            Self::Stage(_) => "artifact staging refused",
        })
    }
}
impl std::error::Error for SourceError {}
fn check(budget: &Budget<'_>) -> Result<(), SourceError> {
    if crate::shutdown::requested() {
        return Err(SourceError::Cancelled);
    }
    budget.check().map_err(|error| match error {
        TunError::Cancelled => SourceError::Cancelled,
        _ => SourceError::Deadline,
    })
}
fn io_error(error: SourceError) -> io::Error {
    io::Error::other(error)
}
fn transport_error(budget: &Budget<'_>, error: SourceError) -> SourceError {
    check(budget).err().unwrap_or(error)
}
/// Trusted startup policy, not browser supplied roots/addresses. HTTPS is the
/// native path. Plain HTTP may be allowed only for explicit numeric-loopback
/// fixtures; a redirect from HTTPS still cannot downgrade.
pub struct SourcePolicy {
    bootstrap: SocketAddr,
    tls: Arc<rustls::ClientConfig>,
    allow_loopback_http: bool,
}
impl fmt::Debug for SourcePolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ArtifactSourcePolicy([private])")
    }
}
fn tls_config(roots: rustls::RootCertStore) -> Result<Arc<rustls::ClientConfig>, SourceError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|_| SourceError::Tls)?
        .with_root_certificates(roots)
        .with_no_client_auth();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    config.max_fragment_size = Some(8192);
    config.resumption = rustls::client::Resumption::disabled();
    Ok(Arc::new(config))
}
impl SourcePolicy {
    pub fn native(bootstrap: SocketAddr) -> Result<Self, SourceError> {
        if bootstrap.port() == 0
            || bootstrap.ip().is_unspecified()
            || bootstrap.ip().is_multicast()
            || matches!(bootstrap,SocketAddr::V6(a)if a.scope_id()!=0||a.ip().to_ipv4_mapped().is_some())
        {
            return Err(SourceError::Input);
        }
        let roots = rustls::RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        Ok(Self {
            bootstrap,
            tls: tls_config(roots)?,
            allow_loopback_http: false,
        })
    }
    /// Explicit host-fixture option, never enabled by native initialization.
    pub fn loopback_fixture(bootstrap: SocketAddr) -> Result<Self, SourceError> {
        let mut policy = Self::native(bootstrap)?;
        policy.allow_loopback_http = true;
        Ok(policy)
    }
    pub(crate) fn validate_artifact(&self, artifact: &Artifact) -> Result<(), SourceError> {
        crate::artifact_stage::metadata(artifact).map_err(|_| SourceError::Input)?;
        Url::parse(&artifact.url, self.allow_loopback_http).map_err(|_| SourceError::Input)?;
        Ok(())
    }
    pub fn fetch(
        &self,
        root: &Path,
        artifact: &Artifact,
        budget: &Budget<'_>,
    ) -> Result<Stage, SourceError> {
        check(budget)?;
        crate::artifact_stage::metadata(artifact).map_err(|_| SourceError::Input)?;
        self.consume_body(&artifact.url, budget, MAX_REDIRECTS, |reader, budget| {
            Stage::from_reader(root, artifact, reader, budget).map_err(SourceError::Stage)
        })
    }
    pub(crate) fn fetch_subscription(
        &self,
        source: &str,
        budget: &Budget<'_>,
    ) -> Result<Vec<u8>, SourceError> {
        let limited = Budget {
            deadline: budget
                .deadline
                .min(Instant::now() + Duration::from_secs(45)),
            cancel: budget.cancel,
        };
        self.consume_body(source, &limited, 3, |reader, budget| {
            read_source_bytes(reader, 2 << 20, budget)
        })
    }
    fn consume_body<T>(
        &self,
        source: &str,
        budget: &Budget<'_>,
        redirect_limit: usize,
        consume: impl FnOnce(&mut dyn Read, &Budget<'_>) -> Result<T, SourceError>,
    ) -> Result<T, SourceError> {
        check(budget)?;
        if redirect_limit > MAX_REDIRECTS {
            return Err(SourceError::Input);
        }
        let mut consume = Some(consume);
        let limited = Budget {
            deadline: budget.deadline.min(Instant::now() + MAX_FETCH_TIME),
            cancel: budget.cancel,
        };
        let mut url =
            Url::parse(source, self.allow_loopback_http).map_err(|_| SourceError::Input)?;
        for redirects in 0..=redirect_limit {
            check(&limited)?;
            let mut transport = self.connect(&url, &limited)?;
            let request = format!(
                "GET {} HTTP/1.1\r\nHost: {}\r\nAccept: application/octet-stream\r\nAccept-Encoding: identity\r\nConnection: close\r\nUser-Agent: be6500panel-rust\r\n\r\n",
                url.path_and_query(),
                url.authority()
            );
            transport.write_request(request.as_bytes())?;
            // Observe fixed transport errors below HTTP framing, before its
            // private-safe generic source error projection replaces the class.
            let observed_error = Cell::new(None);
            let observed = ObservedReader {
                inner: transport,
                error: &observed_error,
            };
            let response = artifact_http::parse_response(observed, &limited).map_err(|_| {
                transport_error(&limited, observed_error.get().unwrap_or(SourceError::Http))
            })?;
            let status = response.status();
            if matches!(status, 301 | 302 | 303 | 307 | 308) {
                if redirects == redirect_limit {
                    return Err(SourceError::Redirect);
                }
                let next = url
                    .resolve_location(response.location().ok_or(SourceError::Redirect)?)
                    .map_err(|_| SourceError::Redirect)?;
                drop(response);
                url = next;
                continue;
            }
            if status != 200 {
                return Err(SourceError::Status);
            }
            let mut reader = ObservedReader {
                inner: response.into_body(),
                error: &observed_error,
            };
            let result = consume.take().ok_or(SourceError::Input)?(&mut reader, &limited).map_err(
                |error| transport_error(&limited, observed_error.get().unwrap_or(error)),
            )?;
            check(&limited)?;
            return Ok(result);
        }
        Err(SourceError::Redirect)
    }
    fn connect<'a>(&self, url: &Url, budget: &Budget<'a>) -> Result<Transport<'a>, SourceError> {
        check(budget)?;
        let addresses = if let Ok(address) = url.host().parse::<std::net::IpAddr>() {
            vec![address]
        } else {
            endpoint_dns::resolve(
                url.host(),
                self.bootstrap,
                budget.deadline,
                Some(budget.cancel),
            )
            .map_err(|_| transport_error(budget, SourceError::Dns))?
        };
        if addresses.is_empty() || addresses.len() > 128 {
            return Err(SourceError::Dns);
        }
        let mut connected = None;
        for address in addresses {
            check(budget)?;
            match readiness_dns::connect_literal(
                SocketAddr::new(address, url.port()),
                budget.deadline,
                Some(budget.cancel),
            ) {
                Ok(stream) => {
                    connected = Some(stream);
                    break;
                }
                Err(_) => check(budget)?,
            }
        }
        let socket = connected.ok_or(SourceError::Connect)?;
        let mut tls = if url.scheme() == "https" {
            let name = rustls::pki_types::ServerName::try_from(url.host().to_owned())
                .map_err(|_| SourceError::Input)?;
            let mut connection = rustls::ClientConnection::new(self.tls.clone(), name)
                .map_err(|_| SourceError::Tls)?;
            connection.set_buffer_limit(Some(16 << 10));
            Some(connection)
        } else {
            None
        };
        let mut transport = Transport {
            socket,
            tls: tls.take(),
            budget: Budget {
                deadline: budget.deadline,
                cancel: budget.cancel,
            },
            handshake_bytes: 0,
        };
        transport.handshake()?;
        Ok(transport)
    }
}
fn read_source_bytes(
    reader: &mut dyn Read,
    limit: usize,
    budget: &Budget<'_>,
) -> Result<Vec<u8>, SourceError> {
    let mut bytes = Vec::with_capacity(8192.min(limit));
    let mut chunk = [0u8; 8192];
    loop {
        check(budget)?;
        let size = chunk.len().min(limit.saturating_sub(bytes.len()) + 1);
        let result = reader.read(&mut chunk[..size]);
        check(budget)?;
        let count = match result {
            Ok(count) => count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(SourceError::Http),
        };
        if count == 0 {
            return Ok(bytes);
        }
        if count > size || count > limit.saturating_sub(bytes.len()) {
            return Err(SourceError::Limit);
        }
        let needed = bytes.len() + count;
        if needed > bytes.capacity() {
            let capacity = needed.max(bytes.capacity().saturating_mul(2)).min(limit);
            bytes
                .try_reserve_exact(capacity - bytes.len())
                .map_err(|_| SourceError::Limit)?;
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
}
struct ObservedReader<'a, R> {
    inner: R,
    error: &'a Cell<Option<SourceError>>,
}
impl<R: Read> Read for ObservedReader<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self.inner.read(buffer) {
            Ok(n) => Ok(n),
            Err(error) => {
                let class = error
                    .get_ref()
                    .and_then(|e| e.downcast_ref::<SourceError>())
                    .copied()
                    .unwrap_or(SourceError::Http);
                if self.error.get().is_none() {
                    self.error.set(Some(class));
                }
                Err(error)
            }
        }
    }
}
struct Transport<'a> {
    socket: TcpStream,
    tls: Option<rustls::ClientConnection>,
    budget: Budget<'a>,
    handshake_bytes: usize,
}
impl fmt::Debug for Transport<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ArtifactTransport([private])")
    }
}
fn transient(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
    )
}
impl Transport<'_> {
    fn wait(&self, events: libc::c_short) -> Result<(), SourceError> {
        loop {
            check(&self.budget)?;
            let millis = self
                .budget
                .deadline
                .saturating_duration_since(Instant::now())
                .min(IO_SLICE)
                .as_millis()
                .max(1) as libc::c_int;
            let mut poll = libc::pollfd {
                fd: self.socket.as_raw_fd(),
                events,
                revents: 0,
            };
            let result = unsafe { libc::poll(&mut poll, 1, millis) };
            check(&self.budget)?;
            if result < 0 {
                if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(SourceError::Connect);
            }
            if result == 0 {
                continue;
            }
            if poll.revents & (libc::POLLERR | libc::POLLNVAL) != 0 {
                return Err(SourceError::Connect);
            }
            if poll.revents & (events | libc::POLLHUP) != 0 {
                return Ok(());
            }
        }
    }
    fn flush_tls(&mut self) -> Result<(), SourceError> {
        while self.tls.as_ref().is_some_and(|tls| tls.wants_write()) {
            check(&self.budget)?;
            let result = self
                .tls
                .as_mut()
                .ok_or(SourceError::Tls)?
                .write_tls(&mut self.socket);
            match result {
                Ok(0) => return Err(SourceError::Tls),
                Ok(_) => {}
                Err(error) if transient(&error) => self.wait(libc::POLLOUT)?,
                Err(_) => return Err(SourceError::Tls),
            }
        }
        check(&self.budget)
    }
    fn read_tls(&mut self) -> Result<(), SourceError> {
        loop {
            check(&self.budget)?;
            let mut limited = (&mut self.socket).take(8192);
            let result = self
                .tls
                .as_mut()
                .ok_or(SourceError::Tls)?
                .read_tls(&mut limited);
            match result {
                Ok(count) => {
                    if count == 0 {
                        return Err(SourceError::Tls);
                    }
                    if self.tls.as_ref().is_some_and(|tls| tls.is_handshaking()) {
                        self.handshake_bytes = self
                            .handshake_bytes
                            .checked_add(count)
                            .ok_or(SourceError::Tls)?;
                        if self.handshake_bytes > HANDSHAKE_BYTES {
                            return Err(SourceError::Tls);
                        }
                    }
                    self.tls
                        .as_mut()
                        .ok_or(SourceError::Tls)?
                        .process_new_packets()
                        .map_err(|_| SourceError::Tls)?;
                    check(&self.budget)?;
                    return Ok(());
                }
                Err(error) if transient(&error) => self.wait(libc::POLLIN)?,
                Err(_) => return Err(SourceError::Tls),
            }
        }
    }
    fn handshake(&mut self) -> Result<(), SourceError> {
        while self.tls.as_ref().is_some_and(|tls| tls.is_handshaking()) {
            self.flush_tls()?;
            if self.tls.as_ref().is_some_and(|tls| tls.is_handshaking()) {
                self.read_tls()?;
            }
        }
        self.flush_tls()?;
        if self
            .tls
            .as_ref()
            .and_then(|tls| tls.alpn_protocol())
            .is_some_and(|alpn| alpn != b"http/1.1")
        {
            return Err(SourceError::Tls);
        }
        check(&self.budget)
    }
    fn write_request(&mut self, mut request: &[u8]) -> Result<(), SourceError> {
        while !request.is_empty() {
            check(&self.budget)?;
            let result = if let Some(tls) = &mut self.tls {
                tls.writer().write(request)
            } else {
                self.socket.write(request)
            };
            match result {
                Ok(0) => return Err(SourceError::Connect),
                Ok(count) => {
                    request = &request[count..];
                    self.flush_tls()?;
                }
                Err(error) if transient(&error) => self.wait(libc::POLLOUT)?,
                Err(_) => return Err(SourceError::Connect),
            }
        }
        self.flush_tls()?;
        check(&self.budget)
    }
}
impl Read for Transport<'_> {
    fn read(&mut self, into: &mut [u8]) -> io::Result<usize> {
        check(&self.budget).map_err(io_error)?;
        if into.is_empty() {
            return Ok(0);
        }
        let limit = into.len().min(8192);
        loop {
            if let Some(tls) = &mut self.tls {
                match tls.reader().read(&mut into[..limit]) {
                    Ok(count) => {
                        check(&self.budget).map_err(io_error)?;
                        return Ok(count);
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                    Err(_) => return Err(io_error(SourceError::Tls)),
                }
                self.flush_tls().map_err(io_error)?;
                self.read_tls().map_err(io_error)?;
            } else {
                match self.socket.read(&mut into[..limit]) {
                    Ok(count) => {
                        check(&self.budget).map_err(io_error)?;
                        return Ok(count);
                    }
                    Err(error) if transient(&error) => self.wait(libc::POLLIN).map_err(io_error)?,
                    Err(_) => return Err(io_error(SourceError::Connect)),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::{
        fs,
        net::TcpListener,
        os::unix::fs::DirBuilderExt,
        path::PathBuf,
        sync::atomic::{AtomicBool, AtomicU64, Ordering},
        thread,
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let root = fs::canonicalize(std::env::temp_dir())
                .unwrap()
                .join(format!(
                    "b6p-https-source-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
            fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn artifact(url: String, raw: &[u8]) -> Artifact {
        Artifact {
            url,
            sha256: format!("{:x}", Sha256::digest(raw)),
            compression: "none".into(),
            version: "tls-fixture".into(),
        }
    }
    fn policy() -> SourcePolicy {
        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(rustls::pki_types::CertificateDer::from(
                include_bytes!("../tests/fixtures/artifact-tls/ca.der").to_vec(),
            ))
            .unwrap();
        SourcePolicy {
            bootstrap: "127.0.0.1:53".parse().unwrap(),
            tls: tls_config(roots).unwrap(),
            allow_loopback_http: true,
        }
    }
    fn config() -> Arc<rustls::ServerConfig> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let key =
            rustls::pki_types::PrivateKeyDer::Pkcs8(rustls::pki_types::PrivatePkcs8KeyDer::from(
                include_bytes!("../tests/fixtures/artifact-tls/leaf-key.der").to_vec(),
            ));
        let mut config = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(
                vec![rustls::pki_types::CertificateDer::from(
                    include_bytes!("../tests/fixtures/artifact-tls/leaf.der").to_vec(),
                )],
                key,
            )
            .unwrap();
        config.alpn_protocols = vec![b"http/1.1".to_vec()];
        Arc::new(config)
    }
    fn tls_peer() -> (SocketAddr, thread::JoinHandle<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let config = config();
        let thread = thread::spawn(move || {
            let mut socket = listener.accept().unwrap().0;
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            socket
                .set_write_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut tls = rustls::ServerConnection::new(config).unwrap();
            let mut request = Vec::new();
            {
                let mut stream = rustls::Stream::new(&mut tls, &mut socket);
                let mut bytes = [0u8; 1024];
                loop {
                    match stream.read(&mut bytes) {
                        Ok(0) => break,
                        Ok(n) => {
                            request.extend_from_slice(&bytes[..n]);
                            if request.windows(4).any(|w| w == b"\r\n\r\n") {
                                break;
                            }
                        }
                        Err(_) => return request,
                    }
                    if request.len() > 16384 {
                        return request;
                    }
                }
                if request.windows(4).any(|w| w == b"\r\n\r\n") {
                    let _=stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 13\r\nContent-Encoding: identity\r\nConnection: close\r\n\r\nverified-core");
                    let _ = stream.flush();
                }
            }
            tls.send_close_notify();
            let _ = tls.write_tls(&mut socket);
            request
        });
        (address, thread)
    }
    #[test]
    fn actual_verified_tls_body_stages_exact_bytes_and_identity_request() {
        let directory = Directory::new();
        let (address, peer) = tls_peer();
        let cancel = AtomicBool::new(false);
        let budget = Budget {
            deadline: Instant::now() + Duration::from_secs(3),
            cancel: &cancel,
        };
        let artifact = artifact(
            format!("https://{address}/core.gz?fixture=1"),
            b"verified-core",
        );
        let stage = policy().fetch(&directory.0, &artifact, &budget).unwrap();
        assert_eq!(stage.admitted().unwrap().length, 13);
        assert_eq!(
            fs::read(stage.admitted().unwrap().path).unwrap(),
            b"verified-core"
        );
        let request = peer.join().unwrap();
        assert!(request.starts_with(b"GET /core.gz?fixture=1 HTTP/1.1\r\n"));
        assert!(
            request
                .windows(b"Accept-Encoding: identity".len())
                .any(|w| w == b"Accept-Encoding: identity")
        );
    }
    #[test]
    fn untrusted_cert_and_wrong_host_refuse_with_no_staging_or_core_admission() {
        let directory = Directory::new();
        let cancel = AtomicBool::new(false);
        let budget = Budget {
            deadline: Instant::now() + Duration::from_secs(3),
            cancel: &cancel,
        };
        let (address, peer) = tls_peer();
        let artifact = artifact(format!("https://{address}/core"), b"verified-core");
        assert_eq!(
            SourcePolicy::native("127.0.0.1:53".parse().unwrap())
                .unwrap()
                .fetch(&directory.0, &artifact, &budget)
                .unwrap_err(),
            SourceError::Tls
        );
        peer.join().unwrap();
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
        let certificate = rustls::pki_types::CertificateDer::from(
            include_bytes!("../tests/fixtures/artifact-tls/leaf.der").to_vec(),
        );
        let root = rustls::pki_types::CertificateDer::from(
            include_bytes!("../tests/fixtures/artifact-tls/ca.der").to_vec(),
        );
        let mut trust = rustls::RootCertStore::empty();
        trust.add(root).unwrap();
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let verifier =
            rustls::client::WebPkiServerVerifier::builder_with_provider(Arc::new(trust), provider)
                .build()
                .unwrap();
        use rustls::client::danger::ServerCertVerifier;
        assert!(
            verifier
                .verify_server_cert(
                    &certificate,
                    &[],
                    &rustls::pki_types::ServerName::try_from("wrong.example").unwrap(),
                    &[],
                    rustls::pki_types::UnixTime::now()
                )
                .is_err()
        );
    }
    #[test]
    fn plaintext_fixture_is_explicit_and_redirects_are_revalidated() {
        let directory = Directory::new();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let peer = thread::spawn(move || {
            for index in 0..2 {
                let mut socket = listener.accept().unwrap().0;
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = [0u8; 1024];
                let _ = socket.read(&mut bytes).unwrap();
                if index == 0 {
                    socket
                        .write_all(
                            b"HTTP/1.1 302 Found\r\nLocation: /final\r\nContent-Length: 0\r\n\r\n",
                        )
                        .unwrap();
                } else {
                    socket.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nD\r\nverified-core\r\n0\r\n\r\n").unwrap();
                }
            }
        });
        let artifact = artifact(format!("http://{address}/start"), b"verified-core");
        let cancel = AtomicBool::new(false);
        let budget = Budget {
            deadline: Instant::now() + Duration::from_secs(3),
            cancel: &cancel,
        };
        assert_eq!(
            SourcePolicy::native("127.0.0.1:53".parse().unwrap())
                .unwrap()
                .fetch(&directory.0, &artifact, &budget)
                .unwrap_err(),
            SourceError::Input
        );
        let stage = SourcePolicy::loopback_fixture("127.0.0.1:53".parse().unwrap())
            .unwrap()
            .fetch(&directory.0, &artifact, &budget)
            .unwrap();
        assert_eq!(
            fs::read(stage.admitted().unwrap().path).unwrap(),
            b"verified-core"
        );
        peer.join().unwrap();
    }
    #[test]
    fn cancel_deadline_and_closed_tls_peer_never_spin_or_create_stage() {
        let directory = Directory::new();
        let canceled = AtomicBool::new(true);
        let budget = Budget {
            deadline: Instant::now() + Duration::from_secs(1),
            cancel: &canceled,
        };
        let metadata = artifact("https://127.0.0.1:1/core".into(), b"x");
        assert_eq!(
            policy()
                .fetch(&directory.0, &metadata, &budget)
                .unwrap_err(),
            SourceError::Cancelled
        );
        let cancel = AtomicBool::new(false);
        let expired = Budget {
            deadline: Instant::now(),
            cancel: &cancel,
        };
        assert_eq!(
            policy()
                .fetch(&directory.0, &metadata, &expired)
                .unwrap_err(),
            SourceError::Deadline
        );
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let peer = thread::spawn(move || {
            let _ = listener.accept().unwrap();
        });
        let budget = Budget {
            deadline: Instant::now() + Duration::from_secs(2),
            cancel: &cancel,
        };
        let now = Instant::now();
        assert!(matches!(
            policy().fetch(
                &directory.0,
                &artifact(format!("https://{address}/core"), b"x"),
                &budget
            ),
            Err(SourceError::Tls) | Err(SourceError::Connect)
        ));
        assert!(now.elapsed() < Duration::from_secs(1));
        peer.join().unwrap();
        assert_eq!(fs::read_dir(directory.0.clone()).unwrap().count(), 0);
    }

    #[test]
    fn actual_wrong_hostname_handshake_and_expired_leaf_refuse_native_verifier() {
        let (address, peer) = tls_peer();
        let cancel = AtomicBool::new(false);
        let budget = Budget {
            deadline: Instant::now() + Duration::from_secs(2),
            cancel: &cancel,
        };
        let policy = policy();
        let socket =
            readiness_dns::connect_literal(address, budget.deadline, Some(budget.cancel)).unwrap();
        let mut connection = rustls::ClientConnection::new(
            policy.tls.clone(),
            rustls::pki_types::ServerName::try_from("wrong.example")
                .unwrap()
                .to_owned(),
        )
        .unwrap();
        connection.set_buffer_limit(Some(16 << 10));
        let mut transport = Transport {
            socket,
            tls: Some(connection),
            budget,
            handshake_bytes: 0,
        };
        assert_eq!(transport.handshake(), Err(SourceError::Tls));
        drop(transport);
        peer.join().unwrap();
        let certificate = rustls::pki_types::CertificateDer::from(
            include_bytes!("../tests/fixtures/artifact-tls/leaf.der").to_vec(),
        );
        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(rustls::pki_types::CertificateDer::from(
                include_bytes!("../tests/fixtures/artifact-tls/ca.der").to_vec(),
            ))
            .unwrap();
        let verifier = rustls::client::WebPkiServerVerifier::builder_with_provider(
            Arc::new(roots),
            Arc::new(rustls::crypto::ring::default_provider()),
        )
        .build()
        .unwrap();
        use rustls::client::danger::ServerCertVerifier;
        let expired =
            rustls::pki_types::UnixTime::since_unix_epoch(Duration::from_secs(10_000_000_000));
        assert!(
            verifier
                .verify_server_cert(
                    &certificate,
                    &[],
                    &rustls::pki_types::ServerName::try_from("localhost").unwrap(),
                    &[],
                    expired
                )
                .is_err()
        );
    }
    #[test]
    fn unanswered_http_headers_and_mid_io_cancel_obey_absolute_budget() {
        let directory = Directory::new();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let peer = thread::spawn(move || {
            let mut socket = listener.accept().unwrap().0;
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = [0u8; 1024];
            let _ = socket.read(&mut request).unwrap();
            let mut later = [0u8; 1];
            let _ = socket.read(&mut later);
        });
        let cancel = AtomicBool::new(false);
        let budget = Budget {
            deadline: Instant::now() + Duration::from_millis(100),
            cancel: &cancel,
        };
        let started = Instant::now();
        let fixture = SourcePolicy::loopback_fixture("127.0.0.1:53".parse().unwrap()).unwrap();
        assert_eq!(
            fixture
                .fetch(
                    &directory.0,
                    &artifact(format!("http://{address}/core"), b"x"),
                    &budget
                )
                .unwrap_err(),
            SourceError::Deadline
        );
        assert!(started.elapsed() < Duration::from_secs(1));
        peer.join().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = cancel.clone();
        let peer = thread::spawn(move || {
            let mut socket = listener.accept().unwrap().0;
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = [0u8; 1024];
            let _ = socket.read(&mut request).unwrap();
            signal.store(true, Ordering::Release);
            let mut later = [0u8; 1];
            let _ = socket.read(&mut later);
        });
        let budget = Budget {
            deadline: Instant::now() + Duration::from_secs(2),
            cancel: &cancel,
        };
        assert_eq!(
            fixture
                .fetch(
                    &directory.0,
                    &artifact(format!("http://{address}/core"), b"x"),
                    &budget
                )
                .unwrap_err(),
            SourceError::Cancelled
        );
        peer.join().unwrap();
        assert_eq!(fs::read_dir(directory.0.clone()).unwrap().count(), 0);
    }

    #[test]
    fn hostname_fetch_uses_bounded_actual_bootstrap_dns_and_certificate_name() {
        let directory = Directory::new();
        let (address, tls_peer) = tls_peer();
        let dns = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let bootstrap = dns.local_addr().unwrap();
        dns.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let dns_peer = thread::spawn(move || {
            let mut buffer = [0u8; 4096];
            for _ in 0..2 {
                let (n, peer) = dns.recv_from(&mut buffer).unwrap();
                let mut response = buffer[..n].to_vec();
                response[2..4].copy_from_slice(&0x8180u16.to_be_bytes());
                response[6..8].copy_from_slice(&1u16.to_be_bytes());
                response.extend_from_slice(&[0xc0, 0x0c]);
                let kind = u16::from_be_bytes([buffer[n - 4], buffer[n - 3]]);
                response.extend_from_slice(&kind.to_be_bytes());
                response.extend_from_slice(&[0, 1, 0, 0, 0, 60]);
                if kind == 1 {
                    response.extend_from_slice(&[0, 4, 127, 0, 0, 1]);
                } else {
                    response.extend_from_slice(&[0, 16]);
                    response.extend_from_slice(&std::net::Ipv6Addr::LOCALHOST.octets());
                }
                dns.send_to(&response, peer).unwrap();
            }
        });
        let mut policy = policy();
        policy.bootstrap = bootstrap;
        let cancel = AtomicBool::new(false);
        let budget = Budget {
            deadline: Instant::now() + Duration::from_secs(3),
            cancel: &cancel,
        };
        let stage = policy
            .fetch(
                &directory.0,
                &artifact(
                    format!("https://localhost:{}/core", address.port()),
                    b"verified-core",
                ),
                &budget,
            )
            .unwrap();
        assert_eq!(
            fs::read(stage.admitted().unwrap().path).unwrap(),
            b"verified-core"
        );
        dns_peer.join().unwrap();
        let request = tls_peer.join().unwrap();
        assert!(
            request
                .windows(b"Host: localhost:".len())
                .any(|w| w == b"Host: localhost:")
        );
    }
    #[test]
    fn actual_framing_encoding_status_and_digest_failures_leave_no_stage() {
        let directory = Directory::new();
        let fixture = SourcePolicy::loopback_fixture("127.0.0.1:53".parse().unwrap()).unwrap();
        for (response, expected) in [
            (
                b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\n\r\nx".as_slice(),
                SourceError::Http,
            ),
            (
                b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\nTransfer-Encoding: chunked\r\n\r\nx"
                    .as_slice(),
                SourceError::Http,
            ),
            (
                b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\nContent-Encoding: gzip\r\n\r\nx"
                    .as_slice(),
                SourceError::Http,
            ),
            (
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".as_slice(),
                SourceError::Status,
            ),
            (
                b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\n\r\nx".as_slice(),
                SourceError::Stage(StageError::Digest),
            ),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let peer = thread::spawn(move || {
                let mut socket = listener.accept().unwrap().0;
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = [0u8; 1024];
                let _ = socket.read(&mut request).unwrap();
                socket.write_all(response).unwrap();
            });
            let cancel = AtomicBool::new(false);
            let budget = Budget {
                deadline: Instant::now() + Duration::from_secs(2),
                cancel: &cancel,
            };
            assert_eq!(
                fixture
                    .fetch(
                        &directory.0,
                        &artifact(format!("http://{address}/core"), b"y"),
                        &budget
                    )
                    .unwrap_err(),
                expected
            );
            peer.join().unwrap();
            assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
        }
    }
    #[test]
    fn bad_metadata_refuses_before_network_and_http_redirect_budget_is_finite() {
        let directory = Directory::new();
        let fixture = SourcePolicy::loopback_fixture("127.0.0.1:53".parse().unwrap()).unwrap();
        let cancel = AtomicBool::new(false);
        let budget = Budget {
            deadline: Instant::now() + Duration::from_secs(3),
            cancel: &cancel,
        };
        let mut bad = artifact("http://127.0.0.1:1/core".into(), b"x");
        bad.sha256 = "invalid".into();
        assert_eq!(
            fixture.fetch(&directory.0, &bad, &budget).unwrap_err(),
            SourceError::Input
        );
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let peer = thread::spawn(move || {
            for _ in 0..=MAX_REDIRECTS {
                let mut socket = listener.accept().unwrap().0;
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = [0u8; 1024];
                let _ = socket.read(&mut request).unwrap();
                socket
                    .write_all(
                        b"HTTP/1.1 302 Found\r\nLocation: /again\r\nContent-Length: 0\r\n\r\n",
                    )
                    .unwrap();
            }
        });
        assert_eq!(
            fixture
                .fetch(
                    &directory.0,
                    &artifact(format!("http://{address}/again"), b"x"),
                    &budget
                )
                .unwrap_err(),
            SourceError::Redirect
        );
        peer.join().unwrap();
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
    }

    #[test]
    fn authenticated_tls_record_failure_after_handshake_keeps_tls_error_class() {
        let directory = Directory::new();
        for after_headers in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let config = config();
            let peer = thread::spawn(move || {
                let mut socket = listener.accept().unwrap().0;
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                socket
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut tls = rustls::ServerConnection::new(config).unwrap();
                {
                    let mut stream = rustls::Stream::new(&mut tls, &mut socket);
                    let mut buffer = [0u8; 1024];
                    let mut request = Vec::new();
                    loop {
                        let n = stream.read(&mut buffer).unwrap();
                        assert!(n > 0);
                        request.extend_from_slice(&buffer[..n]);
                        if request.windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                        assert!(request.len() <= 16384);
                    }
                    if after_headers {
                        stream
                            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 13\r\n\r\n")
                            .unwrap();
                        stream.flush().unwrap();
                    }
                }
                // A deliberately unauthenticated encrypted application record,
                // after the real certificate/hostname-validated handshake.
                socket.write_all(&[23, 3, 3, 0, 20]).unwrap();
                socket.write_all(&[0x7fu8; 20]).unwrap();
                socket.flush().unwrap();
            });
            let cancel = AtomicBool::new(false);
            let budget = Budget {
                deadline: Instant::now() + Duration::from_secs(3),
                cancel: &cancel,
            };
            let failure = policy()
                .fetch(
                    &directory.0,
                    &artifact(format!("https://{address}/core"), b"verified-core"),
                    &budget,
                )
                .unwrap_err();
            peer.join().unwrap();
            assert_eq!(
                failure,
                SourceError::Tls,
                "header/body failure must keep fixed transport class"
            );
            assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
        }
    }

    #[test]
    fn private_subscription_consumer_keeps_exact_bytes_and_refuses_limit_or_extra_redirect() {
        let fixture = SourcePolicy::loopback_fixture("127.0.0.1:53".parse().unwrap()).unwrap();
        let raw = b"proxies: []\nrules: []\n";
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let peer = thread::spawn(move || {
            let mut stream = listener.accept().unwrap().0;
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request).unwrap();
            let header = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", raw.len());
            stream.write_all(header.as_bytes()).unwrap();
            stream.write_all(raw).unwrap();
        });
        let cancel = AtomicBool::new(false);
        let budget = Budget {
            deadline: Instant::now() + Duration::from_secs(3),
            cancel: &cancel,
        };
        assert_eq!(
            fixture
                .fetch_subscription(&format!("http://{address}/subscription"), &budget)
                .unwrap(),
            raw
        );
        peer.join().unwrap();
        let mut reader = std::io::Cursor::new(vec![7u8; (2 << 20) + 1]);
        assert_eq!(
            read_source_bytes(&mut reader, 2 << 20, &budget),
            Err(SourceError::Limit)
        );
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let peer = thread::spawn(move || {
            for _ in 0..=3 {
                let mut stream = listener.accept().unwrap().0;
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = [0u8; 1024];
                let _ = stream.read(&mut bytes).unwrap();
                stream
                    .write_all(
                        b"HTTP/1.1 302 Found\r\nLocation: /again\r\nContent-Length: 0\r\n\r\n",
                    )
                    .unwrap();
            }
        });
        assert_eq!(
            fixture.fetch_subscription(&format!("http://{address}/again"), &budget),
            Err(SourceError::Redirect)
        );
        peer.join().unwrap();
    }
}

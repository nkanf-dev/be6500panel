//! Opt-in legacy request traces and isolated node probes. One caller owns all
//! state and exact children. GET does not resolve, launch, lease, or probe.
//! Root must bind its authoritative revision, trusted SourcePolicy, candidate,
//! and retained accepted Run. HTTP cannot supply URLs, paths, PIDs, or argv.
//! Call tick regularly and close until it returns true before dropping an owner.
use crate::{
    artifact_http::{self, Url}, artifact_source::SourcePolicy,
    http::Method, native::Node, product_io::{Backend, timestamp},
    readiness_dns, readiness_tun::{Budget, FileIdentity, TunError},
    runtime_manager::OwnedRunIdentity,
};
use serde::{Deserialize, Serialize, Deserializer, de::{self, Visitor, SeqAccess}};
use std::cell::Cell;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, VecDeque}, fmt, fs::{self, File, OpenOptions},
    io::{self, Read, Write}, net::{IpAddr, SocketAddr, TcpListener, TcpStream},
    os::{fd::AsRawFd, unix::{fs::{DirBuilderExt, MetadataExt, OpenOptionsExt}, process::CommandExt}},
    path::{Path, PathBuf}, process::{Child, Command, Stdio},
    sync::Arc, time::{Duration, Instant},
};
const PROBES: &str = "/api/proxy/node-probes";
const TRACES: &str = "/api/proxy/request-traces";
const TARGET: &str = "https://www.gstatic.com/generate_204";
const MAX_NODES: usize = 256;
const CAPACITY: usize = 64;
const JOB_HISTORY: usize = 16;
const MAX_NATIVE: usize = 4 << 20;
const MAX_BINARY: u64 = 40 << 20;
const NODE_TIMEOUT: Duration = Duration::from_secs(3);
const TRACE_TIMEOUT: Duration = Duration::from_secs(10);
const START_TIMEOUT: Duration = Duration::from_secs(5);
const JOB_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const BODY_LIMIT: usize = 64 << 10;
const PROC_LIMIT: usize = 1 << 20;
const HANDSHAKE_LIMIT: usize = 256 << 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApiError { pub status: u16, pub code: &'static str, pub message: &'static str }
impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(self.message) }
}
impl std::error::Error for ApiError {}
fn error(status: u16, code: &'static str, message: &'static str) -> ApiError {
    ApiError { status, code, message }
}
fn invalid() -> ApiError { error(400, "invalid_input", "请选择固定诊断目标与直连或当前代理链路。") }
fn artifact_error() -> ApiError {
    error(503, "artifact_unavailable", "请先获取已校验的 sing-box 运行文件；无需启动或切换当前代理。")
}
fn unavailable() -> ApiError {
    error(503, "request_trace_unavailable", "当前已接受的代理监听不可用；请检查核心与原生配置。")
}
fn check(b: &Budget<'_>) -> Result<(), ApiError> {
    b.check().map_err(|e| match e {
        TunError::Cancelled => error(409, "operation_cancelled", "诊断操作已取消。"),
        _ => error(504, "operation_timeout", "诊断操作超过截止时间。"),
    })
}
fn token() -> Result<String, ApiError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| artifact_error())?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
fn digest(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
fn valid_id(id: &str) -> bool { !id.is_empty() && id.len() <= 128 && !id.chars().any(char::is_control) }
fn usable(ip: IpAddr) -> bool {
    !ip.is_unspecified() && !ip.is_multicast() && match ip {
        IpAddr::V4(a) => !a.is_link_local() && !a.is_broadcast(),
        IpAddr::V6(a) => !a.is_unicast_link_local() && a.to_ipv4_mapped().is_none(),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Start { #[serde(default)] all: bool, #[serde(default)] node_ids: Bounded<String, MAX_NODES>, revision: String }
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct TraceInput { target_id: String, route: String }
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResultRow {
    node_id: String, status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")] delay_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")] measured_at: Option<String>,
    target: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")] error_code: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")] evidence: Option<Value>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Job {
    id: String, status: &'static str, total: usize, completed: usize, started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")] finished_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] error_code: Option<&'static str>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Phase {
    id: &'static str, observed: bool, start_ms: Option<f64>, end_ms: Option<f64>, duration_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")] reason: Option<&'static str>,
}
struct Recorder { start: Instant, phases: Vec<Phase>, last: &'static str }
impl Recorder {
    fn new(proxy: bool) -> Self {
        Self { start: Instant::now(), last: "request", phases: ["dns", "tcp", "connect", "tls", "ttfb", "transfer"].into_iter().map(|id| Phase {
            id, observed: false, start_ms: None, end_ms: None, duration_ms: None,
            reason: Some(match (id, proxy) {
                ("dns", true) => "proxy_origin_dns_not_observable",
                ("connect", false) => "not_used_by_direct_route",
                _ => "not_observed",
            }),
        }).collect() }
    }
    fn begin(&mut self, id: &'static str) {
        let now = self.start.elapsed().as_secs_f64() * 1000.0;
        if let Some(p) = self.phases.iter_mut().find(|p| p.id == id) {
            if p.observed && id == "tcp" { p.reason = Some("multiple_connection_attempt_span"); }
            else { p.reason = None; }
            p.observed = true;
            if p.start_ms.is_none() { p.start_ms = Some(now); }
        }
        self.last = id;
    }
    fn end(&mut self, id: &'static str) {
        let now = self.start.elapsed().as_secs_f64() * 1000.0;
        if let Some(p) = self.phases.iter_mut().find(|p| p.id == id && p.start_ms.is_some()) {
            p.end_ms = Some(now); p.duration_ms = p.start_ms.map(|start| now - start);
        }
        self.last = id;
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Trace {
    id: String, target_id: String, target_label: String, url: String, route: String,
    started_at: String, finished_at: String, total_ms: f64, outcome: &'static str,
    status_code: Option<u16>, bytes_read: usize, body_limit_reached: bool,
    peer_address: Option<String>, peer_scope: &'static str, failure_phase: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")] error_code: Option<&'static str>,
    phases: Vec<Phase>, observed_route: &'static str, route_source: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")] redirect: Option<Value>,
}
struct Observation {
    recorder: Recorder, status: Option<u16>, bytes: usize, limit: bool, peer: Option<String>,
    failure: Option<Failure>, redirect: Option<Value>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Failure { phase: &'static str, code: &'static str, outcome: &'static str }
impl Failure {
    fn stage(phase: &'static str, code: &'static str) -> Self { Self { phase, code, outcome: "failed" } }
    fn budget(phase: &'static str, b: &Budget<'_>) -> Option<Self> {
        b.check().err().map(|e| match e {
            TunError::Cancelled => Self { phase, code: "cancelled", outcome: "cancelled" },
            _ => Self { phase, code: "timeout", outcome: "timeout" },
        })
    }
    fn checked(phase: &'static str, b: &Budget<'_>, code: &'static str) -> Self {
        Self::budget(phase, b).unwrap_or(Self::stage(phase, code))
    }
}

/// Root-only executable admission. Named and pinned inode, digest, and private
/// temporary root are checked. Each job makes its own immutable leased copy.
struct Candidate { path: PathBuf, file: File, identity: FileIdentity, sha: [u8; 32], temporary: PathBuf, temporary_id: FileIdentity }
impl Candidate {
    fn unchanged(&self) -> bool {
        self.file.metadata().is_ok_and(|m| FileIdentity::from_metadata(&m) == self.identity)
            && fs::symlink_metadata(&self.path).is_ok_and(|m| FileIdentity::from_metadata(&m) == self.identity)
            && fs::symlink_metadata(&self.temporary).is_ok_and(|m| {
                let id = FileIdentity::from_metadata(&m);
                same_directory(id, self.temporary_id) && m.is_dir() && m.mode() & 0o7777 == 0o700
            })
    }
}
fn same_directory(a: FileIdentity, b: FileIdentity) -> bool {
    (a.device, a.inode, a.mode, a.uid) == (b.device, b.inode, b.mode, b.uid)
}
fn private_dir(path: &Path) -> Result<(PathBuf, FileIdentity), ApiError> {
    if !path.is_absolute() || path.components().any(|p| matches!(p, std::path::Component::ParentDir | std::path::Component::CurDir)) {
        return Err(artifact_error());
    }
    let resolved = path.canonicalize().map_err(|_| artifact_error())?;
    let m = fs::symlink_metadata(&resolved).map_err(|_| artifact_error())?;
    if !m.is_dir() || m.mode() & 0o7777 != 0o700 || m.uid() != unsafe { libc::geteuid() } { return Err(artifact_error()); }
    Ok((resolved, FileIdentity::from_metadata(&m)))
}
fn hash_file(file: &File, size: u64, budget: &Budget<'_>, mut write: Option<&mut File>) -> Result<[u8;32], ApiError> {
    use std::os::unix::fs::FileExt;
    let mut sha = Sha256::new(); let mut offset = 0u64; let mut bytes = [0u8; 8192];
    loop {
        check(budget)?;
        let count = file.read_at(&mut bytes, offset).map_err(|_| artifact_error())?;
        check(budget)?;
        if count == 0 { break; }
        offset = offset.checked_add(count as u64).ok_or_else(artifact_error)?;
        if offset > size || offset > MAX_BINARY { return Err(artifact_error()); }
        sha.update(&bytes[..count]);
        if let Some(out) = write.as_deref_mut() { out.write_all(&bytes[..count]).map_err(|_| artifact_error())?; }
    }
    if offset != size { return Err(artifact_error()); }
    check(budget)?; Ok(sha.finalize().into())
}

/// A kernel-backed identity tied to a Root-owned Run or this module's exact Child.
/// All proc paths are internally constructed; Backend fixture paths stay fake.
#[derive(Clone)]
struct ProcOwner { pid: u32, start: String, executable: PathBuf, executable_stamp: (u64,u64,u64,u32,u32) }
fn executable_stamp(m: &crate::product_io::Metadata) -> Result<(u64,u64,u64,u32,u32),ApiError> {
    if !m.regular || m.symlink || m.size==0 || m.size>MAX_BINARY || m.mode&0o111==0 || m.mode&0o022!=0 { return Err(unavailable()); }
    Ok((m.dev,m.ino,m.size,m.mode,m.uid))
}
fn start_time(raw: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(raw).ok()?; let (_, suffix) = text.rsplit_once(')')?;
    let fields: Vec<&str> = suffix.split_whitespace().collect();
    if fields.len() < 20 || matches!(fields[0], "Z" | "X" | "x") || !fields[19].bytes().all(|b| b.is_ascii_digit()) { return None; }
    Some(fields[19].to_owned())
}
impl ProcOwner {
    fn bind(pid: u32, executable: PathBuf, io: &mut impl Backend, b: &Budget<'_>) -> Result<Self, ApiError> {
        if pid == 0 || !executable.is_absolute() { return Err(unavailable()); }
        check(b)?;
        let raw = io.read(&PathBuf::from(format!("/proc/{pid}/stat")), 4096, b).map_err(|_| unavailable())?;
        if raw.len() > 4096 { return Err(unavailable()); }
        let metadata=io.metadata(&executable,true,b).map_err(|_| unavailable())?;
        let owner = Self { pid, start: start_time(&raw).ok_or_else(unavailable)?, executable, executable_stamp: executable_stamp(&metadata)? };
        owner.identity(io, b)?; Ok(owner)
    }
    fn identity(&self, io: &mut impl Backend, b: &Budget<'_>) -> Result<(), ApiError> {
        check(b)?;
        let raw = io.read(&PathBuf::from(format!("/proc/{}/stat", self.pid)), 4096, b).map_err(|_| unavailable())?;
        if raw.len() > 4096 || start_time(&raw).as_deref() != Some(self.start.as_str()) { return Err(unavailable()); }
        let path = io.read_link(&PathBuf::from(format!("/proc/{}/exe", self.pid)), b).map_err(|_| unavailable())?;
        if path != self.executable { return Err(unavailable()); }
        for path in [&self.executable, &PathBuf::from(format!("/proc/{}/exe",self.pid))] {
            let metadata=io.metadata(path,true,b).map_err(|_| unavailable())?;
            if executable_stamp(&metadata)?!=self.executable_stamp { return Err(unavailable()); }
        }
        check(b)
    }
    fn verify(&self, local: SocketAddr, remote: Option<SocketAddr>, io: &mut impl Backend, b: &Budget<'_>) -> Result<(), ApiError> {
        self.identity(io, b)?;
        let fd_root = PathBuf::from(format!("/proc/{}/fd", self.pid));
        let descriptors = io.list(&fd_root, 1024, b).map_err(|_| unavailable())?;
        if descriptors.len() > 1024 { return Err(unavailable()); }
        let mut inodes = BTreeSet::new();
        for fd in descriptors {
            check(b)?;
            if fd.is_empty() || fd.len() > 20 || !fd.bytes().all(|c| c.is_ascii_digit()) { return Err(unavailable()); }
            if let Ok(target) = io.read_link(&fd_root.join(fd), b) {
                if let Some(inode) = target.to_str().and_then(|s| s.strip_prefix("socket:[")).and_then(|s| s.strip_suffix(']')) {
                    if !inode.is_empty() && inode.len() <= 20 && inode.bytes().all(|c| c.is_ascii_digit()) { inodes.insert(inode.to_owned()); }
                }
            }
        }
        let table = if local.is_ipv4() { "tcp" } else { "tcp6" };
        let bytes = io.read(&PathBuf::from(format!("/proc/{}/net/{table}", self.pid)), PROC_LIMIT, b).map_err(|_| unavailable())?;
        if bytes.len() > PROC_LIMIT { return Err(unavailable()); }
        let text = std::str::from_utf8(&bytes).map_err(|_| unavailable())?;
        let local = proc_address(local);
        let remote = remote.map(proc_address).unwrap_or_else(|| if table == "tcp" { "00000000:0000".into() } else { format!("{}:0000", "0".repeat(32)) });
        let state = if remote.ends_with(":0000") { "0A" } else { "01" };
        let found = text.lines().any(|line| {
            let fields: Vec<&str> = line.split_whitespace().take(11).collect();
            fields.len() >= 10 && fields[1] == local && fields[2] == remote && fields[3] == state && inodes.contains(fields[9])
        });
        if !found { return Err(unavailable()); }
        self.identity(io, b)
    }
}
fn proc_address(address: SocketAddr) -> String {
    let bytes: Vec<u8> = match address.ip() {
        IpAddr::V4(ip) => ip.octets().to_vec(), IpAddr::V6(ip) => ip.octets().to_vec(),
    };
    let host: String = bytes.chunks_exact(4).map(|chunk| {
        let word = u32::from_ne_bytes(chunk.try_into().expect("four bytes")); format!("{word:08X}")
    }).collect();
    format!("{host}:{:04X}", address.port())
}
struct Accepted { hash: String, owner: ProcOwner, verified_lan: Vec<IpAddr> }
#[derive(Deserialize)]
#[serde(default)]
struct Inbound {
    #[serde(rename = "type", deserialize_with = "bounded_text")] kind: String,
    #[serde(deserialize_with = "bounded_text")] listen: String, listen_port: u16,
    users: Option<Bounded<serde::de::IgnoredAny, 256>>, tls: Option<InboundTls>,
}
impl Default for Inbound {
    fn default() -> Self { Self { kind: String::new(), listen: String::new(), listen_port: 0, users: None, tls: None } }
}
#[derive(Deserialize)]
struct InboundTls { #[serde(default)] enabled: bool }
#[derive(Deserialize)]
struct Inbounds { inbounds: Bounded<Inbound, 64> }
struct Bounded<T, const N: usize>(Vec<T>);
impl<T, const N: usize> Default for Bounded<T, N> { fn default() -> Self { Self(Vec::new()) } }
impl<T, const N: usize> std::ops::Deref for Bounded<T, N> {
    type Target = Vec<T>; fn deref(&self) -> &Vec<T> { &self.0 }
}
impl<'de, T: Deserialize<'de>, const N: usize> Deserialize<'de> for Bounded<T, N> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<T, const N: usize>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>, const N: usize> Visitor<'de> for V<T, N> {
            type Value = Bounded<T,N>;
            fn expecting(&self,f:&mut fmt::Formatter<'_>)->fmt::Result { f.write_str("bounded array") }
            fn visit_unit<E:de::Error>(self)->Result<Self::Value,E> { Ok(Bounded::default()) }
            fn visit_seq<S:SeqAccess<'de>>(self,mut s:S)->Result<Self::Value,S::Error> {
                let mut rows=Vec::new();
                while rows.len()<N {
                    match s.next_element()? { Some(row)=>rows.push(row), None=>return Ok(Bounded(rows)) }
                }
                if s.next_element::<de::IgnoredAny>()?.is_some() { return Err(de::Error::custom("diagnostic array limit")); }
                Ok(Bounded(rows))
            }
        }
        d.deserialize_any(V::<T,N>(std::marker::PhantomData))
    }
}
fn bounded_text<'de,D:Deserializer<'de>>(d:D)->Result<String,D::Error> {
    struct V;
    impl Visitor<'_> for V {
        type Value=String;
        fn expecting(&self,f:&mut fmt::Formatter<'_>)->fmt::Result { f.write_str("bounded listener text") }
        fn visit_str<E:de::Error>(self,s:&str)->Result<String,E> {
            if s.len()>128 { return Err(E::custom("diagnostic text limit")); } Ok(s.to_owned())
        }
    }
    d.deserialize_str(V)
}
fn object<'de,T:Deserialize<'de>>(raw:&'de [u8])->Result<T,ApiError> {
    if raw.iter().copied().find(|b| !b.is_ascii_whitespace())!=Some(b'{') { return Err(invalid()); }
    serde_json::from_slice(raw).map_err(|_| invalid())
}
fn mixed_proxy(raw: &[u8], lan: &[IpAddr]) -> Result<SocketAddr, ApiError> {
    if raw.is_empty() || raw.len() > MAX_NATIVE || lan.len() > 32 { return Err(unavailable()); }
    let doc: Inbounds = object(raw).map_err(|_| unavailable())?;
    if doc.inbounds.len() > 64 { return Err(unavailable()); }
    let mut endpoint = None;
    for inbound in doc.inbounds.0.into_iter().filter(|inbound| inbound.kind == "mixed") {
        if endpoint.is_some() || inbound.listen.len() > 64 || inbound.listen_port == 0
            || inbound.users.is_some_and(|users| !users.is_empty()) || inbound.tls.is_some_and(|tls| tls.enabled) { return Err(unavailable()); }
        let ip: IpAddr = inbound.listen.parse().map_err(|_| unavailable())?;
        if !usable(ip) || !(ip.is_loopback() || lan.contains(&ip)) { return Err(unavailable()); }
        endpoint = Some(SocketAddr::new(ip, inbound.listen_port));
    }
    endpoint.ok_or_else(unavailable)
}

struct ProxyRoute { address: SocketAddr, owner: ProcOwner, credentials: Option<(String, String)> }
fn tls_config() -> Result<Arc<rustls::ClientConfig>, Failure> {
    let roots = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
    let mut config = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions().map_err(|_| Failure::stage("tls", "tls_failed"))?
        .with_root_certificates(roots).with_no_client_auth();
    config.alpn_protocols = vec![b"http/1.1".to_vec()]; config.max_fragment_size = Some(8192);
    config.resumption = rustls::client::Resumption::disabled(); Ok(Arc::new(config))
}
fn transient(error: &io::Error) -> bool {
    matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted)
}
fn wait(socket: &TcpStream, events: libc::c_short, phase: &'static str, budget: &Budget<'_>) -> Result<(), Failure> {
    loop {
        if let Some(f) = Failure::budget(phase, budget) { return Err(f); }
        let millis = budget.deadline.saturating_duration_since(Instant::now()).min(Duration::from_millis(20)).as_millis().max(1) as i32;
        let mut poll = libc::pollfd { fd: socket.as_raw_fd(), events, revents: 0 };
        let ready = unsafe { libc::poll(&mut poll, 1, millis) };
        if let Some(f) = Failure::budget(phase, budget) { return Err(f); }
        if ready < 0 {
            if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted { continue; }
            return Err(Failure::stage(phase, "request_failed"));
        }
        if ready == 0 { continue; }
        if poll.revents & (libc::POLLERR | libc::POLLNVAL) != 0 { return Err(Failure::stage(phase, "request_failed")); }
        if poll.revents & (events | libc::POLLHUP) != 0 { return Ok(()); }
    }
}
/// Drop closes the exact diagnostic socket with real SO_LINGER(1,0). It never
/// changes production listener settings, and no socket metadata implies ready.
struct Socket(TcpStream);
impl Socket {
    fn new(socket: TcpStream) -> Result<Self, Failure> {
        let owned = Self(socket);
        let linger = libc::linger { l_onoff: 1, l_linger: 0 };
        let rc = unsafe { libc::setsockopt(owned.0.as_raw_fd(), libc::SOL_SOCKET, libc::SO_LINGER,
            (&linger as *const libc::linger).cast(), std::mem::size_of_val(&linger) as libc::socklen_t) };
        if rc != 0 { return Err(Failure::stage("tcp", "request_failed")); }
        Ok(owned)
    }
}
struct Transport<'a> { socket: Socket, tls: Option<rustls::ClientConnection>, budget: Budget<'a>, phase: &'static str, tls_bytes: usize }
impl Transport<'_> {
    fn flush_tls(&mut self) -> Result<(), Failure> {
        while self.tls.as_ref().is_some_and(|tls| tls.wants_write()) {
            if let Some(f) = Failure::budget(self.phase, &self.budget) { return Err(f); }
            match self.tls.as_mut().ok_or(Failure::stage(self.phase, "tls_failed"))?.write_tls(&mut self.socket.0) {
                Ok(0) => return Err(Failure::stage(self.phase, "tls_failed")),
                Ok(_) => {}, Err(e) if transient(&e) => wait(&self.socket.0, libc::POLLOUT, self.phase, &self.budget)?,
                Err(_) => return Err(Failure::checked(self.phase, &self.budget, "tls_failed")),
            }
        }
        Ok(())
    }
    fn read_tls(&mut self) -> Result<(), Failure> {
        loop {
            if let Some(f) = Failure::budget(self.phase, &self.budget) { return Err(f); }
            let mut limited = (&mut self.socket.0).take(8192);
            match self.tls.as_mut().ok_or(Failure::stage(self.phase, "tls_failed"))?.read_tls(&mut limited) {
                Ok(0) => return Err(Failure::stage(self.phase, "tls_failed")),
                Ok(n) => {
                    if self.tls.as_ref().is_some_and(|tls| tls.is_handshaking()) {
                        self.tls_bytes = self.tls_bytes.checked_add(n).ok_or(Failure::stage("tls", "tls_failed"))?;
                        if self.tls_bytes > HANDSHAKE_LIMIT { return Err(Failure::stage("tls", "tls_failed")); }
                    }
                    self.tls.as_mut().ok_or(Failure::stage("tls", "tls_failed"))?.process_new_packets()
                        .map_err(|e| Failure::stage(self.phase, if matches!(e, rustls::Error::InvalidCertificate(_) | rustls::Error::NoCertificatesPresented) {
                            "tls_verification_failed"
                        } else { "tls_failed" }))?;
                    return Ok(());
                }
                Err(e) if transient(&e) => wait(&self.socket.0, libc::POLLIN, self.phase, &self.budget)?,
                Err(_) => return Err(Failure::checked(self.phase, &self.budget, "tls_failed")),
            }
        }
    }
    fn handshake(&mut self, host: &str) -> Result<(), Failure> {
        self.phase = "tls";
        let name = rustls::pki_types::ServerName::try_from(host.to_owned()).map_err(|_| Failure::stage("tls", "tls_failed"))?;
        let mut tls = rustls::ClientConnection::new(tls_config()?, name).map_err(|_| Failure::stage("tls", "tls_failed"))?;
        tls.set_buffer_limit(Some(16 << 10)); self.tls = Some(tls);
        while self.tls.as_ref().is_some_and(|tls| tls.is_handshaking()) {
            self.flush_tls()?;
            if self.tls.as_ref().is_some_and(|tls| tls.is_handshaking()) { self.read_tls()?; }
        }
        self.flush_tls()?;
        if self.tls.as_ref().and_then(|tls| tls.alpn_protocol()).is_some_and(|alpn| alpn != b"http/1.1") { return Err(Failure::stage("tls", "tls_failed")); }
        Ok(())
    }
    fn write(&mut self, mut bytes: &[u8]) -> Result<(), Failure> {
        while !bytes.is_empty() {
            if let Some(f) = Failure::budget(self.phase, &self.budget) { return Err(f); }
            let result = match &mut self.tls { Some(tls) => tls.writer().write(bytes), None => self.socket.0.write(bytes) };
            match result {
                Ok(0) => return Err(Failure::stage(self.phase, "request_failed")),
                Ok(n) => { bytes = &bytes[n..]; self.flush_tls()?; },
                Err(e) if transient(&e) => wait(&self.socket.0, libc::POLLOUT, self.phase, &self.budget)?,
                Err(_) => return Err(Failure::checked(self.phase, &self.budget, "request_failed")),
            }
        }
        Ok(())
    }
    fn read(&mut self, into: &mut [u8]) -> Result<usize, Failure> {
        loop {
            if let Some(f) = Failure::budget(self.phase, &self.budget) { return Err(f); }
            let result = match &mut self.tls { Some(tls) => tls.reader().read(into), None => self.socket.0.read(into) };
            match result {
                Ok(n) => return Ok(n),
                Err(e) if transient(&e) => {
                    if self.tls.is_some() { self.read_tls()?; self.flush_tls()?; }
                    else { wait(&self.socket.0, libc::POLLIN, self.phase, &self.budget)?; }
                }
                Err(_) => return Err(Failure::checked(self.phase, &self.budget, "request_failed")),
            }
        }
    }
}
/// CONNECT credentials are emitted only after both listener and this accepted
/// server-side socket have been proven to belong to the exact owner.
fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let a = chunk[0]; let b = *chunk.get(1).unwrap_or(&0); let c = *chunk.get(2).unwrap_or(&0);
        out.push(TABLE[(a >> 2) as usize] as char); out.push(TABLE[(((a & 3) << 4) | (b >> 4)) as usize] as char);
        out.push(if chunk.len() > 1 { TABLE[(((b & 15) << 2) | (c >> 6)) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[(c & 63) as usize] as char } else { '=' });
    }
    out
}
fn read_head(transport: &mut Transport<'_>, recorder: &mut Recorder, record_ttfb: bool) -> Result<Vec<u8>, Failure> {
    let mut bytes = Vec::with_capacity(1024); let mut byte = [0u8;1];
    while !bytes.ends_with(b"\r\n\r\n") {
        if bytes.len() == 16 << 10 { return Err(Failure::stage(transport.phase, "http_headers_limit")); }
        let n = transport.read(&mut byte)?;
        if n == 0 { return Err(Failure::stage(transport.phase, "http_response_failed")); }
        if bytes.is_empty() && record_ttfb { recorder.end("ttfb"); }
        bytes.push(byte[0]);
    }
    Ok(bytes)
}
fn head_status(head: &[u8]) -> Result<u16, Failure> {
    let mut headers = [httparse::EMPTY_HEADER;64]; let mut response = httparse::Response::new(&mut headers);
    if !matches!(response.parse(head), Ok(httparse::Status::Complete(n)) if n == head.len()) {
        return Err(Failure::stage("response", "http_response_failed"));
    }
    let status = response.code.ok_or(Failure::stage("response", "http_response_failed"))?;
    if !(100..=599).contains(&status) { return Err(Failure::stage("response", "http_response_failed")); }
    Ok(status)
}
struct ResponseReader<'r,'a> { prefix: &'r [u8], transport: &'r mut Transport<'a>, failure: &'r Cell<Option<Failure>> }
impl Read for ResponseReader<'_, '_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if !self.prefix.is_empty() {
            let n = buffer.len().min(self.prefix.len()); buffer[..n].copy_from_slice(&self.prefix[..n]); self.prefix = &self.prefix[n..]; return Ok(n);
        }
        match self.transport.read(buffer) {
            Ok(n) => Ok(n), Err(f) => { self.failure.set(Some(f)); Err(io::Error::other("diagnostic transport failed")) },
        }
    }
}
fn await_owned(route: &ProxyRoute, socket: &TcpStream, io: &mut impl Backend, b: &Budget<'_>) -> Result<(), Failure> {
    let remote = socket.local_addr().map_err(|_| Failure::stage("connect", "proxy_owner_changed"))?;
    let deadline = b.deadline.min(Instant::now() + Duration::from_millis(250));
    loop {
        if route.owner.verify(route.address, Some(remote), io, b).is_ok() { return Ok(()); }
        if let Some(f) = Failure::budget("connect", b) { return Err(f); }
        if Instant::now() >= deadline { return Err(Failure::stage("connect", "proxy_owner_changed")); }
        // A bounded native poll yields without a resolver thread or worker.
        let _ = unsafe { libc::poll(std::ptr::null_mut(), 0, 2) };
    }
}
fn observe(source: &SourcePolicy, url: &Url, proxy: Option<&ProxyRoute>, io: &mut impl Backend, budget: &Budget<'_>, body_limit: usize) -> Observation {
    let mut out = Observation { recorder: Recorder::new(proxy.is_some()), status: None, bytes: 0, limit: false, peer: None, failure: None, redirect: None };
    let run = (|| -> Result<(), Failure> {
        let addresses = if let Some(route) = proxy {
            route.owner.verify(route.address, None, io, budget).map_err(|_| Failure::checked("connect", budget, "proxy_owner_changed"))?;
            vec![route.address]
        } else {
            if url.host().parse::<IpAddr>().is_err() { out.recorder.begin("dns"); }
            // Reuse trusted policy and its literal-bootstrap endpoint_dns path.
            let endpoints = source.selection_endpoints(url.host(), budget);
            if url.host().parse::<IpAddr>().is_err() { out.recorder.end("dns"); }
            let endpoints=endpoints.map_err(|_| Failure::checked("dns", budget, "dns_failed"))?;
            endpoints.into_iter().map(|host| host.parse::<IpAddr>().map(|ip| SocketAddr::new(ip, url.port()))
                .map_err(|_| Failure::stage("dns", "dns_failed"))).collect::<Result<Vec<_>,_>>()?
        };
        let mut socket = None;
        for address in addresses {
            if let Some(f) = Failure::budget("tcp", budget) { return Err(f); }
            out.recorder.begin("tcp");
            let result = readiness_dns::connect_literal(address, budget.deadline, Some(budget.cancel));
            out.recorder.end("tcp");
            if let Ok(connected) = result { socket = Some(Socket::new(connected)?); break; }
        }
        let socket = socket.ok_or_else(|| Failure::checked("tcp", budget, "tcp_failed"))?;
        out.peer = Some(socket.0.peer_addr().map_err(|_| Failure::stage("tcp", "tcp_failed"))?.to_string());
        let mut transport = Transport { socket, tls: None, budget: *budget, phase: "connect", tls_bytes: 0 };
        if let Some(route) = proxy {
            await_owned(route, &transport.socket.0, io, budget)?;
            let authority = if url.host().contains(':') { format!("[{}]:{}",url.host(),url.port()) } else { format!("{}:{}",url.host(),url.port()) };
            let auth = route.credentials.as_ref().map(|(user, pass)| format!("Proxy-Authorization: Basic {}\r\n", base64(format!("{user}:{pass}").as_bytes()))).unwrap_or_default();
            out.recorder.begin("connect");
            transport.write(format!("CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\n{auth}\r\n").as_bytes())?;
            let head = read_head(&mut transport, &mut out.recorder, false)?;
            let status = head_status(&head).map_err(|_| Failure::stage("connect", "proxy_connect_failed"))?;
            out.recorder.end("connect");
            if status != 200 { return Err(Failure::stage("connect", "proxy_connect_failed")); }
            route.owner.identity(io, budget).map_err(|_| Failure::checked("connect", budget, "proxy_owner_changed"))?;
        }
        out.recorder.begin("tls"); let handshake=transport.handshake(url.host()); out.recorder.end("tls"); handshake?;
        transport.phase = "request";
        transport.write(format!("GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: be6500panel-network-diagnostic\r\nAccept: */*\r\nAccept-Encoding: identity\r\nConnection: close\r\n\r\n", url.path_and_query(), url.authority()).as_bytes())?;
        out.recorder.begin("ttfb"); transport.phase = "ttfb";
        let mut head = read_head(&mut transport, &mut out.recorder, true)?;
        let mut interim = 0;
        while head_status(&head)? < 200 {
            interim += 1;
            if interim > 8 || head_status(&head)? == 101 { return Err(Failure::stage("response", "http_response_failed")); }
            head = read_head(&mut transport, &mut out.recorder, false)?;
        }
        let status = head_status(&head)?; out.status = Some(status);
        let framing_failure = Cell::new(None);
        transport.phase = "transfer";
        let reader = ResponseReader { prefix: &head, transport: &mut transport, failure: &framing_failure };
        let response = artifact_http::parse_response(reader, budget).map_err(|_| framing_failure.get().unwrap_or_else(|| Failure::checked("response", budget, "http_response_failed")))?;
        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            // Legacy presets do not follow redirects. Record actual Location
            // presence and admission without exposing arbitrary origin URLs.
            let accepted = response.location().is_some_and(|location| url.resolve_location(location).is_ok());
            out.redirect = Some(json!({"observed":true,"followed":false,"locationPresent":response.location().is_some(),"admitted":accepted}));
            if !accepted { return Err(Failure::stage("redirect", "redirect_refused")); }
        }
        let mut body = response.into_body();
        out.recorder.begin("transfer");
        let mut bytes = [0u8;8192];
        while out.bytes < body_limit {
            let n = body.read(&mut bytes[..8192.min(body_limit-out.bytes)]).map_err(|_| framing_failure.get().unwrap_or_else(|| Failure::checked("transfer", budget, "response_body_failed")))?;
            if n == 0 { break; } out.bytes += n;
        }
        if out.bytes == body_limit {
            let mut extra = [0u8;1];
            out.limit = body.read(&mut extra).map_err(|_| framing_failure.get().unwrap_or_else(|| Failure::checked("transfer", budget, "response_body_failed")))? != 0;
        }
        out.recorder.end("transfer"); drop(body);
        if let Some(failure) = framing_failure.get() { return Err(failure); }
        if let Some(route) = proxy {
            route.owner.identity(io, budget).map_err(|_| Failure::checked("transfer", budget, "proxy_owner_changed"))?;
        }
        if let Some(f) = Failure::budget("transfer", budget) { return Err(f); }
        if !(200..300).contains(&status) {
            return Err(Failure { phase: "response", code: "http_status", outcome: "http_error" });
        }
        Ok(())
    })();
    if let Err(f) = run { out.failure = Some(f); }
    out
}

fn compile_probe(nodes: &[Node], port: u16, password: &str) -> Result<Vec<u8>, ApiError> {
    if nodes.is_empty() || nodes.len() > MAX_NODES || port == 0 || password.len() != 64 { return Err(artifact_error()); }
    let resolver = json!({"server":"probe-dns","timeout":"3s","strategy":"prefer_ipv4"});
    let mut seen = BTreeSet::new(); let mut users = Vec::new(); let mut outbounds = Vec::new(); let mut rules = Vec::new();
    for (i, node) in nodes.iter().enumerate() {
        if !valid_id(&node.id) || !seen.insert(node.id.as_str()) || crate::native::validate_node(node).is_err() { return Err(artifact_error()); }
        let tag = format!("probe-{i}");
        users.push(json!({"username":tag,"password":password}));
        rules.push(json!({"auth_user":[tag],"outbound":tag}));
        outbounds.push(json!({"type":"vless","tag":tag,"server":node.server,"server_port":node.port,"uuid":node.uuid,
            "flow":node.flow,"packet_encoding":"xudp","domain_resolver":resolver,"connect_timeout":"3s","tcp_fast_open":true,
            "tls":{"enabled":true,"server_name":node.server_name,"utls":{"enabled":true,"fingerprint":node.fingerprint},
                "reality":{"enabled":true,"public_key":node.reality_public_key,"short_id":node.reality_short_id}}}));
    }
    outbounds.push(json!({"type":"direct","tag":"probe-bootstrap","domain_resolver":resolver}));
    rules.push(json!({"action":"reject"}));
    let doc = json!({"log":{"disabled":true},"dns":{"servers":[{"type":"tls","tag":"probe-dns","server":"223.5.5.5","server_port":853,
        "tls":{"enabled":true,"server_name":"dns.alidns.com"},"detour":"probe-bootstrap"}],"final":"probe-dns","cache_capacity":256,"timeout":"3s","strategy":"prefer_ipv4"},
        "inbounds":[{"type":"mixed","tag":"probe-in","listen":"127.0.0.1","listen_port":port,"users":users}],
        "outbounds":outbounds,"route":{"rules":rules,"auto_detect_interface":true,"default_domain_resolver":resolver}});
    let mut bytes = serde_json::to_vec(&doc).map_err(|_| artifact_error())?;
    if bytes.len() > crate::native::MAX_CONFIG_BYTES { return Err(artifact_error()); }
    bytes.push(b'\n'); Ok(bytes)
}
struct LeasedFile { path: PathBuf, file: File, identity: FileIdentity }
impl LeasedFile {
    fn unchanged(&self) -> bool {
        self.file.metadata().is_ok_and(|m| FileIdentity::from_metadata(&m) == self.identity)
            && fs::symlink_metadata(&self.path).is_ok_and(|m| FileIdentity::from_metadata(&m) == self.identity)
    }
    fn unlink(&self) -> Result<(), ApiError> {
        match fs::symlink_metadata(&self.path) {
            Ok(m) if (m.dev(),m.ino()) == (self.identity.device,self.identity.inode) && m.is_file() => fs::remove_file(&self.path).map_err(|_| artifact_error()),
            Err(e) if e.kind() == io::ErrorKind::NotFound && self.file.metadata().is_ok_and(|m| m.nlink() == 0) => Ok(()),
            _ => Err(artifact_error()),
        }
    }
}
struct Session {
    dir: PathBuf, directory: File, dir_id: FileIdentity, binary: Option<LeasedFile>, config: Option<LeasedFile>,
    child: Option<Child>, checking: bool, stopping: bool, owner: Option<ProcOwner>, address: SocketAddr,
    password: String, deadline: Instant,
}
impl Session {
    fn create(candidate: &Candidate, nodes: &[Node], budget: &Budget<'_>) -> Result<Self, ApiError> {
        check(budget)?;
        if !candidate.unchanged() { return Err(artifact_error()); }
        let dir = candidate.temporary.join(format!("be6500panel-node-probe-{}", token()?));
        fs::DirBuilder::new().mode(0o700).create(&dir).map_err(|_| artifact_error())?;
        let directory = match File::open(&dir) { Ok(directory)=>directory, Err(_)=>{let _=fs::remove_dir(&dir);return Err(artifact_error());} };
        let mut stat=std::mem::MaybeUninit::<libc::statvfs>::uninit();
        if unsafe { libc::fstatvfs(directory.as_raw_fd(),stat.as_mut_ptr()) }!=0 { let _=fs::remove_dir(&dir);return Err(artifact_error()); }
        let stat=unsafe { stat.assume_init() };
        let free=(stat.f_bavail as u128).checked_mul(stat.f_frsize as u128).ok_or_else(artifact_error)?;
        if stat.f_frsize==0 || free<(candidate.identity.size as u128)+(2<<20) { let _=fs::remove_dir(&dir);return Err(artifact_error()); }
        let dir_id = FileIdentity::from_metadata(&directory.metadata().map_err(|_| artifact_error())?);
        let mut session = Self { dir, directory, dir_id, binary: None, config: None, child: None, checking: true, stopping: false,
            owner: None, address: "127.0.0.1:1".parse().expect("fixed socket"), password: String::new(), deadline: Instant::now() + START_TIMEOUT };
        let prepared = (|| {
            let path = session.dir.join("sing-box");
            let mut file = OpenOptions::new().read(true).write(true).create_new(true).mode(0o700)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC).open(&path).map_err(|_| artifact_error())?;
            // Retain before copying, so every failed copy has exact-inode cleanup.
            session.binary = Some(LeasedFile { path: path.clone(), file: file.try_clone().map_err(|_| artifact_error())?,
                identity: FileIdentity::from_metadata(&file.metadata().map_err(|_| artifact_error())?) });
            let sha = hash_file(&candidate.file, candidate.identity.size, budget, Some(&mut file))?;
            let metadata = file.metadata().map_err(|_| artifact_error())?;
            session.binary.as_mut().ok_or_else(artifact_error)?.identity = FileIdentity::from_metadata(&metadata);
            if sha != candidate.sha || !candidate.unchanged() { return Err(artifact_error()); }
            let listener = TcpListener::bind("127.0.0.1:0").map_err(|_| artifact_error())?;
            session.address = listener.local_addr().map_err(|_| artifact_error())?;
            session.password = format!("{}{}", token()?, token()?);
            let raw = compile_probe(nodes, session.address.port(), &session.password)?;
            let path = session.dir.join("config.json");
            let mut file = OpenOptions::new().read(true).write(true).create_new(true).mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC).open(&path).map_err(|_| artifact_error())?;
            session.config = Some(LeasedFile { path, file: file.try_clone().map_err(|_| artifact_error())?,
                identity: FileIdentity::from_metadata(&file.metadata().map_err(|_| artifact_error())?) });
            file.write_all(&raw).map_err(|_| artifact_error())?;
            session.config.as_mut().ok_or_else(artifact_error)?.identity = FileIdentity::from_metadata(&file.metadata().map_err(|_| artifact_error())?);
            check(budget)?; drop(listener);
            session.spawn(true)?; Ok(())
        })();
        if let Err(e) = prepared { let _ = session.cleanup_files(); return Err(e); }
        Ok(session)
    }
    fn files_unchanged(&self) -> bool {
        self.binary.as_ref().is_some_and(LeasedFile::unchanged) && self.config.as_ref().is_some_and(LeasedFile::unchanged)
            && self.directory.metadata().is_ok_and(|m| same_directory(FileIdentity::from_metadata(&m), self.dir_id))
            && fs::symlink_metadata(&self.dir).is_ok_and(|m| same_directory(FileIdentity::from_metadata(&m), self.dir_id))
    }
    fn spawn(&mut self, checking: bool) -> Result<(), ApiError> {
        if self.child.is_some() || !self.files_unchanged() { return Err(artifact_error()); }
        let binary = self.binary.as_ref().ok_or_else(artifact_error)?;
        let config = self.config.as_ref().ok_or_else(artifact_error)?;
        let mut command = Command::new(&binary.path);
        command.arg(if checking { "check" } else { "run" }).arg("--config").arg(&config.path)
            .env_clear().env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin").env("HOME", &self.dir).env("TMPDIR", &self.dir)
            .env("GOMEMLIMIT", "48MiB").env("GOGC", "50").current_dir(&self.dir)
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        // Linux parent death is fixed lifecycle protection, not process adoption.
        #[cfg(target_os = "linux")]
        unsafe { command.pre_exec(|| {
            let parent = libc::getppid();
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 || libc::getppid() != parent { libc::_exit(127); }
            Ok(())
        }); }
        self.child = Some(command.spawn().map_err(|_| artifact_error())?);
        self.checking = checking; self.owner = None; self.deadline = Instant::now() + START_TIMEOUT; Ok(())
    }
    /// One exact-child observation per tick. No startup worker or waiting loop.
    fn advance(&mut self, io: &mut impl Backend, budget: &Budget<'_>) -> Result<bool, ApiError> {
        check(budget)?;
        if self.stopping || !self.files_unchanged() { return Err(artifact_error()); }
        let child = self.child.as_mut().ok_or_else(artifact_error)?;
        if let Some(status) = child.try_wait().map_err(|_| artifact_error())? {
            self.child = None;
            if !self.checking || !status.success() { return Err(artifact_error()); }
            self.spawn(false)?; return Ok(false);
        }
        if Instant::now() >= self.deadline { return Err(error(504,"core_start_timeout","隔离测速核心未能在截止时间内就绪。")); }
        if self.checking { return Ok(false); }
        if self.owner.is_none() {
            let pid = child.id(); let path = self.binary.as_ref().ok_or_else(artifact_error)?.path.clone();
            match ProcOwner::bind(pid, path, io, budget) { Ok(owner) => self.owner = Some(owner), Err(_) => return Ok(false) }
        }
        let ready = self.owner.as_ref().ok_or_else(artifact_error)?.verify(self.address, None, io, budget).is_ok();
        if ready { self.deadline = Instant::now() + JOB_TIMEOUT; }
        Ok(ready)
    }
    fn route(&mut self, index: usize, io: &mut impl Backend, budget: &Budget<'_>) -> Result<ProxyRoute, ApiError> {
        if self.stopping || self.checking || !self.files_unchanged() || self.child.as_mut().ok_or_else(artifact_error)?.try_wait().map_err(|_| artifact_error())?.is_some() {
            return Err(artifact_error());
        }
        let owner = self.owner.as_ref().ok_or_else(artifact_error)?;
        owner.verify(self.address, None, io, budget)?;
        Ok(ProxyRoute { address: self.address, owner: owner.clone(), credentials: Some((format!("probe-{index}"), self.password.clone())) })
    }
    fn stop(&mut self) { self.stopping = true; }
    /// Kill only the retained Child, then reap cooperatively. Files stay retained
    /// until exact-child reaping succeeds. Never signals a serialized/disk PID.
    fn cleanup(&mut self) -> Result<bool, ApiError> {
        self.stopping = true;
        if let Some(child) = &mut self.child {
            match child.try_wait().map_err(|_| artifact_error())? {
                Some(_) => self.child = None,
                None => { child.kill().map_err(|_| artifact_error())?; return Ok(false); }
            }
        }
        self.cleanup_files()?; Ok(true)
    }
    fn cleanup_files(&mut self) -> Result<(), ApiError> {
        if self.child.is_some() { return Err(artifact_error()); }
        let named = match fs::symlink_metadata(&self.dir) {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(_) => return Err(artifact_error()),
        };
        if !same_directory(FileIdentity::from_metadata(&named), self.dir_id) { return Err(artifact_error()); }
        if let Some(config) = &self.config { config.unlink()?; }
        if let Some(binary) = &self.binary { binary.unlink()?; }
        fs::remove_dir(&self.dir).map_err(|_| artifact_error())?; Ok(())
    }
}
struct ProbeWork { nodes: Vec<Node>, session: Option<Session>, flight: Option<NodeFlight>, deadline: Instant, stop: Option<(&'static str,&'static str)>, index: usize, revision: String }

/// Privately owns DTO/history/session state. It cannot switch the active core,
/// select a subscription node, or modify router capture/rules.
pub struct Diagnostics {
    source: Option<SourcePolicy>, candidate: Option<Candidate>, accepted: Option<Accepted>,
    revision: String, nodes_identity: String, results_revision: String, job: Option<Job>, results: Vec<ResultRow>, cached: Vec<ResultRow>,
    work: Option<ProbeWork>, traces: VecDeque<Trace>, jobs: VecDeque<Value>, sequence: u64, tracing: bool, closed: bool,
    #[cfg(test)] fixture_outcomes: Option<VecDeque<Observation>>,
}
impl fmt::Debug for Diagnostics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str("Diagnostics([private])") }
}
impl Default for Diagnostics { fn default() -> Self { Self::new() } }
impl Diagnostics {
    pub fn new() -> Self {
        Self { source: None, candidate: None, accepted: None, revision: String::new(), nodes_identity: String::new(),
            results_revision: String::new(), job: None, results: Vec::new(), cached: Vec::new(), work: None,
            traces: VecDeque::new(), jobs: VecDeque::new(), sequence: 0, tracing: false, closed: false,
            #[cfg(test)] fixture_outcomes: None }
    }
    /// Takes the same trusted native public-root/literal-bootstrap policy as the
    /// runtime. No implicit resolver defaults or browser-selected DNS addresses.
    pub fn bind_source(&mut self, source: SourcePolicy) { self.source = Some(source); }
    /// Called after authoritative source publication; byte/credential changes
    /// revoke an existing job even if a caller accidentally reuses a revision.
    pub fn bind_nodes(&mut self, revision: &str, nodes: &[Node]) -> Result<(), ApiError> {
        if !valid_id(revision) || nodes.len() > crate::subscription::MAX_NODES { return Err(invalid()); }
        let identity = node_identity(nodes)?;
        if self.revision != revision || self.nodes_identity != identity {
            if let Some(work) = &mut self.work { work.stop = Some(("invalidated","revision_mismatch")); if let Some(session) = &mut work.session { session.stop(); } }
            self.results.clear(); self.cached.clear(); self.job = None; self.jobs.clear();
        }
        self.revision = revision.into(); self.nodes_identity = identity; Ok(())
    }
    pub fn revoke_nodes(&mut self) {
        self.revision.clear(); self.nodes_identity.clear(); self.results.clear(); self.cached.clear(); self.job = None; self.jobs.clear();
        if let Some(work) = &mut self.work { work.stop = Some(("invalidated","revision_mismatch")); if let Some(s) = &mut work.session { s.stop(); } }
    }
    /// Root supplies only an already admitted native candidate. Hash every byte
    /// here and pin inode/permissions, then run native `check` on each job's own
    /// credential-preserving config before launching its isolated mixed listener.
    pub fn bind_candidate(&mut self, path: &Path, sha: [u8;32], temporary_root: &Path, budget: &Budget<'_>) -> Result<(), ApiError> {
        check(budget)?;
        if self.work.is_some() || !path.is_absolute() { return Err(artifact_error()); }
        let (temporary, temporary_id) = private_dir(temporary_root)?;
        let path = path.canonicalize().map_err(|_| artifact_error())?;
        let file = OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(&path).map_err(|_| artifact_error())?;
        let metadata = file.metadata().map_err(|_| artifact_error())?;
        if !metadata.is_file() || metadata.mode() & 0o111 == 0 || metadata.mode() & 0o022 != 0 || metadata.nlink() != 1
            || metadata.len() == 0 || metadata.len() > MAX_BINARY || !(metadata.uid() == 0 || metadata.uid() == unsafe { libc::geteuid() }) { return Err(artifact_error()); }
        let candidate = Candidate { path, file, identity: FileIdentity::from_metadata(&metadata), sha, temporary, temporary_id };
        if hash_file(&candidate.file, metadata.len(), budget, None)? != sha || !candidate.unchanged() { return Err(artifact_error()); }
        self.candidate = Some(candidate); Ok(())
    }
    pub fn revoke_candidate(&mut self) {
        self.candidate = None;
        if let Some(work) = &mut self.work { work.stop = Some(("failed","artifact_unavailable")); if let Some(s) = &mut work.session { s.stop(); } }
    }
    /// Must be called only while the exact Root-owned accepted Run is current.
    /// OwnedRunIdentity cannot be constructed from HTTP/disk PID metadata. The
    /// kernel owner brackets a real TCP/CONNECT/TLS/HTTP request, never "ready"
    /// merely because configuration or socket metadata exists.
    pub fn bind_accepted(&mut self, native: &[u8], run: &OwnedRunIdentity, executable: &Path, verified_lan: &[IpAddr], io: &mut impl Backend, budget: &Budget<'_>) -> Result<(), ApiError> {
        self.accepted = None;
        if native.is_empty() || native.len() > MAX_NATIVE || verified_lan.len() > 32 || verified_lan.iter().any(|ip| !usable(*ip))
            || digest(native) != run.sha256() { return Err(unavailable()); }
        let address = mixed_proxy(native, verified_lan)?;
        let owner = ProcOwner::bind(run.pid(), executable.to_path_buf(), io, budget)?;
        owner.verify(address, None, io, budget)?;
        self.accepted = Some(Accepted { hash: digest(native), owner, verified_lan: verified_lan.to_vec() }); Ok(())
    }
    pub fn revoke_accepted(&mut self) { self.accepted = None; }
    pub fn handle(&mut self, path: &str, method: Method, query: &str, body: &[u8], native: Option<&[u8]>, nodes: &[Node], io: &mut impl Backend, budget: &Budget<'_>) -> Result<Value, ApiError> {
        if !matches!(path, PROBES | TRACES | "/api/proxy/node-probes/history") { return Err(error(404,"not_found","诊断接口不存在。")); }
        if !query.is_empty() { return Err(invalid()); }
        if method != Method::Post && !body.is_empty() { return Err(invalid()); }
        match (path, method) {
            (PROBES, Method::Get | Method::Head) => Ok(self.snapshot(nodes)),
            ("/api/proxy/node-probes/history", Method::Get | Method::Head) => {
                let jobs:Vec<&Value>=if self.current_nodes(nodes) { self.jobs.iter().filter(|job|job["revision"]==self.revision).collect() } else {Vec::new()};
                Ok(json!({"jobs":jobs,"capacity":JOB_HISTORY}))
            },
            (PROBES, Method::Delete) => {
                if let Some(work) = &mut self.work {
                    work.stop = Some(("cancelled","node_cancelled")); if let Some(session) = &mut work.session { session.stop(); }
                    if let Some(job) = &mut self.job { job.status = "cancelled"; job.error_code = Some("node_cancelled"); }
                }
                Ok(self.snapshot(nodes))
            }
            (PROBES, Method::Post) => { check(budget)?; self.start(body, nodes, io.now_unix())?; Ok(self.snapshot(nodes)) }
            (TRACES, Method::Get | Method::Head) => Ok(self.trace_snapshot()),
            (TRACES, Method::Post) => self.run_trace(body, native, io, budget),
            _ => Err(error(405,"method_not_allowed","Method is not allowed for this endpoint.")),
        }
    }
    fn current_nodes(&self, nodes: &[Node]) -> bool {
        !self.revision.is_empty() && node_identity(nodes).is_ok_and(|id| id == self.nodes_identity)
    }
    fn snapshot(&self, nodes: &[Node]) -> Value {
        let current = self.current_nodes(nodes);
        let code = if self.closed { Some("probes_unavailable") } else if nodes.is_empty() || !current { Some("empty_subscription") }
            else if self.source.is_none() || self.candidate.as_ref().is_none_or(|c| !c.unchanged()) { Some("artifact_unavailable") } else { None };
        let visible = current && self.results_revision == self.revision;
        let results: Vec<&ResultRow> = if visible { self.results.iter().chain(self.cached.iter()).take(MAX_NODES).collect() } else { Vec::new() };
        let mut view = json!({"revision":self.revision,"available":code.is_none(),"target":TARGET,"running":self.work.is_some(),
            "results":results,
            "limits":{"maxNodes":MAX_NODES,"concurrency":1,"timeoutMs":3000}});
        if visible {if let Some(job)=&self.job {view["job"]=json!(job);}}
        if let Some(code) = code { view["unavailableCode"] = json!(code); }
        view
    }
    fn trace_snapshot(&self) -> Value {
        json!({"traces":self.traces,"targets":[{"id":"google204","label":"Google 204","url":TARGET},
            {"id":"cloudflare","label":"Cloudflare","url":"https://www.cloudflare.com/cdn-cgi/trace"}],
            "limits":{"timeoutMs":10000,"bodyBytes":BODY_LIMIT,"concurrency":1,"capacity":CAPACITY},"running":self.tracing})
    }
    fn start(&mut self, body: &[u8], nodes: &[Node], now: u64) -> Result<(), ApiError> {
        if body.len() > 40 << 10 { return Err(error(413,"body_too_large","节点测速请求超过限制。")); }
        let input: Start = object(body)?;
        if !valid_id(&input.revision) || input.revision != self.revision || !self.current_nodes(nodes) {
            return Err(error(409,"revision_mismatch","订阅已变化，请刷新节点列表后再测速。"));
        }
        let bad_nodes = || error(400,"invalid_nodes","请选择当前订阅节点；每批最多 256 个，全部测速不会截断节点。");
        if nodes.is_empty() || input.node_ids.len() > MAX_NODES || (input.all && !input.node_ids.is_empty()) || (!input.all && input.node_ids.is_empty()) {
            return Err(bad_nodes());
        }
        let selected = if input.all {
            if nodes.len() > MAX_NODES { return Err(bad_nodes()); } nodes.to_vec()
        } else {
            let mut seen = BTreeSet::new(); let mut selected = Vec::new();
            for id in input.node_ids.0 {
                if !valid_id(&id) || !seen.insert(id.clone()) { return Err(bad_nodes()); }
                selected.push(nodes.iter().find(|n| n.id == id).ok_or_else(bad_nodes)?.clone());
            }
            selected
        };
        if selected.iter().any(|n| crate::native::validate_node(n).is_err()) { return Err(bad_nodes()); }
        if self.closed { return Err(error(503,"probe_closed","节点测速服务已停止。")); }
        if self.work.is_some() { return Err(error(409,"probe_busy","节点测速正在进行；请先停止当前任务。")); }
        if self.source.is_none() || self.candidate.as_ref().is_none_or(|c| !c.unchanged()) { return Err(artifact_error()); }
        let id = token()?;
        let previous: Vec<ResultRow> = self.results.iter().chain(self.cached.iter()).cloned().collect();
        self.cached.clear();
        if self.results_revision == self.revision {
            let ids: BTreeSet<&str> = selected.iter().map(|n| n.id.as_str()).collect();
            self.cached.extend(previous.into_iter().filter(|r| !ids.contains(r.node_id.as_str()) && r.measured_at.is_some()).take(MAX_NODES-selected.len()));
        }
        self.results_revision = self.revision.clone();
        self.job = Some(Job { id, status: "preparing", total: selected.len(), completed: 0, started_at: timestamp(now), finished_at: None, error_code: None });
        self.results = selected.iter().map(|node| ResultRow { node_id: node.id.clone(), status: "queued", delay_ms: None, measured_at: None,
            target: TARGET, error_code: None, evidence: None }).collect();
        self.work = Some(ProbeWork { nodes: selected, session: None, flight: None, deadline: Instant::now()+JOB_TIMEOUT, stop: None, index: 0, revision: self.revision.clone() });
        Ok(())
    }
    fn run_trace(&mut self, body: &[u8], native: Option<&[u8]>, io: &mut impl Backend, budget: &Budget<'_>) -> Result<Value, ApiError> {
        check(budget)?;
        if body.len() > 1024 { return Err(error(413,"body_too_large","网络诊断请求超过限制。")); }
        let input: TraceInput = object(body)?;
        let (label, target) = match input.target_id.as_str() { "google204" => ("Google 204",TARGET),
            "cloudflare" => ("Cloudflare","https://www.cloudflare.com/cdn-cgi/trace"), _ => return Err(invalid()) };
        if !matches!(input.route.as_str(),"direct"|"proxy") { return Err(invalid()); }
        if self.tracing { return Err(error(409,"request_trace_busy","网络诊断正在进行，请等待当前测试完成。")); }
        let source = self.source.as_ref().ok_or_else(unavailable)?;
        let proxy = if input.route == "proxy" {
            let accepted = self.accepted.as_ref().ok_or_else(unavailable)?; let raw = native.ok_or_else(unavailable)?;
            if raw.len() > MAX_NATIVE || digest(raw) != accepted.hash { return Err(unavailable()); }
            let address = mixed_proxy(raw,&accepted.verified_lan)?;
            accepted.owner.verify(address,None,io,budget)?;
            Some(ProxyRoute { address, owner: accepted.owner.clone(), credentials: None })
        } else { None };
        let url = Url::parse(target,false).map_err(|_| invalid())?;
        self.sequence = self.sequence.checked_add(1).ok_or_else(unavailable)?;
        self.tracing = true;
        let started_at = timestamp(io.now_unix());
        let limited = Budget { deadline: budget.deadline.min(Instant::now()+TRACE_TIMEOUT), cancel: budget.cancel };
        let observed = observe(source,&url,proxy.as_ref(),io,&limited,BODY_LIMIT);
        let total_ms = observed.recorder.start.elapsed().as_secs_f64()*1000.0;
        let failure = observed.failure;
        let trace = Trace { id: self.sequence.to_string(), target_id: input.target_id, target_label: label.into(), url: target.into(),
            route: input.route.clone(), started_at, finished_at: timestamp(io.now_unix()), total_ms,
            outcome: failure.map_or("success",|f| f.outcome), status_code: observed.status, bytes_read: observed.bytes,
            body_limit_reached: observed.limit, peer_address: observed.peer, peer_scope: if proxy.is_some() { "proxy" } else { "origin" },
            failure_phase: failure.map(|f| f.phase), error_code: failure.map(|f| f.code), phases: observed.recorder.phases,
            observed_route: if proxy.is_some() { "proxy" } else { "direct" },
            route_source: if proxy.is_some() { "accepted_native_owned_run" } else { "trusted_literal_bootstrap_direct_socket" }, redirect: observed.redirect };
        let value = serde_json::to_value(&trace).map_err(|_| error(500,"request_trace_failed","无法记录诊断结果。"));
        self.traces.push_front(trace); self.traces.truncate(CAPACITY); self.tracing = false; value
    }
    /// One caller tick advances one stage or probes one node. The job owns its
    /// lifetime; request/page cancellation does not cancel accepted work.
    pub fn tick(&mut self, native: Option<&[u8]>, nodes: &[Node], io: &mut impl Backend, budget: &Budget<'_>) -> Result<(), ApiError> {
        if let Some(accepted) = &self.accepted {
            if native.is_none_or(|raw| raw.len()>MAX_NATIVE || digest(raw)!=accepted.hash) { self.accepted = None; }
        }
        let current = self.current_nodes(nodes);
        let Some(mut work) = self.work.take() else { return Ok(()); };
        if !current || work.revision != self.revision { work.stop = Some(("invalidated","revision_mismatch")); }
        if self.source.is_none() { work.stop=Some(("failed","artifact_unavailable")); }
        if crate::shutdown::requested() || self.closed { work.stop = Some(("cancelled","node_cancelled")); }
        if Instant::now() >= work.deadline && work.stop.is_none() { work.stop = Some(("cancelled","node_cancelled")); }
        // A short root tick budget is not the job deadline. Keep admitted work
        // queued until a caller supplies time, rather than inventing a timeout.
        if budget.check().is_err() && work.stop.is_none() { self.work = Some(work); return Ok(()); }
        if let Some((status,code)) = work.stop {
            work.flight=None;
            if let Some(session) = &mut work.session {
                match session.cleanup() { Ok(true) => {}, Ok(false) => { self.work = Some(work); return Ok(()); },
                    Err(e) => { self.work = Some(work); return Err(e); } }
            }
            self.finish_job(status,if code.is_empty() { None } else { Some(code) },io.now_unix(),work.revision == self.revision && current);
            return Ok(());
        }
        #[cfg(test)]
        if let Some(outcomes)=&mut self.fixture_outcomes {
            if let Some(observed)=outcomes.pop_front() {
                let row=&mut self.results[work.index];
                row.status=if observed.failure.is_none() { "success" } else if observed.failure.is_some_and(|f|f.outcome=="timeout") { "timeout" } else { "unreachable" };
                row.error_code=if row.status=="success" { None } else if row.status=="timeout" { Some("node_timeout") } else { Some("node_unreachable") };
                row.delay_ms=if row.status=="success" { Some(observed.recorder.start.elapsed().as_millis() as u64) } else { None };
                row.measured_at=Some(timestamp(io.now_unix()));
                work.index+=1;
                if let Some(job)=&mut self.job { job.status="running";job.completed=work.index; }
                if work.index==work.nodes.len() { work.stop=Some(("completed","")); }
                self.work=Some(work);return Ok(());
            }
        }
        if work.session.is_none() {
            let result = self.candidate.as_ref().ok_or_else(artifact_error).and_then(|candidate| Session::create(candidate,&work.nodes,budget));
            match result {
                Ok(session) => work.session = Some(session),
                Err(_) => work.stop = Some(("failed","core_unavailable")),
            }
            self.work = Some(work); return Ok(());
        }
        let session = work.session.as_mut().ok_or_else(artifact_error)?;
        if self.job.as_ref().is_some_and(|job| job.status == "preparing") {
            match session.advance(io,budget) {
                Ok(true) => if let Some(job) = &mut self.job { job.status = "running"; },
                Ok(false) => {}, Err(_) => work.stop = Some(("failed","core_unavailable")),
            }
            self.work = Some(work); return Ok(());
        }
        if work.index >= work.nodes.len() { work.stop = Some(("completed","")); self.work=Some(work); return Ok(()); }
        if work.flight.is_none() {
            match session.route(work.index,io,budget).and_then(NodeFlight::new) {
                Ok(flight)=>{
                    if let Some(row)=self.results.get_mut(work.index) {row.status="probing";}
                    work.flight=Some(flight);
                },
                Err(_)=>work.stop=Some(("failed","core_unavailable")),
            }
            self.work=Some(work);return Ok(());
        }
        let observed=work.flight.as_mut().expect("retained node flight").step(io,budget);
        if let Some(observed)=observed {
            work.flight=None;
            let child_current=session.route(work.index,io,budget).is_ok();
            let successful=observed.failure.is_none() && observed.status==Some(204) && !observed.limit && child_current;
            if let Some(row)=self.results.get_mut(work.index) {
                row.measured_at=Some(timestamp(io.now_unix()));
                if successful {row.status="success";row.delay_ms=Some(observed.recorder.start.elapsed().as_millis() as u64);row.error_code=None;}
                else if observed.failure.is_some_and(|f|f.outcome=="timeout") {row.status="timeout";row.error_code=Some("node_timeout");}
                else {row.status="unreachable";row.error_code=Some("node_unreachable");}
                row.evidence=Some(json!({"source":"isolated_native_ephemeral_checker","route":"proxy","statusCode":observed.status,
                    "bytesRead":observed.bytes,"bodyLimitReached":observed.limit,"peerScope":"isolated_proxy",
                    "failurePhase":observed.failure.map(|f|f.phase),"errorCode":observed.failure.map(|f|f.code),
                    "phases":observed.recorder.phases,"ownerUnchanged":child_current}));
            }
            work.index+=1;if let Some(job)=&mut self.job {job.completed=work.index;}
            if work.index==work.nodes.len() {work.stop=Some(("completed",""));}
        }
        self.work=Some(work); Ok(())
    }
    fn finish_job(&mut self, status: &'static str, code: Option<&'static str>, now: u64, visible: bool) {
        if !visible { self.results.clear(); self.cached.clear(); self.job=None; return; }
        for row in &mut self.results {
            if matches!(row.status,"queued"|"probing") {
                row.status=if status=="failed" { "unreachable" } else { "cancelled" };
                row.error_code=Some(if status=="failed" { code.unwrap_or("core_unavailable") } else { "node_cancelled" });
                if status=="failed" { row.measured_at=Some(timestamp(now)); }
            }
        }
        if let Some(job) = &mut self.job {
            job.status=status; job.error_code=code; job.completed=job.total; job.finished_at=Some(timestamp(now));
            self.jobs.push_front(json!({"revision":self.results_revision,"job":job,"results":self.results}));
            self.jobs.truncate(JOB_HISTORY);
        }
    }
    /// Shutdown is also cooperative. Caller must keep this owner until true;
    /// Drop never silently blocks, adopts a process, or claims child cleanup.
    pub fn close(&mut self, io: &mut impl Backend) -> Result<bool, ApiError> {
        self.closed=true; self.accepted=None;
        let Some(mut work)=self.work.take() else { return Ok(true); };
        work.flight=None;
        if let Some(session)=&mut work.session {
            match session.cleanup() { Ok(true)=>{}, Ok(false)=>{self.work=Some(work);return Ok(false);},
                Err(e)=>{self.work=Some(work);return Err(e);} }
        }
        self.finish_job("cancelled",Some("node_cancelled"),io.now_unix(),work.revision==self.revision); Ok(true)
    }
}
fn node_identity(nodes: &[Node]) -> Result<String, ApiError> {
    if nodes.len() > crate::subscription::MAX_NODES { return Err(invalid()); }
    let mut hash=Sha256::new(); let mut seen=BTreeSet::new();
    for node in nodes {
        if !valid_id(&node.id) || !seen.insert(node.id.as_str()) { return Err(invalid()); }
        // Length-delimited fields prevent hash aliasing; none are serialized or
        // logged. A secret rotation with a reused public revision revokes work.
        for field in [&node.id,&node.name,&node.server,&node.uuid,&node.server_name,&node.reality_public_key,&node.reality_short_id,&node.fingerprint,&node.flow] {
            if field.len()>4096 { return Err(invalid()); }
            hash.update((field.len() as u64).to_be_bytes()); hash.update(field.as_bytes());
        }
        hash.update(node.port.to_be_bytes()); hash.update([u8::from(node.udp)]);
    }
    Ok(format!("{:x}",hash.finalize()))
}


/// Node request transport is a cooperative state machine, not a blocking
/// three-second callback. Every tick performs bounded nonblocking work. Each
/// node has one absolute deadline, one owned socket, and one credential route.
#[derive(Clone,Copy)]
enum FlightPhase {Connect,Verify,WriteConnect,ReadConnect,Tls,WriteRequest,ReadHead}
struct NodeFlight {
    route:ProxyRoute,socket:Socket,tls:Option<rustls::ClientConnection>,phase:FlightPhase,
    output:Vec<u8>,sent:usize,head:Vec<u8>,handshake_bytes:usize,deadline:Instant,observed:Option<Observation>,
}
impl NodeFlight {
    fn new(route:ProxyRoute)->Result<Self,ApiError> {
        let mut observed=Observation{recorder:Recorder::new(true),status:None,bytes:0,limit:false,peer:None,failure:None,redirect:None};
        observed.recorder.begin("tcp");
        let stream=connect_nonblocking(route.address).map_err(|_|artifact_error())?;
        let socket=Socket::new(stream).map_err(|_|artifact_error())?;
        Ok(Self{route,socket,tls:None,phase:FlightPhase::Connect,output:Vec::new(),sent:0,head:Vec::with_capacity(1024),
            handshake_bytes:0,deadline:Instant::now()+NODE_TIMEOUT,observed:Some(observed)})
    }
    fn phase_id(&self)->&'static str {
        match self.phase {FlightPhase::Connect=>"tcp",FlightPhase::Verify|FlightPhase::WriteConnect|FlightPhase::ReadConnect=>"connect",
            FlightPhase::Tls=>"tls",FlightPhase::WriteRequest=>"request",FlightPhase::ReadHead=>"ttfb"}
    }
    fn finish(&mut self,failure:Option<Failure>)->Option<Observation> {
        let mut observed=self.observed.take()?;
        observed.failure=failure;Some(observed)
    }
    fn poll(&self,events:libc::c_short)->Result<bool,Failure> {
        let mut p=libc::pollfd{fd:self.socket.0.as_raw_fd(),events,revents:0};
        let n=unsafe{libc::poll(&mut p,1,0)};
        if n<0 {
            if io::Error::last_os_error().kind()==io::ErrorKind::Interrupted {return Ok(false);}
            return Err(Failure::stage(self.phase_id(),"request_failed"));
        }
        if p.revents&(libc::POLLERR|libc::POLLNVAL)!=0 {return Err(Failure::stage(self.phase_id(),"request_failed"));}
        Ok(n>0 && p.revents&(events|libc::POLLHUP)!=0)
    }
    fn tls_flush(&mut self)->Result<bool,Failure> {
        for _ in 0..4 {
            if !self.tls.as_ref().is_some_and(|t|t.wants_write()) {return Ok(true);}
            match self.tls.as_mut().expect("flight TLS").write_tls(&mut self.socket.0) {
                Ok(0)=>return Err(Failure::stage(self.phase_id(),"tls_failed")),Ok(_)=>{},
                Err(e) if transient(&e)=>return Ok(false),Err(_)=>return Err(Failure::stage(self.phase_id(),"tls_failed")),
            }
        }
        Ok(!self.tls.as_ref().is_some_and(|t|t.wants_write()))
    }
    fn tls_read(&mut self)->Result<bool,Failure> {
        let mut limited=(&mut self.socket.0).take(8192);
        match self.tls.as_mut().expect("flight TLS").read_tls(&mut limited) {
            Ok(0)=>Err(Failure::stage(self.phase_id(),"tls_failed")),
            Ok(n)=>{
                if self.tls.as_ref().is_some_and(|t|t.is_handshaking()) {
                    self.handshake_bytes+=n;
                    if self.handshake_bytes>HANDSHAKE_LIMIT {return Err(Failure::stage("tls","tls_failed"));}
                }
                self.tls.as_mut().expect("flight TLS").process_new_packets().map_err(|e|Failure::stage(self.phase_id(),
                    if matches!(e,rustls::Error::InvalidCertificate(_)|rustls::Error::NoCertificatesPresented){"tls_verification_failed"}else{"tls_failed"}))?;
                Ok(true)
            },Err(e) if transient(&e)=>Ok(false),Err(_)=>Err(Failure::stage(self.phase_id(),"tls_failed")),
        }
    }
    fn write_pending(&mut self,tls:bool)->Result<bool,Failure> {
        if self.sent<self.output.len() {
            let result=if tls {self.tls.as_mut().expect("flight TLS").writer().write(&self.output[self.sent..])}else{self.socket.0.write(&self.output[self.sent..])};
            match result {
                Ok(0)=>return Err(Failure::stage(self.phase_id(),"request_failed")),Ok(n)=>self.sent+=n,
                Err(e) if transient(&e)=>return Ok(false),Err(_)=>return Err(Failure::stage(self.phase_id(),"request_failed")),
            }
        }
        let flushed=if tls {self.tls_flush()?} else {true};
        Ok(self.sent==self.output.len() && flushed)
    }
    fn step(&mut self,io:&mut impl Backend,budget:&Budget<'_>)->Option<Observation> {
        if self.observed.is_none(){return None;}
        let stage=self.phase_id();
        if Instant::now()>=self.deadline {
            return self.finish(Some(Failure{phase:stage,code:"timeout",outcome:"timeout"}));
        }
        if crate::shutdown::requested() {
            return self.finish(Some(Failure{phase:stage,code:"cancelled",outcome:"cancelled"}));
        }
        if budget.check().is_err(){return None;}
        let result=self.advance(io,budget);
        match result {Ok(true)=>self.finish(None),Ok(false)=>None,Err(f)=>self.finish(Some(f))}
    }
    fn advance(&mut self,io:&mut impl Backend,budget:&Budget<'_>)->Result<bool,Failure> {
        match self.phase {
            FlightPhase::Connect=>{
                if !self.poll(libc::POLLOUT)? {return Ok(false);}
                if self.socket.0.take_error().map_err(|_|Failure::stage("tcp","tcp_failed"))?.is_some(){return Err(Failure::stage("tcp","tcp_failed"));}
                let observed=self.observed.as_mut().expect("flight observation");observed.recorder.end("tcp");
                observed.peer=Some(self.socket.0.peer_addr().map_err(|_|Failure::stage("tcp","tcp_failed"))?.to_string());
                self.phase=FlightPhase::Verify;Ok(false)
            },
            FlightPhase::Verify=>{
                let remote=self.socket.0.local_addr().map_err(|_|Failure::stage("connect","proxy_owner_changed"))?;
                self.route.owner.identity(io,budget).map_err(|_|Failure::stage("connect","proxy_owner_changed"))?;
                if self.route.owner.verify(self.route.address,Some(remote),io,budget).is_err(){return Ok(false);}
                let auth=self.route.credentials.as_ref().map(|(user,pass)|format!("Proxy-Authorization: Basic {}\r\n",base64(format!("{user}:{pass}").as_bytes()))).unwrap_or_default();
                self.output=format!("CONNECT www.gstatic.com:443 HTTP/1.1\r\nHost: www.gstatic.com:443\r\n{auth}\r\n").into_bytes();
                self.sent=0;self.observed.as_mut().expect("flight observation").recorder.begin("connect");self.phase=FlightPhase::WriteConnect;Ok(false)
            },
            FlightPhase::WriteConnect=>{if self.write_pending(false)? {self.head.clear();self.phase=FlightPhase::ReadConnect;}Ok(false)},
            FlightPhase::ReadConnect=>{
                if !self.read_header(false)?{return Ok(false);}
                let status=head_status(&self.head).map_err(|_|Failure::stage("connect","proxy_connect_failed"))?;
                self.observed.as_mut().expect("flight observation").recorder.end("connect");
                if status!=200{return Err(Failure::stage("connect","proxy_connect_failed"));}
                self.route.owner.identity(io,budget).map_err(|_|Failure::stage("connect","proxy_owner_changed"))?;
                let name=rustls::pki_types::ServerName::try_from("www.gstatic.com".to_owned()).map_err(|_|Failure::stage("tls","tls_failed"))?;
                let mut tls=rustls::ClientConnection::new(tls_config()?,name).map_err(|_|Failure::stage("tls","tls_failed"))?;
                tls.set_buffer_limit(Some(16<<10));self.tls=Some(tls);self.phase=FlightPhase::Tls;
                self.observed.as_mut().expect("flight observation").recorder.begin("tls");Ok(false)
            },
            FlightPhase::Tls=>{
                let _=self.tls_flush()?;
                if self.tls.as_ref().is_some_and(|t|t.is_handshaking()) {let _=self.tls_read()?;return Ok(false);}
                if !self.tls_flush()?{return Ok(false);}
                if self.tls.as_ref().and_then(|t|t.alpn_protocol()).is_some_and(|a|a!=b"http/1.1"){return Err(Failure::stage("tls","tls_failed"));}
                self.observed.as_mut().expect("flight observation").recorder.end("tls");
                self.output=b"GET /generate_204 HTTP/1.1\r\nHost: www.gstatic.com\r\nUser-Agent: be6500panel-node-probe/1\r\nAccept: */*\r\nAccept-Encoding: identity\r\nConnection: close\r\n\r\n".to_vec();
                self.sent=0;self.phase=FlightPhase::WriteRequest;Ok(false)
            },
            FlightPhase::WriteRequest=>{
                if self.write_pending(true)? {self.head.clear();self.observed.as_mut().expect("flight observation").recorder.begin("ttfb");self.phase=FlightPhase::ReadHead;}Ok(false)
            },
            FlightPhase::ReadHead=>{
                if !self.read_header(true)?{return Ok(false);}
                let status=head_status(&self.head)?;
                let observed=self.observed.as_mut().expect("flight observation");observed.status=Some(status);
                if status!=204{return Err(Failure::stage("response","http_status"));}
                // A 204 has no body. Validate actual HTTP framing and any
                // plaintext already buffered after the header, without EOF wait.
                let parsed=artifact_http::parse_response(io::Cursor::new(&self.head),budget)
                    .map_err(|_|Failure::stage("response","http_response_failed"))?;
                let mut body=parsed.into_body();let mut byte=[0u8;1];
                if body.read(&mut byte).map_err(|_|Failure::stage("transfer","response_body_failed"))?!=0{return Err(Failure::stage("transfer","response_body_failed"));}
                observed.recorder.begin("transfer");
                match self.tls.as_mut().expect("flight TLS").reader().read(&mut byte) {
                    Ok(0)=>{},Ok(_)=>{observed.bytes=1;return Err(Failure::stage("transfer","response_body_failed"));},
                    Err(e) if transient(&e)=>{},Err(_)=>return Err(Failure::stage("transfer","response_body_failed")),
                }
                observed.recorder.end("transfer");
                self.route.owner.identity(io,budget).map_err(|_|Failure::stage("transfer","proxy_owner_changed"))?;Ok(true)
            },
        }
    }
    fn read_header(&mut self,tls:bool)->Result<bool,Failure> {
        for _ in 0..1024 {
            if self.head.len()>=16<<10{return Err(Failure::stage(self.phase_id(),"http_headers_limit"));}
            let mut byte=[0u8;1];
            let result=if tls {self.tls.as_mut().expect("flight TLS").reader().read(&mut byte)}else{self.socket.0.read(&mut byte)};
            match result {
                Ok(0)=>return Err(Failure::stage(self.phase_id(),"http_response_failed")),
                Ok(_)=>{
                    if tls && self.head.is_empty(){self.observed.as_mut().expect("flight observation").recorder.end("ttfb");}
                    self.head.push(byte[0]);if self.head.ends_with(b"\r\n\r\n"){return Ok(true);}
                },
                Err(e) if transient(&e)=>{if tls {let _=self.tls_read()?;let _=self.tls_flush()?;}return Ok(false);},
                Err(_)=>return Err(Failure::stage(self.phase_id(),"http_response_failed")),
            }
        }
        Ok(false)
    }
}
fn connect_nonblocking(address:SocketAddr)->io::Result<TcpStream> {
    use std::os::fd::{FromRawFd,OwnedFd};
    let fd=unsafe{libc::socket(if address.is_ipv4(){libc::AF_INET}else{libc::AF_INET6},libc::SOCK_STREAM,0)};
    if fd<0{return Err(io::Error::last_os_error());}
    let socket=TcpStream::from(unsafe{OwnedFd::from_raw_fd(fd)});socket.set_nonblocking(true)?;
    let result=match address {
        SocketAddr::V4(a)=>{
            let mut raw:libc::sockaddr_in=unsafe{std::mem::zeroed()};raw.sin_family=libc::AF_INET as libc::sa_family_t;
            #[cfg(target_vendor="apple")] {raw.sin_len=std::mem::size_of::<libc::sockaddr_in>() as u8;}
            raw.sin_port=a.port().to_be();raw.sin_addr.s_addr=u32::from_ne_bytes(a.ip().octets());
            unsafe{libc::connect(socket.as_raw_fd(),(&raw as *const libc::sockaddr_in).cast(),std::mem::size_of_val(&raw) as libc::socklen_t)}
        },
        SocketAddr::V6(a)=>{
            let mut raw:libc::sockaddr_in6=unsafe{std::mem::zeroed()};raw.sin6_family=libc::AF_INET6 as libc::sa_family_t;
            #[cfg(target_vendor="apple")] {raw.sin6_len=std::mem::size_of::<libc::sockaddr_in6>() as u8;}
            raw.sin6_port=a.port().to_be();raw.sin6_addr.s6_addr=a.ip().octets();raw.sin6_scope_id=a.scope_id();
            unsafe{libc::connect(socket.as_raw_fd(),(&raw as *const libc::sockaddr_in6).cast(),std::mem::size_of_val(&raw) as libc::socklen_t)}
        },
    };
    if result<0 {
        let error=io::Error::last_os_error();
        if !matches!(error.raw_os_error(),Some(libc::EINPROGRESS|libc::EALREADY|libc::EWOULDBLOCK|libc::EINTR)){return Err(error);}
    }
    Ok(socket)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::product_io::{Error, Output, Program, Metadata};
    use std::{collections::BTreeMap, sync::atomic::{AtomicBool,AtomicU64,Ordering}};
    static NEXT: AtomicU64=AtomicU64::new(0);
    #[derive(Default)]
    struct Fake {
        reads: BTreeMap<PathBuf,Vec<u8>>, links: BTreeMap<PathBuf,PathBuf>, lists: BTreeMap<PathBuf,Vec<String>>,
        metadata: BTreeMap<PathBuf,(u64,u64,u64,u32,u32)>, calls: usize,
    }
    impl Backend for Fake {
        fn read(&mut self,path:&Path,limit:usize,_:&Budget<'_>)->Result<Vec<u8>,Error> {
            self.calls+=1; let bytes=self.reads.get(path).ok_or(Error::Unavailable)?;
            if bytes.len()>limit {return Err(Error::Limit);} Ok(bytes.clone())
        }
        fn run(&mut self,_:Program,_:&[String],_:Option<&[u8]>,_:usize,_:&Budget<'_>)->Result<Output,Error> {panic!("diagnostics must not invoke arbitrary command backend")}
        fn now_unix(&self)->u64 {1_700_000_000}
        fn list(&mut self,path:&Path,limit:usize,_:&Budget<'_>)->Result<Vec<String>,Error> {
            self.calls+=1;let list=self.lists.get(path).ok_or(Error::Unavailable)?;
            if list.len()>limit{return Err(Error::Limit);}Ok(list.clone())
        }
        fn read_link(&mut self,path:&Path,_:&Budget<'_>)->Result<PathBuf,Error> {self.calls+=1;self.links.get(path).cloned().ok_or(Error::Unavailable)}
        fn metadata(&mut self,path:&Path,_:bool,_:&Budget<'_>)->Result<Metadata,Error> {
            self.calls+=1;let &(dev,ino,size,mode,uid)=self.metadata.get(path).ok_or(Error::Unavailable)?;
            Ok(Metadata{regular:true,directory:false,symlink:false,dev,ino,size,mode,uid})
        }
    }
    fn budget(cancel:&AtomicBool)->Budget<'_> {Budget{deadline:Instant::now()+Duration::from_secs(5),cancel}}
    fn node(index:usize)->Node {
        Node{id:format!("node-{index}"),name:format!("private node-{index}"),server:"198.51.100.1".into(),port:443,
            uuid:"00000000-0000-0000-0000-000000000000".into(),server_name:"www.example.com".into(),
            reality_public_key:"A".repeat(43),reality_short_id:"abcd".into(),fingerprint:"chrome".into(),flow:"xtls-rprx-vision".into(),udp:true}
    }
    struct Fixture {root:PathBuf,binary:PathBuf}
    impl Fixture {
        fn new()->Self {
            let root=std::env::temp_dir().canonicalize().unwrap().join(format!("b6p-diagnostic-source-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
            fs::DirBuilder::new().mode(0o700).create(&root).unwrap();let binary=root.join("sing-box");
            let mut file=OpenOptions::new().create_new(true).write(true).mode(0o700).open(&binary).unwrap();
            file.write_all(b"fixture bytes never executed").unwrap();Self{root,binary}
        }
        fn diagnostics(&self,nodes:&[Node])->Diagnostics {
            let cancel=AtomicBool::new(false);let b=budget(&cancel);let mut d=Diagnostics::new();
            d.bind_source(SourcePolicy::native("127.0.0.1:53".parse().unwrap()).unwrap());
            d.bind_nodes("rev-one",nodes).unwrap();
            d.bind_candidate(&self.binary,Sha256::digest(b"fixture bytes never executed").into(),&self.root,&b).unwrap();d
        }
    }
    impl Drop for Fixture {fn drop(&mut self){fs::remove_dir_all(&self.root).unwrap();}}
    fn success()->Observation {
        Observation{recorder:Recorder::new(true),status:Some(204),bytes:0,limit:false,peer:None,failure:None,redirect:None}
    }
    fn owner_fixture()->(ProcOwner,Fake) {
        let mut f=Fake::default();let executable=PathBuf::from("/trusted/native/sing-box");
        let stat=format!("42 (process with ) name) {}",(0..20).map(|i| if i==0{"S".into()}else if i==19{"12345".into()}else{"0".into()}).collect::<Vec<String>>().join(" "));
        f.reads.insert("/proc/42/stat".into(),stat.into_bytes());f.links.insert("/proc/42/exe".into(),executable.clone());
        let stamp=(1,2,40,0o100700,0);f.metadata.insert(executable.clone(),stamp);f.metadata.insert("/proc/42/exe".into(),stamp);
        f.lists.insert("/proc/42/fd".into(),vec!["7".into()]);f.links.insert("/proc/42/fd/7".into(),"socket:[789]".into());
        let cancel=AtomicBool::new(false);let owner=ProcOwner::bind(42,executable,&mut f,&budget(&cancel)).unwrap();(owner,f)
    }
    fn tcp_line(state:&str,local:&str,remote:&str,inode:&str)->Vec<u8> {
        format!("sl local_address rem_address st tx_queue tr tm->when retrnsmt uid timeout inode\n 0: {local} {remote} {state} 0:0 00:0 0 0 0 {inode}\n").into_bytes()
    }
    #[test]
    fn strict_inputs_are_objects_unique_and_bounded() {
        for raw in [b"null".as_slice(),b"[]",br#"[false,[],"r"]"#,br#"{"revision":"r","revision":"other"}"#,br#"{"revision":"r","argv":[]}"#] {
            assert!(object::<Start>(raw).is_err());
        }
        let raw=serde_json::to_vec(&json!({"all":false,"nodeIds":vec!["n";257],"revision":"r"})).unwrap();
        assert!(object::<Start>(&raw).is_err());
        assert!(object::<TraceInput>(br#"{"targetId":"google204","route":"direct","url":"https://secret"}"#).is_err());
        let input=object::<Start>(br#"{"all":true,"nodeIds":null,"revision":"r"}"#).unwrap();assert!(input.node_ids.is_empty());
    }
    #[test]
    fn mixed_requires_exact_literal_single_plain_accepted_inbound() {
        let raw=br#"{"inbounds":[{"type":"mixed","listen":"127.0.0.1","listen_port":1080}]}"#;
        assert_eq!(mixed_proxy(raw,&[]).unwrap(),"127.0.0.1:1080".parse::<SocketAddr>().unwrap());
        for raw in [br#"{"inbounds":[{"type":"mixed","listen":"0.0.0.0","listen_port":1080}]}"#.as_slice(),
            br#"{"inbounds":[{"type":"mixed","listen":"127.0.0.1","listen_port":1080,"users":[{"username":"private","password":"secret"}]}]}"#,
            br#"{"inbounds":[{"type":"mixed","listen":"127.0.0.1","listen_port":1080,"tls":{"enabled":true}}]}"#,
            br#"{"inbounds":[{"type":"mixed","listen":"127.0.0.1","listen_port":1080},{"type":"mixed","listen":"127.0.0.1","listen_port":1081}]}"#,
            br#"{"inbounds":[{"type":"mixed","listen":"::ffff:127.0.0.1","listen_port":1080}]}"#,
            br#"{"inbounds":[{"type":"mixed","listen":"127.0.0.1","listen_port":1080,"listen_port":1081}]}"#] {assert!(mixed_proxy(raw,&[]).is_err());}
        let lan=br#"{"inbounds":[{"type":"mixed","listen":"192.168.31.1","listen_port":1080}]}"#;
        assert!(mixed_proxy(lan,&[]).is_err());assert!(mixed_proxy(lan,&["192.168.31.1".parse().unwrap()]).is_ok());
        let mut doc=json!({"inbounds":[]});doc["inbounds"]=json!((0..65).map(|_|json!({"type":"mixed","listen":"127.0.0.1","listen_port":1080})).collect::<Vec<_>>());
        assert!(mixed_proxy(&serde_json::to_vec(&doc).unwrap(),&[]).is_err());
    }
    #[test]
    fn listener_and_accepted_socket_need_child_owned_inode_and_unchanged_identity() {
        let (owner,mut f)=owner_fixture();let cancel=AtomicBool::new(false);let b=budget(&cancel);
        let address="127.0.0.1:1080".parse().unwrap();let peer="127.0.0.1:50000".parse().unwrap();
        f.reads.insert("/proc/42/net/tcp".into(),tcp_line("0A",&proc_address(address),"00000000:0000","789"));
        assert!(owner.verify(address,None,&mut f,&b).is_ok());assert!(owner.verify(address,Some(peer),&mut f,&b).is_err());
        f.reads.insert("/proc/42/net/tcp".into(),tcp_line("01",&proc_address(address),&proc_address(peer),"789"));
        assert!(owner.verify(address,Some(peer),&mut f,&b).is_ok());
        f.links.insert("/proc/42/fd/7".into(),"socket:[999]".into());assert!(owner.verify(address,Some(peer),&mut f,&b).is_err());
        f.links.insert("/proc/42/fd/7".into(),"socket:[789]".into());f.metadata.get_mut(&PathBuf::from("/proc/42/exe")).unwrap().1=333;
        assert!(owner.verify(address,Some(peer),&mut f,&b).is_err());
    }
    #[test]
    fn tcp_proc_encoding_is_host_endian_and_scope_safe() {
        if cfg!(target_endian="little") {assert_eq!(proc_address("127.0.0.1:1080".parse().unwrap()),"0100007F:0438");}
        assert!(mixed_proxy(br#"{"inbounds":[{"type":"mixed","listen":"fe80::1%eth0","listen_port":1080}]}"#,&[]).is_err());
    }
    #[test]
    fn isolated_compiler_preserves_all_credentials_and_never_inherits_capture() {
        let nodes=vec![node(0),node(1)];let bytes=compile_probe(&nodes,23456,&"b".repeat(64)).unwrap();let doc:Value=serde_json::from_slice(&bytes).unwrap();
        assert_eq!(doc["inbounds"].as_array().unwrap().len(),1);assert_eq!(doc["inbounds"][0]["listen"],"127.0.0.1");
        assert_eq!(doc["outbounds"][0]["uuid"],nodes[0].uuid);assert_eq!(doc["outbounds"][1]["tls"]["reality"]["public_key"],nodes[1].reality_public_key);
        assert_eq!(doc["route"]["rules"][2]["action"],"reject");assert!(doc.get("experimental").is_none());
        assert_eq!(doc["dns"]["servers"][0]["type"],"tls");assert_eq!(doc["outbounds"][0]["flow"],"xtls-rprx-vision");
        assert!(compile_probe(&[node(0),node(0)],23456,&"b".repeat(64)).is_err());
        assert!(!String::from_utf8(bytes).unwrap().contains("tun"));
    }
    #[test]
    fn get_is_pure_post_queues_and_cancel_never_executes_fixture_bytes() {
        let f=Fixture::new();let nodes=vec![node(0),node(1)];let mut d=f.diagnostics(&nodes);let mut io=Fake::default();
        let cancel=AtomicBool::new(false);let b=budget(&cancel);
        for _ in 0..8 {let view=d.handle(PROBES,Method::Get,"",&[],None,&nodes,&mut io,&b).unwrap();assert!(view["available"].as_bool().unwrap());}
        assert_eq!(io.calls,0);assert_eq!(fs::read_dir(&f.root).unwrap().count(),1);
        let raw=serde_json::to_vec(&json!({"all":true,"nodeIds":[],"revision":"rev-one"})).unwrap();
        let view=d.handle(PROBES,Method::Post,"",&raw,None,&nodes,&mut io,&b).unwrap();assert_eq!(view["job"]["status"],"preparing");assert_eq!(view["results"][0]["status"],"queued");
        assert_eq!(d.handle(PROBES,Method::Post,"",&raw,None,&nodes,&mut io,&b).unwrap_err().code,"probe_busy");
        d.handle(PROBES,Method::Delete,"",&[],None,&nodes,&mut io,&b).unwrap();d.tick(None,&nodes,&mut io,&b).unwrap();
        let view=d.snapshot(&nodes);assert_eq!(view["running"],false);assert_eq!(view["job"]["status"],"cancelled");assert_eq!(view["job"]["completed"],2);
        for row in view["results"].as_array().unwrap() {assert!(row.get("delayMs").is_none());assert!(row.get("measuredAt").is_none());}
        assert_eq!(fs::read_dir(&f.root).unwrap().count(),1);assert_eq!(io.calls,0);
    }
    #[test]
    fn one_fixture_observation_per_tick_partial_cache_and_cancel_history() {
        let f=Fixture::new();let nodes=vec![node(0),node(1),node(2)];let mut d=f.diagnostics(&nodes);let mut io=Fake::default();
        let cancel=AtomicBool::new(false);let b=budget(&cancel);
        d.start(br#"{"all":true,"nodeIds":[],"revision":"rev-one"}"#,&nodes,io.now_unix()).unwrap();
        d.fixture_outcomes=Some(VecDeque::from([success(),success(),success()]));
        d.tick(None,&nodes,&mut io,&b).unwrap();assert_eq!(d.snapshot(&nodes)["job"]["completed"],1);
        d.handle(PROBES,Method::Delete,"",&[],None,&nodes,&mut io,&b).unwrap();d.tick(None,&nodes,&mut io,&b).unwrap();
        let view=d.snapshot(&nodes);assert_eq!(view["results"][0]["status"],"success");assert_eq!(view["results"][1]["status"],"cancelled");
        d.start(br#"{"all":false,"nodeIds":["node-1"],"revision":"rev-one"}"#,&nodes,io.now_unix()).unwrap();
        d.fixture_outcomes=Some(VecDeque::from([success()]));d.tick(None,&nodes,&mut io,&b).unwrap();d.tick(None,&nodes,&mut io,&b).unwrap();
        let view=d.snapshot(&nodes);assert_eq!(view["results"][0]["nodeId"],"node-1");assert_eq!(view["results"][1]["nodeId"],"node-0");assert_eq!(view["job"]["status"],"completed");
        assert_eq!(d.jobs.len(),2);assert_eq!(io.calls,0);assert_eq!(fs::read_dir(&f.root).unwrap().count(),1);
    }
    #[test]
    fn new_subscription_hides_results_and_credential_rotation_revokes_jobs() {
        let f=Fixture::new();let mut nodes=vec![node(0),node(1)];let mut d=f.diagnostics(&nodes);let mut io=Fake::default();
        let cancel=AtomicBool::new(false);let b=budget(&cancel);
        d.start(br#"{"all":true,"revision":"rev-one"}"#,&nodes,io.now_unix()).unwrap();
        nodes[0].reality_short_id="cdef".into();assert_eq!(d.snapshot(&nodes)["results"],json!([]));
        d.bind_nodes("rev-one",&nodes).unwrap();d.tick(None,&nodes,&mut io,&b).unwrap();
        assert!(d.work.is_none());assert!(d.job.is_none());assert!(d.results.is_empty());
        assert_eq!(node_identity(&nodes).unwrap(),d.nodes_identity);assert_eq!(fs::read_dir(&f.root).unwrap().count(),1);
    }
    #[test]
    fn maximum_batch_is_never_silently_truncated_and_page_from_large_source_is_valid() {
        let f=Fixture::new();let nodes:Vec<Node>=(0..257).map(node).collect();let mut d=f.diagnostics(&nodes);
        assert_eq!(d.start(br#"{"all":true,"revision":"rev-one"}"#,&nodes,1).unwrap_err().code,"invalid_nodes");
        for raw in [br#"{"all":false,"nodeIds":[],"revision":"rev-one"}"#.as_slice(),br#"{"all":true,"nodeIds":["node-0"],"revision":"rev-one"}"#,
            br#"{"nodeIds":["node-0","node-0"],"revision":"rev-one"}"#,br#"{"nodeIds":["missing"],"revision":"rev-one"}"#] {
            assert_eq!(d.start(raw,&nodes,1).unwrap_err().code,"invalid_nodes");
        }
        d.start(br#"{"nodeIds":["node-256"],"revision":"rev-one"}"#,&nodes,1).unwrap();assert_eq!(d.results.len(),1);
        let mut io=Fake::default();assert!(d.close(&mut io).unwrap());
    }
    #[test]
    fn candidate_integrity_and_permissions_are_admission_not_executable_health() {
        let f=Fixture::new();let nodes=vec![node(0)];let mut d=f.diagnostics(&nodes);let cancel=AtomicBool::new(false);let b=budget(&cancel);
        assert!(d.bind_candidate(&f.binary,[0;32],&f.root,&b).is_err());
        fs::write(&f.binary,b"replaced").unwrap();assert_eq!(d.snapshot(&nodes)["unavailableCode"],"artifact_unavailable");
        assert!(d.start(br#"{"all":true,"revision":"rev-one"}"#,&nodes,1).is_err());
        assert_eq!(format!("{d:?}"),"Diagnostics([private])");
    }
    #[test]
    fn absent_evidence_is_null_and_transport_error_stages_are_distinct() {
        let recorder=Recorder::new(true);let wire=serde_json::to_value(&recorder.phases).unwrap();
        for phase in wire.as_array().unwrap() {assert_eq!(phase["observed"],false);assert!(phase["startMs"].is_null());assert!(phase["durationMs"].is_null());}
        assert_eq!(wire[0]["reason"],"proxy_origin_dns_not_observable");
        let mut r=Recorder::new(false);r.begin("dns");r.end("dns");r.begin("tcp");r.end("tcp");r.begin("tls");
        let wire=serde_json::to_value(&r.phases).unwrap();assert!(wire[0]["durationMs"].as_f64().unwrap()>=0.0);assert!(wire[3]["endMs"].is_null());
        assert_ne!(Failure::stage("dns","dns_failed"),Failure::stage("tls","tls_verification_failed"));
        let cancel=AtomicBool::new(true);assert_eq!(Failure::budget("tls",&budget(&cancel)).unwrap().outcome,"cancelled");
    }

    #[test]
    fn retained_job_history_and_public_result_count_are_bounded() {
        let f=Fixture::new();let nodes=vec![node(0)];let mut d=f.diagnostics(&nodes);let mut io=Fake::default();
        let cancel=AtomicBool::new(false);let b=budget(&cancel);
        for _ in 0..JOB_HISTORY+5 {
            d.start(br#"{"all":true,"revision":"rev-one"}"#,&nodes,io.now_unix()).unwrap();
            d.fixture_outcomes=Some(VecDeque::from([success()]));d.tick(None,&nodes,&mut io,&b).unwrap();d.tick(None,&nodes,&mut io,&b).unwrap();
        }
        assert_eq!(d.jobs.len(),JOB_HISTORY);assert!(d.snapshot(&nodes)["results"].as_array().unwrap().len()<=MAX_NODES);
        let history=d.handle("/api/proxy/node-probes/history",Method::Get,"",&[],None,&nodes,&mut io,&b).unwrap();
        assert_eq!(history["jobs"].as_array().unwrap().len(),JOB_HISTORY);
        d.bind_nodes("rev-two",&nodes).unwrap();assert!(d.jobs.is_empty());
        assert_eq!(io.calls,0);assert_eq!(fs::read_dir(&f.root).unwrap().count(),1);
    }
    #[test]
    fn expired_caller_tick_budget_does_not_fabricate_node_timeout_or_launch() {
        let f=Fixture::new();let nodes=vec![node(0)];let mut d=f.diagnostics(&nodes);let mut io=Fake::default();
        d.start(br#"{"all":true,"revision":"rev-one"}"#,&nodes,io.now_unix()).unwrap();
        let cancel=AtomicBool::new(false);let b=Budget{deadline:Instant::now()-Duration::from_secs(1),cancel:&cancel};
        d.tick(None,&nodes,&mut io,&b).unwrap();let view=d.snapshot(&nodes);
        assert_eq!(view["job"]["completed"],0);assert_eq!(view["results"][0]["status"],"queued");
        assert!(d.work.as_ref().unwrap().session.is_none());assert!(d.work.as_ref().unwrap().flight.is_none());
        assert!(d.close(&mut io).unwrap());assert_eq!(io.calls,0);
    }
    #[test]
    fn credentials_encoding_and_url_redirect_rules_are_exact() {
        assert_eq!(base64(b"probe-0:secret"),"cHJvYmUtMDpzZWNyZXQ=");
        let url=Url::parse(TARGET,false).unwrap();assert!(url.resolve_location("http://www.gstatic.com/").is_err());
        assert!(url.resolve_location("https://user:password@example.com/").is_err());
        assert_eq!(url.resolve_location("/next").unwrap().host(),"www.gstatic.com");
    }
}

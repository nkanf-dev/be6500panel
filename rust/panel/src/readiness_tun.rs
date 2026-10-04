//! Read-only owned TUN observations. No process adoption, signalling, repair,
//! shell, DNS lookup, or waiting loop. Callers retain the actual process handle.
//! Native procfs is Linux-only; alternate proc roots are for synthetic fixtures.
//! Listener/DNS startup proof is an explicit cooperative final callback, not a
//! default success. The callback must use the supplied absolute budget.
use crate::runtime_process::ServiceId;
use serde::Deserialize;
use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};
#[cfg(target_os = "linux")]
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt;
use std::fs::{self, Metadata, OpenOptions};
use std::io::Read;
use std::net::{IpAddr, Ipv4Addr};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

pub const MAX_BYTES: usize = 4 << 20;
pub const MAX_FDS: usize = 8192;
const MAX_INTERFACES: usize = 4096;
const SMALL_BYTES: usize = 64 << 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunError {
    Config,
    Address,
    Ipv6Policy,
    LegacyTproxy,
    Identity,
    IdentityChanged,
    Interface,
    ReversePath,
    Descriptor,
    Socket,
    Collision,
    Observation,
    Limit,
    Unavailable,
    ListenerNotWired,
    Deadline,
    Cancelled,
}
impl fmt::Display for TunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Config => "accepted managed TUN configuration invalid",
            Self::Address => "accepted managed TUN address invalid",
            Self::Ipv6Policy => "managed TUN requires unconditional direct IPv6",
            Self::LegacyTproxy => "legacy TPROXY configuration is not supported",
            Self::Identity => "owned process identity unavailable",
            Self::IdentityChanged => "owned process identity changed",
            Self::Interface => "owned TUN interface invalid or unavailable",
            Self::ReversePath => "owned TUN reverse path observation invalid",
            Self::Descriptor => "owned TUN descriptor proof unavailable",
            Self::Socket => "owned TUN private listener proof invalid",
            Self::Collision => "managed TUN name or prefix is occupied",
            Self::Observation => "read-only TUN observation invalid",
            Self::Limit => "read-only TUN observation exceeds limit",
            Self::Unavailable => "native read-only observation unavailable",
            Self::ListenerNotWired => "listener and DNS readiness callback not wired",
            Self::Deadline => "read-only TUN observation deadline exceeded",
            Self::Cancelled => "read-only TUN observation cancelled",
        })
    }
}
impl std::error::Error for TunError {}

#[derive(Clone, Copy)]
pub struct Budget<'a> {
    pub deadline: Instant,
    pub cancel: &'a AtomicBool,
}
impl Budget<'_> {
    pub fn check(&self) -> Result<(), TunError> {
        if self.cancel.load(Ordering::Relaxed) {
            Err(TunError::Cancelled)
        } else if Instant::now() >= self.deadline {
            Err(TunError::Deadline)
        } else {
            Ok(())
        }
    }
}
impl fmt::Debug for Budget<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Budget([bounded])")
    }
}

/// An IPv4 prefix retaining host bits (the target is first-host /30).
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Ipv4Prefix {
    pub address: Ipv4Addr,
    pub bits: u8,
}
impl Ipv4Prefix {
    pub fn new(address: Ipv4Addr, bits: u8) -> Result<Self, TunError> {
        if bits > 32 {
            return Err(TunError::Address);
        }
        Ok(Self { address, bits })
    }
    fn mask(self) -> u32 {
        if self.bits == 0 {
            0
        } else {
            u32::MAX << (32 - self.bits)
        }
    }
    fn network(self) -> u32 {
        u32::from(self.address) & self.mask()
    }
    pub fn overlaps(self, other: Self) -> bool {
        if self.bits > 32 || other.bits > 32 {
            return true; // Invalid public observations cannot prove no collision.
        }
        let mask = if self.bits < other.bits {
            self.mask()
        } else {
            other.mask()
        };
        (u32::from(self.address) & mask) == (u32::from(other.address) & mask)
    }
}
impl fmt::Display for Ipv4Prefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.address, self.bits)
    }
}
impl fmt::Debug for Ipv4Prefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Ipv4Prefix([private])")
    }
}
fn parse_prefix(text: &str) -> Result<Ipv4Prefix, TunError> {
    let (ip, bits) = text.split_once('/').ok_or(TunError::Address)?;
    let address = ip.parse::<Ipv4Addr>().map_err(|_| TunError::Address)?;
    let bits = bits.parse::<u8>().map_err(|_| TunError::Address)?;
    let result = Ipv4Prefix::new(address, bits)?;
    if result.to_string() != text {
        return Err(TunError::Address);
    }
    Ok(result)
}
#[derive(Clone, PartialEq, Eq)]
pub struct TunTarget {
    name: String,
    address: Ipv4Prefix,
}
impl TunTarget {
    pub fn interface_name(&self) -> &str {
        &self.name
    }
    pub fn address(&self) -> Ipv4Prefix {
        self.address
    }
    pub fn peer(&self) -> Ipv4Addr {
        Ipv4Addr::from(u32::from(self.address.address) + 1)
    }
}
impl fmt::Debug for TunTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TunTarget([private])")
    }
}

// Select typed fields without cloning the private JSON tree. Ignored arbitrary
// non-TUN values are streamed; selected maps reject duplicate keys.
#[derive(Deserialize)]
struct Accepted {
    #[serde(default, deserialize_with = "bounded_inbounds")]
    inbounds: Vec<Inbound>,
    #[serde(default)]
    route: Route,
}
#[derive(Default, Deserialize)]
struct Route {
    #[serde(default, deserialize_with = "bounded_rules")]
    rules: Vec<Rule>,
}
fn bounded_list<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
    limit: usize,
) -> Result<Vec<T>, D::Error> {
    struct V<T>(usize, std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Visitor<'de> for V<T> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("bounded list")
        }
        fn visit_seq<S: SeqAccess<'de>>(self, mut s: S) -> Result<Vec<T>, S::Error> {
            let mut values = Vec::new();
            while values.len() < self.0 {
                match s.next_element()? {
                    Some(v) => values.push(v),
                    None => return Ok(values),
                }
            }
            if s.next_element::<de::IgnoredAny>()?.is_some() {
                return Err(de::Error::custom("list exceeds limit"));
            }
            Ok(values)
        }
    }
    d.deserialize_seq(V(limit, std::marker::PhantomData))
}
fn bounded_inbounds<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Inbound>, D::Error> {
    bounded_list(d, MAX_INTERFACES)
}
fn bounded_rules<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Rule>, D::Error> {
    bounded_list(d, MAX_FDS)
}
#[derive(Default)]
struct Rule {
    fields: usize,
    version: Option<u64>,
    direct: bool,
}
impl<'de> Deserialize<'de> for Rule {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Rule;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("route rule")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut m: M) -> Result<Rule, M::Error> {
                let mut r = Rule::default();
                let mut keys = BTreeSet::new();
                while let Some(k) = m.next_key::<String>()? {
                    if !keys.insert(k.clone()) {
                        return Err(de::Error::custom("duplicate rule field"));
                    }
                    r.fields += 1;
                    match k.as_str() {
                        "ip_version" => {
                            r.version = Some(m.next_value()?);
                        }
                        "outbound" => {
                            r.direct = m.next_value::<String>()? == "direct";
                        }
                        _ => {
                            m.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(r)
            }
        }
        d.deserialize_map(V)
    }
}
#[derive(Default)]
struct Inbound {
    keys: BTreeSet<String>,
    kind: String,
    tag: String,
    name: String,
    address: Option<Vec<String>>,
    mtu: Option<u64>,
    stack: Option<String>,
    dns: Option<String>,
    auto_route: Option<bool>,
    auto_redirect: Option<bool>,
    timeout: Option<String>,
    nat: Option<u64>,
    bad: bool,
}
// Known fields on arbitrary non-TUN inbounds may have different shapes. Raw
// values are limited by accepted bytes; only selected fields are allocated.
impl<'de> Deserialize<'de> for Inbound {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Inbound;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("inbound")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut m: M) -> Result<Inbound, M::Error> {
                let mut r = Inbound::default();
                while let Some(k) = m.next_key::<String>()? {
                    if !r.keys.insert(k.clone()) {
                        return Err(de::Error::custom("duplicate inbound field"));
                    }
                    macro_rules! selected {
                        ($field:ident,$ty:ty) => {{
                            let value = m.next_value::<serde_json::Value>()?;
                            match serde_json::from_value::<$ty>(value) {
                                Ok(v) => r.$field = Some(v),
                                Err(_) => r.bad = true,
                            }
                        }};
                    }
                    match k.as_str() {
                        "type" => r.kind = m.next_value()?,
                        "tag" => r.tag = m.next_value()?,
                        "interface_name" => {
                            let v = m.next_value::<serde_json::Value>()?;
                            if let Some(s) = v.as_str() {
                                r.name = s.to_owned();
                            } else {
                                r.bad = true;
                            }
                        }
                        "address" => selected!(address, Vec<String>),
                        "mtu" => selected!(mtu, u64),
                        "stack" => selected!(stack, String),
                        "dns_mode" => selected!(dns, String),
                        "auto_route" => selected!(auto_route, bool),
                        "auto_redirect" => selected!(auto_redirect, bool),
                        "udp_timeout" => selected!(timeout, String),
                        "udp_nat_max" => selected!(nat, u64),
                        _ => {
                            m.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(r)
            }
        }
        d.deserialize_map(V)
    }
}
fn valid_name(name: &str) -> bool {
    let Some(s) = name.strip_prefix("b6p-") else {
        return false;
    };
    let mut bytes = s.bytes();
    (1..=11).contains(&s.len())
        && bytes
            .next()
            .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
pub fn native_target(raw: &[u8]) -> Result<Option<TunTarget>, TunError> {
    if raw.len() > MAX_BYTES {
        return Err(TunError::Limit);
    }
    let c: Accepted = serde_json::from_slice(raw).map_err(|_| TunError::Config)?;
    if c.inbounds.len() > MAX_INTERFACES || c.route.rules.len() > MAX_FDS {
        return Err(TunError::Limit);
    }
    let mut target = None;
    for i in c.inbounds {
        if i.kind == "tproxy" {
            return Err(TunError::LegacyTproxy);
        }
        if i.kind != "tun" && i.tag != "tun-in" {
            continue;
        }
        if target.is_some()
            || i.keys.len() != 11
            || i.bad
            || i.kind != "tun"
            || i.tag != "tun-in"
            || !valid_name(&i.name)
            || i.mtu != Some(1500)
            || i.stack.as_deref() != Some("system")
            || i.dns.as_deref() != Some("disabled")
            || i.auto_route != Some(false)
            || i.auto_redirect != Some(false)
            || i.timeout.as_deref() != Some("2m")
            || i.nat != Some(1024)
        {
            return Err(TunError::Config);
        }
        let addresses = i.address.ok_or(TunError::Address)?;
        if addresses.len() != 1 {
            return Err(TunError::Address);
        }
        let address = parse_prefix(&addresses[0])?;
        if !address.address.is_private()
            || address.bits != 30
            || u32::from(address.address) != address.network() + 1
        {
            return Err(TunError::Address);
        }
        target = Some(TunTarget {
            name: i.name,
            address,
        });
    }
    if target.is_some() {
        let mut direct = false;
        for rule in c.route.rules {
            if rule.version == Some(6) {
                if !rule.direct || rule.fields != 2 {
                    return Err(TunError::Ipv6Policy);
                }
                direct = true;
            }
        }
        if !direct {
            return Err(TunError::Ipv6Policy);
        }
    }
    Ok(target)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct FileIdentity {
    pub device: u64,
    pub inode: u64,
    pub mode: u32,
    pub uid: u32,
    pub links: u64,
    pub size: u64,
    pub modified: (i64, i64),
    pub changed: (i64, i64),
}
impl FileIdentity {
    pub fn from_metadata(m: &Metadata) -> Self {
        Self {
            device: m.dev(),
            inode: m.ino(),
            mode: m.mode(),
            uid: m.uid(),
            links: m.nlink(),
            size: m.len(),
            modified: (m.mtime(), m.mtime_nsec()),
            changed: (m.ctime(), m.ctime_nsec()),
        }
    }
    fn private_file(self) -> bool {
        self.mode & u32::from(libc::S_IFMT) == u32::from(libc::S_IFREG)
            && self.mode & 0o7777 == 0o700
            && self.uid == unsafe { libc::geteuid() }
            && self.inode > 0
            && self.links == 1
            && self.size > 0
            && self.size <= 40 << 20
    }
    fn private_directory(self) -> bool {
        self.mode & u32::from(libc::S_IFMT) == u32::from(libc::S_IFDIR)
            && self.mode & 0o7777 == 0o700
            && self.uid == unsafe { libc::geteuid() }
            && self.inode > 0
    }
    fn same_directory(self, other: Self) -> bool {
        self.device == other.device
            && self.inode == other.inode
            && self.mode == other.mode
            && self.uid == other.uid
    }
}
impl fmt::Debug for FileIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FileIdentity([private])")
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnedStatus {
    pub service: ServiceId,
    pub pid: u32,
    pub generation: u64,
    pub running: bool,
}
impl OwnedStatus {
    fn validate(self) -> Result<(), TunError> {
        if self.service != ServiceId::SingBox
            || self.pid == 0
            || self.pid > i32::MAX as u32
            || !self.running
        {
            Err(TunError::Identity)
        } else {
            Ok(())
        }
    }
}
#[derive(Clone)]
pub struct OwnedIdentity {
    owner: OwnedStatus,
    start: u64,
    root: PathBuf,
    path: PathBuf,
    file: FileIdentity,
    directory: FileIdentity,
}
impl fmt::Debug for OwnedIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OwnedIdentity([private])")
    }
}
fn controlled_path(root: &Path, path: &Path) -> bool {
    let clean = |p: &Path| {
        p.is_absolute()
            && p.components()
                .all(|c| matches!(c, Component::RootDir | Component::Normal(_)))
            && !p
                .as_os_str()
                .as_encoded_bytes()
                .iter()
                .any(|b| matches!(b, b'\n' | b'\r' | 0))
            && p.as_os_str()
                .as_encoded_bytes()
                .split(|b| *b == b'/')
                .skip(1)
                .all(|part| !part.is_empty() && part != b"." && part != b"..")
    };
    clean(root)
        && clean(path)
        && path.parent() == Some(root)
        && path.file_name().and_then(|s| s.to_str()).is_some_and(|s| {
            s.starts_with(".artifact-")
                && (11..=255).contains(&s.len())
                && !s.ends_with(" (deleted)")
        })
}
impl OwnedIdentity {
    /// Metadata must come from the caller's retained/admitted artifact, never a
    /// browser or disk PID record. This only observes; it grants no PID control.
    #[allow(clippy::too_many_arguments)]
    pub fn bind(
        owner: OwnedStatus,
        root: PathBuf,
        path: PathBuf,
        file: FileIdentity,
        directory: FileIdentity,
        observer: &mut impl Observer,
        deadline: Instant,
        cancel: &AtomicBool,
    ) -> Result<Self, TunError> {
        owner.validate()?;
        if !controlled_path(&root, &path) || !file.private_file() || !directory.private_directory()
        {
            return Err(TunError::Identity);
        }
        let budget = Budget { deadline, cancel };
        budget.check()?;
        let mut value = Self {
            owner,
            start: 0,
            root,
            path,
            file,
            directory,
        };
        value.start = value.sample_identity(observer, &budget)?;
        if value.sample_identity(observer, &budget)? != value.start {
            return Err(TunError::IdentityChanged);
        }
        Ok(value)
    }
    fn sample_identity(
        &self,
        observer: &mut impl Observer,
        b: &Budget<'_>,
    ) -> Result<u64, TunError> {
        b.check()?;
        let base = PathBuf::from(format!("/proc/{}", self.owner.pid));
        let raw = read(observer, &base.join("stat"), SMALL_BYTES, b)?;
        let start = parse_process_start(&raw, self.owner.pid)?;
        let link = observer.read_link(&base.join("exe"), b)?;
        b.check()?;
        if link != self.path || !controlled_path(&self.root, &link) {
            return Err(TunError::Identity);
        }
        let disk = observer.metadata(&self.path, false, b)?;
        b.check()?;
        let process = observer.metadata(&base.join("exe"), true, b)?;
        b.check()?;
        let dir = observer.metadata(&self.root, false, b)?;
        b.check()?;
        if disk != self.file
            || process != self.file
            || !disk.private_file()
            || !dir.private_directory()
            || !self.directory.same_directory(dir)
        {
            return Err(TunError::IdentityChanged);
        }
        let after = read(observer, &base.join("stat"), SMALL_BYTES, b)?;
        if parse_process_start(&after, self.owner.pid)? != start {
            return Err(TunError::IdentityChanged);
        }
        Ok(start)
    }
    fn unchanged(
        &self,
        observer: &mut impl Observer,
        b: &Budget<'_>,
        status: &mut impl FnMut() -> Result<OwnedStatus, TunError>,
    ) -> Result<(), TunError> {
        b.check()?;
        let owner = status()?;
        b.check()?;
        owner.validate()?;
        if owner != self.owner || self.sample_identity(observer, b)? != self.start {
            return Err(TunError::IdentityChanged);
        }
        Ok(())
    }
}
pub fn parse_process_start(raw: &[u8], pid: u32) -> Result<u64, TunError> {
    if raw.len() > SMALL_BYTES {
        return Err(TunError::Limit);
    }
    let text = std::str::from_utf8(raw)
        .map_err(|_| TunError::Identity)?
        .trim();
    let open = text.find('(').ok_or(TunError::Identity)?;
    let close = text.rfind(')').ok_or(TunError::Identity)?;
    if open == 0 || close <= open || text[..open].trim() != pid.to_string() {
        return Err(TunError::Identity);
    }
    let mut fields = text[close + 1..].split_whitespace();
    let state = fields.next().ok_or(TunError::Identity)?;
    if !matches!(state, "R" | "S" | "D" | "T" | "t" | "I" | "W" | "P") {
        return Err(TunError::Identity);
    }
    let value = fields.nth(18).ok_or(TunError::Identity)?;
    let start = value.parse::<u64>().map_err(|_| TunError::Identity)?;
    if start == 0 {
        return Err(TunError::Identity);
    }
    Ok(start)
}

#[derive(Clone, PartialEq, Eq)]
pub struct InterfaceAddress {
    pub address: IpAddr,
    pub bits: u8,
}
impl fmt::Debug for InterfaceAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("InterfaceAddress([private])")
    }
}
#[derive(Clone)]
pub struct Interface {
    pub name: String,
    pub up: bool,
    pub mtu: u32,
    pub addresses: Vec<InterfaceAddress>,
}
impl fmt::Debug for Interface {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Interface([private])")
    }
}
/// Only the local read-only operations needed by this module. Implementations
/// must be bounded and cooperate with Budget; no worker thread hides blocking.
pub trait Observer {
    fn read_file(
        &mut self,
        path: &Path,
        limit: usize,
        budget: &Budget<'_>,
    ) -> Result<Vec<u8>, TunError>;
    fn read_link(&mut self, path: &Path, budget: &Budget<'_>) -> Result<PathBuf, TunError>;
    fn metadata(
        &mut self,
        path: &Path,
        follow: bool,
        budget: &Budget<'_>,
    ) -> Result<FileIdentity, TunError>;
    fn list_dir(
        &mut self,
        path: &Path,
        limit: usize,
        budget: &Budget<'_>,
    ) -> Result<Vec<String>, TunError>;
    fn interfaces(&mut self, budget: &Budget<'_>) -> Result<Vec<Interface>, TunError>;
    /// All IPv4 tables are required. Never return only /proc/net/route or main.
    fn ipv4_routes(&mut self, budget: &Budget<'_>) -> Result<Vec<Ipv4Prefix>, TunError>;
}
fn read(
    o: &mut impl Observer,
    path: &Path,
    limit: usize,
    b: &Budget<'_>,
) -> Result<Vec<u8>, TunError> {
    b.check()?;
    let raw = o.read_file(path, limit, b)?;
    b.check()?;
    if raw.len() > limit {
        return Err(TunError::Limit);
    }
    Ok(raw)
}
fn observed_interfaces(o: &mut impl Observer, b: &Budget<'_>) -> Result<Vec<Interface>, TunError> {
    b.check()?;
    let interfaces = o.interfaces(b)?;
    b.check()?;
    if interfaces.len() > MAX_INTERFACES {
        return Err(TunError::Limit);
    }
    for i in &interfaces {
        if i.name.is_empty() || i.name.len() > 15 || i.addresses.len() > MAX_INTERFACES {
            return Err(TunError::Interface);
        }
        for a in &i.addresses {
            if (a.address.is_ipv4() && a.bits > 32)
                || (a.address.is_ipv6() && a.bits > 128)
                || matches!(a.address,IpAddr::V6(ip) if ip.to_ipv4_mapped().is_some())
            {
                return Err(TunError::Interface);
            }
        }
    }
    Ok(interfaces)
}
fn owned_tun(
    target: &TunTarget,
    identity: &OwnedIdentity,
    o: &mut impl Observer,
    b: &Budget<'_>,
) -> Result<(), TunError> {
    let interfaces = observed_interfaces(o, b)?;
    let mut matching = interfaces.iter().filter(|i| i.name == target.name);
    let interface = matching.next().ok_or(TunError::Interface)?;
    if matching.next().is_some() || !interface.up || interface.mtu != 1500 {
        return Err(TunError::Interface);
    }
    let mut host = false;
    for address in &interface.addresses {
        match address.address {
            IpAddr::V6(ip) if ip.is_unicast_link_local() => continue,
            IpAddr::V4(ip) if !host && ip == target.address.address && address.bits == 30 => {
                host = true
            }
            _ => return Err(TunError::Interface),
        }
    }
    if !host {
        return Err(TunError::Interface);
    }
    let rpf = read(
        o,
        &PathBuf::from(format!("/proc/sys/net/ipv4/conf/{}/rp_filter", target.name)),
        64,
        b,
    )?;
    if std::str::from_utf8(&rpf)
        .map_err(|_| TunError::ReversePath)?
        .trim()
        != "2"
    {
        return Err(TunError::ReversePath);
    }
    let base = PathBuf::from(format!("/proc/{}", identity.owner.pid));
    b.check()?;
    let entries = o.list_dir(&base.join("fd"), MAX_FDS, b)?;
    b.check()?;
    if entries.len() > MAX_FDS {
        return Err(TunError::Limit);
    }
    let mut sockets = BTreeSet::new();
    let mut tun = false;
    let mut names = BTreeSet::new();
    for entry in entries {
        b.check()?;
        if !names.insert(entry.clone()) {
            return Err(TunError::Descriptor);
        }
        let Ok(fd) = entry.parse::<u32>() else {
            continue;
        };
        if fd.to_string() != entry {
            continue;
        }
        let link = o.read_link(&base.join("fd").join(&entry), b)?;
        b.check()?;
        if let Some(s) = link
            .to_str()
            .and_then(|s| s.strip_prefix("socket:["))
            .and_then(|s| s.strip_suffix(']'))
        {
            let inode = s.parse::<u64>().map_err(|_| TunError::Descriptor)?;
            if inode == 0 {
                return Err(TunError::Descriptor);
            }
            sockets.insert(inode);
        }
        if link == Path::new("/dev/net/tun") {
            let raw = read(o, &base.join("fdinfo").join(&entry), SMALL_BYTES, b)?;
            let text = std::str::from_utf8(&raw).map_err(|_| TunError::Descriptor)?;
            let mut iff = None;
            for line in text.lines() {
                if let Some(("iff", value)) = line.split_once(':') {
                    if iff.is_some() {
                        return Err(TunError::Descriptor);
                    }
                    iff = Some(value.trim());
                }
            }
            if iff == Some(target.name.as_str()) {
                tun = true;
            }
        }
    }
    if !tun {
        return Err(TunError::Descriptor);
    }
    let raw = read(o, Path::new("/proc/net/tcp"), MAX_BYTES, b)?;
    let inode = private_tcp_listener(&raw, target.address.address)?;
    if !sockets.contains(&inode) {
        return Err(TunError::Socket);
    }
    Ok(())
}
/// Proc IPv4 numeric fields have native byte order, including on big-endian
/// Linux. Fixture writers should encode with u32::from_ne_bytes.
pub fn private_tcp_listener(raw: &[u8], address: Ipv4Addr) -> Result<u64, TunError> {
    if raw.len() > MAX_BYTES {
        return Err(TunError::Limit);
    }
    let text = std::str::from_utf8(raw).map_err(|_| TunError::Socket)?;
    let mut lines = text.trim().lines();
    let header = lines.next().ok_or(TunError::Socket)?;
    if !header.contains("local_address") || !header.contains("inode") {
        return Err(TunError::Socket);
    }
    let mut found = None;
    for line in lines {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() < 10 {
            return Err(TunError::Socket);
        }
        if fields[3] != "0A" {
            continue;
        }
        let (ip, port) = fields[1].split_once(':').ok_or(TunError::Socket)?;
        if ip.len() != 8 || port.len() != 4 {
            return Err(TunError::Socket);
        }
        let ip = u32::from_str_radix(ip, 16).map_err(|_| TunError::Socket)?;
        let port = u16::from_str_radix(port, 16).map_err(|_| TunError::Socket)?;
        if port == 0 {
            return Err(TunError::Socket);
        }
        if Ipv4Addr::from(ip.to_ne_bytes()) != address || port == 53 {
            continue;
        }
        let inode = fields[9].parse::<u64>().map_err(|_| TunError::Socket)?;
        if inode == 0 || found.replace(inode).is_some() {
            return Err(TunError::Socket);
        }
    }
    found.ok_or(TunError::Socket)
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnedObservation {
    NoTunTarget,
    TunOwned,
}
#[allow(clippy::too_many_arguments)]
pub fn observe_owned_once(
    raw: &[u8],
    identity: &OwnedIdentity,
    observer: &mut impl Observer,
    deadline: Instant,
    cancel: &AtomicBool,
    status: &mut impl FnMut() -> Result<OwnedStatus, TunError>,
) -> Result<OwnedObservation, TunError> {
    let b = Budget { deadline, cancel };
    b.check()?;
    let Some(target) = native_target(raw)? else {
        return Ok(OwnedObservation::NoTunTarget);
    };
    identity.unchanged(observer, &b, status)?;
    let proof = owned_tun(&target, identity, observer, &b);
    identity.unchanged(observer, &b, status)?;
    proof?;
    b.check()?;
    Ok(OwnedObservation::TunOwned)
}
pub type ListenerProbe<'a> = dyn FnMut(&[u8], &Budget<'_>) -> Result<(), TunError> + 'a;

/// One startup attempt, not a wait loop. The callback must prove real listener
/// and selected DNS readiness using the same deadline/cancellation budget.
#[allow(clippy::too_many_arguments)]
pub fn check_startup_once(
    raw: &[u8],
    identity: &OwnedIdentity,
    observer: &mut impl Observer,
    deadline: Instant,
    cancel: &AtomicBool,
    status: &mut impl FnMut() -> Result<OwnedStatus, TunError>,
    listener: Option<&mut ListenerProbe<'_>>,
) -> Result<OwnedObservation, TunError> {
    let b = Budget { deadline, cancel };
    let first = observe_owned_once(raw, identity, observer, deadline, cancel, status)?;
    let listener = listener.ok_or(TunError::ListenerNotWired)?;
    b.check()?;
    let listener_result = listener(raw, &b);
    b.check()?;
    let final_observation = if first == OwnedObservation::TunOwned {
        observe_owned_once(raw, identity, observer, deadline, cancel, status)?
    } else {
        first
    };
    listener_result?;
    Ok(final_observation)
}
pub fn check_prestart_once(
    raw: &[u8],
    observer: &mut impl Observer,
    deadline: Instant,
    cancel: &AtomicBool,
) -> Result<(), TunError> {
    let b = Budget { deadline, cancel };
    b.check()?;
    let Some(target) = native_target(raw)? else {
        return Ok(());
    };
    check_prestart_interfaces(&target, &observed_interfaces(observer, &b)?)?;
    let routes = observer.ipv4_routes(&b)?;
    b.check()?;
    if routes.len() > MAX_BYTES / 8 {
        return Err(TunError::Limit);
    }
    check_prestart_routes(&target, &routes)
}
pub fn check_prestart_interfaces(
    target: &TunTarget,
    interfaces: &[Interface],
) -> Result<(), TunError> {
    if interfaces.len() > MAX_INTERFACES {
        return Err(TunError::Limit);
    }
    for interface in interfaces {
        if interface.name == target.name {
            return Err(TunError::Collision);
        }
        for address in &interface.addresses {
            match address.address {
                IpAddr::V4(ip) => {
                    let prefix = Ipv4Prefix::new(ip, address.bits)?;
                    if target.address.overlaps(prefix) {
                        return Err(TunError::Collision);
                    }
                }
                IpAddr::V6(ip) if address.bits <= 128 && ip.to_ipv4_mapped().is_none() => {}
                _ => return Err(TunError::Observation),
            }
        }
    }
    Ok(())
}
pub fn check_prestart_routes(target: &TunTarget, routes: &[Ipv4Prefix]) -> Result<(), TunError> {
    for route in routes {
        if route.bits > 32 || route.network() != u32::from(route.address) {
            return Err(TunError::Observation);
        }
        if route.bits != 0 && target.address.overlaps(*route) {
            return Err(TunError::Collision);
        }
    }
    Ok(())
}
/// Fixed `ip -4 route show table all` output parser for fixture/parity tests.
/// No command is executed by this module. Native Linux uses netlink directly.
pub fn parse_route_output(raw: &[u8]) -> Result<Vec<Ipv4Prefix>, TunError> {
    if raw.len() > MAX_BYTES {
        return Err(TunError::Limit);
    }
    let text = std::str::from_utf8(raw).map_err(|_| TunError::Observation)?;
    let mut routes = Vec::new();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(mut token) = fields.next() else {
            continue;
        };
        if matches!(
            token,
            "unicast"
                | "local"
                | "broadcast"
                | "multicast"
                | "anycast"
                | "throw"
                | "unreachable"
                | "prohibit"
                | "blackhole"
                | "nat"
        ) {
            token = fields.next().ok_or(TunError::Observation)?;
        }
        if token == "default" {
            continue;
        }
        let prefix = if token.contains('/') {
            parse_prefix(token).map_err(|_| TunError::Observation)?
        } else {
            let ip = token
                .parse::<Ipv4Addr>()
                .map_err(|_| TunError::Observation)?;
            if ip.to_string() != token {
                return Err(TunError::Observation);
            }
            Ipv4Prefix::new(ip, 32)?
        };
        if prefix.network() != u32::from(prefix.address) {
            return Err(TunError::Observation);
        }
        if prefix.bits != 0 {
            routes.push(prefix);
        }
    }
    Ok(routes)
}

pub struct NativeObserver {
    proc_root: PathBuf,
}
impl fmt::Debug for NativeObserver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("NativeObserver([private])")
    }
}
impl Default for NativeObserver {
    fn default() -> Self {
        Self::new()
    }
}
impl NativeObserver {
    pub fn new() -> Self {
        Self {
            proc_root: PathBuf::from("/proc"),
        }
    }
    /// Synthetic Linux-format proc tree. Does not change native interface/route
    /// observations: tests should override those trait methods with fixtures.
    pub fn with_proc_root(root: PathBuf) -> Self {
        Self { proc_root: root }
    }
    fn resolve(&self, path: &Path) -> PathBuf {
        path.strip_prefix("/proc")
            .map_or_else(|_| path.to_path_buf(), |p| self.proc_root.join(p))
    }
}
impl Observer for NativeObserver {
    fn read_file(
        &mut self,
        path: &Path,
        limit: usize,
        b: &Budget<'_>,
    ) -> Result<Vec<u8>, TunError> {
        b.check()?;
        let mut f = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC | libc::O_NOFOLLOW)
            .open(self.resolve(path))
            .map_err(|_| TunError::Unavailable)?;
        if !f.metadata().map_err(|_| TunError::Unavailable)?.is_file() {
            return Err(TunError::Observation);
        }
        let mut raw = Vec::new();
        let mut buffer = [0; 8192];
        loop {
            b.check()?;
            let remaining = (limit + 1 - raw.len()).min(buffer.len());
            let n = f
                .read(&mut buffer[..remaining])
                .map_err(|_| TunError::Unavailable)?;
            if n == 0 {
                break;
            }
            raw.extend_from_slice(&buffer[..n]);
            if raw.len() > limit {
                return Err(TunError::Limit);
            }
        }
        b.check()?;
        Ok(raw)
    }
    fn read_link(&mut self, path: &Path, b: &Budget<'_>) -> Result<PathBuf, TunError> {
        b.check()?;
        let p = fs::read_link(self.resolve(path)).map_err(|_| TunError::Unavailable)?;
        b.check()?;
        if p.as_os_str().len() > SMALL_BYTES {
            return Err(TunError::Limit);
        }
        Ok(p)
    }
    fn metadata(
        &mut self,
        path: &Path,
        follow: bool,
        b: &Budget<'_>,
    ) -> Result<FileIdentity, TunError> {
        b.check()?;
        let p = self.resolve(path);
        let m = if follow {
            fs::metadata(p)
        } else {
            fs::symlink_metadata(p)
        }
        .map_err(|_| TunError::Unavailable)?;
        b.check()?;
        Ok(FileIdentity::from_metadata(&m))
    }
    fn list_dir(
        &mut self,
        path: &Path,
        limit: usize,
        b: &Budget<'_>,
    ) -> Result<Vec<String>, TunError> {
        b.check()?;
        let mut entries = Vec::new();
        for entry in fs::read_dir(self.resolve(path)).map_err(|_| TunError::Unavailable)? {
            b.check()?;
            if entries.len() == limit {
                return Err(TunError::Limit);
            }
            entries.push(
                entry
                    .map_err(|_| TunError::Unavailable)?
                    .file_name()
                    .into_string()
                    .map_err(|_| TunError::Observation)?,
            );
        }
        b.check()?;
        Ok(entries)
    }
    fn interfaces(&mut self, b: &Budget<'_>) -> Result<Vec<Interface>, TunError> {
        native_interfaces(b)
    }
    fn ipv4_routes(&mut self, b: &Budget<'_>) -> Result<Vec<Ipv4Prefix>, TunError> {
        native_routes(b)
    }
}

#[cfg(target_os = "linux")]
fn native_interfaces(b: &Budget<'_>) -> Result<Vec<Interface>, TunError> {
    use std::ffi::CStr;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    b.check()?;
    let mut list = std::ptr::null_mut();
    if unsafe { libc::getifaddrs(&mut list) } != 0 {
        return Err(TunError::Unavailable);
    }
    struct List(*mut libc::ifaddrs);
    impl Drop for List {
        fn drop(&mut self) {
            unsafe { libc::freeifaddrs(self.0) };
        }
    }
    let _list = List(list);
    let fd = unsafe { libc::socket(libc::AF_INET, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, 0) };
    if fd < 0 {
        return Err(TunError::Unavailable);
    }
    let socket = unsafe { OwnedFd::from_raw_fd(fd) };
    let mut map: BTreeMap<String, Interface> = BTreeMap::new();
    let mut node = list;
    let mut count = 0;
    while !node.is_null() {
        b.check()?;
        count += 1;
        if count > MAX_INTERFACES {
            return Err(TunError::Limit);
        }
        let row = unsafe { &*node };
        if row.ifa_name.is_null() {
            return Err(TunError::Interface);
        }
        let n = unsafe { libc::strnlen(row.ifa_name, libc::IFNAMSIZ) };
        if n == 0 || n >= libc::IFNAMSIZ {
            return Err(TunError::Interface);
        }
        let name = unsafe { CStr::from_ptr(row.ifa_name) }
            .to_str()
            .map_err(|_| TunError::Interface)?;
        if !map.contains_key(name) {
            // libc's platform ifreq handles both ARM32 and ARM64 union layout.
            let mut req: libc::ifreq = unsafe { std::mem::zeroed() };
            for (to, from) in req.ifr_name.iter_mut().zip(name.bytes()) {
                *to = from as libc::c_char;
            }
            if unsafe { libc::ioctl(socket.as_raw_fd(), libc::SIOCGIFMTU as _, &mut req) } < 0 {
                return Err(TunError::Unavailable);
            }
            let mtu = unsafe { req.ifr_ifru.ifru_mtu };
            if mtu <= 0 {
                return Err(TunError::Interface);
            }
            map.insert(
                name.to_owned(),
                Interface {
                    name: name.to_owned(),
                    up: row.ifa_flags & libc::IFF_UP as u32 != 0,
                    mtu: mtu as u32,
                    addresses: Vec::new(),
                },
            );
        }
        if !row.ifa_addr.is_null() {
            let family = unsafe { (*row.ifa_addr).sa_family } as i32;
            let pair = match family {
                libc::AF_INET => {
                    if row.ifa_netmask.is_null()
                        || unsafe { (*row.ifa_netmask).sa_family } as i32 != libc::AF_INET
                    {
                        return Err(TunError::Interface);
                    }
                    let a = unsafe { &*(row.ifa_addr as *const libc::sockaddr_in) };
                    let m = unsafe { &*(row.ifa_netmask as *const libc::sockaddr_in) };
                    Some((
                        IpAddr::V4(Ipv4Addr::from(a.sin_addr.s_addr.to_ne_bytes())),
                        mask_bits(&m.sin_addr.s_addr.to_ne_bytes())?,
                    ))
                }
                libc::AF_INET6 => {
                    if row.ifa_netmask.is_null()
                        || unsafe { (*row.ifa_netmask).sa_family } as i32 != libc::AF_INET6
                    {
                        return Err(TunError::Interface);
                    }
                    let a = unsafe { &*(row.ifa_addr as *const libc::sockaddr_in6) };
                    let m = unsafe { &*(row.ifa_netmask as *const libc::sockaddr_in6) };
                    Some((
                        IpAddr::V6(std::net::Ipv6Addr::from(a.sin6_addr.s6_addr)),
                        mask_bits(&m.sin6_addr.s6_addr)?,
                    ))
                }
                _ => None,
            };
            if let Some((address, bits)) = pair {
                map.get_mut(name)
                    .ok_or(TunError::Interface)?
                    .addresses
                    .push(InterfaceAddress { address, bits });
            }
        }
        node = row.ifa_next;
    }
    b.check()?;
    Ok(map.into_values().collect())
}
#[cfg(target_os = "linux")]
fn mask_bits(raw: &[u8]) -> Result<u8, TunError> {
    let mut zero = false;
    let mut bits = 0;
    for byte in raw {
        for shift in (0..8).rev() {
            if byte & (1 << shift) != 0 {
                if zero {
                    return Err(TunError::Interface);
                }
                bits += 1;
            } else {
                zero = true;
            }
        }
    }
    Ok(bits)
}
#[cfg(not(target_os = "linux"))]
fn native_interfaces(b: &Budget<'_>) -> Result<Vec<Interface>, TunError> {
    b.check()?;
    Err(TunError::Unavailable)
}

#[derive(Debug)]
pub struct RouteDumpPart {
    pub routes: Vec<Ipv4Prefix>,
    pub done: bool,
}
/// Parse bounded Linux netlink route dump fixtures. Datagram sender validation
/// is performed by native_routes before calling this pure parser.
pub fn parse_netlink_routes(raw: &[u8], sequence: u32) -> Result<RouteDumpPart, TunError> {
    if raw.len() > MAX_BYTES {
        return Err(TunError::Limit);
    }
    let u16_at = |at: usize| u16::from_ne_bytes([raw[at], raw[at + 1]]);
    let u32_at = |at: usize| u32::from_ne_bytes([raw[at], raw[at + 1], raw[at + 2], raw[at + 3]]);
    let mut at = 0;
    let mut routes = Vec::new();
    let mut done = false;
    while at < raw.len() {
        if done || raw.len() - at < 16 {
            return Err(TunError::Observation);
        }
        let len = u32_at(at) as usize;
        if len < 16 || len > raw.len() - at || u32_at(at + 8) != sequence {
            return Err(TunError::Observation);
        }
        let kind = u16_at(at + 4);
        let flags = u16_at(at + 6);
        if flags & 0x10 != 0 {
            return Err(TunError::Unavailable);
        } // NLM_F_DUMP_INTR
        let end = at + len;
        match kind {
            3 => {
                // NLMSG_DONE may contain a signed status.
                if len != 16 && (len < 20 || u32_at(at + 16) != 0) {
                    return Err(TunError::Unavailable);
                }
                done = true;
            }
            2 => return Err(TunError::Unavailable), // NLMSG_ERROR, including unsolicited ACK
            24 => {
                // RTM_NEWROUTE: 12-byte rtmsg + aligned rtattrs.
                if len < 28 || raw[at + 16] != libc::AF_INET as u8 || raw[at + 17] > 32 {
                    return Err(TunError::Observation);
                }
                let bits = raw[at + 17];
                let mut dest = None;
                let mut attr = at + 28;
                while attr < end {
                    if end - attr < 4 {
                        return Err(TunError::Observation);
                    }
                    let size = u16_at(attr) as usize;
                    let field = u16_at(attr + 2) & 0x3fff;
                    if size < 4 || size > end - attr {
                        return Err(TunError::Observation);
                    }
                    if field == 1 {
                        // RTA_DST is a network-order 4-byte address.
                        if size != 8 || dest.is_some() {
                            return Err(TunError::Observation);
                        }
                        dest = Some(Ipv4Addr::new(
                            raw[attr + 4],
                            raw[attr + 5],
                            raw[attr + 6],
                            raw[attr + 7],
                        ));
                    }
                    let next = attr + ((size + 3) & !3);
                    if next > end {
                        return Err(TunError::Observation);
                    }
                    attr = next;
                }
                let address = if bits == 0 {
                    dest.unwrap_or(Ipv4Addr::UNSPECIFIED)
                } else {
                    dest.ok_or(TunError::Observation)?
                };
                let prefix = Ipv4Prefix::new(address, bits)?;
                if prefix.network() != u32::from(address) {
                    return Err(TunError::Observation);
                }
                if bits != 0 {
                    routes.push(prefix);
                }
            }
            _ => return Err(TunError::Observation),
        }
        let next = at + ((len + 3) & !3);
        if next > raw.len() {
            return Err(TunError::Observation);
        }
        at = next;
    }
    Ok(RouteDumpPart { routes, done })
}
#[cfg(target_os = "linux")]
fn native_routes(b: &Budget<'_>) -> Result<Vec<Ipv4Prefix>, TunError> {
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    b.check()?;
    let fd = unsafe {
        libc::socket(
            libc::AF_NETLINK,
            libc::SOCK_RAW | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
            libc::NETLINK_ROUTE,
        )
    };
    if fd < 0 {
        return Err(TunError::Unavailable);
    }
    let socket = unsafe { OwnedFd::from_raw_fd(fd) };
    let mut address: libc::sockaddr_nl = unsafe { std::mem::zeroed() };
    address.nl_family = libc::AF_NETLINK as libc::sa_family_t;
    if unsafe {
        libc::bind(
            fd,
            &address as *const _ as *const libc::sockaddr,
            std::mem::size_of_val(&address) as libc::socklen_t,
        )
    } < 0
    {
        return Err(TunError::Unavailable);
    }
    // RTM_GETROUTE + NLM_F_REQUEST|NLM_F_DUMP; rtm_table=UNSPEC, so the
    // kernel dumps all IPv4 tables, including local and policy tables.
    let mut request = [0u8; 28];
    request[..4].copy_from_slice(&28u32.to_ne_bytes());
    request[4..6].copy_from_slice(&26u16.to_ne_bytes());
    request[6..8].copy_from_slice(&0x301u16.to_ne_bytes());
    request[8..12].copy_from_slice(&1u32.to_ne_bytes());
    request[16] = libc::AF_INET as u8;
    if unsafe {
        libc::sendto(
            fd,
            request.as_ptr().cast(),
            request.len(),
            0,
            &address as *const _ as *const libc::sockaddr,
            std::mem::size_of_val(&address) as libc::socklen_t,
        )
    } != request.len() as isize
    {
        return Err(TunError::Unavailable);
    }
    let mut buffer = vec![0u8; 65536];
    let mut total = 0usize;
    let mut routes = Vec::new();
    loop {
        b.check()?;
        let remaining = b.deadline.saturating_duration_since(Instant::now());
        let wait = remaining.as_millis().clamp(1, 20) as i32;
        let mut poll = libc::pollfd {
            fd: socket.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut poll, 1, wait) };
        if ready < 0 {
            if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            return Err(TunError::Unavailable);
        }
        if ready == 0 {
            continue;
        }
        if poll.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            return Err(TunError::Unavailable);
        }
        b.check()?;
        let mut sender: libc::sockaddr_nl = unsafe { std::mem::zeroed() };
        let mut length = std::mem::size_of_val(&sender) as libc::socklen_t;
        let n = unsafe {
            libc::recvfrom(
                fd,
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                libc::MSG_TRUNC,
                &mut sender as *mut _ as *mut libc::sockaddr,
                &mut length,
            )
        };
        if n < 0 {
            let errno = std::io::Error::last_os_error().raw_os_error();
            if errno == Some(libc::EINTR) || errno == Some(libc::EAGAIN) {
                continue;
            }
            return Err(TunError::Unavailable);
        }
        if n == 0
            || n as usize > buffer.len()
            || length as usize != std::mem::size_of_val(&sender)
            || sender.nl_family != libc::AF_NETLINK as libc::sa_family_t
            || sender.nl_pid != 0
            || sender.nl_groups != 0
        {
            return Err(TunError::Observation);
        }
        total = total.checked_add(n as usize).ok_or(TunError::Limit)?;
        if total > MAX_BYTES {
            return Err(TunError::Limit);
        }
        let part = parse_netlink_routes(&buffer[..n as usize], 1)?;
        routes.extend(part.routes);
        b.check()?;
        if part.done {
            return Ok(routes);
        }
    }
}
#[cfg(not(target_os = "linux"))]
fn native_routes(b: &Budget<'_>) -> Result<Vec<Ipv4Prefix>, TunError> {
    b.check()?;
    Err(TunError::Unavailable)
}

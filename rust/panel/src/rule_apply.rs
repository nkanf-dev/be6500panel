//! Rule-only compilation from actual accepted bytes. Borrow raw settings so
//! unknown dialer/log/telemetry fields are preserved without whole-tree clones.
//! No process/network action; pinned rule-set verification is separate.
use crate::{
    native::{
        self, CompileInput, CompileOutput, DNSEndpoint, LocalDNSConfig, Node, Ports,
        RoutedTUNConfig, RuleSetReference,
    },
    policy::{Diagnostic, Rule},
    readiness_tun,
};
use serde::{
    Deserialize, Deserializer,
    de::{self, MapAccess, Visitor},
};
use serde_json::value::RawValue;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    io::Write,
    net::IpAddr,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApplyError;
impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("accepted rule configuration cannot be preserved")
    }
}
impl std::error::Error for ApplyError {}
type Result<T> = std::result::Result<T, ApplyError>;
struct Object<'a>(BTreeMap<String, &'a RawValue>);
impl<'de> Deserialize<'de> for Object<'de> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Object<'de>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("unique object")
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut object = BTreeMap::new();
                while let Some(key) = a.next_key::<String>()? {
                    if key.len() > 1024 || object.len() >= 256 || object.contains_key(&key) {
                        return Err(de::Error::custom("object bound or duplicate"));
                    }
                    object.insert(key, a.next_value::<&'de RawValue>()?);
                }
                Ok(Object(object))
            }
        }
        d.deserialize_map(V)
    }
}
fn parse<'a, T: Deserialize<'a>>(raw: &'a [u8]) -> Result<T> {
    serde_json::from_slice(raw).map_err(|_| ApplyError)
}
fn part<'a>(object: &'a Object<'a>, name: &str) -> Result<Object<'a>> {
    parse(object.0.get(name).ok_or(ApplyError)?.get().as_bytes())
}
#[derive(Deserialize)]
struct Accepted<'a> {
    inbounds: Vec<Inbound>,
    outbounds: Vec<Outbound>,
    #[serde(borrow)]
    dns: Dns<'a>,
    #[serde(borrow)]
    route: Route<'a>,
}
#[derive(Deserialize)]
struct Inbound {
    #[serde(rename = "type")]
    kind: String,
    tag: String,
    #[serde(default)]
    listen: String,
    #[serde(default)]
    listen_port: u16,
}
#[derive(Deserialize)]
struct Outbound {
    #[serde(rename = "type")]
    kind: String,
    tag: String,
    #[serde(default)]
    server: String,
    #[serde(default)]
    server_port: u16,
    #[serde(default)]
    uuid: String,
    #[serde(default)]
    flow: String,
    #[serde(default)]
    tls: Option<Tls>,
}
#[derive(Deserialize)]
struct Tls {
    enabled: bool,
    server_name: String,
    utls: Utls,
    reality: Reality,
}
#[derive(Deserialize)]
struct Utls {
    enabled: bool,
    fingerprint: String,
}
#[derive(Deserialize)]
struct Reality {
    enabled: bool,
    public_key: String,
    short_id: String,
}
#[derive(Deserialize)]
struct Dns<'a> {
    servers: Vec<DnsServer>,
    #[serde(default, borrow)]
    rules: Vec<&'a RawValue>,
}
#[derive(Deserialize)]
struct DnsServer {
    #[serde(rename = "type")]
    kind: String,
    tag: String,
    #[serde(default)]
    server: String,
    #[serde(default)]
    server_port: u16,
    #[serde(default)]
    detour: String,
    #[serde(default)]
    tls: Option<DnsTls>,
}
#[derive(Deserialize)]
struct DnsTls {
    enabled: bool,
    server_name: String,
}
#[derive(Deserialize)]
struct Route<'a> {
    #[serde(borrow)]
    rules: Vec<&'a RawValue>,
}
#[derive(Default, Deserialize)]
#[serde(default)]
struct Prelude {
    action: String,
    outbound: String,
    inbound: Vec<String>,
    ip_cidr: Vec<String>,
    domain: Vec<String>,
}
#[derive(Default, Deserialize)]
#[serde(default)]
struct LocalRule {
    server: String,
    domain_suffix: Vec<String>,
    domain: Vec<String>,
    query_type: Vec<String>,
}
fn node_matches(out: &Outbound, node: &Node) -> bool {
    out.kind == "vless"
        && out.server == node.server
        && out.server_port == node.port
        && out.uuid == node.uuid
        && out.flow == node.flow
        && out.tls.as_ref().is_some_and(|tls| {
            tls.enabled
                && tls.server_name == node.server_name
                && tls.utls.enabled
                && tls.utls.fingerprint == node.fingerprint
                && tls.reality.enabled
                && tls.reality.public_key == node.reality_public_key
                && tls.reality.short_id == node.reality_short_id
        })
}
fn intention(accepted: &[u8], nodes: &[Node]) -> Result<CompileInput> {
    if accepted.is_empty() || accepted.len() > native::MAX_CONFIG_BYTES {
        return Err(ApplyError);
    }
    let target = readiness_tun::native_target(accepted)
        .map_err(|_| ApplyError)?
        .ok_or(ApplyError)?;
    let cfg: Accepted = parse(accepted)?;
    if cfg.inbounds.len() != 3
        || cfg.outbounds.len() > 64
        || cfg.dns.servers.len() > 16
        || cfg.route.rules.len() > crate::policy::MAX_RULES * 2 + 128
        || cfg.dns.rules.len() > crate::policy::MAX_RULES * 2 + 128
    {
        return Err(ApplyError);
    }
    let proxy = cfg
        .outbounds
        .iter()
        .filter(|out| out.tag == "proxy")
        .collect::<Vec<_>>();
    if proxy.len() != 1 {
        return Err(ApplyError);
    }
    let mut matched = nodes.iter().filter(|node| node_matches(proxy[0], node));
    let node = matched.next().ok_or(ApplyError)?.clone();
    if matched.next().is_some() {
        return Err(ApplyError);
    }
    let mut input = CompileInput {
        node,
        datapath: "routed-tun".into(),
        routed_tun: Some(RoutedTUNConfig {
            interface_name: target.interface_name().into(),
            address: target.address().to_string(),
        }),
        ipv6: "direct".into(),
        failure: "direct".into(),
        ports: Ports {
            mixed: 0,
            tproxy: 7893,
            dns: 0,
        },
        ..CompileInput::default()
    };
    let mut tags = BTreeSet::new();
    for inbound in &cfg.inbounds {
        if !tags.insert(inbound.tag.as_str()) {
            return Err(ApplyError);
        }
        match (inbound.tag.as_str(), inbound.kind.as_str()) {
            ("mixed-in", "mixed") if !inbound.listen.is_empty() && inbound.listen_port > 0 => {
                input.ports.mixed = inbound.listen_port;
                input.mixed_listen_address = inbound.listen.clone();
            }
            ("dns-in", "direct") if !inbound.listen.is_empty() && inbound.listen_port > 0 => {
                input.ports.dns = inbound.listen_port;
                input.dns_listen_address = inbound.listen.clone();
            }
            ("tun-in", "tun") => {}
            _ => return Err(ApplyError),
        }
    }
    let mut dns_tags = BTreeSet::new();
    let mut local = None;
    for server in &cfg.dns.servers {
        if !dns_tags.insert(server.tag.as_str()) {
            return Err(ApplyError);
        }
        match server.tag.as_str() {
            "dns-direct" | "dns-proxy" => {
                let tls = server.tls.as_ref().ok_or(ApplyError)?;
                if server.kind != "tls"
                    || !tls.enabled
                    || server.detour
                        != if server.tag == "dns-direct" {
                            "direct"
                        } else {
                            "proxy"
                        }
                {
                    return Err(ApplyError);
                }
                let endpoint = DNSEndpoint {
                    server: server.server.clone(),
                    port: server.server_port,
                    server_name: tls.server_name.clone(),
                };
                if server.tag == "dns-direct" {
                    input.direct_dns = endpoint
                } else {
                    input.proxy_dns = endpoint
                }
            }
            "dns-local" => {
                if server.kind != "udp"
                    || server.detour != "direct"
                    || server.server.is_empty()
                    || server.server_port == 0
                {
                    return Err(ApplyError);
                }
                local = Some(LocalDNSConfig {
                    server: server.server.clone(),
                    port: server.server_port,
                    ..LocalDNSConfig::default()
                });
            }
            "dns-fake" if server.kind == "fakeip" => input.fake_ip = true,
            _ => return Err(ApplyError),
        }
    }
    if !dns_tags.contains("dns-direct") || !dns_tags.contains("dns-proxy") {
        return Err(ApplyError);
    }
    let mut local = local.ok_or(ApplyError)?;
    for rule in cfg.dns.rules {
        let rule: LocalRule = parse(rule.get().as_bytes())?;
        if rule.server == "dns-local" && rule.query_type.is_empty() {
            local.domains.extend(rule.domain_suffix);
            local.hostnames.extend(rule.domain);
        }
    }
    if cfg.route.rules.len() < 3 {
        return Err(ApplyError);
    }
    let first: Prelude = parse(cfg.route.rules[0].get().as_bytes())?;
    let bypass: Prelude = parse(cfg.route.rules[1].get().as_bytes())?;
    let bootstrap: Prelude = parse(cfg.route.rules[2].get().as_bytes())?;
    if first.action != "hijack-dns"
        || first.inbound != ["dns-in"]
        || bypass.outbound != "direct"
        || bootstrap.outbound != "direct"
        || bypass.ip_cidr.is_empty()
    {
        return Err(ApplyError);
    }
    for value in bypass.ip_cidr {
        let (address, bits) = value.split_once('/').ok_or(ApplyError)?;
        let ip = address.parse::<IpAddr>().map_err(|_| ApplyError)?;
        if bits.parse::<u8>().map_err(|_| ApplyError)? != if ip.is_ipv4() { 32 } else { 128 } {
            return Err(ApplyError);
        }
        input.endpoints.push(ip.to_string());
    }
    input.bootstrap_domains = bootstrap.domain;
    for value in [
        &input.mixed_listen_address,
        &input.dns_listen_address,
        &local.server,
    ] {
        let ip = value.parse::<IpAddr>().map_err(|_| ApplyError)?;
        if !ip.is_unspecified() && !ip.is_loopback() {
            input.management_ips.push(ip.to_string());
        }
    }
    input.local_dns = Some(local);
    Ok(input)
}
struct Bounded(Vec<u8>);
impl Write for Bounded {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > native::MAX_CONFIG_BYTES.saturating_sub(self.0.len()) {
            return Err(std::io::Error::other("native write bound"));
        }
        let need = self.0.len() + bytes.len();
        if need > self.0.capacity() {
            let cap = self
                .0
                .capacity()
                .saturating_mul(2)
                .max(need)
                .min(native::MAX_CONFIG_BYTES);
            self.0
                .try_reserve_exact(cap - self.0.len())
                .map_err(|_| std::io::Error::other("native allocation"))?;
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
/// Only policy arrays and controlled rule-set references change. All admitted
/// non-policy settings, complete listeners/outbounds and private telemetry remain
/// borrowed from accepted raw bytes and never exposed through Debug/Serialize.
pub fn compile_preserving(
    accepted: &[u8],
    nodes: &[Node],
    rules: Vec<Rule>,
    diagnostics: Vec<Diagnostic>,
    refs: Vec<RuleSetReference>,
    acknowledged: bool,
) -> Result<CompileOutput> {
    let mut input = intention(accepted, nodes)?;
    input.rules = rules;
    input.diagnostics = diagnostics;
    input.rule_sets = refs;
    input.accept_unsupported_rules = acknowledged;
    let mut output = native::compile_native(&input).map_err(|_| ApplyError)?;
    drop(input);
    let old: Object = parse(accepted)?;
    let generated: Object = parse(&output.config)?;
    let old_dns = part(&old, "dns")?;
    let old_route = part(&old, "route")?;
    let new_dns = part(&generated, "dns")?;
    let new_route = part(&generated, "route")?;
    let mut dns = old_dns.0;
    dns.insert("rules".into(), *new_dns.0.get("rules").ok_or(ApplyError)?);
    let mut route = old_route.0;
    for key in ["rules", "rule_set"] {
        route.insert(key.into(), *new_route.0.get(key).ok_or(ApplyError)?);
    }
    let mut dns_bytes = Bounded(Vec::with_capacity(4096));
    serde_json::to_writer(&mut dns_bytes, &dns).map_err(|_| ApplyError)?;
    let mut route_bytes = Bounded(Vec::with_capacity(4096));
    serde_json::to_writer(&mut route_bytes, &route).map_err(|_| ApplyError)?;
    let dns_raw: &RawValue = parse(&dns_bytes.0)?;
    let route_raw: &RawValue = parse(&route_bytes.0)?;
    let mut root = old.0;
    root.insert("dns".into(), dns_raw);
    root.insert("route".into(), route_raw);
    let mut encoded = Bounded(Vec::with_capacity(4096));
    serde_json::to_writer(&mut encoded, &root).map_err(|_| ApplyError)?;
    drop(root);
    drop(generated);
    output.config = encoded.0;
    output.sha256 = format!("{:x}", Sha256::digest(&output.config));
    Ok(output)
}

use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
};
fn private_directory(path: &Path) -> Result<(File, (u64, u64))> {
    if !path.is_absolute() {
        return Err(ApplyError);
    }
    let dir = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| ApplyError)?;
    let m = dir.metadata().map_err(|_| ApplyError)?;
    if !m.is_dir() || m.mode() & 0o7777 != 0o700 || m.uid() != unsafe { libc::geteuid() } {
        return Err(ApplyError);
    }
    Ok((dir, (m.dev(), m.ino())))
}
fn read_private(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let mut f = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| ApplyError)?;
    let m = f.metadata().map_err(|_| ApplyError)?;
    if !m.is_file()
        || m.nlink() != 1
        || m.mode() & 0o7777 != 0o600
        || m.uid() != unsafe { libc::geteuid() }
        || m.len() > limit as u64
    {
        return Err(ApplyError);
    }
    let mut out = Vec::new();
    out.try_reserve_exact(m.len() as usize)
        .map_err(|_| ApplyError)?;
    let mut chunk = [0; 8192];
    loop {
        let n = f.read(&mut chunk).map_err(|_| ApplyError)?;
        if n == 0 {
            break;
        }
        if n > limit.saturating_sub(out.len()) {
            return Err(ApplyError);
        }
        out.extend_from_slice(&chunk[..n]);
    }
    let after = f.metadata().map_err(|_| ApplyError)?;
    let disk = fs::symlink_metadata(path).map_err(|_| ApplyError)?;
    if m.dev() != after.dev()
        || m.ino() != after.ino()
        || m.len() != after.len()
        || m.mtime() != after.mtime()
        || m.mtime_nsec() != after.mtime_nsec()
        || disk.dev() != m.dev()
        || disk.ino() != m.ino()
    {
        return Err(ApplyError);
    }
    Ok(out)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RefDto {
    #[serde(rename = "Tag", alias = "tag")]
    tag: String,
    #[serde(rename = "Kind", alias = "kind")]
    kind: String,
    #[serde(rename = "Path", alias = "path")]
    path: String,
    #[serde(rename = "SHA256", alias = "sha256")]
    sha256: String,
    #[serde(rename = "SourceURL", alias = "sourceURL", default)]
    source_url: String,
    #[serde(rename = "MaxBytes", alias = "maxBytes")]
    max_bytes: i64,
}
/// Reads controlled references and hashes each staged SRS with an 8KiB buffer.
/// A rule-only Apply never downloads assets or substitutes a missing set.
pub(crate) fn read_verified_refs(data_dir: &Path) -> Result<Vec<RuleSetReference>> {
    let (_, identity) = private_directory(data_dir)?;
    let raw = read_private(&data_dir.join("rule-sets.json"), 32 << 10)?;
    let entries: Vec<RefDto> = parse(&raw)?;
    if entries.len() < 2 || entries.len() > 3 {
        return Err(ApplyError);
    }
    let mut refs = Vec::with_capacity(entries.len());
    let mut tags = BTreeSet::new();
    for e in entries {
        if !matches!(e.tag.as_str(), "cn-domain" | "cn-ip" | "proxy-domain")
            || !tags.insert(e.tag.clone())
            || e.kind != if e.tag == "cn-ip" { "ip" } else { "domain" }
            || e.max_bytes <= 0
            || e.max_bytes > 8 << 20
            || e.sha256.len() != 64
            || !e.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(ApplyError);
        }
        let path = PathBuf::from(&e.path);
        if !path.is_absolute() || path.extension().is_none_or(|x| x != "srs") {
            return Err(ApplyError);
        }
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(&path)
            .map_err(|_| ApplyError)?;
        let before = file.metadata().map_err(|_| ApplyError)?;
        if !before.is_file()
            || before.nlink() != 1
            || before.mode() & 0o7777 != 0o600
            || before.uid() != unsafe { libc::geteuid() }
            || before.len() > e.max_bytes as u64
        {
            return Err(ApplyError);
        }
        let mut hash = Sha256::new();
        let mut total = 0u64;
        let mut buffer = [0; 8192];
        loop {
            let n = file.read(&mut buffer).map_err(|_| ApplyError)?;
            if n == 0 {
                break;
            }
            total += n as u64;
            if total > e.max_bytes as u64 {
                return Err(ApplyError);
            }
            hash.update(&buffer[..n]);
        }
        let after = file.metadata().map_err(|_| ApplyError)?;
        let disk = fs::symlink_metadata(&path).map_err(|_| ApplyError)?;
        if total != before.len()
            || before.dev() != after.dev()
            || before.ino() != after.ino()
            || before.len() != after.len()
            || before.mtime() != after.mtime()
            || before.mtime_nsec() != after.mtime_nsec()
            || disk.dev() != before.dev()
            || disk.ino() != before.ino()
            || format!("{:x}", hash.finalize()) != e.sha256.to_ascii_lowercase()
        {
            return Err(ApplyError);
        }
        refs.push(RuleSetReference {
            tag: e.tag,
            kind: e.kind,
            path: e.path,
            sha256: e.sha256,
            source_url: e.source_url,
            max_bytes: e.max_bytes,
        });
    }
    if !tags.contains("cn-domain")
        || !tags.contains("cn-ip")
        || private_directory(data_dir)?.1 != identity
    {
        return Err(ApplyError);
    }
    Ok(refs)
}
#[derive(Clone, serde::Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AppliedManifest {
    pub revision: String,
    #[serde(rename = "nativeSHA256")]
    pub native_sha256: String,
    pub generation: u64,
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
pub(crate) fn read_manifest(data_dir: &Path) -> Option<AppliedManifest> {
    private_directory(data_dir).ok()?;
    let raw = read_private(&data_dir.join("local-proxy-rules-applied.json"), 4096).ok()?;
    let doc: AppliedManifest = parse(&raw).ok()?;
    if !valid_hash(&doc.revision) || !valid_hash(&doc.native_sha256) || doc.generation == 0 {
        return None;
    }
    Some(doc)
}
/// This tiny manifest is written only after accepted bytes/generation match.
/// Errors after rename are uncertainty; caller must not claim old state/false
/// applied success. No old manifest/file outside this fixed name is pruned.
pub(crate) fn write_manifest(data_dir: &Path, doc: &AppliedManifest) -> Result<()> {
    if !valid_hash(&doc.revision) || !valid_hash(&doc.native_sha256) || doc.generation == 0 {
        return Err(ApplyError);
    }
    let (dir, identity) = private_directory(data_dir)?;
    let bytes = serde_json::to_vec(doc).map_err(|_| ApplyError)?;
    if bytes.len() > 4096 {
        return Err(ApplyError);
    }
    let mut info = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    if unsafe { libc::fstatvfs(dir.as_raw_fd(), info.as_mut_ptr()) } != 0 {
        return Err(ApplyError);
    }
    let info = unsafe { info.assume_init() };
    let unit = if info.f_frsize == 0 {
        info.f_bsize
    } else {
        info.f_frsize
    };
    let available = u64::try_from(
        u128::from(info.f_bavail)
            .checked_mul(u128::from(unit))
            .ok_or(ApplyError)?,
    )
    .map_err(|_| ApplyError)?;
    if available < (1 << 20) + 8192 {
        return Err(ApplyError);
    }
    let dest = data_dir.join("local-proxy-rules-applied.json");
    if let Ok(metadata) = fs::symlink_metadata(&dest)
        && (!metadata.is_file() || metadata.file_type().is_symlink() || metadata.nlink() != 1)
    {
        return Err(ApplyError);
    }
    let mut random = [0; 16];
    getrandom::fill(&mut random).map_err(|_| ApplyError)?;
    let name = format!(
        ".local-rule-applied-{}",
        random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    let temp = data_dir.join(name);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)
        .map_err(|_| ApplyError)?;
    let result = (|| {
        file.write_all(&bytes).map_err(|_| ApplyError)?;
        file.sync_all().map_err(|_| ApplyError)?;
        if private_directory(data_dir)?.1 != identity {
            return Err(ApplyError);
        }
        fs::rename(&temp, &dest).map_err(|_| ApplyError)?;
        dir.sync_all().map_err(|_| ApplyError)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

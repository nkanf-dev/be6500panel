//! Pure sing-box 1.14.2 native compilation. No file, network, process or kernel I/O.
//!
//! Private DTOs deliberately implement Deserialize, not Serialize. Their Debug
//! output is redacted. Config bytes must be kept private by any future owner;
//! compilation does not check assets, activate capture, or establish readiness.
use crate::policy::{Diagnostic, MAX_RULES, Rule, RuleKind, Target};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr};

pub const CORE_VERSION: &str = "1.14.2";
/// Embedded runtime admission limit. Go-valid inputs that exceed this budget
/// are deliberately refused; compilation never implies runtime acceptance.
pub const MAX_CONFIG_BYTES: usize = 512 << 10;
pub const MAX_DIAGNOSTICS: usize = MAX_RULES;
pub const MAX_RULE_SET_PATH_BYTES: usize = 4096;
pub const MAX_PROVENANCE_URL_BYTES: usize = 4096;

#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Node {
    pub id: String,
    pub name: String,
    pub server: String,
    pub port: u16,
    pub uuid: String,
    pub server_name: String,
    pub reality_public_key: String,
    #[serde(rename = "realityShortID")]
    pub reality_short_id: String,
    pub fingerprint: String,
    pub flow: String,
    pub udp: bool,
}

#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Ports {
    pub mixed: u16,
    #[serde(rename = "tProxy")]
    pub tproxy: u16,
    pub dns: u16,
}

#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct RoutedTUNConfig {
    pub interface_name: String,
    pub address: String,
}

#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct DNSEndpoint {
    pub server: String,
    pub port: u16,
    pub server_name: String,
}

#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalDNSConfig {
    pub server: String,
    pub port: u16,
    #[serde(deserialize_with = "nil_slice")]
    pub domains: Vec<String>,
    #[serde(deserialize_with = "nil_slice")]
    pub hostnames: Vec<String>,
}

/// Metadata admission only. The compiler never opens Path, verifies its hash,
/// downloads SourceURL, or treats a reference as proof that an asset exists.
#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct RuleSetReference {
    pub tag: String,
    pub kind: String,
    pub path: String,
    pub sha256: String,
    #[serde(rename = "sourceURL")]
    pub source_url: String,
    pub max_bytes: i64,
}

#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct CompileInput {
    pub node: Node,
    #[serde(deserialize_with = "nil_slice")]
    pub rules: Vec<Rule>,
    #[serde(deserialize_with = "nil_slice")]
    pub overrides: Vec<Rule>,
    #[serde(deserialize_with = "nil_slice")]
    pub rule_sets: Vec<RuleSetReference>,
    #[serde(deserialize_with = "nil_slice")]
    pub endpoints: Vec<String>,
    #[serde(deserialize_with = "nil_slice")]
    pub bootstrap_domains: Vec<String>,
    #[serde(rename = "managementIPs", deserialize_with = "nil_slice")]
    pub management_ips: Vec<String>,
    pub datapath: String,
    #[serde(rename = "routedTUN")]
    pub routed_tun: Option<RoutedTUNConfig>,
    pub ipv6: String,
    pub failure: String,
    pub ports: Ports,
    pub listen_address: String,
    pub mixed_listen_address: String,
    /// Ignored legacy input. No emitted listener uses TPROXY.
    #[serde(rename = "tProxyListenAddress")]
    pub tproxy_listen_address: String,
    pub dns_listen_address: String,
    #[serde(rename = "directDNS")]
    pub direct_dns: DNSEndpoint,
    #[serde(rename = "proxyDNS")]
    pub proxy_dns: DNSEndpoint,
    #[serde(rename = "localDNS")]
    pub local_dns: Option<LocalDNSConfig>,
    #[serde(rename = "fakeIP")]
    pub fake_ip: bool,
    pub accept_unsupported_rules: bool,
    #[serde(deserialize_with = "nil_slice")]
    pub diagnostics: Vec<Diagnostic>,
}

fn nil_slice<'de, D, T>(deserializer: D) -> std::result::Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}

macro_rules! private_debug {
    ($($ty:ty),+ $(,)?) => {$(impl fmt::Debug for $ty {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(concat!(stringify!($ty), " (private)"))
        }
    })+};
}
private_debug!(
    Node,
    Ports,
    RoutedTUNConfig,
    DNSEndpoint,
    LocalDNSConfig,
    RuleSetReference,
    CompileInput
);

/// Public serialization exposes only fixed diagnostics and non-private metadata.
/// `config` and `endpoint_hosts` are private fields even though library callers
/// can consume them. Debug intentionally does not traverse configuration bytes.
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileOutput {
    #[serde(skip_serializing)]
    pub config: Vec<u8>,
    pub sha256: String,
    pub core_version: String,
    // Go's untouched diagnostic slice encodes as null, not an empty array.
    #[serde(serialize_with = "serialize_diagnostics")]
    pub diagnostics: Vec<Diagnostic>,
    #[serde(skip_serializing)]
    pub endpoint_hosts: Vec<String>,
    pub required_features: Vec<String>,
    pub ipv6: String,
    pub failure: String,
}
impl fmt::Debug for CompileOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CompileOutput")
            .field("core_version", &self.core_version)
            .field("sha256", &self.sha256)
            .field("config_bytes", &self.config.len())
            .finish()
    }
}
fn serialize_diagnostics<S: serde::Serializer>(
    value: &[Diagnostic],
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    if value.is_empty() {
        serializer.serialize_none()
    } else {
        value.serialize(serializer)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileError {
    message: String,
}
impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for CompileError {}
type Result<T> = std::result::Result<T, CompileError>;
fn error(message: &'static str) -> CompileError {
    CompileError {
        message: message.into(),
    }
}
fn diagnostic(scope: &str, index: i64, code: &str, message: &str) -> Diagnostic {
    Diagnostic {
        scope: scope.into(),
        index,
        code: code.into(),
        message: message.into(),
    }
}
fn valid_domain(domain: &str) -> bool {
    !domain.is_empty()
        && domain.len() <= 253
        && !domain.ends_with('.')
        && domain.split('.').all(|part| {
            !part.is_empty()
                && part.len() <= 63
                && !part.starts_with('-')
                && !part.ends_with('-')
                && part.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
}
fn mapped(ip: IpAddr) -> bool {
    matches!(ip, IpAddr::V6(a) if a.to_ipv4_mapped().is_some())
}
fn usable_ip(ip: IpAddr) -> bool {
    !ip.is_unspecified() && !ip.is_multicast()
}
fn parse_prefix(value: &str) -> Option<(IpAddr, u8)> {
    let (address, bits) = value.rsplit_once('/')?;
    if bits.is_empty()
        || !bits.bytes().all(|c| c.is_ascii_digit())
        || bits.len() > 1 && bits.starts_with('0')
    {
        return None;
    }
    let ip: IpAddr = address.parse().ok()?;
    let bits: u8 = bits.parse().ok()?;
    if mapped(ip) || bits > if ip.is_ipv4() { 32 } else { 128 } {
        return None;
    }
    Some((ip, bits))
}
fn validate_node(node: &Node) -> Result<()> {
    let valid_server = match node.server.parse::<IpAddr>() {
        Ok(ip) => usable_ip(ip),
        Err(_) => valid_domain(&node.server),
    };
    if node.port == 0 || !valid_server {
        return Err(error("invalid server endpoint"));
    }
    if !valid_domain(&node.server_name) {
        return Err(error("REALITY certificate DNS identity is required"));
    }
    if node.uuid.len() != 36
        || !node.uuid.bytes().enumerate().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
    {
        return Err(error("invalid VLESS UUID"));
    }
    if node.flow != "xtls-rprx-vision" {
        return Err(error("only xtls-rprx-vision flow is supported"));
    }
    if node.fingerprint != "chrome" {
        return Err(error("only Chrome uTLS fingerprint is supported"));
    }
    // Go RawURLEncoding accepts CR/LF and noncanonical trailing pad bits, but
    // never '=' padding. Bound those ignored bytes before decoding admission.
    if node.reality_public_key.len() > 256 {
        return Err(error("invalid REALITY public key"));
    }
    let mut key_chars = 0;
    for c in node.reality_public_key.bytes() {
        if matches!(c, b'\r' | b'\n') {
            continue;
        }
        if !c.is_ascii_alphanumeric() && !matches!(c, b'-' | b'_') {
            return Err(error("invalid REALITY public key"));
        }
        key_chars += 1;
    }
    if key_chars != 43 {
        return Err(error("invalid REALITY public key"));
    }
    if node.reality_short_id.len() > 16
        || !node.reality_short_id.len().is_multiple_of(2)
        || !node.reality_short_id.bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Err(error("invalid REALITY short ID"));
    }
    if !node.udp {
        return Err(error("VLESS UDP/XUDP must be enabled"));
    }
    Ok(())
}
fn validate_rule(rule: &Rule) -> Result<()> {
    let valid = matches!(rule.target, Target::Direct | Target::Proxy | Target::Block)
        && match rule.kind {
            RuleKind::Domain | RuleKind::DomainSuffix => valid_domain(&rule.value),
            RuleKind::DomainKeyword => {
                !rule.value.is_empty()
                    && rule.value.len() <= 253
                    && !rule.value.bytes().any(|c| matches!(c, b'\r' | b'\n' | 0))
            }
            RuleKind::IpCidr => parse_prefix(&rule.value).is_some(),
            RuleKind::RuleSet => {
                matches!(rule.value.as_str(), "cn-domain" | "cn-ip" | "proxy-domain")
            }
            RuleKind::Match => rule.value.is_empty(),
            RuleKind::Unknown(_) => false,
        };
    if valid {
        Ok(())
    } else {
        Err(error("invalid native rule"))
    }
}
fn validate_tun(datapath: &str, ipv6: &str, tun: &RoutedTUNConfig) -> Result<u32> {
    match datapath {
        "tproxy" => {
            return Err(error(
                "tproxy datapath is no longer supported; use routed-tun",
            ));
        }
        "routed-tun" => {}
        _ => return Err(error("invalid datapath mode")),
    }
    if ipv6 != "direct" {
        return Err(error(
            "routed-tun supports only IPv6 direct; IPv6 follow and block are not supported",
        ));
    }
    let Some(suffix) = tun.interface_name.strip_prefix("b6p-") else {
        return Err(error(
            "routed-tun interface requires an owned b6p- name of 5 to 15 safe characters",
        ));
    };
    if suffix.is_empty()
        || suffix.len() > 11
        || !suffix.as_bytes()[0].is_ascii_alphanumeric() && suffix.as_bytes()[0] != b'_'
        || !suffix
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
    {
        return Err(error(
            "routed-tun interface requires an owned b6p- name of 5 to 15 safe characters",
        ));
    }
    let Some((IpAddr::V4(address), 30)) = parse_prefix(&tun.address) else {
        return Err(error(
            "routed-tun address requires a literal RFC1918 IPv4 /30 first usable host with a usable next peer",
        ));
    };
    if !address.is_private() || format!("{address}/30") != tun.address {
        return Err(error(
            "routed-tun address requires a literal RFC1918 IPv4 /30 first usable host with a usable next peer",
        ));
    }
    let host = u32::from(address);
    if host & 3 != 1 {
        return Err(error(
            "routed-tun address requires the first usable /30 host with a usable next peer",
        ));
    }
    Ok(host & !3)
}
fn overlaps(network: u32, value: &str) -> bool {
    match value.parse::<IpAddr>() {
        Ok(IpAddr::V4(a)) => u32::from(a) & !3 == network,
        Ok(IpAddr::V6(a)) => a
            .to_ipv4_mapped()
            .is_some_and(|a| u32::from(a) & !3 == network),
        _ => false,
    }
}
fn validate_dns(endpoint: &DNSEndpoint) -> Result<()> {
    if !endpoint.server.parse::<IpAddr>().is_ok_and(usable_ip)
        || endpoint.port == 0
        || !valid_domain(&endpoint.server_name)
    {
        return Err(error(
            "DNS requires a literal IP, nonzero TLS port and certificate DNS identity",
        ));
    }
    Ok(())
}
fn sorted_unique(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values.dedup();
    values
}
fn local_names(defaults: &[&str], extras: &[String]) -> Result<Vec<String>> {
    let mut values: Vec<String> = defaults.iter().map(|s| (*s).into()).collect();
    for value in extras {
        // Go TrimSuffix removes one trailing dot, not every trailing dot.
        if value.len() > 254 {
            return Err(error("invalid local DNS domain or hostname"));
        }
        let name = value.strip_suffix('.').unwrap_or(value).to_lowercase();
        if !valid_domain(&name) {
            return Err(error("invalid local DNS domain or hostname"));
        }
        values.push(name);
    }
    Ok(sorted_unique(values))
}
fn compile_local_dns(input: &CompileInput, ports: &Ports) -> Result<LocalDNSConfig> {
    if let Some(local) = &input.local_dns {
        if local.domains.len() > 32 || local.hostnames.len() > 64 {
            return Err(error("local DNS domain or hostname limit exceeded"));
        }
        if local.server.len() > 45
            || local
                .domains
                .iter()
                .chain(&local.hostnames)
                .any(|s| s.len() > 254)
        {
            return Err(error("invalid local DNS domain or hostname"));
        }
    }
    let mut local = input.local_dns.clone().unwrap_or_default();
    if local.server.is_empty() {
        local.server = "127.0.0.1".into();
    }
    if local.port == 0 {
        local.port = 53;
    }
    let address: IpAddr = local
        .server
        .parse()
        .map_err(|_| error("local DNS requires a literal loopback or router management address"))?;
    let link_local = match address {
        IpAddr::V4(a) => a.is_link_local(),
        IpAddr::V6(a) => a.is_unicast_link_local(),
    };
    if !usable_ip(address)
        || mapped(address)
        || link_local
        || address == IpAddr::V4(Ipv4Addr::BROADCAST)
    {
        return Err(error(
            "local DNS requires a literal loopback or router management address",
        ));
    }
    local.server = address.to_string();
    if !address.is_loopback()
        && !input
            .management_ips
            .iter()
            .any(|s| s.parse::<IpAddr>().ok() == Some(address))
    {
        return Err(error(
            "local DNS address must belong to router ManagementIPs",
        ));
    }
    if local.port == ports.mixed || local.port == ports.dns {
        return Err(error("local DNS must not point to a core listener port"));
    }
    if local.domains.len() > 32 || local.hostnames.len() > 64 {
        return Err(error("local DNS domain or hostname limit exceeded"));
    }
    local.domains = local_names(&["lan", "local", "localhost"], &local.domains)?;
    local.hostnames = local_names(
        &[
            "xiaoqiang",
            "miwifi.com",
            "www.miwifi.com",
            "router.miwifi.com",
            "www.router.miwifi.com",
        ],
        &local.hostnames,
    )?;
    Ok(local)
}
fn local_matches(local: &LocalDNSConfig) -> [Value; 3] {
    [
        json!({"domain_suffix": local.domains}),
        json!({"domain": local.hostnames}),
        json!({"domain_regex": ["^[^.]+$"]}),
    ]
}
fn local_reverse_suffixes() -> Vec<String> {
    let mut values: Vec<String> = [
        "10.in-addr.arpa",
        "168.192.in-addr.arpa",
        "127.in-addr.arpa",
        "254.169.in-addr.arpa",
        "c.f.ip6.arpa",
        "d.f.ip6.arpa",
        "8.e.f.ip6.arpa",
        "9.e.f.ip6.arpa",
        "a.e.f.ip6.arpa",
        "b.e.f.ip6.arpa",
    ]
    .iter()
    .map(|s| (*s).into())
    .collect();
    values.push(format!("1.{}ip6.arpa", "0.".repeat(31)));
    values.extend((16..=31).map(|n| format!("{n}.172.in-addr.arpa")));
    values.extend((64..=127).map(|n| format!("{n}.100.in-addr.arpa")));
    sorted_unique(values)
}
fn address_prefixes(values: impl Iterator<Item = String>) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for value in values {
        let ip: IpAddr = value
            .parse()
            .map_err(|_| error("invalid management or bootstrap IP"))?;
        if !usable_ip(ip) || mapped(ip) {
            return Err(error("invalid management or bootstrap IP"));
        }
        out.push(format!("{ip}/{}", if ip.is_ipv4() { 32 } else { 128 }));
    }
    Ok(sorted_unique(out))
}
fn clean_srs_path(path: &str) -> bool {
    path.len() <= MAX_RULE_SET_PATH_BYTES
        && path.starts_with('/')
        && path.ends_with(".srs")
        && !path.bytes().any(|c| matches!(c, b'\r' | b'\n' | 0))
        && path[1..]
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}
// A small provenance parser. No URL is fetched. Match Go's credential-free
// HTTPS/hostname/query/fragment constraints without bringing a transport stack.
fn valid_provenance(url: &str) -> bool {
    if url.len() > MAX_PROVENANCE_URL_BYTES || url.bytes().any(|c| c <= 0x20 || c == 0x7f) {
        return false;
    }
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let (no_fragment, fragment) = rest.split_once('#').unwrap_or((rest, ""));
    if !fragment.is_empty() {
        return false;
    }
    let (no_query, query) = no_fragment.split_once('?').unwrap_or((no_fragment, ""));
    if !query.is_empty() {
        return false;
    }
    let authority = no_query.split('/').next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') || authority.contains('\\') {
        return false;
    }
    let (host, port) = if let Some(bracket) = authority.strip_prefix('[') {
        let Some((host, tail)) = bracket.split_once(']') else {
            return false;
        };
        if !tail.is_empty() && !tail.starts_with(':') {
            return false;
        }
        (host, tail.strip_prefix(':').unwrap_or(""))
    } else if let Some((host, port)) = authority.rsplit_once(':') {
        (host, port)
    } else {
        (authority, "")
    };
    if host.is_empty() || host.contains('%') || !port.bytes().all(|c| c.is_ascii_digit()) {
        return false;
    }
    // Go validates percent escapes even in unused provenance paths.
    let bytes = no_query.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len()
                || !bytes[i + 1].is_ascii_hexdigit()
                || !bytes[i + 2].is_ascii_hexdigit()
            {
                return false;
            }
            i += 2;
        }
        i += 1;
    }
    true
}
fn compile_rule_sets(refs: &[RuleSetReference]) -> Result<(Vec<Value>, BTreeSet<String>)> {
    if refs.len() > 3 {
        return Err(error("at most three controlled rule sets are supported"));
    }
    let mut sorted: Vec<&RuleSetReference> = refs.iter().collect();
    sorted.sort_by(|a, b| a.tag.cmp(&b.tag));
    let mut tags = BTreeSet::new();
    let mut rules = Vec::new();
    for reference in sorted {
        let kind = if reference.tag == "cn-ip" {
            "ip"
        } else {
            "domain"
        };
        if !matches!(
            reference.tag.as_str(),
            "cn-domain" | "cn-ip" | "proxy-domain"
        ) || tags.contains(&reference.tag)
            || reference.kind != kind
        {
            return Err(error("invalid controlled rule set tag or kind"));
        }
        if !clean_srs_path(&reference.path) {
            return Err(error("rule set requires an absolute clean SRS path"));
        }
        if reference.sha256.len() != 64
            || !reference.sha256.bytes().all(|c| c.is_ascii_hexdigit())
            || reference.max_bytes <= 0
            || reference.max_bytes > 8 << 20
        {
            return Err(error("rule set requires a pinned SHA256 and bounded size"));
        }
        if !reference.source_url.is_empty() && !valid_provenance(&reference.source_url) {
            return Err(error("rule set provenance must be credential-free HTTPS"));
        }
        tags.insert(reference.tag.clone());
        rules.push(
            json!({"type":"local","tag":reference.tag,"format":"binary","path":reference.path}),
        );
    }
    Ok((rules, tags))
}
fn rule_match(rule: &Rule) -> Value {
    let key = match rule.kind {
        RuleKind::Domain => "domain",
        RuleKind::DomainSuffix => "domain_suffix",
        RuleKind::DomainKeyword => "domain_keyword",
        RuleKind::IpCidr => "ip_cidr",
        RuleKind::RuleSet => "rule_set",
        _ => return json!({}),
    };
    json!({key: [rule.value]})
}
fn route_action(mut matcher: Value, target: &Target) -> Value {
    if *target == Target::Block {
        matcher["action"] = "reject".into();
    } else {
        matcher["outbound"] = if *target == Target::Direct {
            "direct"
        } else {
            "proxy"
        }
        .into();
    }
    matcher
}
fn append_dns_rules(rules: &mut Vec<Value>, mut matcher: Value, target: &Target, fake: bool) {
    if *target == Target::Block {
        matcher["action"] = "reject".into();
        rules.push(matcher);
        return;
    }
    if *target == Target::Proxy && fake {
        let mut address_match = matcher.clone();
        address_match["query_type"] = json!(["A", "AAAA"]);
        address_match["server"] = "dns-fake".into();
        address_match["rewrite_ttl"] = 60.into();
        rules.push(address_match);
    }
    matcher["server"] = if *target == Target::Direct {
        "dns-direct"
    } else {
        "dns-proxy"
    }
    .into();
    matcher["rewrite_ttl"] = 300.into();
    rules.push(matcher);
}
fn add_fallback(
    route: &mut Vec<Value>,
    dns: &mut Vec<Value>,
    tags: &BTreeSet<String>,
    fake: bool,
    added: &mut bool,
) {
    if *added {
        return;
    }
    *added = true;
    if tags.contains("cn-domain") {
        route.push(json!({"rule_set":["cn-domain"],"outbound":"direct"}));
        dns.push(json!({"rule_set":["cn-domain"],"server":"dns-direct","rewrite_ttl":300}));
    }
    if tags.contains("proxy-domain") {
        route.push(json!({"rule_set":["proxy-domain"],"outbound":"proxy"}));
        append_dns_rules(
            dns,
            json!({"rule_set":["proxy-domain"]}),
            &Target::Proxy,
            fake,
        );
    }
    if tags.contains("cn-ip") {
        route.push(json!({"action":"resolve","server":"dns-proxy","timeout":"5s"}));
        route.push(json!({"rule_set":["cn-ip"],"outbound":"direct"}));
    }
}
fn dns_server(tag: &str, endpoint: &DNSEndpoint, detour: &str) -> Value {
    json!({"type":"tls","tag":tag,"server":endpoint.server,"server_port":endpoint.port,"detour":detour,
        "tls":{"enabled":true,"server_name":endpoint.server_name,"min_version":"1.2"}})
}

/// Compile private input to owned config bytes. Hashes cover those bytes exactly,
/// including the final newline. The formatter matches Go MarshalIndent sorted
/// object keys, two-space indentation, HTML escaping and U+2028/U+2029 escaping.
pub fn compile_native(input: &CompileInput) -> Result<CompileOutput> {
    validate_node(&input.node)?;
    let ipv6 = if input.ipv6.is_empty() {
        "direct"
    } else {
        &input.ipv6
    };
    if !matches!(ipv6, "direct" | "follow" | "block") {
        return Err(error("invalid IPv6 policy"));
    }
    let datapath = if input.datapath.is_empty() {
        "routed-tun"
    } else {
        &input.datapath
    };
    let default_tun = RoutedTUNConfig {
        interface_name: "b6p-tun".into(),
        address: "172.31.255.253/30".into(),
    };
    let tun = input.routed_tun.as_ref().unwrap_or(&default_tun);
    let network = validate_tun(datapath, ipv6, tun)?;
    let failure = if input.failure.is_empty() {
        "direct"
    } else {
        &input.failure
    };
    if failure == "block-proxy" {
        return Err(error(
            "block-proxy requires a surviving classification authority; not supported",
        ));
    }
    if failure != "direct" {
        return Err(error("invalid failure policy"));
    }
    let ports = if input.ports.mixed == 0 && input.ports.dns == 0 {
        Ports {
            mixed: 2080,
            dns: 1053,
            tproxy: 0,
        }
    } else {
        input.ports.clone()
    };
    if ports.mixed == 0 || ports.dns == 0 || ports.mixed == ports.dns {
        return Err(error("listener ports must be nonzero and distinct"));
    }
    let default_listen = if input.listen_address.is_empty() {
        "127.0.0.1"
    } else {
        &input.listen_address
    };
    let mixed_listen = if input.mixed_listen_address.is_empty() {
        default_listen
    } else {
        &input.mixed_listen_address
    };
    let dns_listen = if input.dns_listen_address.is_empty() {
        default_listen
    } else {
        &input.dns_listen_address
    };
    for value in [mixed_listen, dns_listen] {
        if !value.parse::<IpAddr>().is_ok_and(|a| !a.is_multicast()) {
            return Err(error("invalid listener address"));
        }
    }
    let direct_default = DNSEndpoint {
        server: "223.5.5.5".into(),
        port: 853,
        server_name: "dns.alidns.com".into(),
    };
    let proxy_default = DNSEndpoint {
        server: "1.1.1.1".into(),
        port: 853,
        server_name: "cloudflare-dns.com".into(),
    };
    let direct_dns = if input.direct_dns == DNSEndpoint::default() {
        &direct_default
    } else {
        &input.direct_dns
    };
    let proxy_dns = if input.proxy_dns == DNSEndpoint::default() {
        &proxy_default
    } else {
        &input.proxy_dns
    };
    validate_dns(direct_dns)?;
    validate_dns(proxy_dns)?;
    if input.rules.len().saturating_add(input.overrides.len()) > MAX_RULES
        || input.endpoints.len() > 256
        || input.bootstrap_domains.len() > 256
        || input.management_ips.len() > 128
        || input.diagnostics.len() > MAX_DIAGNOSTICS
    {
        return Err(error("compiler input limit exceeded"));
    }
    let local = compile_local_dns(input, &ports)?;
    for value in &input.management_ips {
        if overlaps(network, value) {
            return Err(error(
                "routed-tun address prefix overlaps router management",
            ));
        }
    }
    for value in [
        &input.node.server,
        &direct_dns.server,
        &proxy_dns.server,
        &local.server,
        mixed_listen,
        dns_listen,
    ]
    .into_iter()
    .chain(input.endpoints.iter().map(String::as_str))
    {
        if overlaps(network, value) {
            return Err(error(
                "routed-tun address prefix overlaps a known endpoint or listener",
            ));
        }
    }
    let mut diagnostics = Vec::new();
    for d in &input.diagnostics {
        if d.scope == "rule" {
            if !input.accept_unsupported_rules {
                return Err(CompileError {
                    message: format!(
                        "unsupported subscription rule at index {} requires explicit acknowledgement",
                        d.index
                    ),
                });
            }
            diagnostics.push(diagnostic(
                "rule",
                d.index,
                "ignored-subscription-rule",
                "unsupported subscription rule explicitly acknowledged and omitted",
            ));
        }
    }
    for rule in input.overrides.iter().chain(&input.rules) {
        validate_rule(rule)?;
    }
    let (rule_sets, tags) = compile_rule_sets(&input.rule_sets)?;
    for rule in input.overrides.iter().chain(&input.rules) {
        if rule.kind == RuleKind::RuleSet && !tags.contains(&rule.value) {
            return Err(error("rule references unstaged controlled set"));
        }
    }
    if !tags.contains("cn-domain") || !tags.contains("cn-ip") {
        diagnostics.push(diagnostic("config", -1, "cn-rules-incomplete", "domestic split needs staged cn-domain and cn-ip SRS; missing classifications use the default proxy policy"));
    }
    let node_ip = input.node.server.parse::<IpAddr>().ok();
    for value in input.management_ips.iter().chain(&input.endpoints) {
        if value.len() > 45
            || !value
                .parse::<IpAddr>()
                .is_ok_and(|ip| usable_ip(ip) && !mapped(ip))
        {
            return Err(error("invalid management or bootstrap IP"));
        }
    }
    let mut bypass_ips: Vec<String> = input
        .management_ips
        .iter()
        .chain(&input.endpoints)
        .cloned()
        .collect();
    if let Some(ip) = node_ip {
        bypass_ips.push(ip.to_string());
    }
    bypass_ips.push(direct_dns.server.clone());
    let bypass = address_prefixes(bypass_ips.into_iter())?;
    if !input.bootstrap_domains.iter().all(|s| valid_domain(s)) {
        return Err(error("invalid bootstrap domain"));
    }
    let mut bootstrap = vec![direct_dns.server_name.clone()];
    bootstrap.extend(input.bootstrap_domains.iter().cloned());
    if node_ip.is_none() {
        bootstrap.push(input.node.server.clone());
    }
    if !bootstrap.iter().all(|s| valid_domain(s)) {
        return Err(error("invalid bootstrap domain"));
    }
    let bootstrap = sorted_unique(bootstrap);
    let private = [
        "0.0.0.0/8",
        "10.0.0.0/8",
        "100.64.0.0/10",
        "127.0.0.0/8",
        "169.254.0.0/16",
        "172.16.0.0/12",
        "192.168.0.0/16",
        "192.0.0.0/24",
        "198.18.0.0/15",
        "224.0.0.0/4",
        "240.0.0.0/4",
        "::/128",
        "::1/128",
        "fe80::/10",
        "fc00::/7",
        "ff00::/8",
    ];
    let mut route = vec![json!({"inbound":["dns-in"],"action":"hijack-dns"})];
    if !bypass.is_empty() {
        route.push(json!({"ip_cidr":bypass,"outbound":"direct"}));
    }
    if !bootstrap.is_empty() {
        route.push(json!({"domain":bootstrap,"outbound":"direct"}));
    }
    route.push(json!({"inbound":["mixed-in","tun-in"],"port":[53],"action":"hijack-dns"}));
    route.push(json!({"ip_cidr":private,"outbound":"direct"}));
    for matcher in local_matches(&local) {
        let mut resolve = matcher.clone();
        resolve["action"] = "resolve".into();
        resolve["server"] = "dns-local".into();
        resolve["timeout"] = "5s".into();
        resolve["disable_cache"] = true.into();
        route.push(resolve);
        route.push(route_action(matcher, &Target::Direct));
    }
    route.push(json!({"ip_version":6,"outbound":"direct"}));
    route.push(json!({"inbound":["mixed-in","tun-in"],"action":"sniff","sniffer":["http","tls","dns","quic"],"timeout":"300ms"}));
    let mut dns = Vec::new();
    if !bootstrap.is_empty() {
        dns.push(json!({"domain":bootstrap,"server":"dns-direct","rewrite_ttl":300}));
    }
    for mut matcher in local_matches(&local) {
        matcher["server"] = "dns-local".into();
        matcher["disable_cache"] = true.into();
        dns.push(matcher);
    }
    dns.push(json!({"query_type":["PTR"],"domain_suffix":local_reverse_suffixes(),"server":"dns-local","disable_cache":true}));
    dns.push(json!({"query_type":["AAAA"],"server":"dns-direct","rewrite_ttl":300}));
    let mut fallback_added = false;
    let mut match_seen = false;
    let mut rules_bytes = rendered_rules_bytes(&route) + rendered_rules_bytes(&dns);
    for rule in input.overrides.iter().chain(&input.rules) {
        let route_start = route.len();
        let dns_start = dns.len();
        if match_seen {
            diagnostics.push(diagnostic(
                "rule",
                rule.index,
                "unreachable-rule",
                "rule follows terminal MATCH and is unreachable",
            ));
            continue;
        }
        if rule.kind == RuleKind::Match {
            add_fallback(
                &mut route,
                &mut dns,
                &tags,
                input.fake_ip,
                &mut fallback_added,
            );
            match_seen = true;
        }
        if (rule.kind == RuleKind::IpCidr
            || rule.kind == RuleKind::RuleSet && rule.value == "cn-ip")
            && !rule.no_resolve
        {
            route.push(json!({"action":"resolve","server":"dns-proxy","timeout":"5s"}));
        }
        route.push(route_action(rule_match(rule), &rule.target));
        if matches!(
            rule.kind,
            RuleKind::Domain | RuleKind::DomainSuffix | RuleKind::DomainKeyword | RuleKind::Match
        ) || rule.kind == RuleKind::RuleSet && rule.value != "cn-ip"
        {
            append_dns_rules(&mut dns, rule_match(rule), &rule.target, input.fake_ip);
        }
        rules_bytes +=
            rendered_rules_bytes(&route[route_start..]) + rendered_rules_bytes(&dns[dns_start..]);
        if rules_bytes > MAX_CONFIG_BYTES {
            return Err(error("native configuration output limit exceeded"));
        }
    }
    add_fallback(
        &mut route,
        &mut dns,
        &tags,
        input.fake_ip,
        &mut fallback_added,
    );
    if !match_seen {
        route.push(route_action(json!({}), &Target::Proxy));
        append_dns_rules(&mut dns, json!({}), &Target::Proxy, input.fake_ip);
    }
    let mut servers = vec![
        dns_server("dns-direct", direct_dns, "direct"),
        dns_server("dns-proxy", proxy_dns, "proxy"),
        json!({"type":"udp","tag":"dns-local","server":local.server,"server_port":local.port,"detour":"direct"}),
    ];
    if input.fake_ip {
        servers.push(json!({"type":"fakeip","tag":"dns-fake","inet4_range":"198.18.0.0/15"}));
        diagnostics.push(diagnostic("config", -1, "fakeip-memory", "fake-IP identities use an unbounded native RAM map; restart or direct-failure withdrawal needs coordinated stale DNS cleanup before recapture"));
    }
    let resolver = || json!({"server":"dns-direct","timeout":"5s","strategy":"prefer_ipv4"});
    let node = &input.node;
    let mut config = json!({
        "log":{"level":"warn","timestamp":true},
        "dns":{"servers":servers,"rules":[],"final":"dns-proxy","cache_capacity":1024,"timeout":"5s","reverse_mapping":true,"strategy":"prefer_ipv4"},
        "inbounds":[
            {"type":"mixed","tag":"mixed-in","listen":mixed_listen,"listen_port":ports.mixed},
            {"type":"tun","tag":"tun-in","interface_name":tun.interface_name,"address":[tun.address],"mtu":1500,"dns_mode":"disabled","auto_route":false,"auto_redirect":false,"stack":"system","udp_timeout":"2m","udp_nat_max":1024},
            {"type":"direct","tag":"dns-in","listen":dns_listen,"listen_port":ports.dns}
        ],
        "outbounds":[
            {"type":"direct","tag":"direct","domain_resolver":resolver()},
            {"type":"vless","tag":"proxy","server":node.server,"server_port":node.port,"uuid":node.uuid,"flow":node.flow,"packet_encoding":"xudp","domain_resolver":resolver(),"connect_timeout":"10s","tcp_fast_open":true,
                "tls":{"enabled":true,"server_name":node.server_name,"utls":{"enabled":true,"fingerprint":node.fingerprint},"reality":{"enabled":true,"public_key":node.reality_public_key,"short_id":node.reality_short_id}}}
        ],
        "route":{"rules":[],"rule_set":rule_sets,"final":"proxy","auto_detect_interface":true,"default_domain_resolver":resolver()}
    });
    // Move the largest trees rather than cloning them through json! expansion.
    config["route"]["rules"] = Value::Array(route);
    config["dns"]["rules"] = Value::Array(dns);
    let mut bytes = Vec::new();
    encode_go_json(&config, 0, &mut bytes)?;
    push_bytes(&mut bytes, b"\n")?;
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    let endpoint_hosts = sorted_unique(
        std::iter::once(node.server.clone())
            .chain(input.endpoints.iter().cloned())
            .chain(bootstrap)
            .collect(),
    );
    Ok(CompileOutput {
        config: bytes,
        sha256,
        core_version: CORE_VERSION.into(),
        diagnostics,
        endpoint_hosts,
        required_features: [
            "with_utls",
            "badlinkname",
            "tcp_fast_open",
            "system_tun_tcp_udp",
            "tls_dns",
        ]
        .iter()
        .map(|s| (*s).into())
        .collect(),
        ipv6: ipv6.into(),
        failure: failure.into(),
    })
}

// Charge each rule before accumulating the next one. At most one bounded
// validated rule can exceed the remaining budget. The final writer accounts
// for inbounds/outbounds and object framing as well. No serialization cache.
fn rendered_rules_bytes(rules: &[Value]) -> usize {
    rules.iter().map(|rule| encoded_size(rule, 3) + 8).sum()
}
fn encoded_string_size(value: &str) -> usize {
    2 + value
        .chars()
        .map(|c| match c {
            '"' | '\\' | '\n' | '\r' | '\t' | '\u{8}' | '\u{c}' => 2,
            '<' | '>' | '&' | '\u{2028}' | '\u{2029}' => 6,
            c if c <= '\u{1f}' => 6,
            c => c.len_utf8(),
        })
        .sum::<usize>()
}
fn encoded_size(value: &Value, depth: usize) -> usize {
    match value {
        Value::Object(map) if !map.is_empty() => {
            2 + 1
                + depth * 2
                + map
                    .iter()
                    .map(|(key, value)| {
                        2 + (depth + 1) * 2
                            + encoded_string_size(key)
                            + 2
                            + encoded_size(value, depth + 1)
                    })
                    .sum::<usize>()
                - 1
        }
        Value::Array(values) if !values.is_empty() => {
            2 + 1
                + depth * 2
                + values
                    .iter()
                    .map(|value| 2 + (depth + 1) * 2 + encoded_size(value, depth + 1))
                    .sum::<usize>()
                - 1
        }
        Value::Object(_) | Value::Array(_) => 2,
        Value::String(value) => encoded_string_size(value),
        Value::Bool(true) | Value::Null => 4,
        Value::Bool(false) => 5,
        Value::Number(n) => n.to_string().len(),
    }
}
fn push_bytes(out: &mut Vec<u8>, bytes: &[u8]) -> Result<()> {
    if out.len().saturating_add(bytes.len()) > MAX_CONFIG_BYTES {
        return Err(error("native configuration output limit exceeded"));
    }
    out.extend_from_slice(bytes);
    Ok(())
}
fn indent(out: &mut Vec<u8>, depth: usize) -> Result<()> {
    for _ in 0..depth {
        push_bytes(out, b"  ")?;
    }
    Ok(())
}
fn encode_string(value: &str, out: &mut Vec<u8>) -> Result<()> {
    push_bytes(out, b"\"")?;
    for c in value.chars() {
        match c {
            '"' => push_bytes(out, b"\\\"")?,
            '\\' => push_bytes(out, b"\\\\")?,
            '\n' => push_bytes(out, b"\\n")?,
            '\r' => push_bytes(out, b"\\r")?,
            '\t' => push_bytes(out, b"\\t")?,
            '\u{8}' => push_bytes(out, b"\\b")?,
            '\u{c}' => push_bytes(out, b"\\f")?,
            '<' => push_bytes(out, b"\\u003c")?,
            '>' => push_bytes(out, b"\\u003e")?,
            '&' => push_bytes(out, b"\\u0026")?,
            '\u{2028}' => push_bytes(out, b"\\u2028")?,
            '\u{2029}' => push_bytes(out, b"\\u2029")?,
            c if c <= '\u{1f}' => {
                let hex = b"0123456789abcdef";
                let n = c as usize;
                push_bytes(out, &[b'\\', b'u', b'0', b'0', hex[n >> 4], hex[n & 15]])?;
            }
            c => {
                let mut utf8 = [0; 4];
                push_bytes(out, c.encode_utf8(&mut utf8).as_bytes())?;
            }
        }
    }
    push_bytes(out, b"\"")
}
fn encode_object(map: &Map<String, Value>, depth: usize, out: &mut Vec<u8>) -> Result<()> {
    push_bytes(out, b"{")?;
    if !map.is_empty() {
        // Explicit sort remains correct if another crate enables preserve_order.
        let mut keys: Vec<&String> = map.keys().collect();
        keys.sort();
        for (i, key) in keys.iter().enumerate() {
            push_bytes(out, if i == 0 { b"\n" } else { b",\n" })?;
            indent(out, depth + 1)?;
            encode_string(key, out)?;
            push_bytes(out, b": ")?;
            encode_go_json(&map[*key], depth + 1, out)?;
        }
        push_bytes(out, b"\n")?;
        indent(out, depth)?;
    }
    push_bytes(out, b"}")
}
fn encode_go_json(value: &Value, depth: usize, out: &mut Vec<u8>) -> Result<()> {
    match value {
        Value::Object(map) => encode_object(map, depth, out),
        Value::Array(values) => {
            push_bytes(out, b"[")?;
            if !values.is_empty() {
                for (i, value) in values.iter().enumerate() {
                    push_bytes(out, if i == 0 { b"\n" } else { b",\n" })?;
                    indent(out, depth + 1)?;
                    encode_go_json(value, depth + 1, out)?;
                }
                push_bytes(out, b"\n")?;
                indent(out, depth)?;
            }
            push_bytes(out, b"]")
        }
        Value::String(s) => encode_string(s, out),
        Value::Bool(true) => push_bytes(out, b"true"),
        Value::Bool(false) => push_bytes(out, b"false"),
        Value::Null => push_bytes(out, b"null"),
        Value::Number(n) => push_bytes(out, n.to_string().as_bytes()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocation_budget_counts_exact_go_rendered_bytes() {
        for value in [
            json!({}),
            json!([]),
            json!({"domain_keyword":["<>&\u{2028}\u{1}é"], "outbound":"proxy"}),
            json!({"a":[{},[],{"b":false,"c":null,"d":53}]}),
        ] {
            for depth in 0..=4 {
                let mut bytes = Vec::new();
                encode_go_json(&value, depth, &mut bytes).unwrap();
                assert_eq!(encoded_size(&value, depth), bytes.len());
            }
        }
    }
}

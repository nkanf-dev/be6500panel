//! Pure, bounded IPv4 routed-TUN capture intent. No command execution or I/O.
//!
//! Apply prepares routes/chains before installing hooks, with MARK admission
//! last. Stop on the first apply error and attempt every cleanup command.
//! Resource metadata is intent, not proof of readiness, installation or exclusive
//! ownership. A future owner must serialize generations and verify reservations.
use crate::native::Ports;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr};

pub const CAPTURE_MARK: u32 = 0x4000;
pub const CAPTURE_MASK: u32 = 0x4000;
pub const CAPTURE_TABLE: u32 = 16500;
pub const CAPTURE_PRIORITY: u32 = 16500;
pub const MAX_CAPTURE_CLIENTS_PER_FAMILY: usize = 64;
/// Includes apply, cleanup and independent failure intent.
pub const MAX_PLAN_COMMANDS: usize = 8192;
const MARK_MASK: &str = "0x4000/0x4000";
const MARK: &str = "B6P_V4_TUN_MARK";
const DNS: &str = "B6P_V4_DNS";
const FORWARD: &str = "B6P_V4_TUN_FORWARD";
const RETURN: &str = "B6P_V4_TUN_RETURN";
const INPUT: &str = "B6P_V4_TUN_INPUT";
const OUTPUT: &str = "B6P_V4_TUN_OUTPUT";
const SAFETY: &[&str] = &[
    "0.0.0.0/8",
    "127.0.0.0/8",
    "169.254.0.0/16",
    "192.0.0.0/24",
    "224.0.0.0/4",
    "240.0.0.0/4",
];
const PRIVATE: &[&str] = &[
    "10.0.0.0/8",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "100.64.0.0/10",
    "198.18.0.0/15",
];

/// Explicit lowerCamel DTO with original Go Pascal/acronym aliases.
/// Only `routed-tun` generation is admitted. IPv6 literals are retained as
/// direct-only metadata; they authorize no IPv6 or MAC hooks.
/// Optional client lists/maps preserve Go nil versus non-nil empty intent.
#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RulesPlanInput {
    #[serde(rename = "scope", alias = "Scope")]
    pub scope: String,
    #[serde(
        rename = "lanIPv4Prefixes",
        alias = "LANIPv4Prefixes",
        deserialize_with = "nil_vec"
    )]
    pub lan_ipv4_prefixes: Vec<String>,
    #[serde(rename = "datapath", alias = "Datapath")]
    pub datapath: String,
    #[serde(rename = "tunInterface", alias = "TUNInterface")]
    pub tun_interface: String,
    #[serde(rename = "tunAddress", alias = "TUNAddress")]
    pub tun_address: String,
    #[serde(rename = "clientIPv4", alias = "ClientIPv4")]
    pub client_ipv4: String,
    #[serde(rename = "clientIPv6", alias = "ClientIPv6")]
    pub client_ipv6: String,
    #[serde(rename = "clientIPv4s", alias = "ClientIPv4s")]
    pub client_ipv4s: Option<Vec<String>>,
    #[serde(rename = "clientIPv6s", alias = "ClientIPv6s")]
    pub client_ipv6s: Option<Vec<String>>,
    #[serde(rename = "clientMACs", alias = "ClientMACs")]
    pub client_macs: Option<BTreeMap<String, String>>,
    #[serde(rename = "lanInterface", alias = "LANInterface")]
    pub lan_interface: String,
    #[serde(rename = "ports", alias = "Ports", deserialize_with = "input_ports")]
    pub ports: Ports,
    #[serde(rename = "ipv6", alias = "IPv6")]
    pub ipv6: String,
    #[serde(rename = "failure", alias = "Failure")]
    pub failure: String,
    #[serde(
        rename = "endpointIPs",
        alias = "EndpointIPs",
        deserialize_with = "nil_vec"
    )]
    pub endpoint_ips: Vec<String>,
    #[serde(
        rename = "managementIPs",
        alias = "ManagementIPs",
        deserialize_with = "nil_vec"
    )]
    pub management_ips: Vec<String>,
    #[serde(
        rename = "routerDNSAddresses",
        alias = "RouterDNSAddresses",
        deserialize_with = "nil_vec"
    )]
    pub router_dns_addresses: Vec<String>,
    #[serde(rename = "fakeIP", alias = "FakeIP")]
    pub fake_ip: bool,
}

impl fmt::Debug for RulesPlanInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RulesPlanInput (private capture intent)")
    }
}

fn nil_vec<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    Ok(Option::<Vec<String>>::deserialize(d)?.unwrap_or_default())
}

fn input_ports<'de, D: Deserializer<'de>>(d: D) -> Result<Ports, D::Error> {
    #[derive(Default, Deserialize)]
    #[serde(default, deny_unknown_fields)]
    struct PortsDto {
        #[serde(rename = "mixed", alias = "Mixed")]
        mixed: u16,
        #[serde(rename = "tProxy", alias = "TProxy")]
        tproxy: u16,
        #[serde(rename = "dns", alias = "DNS")]
        dns: u16,
    }
    let dto = Option::<PortsDto>::deserialize(d)?.unwrap_or_default();
    Ok(Ports {
        mixed: dto.mixed,
        tproxy: dto.tproxy,
        dns: dto.dns,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct OwnedChain {
    pub family: u8,
    pub table: String,
    pub name: String,
    pub hook: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct RulesOwnership {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub scope: String,
    #[serde(rename = "LANIPv4Prefixes", skip_serializing_if = "Vec::is_empty")]
    pub lan_ipv4_prefixes: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub datapath: String,
    #[serde(rename = "TUNInterface", skip_serializing_if = "String::is_empty")]
    pub tun_interface: String,
    #[serde(rename = "TUNAddress", skip_serializing_if = "String::is_empty")]
    pub tun_address: String,
    pub mark: u32,
    pub mask: u32,
    pub route_table: u32,
    pub rule_priority: u32,
    #[serde(rename = "LANInterface")]
    pub lan_interface: String,
    #[serde(rename = "ClientIPv4")]
    pub client_ipv4: String,
    #[serde(rename = "ClientIPv6")]
    pub client_ipv6: String,
    #[serde(rename = "ClientIPv4s", skip_serializing_if = "Vec::is_empty")]
    pub client_ipv4s: Vec<String>,
    #[serde(rename = "ClientIPv6s", skip_serializing_if = "Vec::is_empty")]
    pub client_ipv6s: Vec<String>,
    #[serde(rename = "ClientMACs", skip_serializing_if = "BTreeMap::is_empty")]
    pub client_macs: BTreeMap<String, String>,
    pub route_families: Vec<u8>,
    pub chains: Vec<OwnedChain>,
}

/// Ordered executable/argv vectors, never shell command strings.
/// Cleanup is best-effort ALL commands, not a transaction or rollback proof.
/// OnFailure owns independent storage and supports fail-direct only.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct OwnedRulesPlan {
    pub apply: Vec<Vec<String>>,
    pub cleanup: Vec<Vec<String>>,
    pub on_failure: Vec<Vec<String>>,
    pub ownership: RulesOwnership,
    pub warnings: Vec<String>,
}

/// Fixed safe text and an optional input index, never an address or MAC literal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlanError {
    pub message: &'static str,
    pub index: Option<usize>,
}
impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message)?;
        if let Some(index) = self.index {
            write!(f, " (entry {index})")?;
        }
        Ok(())
    }
}
impl std::error::Error for PlanError {}
fn error(message: &'static str) -> PlanError {
    PlanError {
        message,
        index: None,
    }
}
fn at(message: &'static str, index: usize) -> PlanError {
    PlanError {
        message,
        index: Some(index),
    }
}

#[derive(Clone, Copy)]
struct Prefix {
    addr: Ipv4Addr,
    bits: u8,
}
impl Prefix {
    fn mask(self) -> u32 {
        u32::MAX.checked_shl(32 - u32::from(self.bits)).unwrap_or(0)
    }
    fn network(self) -> u32 {
        u32::from(self.addr) & self.mask()
    }
    fn contains(self, addr: Ipv4Addr) -> bool {
        u32::from(addr) & self.mask() == self.network()
    }
    fn overlaps(self, other: Self) -> bool {
        self.contains(other.addr) || other.contains(self.addr)
    }
    fn text(self) -> String {
        format!("{}/{}", self.addr, self.bits)
    }
}
fn prefix(raw: &str) -> Option<Prefix> {
    let (addr, bits) = raw.split_once('/')?;
    let parsed = Prefix {
        addr: addr.parse().ok()?,
        bits: bits.parse().ok()?,
    };
    (parsed.bits <= 32 && raw == parsed.text()).then_some(parsed)
}
fn rfc1918(addr: Ipv4Addr) -> bool {
    let [a, b, _, _] = addr.octets();
    a == 10 || a == 172 && (16..=31).contains(&b) || a == 192 && b == 168
}

/// Independent lexically sorted clone, exactly as Go's prefix contract.
/// Rejects host bits, overlap, public space and any broadening of the declaration.
pub fn canonical_gateway_prefixes(raw: &[String]) -> Result<Vec<String>, PlanError> {
    if raw.is_empty() || raw.len() > 8 {
        return Err(error("gateway requires 1 to 8 declared IPv4 LAN prefixes"));
    }
    let mut parsed: Vec<Prefix> = Vec::with_capacity(raw.len());
    for (i, value) in raw.iter().enumerate() {
        let p = prefix(value)
            .filter(|p| p.bits >= 8 && u32::from(p.addr) == p.network())
            .ok_or_else(|| {
                at(
                    "LANIPv4Prefixes requires canonical IPv4 network prefixes without host bits",
                    i,
                )
            })?;
        let min_bits = if p.addr.octets()[0] == 10 {
            8
        } else if p.addr.octets()[0] == 172 {
            12
        } else {
            16
        };
        if !rfc1918(p.addr) || p.bits < min_bits {
            return Err(at("LANIPv4Prefixes must be wholly inside RFC1918 space", i));
        }
        if parsed.iter().any(|existing| p.overlaps(*existing)) {
            return Err(error(
                "LANIPv4Prefixes must be disjoint without duplicate or overlapping prefixes",
            ));
        }
        parsed.push(p);
    }
    let mut canonical = raw.to_vec();
    canonical.sort();
    Ok(canonical)
}

fn literal(raw: &str) -> Option<IpAddr> {
    let addr: IpAddr = raw.parse().ok()?;
    if matches!(addr, IpAddr::V6(v6) if v6.to_ipv4_mapped().is_some()) {
        return None;
    }
    Some(addr)
}
fn address_list(raw: &[String], message: &'static str) -> Result<Vec<String>, PlanError> {
    let mut out = Vec::with_capacity(raw.len());
    for (i, raw) in raw.iter().enumerate() {
        out.push(literal(raw).ok_or_else(|| at(message, i))?.to_string());
    }
    out.sort();
    out.dedup();
    Ok(out)
}
fn unicast(addr: IpAddr) -> bool {
    !addr.is_unspecified()
        && !addr.is_loopback()
        && !addr.is_multicast()
        && addr != IpAddr::V4(Ipv4Addr::BROADCAST)
}
fn clients(
    singular: &str,
    plural: &Option<Vec<String>>,
    ipv4: bool,
) -> Result<Vec<String>, PlanError> {
    let count = usize::from(!singular.is_empty()) + plural.as_ref().map_or(0, Vec::len);
    if count > MAX_CAPTURE_CLIENTS_PER_FAMILY {
        return Err(error("at most 64 exact clients per family"));
    }
    let mut out = Vec::with_capacity(count);
    let single = (!singular.is_empty()).then_some(singular);
    for (i, raw) in single
        .into_iter()
        .chain(plural.iter().flatten().map(String::as_str))
        .enumerate()
    {
        let addr = literal(raw)
            .filter(|addr| addr.is_ipv4() == ipv4 && unicast(*addr))
            .ok_or_else(|| {
                at(
                    "client requires one unicast literal address of the selected family",
                    i,
                )
            })?;
        out.push(addr.to_string());
    }
    out.sort();
    out.dedup();
    Ok(out)
}
fn canonical_mac(raw: &str) -> Option<String> {
    // net.ParseMAC admits colon/hyphen six-octet and Cisco dotted literals.
    let bytes: Vec<u8> =
        if raw.len() == 17 && (raw.as_bytes()[2] == b':' || raw.as_bytes()[2] == b'-') {
            let separator = raw.as_bytes()[2] as char;
            let parts: Vec<_> = raw.split(separator).collect();
            if parts.len() != 6
                || parts
                    .iter()
                    .any(|part| part.len() != 2 || !part.bytes().all(|b| b.is_ascii_hexdigit()))
            {
                return None;
            }
            parts
                .iter()
                .map(|part| u8::from_str_radix(part, 16).ok())
                .collect::<Option<_>>()?
        } else if raw.len() == 14 {
            let parts: Vec<_> = raw.split('.').collect();
            if parts.len() != 3
                || parts
                    .iter()
                    .any(|part| part.len() != 4 || !part.bytes().all(|b| b.is_ascii_hexdigit()))
            {
                return None;
            }
            let mut bytes = Vec::with_capacity(6);
            for part in parts {
                bytes.push(u8::from_str_radix(&part[..2], 16).ok()?);
                bytes.push(u8::from_str_radix(&part[2..], 16).ok()?);
            }
            bytes
        } else {
            return None;
        };
    if bytes[0] & 1 != 0 || bytes.iter().all(|b| *b == 0) {
        return None;
    }
    Some(
        bytes
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(":"),
    )
}
fn client_macs(
    raw: &Option<BTreeMap<String, String>>,
    clients: &[String],
) -> Result<BTreeMap<String, String>, PlanError> {
    let raw = raw.as_ref().ok_or_else(|| {
        error("routed-tun requires exact ClientMACs for every selected IPv4 client")
    })?;
    if raw.len() > 2 * MAX_CAPTURE_CLIENTS_PER_FAMILY {
        return Err(error("at most 128 exact source IP/MAC pairs"));
    }
    let mut canonical = BTreeMap::new();
    for (i, (source, value)) in raw.iter().enumerate() {
        let source = literal(source)
            .map(|addr| addr.to_string())
            .filter(|source| clients.contains(source))
            .ok_or_else(|| at("ClientMACs keys must be selected exact IPv4 clients", i))?;
        if canonical.contains_key(&source) {
            return Err(at("duplicate canonical ClientMACs source", i));
        }
        canonical.insert(
            source,
            canonical_mac(value).ok_or_else(|| {
                at(
                    "ClientMACs requires nonzero unicast six-byte MAC literals",
                    i,
                )
            })?,
        );
    }
    if clients.iter().any(|source| !canonical.contains_key(source)) {
        return Err(error(
            "every selected IPv4 client requires an exact source MAC",
        ));
    }
    Ok(canonical)
}
fn safe_lan(name: &str) -> bool {
    (1..=15).contains(&name.len())
        && name
            .bytes()
            .enumerate()
            .all(|(i, b)| b.is_ascii_alphanumeric() || b == b'_' || i != 0 && b".:-".contains(&b))
}
fn safe_tun(name: &str) -> bool {
    name.strip_prefix("b6p-").is_some_and(|tail| {
        (1..=11).contains(&tail.len())
            && tail
                .bytes()
                .enumerate()
                .all(|(i, b)| b.is_ascii_alphanumeric() || b == b'_' || i != 0 && b == b'-')
    })
}
fn tun_addresses(
    input: &RulesPlanInput,
    prefixes: &[String],
    clients: &[String],
    management: &[String],
    endpoints: &[String],
) -> Result<(Ipv4Addr, Ipv4Addr), PlanError> {
    if !safe_tun(&input.tun_interface) || input.tun_interface == input.lan_interface {
        return Err(error(
            "TUNInterface must be a distinct owned b6p- interface name of 5 to 15 bytes",
        ));
    }
    let p = prefix(&input.tun_address)
        .filter(|p| p.bits == 30 && rfc1918(p.addr))
        .ok_or_else(|| {
            error("TUNAddress must be a private RFC1918 IPv4 host /30 with a next usable peer")
        })?;
    if u32::from(p.addr) != p.network() + 1 {
        return Err(error(
            "TUNAddress must be the first usable /30 host so the next host is its usable peer",
        ));
    }
    if prefixes
        .iter()
        .any(|value| p.overlaps(prefix(value).expect("validated prefix")))
    {
        return Err(error("TUNAddress /30 overlaps a declared LAN prefix"));
    }
    if clients
        .iter()
        .any(|value| p.contains(value.parse().expect("validated IPv4 client")))
    {
        return Err(error("TUNAddress /30 collides with a selected client"));
    }
    if management
        .iter()
        .chain(endpoints)
        .any(|value| matches!(literal(value), Some(IpAddr::V4(addr)) if p.contains(addr)))
    {
        return Err(error(
            "TUNAddress /30 collides with management or endpoint addresses",
        ));
    }
    Ok((p.addr, Ipv4Addr::from(u32::from(p.addr) + 1)))
}

/// Build only the new routed-TUN IPv4 direct/fail-direct product path.
/// Does not inspect inventory, execute argv, reserve resources or create TUNs.
pub fn plan_owned_rules(input: &RulesPlanInput) -> Result<OwnedRulesPlan, PlanError> {
    if input.endpoint_ips.len() > 256
        || input.management_ips.len() > 128
        || input.router_dns_addresses.len() > 16
    {
        return Err(error(
            "firewall input limit exceeded: at most 256 endpoints, 128 management and 16 router DNS addresses",
        ));
    }
    let gateway = match input.scope.as_str() {
        "" | "devices" => {
            if !input.lan_ipv4_prefixes.is_empty() {
                return Err(error("LANIPv4Prefixes require gateway scope"));
            }
            false
        }
        "gateway" => true,
        _ => return Err(error("invalid capture scope")),
    };
    if input.datapath != "routed-tun" {
        return Err(error("new capture requires the routed-tun datapath"));
    }
    if !matches!(input.ipv6.as_str(), "" | "direct") {
        return Err(error("routed-tun requires IPv6 direct"));
    }
    if !matches!(input.failure.as_str(), "" | "direct") {
        return Err(error("routed-tun requires fail-direct"));
    }
    if !safe_lan(&input.lan_interface) {
        return Err(error(
            "LANInterface must be a safe exact interface name of 1 to 15 bytes",
        ));
    }
    let endpoints = address_list(
        &input.endpoint_ips,
        "EndpointIPs requires literal IP addresses without prefix, zone or mapped IPv4",
    )?;
    let management = address_list(
        &input.management_ips,
        "ManagementIPs requires literal IP addresses without prefix, zone or mapped IPv4",
    )?;
    let router_dns = address_list(
        &input.router_dns_addresses,
        "RouterDNSAddresses requires literal IP addresses without prefix, zone or mapped IPv4",
    )?;
    for (i, destination) in router_dns.iter().enumerate() {
        let addr = literal(destination).expect("validated literal");
        let link_local = match addr {
            IpAddr::V4(v4) => v4.is_link_local(),
            IpAddr::V6(v6) => v6.is_unicast_link_local(),
        };
        if !unicast(addr) || link_local || !management.contains(destination) {
            return Err(at(
                "RouterDNSAddresses must be unicast router LAN addresses also present in ManagementIPs",
                i,
            ));
        }
    }
    let mut ports = input.ports.clone();
    let (prefixes, v4, v6, macs) = if gateway {
        if !input.client_ipv4.is_empty()
            || !input.client_ipv6.is_empty()
            || input.client_ipv4s.is_some()
            || input.client_ipv6s.is_some()
            || input.client_macs.is_some()
        {
            return Err(error(
                "gateway forbids exact client addresses, client lists and MAC maps",
            ));
        }
        let prefixes = canonical_gateway_prefixes(&input.lan_ipv4_prefixes)?;
        if ports.mixed == 0 || ports.dns == 0 || ports.mixed == ports.dns {
            return Err(error(
                "gateway Mixed and DNS listener ports must be actual nonzero and distinct",
            ));
        }
        // An unused compatibility field is not a reserved listener in gateway.
        ports.tproxy = 7893;
        (prefixes, Vec::new(), Vec::new(), BTreeMap::new())
    } else {
        let v4 = clients(&input.client_ipv4, &input.client_ipv4s, true)?;
        if v4.is_empty() {
            return Err(error("at least one exact IPv4 client is required"));
        }
        let v6 = clients(&input.client_ipv6, &input.client_ipv6s, false)?;
        if ports == Ports::default() {
            ports = Ports {
                mixed: 2080,
                tproxy: 7893,
                dns: 1053,
            };
        }
        if ports.mixed == 0
            || ports.tproxy == 0
            || ports.dns == 0
            || ports.mixed == ports.tproxy
            || ports.mixed == ports.dns
            || ports.tproxy == ports.dns
        {
            return Err(error("listener ports must be nonzero and distinct"));
        }
        if ports.tproxy != 7893 {
            return Err(error(
                "device routed-tun requires the unused compatibility TProxy port 7893",
            ));
        }
        let macs = client_macs(&input.client_macs, &v4)?;
        (Vec::new(), v4, v6, macs)
    };
    let (local, peer) = tun_addresses(input, &prefixes, &v4, &management, &endpoints)?;
    let mut ownership = RulesOwnership {
        datapath: "routed-tun".into(),
        tun_interface: input.tun_interface.clone(),
        tun_address: format!("{local}/30"),
        mark: CAPTURE_MARK,
        mask: CAPTURE_MASK,
        route_table: CAPTURE_TABLE,
        rule_priority: CAPTURE_PRIORITY,
        lan_interface: input.lan_interface.clone(),
        client_macs: macs,
        route_families: vec![4],
        ..RulesOwnership::default()
    };
    let sources = if gateway {
        ownership.scope = "gateway".into();
        ownership.lan_ipv4_prefixes = prefixes.clone();
        prefixes
    } else {
        if input.client_ipv4s.as_ref().is_none_or(Vec::is_empty) {
            ownership.client_ipv4 = v4[0].clone();
        } else {
            ownership.client_ipv4s = v4.clone();
        }
        if input.client_ipv6s.as_ref().is_some_and(|v| !v.is_empty()) {
            ownership.client_ipv6s = v6.clone();
        } else if let Some(addr) = v6.first() {
            ownership.client_ipv6 = addr.clone();
        }
        v4.iter().map(|client| format!("{client}/32")).collect()
    };
    let mut plan = OwnedRulesPlan {
        ownership,
        warnings: warnings(gateway, input.fake_ip),
        ..OwnedRulesPlan::default()
    };
    plan.apply.push(route(&input.tun_interface, "add"));
    for source in &sources {
        plan.apply
            .push(source_rule(&input.lan_interface, source, "add"));
    }
    for (table, name, hook) in [
        ("mangle", MARK, "PREROUTING"),
        ("nat", DNS, "PREROUTING"),
        ("filter", FORWARD, "FORWARD"),
        ("filter", RETURN, "FORWARD"),
        ("filter", INPUT, "INPUT"),
        ("filter", OUTPUT, "OUTPUT"),
    ] {
        plan.ownership.chains.push(OwnedChain {
            family: 4,
            table: table.into(),
            name: name.into(),
            hook: hook.into(),
        });
        plan.apply.push(iptables(table, &["-N", name]));
    }
    packet_chains(
        &mut plan,
        ports.dns,
        &management,
        &endpoints,
        &router_dns,
        input.fake_ip,
    );
    for chain in [FORWARD, RETURN] {
        if gateway && chain == RETURN {
            address_bypass(&mut plan, "filter", chain, &management);
        }
        for protocol in ["tcp", "udp"] {
            append(
                &mut plan,
                "filter",
                chain,
                &["-p", protocol, "-j", "ACCEPT"],
            );
        }
        append(&mut plan, "filter", chain, &["-j", "RETURN"]);
    }
    for chain in [INPUT, OUTPUT] {
        append(&mut plan, "filter", chain, &["-p", "tcp", "-j", "ACCEPT"]);
        append(&mut plan, "filter", chain, &["-j", "RETURN"]);
    }
    let mut hooks = Vec::new();
    for source in &sources {
        for protocol in ["tcp", "udp"] {
            let mut args = source_match(input, source, &plan.ownership.client_macs, gateway);
            args.extend(strings(&[
                "-o",
                &input.tun_interface,
                "-m",
                "mark",
                "--mark",
                MARK_MASK,
                "-p",
                protocol,
                "-j",
                FORWARD,
            ]));
            hooks.push(hook("filter", "FORWARD", args));
            hooks.push(hook(
                "filter",
                "FORWARD",
                strings(&[
                    "-i",
                    &input.tun_interface,
                    "-o",
                    &input.lan_interface,
                    "-d",
                    source,
                    "-p",
                    protocol,
                    "-j",
                    RETURN,
                ]),
            ));
        }
    }
    let local_host = format!("{local}/32");
    let peer_host = format!("{peer}/32");
    hooks.push(hook(
        "filter",
        "INPUT",
        strings(&[
            "-i",
            &input.tun_interface,
            "-s",
            &peer_host,
            "-d",
            &local_host,
            "-p",
            "tcp",
            "-j",
            INPUT,
        ]),
    ));
    hooks.push(hook(
        "filter",
        "OUTPUT",
        strings(&[
            "-o",
            &input.tun_interface,
            "-s",
            &local_host,
            "-d",
            &peer_host,
            "-p",
            "tcp",
            "-j",
            OUTPUT,
        ]),
    ));
    for (table, chain) in [("nat", DNS), ("mangle", MARK)] {
        for source in &sources {
            let mut args = source_match(input, source, &plan.ownership.client_macs, gateway);
            args.extend(strings(&["-j", chain]));
            hooks.push(hook(table, "PREROUTING", args));
        }
    }
    plan.apply.extend(hooks.iter().cloned());
    for mut command in hooks.into_iter().rev() {
        command[5] = "-D".into();
        command.remove(7);
        plan.cleanup.push(command);
    }
    for chain in plan.ownership.chains.iter().rev() {
        plan.cleanup
            .push(iptables(&chain.table, &["-F", &chain.name]));
        plan.cleanup
            .push(iptables(&chain.table, &["-X", &chain.name]));
    }
    for source in sources.iter().rev() {
        plan.cleanup
            .push(source_rule(&input.lan_interface, source, "del"));
    }
    plan.cleanup.push(route(&input.tun_interface, "del"));
    plan.on_failure = plan.cleanup.clone();
    if plan.apply.len() + plan.cleanup.len() + plan.on_failure.len() > MAX_PLAN_COMMANDS {
        return Err(error("capture plan command limit exceeded"));
    }
    Ok(plan)
}

fn strings(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| (*s).into()).collect()
}
fn iptables(table: &str, args: &[&str]) -> Vec<String> {
    let mut command = strings(&["iptables", "-w", "5", "-t", table]);
    command.extend(strings(args));
    command
}
fn append(plan: &mut OwnedRulesPlan, table: &str, chain: &str, args: &[&str]) {
    let mut all = vec!["-A", chain];
    all.extend_from_slice(args);
    plan.apply.push(iptables(table, &all));
}
fn hook(table: &str, chain: &str, args: Vec<String>) -> Vec<String> {
    let mut command = iptables(table, &["-I", chain, "1"]);
    command.extend(args);
    command
}
fn source_match(
    input: &RulesPlanInput,
    source: &str,
    macs: &BTreeMap<String, String>,
    gateway: bool,
) -> Vec<String> {
    let mut args = strings(&["-i", &input.lan_interface, "-s", source]);
    if !gateway {
        args.extend(strings(&[
            "-m",
            "mac",
            "--mac-source",
            &macs[source.strip_suffix("/32").expect("validated host source")],
        ]));
    }
    args
}
fn route(interface: &str, operation: &str) -> Vec<String> {
    strings(&[
        "ip", "-4", "route", operation, "default", "dev", interface, "table", "16500",
    ])
}
fn source_rule(interface: &str, source: &str, operation: &str) -> Vec<String> {
    strings(&[
        "ip", "-4", "rule", operation, "priority", "16500", "from", source, "iif", interface,
        "fwmark", MARK_MASK, "lookup", "16500",
    ])
}
fn local_bypass(plan: &mut OwnedRulesPlan, table: &str, chain: &str) {
    append(
        plan,
        table,
        chain,
        &["-m", "addrtype", "--dst-type", "LOCAL", "-j", "RETURN"],
    );
}
fn address_bypass(plan: &mut OwnedRulesPlan, table: &str, chain: &str, values: &[String]) {
    for destination in values {
        if matches!(literal(destination), Some(IpAddr::V4(_))) {
            append(
                plan,
                table,
                chain,
                &["-d", &format!("{destination}/32"), "-j", "RETURN"],
            );
        }
    }
}
fn safety_bypass(plan: &mut OwnedRulesPlan, table: &str, chain: &str) {
    for destination in SAFETY {
        append(plan, table, chain, &["-d", destination, "-j", "RETURN"]);
    }
}
fn packet_chains(
    plan: &mut OwnedRulesPlan,
    dns_port: u16,
    management: &[String],
    endpoints: &[String],
    router_dns: &[String],
    fake_ip: bool,
) {
    let mut bypass = endpoints.to_vec();
    bypass.extend_from_slice(management);
    bypass.sort();
    bypass.dedup();
    local_bypass(plan, "mangle", MARK);
    address_bypass(plan, "mangle", MARK, &bypass);
    safety_bypass(plan, "mangle", MARK);
    let dns_port = dns_port.to_string();
    for destination in router_dns {
        if matches!(literal(destination), Some(IpAddr::V4(_))) {
            for protocol in ["tcp", "udp"] {
                append(
                    plan,
                    "nat",
                    DNS,
                    &[
                        "-d",
                        &format!("{destination}/32"),
                        "-p",
                        protocol,
                        "--dport",
                        "53",
                        "-j",
                        "REDIRECT",
                        "--to-ports",
                        &dns_port,
                    ],
                );
            }
        }
    }
    local_bypass(plan, "nat", DNS);
    address_bypass(plan, "nat", DNS, management);
    safety_bypass(plan, "nat", DNS);
    for protocol in ["tcp", "udp"] {
        append(
            plan,
            "mangle",
            MARK,
            &["-p", protocol, "--dport", "53", "-j", "RETURN"],
        );
        append(
            plan,
            "nat",
            DNS,
            &[
                "-p",
                protocol,
                "--dport",
                "53",
                "-j",
                "REDIRECT",
                "--to-ports",
                &dns_port,
            ],
        );
    }
    for destination in endpoints {
        if !management.contains(destination) {
            address_bypass(plan, "nat", DNS, std::slice::from_ref(destination));
        }
    }
    if fake_ip {
        for protocol in ["tcp", "udp"] {
            append(
                plan,
                "mangle",
                MARK,
                &[
                    "-p",
                    protocol,
                    "-d",
                    "198.18.0.0/15",
                    "-j",
                    "MARK",
                    "--set-xmark",
                    MARK_MASK,
                ],
            );
        }
    }
    for destination in PRIVATE {
        append(plan, "mangle", MARK, &["-d", destination, "-j", "RETURN"]);
        append(plan, "nat", DNS, &["-d", destination, "-j", "RETURN"]);
    }
    for protocol in ["tcp", "udp"] {
        append(
            plan,
            "mangle",
            MARK,
            &["-p", protocol, "-j", "MARK", "--set-xmark", MARK_MASK],
        );
    }
    append(plan, "mangle", MARK, &["-j", "RETURN"]);
    append(plan, "nat", DNS, &["-j", "RETURN"]);
}

fn warnings(gateway: bool, fake_ip: bool) -> Vec<String> {
    let mut out = strings(&[
        "Intent only: verify unused mark 0x4000, chains, table 16500 and priority 16500; serialize generations before applying. No idempotence or rollback success is assumed.",
        "Exact source-IP/MAC and incoming-LAN-interface TCP/UDP capture only. No router OUTPUT capture or full-LAN hooks; declared-client address changes require a new coordinated plan.",
        "Require actual main-core-owned system TUN readiness, matching address/peer, return routing and firewall hook order before activation. No interface create/delete/address, sysctl, netns, offload or main-table route changes are planned.",
        "IPv6 direct deliberately installs no IPv6 capture, DNS redirect or block rules; routed-TUN follow/block are not qualified. Multicast, ESP, GRE, VPN and other transports are not claimed by this TCP/UDP path.",
        "DNS REDIRECT requires the direct DNS listener on the incoming LAN address or a suitable wildcard, not loopback only. Router-local, management and safety DNS retain their factory path except exact RouterDNSAddresses port53 opt-in; other selected-client TCP/UDP port53, including endpoint DNS, reaches managed DNS before private/endpoint bypass. Encrypted DNS needs separate policy.",
        "Private-stack INPUT accepts only TCP from the next peer to the local TUN address on the owned TUN. OUTPUT accepts only the reverse TCP tuple on that TUN; no blanket TUN or router-output acceptance is planned. Return forwarding accepts only TCP/UDP to installed declared clients on the original LAN interface, not all LAN/WAN/guest destinations.",
        "No ECM/PPE/SFE setting is changed. Verify counters and real TCP, UDP, DNS, QUIC, MTU and failure/return paths on this firmware; pure argv intent is not router acceptance.",
        "Fail-direct cleanup removes owned hooks, chains, policy rules and the ordinary TUN route, not core-created interfaces or existing DNS REDIRECT conntrack bindings. Coordinate scoped flow drain/expiry on stop or failure; no global conntrack flush is planned.",
    ]);
    if gateway {
        out[1] = "Declared-prefix and incoming-LAN-interface TCP/UDP capture only. New sources within these prefixes follow automatically; no device inventory or router OUTPUT capture. Guest/IoT/WAN interfaces remain outside this scope.".into();
    }
    if fake_ip {
        out.extend(strings(&[
        "Fake-IP identities 198.18.0.0/15 take precedence over ordinary private-network bypass only because FakeIP is enabled; synchronize native policy. Explicit management and non-DNS endpoint exemptions still win.",
        "Fail-direct withdrawal cannot make cached fake-IP identities directly routable. Coordinate resolver/classifier lifetime and client DNS cache recovery; cleanup alone is not instant direct recovery.",
    ]));
    }
    out
}

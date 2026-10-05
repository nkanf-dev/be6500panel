//! Fresh source-bound capture composition. Current LAN and accepted native
//! bytes are required on each call; journals are never activation authority.
use crate::{
    capture_lan::Snapshot,
    capture_plan::{self, RulesPlanInput},
    capture_state::Desired,
    native::Ports,
    readiness_tun::{self, Budget, Observer},
};
use serde::{
    Deserialize, Deserializer,
    de::{self, SeqAccess, Visitor},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::SystemTime,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputError {
    Off,
    Intent,
    Config,
    Lan,
    Identity,
    Endpoint,
    Limit,
    Deadline,
    Canceled,
}
impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Off => "capture intent is off",
            Self::Intent => "capture intent invalid",
            Self::Config => "accepted capture configuration invalid",
            Self::Lan => "fresh LAN scope unavailable or changed",
            Self::Identity => "selected current client identity unavailable",
            Self::Endpoint => "accepted endpoint DNS unavailable or colliding",
            Self::Limit => "capture input exceeds limit",
            Self::Deadline => "capture input deadline exceeded",
            Self::Canceled => "capture input canceled",
        })
    }
}
impl std::error::Error for InputError {}
fn check(budget: &Budget<'_>) -> Result<(), InputError> {
    budget.check().map_err(|e| match e {
        readiness_tun::TunError::Cancelled => InputError::Canceled,
        _ => InputError::Deadline,
    })
}
#[derive(Default)]
struct Text(String);
impl<'de> Deserialize<'de> for Text {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = Text;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("bounded capture field")
            }
            fn visit_str<E: de::Error>(self, s: &str) -> Result<Text, E> {
                if s.len() > 1024 {
                    return Err(E::custom("field limit"));
                }
                Ok(Text(s.into()))
            }
        }
        d.deserialize_str(V)
    }
}
struct Items<T, const N: usize>(Vec<T>);
impl<T, const N: usize> Default for Items<T, N> {
    fn default() -> Self {
        Self(Vec::new())
    }
}
impl<'de, T: Deserialize<'de>, const N: usize> Deserialize<'de> for Items<T, N> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<T, const N: usize>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>, const N: usize> Visitor<'de> for V<T, N> {
            type Value = Items<T, N>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("bounded capture items")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
                let mut items = Vec::new();
                while items.len() < N {
                    match a.next_element()? {
                        Some(item) => items.push(item),
                        None => return Ok(Items(items)),
                    }
                }
                if a.next_element::<de::IgnoredAny>()?.is_some() {
                    return Err(de::Error::custom("item limit"));
                }
                Ok(Items(items))
            }
        }
        d.deserialize_seq(V::<T, N>(std::marker::PhantomData))
    }
}
#[derive(Deserialize)]
struct Config {
    inbounds: Items<Inbound, 64>,
    outbounds: Items<Outbound, 256>,
    dns: Dns,
}
#[derive(Default, Deserialize)]
#[serde(default)]
struct Inbound {
    #[serde(rename = "type")]
    kind: Text,
    tag: Text,
    listen: Text,
    listen_port: u16,
    network: Text,
    ipv6_only: bool,
}
#[derive(Default, Deserialize)]
#[serde(default)]
struct Outbound {
    server: Text,
}
#[derive(Deserialize)]
struct Dns {
    servers: Items<DnsServer, 64>,
}
#[derive(Default, Deserialize)]
#[serde(default)]
struct DnsServer {
    #[serde(rename = "type")]
    kind: Text,
    tag: Text,
    server: Text,
    server_port: u16,
    detour: Text,
}
fn prefix(raw: &str) -> Result<(Ipv4Addr, u8), InputError> {
    let (address, bits) = raw.split_once('/').ok_or(InputError::Lan)?;
    let address = address.parse::<Ipv4Addr>().map_err(|_| InputError::Lan)?;
    let bits = bits.parse::<u8>().map_err(|_| InputError::Lan)?;
    if bits > 32 || format!("{address}/{bits}") != raw {
        return Err(InputError::Lan);
    }
    Ok((address, bits))
}
fn mask(bits: u8) -> u32 {
    u32::MAX.checked_shl(32 - u32::from(bits)).unwrap_or(0)
}
fn contains(segment: &str, address: Ipv4Addr) -> Result<bool, InputError> {
    let (network, bits) = prefix(segment)?;
    Ok(u32::from(network) & mask(bits) == u32::from(address) & mask(bits))
}
fn usable_host(address: Ipv4Addr, segments: &[String]) -> Result<bool, InputError> {
    for segment in segments {
        let (network, bits) = prefix(segment)?;
        if contains(segment, address)? {
            if bits >= 31 {
                return Ok(true);
            }
            let base = u32::from(network) & mask(bits);
            let last = base | !mask(bits);
            if u32::from(address) != base && u32::from(address) != last {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
/// Trusted fresh snapshot + internal bounded resolver. Private data is not
/// serialized and no caller can supply command arrays. All selected device
/// identities must be current; unresolved selections refuse rather than reuse.
pub fn build_from_observation(
    desired: &Desired,
    accepted: &[u8],
    observed: &Snapshot,
    budget: &Budget<'_>,
    mut resolve: impl FnMut(&str, SocketAddr, &Budget<'_>) -> Result<Vec<IpAddr>, InputError>,
) -> Result<RulesPlanInput, InputError> {
    if !desired.desired {
        return Err(InputError::Off);
    }
    check(budget)?;
    if accepted.is_empty() || accepted.len() > crate::runtime_store::MAX_STORED_CONFIG_BYTES {
        return Err(InputError::Config);
    }
    if desired.ipv6 != "direct"
        || !desired.client_ipv4.is_empty()
        || !desired.client_ipv6.is_empty()
        || desired.devices.len() > 64
    {
        return Err(InputError::Intent);
    }
    let gateway = match desired.scope.as_str() {
        "gateway" => true,
        "" | "devices" => false,
        _ => return Err(InputError::Intent),
    };
    if gateway && !desired.devices.is_empty() || !gateway && !desired.lan_ipv4_prefixes.is_empty() {
        return Err(InputError::Intent);
    }
    let target = readiness_tun::native_target(accepted)
        .map_err(|_| InputError::Config)?
        .ok_or(InputError::Config)?;
    crate::readiness_dns::native_readiness_targets(accepted).map_err(|_| InputError::Config)?;
    let config: Config = serde_json::from_slice(accepted).map_err(|_| InputError::Config)?;
    if config.inbounds.0.len() != 3
        || observed.lan_ipv4_prefixes.is_empty()
        || observed.management_ips.is_empty()
        || observed.management_ips.len() > 128
        || observed.lan_addresses.len() > 128
        || observed.devices.len() > 64
        || observed.interface_addresses.len() > 4096
    {
        return Err(InputError::Lan);
    }
    let segments = capture_plan::canonical_gateway_prefixes(&observed.lan_ipv4_prefixes)
        .map_err(|_| InputError::Lan)?;
    let local_prefix = target.address().to_string();
    let (local, bits) = prefix(&local_prefix)?;
    let tun_network = u32::from(local) & mask(bits);
    let in_tun =
        |address: IpAddr| matches!(address,IpAddr::V4(a) if u32::from(a)&mask(bits)==tun_network);
    let mut self_observed = false;
    for interface in &observed.interface_addresses {
        check(budget)?;
        for address in &interface.addresses {
            if in_tun(address.address) {
                if interface.name != target.interface_name()
                    || !interface.up
                    || address.address != IpAddr::V4(local)
                    || address.bits != 30
                    || self_observed
                {
                    return Err(InputError::Lan);
                }
                self_observed = true;
            }
        }
    }
    let mut management = Vec::with_capacity(observed.management_ips.len());
    for value in &observed.management_ips {
        let address = value.parse::<IpAddr>().map_err(|_| InputError::Lan)?;
        if in_tun(address) {
            if self_observed && address == IpAddr::V4(local) {
                continue;
            }
            return Err(InputError::Lan);
        }
        management.push(value.clone());
    }
    for segment in &segments {
        let (network, segment_bits) = prefix(segment)?;
        let common = mask(segment_bits.min(bits));
        if u32::from(network) & common == tun_network & common {
            return Err(InputError::Lan);
        }
    }
    let mut input = RulesPlanInput {
        scope: desired.scope.clone(),
        datapath: "routed-tun".into(),
        tun_interface: target.interface_name().into(),
        tun_address: local_prefix,
        lan_interface: "br-lan".into(),
        ipv6: "direct".into(),
        failure: "direct".into(),
        management_ips: management,
        ports: Ports {
            mixed: 0,
            tproxy: 7893,
            dns: 0,
        },
        ..RulesPlanInput::default()
    };
    if gateway {
        input.lan_ipv4_prefixes =
            capture_plan::canonical_gateway_prefixes(&desired.lan_ipv4_prefixes)
                .map_err(|_| InputError::Intent)?;
        for declaration in &input.lan_ipv4_prefixes {
            let (network, declared_bits) = prefix(declaration)?;
            let mut supported = false;
            for segment in &segments {
                let (_, segment_bits) = prefix(segment)?;
                if declared_bits >= segment_bits && contains(segment, network)? {
                    supported = true;
                    break;
                }
            }
            if !supported {
                return Err(InputError::Lan);
            }
        }
    } else {
        if desired.devices.is_empty() {
            return Err(InputError::Identity);
        }
        let mut clients = Vec::with_capacity(desired.devices.len());
        let mut macs = BTreeMap::new();
        let mut selected = BTreeSet::new();
        for wanted in &desired.devices {
            if !selected.insert(wanted.mac.as_str()) {
                return Err(InputError::Identity);
            }
            let mut found = observed
                .devices
                .iter()
                .filter(|device| device.mac == wanted.mac);
            let device = found.next().ok_or(InputError::Identity)?;
            if found.next().is_some() {
                return Err(InputError::Identity);
            }
            let address = device
                .ip
                .parse::<Ipv4Addr>()
                .map_err(|_| InputError::Identity)?;
            if !usable_host(address, &segments)?
                || observed.management_ips.contains(&device.ip)
                || macs.insert(device.ip.clone(), device.mac.clone()).is_some()
            {
                return Err(InputError::Identity);
            }
            clients.push(device.ip.clone());
        }
        clients.sort_unstable();
        if clients.len() == 1 {
            input.client_ipv4 = clients.pop().ok_or(InputError::Identity)?;
        } else {
            input.client_ipv4s = Some(clients);
        }
        input.client_macs = Some(macs);
    }
    let mut tags = BTreeSet::new();
    for inbound in &config.inbounds.0 {
        if !tags.insert(inbound.tag.0.as_str()) {
            return Err(InputError::Config);
        }
        match (inbound.kind.0.as_str(), inbound.tag.0.as_str()) {
            ("mixed", "mixed-in") | ("direct", "dns-in") => {
                let address = inbound
                    .listen
                    .0
                    .parse::<IpAddr>()
                    .map_err(|_| InputError::Config)?;
                if inbound.listen_port == 0 || inbound.ipv6_only || in_tun(address) {
                    return Err(InputError::Config);
                }
                if inbound.tag.0 == "mixed-in" {
                    input.ports.mixed = inbound.listen_port;
                } else {
                    if !inbound.network.0.is_empty()
                        || !address.is_ipv4()
                        || address.is_loopback()
                        || !address.is_unspecified()
                            && !observed.lan_addresses.contains(&address.to_string())
                    {
                        return Err(InputError::Config);
                    }
                    input.ports.dns = inbound.listen_port;
                }
            }
            ("tun", "tun-in") => {}
            _ => return Err(InputError::Config),
        }
    }
    for address in &observed.lan_addresses {
        let parsed = address.parse::<IpAddr>().map_err(|_| InputError::Lan)?;
        if let IpAddr::V4(a) = parsed
            && !a.is_unspecified()
            && !a.is_multicast()
            && !a.is_loopback()
            && !a.is_link_local()
        {
            input.router_dns_addresses.push(a.to_string());
        }
    }
    input.router_dns_addresses.sort_unstable();
    input.router_dns_addresses.dedup();
    if input.router_dns_addresses.len() > 16 {
        return Err(InputError::Limit);
    }
    let mut endpoints = BTreeSet::new();
    for outbound in &config.outbounds.0 {
        if !outbound.server.0.is_empty() {
            endpoints.insert(outbound.server.0.as_str());
        }
    }
    let mut bootstrap = None;
    let mut dns_tags = BTreeSet::new();
    for server in &config.dns.servers.0 {
        if !dns_tags.insert(server.tag.0.as_str()) {
            return Err(InputError::Config);
        }
        // Any literal accepted DNS destination may be stolen by the TUN
        // connected /30, including DNS reached through the proxy detour.
        if server.server.0.parse::<IpAddr>().is_ok_and(&in_tun) {
            return Err(InputError::Config);
        }
        if server.kind.0 == "fakeip" {
            input.fake_ip = true;
        }
        if server.detour.0 == "direct" && !server.server.0.is_empty() {
            endpoints.insert(server.server.0.as_str());
        }
        if server.tag.0 == "dns-local" {
            let address = server
                .server
                .0
                .parse::<IpAddr>()
                .map_err(|_| InputError::Config)?;
            if server.kind.0 != "udp"
                || server.detour.0 != "direct"
                || server.server_port == 0
                || address.is_unspecified()
                || address.is_multicast()
                || in_tun(address)
                || server.server_port == input.ports.dns
            {
                return Err(InputError::Config);
            }
            bootstrap = Some(SocketAddr::new(address, server.server_port));
        }
    }
    if endpoints.len() > 256 {
        return Err(InputError::Limit);
    }
    let mut resolved = BTreeSet::new();
    for host in endpoints {
        check(budget)?;
        let addresses = if let Ok(address) = host.parse::<IpAddr>() {
            vec![address]
        } else {
            resolve(host, bootstrap.ok_or(InputError::Endpoint)?, budget)?
        };
        if addresses.is_empty() || addresses.len() > 128 {
            return Err(InputError::Endpoint);
        }
        for address in addresses {
            if in_tun(address)
                || address.is_unspecified()
                || address.is_multicast()
                || matches!(address,IpAddr::V6(a)if a.to_ipv4_mapped().is_some())
            {
                return Err(InputError::Endpoint);
            }
            resolved.insert(address.to_string());
            if resolved.len() > 256 {
                return Err(InputError::Limit);
            }
        }
    }
    input.endpoint_ips = resolved.into_iter().collect();
    check(budget)?;
    capture_plan::plan_owned_rules(&input).map_err(|_| InputError::Config)?;
    Ok(input)
}
/// Native source composition; off intent executes neither observation nor DNS.
pub fn observe_and_build(
    observer: &mut impl Observer,
    desired: &Desired,
    accepted: &[u8],
    budget: &Budget<'_>,
) -> Result<RulesPlanInput, InputError> {
    if !desired.desired {
        return Err(InputError::Off);
    }
    check(budget)?;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|_| InputError::Lan)?
        .as_secs();
    let observed = crate::capture_lan::observe(observer, desired.scope != "gateway", now, budget)
        .map_err(|error| match error {
        crate::capture_lan::LanError::Deadline => InputError::Deadline,
        crate::capture_lan::LanError::Cancelled => InputError::Canceled,
        crate::capture_lan::LanError::Limit => InputError::Limit,
        _ => InputError::Lan,
    })?;
    check(budget)?;
    build_from_observation(
        desired,
        accepted,
        &observed,
        budget,
        |host, bootstrap, b| {
            crate::endpoint_dns::resolve(host, bootstrap, b.deadline, Some(b.cancel)).map_err(
                |error| match error {
                    crate::endpoint_dns::EndpointError::Deadline => InputError::Deadline,
                    crate::endpoint_dns::EndpointError::Canceled => InputError::Canceled,
                    _ => InputError::Endpoint,
                },
            )
        },
    )
}

//! Bounded, caller-invoked accepted-listener readiness. Selection is pure;
//! probes use literal socket addresses and never resolve a target hostname.
//! DNS success requires an actual local A answer. This does not prove Internet,
//! routed-TUN ownership, or external FRPC tunnel health. No background worker.
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub const MAX_CONFIG_BYTES: usize = 4 << 20;
pub const MAX_LISTENERS: usize = 64;
pub const MAX_ARRAY_ITEMS: usize = 256;
// A compiler rule may expand to resolve+route entries. The byte cap remains
// authoritative; this work limit is separate from an inner list's 256 items.
const MAX_RULE_ENTRIES: usize = crate::policy::MAX_RULES * 2 + 128;
const MAX_HIJACK_RULES: usize = MAX_ARRAY_ITEMS;
pub const MAX_FIELD_BYTES: usize = 1024;
pub const MAX_DNS_PACKET: usize = 4096;
const IO_SLICE: Duration = Duration::from_millis(50);
const DIAL_LIMIT: Duration = Duration::from_millis(200);
const RETRY_INTERVAL: Duration = Duration::from_millis(40);
const DNS_ATTEMPT_LIMIT: Duration = Duration::from_secs(6);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Network {
    Tcp,
    Udp,
}

/// Trusted local DTO. Debug never prints accepted addresses or DNS identities.
#[derive(Clone, PartialEq, Eq)]
pub struct ListenerTarget {
    pub network: Network,
    pub address: SocketAddr,
    /// None means a TCP listener smoke check, not DNS readiness.
    pub domain: Option<String>,
}
impl fmt::Debug for ListenerTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ListenerTarget (private)")
    }
}

/// Fixed private-safe errors. Underlying I/O/config/core output is not retained.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadinessError {
    Config,
    Address,
    Network,
    RetiredTproxy,
    MissingDnsRoute,
    NoListener,
    Domain,
    Entropy,
    Io,
    Deadline,
    Canceled,
    InvalidResponse,
    NoAnswer,
    Unavailable,
}
impl fmt::Display for ReadinessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Config => "accepted listener configuration invalid or exceeds limit",
            Self::Address => "native listener address invalid",
            Self::Network => "native listener network invalid",
            Self::RetiredTproxy => "retired TPROXY listener not supported",
            Self::MissingDnsRoute => "native DNS listener missing hijack-dns route",
            Self::NoListener => "no managed proxy listener",
            Self::Domain => "DNS readiness query domain invalid",
            Self::Entropy => "DNS readiness randomness unavailable",
            Self::Io => "local readiness socket operation failed",
            Self::Deadline => "listener readiness deadline exceeded",
            Self::Canceled => "listener readiness canceled",
            Self::InvalidResponse => "DNS readiness response malformed or invalid",
            Self::NoAnswer => "DNS readiness response has no resolved A answer",
            Self::Unavailable => "native readiness socket adapter unavailable",
        })
    }
}
impl std::error::Error for ReadinessError {}
type Result<T> = std::result::Result<T, ReadinessError>;

// Deserialize only the small selected fields. Unknown private config is skipped,
// not cloned into serde_json::Value. Neither these structs nor targets serialize.
#[derive(Default)]
struct Text(String);
impl<'de> Deserialize<'de> for Text {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct TextVisitor;
        impl Visitor<'_> for TextVisitor {
            type Value = Text;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("bounded string")
            }
            fn visit_str<E: de::Error>(self, value: &str) -> std::result::Result<Text, E> {
                if value.len() > MAX_FIELD_BYTES {
                    return Err(E::custom("field limit"));
                }
                Ok(Text(value.to_owned()))
            }
            fn visit_string<E: de::Error>(self, value: String) -> std::result::Result<Text, E> {
                if value.len() > MAX_FIELD_BYTES {
                    return Err(E::custom("field limit"));
                }
                Ok(Text(value))
            }
        }
        d.deserialize_string(TextVisitor)
    }
}
struct Items<T, const N: usize>(Vec<T>);
impl<T, const N: usize> Default for Items<T, N> {
    fn default() -> Self {
        Self(Vec::new())
    }
}
impl<'de, T: Deserialize<'de>, const N: usize> Deserialize<'de> for Items<T, N> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct ItemsVisitor<T, const N: usize>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>, const N: usize> Visitor<'de> for ItemsVisitor<T, N> {
            type Value = Items<T, N>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("bounded array")
            }
            fn visit_unit<E: de::Error>(self) -> std::result::Result<Self::Value, E> {
                Ok(Items::default())
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut result = Vec::new();
                while result.len() < N {
                    match seq.next_element()? {
                        Some(item) => result.push(item),
                        None => return Ok(Items(result)),
                    }
                }
                if seq.next_element::<de::IgnoredAny>()?.is_some() {
                    return Err(de::Error::custom("array limit"));
                }
                Ok(Items(result))
            }
        }
        d.deserialize_any(ItemsVisitor::<T, N>(std::marker::PhantomData))
    }
}
#[derive(Default)]
struct Strings(Vec<Text>);
impl Strings {
    fn values(&self) -> &[Text] {
        &self.0
    }
}
impl<'de> Deserialize<'de> for Strings {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct StringsVisitor;
        impl<'de> Visitor<'de> for StringsVisitor {
            type Value = Strings;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("bounded string or string array")
            }
            fn visit_unit<E: de::Error>(self) -> std::result::Result<Self::Value, E> {
                Ok(Strings::default())
            }
            fn visit_str<E: de::Error>(self, value: &str) -> std::result::Result<Self::Value, E> {
                if value.len() > MAX_FIELD_BYTES {
                    return Err(E::custom("field limit"));
                }
                Ok(Strings(vec![Text(value.to_owned())]))
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while values.len() < MAX_ARRAY_ITEMS {
                    match seq.next_element()? {
                        Some(value) => values.push(value),
                        None => return Ok(Strings(values)),
                    }
                }
                if seq.next_element::<de::IgnoredAny>()?.is_some() {
                    return Err(de::Error::custom("array limit"));
                }
                Ok(Strings(values))
            }
        }
        d.deserialize_any(StringsVisitor)
    }
}
#[derive(Default)]
struct Ports(Vec<u16>);
impl Ports {
    fn values(&self) -> &[u16] {
        &self.0
    }
}
impl<'de> Deserialize<'de> for Ports {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct PortsVisitor;
        impl<'de> Visitor<'de> for PortsVisitor {
            type Value = Ports;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("bounded port or port array")
            }
            fn visit_unit<E: de::Error>(self) -> std::result::Result<Self::Value, E> {
                Ok(Ports::default())
            }
            fn visit_u64<E: de::Error>(self, value: u64) -> std::result::Result<Self::Value, E> {
                let value = u16::try_from(value).map_err(|_| E::custom("port limit"))?;
                Ok(Ports(vec![value]))
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while values.len() < MAX_ARRAY_ITEMS {
                    match seq.next_element()? {
                        Some(value) => values.push(value),
                        None => return Ok(Ports(values)),
                    }
                }
                if seq.next_element::<de::IgnoredAny>()?.is_some() {
                    return Err(de::Error::custom("array limit"));
                }
                Ok(Ports(values))
            }
        }
        d.deserialize_any(PortsVisitor)
    }
}
#[derive(Default, Deserialize)]
#[serde(default)]
struct Inbound {
    #[serde(rename = "type")]
    kind: Text,
    tag: Text,
    listen: Text,
    listen_port: u16,
    network: Strings,
}
#[derive(Default, Deserialize)]
#[serde(default)]
struct RouteRule {
    action: Text,
    inbound: Strings,
    port: Ports,
}
#[derive(Default)]
struct HijackRules(Vec<RouteRule>);
impl<'de> Deserialize<'de> for HijackRules {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct RulesVisitor;
        impl<'de> Visitor<'de> for RulesVisitor {
            type Value = HijackRules;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("bounded route rules")
            }
            fn visit_unit<E: de::Error>(self) -> std::result::Result<Self::Value, E> {
                Ok(HijackRules::default())
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut kept = Vec::new();
                let mut count = 0;
                while let Some(rule) = seq.next_element::<RouteRule>()? {
                    count += 1;
                    if count > MAX_RULE_ENTRIES {
                        return Err(de::Error::custom("route entry limit"));
                    }
                    if rule.action.0 == "hijack-dns" {
                        if kept.len() == MAX_HIJACK_RULES {
                            return Err(de::Error::custom("DNS hijack rule limit"));
                        }
                        kept.push(rule);
                    }
                }
                Ok(HijackRules(kept))
            }
        }
        d.deserialize_any(RulesVisitor)
    }
}
#[derive(Default, Deserialize)]
#[serde(default)]
struct Route {
    rules: HijackRules,
}
#[derive(Default, Deserialize)]
#[serde(default)]
struct Tls {
    server_name: Text,
}
#[derive(Default, Deserialize)]
#[serde(default)]
struct DnsServer {
    tag: Text,
    server: Text,
    tls: Tls,
}
#[derive(Default, Deserialize)]
#[serde(default)]
struct DnsRule {
    server: Text,
    domain: Strings,
}
#[derive(Default, Deserialize)]
#[serde(default)]
struct Dns {
    servers: Items<DnsServer, MAX_ARRAY_ITEMS>,
    // Rules are processed by a second streaming selection pass after the
    // direct resolver identity is known, regardless of JSON field ordering.
    #[serde(rename = "rules")]
    _rules: IgnoredDnsRules,
}
#[derive(Default)]
struct IgnoredDnsRules;
impl<'de> Deserialize<'de> for IgnoredDnsRules {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct RulesVisitor;
        impl<'de> Visitor<'de> for RulesVisitor {
            type Value = IgnoredDnsRules;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("bounded DNS rules")
            }
            fn visit_unit<E: de::Error>(self) -> std::result::Result<Self::Value, E> {
                Ok(IgnoredDnsRules)
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut count = 0;
                while seq.next_element::<DnsRule>()?.is_some() {
                    count += 1;
                    if count > MAX_RULE_ENTRIES {
                        return Err(de::Error::custom("DNS entry limit"));
                    }
                }
                Ok(IgnoredDnsRules)
            }
        }
        d.deserialize_any(RulesVisitor)
    }
}
#[derive(Default, Deserialize)]
#[serde(default)]
struct Config {
    inbounds: Items<Inbound, MAX_LISTENERS>,
    route: Route,
    dns: Dns,
}

/// Select from already accepted bytes. TUN adds no invented TPROXY listener.
pub fn native_readiness_targets(raw: &[u8]) -> Result<Vec<ListenerTarget>> {
    if raw.len() > MAX_CONFIG_BYTES {
        return Err(ReadinessError::Config);
    }
    let config: Config = serde_json::from_slice(raw).map_err(|_| ReadinessError::Config)?;
    let domain = readiness_domain(raw, &config.dns)?;
    let mut targets = Vec::new();
    for inbound in &config.inbounds.0 {
        if inbound.kind.0 == "tproxy" {
            return Err(ReadinessError::RetiredTproxy);
        }
        if inbound.kind.0 != "mixed" && inbound.kind.0 != "direct" {
            continue;
        }
        let ip = match inbound.listen.0.as_str() {
            "" | "0.0.0.0" => IpAddr::V4(Ipv4Addr::LOCALHOST),
            "::" => IpAddr::V6(Ipv6Addr::LOCALHOST),
            host => host.parse().map_err(|_| ReadinessError::Address)?,
        };
        if inbound.listen_port == 0 {
            return Err(ReadinessError::Address);
        }
        let address = SocketAddr::new(ip, inbound.listen_port);
        let (mut udp, mut tcp) = (
            inbound.network.values().is_empty(),
            inbound.network.values().is_empty(),
        );
        for network in inbound.network.values() {
            match network.0.as_str() {
                "" => {
                    udp = true;
                    tcp = true;
                }
                "udp" => udp = true,
                "tcp" => tcp = true,
                _ => return Err(ReadinessError::Network),
            }
        }
        let dns = inbound.kind.0 == "direct"
            && config.route.rules.0.iter().any(|rule| {
                rule.action.0 == "hijack-dns"
                    && (rule.inbound.values().is_empty()
                        || rule
                            .inbound
                            .values()
                            .iter()
                            .any(|tag| tag.0 == inbound.tag.0))
                    && (rule.port.values().is_empty()
                        || rule.port.values().contains(&inbound.listen_port))
            });
        if inbound.kind.0 == "direct"
            && !dns
            && (inbound.tag.0 == "dns-in" || inbound.listen_port == 53)
        {
            return Err(ReadinessError::MissingDnsRoute);
        }
        if dns {
            if udp {
                targets.push(ListenerTarget {
                    network: Network::Udp,
                    address,
                    domain: Some(domain.clone()),
                });
            }
            if tcp {
                targets.push(ListenerTarget {
                    network: Network::Tcp,
                    address,
                    domain: Some(domain.clone()),
                });
            }
        } else if tcp {
            targets.push(ListenerTarget {
                network: Network::Tcp,
                address,
                domain: None,
            });
        }
    }
    if targets.is_empty() {
        return Err(ReadinessError::NoListener);
    }
    Ok(targets)
}

fn normalized_domain(domain: &str) -> Option<String> {
    let domain = domain.strip_suffix('.').unwrap_or(domain);
    if domain.is_empty() || domain.len() > 253 || domain.parse::<IpAddr>().is_ok() {
        return None;
    }
    if !domain.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    }) {
        return None;
    }
    Some(domain.to_ascii_lowercase())
}
struct DnsSelection<'a> {
    identity: Option<&'a str>,
    first: Option<String>,
    matched: bool,
}
struct DnsRulesSeed<'a, 'b>(&'a mut DnsSelection<'b>);
impl<'de> DeserializeSeed<'de> for DnsRulesSeed<'_, '_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> std::result::Result<(), D::Error> {
        d.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for DnsRulesSeed<'_, '_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded DNS rules")
    }
    fn visit_unit<E: de::Error>(self) -> std::result::Result<(), E> {
        Ok(())
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> std::result::Result<(), A::Error> {
        let mut count = 0;
        while let Some(rule) = seq.next_element::<DnsRule>()? {
            count += 1;
            if count > MAX_RULE_ENTRIES {
                return Err(de::Error::custom("DNS entry limit"));
            }
            if rule.server.0 == "dns-direct" {
                for value in rule.domain.values() {
                    if let Some(domain) = normalized_domain(&value.0) {
                        if self.0.identity == Some(domain.as_str()) {
                            self.0.matched = true;
                        }
                        if self.0.first.is_none() {
                            self.0.first = Some(domain);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
struct DnsObjectSeed<'a, 'b>(&'a mut DnsSelection<'b>);
impl<'de> DeserializeSeed<'de> for DnsObjectSeed<'_, '_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> std::result::Result<(), D::Error> {
        d.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for DnsObjectSeed<'_, '_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DNS object")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<(), A::Error> {
        let mut seen = false;
        while let Some(key) = map.next_key::<String>()? {
            if key == "rules" {
                if seen {
                    return Err(de::Error::custom("duplicate DNS rules"));
                }
                seen = true;
                map.next_value_seed(DnsRulesSeed(self.0))?;
            } else {
                map.next_value::<de::IgnoredAny>()?;
            }
        }
        Ok(())
    }
}
struct DnsConfigSeed<'a, 'b>(&'a mut DnsSelection<'b>);
impl<'de> DeserializeSeed<'de> for DnsConfigSeed<'_, '_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> std::result::Result<(), D::Error> {
        d.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for DnsConfigSeed<'_, '_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("accepted object")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<(), A::Error> {
        let mut seen = false;
        while let Some(key) = map.next_key::<String>()? {
            if key == "dns" {
                if seen {
                    return Err(de::Error::custom("duplicate DNS object"));
                }
                seen = true;
                map.next_value_seed(DnsObjectSeed(self.0))?;
            } else {
                map.next_value::<de::IgnoredAny>()?;
            }
        }
        Ok(())
    }
}
fn readiness_domain(raw: &[u8], dns: &Dns) -> Result<String> {
    let identity = dns
        .servers
        .0
        .iter()
        .filter(|server| server.tag.0 == "dns-direct")
        .find_map(|server| {
            normalized_domain(&server.tls.server_name.0)
                .or_else(|| normalized_domain(&server.server.0))
        });
    let mut selected = DnsSelection {
        identity: identity.as_deref(),
        first: None,
        matched: false,
    };
    let mut decoder = serde_json::Deserializer::from_slice(raw);
    DnsConfigSeed(&mut selected)
        .deserialize(&mut decoder)
        .map_err(|_| ReadinessError::Config)?;
    decoder.end().map_err(|_| ReadinessError::Config)?;
    if selected.matched {
        return Ok(identity.expect("matched resolver identity"));
    }
    Ok(selected
        .first
        .or(identity)
        .unwrap_or_else(|| "dns.alidns.com".to_owned()))
}

/// Build a normalized A/IN question. Network probes supply an OS-random ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestionType {
    A,
    Aaaa,
}
impl QuestionType {
    fn number(self) -> u16 {
        match self {
            Self::A => 1,
            Self::Aaaa => 28,
        }
    }
}
pub fn dns_query(domain: &str, id: u16) -> Result<Vec<u8>> {
    dns_query_for(domain, id, QuestionType::A)
}
pub fn dns_query_for(domain: &str, id: u16, question: QuestionType) -> Result<Vec<u8>> {
    let domain = normalized_domain(domain).ok_or(ReadinessError::Domain)?;
    let mut query = vec![0; 12];
    query[..2].copy_from_slice(&id.to_be_bytes());
    query[2..4].copy_from_slice(&0x0100u16.to_be_bytes());
    query[5] = 1;
    for label in domain.split('.') {
        query.push(label.len() as u8);
        query.extend_from_slice(label.as_bytes());
    }
    query.push(0);
    query.extend_from_slice(&question.number().to_be_bytes());
    query.extend_from_slice(&1u16.to_be_bytes());
    Ok(query)
}
fn word(packet: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes([packet[offset], packet[offset + 1]])
}
fn name(
    packet: &[u8],
    mut offset: usize,
    boundaries: &mut [bool; MAX_DNS_PACKET],
) -> Result<(String, usize)> {
    let mut result = String::new();
    let mut end = None;
    let mut size = 0;
    for _ in 0..packet.len() {
        let length = usize::from(*packet.get(offset).ok_or(ReadinessError::InvalidResponse)?);
        boundaries[offset] = true;
        if length & 0xc0 == 0xc0 {
            let next = *packet
                .get(offset + 1)
                .ok_or(ReadinessError::InvalidResponse)?;
            let pointer = ((length & 0x3f) << 8) | usize::from(next);
            if pointer < 12 || pointer >= offset || !boundaries[pointer] {
                break;
            }
            end.get_or_insert(offset + 2);
            offset = pointer;
            continue;
        }
        if length & 0xc0 != 0 {
            break;
        }
        offset += 1;
        if length == 0 {
            return Ok((result, end.unwrap_or(offset)));
        }
        let label = packet
            .get(offset..offset + length)
            .ok_or(ReadinessError::InvalidResponse)?;
        size += length + 1;
        if size > 254
            || !label
                .iter()
                .all(|b| (0x21..=0x7e).contains(b) && *b != b'.')
        {
            break;
        }
        if !result.is_empty() {
            result.push('.');
        }
        for b in label {
            result.push(char::from(b.to_ascii_lowercase()));
        }
        offset += length;
    }
    Err(ReadinessError::InvalidResponse)
}

/// Parse every section before accepting a reachable answer-section IN A record.
pub fn validate_dns_response(response: &[u8], query: &[u8]) -> Result<()> {
    // Preserve the original public readiness contract: an A/IN query and real
    // A answer, never an AAAA-only readiness success.
    if query.len() < 4 || query[query.len() - 4..] != [0, 1, 0, 1] {
        return Err(ReadinessError::InvalidResponse);
    }
    dns_response_addresses(response, query).map(|_| ())
}
pub fn dns_response_addresses(response: &[u8], query: &[u8]) -> Result<Vec<IpAddr>> {
    let invalid = ReadinessError::InvalidResponse;
    if !(12..=MAX_DNS_PACKET).contains(&query.len())
        || !(12..=MAX_DNS_PACKET).contains(&response.len())
    {
        return Err(invalid);
    }
    let mut query_boundaries = [false; MAX_DNS_PACKET];
    let (mut requested, query_end) = name(query, 12, &mut query_boundaries)?;
    if query_end + 4 != query.len() {
        return Err(invalid);
    }
    let question_type = word(query, query_end);
    if !matches!(question_type, 1 | 28)
        || query[query_end + 2..] != [0, 1]
        || word(query, 2) != 0x0100
        || word(query, 4) != 1
        || query[6..12] != [0; 6]
        || normalized_domain(&requested).as_deref() != Some(&requested)
    {
        return Err(invalid);
    }
    let flags = word(response, 2);
    if word(response, 0) != word(query, 0)
        || flags & 0x8000 == 0
        || flags & (0x7800 | 0x0200 | 0x0040 | 0x000f) != 0
        || word(response, 4) != 1
    {
        return Err(invalid);
    }
    let mut boundaries = [false; MAX_DNS_PACKET];
    let (question, mut offset) = name(response, 12, &mut boundaries)?;
    if offset + 4 > response.len()
        || question != requested
        || response[offset..offset + 4] != query[query_end..]
    {
        return Err(invalid);
    }
    offset += 4;
    let count: usize = (0..3)
        .map(|section| usize::from(word(response, 6 + 2 * section)))
        .sum();
    if count > response.len() / 11 {
        return Err(invalid);
    }
    let mut aliases: Vec<(String, String)> = Vec::new();
    let mut addresses = Vec::new();
    let mut all_aliases = Vec::new();
    let mut opt = false;
    for section in 0..3 {
        for _ in 0..word(response, 6 + 2 * section) {
            let (owner, next) = name(response, offset, &mut boundaries)?;
            if next + 10 > response.len() {
                return Err(invalid);
            }
            let typ = word(response, next);
            let class = word(response, next + 2);
            let start = next + 10;
            let end = start + usize::from(word(response, next + 8));
            if end > response.len() {
                return Err(invalid);
            }
            match typ {
                1 => {
                    if end - start != 4 {
                        return Err(invalid);
                    }
                    if section == 0 && class == 1 {
                        let address = Ipv4Addr::new(
                            response[start],
                            response[start + 1],
                            response[start + 2],
                            response[start + 3],
                        );
                        addresses.push((owner, IpAddr::V4(address)));
                    }
                }
                5 => {
                    let (alias, alias_end) = name(response, start, &mut boundaries)?;
                    if alias_end != end || alias.is_empty() {
                        return Err(invalid);
                    }
                    if all_aliases
                        .iter()
                        .any(|(existing, value)| existing == &owner && value != &alias)
                    {
                        return Err(invalid);
                    }
                    all_aliases.push((owner.clone(), alias.clone()));
                    if section == 0 && class == 1 {
                        if aliases
                            .iter()
                            .any(|(existing, value)| existing == &owner && value != &alias)
                        {
                            return Err(invalid);
                        }
                        aliases.push((owner, alias));
                    }
                }
                // Known name-bearing records must validate their compressed
                // RDATA too, including records outside the answer section.
                2 | 3 | 4 | 7 | 8 | 9 | 12 | 39 => {
                    let (_, consumed) = name(response, start, &mut boundaries)?;
                    if consumed != end {
                        return Err(invalid);
                    }
                }
                6 => {
                    let (_, next) = name(response, start, &mut boundaries)?;
                    let (_, consumed) = name(response, next, &mut boundaries)?;
                    if consumed + 20 != end {
                        return Err(invalid);
                    }
                }
                15 | 18 | 21 | 36 | 33 => {
                    let prefix = if typ == 33 { 6 } else { 2 };
                    if start + prefix >= end {
                        return Err(invalid);
                    }
                    let (_, consumed) = name(response, start + prefix, &mut boundaries)?;
                    if consumed != end {
                        return Err(invalid);
                    }
                }
                28 => {
                    if end - start != 16 {
                        return Err(invalid);
                    }
                    if section == 0 && class == 1 {
                        let octets: [u8; 16] =
                            response[start..end].try_into().map_err(|_| invalid)?;
                        addresses.push((owner, IpAddr::V6(Ipv6Addr::from(octets))));
                    }
                }
                16 => {
                    let mut text = start;
                    while text < end {
                        text += 1 + usize::from(response[text]);
                        if text > end {
                            return Err(invalid);
                        }
                    }
                }
                46 => {
                    if start + 18 >= end {
                        return Err(invalid);
                    }
                    let (_, consumed) = name(response, start + 18, &mut boundaries)?;
                    if consumed >= end {
                        return Err(invalid);
                    }
                }
                47 => {
                    let (_, mut bitmap) = name(response, start, &mut boundaries)?;
                    let mut previous = None;
                    while bitmap < end {
                        if bitmap + 2 > end {
                            return Err(invalid);
                        }
                        let window = response[bitmap];
                        let size = usize::from(response[bitmap + 1]);
                        if previous.is_some_and(|value| value >= window)
                            || !(1..=32).contains(&size)
                            || bitmap + 2 + size > end
                        {
                            return Err(invalid);
                        }
                        previous = Some(window);
                        bitmap += 2 + size;
                    }
                    if bitmap != end {
                        return Err(invalid);
                    }
                }
                41 => {
                    if opt || section != 2 || !owner.is_empty() || response[next + 4] != 0 {
                        return Err(invalid);
                    }
                    opt = true;
                    let mut option = start;
                    while option < end {
                        if option + 4 > end {
                            return Err(invalid);
                        }
                        option += 4 + usize::from(word(response, option + 2));
                        if option > end {
                            return Err(invalid);
                        }
                    }
                }
                _ => {}
            }
            offset = end;
        }
    }
    if offset != response.len() {
        return Err(invalid);
    }
    // Build bounded edges once, then reject cycles in linear graph walks.
    // Finding edges is at most quadratic in the <=4096-byte record budget.
    let edges: Vec<_> = all_aliases
        .iter()
        .map(|(_, alias)| all_aliases.iter().position(|(owner, _)| owner == alias))
        .collect();
    let mut visited = vec![0u8; edges.len()];
    let mut path = Vec::new();
    for start in 0..edges.len() {
        let mut current = Some(start);
        while let Some(index) = current {
            if visited[index] == 1 {
                return Err(invalid);
            }
            if visited[index] == 2 {
                break;
            }
            visited[index] = 1;
            path.push(index);
            current = edges[index];
        }
        for index in path.drain(..) {
            visited[index] = 2;
        }
    }
    for _ in 0..=aliases.len() {
        if let Some((_, alias)) = aliases.iter().find(|(owner, _)| owner == &requested) {
            // CNAME with an A at the same owner is not a valid resolved chain.
            if addresses.iter().any(|(owner, _)| owner == &requested) {
                return Err(invalid);
            }
            requested.clone_from(alias);
        } else {
            let mut resolved: Vec<IpAddr> = addresses
                .into_iter()
                .filter(|(owner, address)| {
                    owner == &requested
                        && match address {
                            IpAddr::V4(_) => question_type == 1,
                            IpAddr::V6(_) => question_type == 28,
                        }
                })
                .map(|(_, address)| address)
                .collect();
            resolved.sort_unstable();
            resolved.dedup();
            return if resolved.is_empty() {
                Err(ReadinessError::NoAnswer)
            } else {
                Ok(resolved)
            };
        }
    }
    Err(ReadinessError::NoAnswer)
}

fn check(deadline: Instant, cancel: Option<&AtomicBool>) -> Result<()> {
    if cancel.is_some_and(|flag| flag.load(Ordering::Acquire)) {
        return Err(ReadinessError::Canceled);
    }
    if Instant::now() >= deadline {
        return Err(ReadinessError::Deadline);
    }
    Ok(())
}

/// One attempt, one absolute deadline for connect/write/framing/fragmented reads.
/// Cancellation is checked at most every 50ms of blocked I/O; TCP dial <=200ms.
pub fn probe_once(
    target: &ListenerTarget,
    deadline: Instant,
    cancel: Option<&AtomicBool>,
) -> Result<()> {
    check(deadline, cancel)?;
    if target.address.port() == 0 || target.address.ip().is_unspecified() {
        return Err(ReadinessError::Address);
    }
    if target.domain.is_none() && target.network != Network::Tcp {
        return Err(ReadinessError::Network);
    }
    let query = match &target.domain {
        Some(domain) => {
            let mut nonce = [0; 2];
            getrandom::fill(&mut nonce).map_err(|_| ReadinessError::Entropy)?;
            Some(dns_query(domain, u16::from_be_bytes(nonce))?)
        }
        None => None,
    };
    #[cfg(unix)]
    {
        let result = socket_io::probe(target, query.as_deref(), deadline, cancel);
        check(deadline, cancel)?;
        result
    }
    #[cfg(not(unix))]
    {
        let _ = query;
        Err(ReadinessError::Unavailable)
    }
}

/// Retry only while invoked by the caller. No retained state or idle timer.
/// All selected transports must pass during the same attempt.
pub fn wait_readiness(raw: &[u8], deadline: Instant, cancel: Option<&AtomicBool>) -> Result<()> {
    check(deadline, cancel)?;
    let targets = native_readiness_targets(raw)?;
    loop {
        check(deadline, cancel)?;
        let mut ready = true;
        for target in &targets {
            let attempt = deadline.min(
                Instant::now()
                    + if target.domain.is_some() {
                        DNS_ATTEMPT_LIMIT
                    } else {
                        DIAL_LIMIT
                    },
            );
            match probe_once(target, attempt, cancel) {
                Ok(()) => {}
                Err(ReadinessError::Canceled) => return Err(ReadinessError::Canceled),
                Err(error @ (ReadinessError::Entropy | ReadinessError::Unavailable)) => {
                    return Err(error);
                }
                Err(_) => {
                    ready = false;
                    break;
                }
            }
        }
        check(deadline, cancel)?;
        if ready {
            return Ok(());
        }
        std::thread::sleep(RETRY_INTERVAL.min(deadline.saturating_duration_since(Instant::now())));
    }
}

pub(crate) fn exchange_dns(
    network: Network,
    address: SocketAddr,
    query: &[u8],
    deadline: Instant,
    cancel: Option<&AtomicBool>,
) -> Result<Vec<u8>> {
    check(deadline, cancel)?;
    if address.port() == 0
        || address.ip().is_unspecified()
        || !(12..=MAX_DNS_PACKET).contains(&query.len())
    {
        return Err(ReadinessError::Address);
    }
    #[cfg(unix)]
    {
        let response = socket_io::exchange(network, address, query, deadline, cancel)?;
        check(deadline, cancel)?;
        Ok(response)
    }
    #[cfg(not(unix))]
    {
        let _ = (network, address, query);
        Err(ReadinessError::Unavailable)
    }
}
pub(crate) fn truncated_dns_response(response: &[u8], query: &[u8]) -> Result<bool> {
    if !(12..=MAX_DNS_PACKET).contains(&response.len())
        || !(12..=MAX_DNS_PACKET).contains(&query.len())
    {
        return Err(ReadinessError::InvalidResponse);
    }
    let flags = word(response, 2);
    if flags & 0x0200 == 0 {
        return Ok(false);
    }
    if word(response, 0) != word(query, 0)
        || flags & 0x8000 == 0
        || flags & (0x7800 | 0x0040 | 0x000f) != 0
        || word(response, 4) != 1
    {
        return Err(ReadinessError::InvalidResponse);
    }
    let mut boundaries = [false; MAX_DNS_PACKET];
    let (requested, end) = name(query, 12, &mut boundaries)?;
    if end + 4 != query.len() {
        return Err(ReadinessError::InvalidResponse);
    }
    let mut boundaries = [false; MAX_DNS_PACKET];
    let (question, offset) = name(response, 12, &mut boundaries)?;
    if question != requested
        || offset + 4 > response.len()
        || response[offset..offset + 4] != query[end..]
    {
        return Err(ReadinessError::InvalidResponse);
    }
    Ok(true)
}

#[cfg(unix)]
mod socket_io {
    use super::*;
    use std::io::{self, Read, Write};
    use std::net::{TcpStream, UdpSocket};
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};

    fn wait(
        fd: RawFd,
        events: libc::c_short,
        deadline: Instant,
        cancel: Option<&AtomicBool>,
    ) -> Result<()> {
        loop {
            check(deadline, cancel)?;
            let remaining = deadline
                .saturating_duration_since(Instant::now())
                .min(IO_SLICE);
            let millis = remaining.as_millis().max(1) as libc::c_int;
            let mut poll = libc::pollfd {
                fd,
                events,
                revents: 0,
            };
            // SAFETY: poll points to one live initialized element; finite timeout.
            let ready = unsafe { libc::poll(&mut poll, 1, millis) };
            check(deadline, cancel)?;
            if ready > 0 {
                return Ok(());
            }
            if ready < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
                return Err(ReadinessError::Io);
            }
        }
    }
    fn transient(error: &io::Error) -> bool {
        matches!(
            error.kind(),
            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
        )
    }
    fn connect(
        address: SocketAddr,
        deadline: Instant,
        cancel: Option<&AtomicBool>,
    ) -> Result<TcpStream> {
        check(deadline, cancel)?;
        let deadline = deadline.min(Instant::now() + DIAL_LIMIT);
        let family = if address.is_ipv4() {
            libc::AF_INET
        } else {
            libc::AF_INET6
        };
        // SAFETY: socket has no pointers; ownership is adopted immediately.
        let fd = unsafe { libc::socket(family, libc::SOCK_STREAM, 0) };
        if fd < 0 {
            return Err(ReadinessError::Io);
        }
        // SAFETY: fd is a new valid socket with unique ownership.
        let owned = unsafe { OwnedFd::from_raw_fd(fd) };
        let stream = TcpStream::from(owned);
        stream
            .set_nonblocking(true)
            .map_err(|_| ReadinessError::Io)?;
        let result = match address {
            SocketAddr::V4(address) => {
                // SAFETY: zero is valid for all sockaddr fields.
                let mut raw: libc::sockaddr_in = unsafe { std::mem::zeroed() };
                raw.sin_family = libc::AF_INET as libc::sa_family_t;
                #[cfg(target_vendor = "apple")]
                {
                    raw.sin_len = std::mem::size_of::<libc::sockaddr_in>() as u8;
                }
                raw.sin_port = address.port().to_be();
                raw.sin_addr.s_addr = u32::from_ne_bytes(address.ip().octets());
                // SAFETY: pointer and length describe an initialized sockaddr.
                unsafe {
                    libc::connect(
                        stream.as_raw_fd(),
                        (&raw as *const libc::sockaddr_in).cast(),
                        std::mem::size_of_val(&raw) as libc::socklen_t,
                    )
                }
            }
            SocketAddr::V6(address) => {
                // SAFETY: zero is valid for all sockaddr fields.
                let mut raw: libc::sockaddr_in6 = unsafe { std::mem::zeroed() };
                raw.sin6_family = libc::AF_INET6 as libc::sa_family_t;
                #[cfg(target_vendor = "apple")]
                {
                    raw.sin6_len = std::mem::size_of::<libc::sockaddr_in6>() as u8;
                }
                raw.sin6_port = address.port().to_be();
                raw.sin6_flowinfo = address.flowinfo().to_be();
                raw.sin6_scope_id = address.scope_id();
                raw.sin6_addr.s6_addr = address.ip().octets();
                // SAFETY: pointer and length describe an initialized sockaddr.
                unsafe {
                    libc::connect(
                        stream.as_raw_fd(),
                        (&raw as *const libc::sockaddr_in6).cast(),
                        std::mem::size_of_val(&raw) as libc::socklen_t,
                    )
                }
            }
        };
        if result < 0 {
            let error = io::Error::last_os_error();
            if !matches!(
                error.raw_os_error(),
                Some(libc::EINPROGRESS | libc::EALREADY | libc::EWOULDBLOCK | libc::EINTR)
            ) {
                return Err(ReadinessError::Io);
            }
            wait(stream.as_raw_fd(), libc::POLLOUT, deadline, cancel)?;
            if stream
                .take_error()
                .map_err(|_| ReadinessError::Io)?
                .is_some()
            {
                return Err(ReadinessError::Io);
            }
        }
        check(deadline, cancel)?;
        Ok(stream)
    }
    fn write_all(
        stream: &mut TcpStream,
        mut bytes: &[u8],
        deadline: Instant,
        cancel: Option<&AtomicBool>,
    ) -> Result<()> {
        while !bytes.is_empty() {
            check(deadline, cancel)?;
            match stream.write(bytes) {
                Ok(0) => return Err(ReadinessError::Io),
                Ok(size) => bytes = &bytes[size..],
                Err(error) if transient(&error) => {
                    wait(stream.as_raw_fd(), libc::POLLOUT, deadline, cancel)?
                }
                Err(_) => return Err(ReadinessError::Io),
            }
        }
        Ok(())
    }
    fn read_exact(
        stream: &mut TcpStream,
        mut bytes: &mut [u8],
        deadline: Instant,
        cancel: Option<&AtomicBool>,
    ) -> Result<()> {
        while !bytes.is_empty() {
            check(deadline, cancel)?;
            match stream.read(bytes) {
                Ok(0) => return Err(ReadinessError::Io),
                Ok(size) => bytes = &mut bytes[size..],
                Err(error) if transient(&error) => {
                    wait(stream.as_raw_fd(), libc::POLLIN, deadline, cancel)?
                }
                Err(_) => return Err(ReadinessError::Io),
            }
        }
        Ok(())
    }
    pub(super) fn probe(
        target: &ListenerTarget,
        query: Option<&[u8]>,
        deadline: Instant,
        cancel: Option<&AtomicBool>,
    ) -> Result<()> {
        let Some(query) = query else {
            if target.network != Network::Tcp {
                return Err(ReadinessError::Network);
            }
            let _stream = connect(target.address, deadline, cancel)?;
            return check(deadline, cancel);
        };
        let response = exchange(target.network, target.address, query, deadline, cancel)?;
        validate_dns_response(&response, query)
    }
    pub(super) fn exchange(
        network: Network,
        address: SocketAddr,
        query: &[u8],
        deadline: Instant,
        cancel: Option<&AtomicBool>,
    ) -> Result<Vec<u8>> {
        if network == Network::Tcp {
            let mut stream = connect(address, deadline, cancel)?;
            let mut framed = Vec::with_capacity(query.len() + 2);
            framed.extend_from_slice(&(query.len() as u16).to_be_bytes());
            framed.extend_from_slice(query);
            write_all(&mut stream, &framed, deadline, cancel)?;
            let mut prefix = [0; 2];
            read_exact(&mut stream, &mut prefix, deadline, cancel)?;
            let size = usize::from(u16::from_be_bytes(prefix));
            if !(12..=MAX_DNS_PACKET).contains(&size) {
                return Err(ReadinessError::InvalidResponse);
            }
            let mut response = vec![0; size];
            read_exact(&mut stream, &mut response, deadline, cancel)?;
            check(deadline, cancel)?;
            Ok(response)
        } else {
            let bind = if address.is_ipv4() {
                "0.0.0.0:0"
            } else {
                "[::]:0"
            };
            let socket = UdpSocket::bind(bind).map_err(|_| ReadinessError::Io)?;
            socket
                .set_nonblocking(true)
                .map_err(|_| ReadinessError::Io)?;
            socket.connect(address).map_err(|_| ReadinessError::Io)?;
            loop {
                check(deadline, cancel)?;
                match socket.send(query) {
                    Ok(size) if size == query.len() => break,
                    Err(error) if transient(&error) => {
                        wait(socket.as_raw_fd(), libc::POLLOUT, deadline, cancel)?
                    }
                    _ => return Err(ReadinessError::Io),
                }
            }
            let mut response = [0; MAX_DNS_PACKET + 1];
            loop {
                check(deadline, cancel)?;
                match socket.recv(&mut response) {
                    Ok(size) => {
                        check(deadline, cancel)?;
                        if size > MAX_DNS_PACKET {
                            return Err(ReadinessError::InvalidResponse);
                        }
                        return Ok(response[..size].to_vec());
                    }
                    Err(error) if transient(&error) => {
                        wait(socket.as_raw_fd(), libc::POLLIN, deadline, cancel)?
                    }
                    Err(_) => return Err(ReadinessError::Io),
                }
            }
        }
    }
}

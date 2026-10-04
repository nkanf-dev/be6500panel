//! Fresh, bounded LAN identity. No UCI cache, journal replay or network writes.
//! Only the local Observer supplies interfaces and current lease/ARP bytes.
use crate::readiness_tun::{Budget, Interface, Observer, TunError};
use std::{
    fmt,
    net::{IpAddr, Ipv4Addr},
    path::Path,
};

pub const MAX_MANAGEMENT_IPS: usize = 128;
pub const MAX_LAN_PREFIXES: usize = 8;
pub const MAX_DEVICES: usize = 64;
pub const MAX_SOURCE_BYTES: usize = 1 << 20;
pub const MAX_SOURCE_ROWS: usize = 8192;
const MAX_INTERFACES: usize = 4096;
const LEASE_PATHS: [&str; 3] = [
    "/tmp/dhcp.leases",
    "/tmp/dnsmasq.leases",
    "/var/lib/misc/dnsmasq.leases",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LanError {
    Sources,
    Scope,
    Limit,
    Deadline,
    Cancelled,
}
impl fmt::Display for LanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Sources => "current LAN identity sources unavailable or invalid",
            Self::Scope => "current br-lan IPv4 scope unavailable",
            Self::Limit => "current LAN observation limit exceeded",
            Self::Deadline => "current LAN observation deadline exceeded",
            Self::Cancelled => "current LAN observation cancelled",
        })
    }
}
impl std::error::Error for LanError {}

#[derive(Clone, PartialEq, Eq)]
pub struct Device {
    pub mac: String,
    pub ip: String,
}
impl fmt::Debug for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Device([private])")
    }
}
/// Trusted only when returned successfully by observe in local code. Public
/// fields support the internal builder, not deserialization or client admission.
pub struct Snapshot {
    pub lan_ipv4_prefixes: Vec<String>,
    pub lan_addresses: Vec<String>,
    pub management_ips: Vec<String>,
    pub interface_addresses: Vec<Interface>,
    pub devices: Vec<Device>,
}
impl fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LanSnapshot([private])")
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Prefix {
    network: u32,
    bits: u8,
}
impl Prefix {
    fn host(self, ip: u32) -> bool {
        let mask = u32::MAX << (32 - self.bits);
        ip & mask == self.network
            && (self.bits >= 31 || ip != self.network && ip != (self.network | !mask))
    }
}
struct Scope {
    prefixes: Vec<Prefix>,
    lan: Vec<IpAddr>,
    management: Vec<IpAddr>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    Lease,
    LanArp,
    ForeignArp,
}
// Binary identities keep 16384 source rows compact; no per-row owned text.
struct Row {
    ip: u32,
    mac: [u8; 6],
    source: Source,
    conflict: bool,
}

fn error(e: TunError) -> LanError {
    match e {
        TunError::Deadline => LanError::Deadline,
        TunError::Cancelled => LanError::Cancelled,
        TunError::Limit => LanError::Limit,
        _ => LanError::Sources,
    }
}
fn check(budget: &Budget<'_>) -> Result<(), LanError> {
    budget.check().map_err(error)
}
fn valid_interface(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 15
        && name.bytes().all(|b| b > 32 && b != 127 && b != b'/')
}
fn read(
    observer: &mut impl Observer,
    path: &str,
    budget: &Budget<'_>,
) -> Result<Vec<u8>, LanError> {
    check(budget)?;
    let result = observer.read_file(Path::new(path), MAX_SOURCE_BYTES, budget);
    check(budget)?;
    let raw = result.map_err(error)?;
    if raw.len() > MAX_SOURCE_BYTES {
        return Err(LanError::Limit);
    }
    Ok(raw)
}
fn leases(observer: &mut impl Observer, budget: &Budget<'_>) -> Result<Vec<u8>, LanError> {
    for path in LEASE_PATHS {
        check(budget)?;
        let result = observer.read_file(Path::new(path), MAX_SOURCE_BYTES, budget);
        check(budget)?;
        match result {
            Ok(raw) => {
                if raw.len() > MAX_SOURCE_BYTES {
                    return Err(LanError::Limit);
                }
                return Ok(raw);
            }
            // Missing/unavailable preferred source may use the next fixed path.
            // Malformed/unsafe sources cannot be masked by a fallback.
            Err(TunError::Unavailable) => {}
            Err(e) => return Err(error(e)),
        }
    }
    Err(LanError::Sources)
}

/// Current lease OR complete br-lan ARP can identify a host, but both sources
/// must be available and well formed when devices are required. Gateway scope
/// never reads them. Expired leases do not authorize old addresses.
pub fn observe(
    observer: &mut impl Observer,
    require_devices: bool,
    now_unix: u64,
    budget: &Budget<'_>,
) -> Result<Snapshot, LanError> {
    check(budget)?;
    let result = observer.interfaces(budget);
    check(budget)?;
    let interface_addresses = result.map_err(error)?;
    let Scope {
        prefixes,
        lan: lan_addresses,
        management: management_ips,
    } = scope(&interface_addresses, budget)?;
    let devices = if require_devices {
        let mut rows = Vec::new();
        let raw = leases(observer, budget)?;
        parse_leases(&raw, now_unix, &mut rows, budget)?;
        drop(raw);
        let raw = read(observer, "/proc/net/arp", budget)?;
        parse_arp(&raw, &mut rows, budget)?;
        drop(raw);
        eligible(&mut rows, &prefixes, &management_ips, budget)?
    } else {
        Vec::new()
    };
    check(budget)?;
    let snapshot = Snapshot {
        lan_ipv4_prefixes: prefixes
            .iter()
            .map(|p| format!("{}/{}", Ipv4Addr::from(p.network), p.bits))
            .collect(),
        lan_addresses: lan_addresses.iter().map(ToString::to_string).collect(),
        management_ips: management_ips.iter().map(ToString::to_string).collect(),
        interface_addresses,
        devices,
    };
    check(budget)?;
    Ok(snapshot)
}

fn scope(
    interfaces: &[Interface],
    budget: &Budget<'_>,
) -> Result<Scope, LanError> {
    if interfaces.len() > MAX_INTERFACES {
        return Err(LanError::Limit);
    }
    let mut names = Vec::with_capacity(interfaces.len());
    let mut prefixes = Vec::new();
    let mut lan = Vec::new();
    let mut management = Vec::new();
    let mut address_count = 0usize;
    let mut lan_up = false;
    for interface in interfaces {
        check(budget)?;
        if !valid_interface(&interface.name) {
            return Err(LanError::Sources);
        }
        names.push(interface.name.as_str());
        if interface.name == "br-lan" {
            if !interface.up {
                return Err(LanError::Scope);
            }
            lan_up = true;
        }
        for address in &interface.addresses {
            address_count += 1;
            if address_count > MAX_MANAGEMENT_IPS {
                return Err(LanError::Limit);
            }
            let max_bits = if address.address.is_ipv4() { 32 } else { 128 };
            if address.bits > max_bits
                || matches!(address.address, IpAddr::V6(ip) if ip.to_ipv4_mapped().is_some())
            {
                return Err(LanError::Sources);
            }
            let ip = address.address;
            if !ip.is_unspecified() && !ip.is_multicast() {
                management.push(ip);
                if interface.name == "br-lan" {
                    lan.push(ip);
                }
            }
            if interface.name != "br-lan"
                || ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
            {
                continue;
            }
            if let IpAddr::V4(ip) = ip
                && address.bits > 0
                && address.bits < 32
            {
                let prefix = Prefix {
                    network: u32::from(ip) & (u32::MAX << (32 - address.bits)),
                    bits: address.bits,
                };
                if !prefixes.contains(&prefix) {
                    if prefixes.len() == MAX_LAN_PREFIXES {
                        return Err(LanError::Limit);
                    }
                    prefixes.push(prefix);
                }
            }
        }
    }
    names.sort_unstable();
    if names.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(LanError::Sources);
    }
    if !lan_up || prefixes.is_empty() {
        return Err(LanError::Scope);
    }
    prefixes.sort_unstable_by_key(|p| (p.network, p.bits));
    lan.sort_unstable();
    lan.dedup();
    management.sort_unstable();
    management.dedup();
    check(budget)?;
    Ok(Scope {
        prefixes,
        lan,
        management,
    })
}

fn ip(text: &str) -> Result<u32, LanError> {
    let ip: Ipv4Addr = text.parse().map_err(|_| LanError::Sources)?;
    if ip.is_unspecified() || ip.is_multicast() {
        return Err(LanError::Sources);
    }
    Ok(u32::from(ip))
}
fn mac(text: &str) -> Result<[u8; 6], LanError> {
    let mut result = [0u8; 6];
    // Standard colon/hyphen EUI-48 and Go-compatible dotted three-group form.
    if text.len() == 17 {
        let bytes = text.as_bytes();
        let separator = bytes[2];
        if separator != b':' && separator != b'-' {
            return Err(LanError::Sources);
        }
        for (i, out) in result.iter_mut().enumerate() {
            let offset = i * 3;
            if i < 5 && bytes[offset + 2] != separator {
                return Err(LanError::Sources);
            }
            *out = (hex(bytes[offset])? << 4) | hex(bytes[offset + 1])?;
        }
    } else if text.len() == 14 {
        let bytes = text.as_bytes();
        if bytes[4] != b'.' || bytes[9] != b'.' {
            return Err(LanError::Sources);
        }
        for (i, out) in result.iter_mut().enumerate() {
            let offset = i * 2 + i / 2;
            *out = (hex(bytes[offset])? << 4) | hex(bytes[offset + 1])?;
        }
    } else {
        return Err(LanError::Sources);
    }
    if result == [0; 6] || result[0] & 1 != 0 {
        return Err(LanError::Sources);
    }
    Ok(result)
}
fn hex(byte: u8) -> Result<u8, LanError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(LanError::Sources),
    }
}
fn fields<const N: usize>(line: &str) -> Result<[&str; N], LanError> {
    let mut fields = line.split_ascii_whitespace();
    let mut out = [""; N];
    for item in &mut out {
        *item = fields.next().ok_or(LanError::Sources)?;
    }
    if fields.next().is_some() {
        return Err(LanError::Sources);
    }
    Ok(out)
}
fn push(rows: &mut Vec<Row>, row: Row) -> Result<(), LanError> {
    if rows.len() == 2 * MAX_SOURCE_ROWS {
        return Err(LanError::Limit);
    }
    if rows.len() == rows.capacity() {
        let capacity = rows
            .capacity()
            .saturating_mul(2)
            .max(16)
            .min(2 * MAX_SOURCE_ROWS);
        rows.try_reserve_exact(capacity - rows.len())
            .map_err(|_| LanError::Limit)?;
    }
    rows.push(row);
    Ok(())
}
fn text(raw: &[u8]) -> Result<&str, LanError> {
    if raw.len() > MAX_SOURCE_BYTES {
        return Err(LanError::Limit);
    }
    std::str::from_utf8(raw).map_err(|_| LanError::Sources)
}
fn parse_leases(
    raw: &[u8],
    now: u64,
    rows: &mut Vec<Row>,
    budget: &Budget<'_>,
) -> Result<(), LanError> {
    for (i, line) in text(raw)?.lines().enumerate() {
        check(budget)?;
        if i >= MAX_SOURCE_ROWS {
            return Err(LanError::Limit);
        }
        if line.trim().is_empty() {
            continue;
        }
        let f = fields::<5>(line)?;
        if f[0].is_empty() || !f[0].bytes().all(|b| b.is_ascii_digit()) {
            return Err(LanError::Sources);
        }
        let expiry: u64 = f[0].parse().map_err(|_| LanError::Sources)?;
        let mac = mac(f[1])?;
        let ip = ip(f[2])?;
        if f[3].len() > 253 || f[3].chars().any(|c| c.is_control()) {
            return Err(LanError::Sources);
        }
        if expiry != 0 && expiry <= now {
            continue;
        }
        // Go lease reference rejects dates later than year 9999.
        if expiry > 253_402_300_799 {
            return Err(LanError::Sources);
        }
        push(
            rows,
            Row {
                ip,
                mac,
                source: Source::Lease,
                conflict: false,
            },
        )?;
    }
    rows.sort_unstable_by_key(|r| (r.ip, r.mac));
    if rows
        .windows(2)
        .any(|pair| pair[0].ip == pair[1].ip && pair[0].mac == pair[1].mac)
    {
        return Err(LanError::Sources);
    }
    check(budget)
}
fn parse_arp(raw: &[u8], rows: &mut Vec<Row>, budget: &Budget<'_>) -> Result<(), LanError> {
    let mut lines = text(raw)?.lines();
    let header = lines.next().ok_or(LanError::Sources)?;
    if !header.starts_with("IP address") || !header.contains("HW address") {
        return Err(LanError::Sources);
    }
    for (i, line) in lines.enumerate() {
        check(budget)?;
        if i >= MAX_SOURCE_ROWS {
            return Err(LanError::Limit);
        }
        if line.trim().is_empty() {
            continue;
        }
        let f = fields::<6>(line)?;
        let ip = ip(f[0])?;
        let _hardware = u32::from_str_radix(f[1].strip_prefix("0x").unwrap_or(f[1]), 16)
            .map_err(|_| LanError::Sources)?;
        let flags = u32::from_str_radix(f[2].strip_prefix("0x").unwrap_or(f[2]), 16)
            .map_err(|_| LanError::Sources)?;
        if !valid_interface(f[5]) {
            return Err(LanError::Sources);
        }
        if flags & 2 == 0 {
            continue;
        }
        push(
            rows,
            Row {
                ip,
                mac: mac(f[3])?,
                source: if f[5] == "br-lan" {
                    Source::LanArp
                } else {
                    Source::ForeignArp
                },
                conflict: false,
            },
        )?;
    }
    check(budget)
}

fn eligible(
    rows: &mut [Row],
    prefixes: &[Prefix],
    management: &[IpAddr],
    budget: &Budget<'_>,
) -> Result<Vec<Device>, LanError> {
    check(budget)?;
    rows.sort_unstable_by_key(|r| (r.ip, r.mac));
    check(budget)?;
    let mut start = 0;
    while start < rows.len() {
        check(budget)?;
        let end = start + rows[start..].partition_point(|r| r.ip == rows[start].ip);
        let conflict = rows[start].mac != rows[end - 1].mac;
        for row in &mut rows[start..end] {
            row.conflict = conflict;
        }
        start = end;
    }
    rows.sort_unstable_by_key(|r| (r.mac, r.ip));
    check(budget)?;
    let mut out = Vec::new();
    start = 0;
    while start < rows.len() {
        check(budget)?;
        let end = start + rows[start..].partition_point(|r| r.mac == rows[start].mac);
        let group = &rows[start..end];
        let mut leases = group.iter().filter(|r| r.source == Source::Lease);
        let lease = leases.next();
        if leases.next().is_some() {
            start = end;
            continue;
        }
        // Current lease suppresses old ARP addresses for this MAC, but IP-owner
        // conflicts were marked before suppression and cannot disappear.
        let candidate = match lease {
            Some(row) => Some(row),
            None if group[0].ip == group[group.len() - 1].ip => Some(&group[0]),
            None => None,
        };
        if let Some(row) = candidate {
            let address = Ipv4Addr::from(row.ip);
            let foreign = group.iter().any(|r| r.ip == row.ip && r.source == Source::ForeignArp);
            if !row.conflict
                && !foreign
                && !address.is_loopback()
                && !address.is_link_local()
                && address != Ipv4Addr::BROADCAST
                && !management.contains(&IpAddr::V4(address))
                && prefixes.iter().any(|p| p.host(row.ip))
            {
                if out.len() == MAX_DEVICES {
                    return Err(LanError::Limit);
                }
                out.push((row.ip, row.mac));
            }
        }
        start = end;
    }
    // Sort compact identities first; format only the at-most-64 accepted hosts.
    out.sort_unstable();
    let devices = out
        .into_iter()
        .map(|(ip, mac)| Device {
            mac: format!(
                "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
            ),
            ip: Ipv4Addr::from(ip).to_string(),
        })
        .collect();
    check(budget)?;
    Ok(devices)
}

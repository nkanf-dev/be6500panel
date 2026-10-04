//! Caller-driven direct endpoint lookup through an accepted literal bootstrap.
//! Reuses strict DNS parser and finite socket I/O; no system resolver, worker,
//! configuration writes, captured DNS listener or retained lookup cache.
use crate::readiness_dns::{self, Network, QuestionType, ReadinessError};
use std::{
    fmt,
    net::{IpAddr, SocketAddr},
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};
pub const MAX_ADDRESSES: usize = 128;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndpointError {
    Invalid,
    Unavailable,
    Deadline,
    Canceled,
    Unresolved,
    Limit,
}
impl fmt::Display for EndpointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Invalid => "endpoint DNS input or response invalid",
            Self::Unavailable => "endpoint DNS transport unavailable",
            Self::Deadline => "endpoint DNS deadline exceeded",
            Self::Canceled => "endpoint DNS canceled",
            Self::Unresolved => "endpoint DNS has no usable address",
            Self::Limit => "endpoint DNS address limit exceeded",
        })
    }
}
impl std::error::Error for EndpointError {}
fn map(error: ReadinessError) -> EndpointError {
    match error {
        ReadinessError::Deadline => EndpointError::Deadline,
        ReadinessError::Canceled => EndpointError::Canceled,
        ReadinessError::NoAnswer => EndpointError::Unresolved,
        ReadinessError::InvalidResponse | ReadinessError::Domain | ReadinessError::Address => {
            EndpointError::Invalid
        }
        _ => EndpointError::Unavailable,
    }
}
fn check(deadline: Instant, cancel: Option<&AtomicBool>) -> Result<(), EndpointError> {
    if cancel.is_some_and(|c| c.load(Ordering::Acquire)) {
        return Err(EndpointError::Canceled);
    }
    if Instant::now() >= deadline {
        return Err(EndpointError::Deadline);
    }
    Ok(())
}
fn usable(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(a) => {
            !a.is_unspecified() && !a.is_multicast() && !a.is_broadcast() && !a.is_link_local()
        }
        IpAddr::V6(a) => {
            !a.is_unspecified()
                && !a.is_multicast()
                && !a.is_unicast_link_local()
                && a.to_ipv4_mapped().is_none()
        }
    }
}
pub fn resolve(
    host: &str,
    bootstrap: SocketAddr,
    deadline: Instant,
    cancel: Option<&AtomicBool>,
) -> Result<Vec<IpAddr>, EndpointError> {
    check(deadline, cancel)?;
    if bootstrap.port() == 0
        || !usable(bootstrap.ip())
        || matches!(bootstrap,SocketAddr::V6(a)if a.scope_id()!=0)
    {
        return Err(EndpointError::Invalid);
    }
    if let Ok(address) = host.parse::<IpAddr>() {
        return if usable(address) {
            Ok(vec![address])
        } else {
            Err(EndpointError::Invalid)
        };
    }
    let mut addresses = Vec::new();
    for question in [QuestionType::A, QuestionType::Aaaa] {
        check(deadline, cancel)?;
        let mut nonce = [0u8; 2];
        getrandom::fill(&mut nonce).map_err(|_| EndpointError::Unavailable)?;
        let query =
            readiness_dns::dns_query_for(host, u16::from_be_bytes(nonce), question).map_err(map)?;
        let mut response =
            readiness_dns::exchange_dns(Network::Udp, bootstrap, &query, deadline, cancel)
                .map_err(map)?;
        if readiness_dns::truncated_dns_response(&response, &query).map_err(map)? {
            response =
                readiness_dns::exchange_dns(Network::Tcp, bootstrap, &query, deadline, cancel)
                    .map_err(map)?;
        }
        match readiness_dns::dns_response_addresses(&response, &query) {
            Ok(found) => {
                if found.len() > MAX_ADDRESSES.saturating_sub(addresses.len()) {
                    return Err(EndpointError::Limit);
                }
                if found.iter().any(|a| !usable(*a)) {
                    return Err(EndpointError::Invalid);
                }
                addresses.extend(found);
            }
            Err(ReadinessError::NoAnswer) => {}
            Err(error) => return Err(map(error)),
        }
    }
    check(deadline, cancel)?;
    addresses.sort_unstable();
    addresses.dedup();
    if addresses.is_empty() {
        Err(EndpointError::Unresolved)
    } else {
        Ok(addresses)
    }
}

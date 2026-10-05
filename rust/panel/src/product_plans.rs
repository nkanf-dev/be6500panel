//! Existing credential-free product plans. Planning never changes resources.
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApiError {
    pub status: u16,
    pub code: &'static str,
    pub message: &'static str,
}
fn invalid() -> ApiError {
    ApiError {
        status: 400,
        code: "invalid_input",
        message: "Plan fields or values are invalid.",
    }
}
#[derive(Default)]
pub struct Plans {
    generation: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Proxy {
    mode: String,
    dns_strategy: String,
    ipv6_policy: String,
    failure_policy: String,
    node_count: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Frpc {
    server_address: String,
    server_port: u16,
    tls: bool,
    transport: String,
    proxies: Vec<ProxyEntry>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ProxyEntry {
    name: String,
    #[serde(rename = "type")]
    kind: String,
    local_address: String,
    local_port: u16,
    #[serde(default)]
    remote_port: Option<u16>,
    #[serde(default)]
    domains: Vec<String>,
}
fn dns(host: &str) -> bool {
    let host = host.strip_suffix('.').unwrap_or(host);
    !host.is_empty()
        && host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}
fn host(raw: &str) -> bool {
    raw.parse::<std::net::IpAddr>().is_ok() || dns(raw)
}
fn name(raw: &str) -> bool {
    !raw.is_empty()
        && raw.len() <= 64
        && raw
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}
impl Plans {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn plan(&mut self, path: &str, body: &[u8]) -> Result<Value, ApiError> {
        if body.len() > 64 << 10
            || body
                .iter()
                .find(|b| !b.is_ascii_whitespace())
                .is_none_or(|b| *b != b'{')
        {
            return Err(invalid());
        }
        let (summary, steps, warnings) = match path {
            "/api/proxy/plan" => {
                let input: Proxy = serde_json::from_slice(body).map_err(|_| invalid())?;
                if !matches!(input.mode.as_str(), "split" | "global" | "direct")
                    || !matches!(input.dns_strategy.as_str(), "split" | "direct")
                    || !matches!(input.ipv6_policy.as_str(), "follow" | "direct" | "block")
                    || !matches!(input.failure_policy.as_str(), "direct" | "block-proxy")
                    || input.node_count > 4096
                    || (input.mode != "direct" && input.node_count == 0)
                {
                    return Err(invalid());
                }
                let mut warnings = vec![
                    "IP alone cannot reliably classify domestic/foreign destinations; explicit overrides and maintained domain/IP policies are required.",
                    "Management, LAN and node-endpoint exclusions are plan requirements, not installed rules.",
                    "No proxy runtime or network changes are performed by this plan.",
                ];
                if input.ipv6_policy == "follow" {
                    warnings.push("Following policy on IPv6 requires a verified capture and DNS path before apply.");
                }
                if input.failure_policy == "direct" {
                    warnings.push(
                        "Direct fallback can send traffic outside the proxy if the runtime fails.",
                    );
                }
                (
                    format!(
                        "Read-only {} proxy plan ({} nodes)",
                        input.mode, input.node_count
                    ),
                    vec![
                        json!({"module":"network","action":"exclude","detail":"Keep management addresses, LAN destinations and configured node endpoints outside capture."}),
                        json!({"module":"dns","action":"plan","detail":format!("Coordinate resolver path with dnsStrategy={}; avoid resolver/capture loops.",input.dns_strategy)}),
                        json!({"module":"network","action":"plan","detail":format!("Plan IPv6 handling with ipv6Policy={}; network retains ownership of interfaces and routes.",input.ipv6_policy)}),
                        json!({"module":"firewall","action":"plan","detail":format!("Request capture/forwarding contributions with failurePolicy={}; firewall owns chains and marks.",input.failure_policy)}),
                        json!({"module":"proxy","action":"validate","detail":format!("Validate {} policy for {} credential-free node placeholders; no runtime starts.",input.mode,input.node_count)}),
                    ],
                    warnings,
                )
            }
            "/api/frpc/plan" => {
                let input: Frpc = serde_json::from_slice(body).map_err(|_| invalid())?;
                if !host(&input.server_address)
                    || input.server_port == 0
                    || !matches!(input.transport.as_str(), "tcp" | "quic")
                    || !(1..=64).contains(&input.proxies.len())
                {
                    return Err(invalid());
                }
                let mut seen = BTreeSet::new();
                for proxy in &input.proxies {
                    if !name(&proxy.name)
                        || !seen.insert(&proxy.name)
                        || !host(&proxy.local_address)
                        || proxy.local_port == 0
                        || !matches!(proxy.kind.as_str(), "tcp" | "udp" | "http" | "https")
                    {
                        return Err(invalid());
                    }
                    if matches!(proxy.kind.as_str(), "tcp" | "udp") {
                        if proxy.remote_port.is_none_or(|port| port == 0)
                            || !proxy.domains.is_empty()
                        {
                            return Err(invalid());
                        }
                    } else {
                        if proxy.remote_port.is_some() || !(1..=64).contains(&proxy.domains.len()) {
                            return Err(invalid());
                        }
                        let mut domains = BTreeSet::new();
                        for domain in &proxy.domains {
                            if !dns(domain) || !domains.insert(domain.to_lowercase()) {
                                return Err(invalid());
                            }
                        }
                    }
                }
                let mut warnings = vec![
                    "Reverse tunnels can expose local services to the public internet; explicit authorization and access control are required.",
                    "No frpc process, listener, firewall rule or public endpoint is created by this plan.",
                    "Server authentication credentials are intentionally excluded; credentials remain private.",
                ];
                if !input.tls {
                    warnings.push("TLS is disabled in this proposal; verify transport encryption before deployment.");
                }
                (
                    format!("Read-only frpc plan ({} tunnels)", input.proxies.len()),
                    vec![
                        json!({"module":"frpc","action":"validate","detail":format!("Validate {} {} tunnel definitions; no server connection is made.",input.proxies.len(),input.transport)}),
                        json!({"module":"network","action":"plan","detail":"Review server reachability and management exclusions; do not change interfaces or routes."}),
                        json!({"module":"firewall","action":"review","detail":"Review public exposure and service access controls before apply."}),
                        json!({"module":"frpc","action":"review","detail":"Verify server permissions, authentication, transport security and per-service acceptance."}),
                    ],
                    warnings,
                )
            }
            _ => {
                return Err(ApiError {
                    status: 404,
                    code: "not_found",
                    message: "Plan endpoint is unavailable.",
                });
            }
        };
        self.generation = self.generation.checked_add(1).ok_or_else(invalid)?;
        let mut entropy = [0; 16];
        getrandom::fill(&mut entropy).map_err(|_| ApiError {
            status: 500,
            code: "plan_unavailable",
            message: "Plan identifier is unavailable.",
        })?;
        let id = entropy
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        Ok(
            json!({"id":id,"generation":self.generation,"readOnly":true,"summary":summary,"steps":steps,"warnings":warnings,"canApply":false}),
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plans_preserve_readonly_wire_and_bounded_validation() {
        let mut plans = Plans::new();
        let value=plans.plan("/api/proxy/plan",br#"{"mode":"split","dnsStrategy":"split","ipv6Policy":"direct","failurePolicy":"direct","nodeCount":2}"#).unwrap();
        assert_eq!(value["readOnly"], true);
        assert_eq!(value["canApply"], false);
        assert_eq!(value["steps"].as_array().unwrap().len(), 5);
        assert!(plans.plan("/api/proxy/plan",br#"{"mode":"global","dnsStrategy":"split","ipv6Policy":"direct","failurePolicy":"direct","nodeCount":0}"#).is_err());
        assert!(plans.plan("/api/frpc/plan",br#"{"serverAddress":"relay.example","serverPort":7000,"tls":true,"transport":"quic","proxies":[{"name":"web","type":"https","localAddress":"localhost","localPort":443,"domains":["public.example"]}]}"#).is_ok());
    }
    #[test]
    fn credentials_and_unsupported_tunnel_fields_are_not_plan_inputs() {
        let mut plans = Plans::new();
        assert!(plans.plan("/api/frpc/plan",br#"{"serverAddress":"user@relay.example","serverPort":7000,"tls":true,"transport":"tcp","proxies":[]}"#).is_err());
    }
}

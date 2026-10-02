# Split-routing policy

## Resource ownership

Proxy produces routing intent. Network owns policy tables and routes; firewall owns marks and chains; DNS owns resolver forwarding. A coordinated operation records its contributions and removes only those contributions on stop or rollback. Factory LAN management traffic, loopback, private/linked networks and the proxy endpoints bypass capture.

## Rule order

1. Management and bootstrap bypass.
2. Per-device and explicit domain/network overrides.
3. Explicit blocked targets.
4. Domestic domain set: direct; resolve through the direct resolver path.
5. Domestic IP set: direct for destinations not classified by a domain rule.
6. Selected foreign domain set: proxy; resolve through the proxy resolver path.
7. Unknown destinations: selected default policy, initially proxy in split mode.

Private and link-local address handling is explicit for IPv4 and IPv6. Classification is not guessed from a domain suffix alone. CDN names and resolved IPs may disagree; domain intent takes precedence when the flow is associated with a domain. Per-rule diagnosis shows the input, matching rule, DNS path and final outbound.

## DNS

LAN DNS requests enter the DNS module's coordinated resolver path. Domestic/bootstrap names use a directly reachable resolver; foreign names use the proxy resolver. The resolver never needs the proxy to resolve the first proxy endpoint or runtime download host. Cache size and TTL are bounded. A failed update retains the last accepted rule set instead of activating a partially downloaded one.

Endpoint IPs and DNS identities are refreshed with the active network generation. Exemption refresh must precede changing a proxy server address. TCP DNS and UDP DNS are both considered. Applications with private encrypted DNS require explicit policy rather than an assumption that all names pass through port53.

## UDP and IPv6

The target core supports VLESS XUDP. A TCP-only REDIRECT rule is not sufficient for LAN-wide transparent operation; TCP and UDP use coordinated TPROXY paths. DNS, QUIC application traffic and return routing are tested separately.

IPv6 modes: `follow` applies the same rule intent with IPv6 capture and bypasses; `direct` deliberately does not proxy IPv6; `block` rejects captured IPv6 where supported. The UI must show the selected policy. LAN-wide split mode is not declared active while only IPv4 capture is verified.

## Failure policy

`direct` withdraws owned capture state and restores ordinary forwarding when runtime bootstrap or the core fails. `block-proxy` preserves direct domestic/management traffic and rejects only proxy-selected traffic. Classification-dependent blocking requires DNS/rule state to remain available; it is not equivalent to dropping all WAN access.

A monitor detects process exit and data-path failure. Recovery uses bounded backoff. An unavailable proxy does not remove the panel/SSH management path. Node selection avoids concurrent probes of every subscribed node; probes use bounded concurrency and hysteresis to avoid flapping.

## Acceptance sequence

1. Validate a native minimal node config and explicit mixed SOCKS/HTTP connectivity.
2. Observe RAM, CPU, uptime, connection/error counters and UDP behavior.
3. Capture one selected client; verify rule counters, resolver path, IPv4/IPv6, management bypass and return routing.
4. Test core termination, rejected config, broken download, WAN renew and firewall reload.
5. Test the QSDK ECM/PPE/SFE path with counters and actual connections, then enable broader capture.
6. Test reboot reconstruction and a previous-generation rollback.

Runtime binary/version/hash, rules revision, operation generation and last successful verification belong in diagnostics. Historical samples are bounded; long retention belongs on an external collector.

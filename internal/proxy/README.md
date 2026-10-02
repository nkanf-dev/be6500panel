# Native proxy engine

`internal/proxy` imports private subscription data, compiles native sing-box **1.14.2** JSON, and returns intended single-client network commands. It does not fetch subscriptions, resolve hosts, open connections, run commands, or contact the router.

## Root API

```go
sub, err := proxy.ParseClashYAML(reader) // at most 2 MiB
nodes := sub.PublicNodes()              // authenticated UI only
out, err := proxy.CompileNative(proxy.CompileInput{
    Node: sub.Nodes[selected],
    Rules: sub.Rules,
    Diagnostics: sub.Diagnostics,
    // Rules not supported by forwarded traffic require explicit acknowledgement.
    AcceptUnsupportedRules: acknowledged,
    RuleSets: stagedReferences,
    ManagementIPs: managementIPs,
    Endpoints: resolvedEndpointIPs,
    BootstrapDomains: artifactBootstrapDomains,
    IPv6: proxy.IPv6Direct,
    Failure: proxy.FailureDirect,
})
```

- `ParseClashYAML(io.Reader) (Subscription, error)`: bounded YAML, at most 2048 nodes, 8192 rules, 128 groups. A conservative pre-decode 65536-unit lexical budget and collection/indentation admission limits bound YAML tree amplification. Anchors, aliases, duplicate keys, multiple documents, excessive nesting/scalars are rejected. A syntax-agnostic lexical budget can reject unusually punctuation-heavy otherwise valid subscriptions; it is separate from the byte/node/rule count limits. Entries require VLESS native TCP, TLS REALITY, Vision, Chrome uTLS and UDP/XUDP. No public conversion service is used.
- `Subscription.Nodes []Node`: private UUID/server name/REALITY parameters. `Node.Public()` / `Subscription.PublicNodes()` return `{id,label,server,port,protocol,transport,reality,vision,utls,udp}`. Label is the original node name. The view is suitable for authenticated node selection, not public fixture/history publication. UUID and REALITY material are never projected. Config/input formatting and JSON exclude private data.
- `Subscription.Rules []Rule`: domain, domain-suffix, domain-keyword, IPv4/IPv6 CIDR, MATCH/FINAL, and CN GEOIP/GEOSITE as controlled local SRS references. Original rule order and no-resolve flags are kept. DIRECT/REJECT/PROXY and known node/group targets are recognized. Named selectors collapse to the selected node; there is no concurrent all-node probing or selector runtime. Unsupported process rules produce indexed fixed diagnostics, since router process names cannot classify forwarded client traffic. Unknown rules/targets are never silently converted. `AcceptUnsupportedRules` is an explicit activation acknowledgement.
- `CompileNative(CompileInput) (CompileOutput,error)`: `Config` contains private native JSON. `SHA256` covers its exact bytes including the trailing newline. JSON public output excludes the config and endpoint hosts. No file is written. The owner must atomically persist Config with mode **0600**, run target `sing-box check -c <private-file>`, and serialize activation with rollback.
- `VerifyRuleSet(RuleSetReference) error`: separately reads a bounded staged local SRS and checks SHA256. `CompileNative` only validates references and emits local binary rule sets. It does not read/download them. Root must verify files immediately before check/start and retain the last accepted revision. Tags are `cn-domain` (domain), `cn-ip` (ip), and optional `proxy-domain` (domain). Paths must be clean absolute `.srs` paths. SHA256 and `MaxBytes` are mandatory (maximum 8 MiB each). Optional source provenance must be credential-free HTTPS and is not embedded in core config. Root controls source selection and hashes; this package has no built-in public rule download URLs. Missing CN sets produce `cn-rules-incomplete`, not a false claim of domestic classification.

### CompileInput listeners and defaults

`Ports{Mixed,TProxy,DNS}` defaults to `{2080,7893,1053}`. Ports must be nonzero and distinct.

`ListenAddress` is a shared shorthand. `MixedListenAddress`, `TProxyListenAddress`, and `DNSListenAddress` override it separately:

- Mixed defaults to `127.0.0.1`. This is a native SOCKS5/HTTP mixed listener. SOCKS5 supports TCP and UDP; the outbound uses `packet_encoding: xudp` and does not set the TCP-only `network` restriction.
- TPROXY defaults to `127.0.0.1` for IPv4; `::` for IPv6 follow. Root must verify Linux dual-stack IPv4-mapped acceptance for `::`, matching the planner's loopback `--on-ip` addresses.
- DNS defaults to `127.0.0.1`, for local/dnsmasq forwarding. It is a native direct inbound with a `hijack-dns` route action, not the removed DNS outbound. NAT REDIRECT needs a LAN-address or wildcard DNS bind, because the destination becomes the ingress-interface address, **not loopback**. Root must coordinate this bind with client-scoped INPUT access/factory firewall; DNS wildcard binding is not a license to expose an open resolver.
- Never use a wildcard shared `ListenAddress` merely to expose DNS: it also widens the anonymous mixed listener. Use the separate fields. No authentication secrets are invented by the compiler.

### Policy and native DNS

Management/endpoint/bootstrap and explicit private/link-local bypasses precede overrides. Overrides precede supported imported rules. Imported rules preserve relative order. Before terminal MATCH, controlled CN-domain direct intent precedes optional foreign-domain proxy intent, then CN-IP fallback; CN-IP resolves unclassified names via the proxy resolver only after domain intent. Without MATCH the default is proxy. Later rules after MATCH get an unreachable diagnostic and do not influence DNS. CIDR `no-resolve` does not generate an eager resolve action.

Native typed DNS servers use TLS IP-addressed endpoints with explicit certificate `server_name`. Defaults are `223.5.5.5:853` / `dns.alidns.com` direct and `1.1.1.1:853` / `cloudflare-dns.com` proxy. Root may override these through `DNSEndpoint`. Node endpoint names always bootstrap through the direct resolver. Add artifact/rule hostnames to BootstrapDomains and resolved node endpoint addresses to Endpoints **before** switching nodes or enabling capture. Config compilation cannot prove DNS reachability or exemption freshness. DNS result cache capacity is 1024, query timeout 5 seconds, ordinary answer TTL 300 seconds. Unknown/domain-proxy queries use proxy DNS; direct rules use direct DNS. Native sniff actions use HTTP/TLS/DNS, not legacy inbound sniff keys. No geoip/geosite database, Clash API, TUN/gVisor, or QUIC transport is configured.

`FakeIP` defaults to **false** for initial explicit/transparent testing. Optional native `type: fakeip` uses `198.18.0.0/15`, plus `fc00::/18` in IPv6 follow. A/AAAA answers are fake; other query types retain the same real DNS path. Fake identities are native RAM maps with no exposed strict capacity option in 1.14.2, unlike the bounded result cache. Restart loses mappings. Stale fake client DNS cannot become ordinary direct forwarding merely by withdrawing capture: root must coordinate DNS/cache reset on restart, stop, or direct failure. Do not claim successful fail-open for fake-IP until this is tested. Native metadata unmaps known fake addresses before rules; the planner reserves IPv6 fake range before ordinary ULA bypass.

IPv6 defaults to `direct` until verification. `follow` needs IPv6 scoped capture and a dual-stack TPROXY listener; `block` rejects non-bypassed IPv6 and returns empty AAAA answers. IPv6 direct uses direct AAAA DNS and deliberately avoids IPv6 capture. TLS DNS and outbound bootstrap prefer IPv4 (`ipv4_only` in block mode). `FailureDirect` is supported. `FailureBlockProxy` returns an explicit error because selective blocking after core death requires a surviving DNS/rule classifier; it is not equivalent to dropping all WAN traffic.

## Firewall intent

```go
plan, err := proxy.PlanOwnedRules(proxy.RulesPlanInput{
    ClientIPv4: clientIPv4,
    ClientIPv6: clientIPv6,
    LANInterface: lanInterface,
    Ports: proxy.Ports{Mixed:2080,TProxy:7893,DNS:1053},
    IPv6: proxy.IPv6Direct,
    Failure: proxy.FailureDirect,
    EndpointIPs: resolvedEndpointIPs,
    ManagementIPs: managementIPs,
})
```

Read the types/comments in `firewall.go` for exact ownership and application preconditions. `Apply`, `Cleanup`, `OnFailure` are pure `[][]string`, including executable names; there is no shell. One ClientIPv4 is mandatory. IPv6 follow/block requires one ClientIPv6. No full-LAN or OUTPUT capture is provided. Empty modes default to IPv6 direct/failure direct; all-zero ports use the compiler defaults. `FakeIP` is opt-in and must match the compiler, so ordinary private ULA/benchmark ranges stay direct when it is false.

Root must preflight existing chains, table, priority and mark ownership; serialize the operation; validate modules/argv on RN02; apply routes and prepared owned chains before hooking capture; record every completed contribution and clean only those on partial failure. Cleanup removes exact scoped hooks, owned chains, exact mark/mask policy rules and exact local routes. It does not flush system chains or route tables. Fixed names are single-operation ownership, not an idempotent concurrent manager.

The chosen capture bit is **0x4000**, mask **0x4000**, table/priority **16500**, chains `B6P_*`. Factory QoS mask `0xffff8000`, mwan `0x3f00`, parent `0xf`, and UU `0xf0` occupy every other bit in their union. `0x40000000` overlaps the factory QoS mask and is not safe. Root must still inspect actual target marks/table priorities before activation.

DNS interception covers TCP and UDP port53 through scoped NAT REDIRECT, with mangle DNS return before TPROXY. Router-local/private/management/endpoint destinations are bypassed, keeping factory SSH reachable without globally exempting Internet port22; management-address DNS remains on dnsmasq and must be coordinated there. Application DoH/DoT and QUIC remain ordinary explicitly routed traffic, not a claim that all names pass through port53.

No ECM/PPE/SFE global disable is generated. Actual scoped capture/acceleration behavior and counters are unknown until tested. Direct OnFailure equals Cleanup. Existing NAT DNS conntrack bindings are not removed by rule withdrawal; root must coordinate scoped drain/expiry, never a global conntrack flush. Fake-IP stale-cache considerations still apply.

## Native source and verification

Checked against sing-box v1.14.2 source: `option/dns.go`, `option/rule_action.go`, `option/inbound.go`, `option/simple.go`, `option/vless.go`, `option/tls.go`, `option/route.go`, `option/rule_set.go`, `protocol/vless/outbound.go`, `dns/transport/tls.go`, `dns/transport/fakeip/{fakeip,memory}.go`, and `route/route.go`. The expected minimal build uses `with_utls` plus `badlinkname,tfogo_checklinkname0`/TFO compatibility. It does not need `with_gvisor`, `with_clash_api`, or `with_quic` for VLESS TCP REALITY with XUDP. No downloaded executable is run by this worker.

Synthetic tests cover real parameter retention, 170-node/10-group/1020-rule scale, bounded YAML, unsupported rules, public projection, native DNS/config keys, listener scope, deterministic hashes, pinned SRS verification, IPv6/failure policy and exact owned argv. Root still owns ARM native config check, actual selected-node TCP/UDP, single-client DNS/IPv6 routing, ECM compatibility, process-death recovery, boot and rollback tests.

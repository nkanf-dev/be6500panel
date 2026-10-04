# Fresh capture input source qualification

## Source ownership and current observations

`capture_lan::observe` uses the existing native bounded `Observer`. Up br-lan current addresses/masks supply LAN scope. A /32 address does not invent a subnet from UCI or history. Management includes all actual interface addresses, including foreign/down/TUN addresses needed for collision checks. Gateway scope reads no DHCP/ARP source. Device scope requires current valid lease and ARP sources; conflicted IP/MAC/guest/interface evidence cannot authorize a client. Source rows use compact binary IP/MAC identities and bounded sorting; only final eligible devices own text.

`capture_input::observe_and_build` observes only when capture desired is on. Every build uses actual accepted bytes and a fresh observation, not saved journal input. `build_from_observation` supports isolated fixtures and an internal finite resolver. Saved gateway declarations are retained exactly and must be wholly within observed segments; a narrow /32 canary is not expanded. Device lifecycle capture requires stable selected MACs and a fresh unique eligible address for every selection. This version refuses unresolved selections rather than returning partial activation.

Only a positively observed exact accepted ownedTUN first-host/30 address on its up interface is removed from management collision checks. Mere stored management or foreignTUN address cannot exempt the prefix. Accepted mixed/DNS/TUN listener settings are authoritative; missing, loopback-only captureDNS, duplicate, wrong port/network/IPv6 and direct-policy absence are refused. All literal accepted DNS destinations, not just direct-DNS bypass destinations, are checked against the connectedTUN/30.

## Endpoint DNS

The existing strict4096B question/nonce/compression/CNAME/record-section validator now exposes terminal answer A/AAAA addresses. The old readiness `dns_query`/`validate_dns_response` API remains A-only and its original tests pass. `endpoint_dns` uses actual accepted dns-local UDP literal address and exact port as bootstrap; no system resolver, worker/cache, public hardcodedDNS, or TLS853-to-plaintext53 downgrade. A validated matching UDP truncated question may retry TCP with the same absolute deadline. DNS data is not logged or serialized. No-answer, malformed, collision, cancellation and unavailable sources refuse activation.

## In-process capture seam

`CaptureRuntime::with_observer` supplies the fresh native builder to the existing cleanup/readiness/restore hooks. Construction performs no observer/DNS/network command. Off capture restoration is zero actions and does not parse accepted bytes. The current diagnostic/rule-draft main still has no Manager or production listener attachment; this source seam is not a deployed owner.

## Caps and resource boundary

Accepted legacy reads4MiB; new config512KiB unchanged. Interfaces/actual addresses use existing native adapter limits; management128,LANscope8,selected/eligibledevices64,routerDNS16,endpoints256. Device source files1MiB each and8192 rows each, dropped serially. DNS responses4096B,addresses128 perhost. Existing hook deadlines and independent failed-apply cleanup remain authoritative. No new thread, generic scheduler, RPC or daemon. Sorting/allocation estimates are source facts, not measured ARM memory gains.

## Evidence

Host fixture tests and strict native gates are saved under `live-inspection/embedded-performance-2026-10-04/rust-fresh-capture-input-*`. Synthetic loopback UDP/TCP DNS and fake core/commands only; no real core or household network operation. ARMv7 crossbuild is separate from true Linux lifecycle/kernel execution. Full production binding, UCI/FRPC/telemetry parity, admittedartifact migration and exclusive qualified handover still remain before Go source removal.

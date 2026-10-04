# Routed TUN Dataplane Repair Implementation Plan

> **For agentic workers:** Use subagent-driven development with isolated worktrees and root-owned integration. Never operate the router from a worker.

**Goal:** Turn the proven IPv4 original-packet TCP/UDP path into an explicit managed backend without losing client identity or changing existing accepted configuration by default.

**Architecture:** Compile an optional system-TUN inbound directly into the main sing-box core, not a SOCKS sidecar. Exact-client mark hooks route original packets to that owned interface. Dedicated forward/return/private-stack chains and accepted-config-derived readiness keep the current factory ACL, management and core lifecycle boundaries intact. Default TPROXY compilation and old journals remain unchanged.

**Tech Stack:** Go native project tests, sing-box1.14.2 system stack, original-IP Linux policy routing and iptables. Serialized Go -p1/race/vet/ARMv7 checks.

## Evidence and limits

- Actual `synthetic-1791080981`: TLS-verified Google204486ms; UDP1.1.1.1:53 nonce reply100ms; TCP forward14/return13 and UDP forward1/return1; post cleanup no test processes/interfaces/rules/routes/owner, mainPID9516/gen8 unchanged.
- Diagnostic SOCKS outbound hides client identity at main. The product uses a TUN inbound in the main core so route metadata still contains the original client source.
- Originaltuple control supports failed TPROXY socket handoff. The exact live clearing function is not directly measured. The earlier fake-dst-noRST/TUN exclusion was retracted after bridge callback evidence.
- IPv6Direct is the only qualified initial routed-TUN family policy. Reject routed-TUN Follow/Block rather than silently claim IPv6 proxy support. Keep other protocols and private/encryptedDNS/VPN semantics explicit. IPv6/QUIC/VPN/fullLAN acceptance follows, not inferred from this test.
- Cooperative withdraw-before-stop is deployed. Independent production core/parent crash guardian and node/DNS health are still required before household activation. No automatic capture or backend migration occurs from source deployment or GET.

## Shared contract

`proxy.DatapathMode`: `DatapathTPROXY = "tproxy"`, `DatapathRoutedTUN = "routed-tun"`; empty means TPROXY. `proxy.RoutedTUNConfig{InterfaceName string, Address string}`. Compiler receives `CompileInput.Datapath` and `CompileInput.RoutedTUN`. Rules input/ownership receive `Datapath`, `TUNInterface`, `TUNAddress`. Routed TUN is explicit opt-in; absent fields leave prior compiler/plan/journal bytes and cleanup semantics unchanged.

Initial qualified managed TUN defaults: `b6p-tun`, `172.31.255.253/30`, MTU1500, system stack, DNS disabled, no auto-route/auto-redirect, UDP mapping4 and bounded UDP timeout. No gVisor requirement, no SOCKS outbound, no independent proxy core. Validate one host /30 with usable next peer inside prefix and no private-prefix/local-management collision. Reject private/local/wildcard interface misuse and conflicting Ports despite no TPROXY listener in that backend.

## Task 1: Native compiler and explicit configuration selector

Files: internal/proxy/types.go (root contract), native.go and new native_routed_tun.go/tests (worker); internal/httpapi/proxy_runtime.go and focused API tests (root).

- Test empty/default compiler still emits byte-identical TPROXY intent.
- Add routed-TUN inbound replacing only tproxy-in, preserve original Node/outbounds, DNS/local authority, policy order and sniff[http,tls,dns,quic]. Include tun-in in client sniff/DNS policy.
- Test no auto/default/global output marking or SOCKS sidecar; native source original client metadata preserved. Required feature system-tun replaces tproxy_tcp_udp only when selected.
- Reject unknown datapath, missing/bad TUN config, collision address/IPv6Follow/Block for this first qualified phase.
- Root API selector is optional; missing selector preserves actual accepted backend on node changes, not an automatic fallback. Explicit TUN change uses existing controlled Configure generation/capture-off guards. No live accepted write until qualified offline integration and root safe deployment.

## Task 2: Pure owned routed-TUN planner

Files: internal/proxy/firewall.go and new firewall_routed_tun.go/tests (worker).

- Dispatch after canonical validations. Legacy/default TPROXY planner byte shape and Cleanup/Ownership unchanged.
- Dedicated B6P_V4_TUN_MARK, B6P_V4_TUN_FORWARD, B6P_V4_TUN_RETURN, B6P_V4_TUN_INPUT and B6P_V4_TUN_OUTPUT; no system chain flush, no interface create/delete, no kernel address/sysctl/global accelerator commands.
- SourceIP+MAC+iif hooks; management/LOCAL/safety/private endpoint exemptions retain existing policy, ordinary DNS semantics consistent with33149bf. MARK masks only4000. Own ordinary defaultdevTUN table16500 and exact source+iif+mark rules. Prepare chains/routes before last hooks.
- Return only declared installed clients, private INPUT next-peer to own local address and own TUN interface; dynamic TCP listener may be pinned later by readiness, do not permit otherinterfaces. Preserve factory WAN/guest/IoT rules and no allLAN scope expansion.
- Remove MARK hooks first, then all exact hooks/owned chains/rules/routes. Never delete core-created TUN interface; lifecycle owns it.
- Test malicious field/injection/sourceMAC/IP reuse, multipleclients, defaultoldjournal and inverse apply/cleanup shape. Use fake runner only.

## Task 3: Accepted-native extraction, preflight and observation

Files: internal/capture/native.go, desired.go, validation.go, preflight.go, diagnostics.go and new focused tests (worker).

- Extract backend from accepted inbounds, not journal/selection override; validate exact systemTUN fields noauto-route and known family. Clients/management/DNS/exceptions remain sourced from live router observation.
- Admit old journals via exact old Ownership/Cleanup; add TUN fields only for new explicit backend. Never replay stored Apply.
- Readiness/preflight verifies occupied interface belongs to actual main core (application readiness owner proof), valid owned prefix/address, tablealias references/markbit collisions, TUNprefix overlap with routerLAN/management/routes and installedscope. Observation checks ordinary devTUN route, exact owned chains/hooks, not localdevlo.
- GET/Diagnostics read-only; missing data null/unavailable and never apply/repair/withdraw.
- Test legacyjournal cleanup, TUNordinaryroute drift, foreign/namedtable/CONNMARK collisions, wrongacceptedbackend/interface/address, DNSfamily explicit.

## Task 4: Runtime interface readiness and managed fail-open

Files: cmd/be6500panel/readiness.go and new readiness_tun.go/tests (root), internal/runtime lifecycle boundary (subsequent isolated task).

- Require actual starting/running core PID/exe plus TUN fd ownership (fdinfo iff where supported); address/UP/rp_filter2 and unique nonDNS private TCP listener inode->corefd. No repeated fullbinary hash readiness loop.
- Refuse unsupported ownership observation rather than infer ready from interface existence. Mixed/DNS local smoke checks remain separate from Internet/dataplane success.
- Withdraw owned rules before stopping core. Parent/core independent production guardian uses fixed exact ownership and retains recovery journal; failure refuses new control writes but preserves or restores factory direct path.
- Fault tests require capture withdrawal on parent/core exit, oldconntrack/DNS expiry and proxy-node/DNS loss; no full fail-open claim before actual gates. Offline control refusal does not disable original forwarding.

## Task 5: Root integration and acceptance

- Cherry-pick exact worker commits, resolve no overlapping ownership blindly. Full native `go test -p1 ./...`, vet/race and ARMv7; reuse frontend node_modules and no duplicate browsers.
- Preserve deployed accepted node/core config until a separate controlled application of explicit backend, captureoff before/after. No source push or private helper/artifact staging into product.
- First integrated live acceptance remains fixed synthetic scope with independent guard, not realdevice/fullLAN. Require actual TLS204 + UDP originalsource/nonce AND both owned forward/return counters. Then QUIC/multidest/MTU and rollback/fault gates.
- Add declared LAN-prefix wholehome management after backend+availability qualification; device policies are exceptions, never normal per-device onboarding. Household activation is not performed by this plan.

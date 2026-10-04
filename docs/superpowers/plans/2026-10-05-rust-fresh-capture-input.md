# Fresh Capture Input Implementation Plan

> **For agentic workers:** Use isolated source lanes or inline execution task-by-task. Root alone runs serial native and ARM gates in the shared target tree.

**Goal:** Build routed-TUN capture input only from current accepted bytes, current br-lan/device identity and bounded actual endpoint DNS answers.

**Architecture:** Reuse `NativeObserver` interface and no-follow file reads, `native_target` and strict DNS packet parsing. A small source-bound builder joins fresh observations with accepted config; no stored journal input, generic RPC, global owner, background DNS thread or unbounded system resolver. Native caller invokes the builder under the existing hook deadline. This is not production activation.

**Tech Stack:** Rust1.93, std/libc, existing serde/serde_json, existing native validators.

---

## Task1: Current LAN/device observation

Files: create `rust/panel/src/capture_lan.rs`, tests `rust/panel/tests/capture_lan.rs`; root adds module export.

- [x] Implement a private-safe current snapshot from `readiness_tun::Observer::interfaces` and bounded native files. Up `br-lan` address/mask supplies LAN scope; management includes all actual interface addresses. No UCI/config inventory cache authorizes live capture.
- [x] Current DHCP leases + `/proc/net/arp` are required for device identity. Reject bad rows, expired leases, duplicate/conflicting IP/MAC identity and foreign/guest interface. Current lease or complete br-lan ARP may authorize exactly one LAN host; never use old address when MAC unresolved. Strict all-source failure refuses device scope; gateway need not read lease/ARP sources.
- [x] Bounded limits: max128 management IPs,8 LAN prefixes,16 routerDNS addresses,64 selected devices,1MiB each source,8192 source rows. Store fixed errors, never private lines or argv.
- [x] Pure fake `Observer` tests cover masks, `/32` unavailable scope, guest isolation, LAN host/network/broadcast, lease expiry, multiple MAC/IP/interface ambiguity, source failures and deadline.

## Task2: Direct bounded endpoint DNS

Files: modify `rust/panel/src/readiness_dns.rs`, create `rust/panel/src/endpoint_dns.rs`, test `rust/panel/tests/endpoint_dns.rs`; root owns these files.

- [x] Refactor strict response validator to expose terminal answer addresses and support A/AAAA query types while preserving current readiness A-answer contract and all existing malformed-message tests. Keep4096B packet cap/validated question/CNAME graph/sections/exact nonce; no custom crypto.
- [x] Native UDP then TCP-on-truncation uses only a supplied accepted literal bootstrap SocketAddr, one shared finite deadline/cancel, bounded addresses and IO slices; no libc getaddrinfo/worker thread or trusted port-open health. No public host or payload diagnostics.
- [x] Synthetic loopback DNS tests: actual A/AAAA, CNAME, unrelated glue rejected, ID/question/type mismatch, truncation TCP fallback, deadline/cancel. Existing readiness tests must remain exact.

## Task3: Source-bound accepted-input join

Files: create `rust/panel/src/capture_input.rs`, test `rust/panel/tests/capture_input.rs`; root owns these files.

- [x] Parse only small accepted fields with bounded arrays/strings; keep route/DNS rule outer limits separate from inner256. Reuse `native_target` and readiness selectors for exactTUN/IPv6Direct/DNS hijack proof.
- [x] Preserve saved declared gateway scope; each declaration must be within observed br-lan segment. Device scope uses current unique MAC resolution and no literal-only lifecycle authorization. Accepted mixed/DNS/TUN ports/binds authoritative; remove only exact positively observed acceptedTUN local/30 from management.
- [x] Extract outbound endpoint and direct DNS literal/hosts, bounded256; resolve hostnames through accepted dns-local UDP literal server+actual port only (no TLS853-to-plaintext53 downgrade or hardcoded DNS); deduplicate answers, reject collision withTUN private/30, invoke pureplan compiler before admission. Off flag returns refusal without observation/DNS/commands.
- [x] Frozen native/capture fixtures + synthetic snapshot/resolver tests cover settings/bind scope/collisions/narrowcanary/unresolvedidentity/duplicate/bounds/cancel. No Go generation/build or router write.

## Task4: Serial integration

- [x] Collect source commits, add only exports/adapters needed for ordinary in-process builder. Validate no journal replay, extra threads/daemon/default activation.
- [x] Root full Rust test/fmt/clippy/diff plus ARMv7 build in `.build/rust`,jobs1,incrementalfalse. Report source qualification separately from production/nativeARM lifecycle.

## Result

295 full host tests, fmt, strict all-target Clippy, diff check and current-source ARMv7 crossbuild passed. Added non-direct DNS literalTUNcollision and native observer seam zero-off-action regressions. Input join refuses incomplete selected device identity. Default main remains diagnostic/rule-draft only; no live device or production management action.

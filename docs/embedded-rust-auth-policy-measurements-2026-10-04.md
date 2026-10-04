# Rust session and pure-policy migration: measured slice

Source integration: `82a092e` (session/auth), `cf5a8b9` (pure policy), `4611342` (Go reference fixtures), `862c0e5` (permanent differential Rust test).

## Implemented and verified

- 52 native tests passed, including the original foundation tests, 13 policy tests and the permanent 23-case exact Go differential fixture check.
- Locked Cargo tests, fmt check, clippy all-targets with warnings denied, native release and ARMv7 static-musl release builds passed.
- Session security uses OS 32-byte entropy, hex64 response cookies, only SHA256 token/password hashes in memory, vetted constant-time comparison, bounded session/attempt counts and monotonic TTL. Same-origin, JSON/body bounds and one absolute read deadline cover login/logout. No async runtime or web framework.
- Pure local-policy validation/revision/fingerprints/ordered merge preserves Go semantics, including invalid diagnostics, exact occurrence references, disabled and orphan rules, terminal MATCH, Unicode/HTML serialization and source identity. It does not yet expose writable rules HTTP routes, persist a draft or compile native configuration.

Focused dependencies are getrandom, serde, serde_json, sha2 and subtle. The earlier zero-dependency foundation claim does not describe this expanded source slice. Dependencies are justified by parsing and security rather than implementing custom cryptography.

## Artifacts

Native macOS ARM64 release: 402896 bytes. Static ARMv7 release: 499056 bytes (about 487 KiB), SHA256 `969a3ea882e2a07e27848641094e35109c13d18fe348b8227710ba40f1217447`.

## Authentication workload on actual BE6500

A temporary service listened only on 127.0.0.1:8790. A synthetic private password was provided through environment/file, never the production password or argv. Two finite runs each verified initial 401, login, 600 protected health/memory reads, public streaming of the existing 609282-byte asset, logout and post-logout 401.

| Round | Protected requests | Warm RSS KiB | Peak RSS KiB | P50 ms | P95 ms | P99 ms | CPU ticks |
|---|---:|---:|---:|---:|---:|---:|---:|
| 1 | 600 | 412 | 412 | 1.571250 | 2.073458 | 5.120291 | 30 |
| 2 | 600 | 416 | 416 | 1.495125 | 1.768542 | 1.937875 | 29 |

All 1200 protected requests passed. One service thread was observed and OOM remained zero. Cold first-touch RSS was not used as the stable-memory claim. CPU is recorded in actual ticks; CLK_TCK was not established, so no target CPU percentage is inferred. Latency includes the finite benchmark client and TCP loopback.

Native host measurement: 2000 protected requests, login/logout and 401 checks passed. Warm RSS was 1632 KiB; native time reported maximum RSS 1671168 bytes. Host P95 was 0.232250 ms, not a router claim.

The pure policy library is linked but policy documents are not exercised by this HTTP benchmark. There is no claim about maximum-document policy memory or full-backend operation peaks. Those need separate benchmarks as storage/compiler and rules HTTP are added.

## Production boundary

Both temporary service and isolated target RAM files were removed after exact PID/executable checks. The old owner remained PID 6642 and sing-box remained PID 7858/generation9/restarts0. Current capture remained desired=false/active=false; no gateway enable/withdrawal occurred. The Go :8788 entry remains memory-paused. No production owner cutover, authenticated Rust LAN service, router reboot or live Apply was performed.

Evidence: `/Users/nkanf/docs/miwifibe6500/live-inspection/embedded-performance-2026-10-04/auth-policy-benchmark`. Next slice: private draft persistence and native compiler parity on copied/public synthetic fixtures, then a real rules HTTP vertical slice. Keep one final Rust owner and one static tree.

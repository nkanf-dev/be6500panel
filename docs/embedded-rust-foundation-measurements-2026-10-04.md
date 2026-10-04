# Embedded Rust foundation: measured first slice

Source: `990a23a575f6d84588a263c45801a45682a15608` on `rewrite/rust-embedded`.

This is an engineering diagnostic slice, not the complete panel backend. It has no authentication, rule compiler, runtime owner, UCI or FRPC management implementation yet. No production owner handover occurred. The paused Go :8788 entry was not restored.

## Native validation

Rust 1.93.0 native `cargo test` passed 18 tests. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and release builds passed. There are zero third-party dependencies in this foundation crate. The service has one thread, no async runtime, no sampler and no per-request subprocess.

Requests have a 16 KiB/64-header bound, 2048-byte target bound and absolute 5-second read/write deadlines. Procfs reads are bounded at 64 KiB+1. Static bodies use an 8 KiB streaming buffer and one trusted static root. Memory units and missing MemAvailable semantics are tested. Explicit contracts: GET/HEAD `/api/health` and `/api/system/memory`; unimplemented APIs return 404.

## Release artifacts

| Optimization | Native macOS ARM64 bytes | Static ARMv7 bytes | ARM SHA256 |
|---|---:|---:|---|
| `3` | 336400 | 429184 | `c9812eddd025ef2e8ca47f7d67d0bc6a4c47c371f39f7cbda9eecb32370203b1` |
| `s` | 319888 | 411952 | `8b60cc043c4b5f6bef7c4847c2a23b5fe92af25e2d26ea71078c5c70141f4cde` |

ARM target: `armv7-unknown-linux-musleabihf`, Rust bundled `rust-lld`, self-contained musl/static CRT. Both are ELF32 ARM (machine 40). No application C code was introduced.

## Development-host measurement

Actual macOS ARM64 process idle RSS was 1376 KiB for both profiles. Native `/usr/bin/time -l` recorded maximum RSS 1409024 bytes for both; each used approximately 0.01 s user and 0.05 s system CPU over a ~95 s lifetime containing 2000 sequential requests. Both processes were intentionally stopped afterward. Host bursts ran in parallel on separate listeners and share host load; they do not rank optimization levels.

Host P95: `3` 0.980625 ms; `s` 0.993083 ms. These are not router latency or memory numbers.

## Actual BE6500 ARMv7 measurement

Each profile ran alone as a short-lived loopback diagnostic service. A finite Rust-only client alternated health and real `/proc/meminfo` queries, then HEAD/GET of the existing 609282-byte ECharts asset. Assets were streamed from the existing tree, not copied into another RAM extraction. Existing production panel/core and capture configuration were not changed.

| Profile/round | Requests | Warm/after RSS KiB | HWM KiB | P50 ms | P95 ms | P99 ms | Service CPU ticks |
|---|---:|---:|---:|---:|---:|---:|---:|
| `3`/1 | 600 | 344 | 344 | 1.050917 | 1.391292 | 1.695458 | 19 |
| `3`/2 | 600 | 344 | 344 | 1.094083 | 1.395166 | 2.300334 | 24 |
| `s`/1 | 600 | 324 | 324 | 1.076583 | 1.259416 | 1.441208 | 23 |
| `s`/2 | 600 | 328 | 328 | 1.072167 | 1.218500 | 1.330292 | 25 |

All 2400 requests passed. One service thread was observed. OOM counter remained zero. Initial 4 KiB cold RSS is a pre-touch demand-paging sample, not a stable idle-memory claim. CPU values are actual user+system ticks; kernel CLK_TCK was not verified, so no target CPU percentage is inferred. Latency includes the benchmark client and TCP loopback overhead. Ambient household load and two rounds are insufficient to claim globally optimal tuning.

Keep performance-oriented `opt-level=3` provisionally. `s` saves 17232 ARM binary bytes and 16–20 KiB RSS, but did not demonstrate lower service CPU ticks. Do not choose from binary size alone. Revisit the profile as real authenticated management workloads are added.

All diagnostic processes were stopped and their isolated RAM staging was removed after exact PID/executable checks. The production owner stayed PID 6642; sing-box stayed PID 7858, generation 9, restarts 0. Capture was observed desired=false/active=false before and after; it was not enabled or withdrawn by these tests. Persistent rules, history, SSH, FRPC and backups remain unchanged.

## Acceptance and next slice

The proposed first-slice caps (idle <=3 MiB, burst <=6 MiB, stripped ARM <=2 MiB) are met by the observed diagnostic measurements. They are **not** a complete-backend acceptance result. Next implement authentication/same-origin handling and the independent local policy data layer with differential fixture tests. Keep the final design one Rust management owner, one asset tree and explicit bounded resource budgets.

Evidence directory: `/Users/nkanf/docs/miwifibe6500/live-inspection/embedded-performance-2026-10-04/foundation-benchmark` (`release-builds.json`, host time/client reports, native target rounds, `target-measurement-summary.json`, stop/cleanup reports). Router reboot, fault injection and production takeover were not performed.

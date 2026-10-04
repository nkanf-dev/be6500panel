# Embedded Rust management backend

## User decision

Performance comes first on this embedded BE6500 device. Management is single-user and low-concurrency. Necessary rewriting should happen early, in Rust, rather than accumulating Go legacy. C is not an equal default. Reuse a C system ABI only where Rust needs it; do not implement new application components in C.

## Current live boundary

The extra Go rules entry :8788 is paused after a memory-pressure incident. Its persistent rule drafts, native exception, package and backups remain. Original panel PID 6642 and sing-box PID 7858/configuration generation 9 were retained. Capture was subsequently observed desired=false/active=false; do not re-enable it as a rewrite side effect. Preserve independent SSH, factory ACLs, guest isolation and FRPC support. No production owner replacement or reboot is part of the initial Rust slice.

## Rewrite boundary

Replace the management backend: HTTP/auth, real system/device observation, persistent bounded history, independently editable rules, native configuration compiler and controlled runtime/configuration operations. Keep the established browser contract and migrate durable data explicitly. The final product has one Rust management process. It is not a second permanent reverse-proxy daemon, generic IPC layer or new guardian framework.

Retain the proven sing-box forwarding engine as an independent native component. Rust supervises it only after an ownership handover is qualified. Do not reimplement VLESS, TLS, QUIC, DNS transports or the kernel dataplane merely to change languages. Retain FRPC as its existing independent component and preserve remote authenticated management; single-user does not mean LAN-only.

## Resource design

Use Rust std and small focused parsing/serialization crates. No Tokio, large web framework, general scheduler, ORM or plugin framework. Start with sequential bounded HTTP requests for the first isolated slice. Later use only the small, fixed number of connections needed for one browser plus its event stream. Do not allow one persistent stream to block management actions. Read/write/header/body limits, deadlines, bounded logs, collector cardinality and durable-file budgets are explicit.

Static assets have one on-device copy. Stream files without loading them into a full response buffer. Do not duplicate all browser assets in tmpfs or run an extra runtime just to add a route. No measurements or historical records are fabricated. Missing MemAvailable is an error, not MemFree or zero presented as equivalent.

## First runnable slice

Create a standalone Rust crate under rust/panel. Bind only loopback by default, expose /api/health and a new explicit /api/system/memory diagnostic contract, and serve optional static files from one configured root with a fixed streaming buffer. No authentication or controlled writes are claimed in this slice. It is an isolated engineering executable, not a replacement production panel yet.

Validate HTTP framing, limits and deadlines with malformed/truncated requests; reject request transfer encodings and ambiguous content lengths. Health and memory diagnostics do not read private configurations, create persistent files, spawn child processes or mutate the router. Tests use temporary fixture procfs data and host loopback only. ARMv7 release build is required before a device-side benchmark is considered.

## Acceptance budgets

Initial proposal, not achieved results: first slice release RSS <=3 MiB idle and <=6 MiB under a bounded sequential diagnostic burst; stripped ARMv7 binary <=2 MiB; no duplicate static-asset RAM cost; idle CPU near zero with no periodic collector. Record observed RSS/PSS/CPU, latency distribution, binary bytes and open descriptors. Baseline against the paused Go front and original owner, but do not attribute kernel slab or sing-box traffic growth to the language runtime without evidence.

The real full-feature budget will be allocated from measured headroom and the first benchmark. A language change is not evidence of optimal performance. Safety/error semantics must remain correct; faster wrong routing is not acceptable.

## Subsequent vertical slices

1. Authentication and same-origin HTTP contracts, with one browser session and no remote regressions.
2. Actual observers and tiered read-only history, reusing bounded durable formats or explicit import tools. Bound collection rates and memory by source cardinality.
3. Local policy CRUD/save/preview and compiler parity from existing golden fixtures. Preserve enabled/order, exact subscription fingerprints, orphans and source provenance.
4. Controlled native configuration/runtime operations and FRPC. Native check, generation/readback, rollback and withdraw-before-stop are required before ownership migration.
5. Qualify one-process replacement using host fake processes and isolated router diagnostics, then perform only an authorized production handover with adequate memory/storage headroom. Never restore the paused Go front to hide an incomplete Rust migration.

## Toolchain evidence

The development host has Rust 1.93.0. `armv7-unknown-linux-musleabihf` standard libraries are installed. A local Rust-only smoke program linked with the bundled `rust-lld`, self-contained musl and static CRT produced a 351260-byte ELF32 ARM executable (machine=40). It has not been run on the router. This verifies a native cross-link path, not the service's memory, latency or production readiness.

Exact cross-link flags: `-C linker=/Users/nkanf/.rustup/toolchains/1.93.0-aarch64-apple-darwin/lib/rustlib/aarch64-apple-darwin/bin/rust-lld -C linker-flavor=ld.lld -C target-feature=+crt-static -C link-self-contained=yes`.

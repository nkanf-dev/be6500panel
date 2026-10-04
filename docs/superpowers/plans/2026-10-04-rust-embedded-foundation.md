# Rust Embedded Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce an isolated, bounded Rust diagnostic HTTP service and measure its native resource cost before migrating the management backend.

**Architecture:** One synchronous Rust process with no periodic sampler and no async runtime. Serve bounded diagnostic GET/HEAD requests and stream static files from one configured root. Default and accepted listen addresses are loopback-only until authentication is implemented; do not start it on the router in this implementation lane.

**Tech Stack:** Rust 1.93, std networking/filesystem, serde/serde_json only if serialization needs them. Cargo native tests/clippy/release; Rust bundled rust-lld and armv7-unknown-linux-musleabihf std for the ARM release.

---

## File structure

- Create `rust/panel/Cargo.toml`, `Cargo.lock`, `.cargo/config.toml` only if needed for cross-linking, `.gitignore` for target.
- Create `rust/panel/src/lib.rs`: module exports and constants.
- Create `rust/panel/src/main.rs`: small flag parser and loopback-only listener, no threads or processes.
- Create `rust/panel/src/http.rs`: bounded request parser, deadlines and exact response framing.
- Create `rust/panel/src/memory.rs`: real bounded procfs diagnostics, no fake fallback.
- Create `rust/panel/src/static_files.rs`: one asset root, traversal rejection and fixed-buffer streaming.
- Create `rust/panel/tests/http_integration.rs`: native host loopback endpoint and malformed-input checks; tests own finite servers that terminate.
- Create `rust/panel/README.md`: exact native/cross build and diagnostic-slice limits, no production readiness claim.

## Task 1: memory diagnostics with actual units and strict bounds

- [ ] Add tests before implementation:

```rust
#[test]
fn memory_requires_available_and_preserves_kib_units() {
    let m = parse_meminfo("MemTotal: 100 kB\nMemAvailable: 25 kB\n").unwrap();
    assert_eq!(m.total_bytes, 102400);
    assert_eq!(m.available_bytes, 25600);
    assert!(parse_meminfo("MemTotal: 100 kB\nMemFree: 25 kB\n").is_err());
    assert!(parse_meminfo("MemTotal: 1 kB\nMemAvailable: 2 kB\n").is_err());
    assert!(parse_meminfo("MemTotal: 1 kB\nMemTotal: 1 kB\nMemAvailable: 0 kB\n").is_err());
}
```

- [ ] Run `cargo test --manifest-path rust/panel/Cargo.toml memory` and confirm missing API failure.
- [ ] Implement `MemorySnapshot { total_bytes: u64, available_bytes: u64 }`, `parse_meminfo(&str) -> Result<MemorySnapshot, MemoryError>` and `read_memory(&Path)`. Read at most 64 KiB+1, reject missing/duplicate required fields, unsupported units, overflow and available>total. Other valid procfs fields may be ignored. Use `checked_mul(1024)`; do not substitute MemFree for MemAvailable.
- [ ] Run the same targeted tests and full `cargo test`.

## Task 2: bounded synchronous HTTP

- [ ] Add parser tests for GET and HEAD health, missing/duplicate Host in HTTP/1.1, duplicate Content-Length, any Transfer-Encoding, nonzero GET body, oversized headers, truncated headers, malformed paths, unsupported method and slow/truncated reads.

```rust
#[test]
fn ambiguous_framing_is_rejected() {
    assert!(parse_request(b"GET /api/health HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\nContent-Length: 0\r\n\r\n").is_err());
    assert!(parse_request(b"GET /api/health HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n").is_err());
}
```

- [ ] Run `cargo test --manifest-path rust/panel/Cargo.toml http` and confirm failures before implementation.
- [ ] Implement a 16 KiB total header cap, at most 64 headers, 5-second absolute request/header and write deadline, close every connection, GET/HEAD only, HTTP/1.0 or 1.1 framing. Return fixed public errors with exact Content-Length, Connection: close, Content-Type, X-Content-Type-Options and Cache-Control: no-store for diagnostics. Never echo untrusted input.
- [ ] `GET /api/health` returns `{"status":"ok","mode":"host","readOnly":true}`. `GET /api/system/memory` returns explicit JSON `{"source":"procfs","totalBytes":...,"availableBytes":...}` from the selected proc root. Missing proc source returns 503, not zero.
- [ ] CLI: `--listen 127.0.0.1:8790`, `--proc-root /proc`, optional `--web-dir <path>`. Reject a non-loopback IP. Read one accepted connection at a time, no detached worker, no telemetry loop, no subprocess, no persistent settings mutation. Fail duplicate or unknown arguments.
- [ ] Run finite loopback integration tests for exact response bodies, no extra bytes and HEAD empty body.

## Task 3: static assets without duplicate RAM

- [ ] Add traversal, percent-encoded traversal, symlink escape, unknown file, directory and HEAD tests before implementation. Include a file larger than the transfer buffer.
- [ ] Implement canonical trusted asset root, bounded path length <=2048 bytes, relative normalized path components only, percent decoding with UTF-8/control validation, reject symlink components. Map `/` to `index.html`, reject traversal and unknown assets; no whole-file response allocation or copies of the asset tree.
- [ ] Stream using `std::io::copy` with explicit reusable 8 KiB buffer or an equivalent exact bounded loop. Static file metadata determines Content-Length; HEAD does not read content. Use fixed MIME types for html/js/css/json/svg/png/ico/woff2, fallback application/octet-stream. Do not claim an authenticated production UI.
- [ ] Run all native tests; `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, native release build. Release profile uses LTO, one codegen unit, panic=abort and stripping; compare opt-level `s` versus `3` by measurements, not assumption.
- [ ] Build ARMv7 with native Rust std/musl and bundled rust-lld. Record the exact command/toolchain, stripped bytes, SHA256 and build result. Build no application C source and install no router dependencies.
- [ ] Host benchmark uses a release process on loopback and actual proc fixture, sequential requests. Record baseline/peak RSS, CPU and latency, then stop the process. Proposed caps: <=3 MiB idle RSS, <=6 MiB under a sequential burst, <=2 MiB stripped ARM binary. These are acceptance targets until measured.

## Commit and handoff

- [ ] Commit only `rust/panel` and the two approved Rust design/plan documents, or report source implementation SHA separately if root owns the documents.
- [ ] Explicitly report native checks, cross build, observed resource measurements and missing functionality. Do not install on the router, claim management parity, replace current core ownership or re-enable capture.

## Next stage boundary

After foundation measurements, add authentication/real existing contracts and migrate local rules using golden fixtures. SSE cannot block management writes when introduced. Keep final architecture one Rust manager and one copy of static assets; do not revive a second heavy Go front to compensate for incomplete Rust implementation.

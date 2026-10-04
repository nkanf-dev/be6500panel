# Rust panel diagnostic foundation

This crate is the first runnable slice of the embedded Rust backend. It is an
isolated engineering service, **not** a production panel replacement.

## Native build and test

Rust 1.93 or later is required. There are no third-party dependencies.

```sh
cd rust/panel
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo build --release
```

The release profile uses `opt-level = 3`, LTO, one codegen unit, `panic = "abort"`,
and stripping. The choice of optimization level is provisional. Compare actual
performance and binary size against `s`; do not infer resource cost from Rust alone.

## Host-only use

```sh
./target/release/be6500-panel --listen 127.0.0.1:8790 --proc-root /proc
# Optional: serve assets directly from one trusted directory.
./target/release/be6500-panel --listen 127.0.0.1:8790 --proc-root /proc --web-dir /path/to/assets
```

Only numeric loopback addresses are accepted, including IPv6 `::1`. The default
is `127.0.0.1:8790`. Unknown, missing, and duplicate flags fail before listening.
No router deployment, service installation, existing panel/core/config change,
capture change, or SSH operation is included. Stop the host process after tests
or measurements. On hosts without `/proc/meminfo`, supply a fixture proc root;
the memory route otherwise returns 503.

## Exact public contracts

GET and HEAD accept HTTP/1.0 or HTTP/1.1. HEAD returns the same status and
Content-Length as GET, but no response body, including recognized malformed HEAD
requests. All connections close after one request. No keep-alive or pipelining
is supported.

- `GET /api/health`: `200`, exactly
  `{"status":"ok","mode":"host","readOnly":true}`.
- `GET /api/system/memory`: `200`, exactly
  `{"source":"procfs","totalBytes":N,"availableBytes":N}` using real Linux
  `MemTotal` and `MemAvailable` from the selected proc root. `kB` means 1024 bytes.
- A missing, invalid, duplicate, overflowing, or oversized memory source returns
  `503` with `{"error":"memory unavailable"}`. `MemFree` is never substituted.
- Other API routes, including `/api/system`, return `404` with
  `{"error":"not found"}`. This is **not** the legacy complete system schema.
- If `--web-dir` is set, `/` maps to `index.html` and other non-API paths serve
  regular files from that one directory. Unknown files and directories return
  404. API paths do not fall through to assets.

JSON errors are fixed public strings. No request input, private file paths, or
OS errors are echoed. Responses set exact Content-Length, Content-Type,
Connection: close, X-Content-Type-Options: nosniff, and Cache-Control: no-store.
405 also sets `Allow: GET, HEAD`.

## Explicit bounds

- One synchronous process; one accepted connection at a time. No workers,
  subprocesses, framework, async runtime, idle sampler, or per-request logs.
- Request headers: at most 16 KiB and 64 fields. Request target: at most 2048 bytes.
- Absolute request/header deadline: 5 seconds, not renewed by a slow trickle.
- Absolute response-write deadline: 5 seconds, including streamed body writes.
- GET/HEAD only. HTTP/1.1 requires exactly one nonempty Host field. Duplicate
  Content-Length, any Transfer-Encoding, and nonzero body lengths are rejected.
  Unsupported methods return 405. Malformed framing returns 400; request timeout
  408; target overflow 414; header size/count overflow 431.
- Proc source read: at most 64 KiB plus one sentinel byte to detect overflow.
- Static body transfer: fixed 8 KiB stack buffer. Metadata sets Content-Length;
  HEAD does not read file content. Growth cannot exceed that length; shrinkage
  terminates the connection. No whole-file body allocation or asset-tree copy.
- Percent decoding validates UTF-8 and rejects controls/backslashes. Traversal,
  empty/dot components, and all symlink components are rejected.

The optional asset directory is a **trusted, stable tree**. Do not allow another
process or user to mutate its paths during serving. The std-only component checks
are not an atomic filesystem sandbox against concurrent tree replacement.
Static-file and procfs reads use the local filesystem; HTTP socket deadlines do
not provide a timeout for a stalled local filesystem operation.

## Validation and remaining work

Native tests use temporary fixture files and finite host loopback servers. They
cover memory units, missing/duplicate/overflow fields, bounded source reads,
HTTP ambiguity/limits/truncation, GET/HEAD framing, absolute slow-read/write
deadlines, traversal and symlink rejection, and files larger than the stream
buffer. Test-only threads and child CLI processes do not exist in the service.

ARMv7 qualification and host/resource benchmark records belong to the root
implementation lane. No observed RSS, PSS, CPU, latency, or achieved ARM binary
size is claimed in this crate. The initial design caps are proposals until
measured. This crate does not implement authentication, full management routes,
durable history, rules, compilation, config writes, supervision, ownership
handover, or remote management. Do not expose it beyond loopback.

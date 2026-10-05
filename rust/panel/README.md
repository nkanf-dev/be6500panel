# Rust native panel

This crate is the production embedded control plane. Native mode serves the full
product through the existing authenticated API and static browser tree. Diagnostic
mode remains a loopback-only development entry without a runtime owner.

## Build and test

Rust 1.93 or later is required. Project builds and tests use Rust and Bun, not Go.
The crate uses the dependencies declared in `Cargo.toml`; it has no Tokio runtime
or per-HTTP-client threads. From the repository root:

```sh
make test
make build
make armv7
```

All Cargo work shares `.build/rust`, runs serialized with one build job, and uses
no incremental cache. Do not create per-worker target directories. Other source
worktrees must point `CARGO_TARGET_DIR` at the integration tree's `.build/rust`.
The root README describes ARMv7 compiler and archiver requirements.

The release profile uses `opt-level = 3`, LTO, one codegen unit, `panic = "abort"`
and stripping. The complete production executable has been qualified as static
ARMv7; target resource costs must still be measured, not inferred from Rust.

## Entry modes

The default listener is `127.0.0.1:8790`. Diagnostic mode accepts only numeric
loopback addresses. It exposes health/memory and private subscription/rule draft
read, save and preview; it does not attach an owner or apply runtime changes.
On hosts without `/proc/meminfo`, use `--proc-root` with a fixture directory.

Native mode requires a nonempty `BE6500PANEL_PASSWORD`, existing private data/run
directories, the real `/proc` adapter and a verified command manifest. Use isolated
directories for development:

```sh
BE6500PANEL_PASSWORD='<private password from your secret store>' \
  .build/rust/release/be6500-panel --native-runtime \
  --listen 127.0.0.1:8790 --data-dir /absolute/private/data \
  --run-dir /absolute/private/run \
  --command-manifest /absolute/private/command-bindings.json \
  --web-dir /absolute/trusted/web/dist
```

Only authenticated native mode can bind beyond loopback. The optional static
root is one trusted, stable tree. API paths never fall through to browser assets.
Unknown, missing and duplicate flags fail before listening.

The private manifest is a `0600` JSON object of at most 4 KiB. It contains fixed
`ip` and `iptables` objects with absolute `path` and release `sha256`, a literal
`dnsBootstrap` address with port, and optional `routeTables` text. Optional
`singBox` and `frpc` objects bind local core executables by absolute `path` and
release `sha256`, verified before check/start. It does not admit core configuration,
arbitrary commands or TLS overrides.

## Product ownership and source map

- `server.rs`, `server_loop.rs`, `auth.rs`, `static_files.rs`: one bounded HTTP
  admission/owner lane, sessions, authenticated routes, SSE and static assets.
- `product_gateway.rs`, `product_observations.rs`, `product_io.rs`: system/router,
  network, device and module observations on native adapters.
- `product_configuration.rs`: private configuration generations, UCI staging,
  validation, provisional commits, confirmation and rollback.
- `product_telemetry.rs`: WAN rings, device activity/annotations and core metrics.
- `product_diagnostics.rs`, `product_events.rs`, `product_support.rs`: bounded
  diagnostics, event streams and public logs.
- `product_maintenance.rs`, `product_plans.rs`: backup/import, maintenance and
  bounded domain plans.
- `native_owner.rs`, `runtime_*`, `subscription*`, `rules_http.rs`, `rule_apply.rs`,
  `policy*`, `capture_*`: one exclusive core/FRPC owner, verified artifacts,
  subscriptions, policy compilation, actual readiness and owned capture cleanup.

The listener invokes bounded recovery/telemetry ticks without a browser client.
Long operations stay on the same owner lane. There are no HTTP client threads or
Tokio executor. Native commands and external core processes are explicit, bounded
runtime components, not arbitrary browser-supplied commands.

SIGTERM cancels new work. Failed withdrawal retains the same authenticated entry,
owner and handles for cleanup retry; the process does not claim clean shutdown or
abandon installed capture. Status reads do not start cores or apply capture.

## Handover and compatibility

The production handover record has a ready owner on `8776`, sing-box on `8806`,
runtime generation `9` and capture off. Accepted configuration generation `8`,
draft version `1` and the runtime hash were preserved. FRPC remains an independent
native component but is currently unconfigured. Process liveness does not prove
external tunnel connectivity.

Recorded qualification: 23 real views with zero actual alerts, 12 behavior checks,
589 full Rust tests and a static ARMv7 executable. The prior full frontend suite
passed 1,602 tests, followed by 38 copy/status tests. These records are not new
measurements from documentation edits.

Go project source (`cmd/`, Go files under `internal/`, `go.mod`, `go.sum`) is retired.
The frozen `mature-integration` tree outside this source tree is reference-only.
Keep all four golden compatibility JSON files under `tests/fixtures/`:
`native-go.json`, `subscription-go.json`, `local-policy-go.json`, `capture-go.json`.
Their Go-derived names describe provenance, not a build dependency. Live data,
accepted configuration, history, rescue SSH and factory ACLs are not source
cleanup targets. Native bootstrap and device changes belong to the root lane.

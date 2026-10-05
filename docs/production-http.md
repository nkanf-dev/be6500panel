# Production HTTP integration

The full native product uses the existing authenticated API and static browser
tree. `rust/panel/src/server.rs` and `server_loop.rs` own HTTP admission and the
single native-owner lane. `product_gateway.rs` assembles the management modules;
`runtime_http.rs`, `rules_http.rs` and `capture_http.rs` use that same owner.
There is no second management server, per-HTTP-client thread or Tokio runtime.

## Entry and request bounds

Diagnostic mode is loopback-only and has no runtime owner. Production requires
explicit `--native-runtime`, a nonempty `BE6500PANEL_PASSWORD`, private data/run
directories and a verified command manifest. `--web-dir` serves the existing
trusted browser tree. API paths never fall through to static assets.

Sessions protect management routes. Configuration content is not logged. Public
errors use stable code/status projections rather than private paths or OS errors.
Headers are bounded to 16 KiB / 64 fields, targets to 2,048 bytes. Normal JSON
bodies are at most 64 KiB. Explicit endpoint classes admit bounded larger bodies:
login 4 KiB, rules 256 KiB, runtime/subscription 2 MiB and backup import 3 MiB.
Absolute header/body and response deadlines apply; selected long operations have
explicit larger budgets. Acquisitions are synchronous on the owner lane, so their
frontend timeout must match the operation rather than the ordinary request budget.

The listener invokes bounded recovery and telemetry ticks without browser clients.
Product routes include native observations, UCI transactions, subscriptions/node
selection, proxy/FRPC lifecycle, rules/capture, telemetry/history, diagnostics,
backup/import and maintenance. GET status does not start a core or apply capture.

## Mutation and ownership

`product_configuration.rs` stages private generations before UCI mutation. Risky
commits remain provisional until confirmed. Verification or timeout failure
restores the prior generation. Management reachability must be verified after a
network reload; reload success alone is not a successful commit. Network changes
withdraw owned capture before mutation.

Capture accepts only compiled owned arguments from `capture_plan.rs` and
`capture_executor.rs`, never browser-provided commands. Existing rule/table state
is checked for collisions. Partial apply and process cleanup withdraw only owned
resources. Failed shutdown keeps the same authenticated entry and native owner
available for cleanup retry instead of abandoning capture or claiming clean exit.

The private `0600` manifest is at most 4 KiB. Required `ip`/`iptables` bindings and
optional `singBox`/`frpc` bindings use absolute executable paths and release
SHA256 values. `dnsBootstrap` is a literal address with port; `routeTables` is
optional text. No arbitrary command, configuration or TLS override is admitted.
FRPC is currently unconfigured; retained process liveness is not external tunnel
connectivity.

## Handover and historical seam

The production handover preserved accepted configuration generation `8`, draft
version `1`, runtime generation `9` and its hash on the same API/static tree. The
recorded native owner/core were ready on `8776`/`8806`, with capture off. See the
root README for the qualified views, behavior checks and test/build record.

The former Go `RuntimeDataDir` / `EnableControl` integration seam and
`internal/httpapi`, `internal/proxy` and process cleanup hook names are historical
reference only. Their frozen implementation remains in `mature-integration`
outside this tree. Current ownership is in the Rust paths above. Project Go
source is retired; all four golden compatibility JSON files in
`rust/panel/tests/fixtures/` remain. Build/test tooling is Rust and Bun only.

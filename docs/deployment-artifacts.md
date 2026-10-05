# Deployment artifacts

The panel is published as a signed/checksummed release artifact generated from public source. The package contains the static ARMv7 Rust executable and the existing browser asset tree. External sing-box and FRPC remain separate native artifacts with upstream notices. Credentials are never included. Makefile and CI build/test the project with Rust and Bun only; neither external core is a project Go build target. Native bootstrap and device installation are owned by the root deployment lane through `scripts/bootstrap.sh`.

The launcher keeps the last accepted panel archive in `/data`, reconstructs it under `/tmp` and starts LAN-bound authenticated control. Large core artifacts are downloaded to `/tmp` from fixed checksum-pinned URLs. Small native settings, operation journal and desired-runtime metadata persist. Missing artifacts produce rebuilding state, not a fabricated running state.

For sing-box, `GPL-3.0-or-later` notices and fixed source/build recipe accompany the minimal binary. FRPC is upstream Apache-2.0. CN rule data is fetched from official source, not bundled in the MIT panel repo: rule-generation programs are GPL-3.0-or-later; domain data has upstream MIT attribution; GeoLite2-derived IP data has MaxMind's data/EULA requirements. Follow update and removal requirements instead of treating a pinned geolocation asset as forever-valid.

## Native artifact admission

`src/artifact_source.rs`, `artifact_http.rs`, `artifact_stage.rs` and
`runtime_bindings.rs` own bounded native acquisition and release admission.
Certificate verification, artifact size bounds and SHA256 checks remain required.
No TLS verification is disabled. Acquisition uses the existing authenticated
runtime API, not a second HTTP service or per-client worker.

The private command manifest accepts required `ip`/`iptables` bindings and optional
`singBox`/`frpc` bindings. Each executable binding uses an absolute `path` and
release `sha256`; cores are verified before check/start. This supports retained
local native artifacts without adding a Go toolchain or accepting arbitrary
commands. FRPC is currently unconfigured; no external tunnel is claimed.

The old Go deployment flag `--artifact-transport curl` and its stock-curl budgets
are historical reference only in the frozen `mature-integration` tree outside this
tree. They are not Rust CLI options. Go project source is retired; the four
Go-derived golden JSON fixtures under `tests/fixtures/` remain required
compatibility evidence. Shared Cargo output stays in `.build/rust`, serialized
with one build job.

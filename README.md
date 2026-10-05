# be6500panel

A modular, professional control panel for the Xiaomi BE6500 (RN02).

**Native control plane:** Rust. The full native product now serves the existing authenticated API and static browser tree. It uses bounded requests, private configuration generations and one exclusive runtime owner. **Browser:** React, Effect, Motion, Tailwind CSS, accessible Radix/shadcn-style components and Apache ECharts. No Node.js runtime on the router.

## Modules

| Module | Capabilities |
| --- | --- |
| System | CPU/memory/load/uptime, diagnostics, native system configuration |
| Network | Interfaces, IPv4/IPv6 routes, traffic counters, native WAN/LAN commits |
| Devices | DHCP leases and ARP observations |
| Wi-Fi | Radio/SSID configuration observation, native wireless editor |
| DNS / DHCP | Resolver and lease observations, native DHCP/DNS configuration |
| Firewall | IPv4/IPv6 policy observations and native firewall commits |
| Proxy | Local VLESS import, REALITY/Vision/uTLS, native core management and scoped split-routing intent |
| FRPC | Native TOML configuration, verified artifact acquisition, start/stop and process supervision |

The console uses dense tables, Cmd+K, configuration diffs, explicit Commit, risk confirmation and provisional rollback. Light/dark/system themes share semantic tokens with traffic charts, waterfalls, heatmaps, latency distributions and rule-hit charts. Actual interface samples feed the live traffic graph; other collectors retain a short empty state until connected.

## Configuration workflow

**Edit → Stage → Diff/Validate → Commit.** Editing or staging does not change live UCI files. Risk-sensitive network, management and primary Wi-Fi commits require one acknowledgment and remain provisional until confirmed. Timeout or failed verification restores the previous generation. Native expert editing supports network, wireless, DHCP, firewall, system and Dropbear documents.

Runtime configurations use explicit **Commit 配置**, followed by Start/Stop. Subscriptions are parsed locally; no public conversion service receives node credentials. Proxy and FRPC are independent modules.

## Development

Rust 1.93, Bun and Node.js 20.20+. Makefile and CI build/test the project with Rust and Bun only; Go is not a project build or test requirement.

```text
make setup
make dev-api
# Another terminal:
make dev-web
make test
make build
make armv7
```

The development service defaults to loopback `127.0.0.1:8790` diagnostic mode without a runtime owner. Production uses explicit `--native-runtime` mode with a password, existing private data/run directories and a verified command manifest. The full product includes system/router observations, UCI transactions, subscriptions and node selection, core/FRPC lifecycle, rules and scoped capture, telemetry/history, diagnostics, backup/import and maintenance. Add `--web-dir` to serve the same browser assets.

Native mode example (use isolated directories for development):

```text
BE6500PANEL_PASSWORD='<private password from your secret store>' \
  cargo run --locked --manifest-path rust/panel/Cargo.toml -- \
  --native-runtime --listen 127.0.0.1:8790 \
  --data-dir /absolute/private/data --run-dir /absolute/private/run \
  --command-manifest /absolute/private/command-bindings.json
```

The command manifest is a private `0600` JSON object, at most 4 KiB. Its fields are `ip` and `iptables` objects with absolute `path` and release `sha256`, `dnsBootstrap` as a literal address with port, and optional `routeTables` text. Optional `singBox` and `frpc` objects bind fixed local executables by absolute `path` and release `sha256`; the runtime verifies them before check/start. The manifest does not admit core configuration, arbitrary commands, browser paths or certificate overrides. SIGTERM cancels new work. If withdrawal fails, the same authenticated entry and owner remain alive for explicit cleanup retry; the process does not claim a clean shutdown or abandon installed capture.

All Cargo commands share `.build/rust`, run serialized with one build job and use no incremental cache. Do not pass a per-worker `--target-dir`. Separate source worktrees must set `CARGO_TARGET_DIR` to the integration worktree's `.build/rust` and share the existing `web/node_modules` directory.

For ARMv7 cross-builds, install the Rust `armv7-unknown-linux-musleabihf` target. Native TLS also needs an ARM-capable compiler and archiver, not just a linker. Linux CI uses `arm-linux-gnueabihf-gcc` and `arm-linux-gnueabihf-ar`.

On this Apple Silicon host, install `llvm-tools-preview` for Rust 1.93.0. Set `RUST_TOOL_BIN` to the absolute `lib/rustlib/aarch64-apple-darwin/bin` directory inside that Rust toolchain, then run:

```text
make armv7 CARGO="cargo +1.93.0" ARM_CC=clang \
  ARM_AR="$RUST_TOOL_BIN/llvm-ar" ARM_LINKER="$RUST_TOOL_BIN/rust-lld" \
  ARM_CFLAGS="-ffreestanding -U__musl__ -march=armv7-a -mfpu=vfpv3-d16 -mfloat-abi=hard" \
  ARM_RUSTFLAGS="-C linker-flavor=ld.lld -C target-feature=+crt-static -C link-self-contained=yes"
```

Apple `ar` does not build the required ARM ELF crypto archive. If changing archivers after an earlier build, clean only the affected Ring ARM package before rebuilding:

```text
cargo +1.93.0 clean --manifest-path rust/panel/Cargo.toml --package ring --release --target armv7-unknown-linux-musleabihf
```

Inspect the complete native executable, not an earlier diagnostic binary whose unused TLS code was removed by the linker. The qualified build is static ARMv7. Cross-build success alone does not prove device RSS, latency or process lifecycle.

## Production handover and source retirement

The full native product has completed the production handover on the same authenticated API and static tree. The qualification record for source `483f780d7f35e1f3070ff897b713e5a9af33c30b` has a ready native owner on port `8776`, a ready sing-box core on port `8806` at runtime generation `9`, and capture **off**. Accepted configuration generation `8`, draft version `1`, and the runtime configuration hash were preserved. These values describe the qualified handover snapshot.

Qualification covered 23 real views with zero actual alerts and 12 behavior checks. The full Rust suite passed 589 tests and the complete ARMv7 executable was static. The prior full frontend run passed 1,602 tests; a further 38 copy/status tests passed. These are recorded results, not checks rerun by documentation edits.

`cmd/`, Go source under `internal/`, `go.mod` and `go.sum` are retired project source. The frozen `mature-integration` tree outside this tree remains a historical reference, not a build dependency. Keep all four compatibility JSON fixtures in `rust/panel/tests/fixtures/`: `native-go.json`, `subscription-go.json`, `local-policy-go.json` and `capture-go.json`.

User rule drafts, accepted configurations, native routing data, history, rescue SSH and factory ACLs remain intact. External sing-box and FRPC remain native runtime components, not project Go build targets. FRPC is currently unconfigured; no external tunnel connectivity is claimed. `scripts/bootstrap.sh` is the native bootstrap entry point; deployment and live-device changes belong to the root deployment lane.

## Architecture

```text
Browser modules ─ Shared UI ─ Theme ─ Visualizations
                         │ JSON / SSE
              Registry / Sessions / Coordinator
                         │
     System · Network · Devices · Wi-Fi · DNS · Firewall
                      Proxy · FRPC
                         │
       RN02 adapter · Config transactions · Runtime manager
```

Resource ownership is explicit: network owns routes; DNS owns resolver intent; firewall owns chains and marks; proxy requests compiled contributions. Runtime binaries are fixed executable types, not shell commands. Process exits trigger owned cleanup and bounded restart backoff.

[Architecture](docs/superpowers/specs/2026-10-02-be6500panel-design.md) · [Production design](docs/superpowers/specs/2026-10-02-production-deployment.md) · [Official capability matrix](docs/official-capability-matrix.md) · [Split policy](docs/proxy-policy.md) · [Runtime provenance](docs/runtime-provenance.md)

## Official feature coverage

The capability matrix tracks dedicated RN02 adapters for official Wi-Fi7/MLO/Mesh, multi-WAN, QoS/ECM, IPTV/VLAN, NAT/UPnP, parental/security, DDNS/VPN and maintenance features. Native UCI editing is a control mechanism, not a substitute for those dedicated operations. The current runtime and transaction layer is the base for that coverage.

## Source boundary

Only source, tests and synthetic examples belong in Git. Subscriptions, keys, Wi-Fi credentials, live snapshots and private configuration remain on the managed device/developer machine. The panel is MIT; external binaries and data retain their upstream licenses.

Browser CI runs isolated fixture suites without starting a Go process. The old
Go-demo-dependent `console.spec.ts` and `modern-layout.spec.ts` remain historical
reference tests, not active Rust parity gates. Production handover evidence comes
from the real views and behavior checks above; it is separate from fixture CI.

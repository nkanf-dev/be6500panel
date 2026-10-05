# be6500panel

A modular, professional control panel for the Xiaomi BE6500 (RN02).

**Native control plane:** Rust. The replacement uses bounded requests, private configuration generations and explicit resource ownership. Production migration is not complete; the existing device manager remains in place until the safe handover is qualified. **Browser:** React, Effect, Motion, Tailwind CSS, accessible Radix/shadcn-style components and Apache ECharts. No Node.js runtime on the router.

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

Rust 1.93, Bun and Node.js 20.20+. Go is no longer a project build requirement.

```text
make setup
make dev-api
# Another terminal:
make dev-web
make test
make build
make armv7
```

The Rust development service defaults to loopback `127.0.0.1:8790` diagnostic mode. It serves rule draft read/save/preview and memory diagnostics without a runtime owner. An explicit `--native-runtime` mode attaches the one exclusive native owner. It requires a password, existing private data/run directories and a private verified command manifest. This mode supports authenticated core/FRPC acquisition, configuration, start/stop, rule Apply and scoped capture. Subscription import/node selection and other management modules are still being ported. It is not yet a production replacement.

Native mode example (use isolated directories until handover is qualified):

```text
BE6500PANEL_PASSWORD=<private password from your secret store> \
  cargo run --locked --manifest-path rust/panel/Cargo.toml -- \
  --native-runtime --listen 127.0.0.1:8790 \
  --data-dir /absolute/private/data --run-dir /absolute/private/run \
  --command-manifest /absolute/private/command-bindings.json
```

The command manifest is a private `0600` JSON object, at most 4 KiB. Its fixed fields are `ip` and `iptables` objects with absolute `path` and release `sha256`, `dnsBootstrap` as a literal address with port, and optional `routeTables` text. It does not admit core boot files, shell commands, browser paths or certificate overrides. SIGTERM cancels new work. If withdrawal fails, the same authenticated entry and owner remain alive for explicit cleanup retry; the process does not claim a clean shutdown or abandon installed capture.

All Cargo commands use `.build/rust`, one build job and no incremental cache. Do not pass a per-worker `--target-dir`. Separate source worktrees must set `CARGO_TARGET_DIR` to the integration worktree's `.build/rust` and share the existing `web/node_modules` directory.

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

Inspect the complete native executable, not an earlier diagnostic binary whose unused TLS code was removed by the linker. Cross-build success does not prove device RSS, latency, process lifecycle or production readiness.

## Production migration boundary

Do not install the current Rust slice over the running device manager yet. The existing manager and native cores remain independent from source-side cleanup. Production activation requires actual readiness, resource budgets, exclusive ownership, remaining API parity and data migration checks.

The obsolete extra Go management front has been retired. User rule drafts, accepted configurations, native routing data, history, rescue SSH and factory ACLs are preserved. External sing-box and FRPC are retained components, not project Go build targets.

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

Browser CI currently runs isolated fixture suites only. The old Go-demo-dependent
`console.spec.ts` and `modern-layout.spec.ts` remain historical tests, not active
Rust parity gates. They must be ported to actual Rust contracts before production
migration is declared complete. No browser test starts a Go process.

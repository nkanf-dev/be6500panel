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

The Rust development service binds to `127.0.0.1:8790`. It implements authenticated rule draft read/save/preview and memory diagnostics. Runtime Apply and unrelated management APIs remain unavailable until their migration is complete. This is not yet a production replacement.

All Cargo commands use `.build/rust`, one build job and no incremental cache. Do not pass a per-worker `--target-dir`. Separate source worktrees must set `CARGO_TARGET_DIR` to the integration worktree's `.build/rust` and share the existing `web/node_modules` directory.

For ARMv7 cross-builds, install the Rust `armv7-unknown-linux-musleabihf` target and provide a compatible linker. The current macOS validation uses Rust's bundled `rust-lld` with `linker-flavor=ld.lld`, `target-feature=+crt-static` and `link-self-contained=yes`; Linux CI uses `arm-linux-gnueabihf-gcc`.

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

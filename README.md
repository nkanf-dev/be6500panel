# be6500panel

A modular control plane for the Xiaomi BE6500 (RN02), built for professional users.

**Core:** one Go service, typed APIs, bounded events and a shared change coordinator. **Browser:** React, TypeScript, Effect, Motion, Tailwind CSS, accessible Radix/shadcn-style components and Apache ECharts. No Node.js runtime on the router.

## Modules

| Module | Initial capability |
| --- | --- |
| System | Linux CPU, memory, load and uptime observations |
| Network | Interfaces and addresses |
| Devices / Wi-Fi / DNS / firewall | Adapter registration and capability state |
| Proxy | Split-policy validation and coordinated plan preview |
| FRPC | Tunnel validation and exposure-aware plan preview |

The console uses dense tables, keyboard navigation, an operation plan inspector and structured diagnostics. Traffic charts, phase waterfalls, activity heatmaps, latency distributions and rule-hit charts share a decoupled theme layer. Demo datasets carry a source label; disconnected collectors use an empty state.

## Development

Go 1.26, Bun and Node.js 20.20+.

```text
make setup
make dev-api
# Another terminal:
make dev-web
```

API: `127.0.0.1:8787`. The development API uses `--demo`. Omit that flag for Linux host observations.

```text
make test
make build
./dist/be6500panel --listen 127.0.0.1:8787 --demo --web-dir web/dist
make armv7
```

Browser assets are local static files. Non-loopback service binding requires `BE6500PANEL_PASSWORD`; authentication uses a session cookie. Linux `/proc` observation errors appear in diagnostics.

## Architecture

```text
Browser modules ─ Shared UI ─ Theme ─ Visualizations
                         │ JSON / SSE
              Registry / Sessions / Coordinator
                         │
     System · Network · Devices · Wi-Fi · DNS · Firewall
                      Proxy · FRPC
                         │
                  Platform adapters
```

Modules register their capabilities and own domain state. Network, DNS and firewall own their respective resources; proxy and FRPC request coordinated contributions. Apply follows validation, planning, verification and rollback. The initial build exposes observations and plans; runtime execution is the next implementation stage.

[Design](docs/superpowers/specs/2026-10-02-be6500panel-design.md) · [Plan](docs/superpowers/plans/2026-10-02-be6500panel-foundation.md) · [Module contracts](docs/modules.md)

## Runtime direction

The target firmware uses **ARMv7**, a read-only root filesystem and a small persistent data partition. Runtime binaries can be downloaded, checksum-verified and reconstructed in `/tmp` at boot. Small configuration stays persistent. Bootstrap sources must be directly reachable before the proxy starts.

Proxy split policy combines explicit overrides, domestic domain/IP sets, DNS routing, UDP and IPv6. Gateway process-name rules are not used for forwarded clients. Management bypass and a defined failure policy are coordinated with the network module. FRPC is independent and never publishes the panel automatically.

## Source boundary

Source, tests and synthetic examples only. Device snapshots, subscriptions, keys and live configurations stay outside the public repository. External runtime binaries retain their own licenses.

MIT license.

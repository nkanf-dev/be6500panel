# Local proxy telemetry

`internal/telemetry` reads the accepted sing-box configuration through a private callback. It never exposes native configuration or the core listener to the browser. It does not modify routing, start the core, or activate capture.

## Upstream capability evidence

The existing minimal ARMv7 build recipe in `docs/runtime-provenance.md` omits `with_clash_api`. Unmodified sing-box v1.14.2 `include/clashapi_stub.go` explicitly returns `clash api is not included in this build, rebuild with -tags with_clash_api`. Enable the supported upstream build tag before adding `experimental.clash_api` to native configuration. Do not patch the core.

Source references, all v1.14.2:

- `include/clashapi.go` / `include/clashapi_stub.go`: feature admission.
- `option/experimental.go`: supported Clash API configuration.
- `experimental/clashapi/connections.go`: finite GET `/connections`, active start/upload/download/chains/matched rule/traffic totals. No connection end time, handshake phases or HTTP request trace is returned.
- `experimental/clashapi/proxies.go`: explicit GET `/proxies/proxy/delay`, timeout 5000 ms. Delay zero is rejected by the upstream endpoint, not a valid measured zero. The upstream probe context does not inherit request cancellation, but its own timeout remains bounded.
- `common/urltest/urltest.go`: outbound TCP + TLS + HEAD request measurement, not per-connection RTT. Fixed target `https://www.gstatic.com/generate_204`.

Native options: `external_controller: "127.0.0.1:9090"`, a privately generated persistent `secret`, no external UI or cache file. `ClashOptions(secret)` returns these supported private options. Root must check the complete native candidate with the exact target core and preserve the options on node selection. Old builds/configurations without this feature stay clearly unavailable.

## Integration

```go
collector, err := telemetry.New(telemetry.Options{
    Config: func(ctx context.Context) (telemetry.CoreConfig, error) {
        status, err := runtimeManager.Status(managedruntime.SingBox)
        if err != nil || status.State != managedruntime.Running {
            return telemetry.CoreConfig{}, telemetry.ErrUnavailable
        }
        raw, generation, err := runtimeManager.Config(managedruntime.SingBox)
        if err != nil { return telemetry.CoreConfig{}, telemetry.ErrUnavailable }
        cfg, err := telemetry.FromNativeConfig(raw)
        cfg.Epoch = fmt.Sprintf("%d:%d", generation, status.PID)
        return cfg, err
    },
})
// Check err, then Start with server lifetime context; Close on server shutdown.
collector.Start(ctx)
```

`Snapshot() Snapshot` is a pure cached read. `Probe(context.Context) error` performs one explicit fixed selected-outbound request. `Collect(context.Context) error` is available for tests or a one-off cache refresh. Nil collectors should return `Unavailable(fixedReason)` rather than fake samples.

HTTP contract: authenticated GET `/api/proxy/metrics` returns `Snapshot`. Authenticated same-origin POST `/api/proxy/probe` accepts only an empty JSON object, performs `Probe`, and returns the latest snapshot. Never accept a user URL, outbound name, or core address. Error mapping: `ErrBusy` 409, `ErrCooldown` 429, `ErrUnavailable` 503, `ErrProbeFailed` 502. Existing panel auth/JSON/origin middleware remains in charge.

The frontend boundary is `web/src/modules/proxy/telemetry-api.ts`. `use-proxy-telemetry.ts` polls only the cached panel API every four seconds. It cancels reads on unmount and never probes automatically. `ProxyTelemetryOverview` has no required props and is safe for the homepage dashboard.

## Coverage and bounds

- Default core collection interval is two seconds. Each finite response is limited to 1 MiB and 4096 raw connections before typed allocation; only the newest 128 connection rows leave the collector.
- Actual client/target IP, ports, and bounded destination hostname remain useful to the authenticated owner. No process path, subscription/server credentials, core secret, UUID, private native config, or raw core error is returned. Public ID/rule ID are per-process HMAC references. Supported matched domain/IP/rule-set routing conditions remain readable; credential-bearing condition types are omitted.
- Core cumulative totals include direct and proxied connections. These are not WAN interface totals. Rates come from real elapsed time and counter deltas. First samples, reset, epoch changes, and missed intervals produce explicit gaps, not measured zero or spikes.
- The volatile ring holds at most 900 rate samples (about 30 minutes at default cadence) and 32 explicit probes. It is deliberately separate from durable WAN history and consumes no persistent flash. Panel restart clears the ring. Node/core epoch changes clear old selected-node probes.
- Active routing charts count only connections visible in the current bounded snapshot. They are not global rule-hit counters. Short flows between polls and blocked attempts can be absent. Connection start and age are real; end time and request phases are unavailable.
- Read failures retain the last observed sample as `stale`, with current capabilities unavailable and a fixed reason. A stale chart is visibly marked as historical; active probing is disabled in that UI state.
- Only literal loopback API listeners are admitted. The HTTP client ignores environment proxy settings and rejects redirects. Neither the browser nor collector can select a LAN/WAN destination.

Validation: `go test -race ./internal/telemetry`, `go test ./...`, and `cd web && bun run typecheck && bun run test && bun run build`.

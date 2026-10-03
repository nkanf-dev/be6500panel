# Recent device activity

The application owns collection. Create `devicetelemetry.New(router.NewDeviceSource(adapter))`, call `Start(ctx)` once, and call `Close()` during shutdown. `httpapi.DeviceActivity` reads memory only. The authenticated server must wire `GET /api/devices/activity`.

- Fixed read-only source: `ubus call trafficd hw {"detail":true,"wlan":true,"mlo":true}`. A fixture root reads only `/var/run/be6500panel/trafficd-hw.json`.
- Canonical MAC identity. MLO MAC-interface rows retain distinct wireless links but identical per-IP counters are counted once. Conflicting per-link counter sets yield no counter coverage.
- Byte counter deltas drive heatmap and range totals. Latest B/s rates use one consecutive observed interval, not vendor rate fields. RX/TX remains vendor direction. Wireless negotiated rates stay vendor strings.
- First sample, source failure, reset, long gap, IP set change and reappearance establish baselines without inventing bytes. Missing points serialize `null`; real zero has positive coverage.
- Device cap 128, address cap 16, source cap 512 KiB/512 rows. Memory has 289 five-minute buckets and 169 hourly buckets per retained identity. The fixed bucket payload is 1,875,968 bytes (about 1.79 MiB), plus bounded metadata. No files or flash allocations.
- Detail retention: 24 hours. Coarse hourly retention: seven days. It clears on process restart. This does not change the independent persistent WAN yearly history.
- Query ranges `30m`, `24h`, `7d`; maxPoints 1–288, row limit 1–64, search at most 64 characters. Range edges round to bucket boundaries. Groups include all matching identities before top-row truncation. Group/device totals only imply a measurement when `coverageSeconds > 0`.
- Current IP conflicts are listed per MAC and never silently merged. `search=<canonical MAC>&limit=1` supports an individual device query.
- No request counts, TLS decryption, active connection fabrication, capture/offload changes, or diagnostic probes.

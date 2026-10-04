# Professional history: source map and bounded storage design

## Scope and delivery status

This batch extends the existing persistent WAN query presets. It does not add
connection, proxy, device, activity, latency, count, or request-trace history.
It does not change a collector, API route, physical ring, or chart implementation.

The canonical WAN presets, in selector order, are:

`30m`, `1h`, `3h`, `6h`, `10h`, `12h`, `1d`, `3d`, `7d`, `30d`, `180d`, `1y`.

`1d` has the single label **最近 24 小时**. WAN has no prior `24h` alias, so this
batch does not introduce one. The existing device source still uses its own
`24h` contract; unifying that source requires a separate backend change.
The WAN selector already maps `TRAFFIC_HISTORY_RANGES` and its labels. Empty
older coverage stays empty. Enabling a year-long selection does not create old
measurements.

## Current source inventory

These are source-code facts, not new router measurements. The router budget of
about 20 MiB total on `/data`, with about 5 MiB free, is supplied deployment
context. Free space must be measured again before admitting any new store.

| Source and existing contract | Granularity and limits | Current history / persistence | Coverage limits |
| --- | --- | --- | --- |
| WAN, `internal/traffic`, GET `/api/traffic/history` | Raw default-route interface counters every 2 seconds; one aggregate series; default 1,500 output points, maximum 2,000 | Fixed 30-second / 48-hour, 5-minute / 30-day, and 1-hour / 400-day rings; 3,072,192 bytes total | Counter deltas and observed seconds only. Baselines, outages, resets, source changes and regressions leave gaps. `oldestAt` is retained measured coverage, not configured retention. |
| Core traffic, `internal/telemetry`, GET `/api/proxy/metrics` | Core upload/download counter deltas at the default 2-second polling interval, including direct traffic | Last 900 samples in RAM; roughly 30 minutes at a healthy default cadence; lost on restart | Not WAN totals. Polling gaps and core epochs reset the baseline. Neither 900 points nor elapsed wall time proves continuous coverage. |
| Core connections and routing, same metrics snapshot | At most 128 returned active rows, newest first; total active count and truncation are separate; oversized input above 4,096 rows is rejected | Current/last observed active list only; no closed-connection record store | Start/age, original tuple, host, bytes, actual observed native rule and outbound are available. No exact close time, device MAC/name, complete rule-hit count, rejected-flow count, or client request phase history. Short flows between polls and rows outside the returned cap may be missed. |
| Device traffic, `internal/devicetelemetry`, GET `/api/devices/activity` | `trafficd` MAC-keyed counters every 15 seconds; at most 128 devices, 16 addresses/device, 512 source rows and 512 KiB input | RAM only: 289 five-minute buckets and 169 one-hour buckets/device; fixed bucket payload 1,875,968 bytes; seven-day retention ceiling; reboot loses all history | Existing queries: `30m` uses five-minute buckets; `24h` and `7d` use hourly buckets. At most 288 points and 64 returned devices/query. Missing byte values are null; observed zero retains coverage. RX/TX vendor direction must not be relabeled as verified upload/download. |
| Router/browser traffic cache, `web/src/app/console-context.tsx` | Browser refresh nominally every 2 seconds; current source retains **300** rate points | Tab-owned volatile cache, roughly ten minutes at uninterrupted cadence | Not the production WAN history widget. Earlier descriptions of a 600-sample browser cache are not a retention contract and do not describe this revision. The server core sampler holds only its latest observation, not a 600-point WAN archive. |
| Manual request traces, `internal/requesttrace`, GET `/api/proxy/request-traces` | Last 64 opt-in fixed-target diagnostics; one running request; 10-second timeout; response body capped at 64 KiB | RAM only; timestamped started/finished diagnostics, no autonomous collection | DNS/TCP/TLS/TTFB phases describe the panel's own diagnostic request, not forwarded client HTTPS. Phase duration units are milliseconds; absent phases remain unobserved, not zero. |
| Manual proxy latency probes, core telemetry | Last 32 probe results | RAM only; event-count cap, not an elapsed-time retention promise | Delay is milliseconds for a selected diagnostic route, not connection RTT. No year-long probe series exists. |
| Panel events, `internal/core/logs.go`, GET `/api/logs` | Last 500 fixed public event projections | RAM only | Selected level/code/module/message events, not full connection/access logs. Not a record of every request or every rule hit. |

Device name annotations are a separate bounded persistent identity store. They
are not a historical device-to-address map and must not rewrite past flow names.
No client application/process is inferred from a domain name.

### WAN tier and query behavior after the extension

| Presets | Existing native tier | Native retention | Example returned resolution at 1,500 points |
| --- | ---: | ---: | ---: |
| `30m`, `1h`, `3h`, `6h`, `10h`, `12h` | 30 seconds | 48 hours | 30 seconds |
| `1d` | 30 seconds | 48 hours | 60 seconds |
| `3d` | 5 minutes | 30 days | 5 minutes |
| `7d` | 5 minutes | 30 days | 10 minutes |
| `30d` | 5 minutes | 30 days | 30 minutes |
| `180d` | 1 hour | 400 days | 3 hours |
| `1y` (365 days) | 1 hour | 400 days | 6 hours |

Returned resolution is an integer multiple of the native tier and depends on
`maxPoints`. The current bucket adds one native slot to the requested count.
Query edges round outward to native boundaries. Query uses one tier only,
sums byte totals and coverage, and takes peak maxima. It never sums rates or
combines duplicate overlapping tiers. The 400-day ring has a 35-day reserve
beyond the one-year query. No history file grows when a preset is added.

The current files are unchanged:

| File | Exact allocated bytes |
| --- | ---: |
| `wan-30s.ring` | 737,344 |
| `wan-300s.ring` | 1,105,984 |
| `wan-3600s.ring` | 1,228,864 |
| **Total** | **3,072,192** |

See `docs/traffic-history-storage.md` for the existing storage mechanics; this
report supersedes its older list of public range names, not its ring layout.

## Next backend: small record store, not a new framework

**Design only.** Root owns the next source/backend batch after the immediate UI
fixes. First measure router space, source cardinality, bytes per encoded record,
observation frequency, dirty writes and peak RAM. Do not start with a generic
service, concurrency protocol, database migration, or unbounded JSON archive.
Reuse the traffic ring's admission, fixed-allocation, checksum, alternate-slot,
minute flush, startup recovery and visible-error patterns where they fit.

Recommended retention policy:

1. Keep detailed observed connection records for a target of **24–72 hours**.
   Evict at the earliest of the age limit and the storage cap. Set an initial
   detailed store cap of **1 MiB**, and an absolute configurable ceiling of
   **2 MiB**, including detailed indexes/record overhead. These are caps, not a
   guaranteed time horizon under heavy load. Report actual oldest retained time,
   evictions, dropped observations and truncation.
2. Roll older data into measured **5-minute and 1-hour aggregates**, with a
   **400-day** long-tier target, like WAN. Aggregate bytes and observed flow
   counts by stable device, destination, native rule and outbound IDs. Keep each
   source and direction distinct. Read only one covering tier per group/span.
3. Admit a fixed number of aggregate identities after calculating its disk
   layout. Preserve a bounded global/other group when an identity is not
   admitted; expose grouping/truncation rather than silently dropping it.
   The existing WAN aggregate already has the 400-day guarantee. A new
   per-device/per-destination store must earn its own retention guarantee;
   the fixed detail cap cannot promise arbitrary-cardinality 400-day history.
4. Keep selected panel event logs, request diagnostics and probes in separate
   small fixed-cap stores only if their persistence is actually needed.
   Account for every store and its metadata before allocation. A shared chart
   range contract does not force all sources to retain full detailed events.

### Fixed budget and flash admission

An illustrative next-store envelope is **3 MiB maximum new allocated files**:
1 MiB detailed connections, at most 1.75 MiB aggregate payload/indexes, and
at most 0.25 MiB bounded shared device/destination/rule/outbound dictionaries.
Selected logs, diagnostics and their indexes must fit this same envelope or
remain RAM-only. Do not allocate an extra log store outside the accounting.
Increasing detail to 2 MiB requires rebalancing/re-admitting this envelope or
explicit external storage. It is not an automatic extra 2 MiB allowance.

This is a candidate ceiling, **not approval to consume the currently reported
5 MiB free**. Existing WAN allocation is already part of the used space; do not
subtract it twice. `internal/storage` protects at least 1 MiB ordinary recovery
headroom and maintains a 256 KiB physically allocated emergency reserve when
configured. New history cannot reclaim emergency recovery space. Measure free
bytes and outstanding reservations on the actual volume, add filesystem
metadata and full temporary allocation, and reserve the exact upgrade/rollback
staging requirements. Deny new growth if any of those conditions fail. Existing
complete fixed rings remain readable without new allocation. No background
history job deletes accepted configuration or fills flash to rescue its quota.

For scale: a single aggregate identity using the existing two-copy 64-byte
record layout for 30-day five-minute and 400-day hourly tiers already needs
`(8,640 + 9,600) * 128 + 2 * 64 = 2,334,848` bytes. That leaves no room for an
unbounded device × destination × rule × outbound grid. Even a hypothetical
compact single-copy 32-byte layout would need 583,680 bytes/identity before
indexes and recovery copies. Encoding savings alone do not solve cardinality.
Benchmark a smaller bounded grouped/sparse layout and its retention under load
before promising anything beyond the WAN aggregate. If the proposed on-flash
layout cannot fit, reduce admitted groups/detail depth or use external storage.

Use compact integer device/destination/rule/outbound IDs and unsigned integer
UTC timestamps/counters in stored records. Bound the dictionaries and string
lengths. Freeze the observed device name/MAC mapping with its mapping timestamp;
do not resolve history through today's DHCP/name table. Preserve the original
source IP/port and destination IP/host/port/network even when no MAC mapping is
known. Rotate/evict dictionary entries only when no retained record references
them. Store structured missing/overflow reasons, not zeros that imply success.

Record changed counters and lifecycle observations in bounded RAM, then batch
flush at a measured cadence. Do not persist every identical active snapshot or
rewrite the full archive at every poll. Calculate logical bytes per minute from
changed slots and measure real filesystem/UBI write amplification separately.
A full 2 MiB turnover every day is about 730 MiB/year of logical writes before
flash amplification. Set a write-rate cap and record coverage loss if it is hit.

A hundred million full connection records/year cannot be promised on this
flash. Even 64 bytes/record is 6.4 GB before strings, indexes and recovery copies.
For longer detailed history, require an explicit operator-selected mounted path,
for example `history-data-dir=/mnt/usb/be6500panel-history`, with its own measured
budget and health. An explicit operator export destination, for example
`/mnt/usb/be6500panel-exports`, is an alternative for snapshots. These are
proposed configuration paths, not existing implemented flags, and no path or
remote upload is enabled by default. A missing external mount must fail that
source, not fall back to `/data` or RAM while claiming persistence.

## Record semantics, queries and chart capabilities

A future connection record needs:

- Stable observed flow identity plus core epoch, original tuple and destination
  host/IP/port/network; original native rule description/ID and actual outbound.
- `firstSeenAt`, `lastSeenAt`, native start if supplied, observed byte counters
  and valid deltas. Disappearance emits an **observed-ended** event tied to the
  last successful observation and subsequent successful absence observation.
  It is not an exact transport close timestamp. A failed read, core restart or
  truncated list cannot prove disappearance; retain an unknown/lost state.
- Observed device MAC/name, mapping observation timestamp and mapping source.
  Missing/conflicting mapping remains unknown. Never retroactively rename
  earlier records or infer a process/application from a domain.
- Source sampling coverage, cap/truncation and missed/dropped observation
  reasons. Polling cannot quantify every short unobserved flow; expose that
  limitation without inventing a missed-flow count.

Use a bounded authenticated, read-only query API. Suggested hard limits are
2,000 aggregate points, 128 detail rows/page, a bounded cursor, 128 characters
of search and a fixed scan/output budget. Filter on the backend before page
limits. Search retained MAC/IP/device-name snapshots, host/domain, complete
original tuple, native rule and outbound, plus application metadata only when
an actual trusted source supplies it. Do not call a search of the current 128
active rows a search of all historical connections. Preserve native rule
provenance; joins to a later local-policy stable ID need their own validated map.

Each historical source should eventually publish a capability contract with:

- Supported canonical presets and source-specific native/returned resolution.
- Persistence state, configured retention, measured oldest/latest timestamps,
  sampling interval and captured time coverage.
- Coverage status/reasons, truncated/dropped rows and observed flow counts with
  their semantics. `capturedFlows` counts observed identities, not all traffic.
- Explicit missing buckets/null values; zero only for a measured zero. Count
  charts stay counts; latency/phases stay milliseconds; timestamps stay UTC.

All historical charts can then share the preset order while querying their own
source. Do not add a year selector to the current active-connection view or
900-point core rate chart. Unknown old coverage must be shown as absent data,
not borrowed from WAN or filled with zeros. Long source capability support is
not delivered until backend collection, storage, bounded queries, UI and
readback tests pass.

## Verification boundary

`internal/traffic/range_presets_test.go` verifies all twelve names and durations,
point bounds, native tier selection, returned integer-multiple resolution,
full-range measured totals after simulated wrap-around, byte/coverage
conservation, peaks and missing buckets. Its old-layout fixture
asserts exact physical sizes and SHA-256 hashes remain unchanged through
open, all range queries, clean flush and close, with zero allocation growth.
The fixture holds eight seconds of measured coverage; it does not claim a year
of real data.

The existing `TestOneYearRetentionBoundedRingsAndExactDownsampling` independently
fills 405 simulated days, exercises all ring capacities/wrap-around, verifies
400-day oldest coverage and a one-year query, and reopens persisted files.
This is retention-capacity evidence, not proof that the live router has already
collected a year's history. Existing restart, corruption, coverage and storage
failure tests remain in the native suite.

Frontend range tests cover exact order/seconds/labels, Effect schema decoding,
all GET range requests, automatic selector options, a single 24-hour label,
and no chart/export for an all-uncovered range. Existing UTC/CSV checks keep
raw counter totals and empty values for uncovered buckets. No byte-format,
chart-unit or request-phase implementation is changed by this batch.

Local verification commands use existing dependencies only:

```sh
go test ./internal/traffic
go test -race ./internal/traffic
go vet ./internal/traffic
cd web
bun run test src/lib/traffic-history-api.test.ts src/components/traffic-history/TrafficHistoryPanel.test.tsx src/components/traffic-history/use-traffic-history.test.tsx --maxWorkers=1 --no-file-parallelism
bun run typecheck
```

Results for this batch: native traffic tests, race and vet passed. The
serialized frontend run passed **36 tests across three files**. Full frontend
`tsc --noEmit` passed using the existing linked dependencies; nothing was
installed. The new Go file adds five scoped preset tests. No full frontend
build or browser acceptance is claimed by this range-only batch.

No SSH, browser session, real core, diagnostic network request, live ring,
service restart or router configuration write was performed for these checks.

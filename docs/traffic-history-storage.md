# WAN traffic history storage

`internal/traffic` records one real WAN series. A server-owned worker samples
`router.NewWANSource(routerAdapter).Snapshot` every two seconds. This bounded
source reads only `/proc/net/dev`, `/proc/net/route`, and IPv6 routes when no
IPv4 default exists. It has its own cache/lock and never reads device leases,
WiFi, platform, or firewall, and never starts subprocesses. No browser, SSE subscription, chart,
or device client needs to be connected. It uses raw receive/transmit counters,
not the adapter's rate fields.

## Enable and integrate

Provide `traffic.Options{DataDir: filepath.Join(dataDir, "traffic"), Source:
router.NewWANSource(routerAdapter)}` to `traffic.New`. Set `dataDir` to a persistent location such as
`/data/be6500panel`. Empty `dataDir` must disable collection explicitly. Never use
`/tmp`, silently fall back to RAM, or create an implicit series per interface.

Call `Start(ctx)` once and `Close()` on shutdown. Both are idempotent; report a
`Close` error. `New` returns filesystem, allocation, sync, or invalid-header
errors. The panel should keep working but expose a disabled/unavailable reason
if startup fails. Do not call `New` repeatedly every HTTP request.

Wire `httpapi.TrafficHistory(w, r, collector)` to the authenticated GET route
`/api/traffic/history`. Nil collector returns the full disabled contract.

## Measurement rules

- Choose exactly the lowest-metric IPv4 default-route interface. Use the
  lowest-metric IPv6 default route only if no IPv4 default route exists. Break
  metric ties by interface name. Never infer WAN from a name or sum a bridge
  and its physical member. Route or counter errors for the chosen source clear
  its baseline. An error in an unrelated route family does not discard a valid
  selected route.
- A first sample is only a baseline. Each later valid same-source counter pair
  contributes its actual `uint64` deltas and elapsed coverage. Split deltas
  across bucket boundaries by cumulative integer time fractions; every delta
  piece sums to its exact original total. Time attribution within that short
  two-second measurement interval is an estimate; the counter total is not.
- Rates are byte totals divided by **observed coverage**, in bytes/second. Peaks
  are the highest two-second counter-derived rate, not an instantaneous peak.
  Coarse buckets sum bytes and coverage and retain the maximum peak. They do
  not sum rate samples or fill missing time with zero traffic.
- Missing reads, missing WAN interfaces, a source change, decreased counters,
  a wall-clock regression, and gaps longer than ten seconds reset the baseline
  and leave uncovered time. A process restart always begins with a new baseline
  even when the new boot's counters already exceed the old counters. A measured
  rate greater than 1 TiB/s is invalid for this device and is not retained.
- The API provides explicit buckets with `coverageSeconds: 0` when no valid
  interval covers them. Partial coverage is also explicit. Chart clients must
  render no-data/partial coverage as a gap, not fabricate an observed zero.

## Resource and retention budget

Three fixed files store one interface-independent WAN history:

| File | Native resolution | Logical slots | Retention | Allocated bytes |
| --- | ---: | ---: | ---: | ---: |
| `wan-30s.ring` | 30 seconds | 5,760 | 48 hours | 737,344 |
| `wan-300s.ring` | 5 minutes | 8,640 | 30 days | 1,105,984 |
| `wan-3600s.ring` | 1 hour | 9,600 | 400 days | 1,228,864 |
| **Total** | | **24,000** | | **3,072,192 (2.93 MiB)** |

Each file has a 64-byte checksummed layout header. Each logical bucket has two
64-byte physical records. Records contain a 32-bit generation, millisecond observed-end offset, UTC bucket start,
receive/transmit byte totals, nanosecond coverage, receive/transmit peaks, CRC32,
and a record marker. No schema sidecar, JSON archive, SQLite, CGO, or external
storage dependency is required. Memory contains 24,000 fixed buckets (under
2 MiB on 64-bit Go) plus fixed dirty flags. Open/recovery temporarily reads at
most one ring (~1.2 MiB); returned queries contain at most 2,000 samples. There
are no per-interface or per-client histories. Route/traffic inputs exceeding
4,096 rows are rejected.

New files are created once with actual zero writes, synced, then renamed and
the directory synced. Creation needs the stated final budget plus filesystem
metadata, not a complete duplicate archive. ENOSPC is returned at startup,
including on sparse-file-capable filesystems. The deployment needs spare flash
space for panel binaries/upgrade staging in addition to this history budget.

All three resolutions receive the same counter deltas at collection time.
Queries use **one** tier that retains the entire requested range; tiers are not
joined or added together. The public ranges are `30m`, `3h`, `6h`, `1d`, `7d`,
`30d`, `180d`, and `1y` (365 days). The long tier retains 400 days, so a one-year
query remains available after recording long enough. Nothing creates a year
of measurements before collection began; `oldestAt` is the oldest still-retained
bucket with actual coverage.

Queries group whole native buckets by integer multiples to meet `maxPoints`
(default 1,500; allowed 1–2,000). Requested edges round outward to native bucket
boundaries, by less than one native bucket. Historical fractions of an already
coarsened bucket cannot be recovered and are not fabricated. The final bucket
can be partial/in-progress. A tier at its exact retention edge can lose the
oldest native bucket as its ring wraps. This does not affect the 365-day query,
which has a 35-day reserve. `resolutionSeconds`, timestamps, and coverage let
clients show these boundaries.

## Writes, loss window, and recovery

The central worker flushes dirty slots and syncs changed rings **once per
minute**, plus a final shutdown flush. Only changed records are written; there
is no whole-history rewrite. A normal minute updates only a few 64-byte slots
per tier. Filesystem/UBI page and journal write amplification is separate from
these logical bytes. Manual `Flush()` exists for tests or explicit shutdown
control; do not call it at the sampling cadence.

A crash can lose up to one minute of unsynced measurements. Starting a new
process also skips its first counter interval (normally up to two seconds),
and last observation boundaries persist at millisecond precision to prevent
re-counting intervals after a backward wall-clock change,
and all downtime is uncovered. `Close()` attempts to save current measurements.
`persistent: true` means persistent storage is enabled and healthy, **not** that
the newest minute of samples has already been saved. `maxUnsyncedSeconds: 60`
states the scheduled flush interval. Optional `lastFlushAt` is the latest
successful dirty-data sync time in the current process; it is omitted until a
flush after restart because observation timestamps are not flush timestamps.
The source name identifies the **current** default route; this single aggregate
WAN history can contain earlier interfaces and must not label all past samples
as belonging to the latest interface. A flush error makes API `persistent: false` and supplies a safe error message;
collection remains bounded and can recover if a later minute flush succeeds.
This is not an undisclosed memory-only fallback.

A flush writes the alternate physical slot. Only a successful sync advances
its committed generation in memory. Recovery accepts checksum-valid records
and takes the newer generation; a torn/corrupt latest record can fall back to
its previous valid slot. If both are invalid, the bucket is missing. A truncated
tail is restored with allocated zeros after valid records are read. The API
reports a recovery warning with `error`, and coverage reflects any lost data.
CRC32 detects accidental corruption, not intentional file tampering. A damaged
header or oversized file produces a startup error instead of silently resetting
or rewriting the archive. Files from incompatible layouts are not migrated
implicitly. Operators must inspect/archive/remove an unusable ring before a
fresh recording can start.

## Verification

Native tests cover exact delta partitioning, rates versus totals, peak
aggregation, explicit gaps, resets, source changes, chosen/unrelated route errors,
clock regressions, 405 days of wrap-around with a retained one-year query,
point/cardinality/disk bounds, cancellation, central collection with no
subscribers, restart loss window, torn/checksum/truncated records, startup errors,
and visible flush errors. Run:

```sh
go test ./internal/traffic ./internal/httpapi
go test -race ./internal/traffic ./internal/httpapi
go vet ./...
CGO_ENABLED=0 GOOS=linux GOARCH=arm GOARM=7 go test -c ./internal/traffic -o /tmp/be6500panel-traffic-armv7.test
```

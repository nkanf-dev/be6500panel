# WAN traffic history storage

`src/product_telemetry.rs` records one real WAN series and preserves
the WANRING disk layout. `product_gateway.rs` owns one telemetry instance;
`server_loop.rs` invokes cooperative ticks on the single native-owner lane,
nominally once per second. Long requests can delay ticks. There is no independent
sampler thread, HTTP client thread or Tokio runtime, and collection does not need
a browser, SSE subscription or chart client.

The WAN source reads raw receive/transmit counters from `/proc/net/dev` and selects
the default route from `/proc/net/route` or IPv6 routes. It does not derive totals
from displayed rate fields or create a series per interface. The same telemetry
module has separate device/core observations; those are not WAN history.

## Enable and integrate

Production `--native-runtime --data-dir` opens the full product. Use a persistent
private data location such as `/data/be6500panel`; rings live in its `traffic/`
subdirectory. Do not use `/tmp` for persistent history. Initialization opens or
admits fixed tiers once, not on every request. Startup/admission failures remain
visible, valid existing tiers remain readable, and no unavailable store is
silently relabeled as persistent RAM history.

`product_gateway.rs` dispatches authenticated GET `/api/traffic/history`.
`Telemetry::tick` performs bounded observation and scheduled dirty flushes;
`Telemetry::close` attempts the final flush. Errors use safe public projections.

The former `internal/traffic`, `router.NewWANSource`, `traffic.Options`,
`Start(ctx)` and `httpapi.TrafficHistory` names are historical/reference-only in
the frozen `mature-integration` tree outside this tree. Project Go source is
retired. Keep all four golden JSON fixtures in `tests/fixtures/`.

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
  measurement interval is an estimate; the counter total is not.
- Rates are byte totals divided by **observed coverage**, in bytes/second. Peaks
  are the highest observed-interval counter-derived rate, not an instantaneous peak.
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
storage dependency is required. The Rust implementation scans bounded record
slabs from disk and keeps at most 128 dirty slots per tier; it does not recreate
the historical Go array of 24,000 in-memory buckets. Returned queries contain at
most 2,000 samples. There are no per-interface or per-client histories.
Route/traffic inputs are bounded; oversized input is rejected.

New files are created once with actual zero writes, synced, then renamed and
the directory synced. Creation needs the stated final budget plus filesystem
metadata, not a complete duplicate archive. ENOSPC is returned at startup,
including on sparse-file-capable filesystems. The deployment needs spare flash
space for panel binaries/upgrade staging in addition to this history budget.

All three resolutions receive the same counter deltas at collection time.
Queries use **one** tier that retains the entire requested range; tiers are not
joined or added together. The public ranges are `30m`, `1h`, `3h`, `6h`, `10h`,
`12h`, `1d`, `3d`, `7d`, `30d`, `180d`, and `1y` (365 days). The long tier retains
400 days, so a one-year query remains available after recording long enough. Nothing creates a year
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

The cooperative owner lane schedules dirty-slot syncs **once per minute** and
attempts a final shutdown flush. A full bounded dirty map can flush sooner. Only
changed records are written; there is no whole-history rewrite. A normal minute
updates only a few 64-byte slots per tier. Filesystem/UBI page and journal write
amplification is separate from these logical bytes. Long owner-lane operations
can delay both observation and the next scheduled flush.

A crash loses measurements since the last successful sync. Sixty seconds is the
scheduled flush interval, not a strict loss bound while the owner lane is busy.
A new process skips its first counter interval, persisted observation boundaries
prevent recounting after a backward clock change, and downtime stays uncovered.
`Telemetry::close` attempts to save current measurements. `persistent: true` means
persistent storage is enabled and healthy, **not** that the newest samples have
already been saved. `maxUnsyncedSeconds: 60` states the scheduled flush interval.
Optional `lastFlushAt` is the latest successful dirty-data sync time in the current process; it is omitted until a
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

Current Rust coverage is in `tests/product_telemetry.rs` and the
module-local telemetry tests. It covers compatible ring layout/readback,
counter totals and gaps, tier/query bounds, persistence/corruption and visible
source/storage failures. The old Go traffic/httpapi test, race, vet and Go ARM
commands are historical reference only, not commands for this retired source.

Use the repository's Rust/Bun-only entry points:

```sh
make test
make armv7
```

All Cargo output shares `.build/rust`, serialized with one build job. A static
ARMv7 build proves target format, not a year of actual router observations. See
`docs/professional-retention-source-map.md` for the historical 405-day simulated
retention record and the separate, design-only grouped record-store proposal.

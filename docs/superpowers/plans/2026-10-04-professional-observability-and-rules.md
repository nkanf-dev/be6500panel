# Professional observability and local rules implementation plan

**Goal:** Clear reliable active status, useful charts and traceable connection/rule history for the single-user professional gateway panel.

**Live invariant:** Household gateway192.168.31.0/24 is active and user accepted. Main7858/gen9 must stay running. Do not withdraw, restart, fault-test, alter applied rules or remove active resources while developing observability/rule features. Immediate fixes deploy frontend-only with the existing exact backend; backend history/rules deploy only through a later non-disruptive qualification.

## Batch 1: Current UI correctness

1. Reproduce misleading withdrawal prompt with actual observation codes. Separate failed READ/busy/canceled request from actual failed cleanup. A read uncertainty never offers a destructive retry as its default. Real persistent off-write/cleanup failures and actual scope mismatch remain actionable. Last confirmed status is labeled as such and cannot authorize a new activation. Tests cover healthy+transient, initial unknown, real cleanup and failed user actions. No network mutation on mount/retry-read.
2. Reproduce blank ConnectionTimeline using live GET metrics, actual chart dimensions/options and console/canvas evidence. Fix the actual rendering issue, not fixture shape guessing or invented durations. Invalid/missing start/age stays unknown; zero is measured zero. Make large active lists useful through bounded visible rows/zoom. Correlate sourceIP with device labels and retain original tuple in tooltip/table.
3. Shared decimal byte scale B/KB/MB/GB/TB, rates /s. Choose one consistent unit per axis/data series from finite observed maximum, retain raw byte values in source/export. Axis/title/tooltips/summary/table show units, sensible precision and unknowns as dash (not zero). All byte charts reuse helper; counts/ms charts keep meaningful units.
4. Source tests, full frontend typecheck/serialized suite/build. GET-only browser acceptance. UI-only no process/backend/network restart. Restore old package on frontend failure; exact backend hash/PID/gen/capture scope unchanged.

## Batch 2: Common history and traceability

Shared time presets:30m,1h,3h,6h,10h,12h,1d(24h),3d,7d,30d,180d,1y. Do not duplicate 24h/day labels. Source resolution/coverage/oldest time must be visible in data details. Empty older coverage is not fabricated.

WAN already retains400days in bounded30s/5m/1h rings. Add missing range names without rewriting old files. Proxy/device/activity/connection charts need their own measured history; a selector does not pretend volatile900-point data covers a year.

Connection records retain original source tuple plus observed device MAC/name, destination IP/host/port, network, actual native rule and outbound, first/last observed times/counters. Disappearance records an observed-ended event, not exact transport close. Short unobserved flows remain explicitly missing. Never infer application/process from domain. Fine detailed records have a bounded storage cap; older intervals aggregate bytes/counts by stable device/target/rule/outbound with tiered resolution. Plan hard storage admission against router /data20MiB; configure external storage/export for longer detailed logs rather than fill flash. Search common device/MAC/IP/name/domain/tuple/rule/outbound/known application metadata. Summaries must not double-count tier overlaps or missing samples.

Before implementation, inventory source granularity and disk/write budgets. Reuse traffic ring/admission patterns, no new generic service/concurrency protocol framework. Query bounded and read-only; no telemetry/raw credentials exposed to browser.

## Batch 3: Independent editable local policy

Keep subscription immutable input and persist local overlay separately. Local rules have stable IDs, enabled, ordered position, matcher kind/value and direct/proxy/block action, label/note and provenance. Rules prepend by default; explicit subscription rewrite/disable references normalized semantic fingerprint (not positional index). Subscription refresh keeps local overlay; unresolved references stay orphaned/inactive and visible rather than matching unrelated new entry. Preview final order + blocked/unreachable/unsupported findings before explicit Apply. Saving drafts never changes active policy; Apply compiles/checks under existing controlled generation guard and verifies readback. No live apply during current network-preserving source work.

Reuse CompileInput.Overrides plus a pure effective-rule merger. Add validated bounded store/API and real UI editor later, not just a low-level compiler field. Keep mandatory management/local bootstrap bypass ahead of user rules. Local editor supports domain/suffix/keyword/IP-CIDR/controlled ruleset/match first; device source/network/ports require exact1.14.2 native matcher support and compile proof before emitting. Process matching on forwarded clients is not offered without a real agent/source. Effective rule map joins native rule indexes to subscription/local stable IDs and labels, so connection tracing explains which layer/rule matched.

## Delivery order and review

Immediate false prompt/blank timeline/units first. Parallel independent lanes only with exact file ownership. Common range/source-budget work and pure local-rule core can proceed in source while current household network remains unchanged. Full history/editor is not called delivered until APIs, UI, persistent storage and real readback pass. Luna handles requirements/evidence notes, Root owns active router writes and deployment decisions.

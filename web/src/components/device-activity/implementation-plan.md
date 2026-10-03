# Device Activity Frontend Implementation Plan

**Goal:** Display real trafficd device byte history with honest gaps, states and derived vendor RX/TX rates.
**Architecture:** A separate validated read-only API feeds one cancellable polling hook. A standalone panel owns range/search controls, metadata and grouped/top-device tables. ActivityHeatmap accepts real device samples and retains only explicitly labelled deterministic byte demo fixtures.
**Tech Stack:** React 19, TypeScript, Effect Schema, existing ECharts, Vitest and Testing Library.

## Scope and sequence

- [x] Add `web/src/lib/device-activity-contracts.ts` and `device-activity-api.ts`. Validate exact memory-only 7-day trafficd contract, finite nonnegative bytes, RFC3339 time, null gaps, 288 points/device, 32 devices and requested range. Test GET URL, search bound, invalid values, zero/null and abort.
- [x] Add `web/src/components/device-activity/use-device-activity.ts`. Read initially and every 30 seconds only while active and visible. Abort on filter changes, hide, disable or unmount. Reject stale completions. Preserve same-query previous data on refresh failures. Test each lifecycle.
- [x] Update `web/src/components/visualizations/ActivityHeatmap.tsx`. Use vendor RX+TX/RX/TX byte cells, actual UTC times, cap 16 device rows, omit missing samples, keep measured zero, and offer accessible detail table. Generate own neutral byte fixtures only in explicit demo mode. Add focused option/table/filter tests and adapt existing demo contract assertions only where needed.
- [x] Add `DeviceActivityPanel.tsx`, scoped `device-activity.css` and `index.ts`. Always show trafficd source. Provide range, 64-character search and read-only refresh. Explain memory retention/restart, partial coverage, stale state and unsupported connection/request metrics. Show grouped rates and top-device table from the same response. Use native keyboard controls and bounded scroll regions at 320/390 pixels. Add focused state/filter/unit tests.
- [x] Run `npm --prefix web test -- src/lib/device-activity-api.test.ts src/components/device-activity src/components/visualizations/ActivityHeatmap.test.tsx src/components/visualizations/visualizations.test.tsx`, then `npm --prefix web run typecheck` and report files and results to parent. Parent owns integration and commits; do not commit.

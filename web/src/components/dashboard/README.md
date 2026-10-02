# Native homepage dashboard

`CustomDashboard` takes `{ navigate: (id: PageId) => void }`. The root overview
entry point can re-export it as `OverviewPage` after the history and telemetry
components are integrated.

The dashboard renders six built-in widgets:

- `systemSummary`: observed system snapshot; errors and last samples stay labeled.
- `trafficHistory`: `TrafficHistoryWidget` from the persistent history feature.
- `environment`: host platform and control-plane mode.
- `devices`: bounded current router devices and ARP observations, not an activity estimate.
- `proxy`: `ProxyTelemetryOverview` from the real telemetry feature.
- `moduleStatus`: actual capability status and navigation shortcuts.

## Layout behavior

This is one named layout per browser origin. No daemon or external dashboard service
is needed. Edit mode previews visibility, order and widths. Native checkboxes,
selects and move buttons work with the keyboard. Focus enters the editor and returns
to the Edit button on Save or Cancel. A live region announces moves and outcomes.

Save writes only the layout name and each widget's ID, visibility and size to
`be6500panel.dashboard.layout`. Cancel does not write. Reset previews the default
layout; Save makes that reset durable. Names must contain 1–64 characters. All
widgets can be hidden; the empty layout retains a recovery action.

A corrupt or blocked read uses the default layout and shows a notice. Reads never
overwrite stored settings. A failed write retains the draft and shows an error.
Removed widget IDs are ignored; added widgets receive defaults. Layout storage
contains no device identities, telemetry, configuration or authentication data.

Sizes are `compact` (1/3), `wide` (2/3), and `full` (whole row). Narrow screens
collapse the grid. Widgets keep their own explicit data sources, empty states,
errors, and demo labels. Demo is not a production fallback.

## Validation

From `web`:

```text
bun run typecheck
bun run test src/components/dashboard --maxWorkers=1 --no-file-parallelism
bun run build
```

History owns `components/traffic-history/TrafficHistoryPanel.tsx`. Telemetry owns
`modules/proxy/telemetry-overview.tsx`. Both exports must be present for integration
validation. This directory does not modify the old overview or the shared API contracts.

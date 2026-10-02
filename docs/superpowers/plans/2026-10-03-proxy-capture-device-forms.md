# Reliable proxy capture and usable configuration forms Implementation Plan

> **For agentic workers:** Use executing-plans to implement and verify each isolated task. Parallel workers own separate worktrees.

**Goal:** Restore real Mac proxy connectivity, retain explicitly selected device capture through core/node changes, and make configuration fields editable without requiring native-file editing.

**Architecture:** Separate persistent desired device scope from live capture ownership. Resolve selected MAC identities using current router observations, compile bounded multi-client owned rules, and restore capture only after the accepted core configuration is ready. Device selection and native UCI field forms use typed API contracts and the existing staging/diff/commit transaction layer.

**Tech Stack:** Go runtime/capture/HTTP API, React/TypeScript/Effect, Bun/Vitest, ARMv7 RN02 deployment.

## Interface contract
- Keep existing single-address capture input readable for compatibility. Add `devices: [{mac: string}]` plus `ipv6` to POST /api/proxy/capture for UI selections.
- Add status `desired: boolean`, `clients: [{mac: string, ip: string, hostname: string}]`, and optional `error`; keep existing status fields and add state/cleanupPending to web decoding.
- GET /api/router adds optional `currentClientIP`, derived from the actual HTTP peer rather than forwarded headers.
- Device selection defaults to the requesting device when its IP matches a current router device; explicit Apply remains required. It never silently captures the entire LAN.
- Core stop withdraws live rules while retaining selected scope; explicit capture DELETE disables saved scope. Node changes, core restarts/recovery and boot rebuild the selected scope from current devices and accepted config. Do not restore arbitrary journal commands.
- IPv6 direct remains default and visible. Follow/block retain existing explicit address requirements; do not claim unsupported IPv6 coverage.

## Task 1: Persistent capture lifecycle and device-scoped backend
**Worktree:** ../be6500panel-worktrees/fix-capture-lifecycle
**Files:** internal/capture/*.go, internal/proxy/firewall*.go, internal/runtime/{types,manager}*.go, internal/httpapi/{proxy_capture,proxy_runtime,router}*.go, cmd/be6500panel/{main,readiness,restore}*.go.
- [ ] Add regression tests: active scope survives node Configure; Stop withdraws but Start restores; failure withdraws and retry restores; fresh manager boot reloads desired scope; explicit DELETE disables; two MAC selections compile exact scopes; DHCP IP change re-resolves without widening scope; unresolved devices stay pending rather than capture a reused stale IP.
- [ ] Persist the desired MAC selection separately from capture-journal.json. Cleanup only removes live resources; restore uses current device observations and actual accepted native config, not stale ports/endpoints.
- [ ] Extend owned compiler/recovery/preflight consistently for multiple exact clients with one owned route per family and one shared chain per family/table. Cleanup remains limited to owned resources.
- [ ] Add request-peer currentClientIP and device status API. Retain single-IP compatibility. Expose suspended/error state when restoration fails rather than reporting a working path.
- [ ] Run `go test ./...` and `go vet ./...`, then commit a focused backend change.

## Task 2: Device checkbox capture UI
**Worktree:** ../be6500panel-worktrees/fix-capture-ui
**Files:** web/src/modules/proxy/capture-panel.tsx, web/src/lib/{contracts,api}.ts, web/src/modules/proxy/production-proxy.test.tsx, related UI tests.
- [ ] Add tests for discovered devices, current-device marker/default, checkbox selection, restoring saved selections, offline/error/empty list, no auto POST, changed selection confirmation, active vs desired/suspended states.
- [ ] Replace required manual IPv4 entry with observed-device checkboxes, showing name/IP/MAC/online state and current-terminal marker. Keep advanced address input only where technically required (IPv6).
- [ ] Submit `{devices: selected.map(mac => ({mac})), ipv6}`. Display desired selection separately from verified active state. Add explicit disable-capture action without making Stop erase selection.
- [ ] Add typed API status/currentClientIP fields and DELETE helper. Preserve other API contracts.
- [ ] Run `cd web && bun run typecheck && bun run test`, then commit a focused frontend change.

## Task 3: User-centred editable native configuration fields
**Worktree:** ../be6500panel-worktrees/fix-field-editors
**Files:** web/src/components/configuration/{NativeEditor,native-document}.*, new focused field schema/form files, configuration.css, configuration tests.
- [ ] Add tests that field changes edit the local buffer, Stage contains changed UCI, lists add/remove, booleans/selects/numbers/passwords work, quotes/comments/unknown fields are preserved, and native/form views round-trip without discarding unrelated values.
- [ ] Make form/field editing the default. Provide module/section-aware labels, hints and controls for network, wireless, DHCP/DNS, firewall, system and SSH. Unknown fields remain editable text rather than read-only rows.
- [ ] Use switches/checkboxes for booleans, selects for protocol/policy/encryption, numeric inputs for limits/ports/timeouts, multi-value editors for lists, masked editable fields for secrets, and text inputs for addresses/names. Preserve existing vendor-specific fields.
- [ ] Keep native text as clearly labelled advanced editing; preserve staging, diff, generation conflict and rollback behavior. Do not require users to open raw config to edit supported fields.
- [ ] Run `cd web && bun run typecheck && bun run test`, then commit a focused configuration-form change.

## Task 4: Actual DNS/data path fix and integration (root)
**Files:** focused proxy/capture/readiness fixes if live evidence identifies additional defects; deployment documentation/tests as needed.
- [ ] Inspect current live capture/runtime/logs and DNS TCP/UDP, kernel counters and bounded packet traces; identify exact failure rather than assume cache.
- [ ] Add regression coverage for any confirmed DNS/return-path defect and fix the owning component.
- [ ] Integrate the isolated commits without overwriting the pre-existing scripts/bootstrap.sh edit. Run `make test`, race-enabled Go tests, `make build`, `make armv7`.
- [ ] Deploy verified panel/UI while preserving private credentials and prior release backup. Apply only the user's Mac device capture initially.
- [ ] Verify fresh DNS over UDP and TCP, ordinary HTTPS Google, local management, a node recommit/restart, and retained device scope. Report what passed and any explicit limitations.

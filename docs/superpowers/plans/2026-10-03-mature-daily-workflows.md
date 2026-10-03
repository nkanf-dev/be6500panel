# Mature Daily Workflows and Connected Charts Implementation Plan

> **For agentic workers:** Use executing-plans in the assigned isolated worktree. Each task has a separate source owner and explicit offline verification. The root integrates shared wiring and runs end-to-end tests.

**Goal:** Build a professional modern gateway control center that surpasses the reference panels through complete daily workflows, detailed device management, reliable recovery and real observability, while preserving the independent LAN rescue service.

**Architecture:** Existing private generations and shared storage admission remain the mutation layer. Add authoritative recovery state, typed readback/preservation for runtime forms, explicit policy-omission acknowledgment, staged maintenance backup, and read-only runtime/device collectors. Diagnostic HTTP traces are opt-in measurements, not fabricated decrypted client traffic.

**Tech Stack:** Go Linux ARMv7, React/TypeScript/Effect/ECharts, Bun/Vitest/Playwright, stock RN02 QSDK procd/ubus/trafficd.

## Task boundaries and common contracts
- Root owns `cmd/be6500panel/main.go`, HTTP `server.go` wiring, shared strings dictionary, final dashboard integration, source/deployment packaging.
- No worker SSH, service reload, capture, factory reset, router reboot, or rescue credential operation. Root may inspect live services read-only.
- Keep per-task new API/contracts modules to avoid overlapping `web/src/lib/api.ts/contracts.ts`. Additions optional where existing clients must remain readable.
- UI/copy authority: `/Users/nkanf/docs/miwifibe6500/reviewed-docs/mature-flows-ux-spec.md`; root forwards chart supplement when delivered. Core-running/accepted/native-rules states never imply full remote health.
- The root excludes `/data/ssh`, password/shadow/private-key files from maintenance archives by explicit backup scope, not general scanning. Backup files can contain private router configuration; default manual external download, not unbounded flash history.

## Task 1 — FRPC form readback and lossless preservation
**Files:** `web/src/modules/frpc*.tsx`, new `frpc-document.ts` and `frpc-session.ts`, focused tests. No Go or shared router/API edits.
- [ ] Add failing tests: reopening saved TOML loads server/transport/mappings; blank token preserves previous token; explicit replace/clear; unknown options survive; page switch retains dirty form; stale generation blocks before deliberate reload/rebase; unsupported document gives clear reason rather than destructive overwrite.
- [ ] Implement supported TOML parsing + source-preserving field patches, with original accepted document kept only in session memory. Never use localStorage for private config/token. Existing native editor remains expert entry, not mandatory routine editing.
- [ ] Add explicit token intent (`preserve`, `replace`, `clear`); preserve credentials by default and redact local preview without erasing actual server data. Preserve untouched sections, comments, service-specific options and mapping fields; change only modeled intent.
- [ ] Follow reviewed dictionary copy. Export `clearFrpcFormSession()` for root logout/401 integration.
- [ ] Run `cd web && bun run typecheck && bun run test --maxWorkers=1 --testTimeout=20000`, commit focused batch.

## Task 2 — Global pending and recovery state
**Files:** `internal/control/{types,manager}.go`, focused tests; configuration status client/schema; new global recovery banner/provider under web configuration. Root mounts it.
- [ ] Add failing tests: expiry failure retains operation ID/phase/actions; reconnect after lost response; terminal state query; retry recovery; cross-page banner/counter/action; never translate missing pending into restored.
- [ ] Add optional `operation` in Status with current last Operation, `phase`, `errorCode`, `canConfirm`, `canRollback`; preserve private document limits and durable journal truth.
- [ ] Global top banner reads this small status independently of configuration page. Show timed provisional state or visible recovery failure; Confirm/Restore remain explicit, not automatically replayed. Keep internal IDs in closed diagnostic details.
- [ ] Keep existing configuration editor controller compatible; phase/error truth shared. Root mounts banner once under authenticated console and wires expired-session cleanup.
- [ ] Run Go tests/race/vet and focused frontend tests, commit.

## Task 3 — Explicit proxy policy acknowledgment
**Files:** `internal/httpapi/proxy_runtime.go`, focused API tests; `web/src/modules/proxy/{node-selector,diagnostics}.tsx` and new policy-review components. Root coordinates possible local DNS updates in the same Go path.
- [ ] Add tests: unsupported rules refuse selection unless matching explicit acknowledgment; known omitted-rule list/count derived from current parsed subscription; changed subscription invalidates old acknowledgment; selection/filter alone never writes.
- [ ] Add parsed policy summary (total, supported, omitted, reasons, revision) to nodes API. Supported rules are labeled eligible/compiled, not runtime hit counts.
- [ ] `proxySelect` input carries explicit acknowledged policy revision; no hardcoded `AcceptUnsupportedRules:true` without actual action. User sees impact summary and checks acknowledgment before Apply. Native compiler still verifies exact supported rules.
- [ ] Tests cover PROCESS-NAME unsupported at gateway and true active-only routing chart labeling. Dictionary-reviewed wording.
- [ ] Run Go and UI tests/vet/typecheck; commit focused batch.

## Task 4 — Configuration backup and staged import
**Files:** new `internal/maintenance` + HTTP helper/tests; new frontend maintenance backup surface. Root mounts/routes.
- [ ] Test backup round-trip scoped six native documents plus accepted runtime settings when explicitly selected; manifest contains model/build/time/scopes/digests. Download never applies, no tar arbitrary members or arbitrary file path endpoint.
- [ ] Support bounded standard JSON envelope using existing current project format, no separate schema migration system. Validate unique module names, known scope, text sizes, model mismatch, missing components, malformed documents. Default preservation of unknown vendor data.
- [ ] Import first yields exact diff summary and diagnostics against current accepted generation. Selecting document groups stages them as one draft operation where dependencies are assessed; final Apply remains the current user-confirmed transaction. No silent restore, reboot, immediate filesystem writes.
- [ ] Private backup download warning readable; original rescue/password/key artifacts excluded. Maintain shared storage budget on staged drafts and no automatic on-router archival growth.
- [ ] Run synthetic tests/typecheck, commit; root native API auth/origin tests mandatory.

## Task 5 — Authoritative procd/runtime state and controlled actions
**Files:** new `internal/router/services.go` + parser/tests, new HTTP helper and frontend system services table. Do not edit existing adapter.go/server.go/main.go.
- [ ] Bounded fixed read-only ubus `service list` snapshot + proc PID/exe/start/RSS verification. Report configured/registered/running/failure/unknown independently; no unknown=stopped or green based only on UCI.
- [ ] Fixture roots never execute host commands; no arbitrary tool/service argv selected by browser. Preserve finite row count/output/time limits and avoid broad whole-router polling at high frequency.
- [ ] Include independent rescue service observed state without reading credentials or enabling/disabling it. Rescue controls must not be exposed as a routine stop action.
- [ ] UI shows source/time/PID/runtime/error, search/filter, stale state and controlled service actions. Implement fixed discovered DDNS start/stop/restart/reload and DNS/DHCP reload/restart with an explicit interruption prompt and post-action readback. Managed proxy/FRPC actions use their existing runtime owner. Protect rescue/network dependencies from accidental generic stop actions.
- [ ] Go tests/race/vet + UI tests; root wires server collection and source capability.

## Task 6 — Real device activity heatmap and charts
**Files:** new device telemetry package/parser/history + API helpers; `ActivityHeatmap.tsx`, new device charts panel/API/hook and tests. Root adds dashboard widget.
- [ ] Inspect existing trafficd ubus schema (root supplies sanitized live evidence). Prefer actual per-MAC byte counters and direction over invented request counts. Fallback association/core-only coverage must explicitly state limited source.
- [ ] Central collector works without browser. Bound device cardinality (e.g.128), buckets, query points and memory; stale/gaps/counter reset/address changes preserve correct identity and units. Do not allocate another multi-MiB per-device yearly flash database without shared budget; recent detailed memory history + coarse fixed persistent rollups may be planned only with measured admission.
- [ ] Heatmap shows device × time traffic/activity, supports scope/time/filter/legend/data table. Genuine no observations = gaps, zero only where measured. Overview/developer demo distinction remains explicit.
- [ ] Device top usage/rate/activity views derive from same counter source. User's requested chart appears in homepage custom-widget registry and devices page, not buried in demo only.
- [ ] Run bounded synthetic/real-response fixture tests, UI tests/typecheck and commit.

## Task 7 — Actual request diagnostic waterfall
**Files:** new request trace package/API helpers; `RequestWaterfall.tsx` plus diagnostic controls and tests. Root routes/mounts under proxy diagnostics and dashboard.
- [ ] Explicit user click starts fixed/bounded HTTP(S) diagnostic trace only. No auto network probe on page load/empty chart. Capture no client traffic and no TLS interception.
- [ ] Measure actual DNS/connect/TLS/first-byte/body timing using native Go HTTP trace for direct path. For proxy path use accepted mixed SOCKS/HTTP listener with correct loopback/LAN scope and preserve certificate verification. Proxy-side DNS not exposed to client tracing is `unknown`, not a fake zero.
- [ ] Bounded target selection from meaningful presets or validated HTTP(S) URL with no credentials/unsafe scheme; count/concurrency/timeout/body cap/redirect policy explicit. No arbitrary network admin writes. Failures retain completed phases and unknown absent phases; cancelled traces visible.
- [ ] Store bounded trace ring with labels/route/status/intervals; UI real phase stacks, actual failure position, filter and table. Connection timeline remains a distinct chart, not claimed HTTP waterfall.
- [ ] Tests fake HTTP/TLS/delayed-firstbyte/failure/cancel/proxy paths and assert no insecure TLS/no automatic probe; UI tests/typecheck.


## Task 8 — Detailed device workspace and editable annotations
**Files:** new `internal/deviceinfo` annotation store + HTTP helpers; new device workspace/API/alias provider; root mounts provider and page.
- [ ] Persist per-MAC display name, note and tags with revision checks and shared storage admission. Explicit Save updates all pages through one annotation provider; retain system hostname separately.
- [ ] Device details combine source-stamped trafficd counters, current leases/addresses, wireless signal/rate/protocol/MLO links, proxy connections/outbound observations and history. Preserve conflict/unknown/stale distinctions; MAC identity cannot be reassigned by a stale IP.
- [ ] Add search, bounded rows, drilldown, multi-device comparison and inline note editing. Heatmap selection opens the same device details, and useful configuration shortcuts open the typed editor.
- [ ] Test annotation persistence/conflict/atomic failures, cross-widget naming, current-IP correlation, multi-link duplicate counters, missing-source behavior and keyboard/mobile interaction.

## Task 9 — Firmware-evidence field help
**Files:** field-schema/help metadata/NativeFields help UI, module-specific evidence catalog/tests, documentation.
- [ ] Inventory every modeled field across the six native modules. Identify actual consumers in init scripts, Lua APIs, config helpers and driver interfaces; record firmware/source path/line.
- [ ] Explain purpose, unit, documented defaults/ranges, dependencies and apply/reload impact adjacent to the control. Detailed evidence sits in expandable help, not a default screen full of file paths.
- [ ] Preserve editable vendor values and the actual parser rules. Derived/internal options receive precise behavior guidance, not arbitrary blanket read-only restrictions.
- [ ] Validate catalog coverage and evidence anchors; root samples live1.0.64 scripts where the baseline1.0.43 evidence needs confirmation. The designated UI/UX lead reviews all descriptions.

## Root integration acceptance
- [ ] Integrate each small verified commit without changing independent rescue or credentials. Shared dictionary and public/runtime schemas reconcile consciously.
- [ ] Full Go/vet/race serialized (`-p1`), frontend typecheck/all tests/build, ARM build. Browser workflows 220nodes/128devices/largeforms/390px keyboard and screenshot review by designated expert.
- [ ] Read-only live API verifies service/device/history/request source capability before writing any config. Deploy panel only with capture desired=false; rescue ports22/2222 remain reachable.
- [ ] No router reboot/capture/risky WAN/WiFi changes without user confirmation. Report named workflow acceptance, measured improvements and the next dedicated adapters to finish. Drive the superiority target through verified task completion and keep reported results grounded in actual measurements.

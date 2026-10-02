# Bounded Node Selector Implementation Plan

> **For agentic workers:** Execute this scoped plan inline with tests and review checkpoints.

**Goal:** Make 150-node subscriptions fast to browse and apply without scrolling the entire list.

**Architecture:** Keep node selection and exact compiler submission in NodeSelector. Use a focused browser-preference helper, bounded 20-card pagination, native keyboard-accessible buttons, and scoped CSS. Save only IDs and view/config preferences, never subscription payloads or credentials.

**Tech Stack:** React 19, TypeScript, native forms, Testing Library, Vitest.

---

### Task 1: Specify behavior with focused tests

**Files:** Create `web/src/modules/proxy/node-selector.test.tsx`.

- [x] Create a synthetic 150-node fixture and an Effect-backed runtime mock.
- [x] Test 20-card default pagination; case-insensitive name, endpoint, and protocol search; combined protocol/transport filters; empty search and clearing.
- [x] Test favorites, missing IDs, remount view retention, active-node jump, hidden selection, stale selection, explicit exact compile inputs, and import/runtime busy guards.
- [x] Run `npm test -- --maxWorkers=1 --testTimeout=20000 src/modules/proxy/node-selector.test.tsx`; confirm new behavior fails before implementation.

### Task 2: Implement browser-only preferences

**Files:** Create `web/src/modules/proxy/node-selector-preferences.ts`.

- [x] Define validated default session preferences for query, protocol, transport, favorites-only, page, selected ID, IPv6, and ports.
- [x] Add guarded browser storage reads/writes and a React state hook. Favorites use a separate localStorage key containing only node IDs.
- [x] Prune missing favorite IDs only when node data is known; never replace a stale selected ID with the current committed ID.

### Task 3: Implement bounded browsing and nearby Apply

**Files:** Modify `web/src/modules/proxy/node-selector.tsx`; create `web/src/modules/proxy/node-selector.css`.

- [x] Put committed and pending selection summaries and the explicit save/apply button before the bounded list in a sticky bar.
- [x] Add labeled search, clear/reset controls, protocol/transport selectors, favorites checkbox, count, and native page controls.
- [x] Render at most 20 compact cards with whole-card selection buttons and add favorite toggle buttons. Preserve selections while filtered. Jump to current committed or pending node by clearing filters and revealing its page, then focus its selection button.
- [x] Keep IPv6 editable and put ports in native details; preserve validation, plain saved/applied result, and exact `proxySelect({ nodeId, ipv6, failure: "direct", ports })` flow.
- [x] Add scoped responsive wrapping and bounded scroll styles without changing global styles or other workers’ files.

### Task 4: Validate and commit

- [x] Run focused tests, `npm run typecheck`, and full `npm test -- --maxWorkers=1 --testTimeout=20000` using root’s existing same-lock node_modules symlink.
- [x] Run Prettier for only changed scoped files and inspect the diff for accidental storage of imported data or network requests.
- [x] Commit the scoped implementation and report the commit hash and results to the parent.

**Refinements from parent/expert:** Region shortcuts match label text only. No native radio/table rows. Accepted native config GET preserves IPv6/custom ports before applying; unsupported bind/routing shapes block Apply. Hashes and technical Commit text are not shown. Dictionary extraction is root-owned after integration.

**Validation:** typecheck passed; focused 21/21 passed; full Vitest 310/310 passed with `--maxWorkers=1 --testTimeout=20000`.

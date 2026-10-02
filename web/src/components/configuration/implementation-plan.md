# FullControl configuration UI implementation plan

**Goal:** Native authenticated UCI editing with draft/diff/validate and an explicit Commit boundary.
**Architecture:** Local Effect schemas/client; abortable shared controller; standalone module editor and all-document workspace. Only `web/src/components/configuration/**` is changed.
**Stack:** React 19, Effect Schema, Radix Dialog, existing UI/theme tokens, Bun/Vitest.

- [x] Add synthetic fetch-mock tests for stage-only behavior, generation conflict, draft diff, risk Commit, pending confirmation, deadline and cancellation.
- [x] Add local `contracts.ts`, `client.ts` and `native-document.ts` for typed endpoints and section-aware native navigation.
- [x] Add `use-configuration.ts` to load authenticated configuration/status/drafts, retain unsaved text, serialize mutations, stage expected generations and reconcile only pending status every 10 seconds.
- [x] Add `ConfigurationEditor.tsx`, `ConfigurationWorkspace.tsx`, shared queue/diff/Commit dialog components, index exports and theme-token-only CSS.
- [x] Run `bun run test -- src/components/configuration`; run `bun run typecheck`; format only owned files; review owned diff and commit owned paths.

Validation uses only synthetic private-native fixtures. No content enters logs, local storage, public demo data, or shell calls. Explicit Commit applies selected validated drafts. Low-risk commits have no extra gate. High-risk Commit gets one concise changed-field/risk dialog. Pending deadline expiration blocks confirmation; status, not the local clock, decides rollback.

**Result:** Synthetic fetch-mock cases, native parser tests, full frontend tests and typecheck pass. No real router configuration enters fixtures.

## Routine section forms follow-up (approved design)

Goal: Add/remove everyday DHCP leases, firewall rules/port forwards, network interfaces/routes and wireless SSIDs without raw text editing. New sections receive a generated internal identifier, hidden in advanced settings. Existing interfaces/radios/zones supply editable reference suggestions. All changes stay in the local buffer until checked and explicitly applied.

- [x] Replace Git/transaction jargon with friendly check/apply/confirm/restore copy; internal IDs and generations stay in advanced details. Verify temporary application and server-confirmed restoration states.
- [x] Add source-preserving section insert/remove helpers and synthetic round-trip tests.
- [x] Add bounded module templates with inline required/address/port/secret validation.
- [x] Add friendly section dialogs and reference suggestions; integrate selection/navigation.
- [x] Test local add/delete, cancel, unknown values/comments, and actual buffer-to-check submission. Run typecheck and serialized frontend tests; commit copy and section changes separately.

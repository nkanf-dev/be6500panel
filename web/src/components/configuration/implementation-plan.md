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

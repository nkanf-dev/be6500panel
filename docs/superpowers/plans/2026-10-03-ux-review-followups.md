# UX review follow-up implementation plan

**Goal:** Fix low-risk user-facing status and recovery gaps found in the 2026-10-03 audit. Do not modify router state or claim missing adapters exist.

**Boundary:** This isolated worktree owns shell.tsx, httpapi/modules.go, plan-view.tsx, proxy.tsx, a proxy prerequisite component, and runtime/native-config-editor.tsx. Other workers own overview/history, metrics, capture/device APIs, and configuration forms.

1. Write focused React and Go tests for unknown/control shell copy, deployed runtime capabilities, planned (not completed) steps, proxy prerequisites and read retry, and guarded runtime config reload.
2. Run those tests against the baseline. Confirm each fails on the target behavior.
3. Replace Full control with configuration-specific copy. Unknown health must remain unknown.
4. In ModuleList only, replace obsolete runtime-not-integrated claims when a runtime manager exists. Keep preview plans non-applicable and router capabilities unchanged.
5. Replace green completed plan ticks with ordered proposed steps and explicit no-change copy.
6. Add a prerequisite/recovery panel to proxy nodes. Link to runtime/start and optional single-client capture. Disable tab shortcuts during mutations. Display runtime read errors with retry. Do not enable new operations.
7. Guard dirty runtime reload/new config with explicit discard/cancel. Never fetch or overwrite before confirmation.
8. Run focused tests, Go test/vet, Bun typecheck/test/build, and demo-only Playwright. Record environment failures separately.
9. Finish docs/ux-review-2026-10-03.md with paths, severity, verified implemented/missing boundaries, fixed items, and remaining backlog. Commit tested source and audit in coherent batches.

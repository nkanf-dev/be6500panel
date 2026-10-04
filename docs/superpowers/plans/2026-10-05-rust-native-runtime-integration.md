# Rust native runtime integration

## Goal and boundaries

Finish the missing native glue, not a new framework. One manager, one build directory, one costly operation at a time. No Go commands, new test VM, production core actions or firewall operations during implementation.

## Deliverables

1. A restricted capture executor accepting only exact argv from a recompiled owned plan. Trusted executable paths, clean environment, bounded combined output, deadline/cancel and exact child-group cleanup. No arbitrary shell/argv endpoint. Tests use fake executable fixtures.
2. Native manager prestart/readiness callbacks using the accepted config path/hash and retained process status. Bind actual artifact inode metadata and process starttime before/after TUN/DNS observation. FRPC process readiness must not imply tunnel connectivity. Capture cleanup and restore remain explicit owner callbacks.
3. Single root-owned serialized Cargo validation and ARMv7 check. Preserve current Apply refusal until actual kernel preflight, artifact admission and controller wiring are qualified. Report this as source integration, not production migration.

## Work order

- [ ] Implement/test the restricted executor in an independent clean source worktree without starting builds.
- [ ] Add retained status/artifact context to manager hooks and implement native readiness glue; no new daemon or provider protocol.
- [ ] Commit owned source/tests, integrate, run one full native test/fmt/clippy pass using `.build/rust`, jobs1, incremental=false.
- [ ] Cross-check ARMv7 using the same target tree after host gates settle. No parallel heavy build.
- [ ] Remove completed task worktrees and unnecessary debug artifacts. Keep source, fixtures and reports.

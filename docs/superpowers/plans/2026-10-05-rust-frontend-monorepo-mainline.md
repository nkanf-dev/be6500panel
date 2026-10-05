# Rust Frontend MonoRepo Mainline Implementation Plan

**Goal:** Make the current production Rust backend and existing frontend the main project, without a nested Rust replacement or retired Go implementation.

**Architecture:** Root `Cargo.toml`, `Cargo.lock`, `src/`, `tests/`, and `examples/` contain the one Rust backend. `web/` contains the frontend. Both share Makefile and CI; one authenticated manager and native sing-box/FRPC remain on the router.

**Tech Stack:** Rust 1.93, React/Effect, Bun, ARMv7 musl, existing bounded synchronous owner.

- [x] Move Cargo manifest/lock/source/tests/examples from `.` to the root. Remove the old `rust/` tree and nested README/ignore.
- [x] Remove project Go roots, unused Go-dependent browser demos, transition examples, optional Go-reference fixture adapters and dated migration-only documentation. Retain product regression fixtures under language-neutral names. Preserve factory/native configuration and user data; do not add a second backend or compatibility proxy.
- [x] Update Makefile, CI, root README, deployment scripts and active docs to root Cargo commands. Keep the single shared `.build/rust` output.
- [x] The unfinished history patch was reverted to keep this delivery focused. Validate the canonical root sources and retained product fixture data. Run full root Rust tests/fmt/clippy/ARM, frontend tests/typecheck/build, bootstrap syntax and fixture browser suites serially.
- [ ] Commit the complete root MonoRepo. Fast-forward clean main; push main. No forced reset or rewritten history.
- [ ] Publish the mainline native release, verify same production config/drafts/history/core/capture-off and visible UI/operations, then record final same-device efficiency measurements with failures and limitations.

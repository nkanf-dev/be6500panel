# Rust Authentication and Local Policy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Migrate session security and the pure local rule overlay into Rust with contract fixtures before implementing device writes or ownership handover.

**Architecture:** Independent small modules, tested from copied/native fixtures. One Rust service integrates authentication; pure policy logic is a library module with no I/O. Keep current production owners and paused Go entry unchanged.

**Tech Stack:** Rust std, focused serde/serde_json/sha2/getrandom crates where necessary. No Tokio, HTTP framework, C application code, async runtime or daemon/RPC layer.

---

## Task 1: pure policy library (independent worker)

**Files:** create `rust/panel/src/policy.rs`, `rust/panel/tests/policy.rs`; modify Cargo.toml/Cargo.lock and lib.rs only to export this module and the focused parsing/hash dependencies.

- [ ] Implement typed existing JSON shape for Rule, LocalRule, SubscriptionEdit, Policy, Diagnostic, EffectiveRuleIdentity and EffectivePolicy. All shape/name/action tokens match `internal/proxy/local_rules.go` and `types.go`. `TargetBlock` is `block`, not `reject`.
- [ ] Test first exact bounds (512 local/1024 edits,64-byte ASCII stable IDs,64-rune labels,256-rune notes,253-byte matcher values), unsupported process matchers, invalid/no-resolve combinations, invalid controls, CIDR canonicalization and controlled rule-set names.
- [ ] Test first local precedence, disabled retention, subscription exact replace/disable by semantic SHA256+occurrence, orphan inactivity, terminal MATCH diagnostics and stable provenance/source indexes. Preview EffectiveIndex is not a native rule index.
- [ ] Port existing private rule validation and normalized semantic hashes exactly, including Go JSON escaping for `<`, `>`, `&`, U+2028/U+2029 when serializing hash input. Never silently normalize labels/IDs or lose unsupported diagnostic meanings.
- [ ] Public APIs: `validate_policy(&Policy)`, `policy_revision(&Policy)`, `subscription_fingerprints(&[Rule])`, `merge_effective_policy(&[Rule], &Policy)`; no storage/network/compiler/runtime operations.
- [ ] `cargo test`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, release build. Commit only owned files. Root generates differential fixtures via original Go environment and integrates them separately.

## Task 2: authenticated bounded HTTP (independent worker)

**Files:** create `rust/panel/src/auth.rs`, `rust/panel/tests/auth.rs`, extend existing http/server/main/tests; modify Cargo.toml/Cargo.lock/lib.rs for focused security/parser dependencies.

- [ ] Test first session/login/logout exact JSON shapes, authentication of non-session APIs, fixed errors and no private password/token logs. Reuse cookie name `be6500panel_session`, hashed random token storage, bounded TTL/attempt limits from `internal/httpapi/auth.go`.
- [ ] Support bounded POST for login/logout without ambiguous framing. Read each body only after validated Content-Length and under the original absolute request deadline; ordinary bodies <=64 KiB, login body <=4 KiB. Reject duplicate/unknown JSON fields, Transfer-Encoding and invalid Content-Type.
- [ ] Preserve same-origin unsafe request checks, including FRPC-style Host/Origin regression fixtures. Do not trust X-Forwarded-Proto or a browser-selected origin blindly. Numeric loopback remains the accepted listen binding for this pre-parity engineering slice; no LAN/remote deployment yet.
- [ ] Use OS cryptographic entropy, SHA256 and constant-time password comparison; no custom RNG or ad hoc hashing. Keep session maps and attempt keys bounded. Clock/entropy seams for tests are narrow module injection, not a provider framework.
- [ ] Read `BE6500PANEL_PASSWORD` only from environment at startup. Zero unauthenticated success claims for protected APIs; no persistent secret file, no password in flags or logs. Keep no-password mode explicitly loopback diagnostic and report authRequired=false.
- [ ] `/api/session`, `/api/session/login`, `/api/session/logout` match current frontend contracts. `/api/health`/memory are protected when configured, static login assets remain public. Existing diagnostic/malformed/stream/deadline tests still pass.
- [ ] Run complete native cargo checks and release build. Commit only owned files, report actual scope. Root integrates Cargo/lib overlaps and cross-builds afterward.

## Task 3: root integration and compatibility evidence

- [ ] Generate bounded policy golden fixtures by executing original Go functions with fake/public test rules, not importing project code into the orchestration kernel. No router source credentials or live policy changes.
- [ ] Compare Rust validation, revision, semantic occurrence fingerprints, effective rule order/provenance/diagnostics against exact Go output. Run Unicode/HTML escaping/CIDR/disabled/orphan/terminal cases.
- [ ] Combine auth and policy source commits, resolve only Cargo/lib overlaps, run complete native checks and both ARM release builds. Benchmark authenticated one-browser loads and maximum bounded policy documents before storage/Apply integration.
- [ ] Do not claim writable rules, native compiler parity or production replacement until those later slices are actually implemented and tested. No production owner takeover, capture enable or Go-front restoration in this plan.

# Rust Owner Composition Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Compose fixed runtime storage and owned child lifetimes with retained cleanup/readiness semantics in isolated tests; prepare capture journal ownership without executing router commands.

**Architecture:** One future Rust manager owns one RuntimeStore and fixed sing-box/FRPC ProcessOwner handles. A separate in-process capture controller persists explicit desired intent and an internally compiled routed-TUN cleanup journal. These are library components, not new daemons or a generic RPC/scheduler framework. Runtime HTTP and production handover remain unavailable until real readiness, artifact admission, capture observation and full integration are qualified.

**Tech Stack:** Existing Rust std/serde/serde_json/sha2/libc. Native fake-process/fake-runner tests and temporary fixture files; no router operations.

---

## Task 1: capture desired state and retained cleanup journal

**Files:** create rust/panel/src/capture_state.rs and tests/capture_state.rs; lib.rs export. If necessary, add only Deserialize/Serialize field adapters to capture_plan.rs, without changing plan semantics.

- [ ] Keep capture-desired.json and capture-journal.json separate. Desired uses existing scope/lanIPv4Prefixes/desired/devices/clientIPv4/clientIPv6/ipv6 fields. Device identity and gateway declaration are user intent, not stored listener or kernel command authority. Preserve disabled declaration and current off state on load.
- [ ] Constructor only reads bounded no-follow regular private files. Missing state is off; corrupt or unsupported state is an error, not an empty reset. A journal is staged cleanup ownership, never installed proof. Reconstruct from its bounded input/ownership and compare regenerated ownership and cleanup; never replay stored Apply or run caller-provided command vectors.
- [ ] Generate plans only through capture_plan::plan_owned_rules. Admit complete journal/desired temporary bytes before changes, retain >=1 MiB free headroom and old emergency data. Atomically fsync/rename/dir-fsync. Unknown or old TPROXY journals are refused and retained for the later narrowly scoped legacy withdrawal migration; do not add TPROXY generation.
- [ ] Use a narrow testable command callback receiving exact internal argv and finite deadline. No production command executor is wired in this slice. Apply requires caller preflight before journal save; persist journal before first command. Stop on first Apply error, then attempt ALL cleanup commands under an independent bounded deadline. Retain journal/ownership whenever cleanup or journal removal durability is uncertain.
- [ ] Disable latches desired=false in memory before any persistence attempt. Best-effort owned cleanup still runs if off-state persistence fails; mark disable-not-persisted truthfully. Cleanup callback failure must propagate to runtime stop so no TERM/KILL occurs before withdrawal succeeds.
- [ ] Do not turn busy/read-error/unknown observation into cleanup failure or active success. No GET auto-enable, no fake installed counters, no legacy arbitrary command execution. Status is explicit staged/active-by-success/cleanup-pending/off; actual future observed resource verification remains separate.
- [ ] Tests: constructor zero commands; compatible off gateway/device input; private modes; malformed/duplicate/oversized/FIFO/symlink refusal; regenerated journal rejects tampered Cleanup/Ownership but ignores untrusted stored Apply; preflight before persist/execute; journal before first Apply; Apply partial failure all cleanup; cleanup failure retains; disable persistence failure still cleanup with off latch; no command outside compiled intent. Owned source commit and native gates. No device/kernel tests.

## Task 2: fixed-service runtime manager composition

**Files:** create rust/panel/src/runtime_manager.rs and tests/runtime_manager.rs; lib.rs export. Targeted narrow runtime_store/process adapters only if necessary; no HTTP/main/capture execution.

- [ ] Build Manager from an exclusive RuntimeStore and fixed trusted artifact bindings. Constructor does not start or adopt any persisted PID, prune configurations, read capture commands, or run cleanup. Artifacts bind fixed service/root/path/SHA256 provenance; no arbitrary argv, URL fetch or shell API.
- [ ] Map the two ServiceId enums explicitly. Keep one Run and at most one finite Check per service; no extra permanent process owner or generic provider abstraction. Status/config remain typed and private-safe.
- [ ] Configure requires current accepted generation, validates a candidate through the real fixed verifier before disturbing a live Run, and uses exact-byte VerificationProof only after verifier success. Refusal or verifier failure preserves running PID and accepted state. No stale generation commits or false readiness from a successful checker.
- [ ] For a live replacement: cleanup callback succeeds before old Run termination; failure retains live child and accepted state/candidate ownership for retry. Start new accepted config with native prestart/readiness hooks, then mark ready and restore resources. The hooks are narrow internal seams for tests and future native observations, not HTTP-provided success flags.
- [ ] If a checked replacement fails readiness/start, stop it only after cleanup and recover the frozen proven-ready previous config/artifact. Recovery uses exact-byte hashes and appropriate new monotonic generation/readback. Keep restored/needsRecovery truth distinct; report uncertainty on postcommit dir-sync failure and do not delete snapshots.
- [ ] Respect desired=false on startup and stop. Explicit start/stop/restart/restore APIs are fixed-service operations only. Do not wire an unimplemented action to the browser. Close performs cleanup before signaling; failed cleanup leaves explicit ownership live rather than reporting shutdown success. No Drop cleanup bypass.
- [ ] Tests use fake local executables only: actual checker success/failure/mutated candidate; Run persists during checker; cleanup failure before stop; readiness and restore order; new-ready/lastGood proof; generation conflict; failure recovers previous artifact/config; restore refuses unready; bounded timeout/cancel; no output/password leak; close refuses cleanup failure and can retry. Constructor acquires lock but executes zero process/kernel actions.
- [ ] Full native tests/fmt/clippy/release; commit owned files. Root cross-build and Linux fake-process qualification. macOS tests do not prove Linux PDEATHSIG or production readiness.

## Root integration and release boundary

- [ ] Merge source components after actual commits. Run existing policy/compiler/subscription/capture reference tests and all fake recovery tests. Resolve exports narrowly.
- [ ] Cross-build Linux ARMv7, plus build tests/fake helpers through the target environment when available. Do not execute these lifecycle helpers on the household router in this source-only composition phase.
- [ ] Before production actions, port actual owned readiness/prestart, capture preflight/current-observation and bounded argv executor; add artifact admission and full status/config/API integration. Still-unported UCI/FRPC editing/history/remote management and safety semantics block deletion of legacy production owner.
- [ ] No Go-front restoration, capture enable, real core stop/restart, firewall command or production owner cutover is allowed by this plan. Final topology remains one Rust management owner; delete project-owned Go and migration glue only after full accepted handover and data migration.

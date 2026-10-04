# Rust Rule Storage and Native Compiler Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the private persistence and native compilation gaps for Rust rule management without taking over the live router.

**Architecture:** A small private draft store and a pure native compiler are independent Rust library modules. Generate compiler reference cases through the existing Go project and compare exact configuration semantics. HTTP wiring and controlled runtime ownership remain a later slice; Save and Compile never imply Apply.

**Tech Stack:** Rust std plus existing serde/serde_json/sha2. Focused libc bindings are allowed for Unix no-follow and statvfs calls, not application C code. No async runtime, web framework, new daemon or generic provider/scheduler framework.

---

## Task 1: independent bounded private draft store

**Files:** create `rust/panel/src/policy_store.rs`, `rust/panel/tests/policy_store.rs`; modify module exports and necessary Cargo files. Changes to policy structs are limited to strict serde field attributes if needed; keep public policy semantics unchanged.

- [ ] Test first missing file as empty overlay, save/reopen same bytes/semantic revision, malformed/unknown/duplicate/null input rejection, 256 KiB document cap and 0600 file/0700 directory modes.
- [ ] Use existing `local-proxy-rules.json` JSON shape and `policy_revision` from the pure module. Do not read native runtime config or subscriptions. Snapshot and save responses own their data.
- [ ] Open the configured private directory without following a final symlink, retain its device/inode identity, and refuse replacement. Document reads reject symlink/nonregular files and use O_NOFOLLOW/O_NONBLOCK with a 256 KiB+1 read cap. Reject duplicate/unknown fields through typed parsing without an unbounded whole-document Value cache; require rules/subscriptionEdits arrays, not null.
- [ ] Validate and bound before writes. Charge the complete temporary document rounded to allocation units, retain at least 1 MiB measured free space and the existing emergency reserve. No reserve creation or reclaim in the store. Fail closed if space cannot be measured.
- [ ] Create a unique private file in the same directory, write bounded chunks, fsync, atomic rename, directory fsync. Precommit failures retain old data and remove temporary files. A post-rename directory-sync failure must return a committed snapshot plus an error, never false old state. Expose this distinction explicitly in `SaveOutcome`.
- [ ] Fixed public error text, no path or rejected policy values in error/Debug output. Tests use temporary directories and source-only failure injection; no persistence to the router or shared production data.
- [ ] Run native Cargo tests/fmt/clippy/release. Commit only owned files and report actual API/limits.

## Task 2: pure native sing-box compiler

**Files:** create `rust/panel/src/native.rs`, `rust/panel/tests/native.rs`; export module and adjust only necessary Cargo dependencies. Do not edit store/auth/HTTP modules.

- [ ] Port private input types and validation from `internal/proxy/{types,subscription,native,native_routed_tun}.go`. Use routed-TUN as the only emitted backend, IPv6 direct and fail-direct. Reject retired TPROXY and unsupported policies rather than guessing.
- [ ] Preserve compiler defaults, exact mandatory DNS/bootstrap/management/private/IPv6/sniff prelude, ordered overrides+rules, terminal MATCH behavior, direct/proxy/block actions, controlled rule-set metadata, real local dnsmasq authority and required feature list.
- [ ] Compiler does no DNS resolution, network, file read/verification, process spawn or kernel write. Rule-set metadata validation is pure; actual asset checksum verification belongs to later bounded I/O ownership. Input nodes/config contain synthetic or copied private credentials; Debug and errors must not expose them.
- [ ] Cap emitted private configuration at 512 KiB, matching the current runtime owner. Reject oversize input/output with fixed safe errors and keep temporary construction/serialization bounded; do not allocate a complete oversized encoding before checking. Pathological Go-pure-compiler inputs above the runtime cap are an intentional resource refusal, not a parity success. Return private native config bytes, SHA256 and safe diagnostics. Hash exactly emitted bytes. Match Go `MarshalIndent` key order and escaping for reference byte parity if practical; if serialization differs, prove parsed configuration semantics identical and label hashes as implementation-local, not interchangeable accepted runtime hashes.
- [ ] Test defaults/custom TUN, address/listener collisions, local DNS aliases and PTR/AAAA rules, all matchers and actions, noResolve, terminal fallback, fake-IP DNS behavior, omitted-rule acknowledgment, endpoint bypass, custom DNS and credentials/REALITY/uTLS preservation. Do not downgrade UDP/QUIC or transport fields.
- [ ] Run native Cargo tests/fmt/clippy/release; commit only owned files and report APIs and parity limits.

## Task 3: exact Go reference fixture generation

**Files:** create `internal/proxy/rust_native_golden_test.go` and `rust/panel/tests/fixtures/native-go.json` in an independent source worktree.

- [ ] Invoke Go `CompileNative` using explicit public synthetic input DTOs, not JSON of credential-redacted CompileInput. Cases include normal defaults, custom TUN/ports/DNS, ordered direct+block rules, supported controlled sets, terminal fallback/noResolve, fake-IP and fixed invalid inputs.
- [ ] Output exact generated config string, actual SHA256, safe diagnostics and metadata. Generator writes only for an explicit output-path environment; ordinary Go tests must not mutate fixtures. Repeated output is byte-identical.
- [ ] Run project-native Go tests/vet and read-only comparison. Commit exactly generator+fixture. Root adds the permanent Rust differential test after compiler integration.

## Root integration and next gate

- [ ] Resolve module/Cargo overlap only; run all Rust tests including existing session security and Go policy parity.
- [ ] Run permanent compiler reference comparisons and private store recovery tests on host temporary directories.
- [ ] Build ARMv7 and measure maximum bounded policy store/compile operations separately from the earlier authenticated diagnostics. Linked but unused code is not an operation-memory benchmark.
- [ ] Wire authenticated rule CRUD/preview only after store/compiler parity passes. Runtime Apply and owner handover remain unimplemented until their safety batch is ported. No production takeover, live rule updates, capture enable or old Go-front restoration in this plan.

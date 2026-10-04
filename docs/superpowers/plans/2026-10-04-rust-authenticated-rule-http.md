# Rust Authenticated Rule HTTP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Serve the existing local-rule editor's authenticated read/save/preview contract from Rust and actual bounded subscription data, without claiming runtime Apply or production-owner parity.

**Architecture:** One Rust service, one optional private data directory and one static tree. A pure bounded subscription parser produces typed private nodes/public rules; a rules state handler owns one private store. Session security gates all rule APIs. No old Go reverse proxy, duplicated daemon, upstream credentials, runtime process control or auto-Apply.

**Tech Stack:** Rust std plus existing serde/serde_json/sha2 and serde-saphyr deserialize-only if its budgets pass native tests. No Tokio/framework or C application source.

---

## Shared module contracts

`subscription::Subscription { nodes: Vec<native::Node>, rules: Vec<policy::Rule>, diagnostics: Vec<policy::Diagnostic>, group_count: usize, fake_ip: bool }`.
`subscription::parse_clash_yaml(&[u8]) -> Result<Subscription, SubscriptionError>`; private Debug, no Serialize on private nodes. `Subscription::public_nodes()` serializes existing id/label/server/port/protocol/transport/reality/vision/utls/udp contract.
`subscription::summarize_policy(&Subscription) -> PolicySummary` ports total/supported/omitted/reasons/omittedRules/revision from Go exactly. Fixed-safe diagnostics, no original YAML in errors. New pure parser does not read files, resolve nodes or configure services.
`rules_http::RulesState::open(data_dir)` reads bounded private subscription.yaml (missing = empty subscription for independent local draft), owns one Store. It must refuse a corrupt existing subscription, not silently make the current overlay appear empty. Only Load/Save of independent draft are implemented.

## Task 1: bounded subscription parser and summary

**Files:** create rust/panel/src/subscription.rs, tests/subscription.rs; export module and add deserialize-only YAML dependency. Only native validation visibility may change to reuse it.

- [ ] Test before code: real supported VLESS/REALITY/Vision/Chrome-uTLS fake nodes, stable node IDs, direct-only and reject-only nested groups, mixed/cyclic selectors not guessed, supported ordered matchers and indexed fixed omissions.
- [ ] Read source <=2 MiB; max2048 nodes/8192 rules/128 groups, depth32, per scalar8192, global events/nodes bounded. Reject aliases, anchors, merge keys, duplicate/nonstring keys and multiple documents. Disable comment retention, inclusion and property substitution.
- [ ] Use parser-native event/input/depth/anchor budgets rather than a whole-document unbounded Value AST. Allow only necessary YAML deserialization features. Unknown ignored options must remain bounded and syntax-checked.
- [ ] Preserve typed Boolean semantics and scalar spelling important to Go behavior. Unsupported private node options produce safe indexed diagnostics, not guessed transports. Keep unknown rule types and process rules as actual omissions.
- [ ] Match source IDs (first8 bytes SHA256 of server:port NUL uuid NUL name), exact Rule.Index and Go policy summary hash/escaping/order, with no credential-containing Debug.
- [ ] Full cargo test/fmt/clippy/release and exact Go-reference fixtures before source integration. Intentional resource refusals may be stricter than legacy but must be explicit, not silent policy changes.

## Task 2: independent Go subscription fixture generator

**Files:** internal/proxy/rust_subscription_golden_test.go and rust/panel/tests/fixtures/subscription-go.json.

- [ ] Fake-only YAML inputs; actual ParseClashYAML output through explicit private-node DTO plus public node/rule/diagnostic/group/fake-IP metadata. Generator writes only with explicit output env; comparison is read-only; twice-generated output identical.
- [ ] Cover ordinary nodes/groups/rules, duplicate/unsupported node diagnostics, uniform direct/reject nested groups, mixed/cyclic/unknown selectors, process/unsupported rules, fake-IP, quoted YAML scalars and strict type failures, anchors/duplicates/multidoc/limits.
- [ ] Go project tests/vet and owned two-file source commit. Root's permanent Rust test compares validity, private synthetic fields, public nodes and rule/diagnostic order. Private fixture content is synthetic and never production data.

## Task 3: authenticated rules HTTP vertical slice

**Files:** create rust/panel/src/rules_http.rs, tests/rules_http.rs; targeted server/http/main/lib changes and tests. No runtime/core/capture/compiler ownership code.

- [ ] Test first GET /api/proxy/local-rules, POST same endpoint {policy}, POST /preview {policy}; body cap256KiB and absolute request deadline, exact unique typed JSON fields, arrays required, authenticated same-origin before mutation. Ordinary login cap4KiB remains.
- [ ] Return real draft+revision, subscription rules with exact fingerprints, merged provenance/diagnostics and policySummary. `applied.state=unknown`, runtimeGeneration=0 are explicit because this slice has no runtime owner. Do not interpret stale applied file as current acceptance. GET /api/proxy/nodes returns actual parsed public nodes, safe diagnostics, selectedNodeId empty without owned runtime proof, stable subscription view revision.
- [ ] Preview does no store save/fsync or core activity. Save uses existing SaveOutcome and fresh snapshot; postrename durability failure reports committed revision and uncertainty, never old-state success. Invalid input/precommit error keeps current draft. Constructor/reads never auto-save/apply.
- [ ] POST /apply and /select explicitly refuse with fixed runtime_unavailable; no body/native config writes or false success. Do not expose compiled config or node credentials over these endpoints. The general HTTP loopback-only restriction stays until complete security/bootstrap/ownership parity.
- [ ] Add optional --data-dir. It opens only subscription/draft state, never manager.lock, capture files, config-N, executable or kernels. Existing diagnostic no-data mode still works. Health readOnly reflects supported managed draft writes, not incomplete production readiness.
- [ ] Serialize typed response fields without cloning a full subscription/Value tree; use bounded <=8MiB response accounting and byte-safe framing. Large output must fail closed, not return truncated valid-looking policy. HEAD has exact declared length and no body. No whole-file asset copy.
- [ ] RulesState dependency is integrated only after parser/store source is available. Reuse stable shared state, no replacement per GET or generic IPC layer. Full native checks, frontend contract decoding, real-host browser save/readback/preview and resource tests before claiming this slice usable.

## Root fan-in and qualification

- [ ] Merge parser and exact fixture sources, add permanent comparisons, then integrate HTTP changes and run all previous84 tests plus new tests.
- [ ] Build ARMv7 and measure actual HTTP read/save/preview at representative/max bounded source sizes. Source-only fixtures and isolated temporary scratch; no production credentials/configs/subscriptions or live owner handover.
- [ ] Reuse the existing rule UI in an isolated host entry for browser validation. Do not pretend the rest of the complete dashboard/runtime API is implemented. Runtime Apply, subscription imports/downloads, native checks/readiness and exclusive-owner handover remain next safety batch.

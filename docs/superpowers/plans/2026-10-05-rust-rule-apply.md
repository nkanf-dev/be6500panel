# Fixed-owner Rust rule Apply

## Scope

Close the existing local-rule editor Apply path, not another management layer. Reuse RuntimeHttp's actual Manager and the existing private Store, PreparedSubscription and native compiler. The default main still has no production runtime owner; no device activation occurs.

## Implementation

- [ ] Derive native compile intent from actual accepted routed-TUN config and match the selected subscription node by all credential/transport fields. Do not pick an arbitrary first node or trust a label/disk selection alone.
- [ ] Preserve accepted listener/TUN/DNS/dialer/log/telemetry settings while replacing only compiled policy rule lists. Verify pinned controlled SRS files with bounded reads. New generated config cap remains512KiB; oversized legacy accepted reads remain available, but Apply may explicitly refuse preserving a config above the new write budget.
- [ ] Authenticated Apply receives only draft revision, runtime generation and omission acknowledgement. Save/preview do not Apply. Generation/revision/ack mismatches refuse before checker or cleanup. Compile same effective policy and feed the actual fixed native verifier/manager.
- [ ] After manager success, accepted byte SHA and generation/readiness must match before persisting known-applied metadata. Metadata persistence failure reports current truth and uncertainty, never old-state or false success. GET shows draft and applied independently.
- [ ] Tests use synthetic config/node data, temporary SRS fixtures and fake native executables. Correctness includes credential mismatch, preservation, checker/cleanup failure, rollback, accepted-readback and no-owner refusal. Root serial tests/fmt/clippy and ARM build only in one target tree. No Go build, production core/capture write, new VM or duplicate assets.

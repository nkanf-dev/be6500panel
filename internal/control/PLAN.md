# RN02 configuration transaction implementation plan

Goal: Private native UCI drafts with a single explicit Commit boundary and durable rollback.
Architecture: one manager mutation gate, a bounded private disk store, isolated fixed-argv validation, and one retained transaction journal. Restart rolls back incomplete transactions before accepting changes. Raw document text and secrets never enter logs.
Tech stack: Go standard library.

1. Define exported request/result/options types and typed errors in types.go.
2. Implement native quote-aware UCI parsing, domain validation and risk classification in validate.go. Validate candidates with isolated `uci -c DIR show MODULE`.
3. Implement bounded private atomic disk helpers and journal recovery in store.go.
4. Implement manager lifecycle, draft APIs, generation CAS, fixed reloads, commit/confirm/rollback and timeout worker in manager.go.
5. Add synthetic root fixtures and injected runner tests: stage isolation, invalid candidates, conflict, risk acknowledgement, reload/verify rollback, timeout/restart and Close cancellation.
6. Run gofmt, package/full tests, race and vet. Document API and commit internal/control only.

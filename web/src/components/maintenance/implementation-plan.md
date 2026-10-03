Implementation plan (maintenance-only new files):

1. Add scoped backup envelope/preview/stage Effect schemas and native/runtime scope labels.
2. Add custom raw upload/download helpers retaining ApiError and 401 events. Stage uses the maintenance-owned Effect/Schema boundary so cleanup failure retainedDraftIds survive; runRequest executes every request.
3. Add accessible scope selection/download panel and upload preview modal. Raw bytes validated before decode; no JSON parsing/stringification. Native selection, model acknowledgement, generation, expiry, errors/dependencies/risks all visible. Stage only then queue navigation; partial cleanup error links queue.
4. Keep request state transient. Abort unmount/logout/401 and clear local previews, with no browser persistence. Canceling or replacing a preview sends DELETE for its ID. Backend root service.Clear owns session-wide server cleanup; expiry remains a fallback. Clear file input immediately on pick.
5. Add API and UI tests for byte preservation/limits, scopes, warnings/diffs/model mismatch, native-only stage, conflicts/partial cleanup, cancellation/session, no auto-apply/storage. bun focused maxWorkers=1 then typecheck.

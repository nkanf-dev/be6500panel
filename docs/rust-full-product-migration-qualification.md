# Complete native product migration qualification

The integrated owner serves existing product modules on the same authenticated
listener and caller-driven mutation lane. It does not run a Go compatibility
backend. Native sing-box and FRPC remain separate retained components.

## Source and target checks

- Complete host suite: 580 Rust tests passed, then 18 configuration tests passed
  after the final exact MAC wildcard separator correction. Clippy, fmt, diff
  checks and the complete ARMv7 musl link passed on the settled source.
- Frontend: 1,602 tests passed with one worker; type check and production build
  passed. The only UI schema change removes the artificial cron verbosity upper
  bound, retaining nonnegative integer validation and the console log limit.
  The UI owner approved this change before delivery.
- Actual BE6500 existing-data qualification: configuration generation 8, one
  original draft, native generation 9; six lossless native documents and original
  persistent WAN history remained readable. The final detailed browser exercised
  27 ready views including all system tabs and visible chart canvases, with no
  page errors or failed API responses. Private copied-data save/delete draft,
  scoped backup download, import preview and discard were exercised without a
  production commit/reload.
- Native health uses the existing `host` contract. Runtime capability is still
  supplied by the actual runtime API. No UI capability is enabled by a new mode.

## Real corrections and resource changes

- Configuration storage opens the existing `configuration` subdirectory.
- Native cron init accepts nonnegative verbosity; original factory value 9 is
  preserved rather than clamped to the old console-log 0..8 limit.
- Existing hyphen MAC and mapped-IPv4 validation behavior is retained. Wildcard
  MACs still use the original colon form; malformed inputs are rejected.
- Public observation snapshots reuse the existing 1s/5s observation intervals.
  Mutations invalidate them; the current caller address is never borrowed from
  another request's cached value.
- Native child pipe EOF polls have a 1ms tail rather than 20ms.
- Node probe jobs retain the verified immutable native executable FD instead of
  making another approximately 34MiB tmpfs executable copy. Cleanup removes only
  job-created configuration files and always reaps the retained job child.

## Delivery boundary

These are pre-handover qualifications, not a claim that production has switched
or that configured operation is faster. Actual single-owner handover, preserved
state/readiness checks and same-device request measurements are recorded after
installation. Old baseline records include timeouts; failed routes do not become
successful latency samples. Empty-data RSS is not a configured-workload gain.

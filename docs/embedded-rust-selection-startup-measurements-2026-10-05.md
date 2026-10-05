# Node selection and native startup qualification — 2026-10-05

## Source and delivery

Qualified implementation commit: `58a384a197132f92084a9e827032b4b6b2c77afc`. Pushed to `origin/rewrite/rust-embedded`. This record does not claim production handover.

- Complete serialized host suite: 480 passed. Separate capture/compiler focused suite: 23 passed.
- Rust fmt, all-target clippy with `-D warnings`, and `git diff --check`: passed.
- Complete native HTTPS/owner ARM executable: 3762552 bytes, SHA256 `34674df3e57c6917e650dd353808f0f45934cf95a330fc5772d19ecf9b6a20c8`. LLVM ar, ELF32 ARMv7/static/no PT_INTERP. Binary size is not RSS.

## Measured allocation hotspot

`cargo +1.93.0 run --manifest-path rust/panel/Cargo.toml --locked --release --example selection_allocation_bench` counts allocator calls on the fixed synthetic native fixture. Removing the duplicate owned `selection_settings` derivation eliminates 684 allocation calls and 47,701 cumulative allocated bytes for that operation. The optimized compiler repeated output/config SHA matches exactly. This is a host operation diagnostic, not a target RSS/timing or whole-application memory gain.

## Real BE6500 ARMv7 startup

The complete executable ran briefly on the target with fresh private RAM-only data/run roots and an admitted synthetic command that would fail if executed. Listener was loopback-only. No production data was loaded by that process.

- Unauthenticated runtime GET: 401; login: 200.
- Authenticated health: manager mode/runtime enabled.
- Both native services: not configured, desired off, no child PID.
- Capture: inactive, desired off, commands 0. No command fixture marker was created.
- One thread. Snapshot VmRSS 1,968 KiB, VmHWM 1,972 KiB. This is idle empty-data native assembly, not a configured core/capture workload.
- SIGTERM exited the exact temporary owner. Its files were removed.
- SSH tunnel forwarding was refused by existing Dropbear policy; inspection used target loopback curl. SSH configuration was not changed.

Production readback after this check: original core PID 7858, generation 9, running, restarts 0; capture inactive/desired off/commands 0. This dated readback is not a permanent current-state claim. Original owner/core, rescue access, startup archive and production bootstrap were not changed.

## Actual behavior changes

Import preserves independent drafts/current core. Explicit node selection can use a new subscription without the old node, preserves non-controlled native settings, and keeps configured-off distinct from applied-ready. Checked-core configuration changes prepare before withdrawing the old Run. Authoritative commit durability uncertainty still starts the checked new core and restores resources before returning the fixed uncertainty error.

Removed redundant owned settings derivation, duplicate current-proof config hash/TUN probe, source GET rehash, just-added durability latch/sync gate, and immediate duplicate post-Apply observation. Inconclusive secondary observation no longer automatically withdraws a successful installation. GET remains read-only; auth, fixed request bounds, exact child ownership and explicit safety-off remain.

## Remaining production boundary

Production is still the original backend. Native startup qualification is complete; configured native lifecycle, legacy state/artifact/config migration, single-owner production handover and remaining product API parity are not complete. Do not replace the production bootstrap or claim live deployment from this record.

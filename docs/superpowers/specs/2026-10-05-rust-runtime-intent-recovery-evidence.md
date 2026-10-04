# Fixed runtime startup intent and exit recovery

## Source scope

`RuntimeHttp` owns the existing exclusive `Manager`. `load_saved_intent(&mut self, data_dir)` loads only fixed sing-box/frpc Boolean flags from `desired-services.json`. Missing file means off. No constructor, GET, draft save, or preview starts a child. Loading is startup-only, is refused after an active manager exists, and borrows the owner so any error retains the cleanup handle.

`restore_saved` is an explicit integration call. `poll_recovery(now)` is also an explicit caller action; it creates no threads or timers and is not called by GET. A new main owner must call these APIs from its single bounded loop. The current loopback diagnostic/rule-draft main does not attach a Manager, load intent, restore, or poll recovery.

## Storage

4KiB maximum; only two known Boolean keys; duplicate/unknown/null/invalid JSON is refused without overwriting old evidence. Existing partial Go maps are accepted. The directory is private0700, file0600, no symlink or hardlink. Reads/writes pin the directory and file identity, and reject in-place modification. Same-directory temporary writes use OS random names, file fsync, atomic rename, directory fsync and measured free-space admission. Charge actual filesystem allocation units and keep at least1MiB headroom. Existing reserve is not consumed. Postrename uncertainty retains the committed fixed flag pair in memory and is not reported as durable success.

A failed off persistence latches off in-process first, cancels its retry state, and still attempts actual manager cleanup/stop. Future paired writes use both effective in-memory flags, not stale disk values. The response reports actual runtime state and `intentDurabilityUncertain`; it does not claim next-boot persistence when the write failed.

## Recovery

Observe only the retained owned child; do not adopt a disk PID. Withdraw a newly exited core immediately before waiting for restart backoff. A withdrawal failure retains its child/resource evidence and prohibits launching a new Run. Finite recovery uses2s exponential delay capped60s and a lifetime budget of8 recovery attempts until an explicit user start/restart resets it. Short successful relaunches do not reset the crash-loop budget. Initial startup restore failure counts in that budget. `restarts` counts actual successful automatic launches, separately from `recoveryAttempts`; `recoveryExhausted` is explicit. Manual configuration generation/readiness/rollback rules remain unchanged.

Explicit close cancels recovery and keeps private saved startup intent. Failed close retains its owner/child for retry. HTTP start/configure/restore/restart refuse a closing owner; cleanup retry and readback remain possible.

## Qualification boundary

Tests use local temporary files, loopback HTTP, fake shell child and simulated monotonic deadlines. They cover saved intent/no implicit activation, stopped/durable flags, bad JSON/private file/links/bounds/in-place modification, off persistence failure/cleanup refusal, withdrawal-before-relaunch, eight-attempt crash-loop termination, close cancellation and rejected-load ownership retention. Full host test/fmt/clippy and ARMv7 crossbuild are recorded outside the repo under embedded-performance-2026-10-04. Crossbuild does not prove real ARMv7 Linux process lifecycle or native capture execution. No production owner/core/capture operations were executed.

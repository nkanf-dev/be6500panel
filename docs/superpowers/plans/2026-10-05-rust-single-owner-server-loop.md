# Single Owner Server Loop Implementation Plan

> **For agentic workers:** Execute inline in the retained isolated integration workspace. Root alone runs serial host/ARM gates; no live operations.

**Goal:** Make the single synchronous listener usable with a borrowed actual RuntimeHttp owner, caller-driven exit recovery and explicit shutdown without new threads.

**Architecture:** `server_loop::serve(listener,service,runtime,cancel)` owns no Manager; it borrows one optional actual owner for the entire loop. Existing bounded HTTP handler runs on the same thread. A fixed monotonic cadence invokes owner recovery outside requests; status reads never start services. External cancellation is a borrowed AtomicBool; close failures return without dropping the owner, which can be retried by the caller. Main uses this same loop in diagnostic mode only until startup/artifact migration is qualified.

**Tech Stack:** Rust std/libc, existing Service/RuntimeHttp and native handlers.

---

## Task1: Fixed loop and truthful mode projection

Files: create `rust/panel/src/server_loop.rs`, `rust/panel/tests/server_loop.rs`; modify `rust/panel/src/server.rs`, `src/lib.rs`, `src/main.rs`.

- [x] Test fake preowned RuntimeHttp: authenticated GET through same bound listener, no constructor/startup implicit restoration, error/read-only health mode preserved; whole loop uses one caller thread.
- [x] Implement fixed poll idle interval100ms and recovery cadence1s using libc poll on one listener; no generic scheduler/backpressure/channel or perconnection thread. At most one accepted client per iteration; handler retains existing absolute header/body/write/operation deadlines.
- [x] Cancellation checked before new accept/recovery; Interrupted transient, fixed error codes with no OS/private strings. Terminal listener error calls owner.close. Shutdown/close failure is returned while borrowed owner remains accessible; no Drop-fallback kill or recoveryafterclose.
- [x] Test shutdown while fake child running confirms cleanup before childreap. Failedcleanup retains child/owner and caller can retryclose; cancellation never resurrects savedintent.
- [x] Main diagnostic mode reuses serve with None and unchanged loopback/flags/auth rules. Do not add unqualified artifact inputs or production listener. `/health` dynamically says manager/readOnlyfalse/runtimeEnabledtrue only when an actual owner is attached; olddiagnostic body remains unchanged.

## Task2: Native qualification

- [x] Run focused actualloop tests plus existingHTTP/runtimeHTTP/CLI tests. Full rootserialized test/fmt/clippy/diff, then ARMv7build insharedtree. NoGo/VM/routeroperation.
- [x] Save sourceevidence,commit tested files,leave production main/artifact parity unclaimed. Next production startup module must supply trusted admittedbinary and actualnativehooks, call explicitrestore beforeloop and handlecloseerrors retainingowner.

## Result

300 full host tests, fmt, strict all-target Clippy, diff check and current-source ARMv7 crossbuild passed. One synchronous loop borrows the actual owner; requires authentication; cancellation/terminal listener failure closes it with retryauthority retained on failure. CLI remains unownedloopbackdiagnostic. No production/network action or independentwatchdog claim.

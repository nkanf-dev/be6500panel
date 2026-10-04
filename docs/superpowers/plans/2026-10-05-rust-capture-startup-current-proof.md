# Capture Startup Withdrawal and Current Proof Implementation Plan

**Goal:** Withdraw validated retained cleanup ownership explicitly before restoration, and report capture current proof separately from desired/journal/Apply history.

**Architecture:** Extend existing CaptureRuntime with startup_withdraw(deadline) that uses only validated regenerated cleanup, never fresh Apply or savedPID. Read-only observation uses accepted bytes/actual retained Manager HookContext, NativeReadiness ownership/listener probe and exact kernel query proof. A shared capture/readiness handle uses the same mandatory callbacks as runtime hooks, with no extra daemon/observerthread/owner. Observation doesnot repair/clearjournal/restore; failedreads returnunknown and cannot erase cleanupfailure.

- [x] Add explicit startup cleanup that needs no currentRun; all regenerated cleanup attempted under boundedoperationdeadline; constructor/status/GETzero mutatingcommands. On failure retainjournal/executor for same-ownerretry. Off/nojournalzero commands and no freshbuilder.
- [x] Add NativeReadiness one-shot current ownership/listener/DNS proof, no startup wait or newthread. Verify actualretainedidentity before/after probes; FRPC process-onlyproof nevertunnelstatus.
- [x] Capture current observation privatefixedstate: inactive/noownedjournal, staged/not-current, cleanup-pending, unknownquery/core/scope, or active onlyfreshnativeownedproof+installedexactcompiledresources+sameacceptedcfg hash. Read-onlycommands only; storedApply input neverreplayed. No stalePID/booleans asauthority.
- [x] Keep shared handle after into_hooks; explicitManager-owned HookContext callback grants currentconfig/artifact/retainedownerstatus only and readdeadline. Startuphandle doesnotadopt/executecore. Borrowconflict/deadlinefailclosed.
- [x] Fakecommand tests startupjournaldelete/refusal/retry/noApply/zerooff; nativeobserver/listener/kernel prooffixtures active/missing/readfailure/stagedafterreopen/cleanupfailurepreserved; no device/kernel actions. Root fullserial/fmt/clippy/diff+ARM. LegacyGo journalmigration and API/mainproductionbinding remain separate.

## Result

405 full serialized host tests, fmt, strict all-target Clippy, diff check and current-source ARMv7 build passed. Reviewfixed last-usebufferrelease andpoststatusdeadline regression included. Explicitvalidatedjournalcleanup andcurrentread-onlyproof remain sourceonly; legacyjournalmigration/CaptureHTTP/mainnativebinding/fullparity/livehandover stillseparate. No device/network operation or targetgain claim.

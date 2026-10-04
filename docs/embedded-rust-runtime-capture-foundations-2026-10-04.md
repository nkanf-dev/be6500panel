# Rust fixed-service ownership foundations

Source integrations: runtime store251bfe7, process ownershipe049c0e, pure routed-TUN plannerd98a145, exact Go capture fixtures81109be/permanent differential7e7b4a9. Test-only fake startup diagnostic fixa7991aa.

## Verified scope

164 native tests passed after integration, plus fmt check, clippy all-targets with warnings denied and native release build. ARMv7 service and native test binaries cross-built with `--no-run`; no runtime lifecycle test has been executed on the router for this foundation stage. This is not a production owner handover or Linux PDEATHSIG acceptance claim.

The runtime store holds an exclusive services/.manager.lock and uses compatible private metadata/config filenames, bounded reads, candidate hashes, monotonic generations and ready-only recovery snapshots. Constructor only loads. No PID is adopted and no configuration is pruned. Postcommit directory durability uncertainty returns the authoritative committed state.

The process wrapper owns exactly fixed sing-box/FRPC handles and verbs. Each service has one128 KiB supervisor, at most one Run and one transient Check, private32 KiB output tail per child and bounded pipes/channels. Idle owners block instead of polling. Candidate checker success/failure/timeout leaves the old Run PID and group untouched. Cleanup must succeed before signaling; failure retains the owned child. Linux uses stable creator-thread lifetime and waitid WNOWAIT, but the runtime tests were performed on Darwin with fake executables only.

The pure capture planner preserves exact Go argv order, ownership and warning strings in30 synthetic cases (9 valid/21 refused), including mark0x4000/table-priority16500, six owned chains, DNS ordering and management/return bypass. It runs no commands and changes no network state. New TPROXY generation remains unsupported.

## First integrated test failure and correction

One fake-process test originally timed out waiting three seconds for its shell PID marker. The exact isolated test and all13 lifecycle tests passed when rerun. Concurrent build load was plausible but not proved causal; no production defect was reproduced. The test-only correction replaces pre-marker cat with builtin read, places readiness trap/PID marker before optional environment probe and makes the finite wait inspect actual owned exit status. Production timeouts and process source semantics were not changed. The original failed output is retained with isolated/serial rerun evidence.

## Outstanding composition and production boundary

Capture desired/journal controller and fixed runtime manager composition are in progress in separate source worktrees. Real TUN/listener/DNS/prestart readiness, capture preflight/current observation/executor, artifact admission, runtime HTTP/Apply, UCI, full FRPC/remote and historical observer parity are not delivered by these foundations. Do not turn caller-attestation constructors into public client-provided readiness proof.

No existing device process was stopped or started, no firewall command executed and no current private state read/write migration performed by this batch. Original production owner6642/core7858generation9 remain; capture last observed off and Go8788 memory-paused. Final migration removes project-owned Go implementation/build/tests/temporaryfront/glue after qualified handover, not before.

Evidence: `/Users/nkanf/docs/miwifibe6500/live-inspection/embedded-performance-2026-10-04/` final-integrated-tests, first failure, isolated/serial recheck and ARM no-run build reports. Runtime resource use for the eventual composed manager still needs direct measurement; the earlier authenticated rules HTTP peak figures do not establish composed-owner memory.

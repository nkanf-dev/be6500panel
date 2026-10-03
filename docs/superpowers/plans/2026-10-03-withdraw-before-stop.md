# Withdraw owned interception before normal stop — Implementation Plan

> For agentic workers: use executing-plans in a dedicated worktree. Root owns integration and live operations. All tests in this phase use local fixture processes and fake resource hooks.

**Goal:** Normal Stop/Restart/update must not intentionally stop a live listener while its owned interception is still installed.

**Architecture:** The runtime mutation lane remains the serialization boundary. Run the existing bounded CleanupHook before cancelling the live watcher, clearing the process identity or signalling the process. Failed withdrawal refuses the operation and retains the real process and its supervision. Audit all stop callers so recovery cannot launch a second process, replace a retained process identity, or delete its executable.

**Tech Stack:** Go runtime manager, local process fixtures, bounded fake resource hooks.

## Authority and limits

- No live Stop/Restart/Configure, capture activation, packet test, offload write, reboot or topology change.
- Preserve factory firewall/isolation/rescue. Cleanup only calls the existing owned-resource hook.
- This fixes cooperative operation order only. Unexpected crash, panel process exit/Pdeathsig, DNS-NAT bindings, stale flows, node/DNS health and global fail-open still require separate design/qualification.
- In particular, returning from Close after a cleanup failure cannot promise a listener survives parent exit. Identify and document that boundary; do not add an untested persistent guardian or silently force-kill.

## Task 1 — failing cooperative lifecycle regression

Files: internal/runtime/withdraw_before_stop_test.go.

- [x] Add local fixture test whose CleanupHook checks that the original process still exists, its Status PID remains the original and its watcher identity is retained when withdrawal starts.
- [x] Make withdrawal return a synthetic fixed error. Assert Stop/Restart return an error, original process remains, no replacement starts, generation/desire/accepted bytes are accounted for accurately, and the existing watcher still handles an unexpected exit.
- [x] Re-run explicit Stop after withdrawal succeeds; assert cleanup was observed before termination, then process is gone and state stopped.
- [x] Exercise Configure/Acquire/Restore recovery after a blocked withdrawal. Prove at most one live process, no retained executable unlink, actual accepted-versus-running divergence visible, and no silent success.
- [x] Run native go test -p 1 ./internal/runtime -run 'TestWithdraw|TestRestart|Test.*Cleanup' -count=1 and observe expected failing order before implementation.

## Task 2 — bounded lifecycle implementation

Files: internal/runtime/manager.go; only directly affected existing runtime tests if semantics require correction.

- [x] In stopProcess, snapshot current process while holding m.mu, then release m.mu and call bounded cleanup(id) before cancelling watcher, changing epoch/process identity or signalling the child.
- [x] If cleanup fails while process is live, return fixed failure; retain process, watcher and usable listener. Status must expose cleanupPending/error/needsRecovery without presenting absent PID or completed stop. Do not disable the existing supervisor merely to mark a rejected operation stopped.
- [x] After successful cleanup, change watcher/epoch/desire identity as before, then terminate and reap the exact owned process group. No recursive manager mutation from hook.
- [x] Audit restartChange/recoverRuntime/startProcess readiness failure/Close. When withdrawal fails, do not launch a second core or replace/unlink the executable of a retained live process. Commit and running-state boundaries must remain generation-checked.
- [x] Keep a truthful failed-close boundary. Report any stronger independent guardian work separately instead of pretending this small patch guarantees parent-crash safety.

## Task 3 — validation and review

- [x] Run serialized go test -p 1 ./internal/runtime, go test -race -p 1 ./internal/runtime and go vet -p 1 ./internal/runtime.
- [x] Check existing PID-reuse/process-group, capture lifecycle, recovery, generation, readiness cancellation and frozen-probe-lease regressions remain valid.
- [x] Independent read-only review checks lock ordering, watcher retention, no duplicate process, executable lifetime, failure truth and Close limitations.
- [x] Commit exact runtime-owned files. Root reviews before integration; deployment is a separate authorized step with capture still disabled.

## Completed qualification must not say

- Never drops a packet under every failure.
- Proxy-required destinations work after direct fallback.
- Node/DNS timeout fallback or crash watchdog is implemented by changing Stop order.
- The earlier phone outage is fixed by this lifecycle patch.


## Final offline checkpoint

- Implementation: `4965e5a39b17a84a6ea3c93655ab1e15f69ccdfe`; candidate equivalent: `2e899676eaa2a3c2b05828124a05ac38deef7d89`.
- Independent original-order fixture failed on the baseline and passed on the candidate.
- Integrated full Go normal/vet/race and ARMv7 build completed with exit 0.
- Independent source review: qualified PASS, with no remaining concrete P1/P2 in the six runtime paths.
- Actual running config/executable/provenance is separate from accepted candidates. Blocked withdrawal retains PID/watcher/known files; repeated accepted writes are refused until withdrawal succeeds. ReadyOperation rejects recovery/drift before callbacks.
- Failed Close retains the private store lock through exact child reap, including caller-reference loss and GC. Its lifetime pin signals nothing and retries nothing.
- **Not deployed. No live Stop/Restart/update, capture test, offload write or reboot was performed in this phase.**
- `main.go` still ignores the deferred Close error, and parent exit/Pdeathsig remains outside this patch. This is not a guardian or complete fail-open proof, and does not establish the original phone outage cause.
- Integration evidence: `live-inspection/cooperative-withdraw-before-stop-integration.log`; structured scope/validation: `retry-user-enabled/offline-dataplane-audits/withdraw-before-stop-validation.json`.

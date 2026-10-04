# Authenticated Rust rule HTTP slice: validation and device resources

Integration source: `ea6a800` (HTTP), `5c6e627` (bounded subscription), `7fd4671` (permanent subscription differential), plus previous pure policy/store/compiler modules.

112 locked native tests passed. Fmt, clippy all-targets with warnings denied, native release and ARMv7 static-musl build passed. Permanent Go comparisons retain 23 policy, 30 exact native compiler and 34 subscription/summary cases. Rust parses actual bounded Clash data, not a fake rule cache. It has one stable draft store/source state, authenticated read/save/preview, exact HEAD, fixed errors and no runtime ownership. Apply and select explicitly return 503 runtime_unavailable.

## Real browser contract

The existing LocalRulesEditor was built as an isolated page and served by the Rust process on loopback. Login, initial zero-write load, pure preview, metadata edit, draft save and fresh GET readback passed. The exact-domain direct draft retained its order and source identity. Browser errors and prohibited operations were empty. Applied remained unknown/runtimeGeneration0; Apply was disabled because this slice owns no runtime. This is not a claim that the full old dashboard or runtime controls are implemented.

## Native host workload

Representative fake source: six subscription rules and one local rule. Twenty GET/preview pairs and three same-policy saves passed. Host median GET 0.183209 ms, preview 0.189521 ms and save/readback 12.195916 ms. Warm RSS after browser was 2272 KiB, after the finite workload 2256 KiB. Host timings and memory are not target figures.

## Actual BE6500 ARMv7 HTTP workload

Only numeric loopback listeners were used. Synthetic nodes/password/drafts lived in an independent RAM directory. The finite Rust client streams response bodies in an 8 KiB buffer and does not hold the entire 5 MiB response. Its readback rolling check is an equality assertion, not a security or configuration hash.

| Case | GET ms | Preview ms | Same-policy save/readback ms | Peak RSS KiB | Readback bytes |
|---|---:|---:|---:|---:|---:|
| Representative source, 3 rounds | 1.695167 | 1.891542 | 2.657041 | 1732 | 4733 |
| Maximum counts, 2 rounds | 350.500625 | 287.589583 | 649.477125 | 8708 | 5062157 |

Maximum input: 8192 subscription rules, 512 locals, 1024 subscription edits; request247326 bytes, preview3342059 bytes. Response cap is 8 MiB, policy body cap256 KiB, fixed8 KiB transport buffer. Large responses use a typed counting pass before headers, then bounded buffering/streaming; no full-response Vec. Small source reads allocate from checked metadata rather than a fixed2 MiB buffer.

Both cases passed exact same-policy readback, login/logout/post-logout401 and explicit Apply503. No test error or new OOM occurred. One service thread was observed. Maximum warm-after RSS3744 KiB and peak8708 KiB (~8.50 MiB) include request bodies, store snapshots, source/fingerprints and effective preview. The earlier416 KiB diagnostic-auth benchmark does not describe full rules HTTP memory. Two maximum-count rounds are not robust latency guarantees or a global optimum claim. Preview/save duplicate merge passes and owned identity allocation are concrete future optimization points only if measured useful; preserve actual rule semantics and source provenance.

## Production boundary and cleanup

All temporary host/device processes were stopped. Exact PID/executable guards preceded deletion of the isolated RAM fixtures and synthetic password. Existing panel6642 and sing-box7858/generation9/restarts0 remained unchanged. Capture remained desired=false/active=false/commands0; the Go8788 entry remains memory-paused. No production draft, subscription, accepted config or core process was changed. No Rust management owner, native Apply, UCI/FRPC parity or production cutover is claimed.

Evidence: `/Users/nkanf/docs/miwifibe6500/live-inspection/embedded-performance-2026-10-04/authenticated-rule-http-browser` and `rule-http-resource-benchmark`. Next task: the runtime/capture ownership safety batch and remaining management APIs. After full migration acceptance, retire project-owned Go implementation/build/tests/temporary front; retain user data and required sing-box/FRPC/SSH components.

# Rust rule persistence and compiler: measured pipeline

Integrated source: store `8ba7085`, allocation optimization `0bae728`, native compiler `b8f4719`, exact native differential test `7699c67`, operation example `6dc01a3`.

All 84 native tests, fmt, clippy and release build passed after integration. Permanent comparisons cover 23 Go policy cases and 30 Go native compiler cases. Valid native outputs match exact Go pretty-printed bytes, final newline, SHA256, metadata and diagnostics. Invalid cases match safe refusal semantics. Oversized outputs are an intentional resource refusal at 512 KiB, matching current runtime capacity; draft documents are capped at 256 KiB.

## Actual BE6500 ARMv7 one-shot pipeline

Both cases use 512 local rules, 1024 explicit subscription disables and the same 247315-byte synthetic draft. The benchmark creates an isolated RAM scratch directory, merges policy, compiles, saves/reopens the draft and removes its scratch. It never reads or writes production rules, config, subscriptions or owner state.

| Case | Subscription rules | Effective rules | Native config | Peak RSS KiB | Compile ms | Save + reopen ms | Total ms |
|---|---:|---:|---|---:|---:|---:|---:|
| Normal bounded configuration | 2048 | 1536 | 412654 bytes | 3892 | 52.560375 | 79.457292 | 212.961583 |
| Oversized configuration refused | 8192 | 7680 | refused: native configuration output limit exceeded | 5748 | 35.363000 | 76.928166 | 365.659791 |

No request/process error or OOM occurred. The larger case correctly refuses before producing a configuration over 512 KiB. Peak memory is 3.80 MiB for the successful pipeline and 5.61 MiB for the refusal/storage path. These are one sample per case under ambient household load, not stable latency guarantees or a globally optimal claim.

This benchmark deliberately releases completed intermediate results before the next stage. A future HTTP response retaining effective preview/provenance while compiling or encoding may cost more. The actual end-to-end handler must therefore be measured again. Runtime verification children and full management-owner operation are not included.

## Cleanup and production state

The two one-shot programs finished. Scratch directories and test binaries were removed from RAM. Original panel PID6642 and sing-box PID7858/config generation9/restarts0 remained unchanged. Capture was observed desired=false/active=false/commands0 before and after. The Go8788 entry remains memory-paused. No production Rust service, kernel route/firewall write, capture enable, owner cutover or Apply was performed.

Evidence directory: `/Users/nkanf/docs/miwifibe6500/live-inspection/embedded-performance-2026-10-04/rule-pipeline-benchmark` (`target-results.json`, raw host/target outputs, benchmark source and build command in the Git example). Next gate: authenticated rules HTTP and bounded subscription parsing, then measure actual response/persistence operations before runtime ownership migration.

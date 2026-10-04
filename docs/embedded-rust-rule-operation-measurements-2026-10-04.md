# Rust policy operation: measured allocation improvement

Baseline code: `82a092e`. Optimized policy code: `0bae728` (original feature `1caa14a`). Private draft store is integrated at `8ba7085`; store operation target costs are not measured by this merge benchmark.

The same synthetic case has 8192 subscription rules, 512 local rules, 1024 disabled subscription edits, 247315-byte draft JSON, 7680 effective rules and 1024 diagnostics. Both outputs have revision `4a372db3c573a64a197774cef777474ba1213f7aa39aef303fc4b1940dca8331`. Three merges are run in a finite one-shot process, not a permanently resident service.

| Actual ARMv7 metric | Before | After |
|---|---:|---:|
| Peak RSS KiB | 7608 | 6576 |
| After-operation RSS KiB | 3140 | 2640 |
| Three-merge elapsed ms | 481.000750 | 415.395583 |
| Errors | 0 | 0 |

The observed target peak RSS reduction is 1032 KiB (13.56%). One sample per revision is not evidence of a robust timing gain. CPU tick frequency is not inferred. No OOM occurred. Exact semantic revision and output counts remain identical.

The host benchmark collected five samples per binary: maximum RSS median 13451264→9830400 bytes (26.9% decrease). Host elapsed samples were noisy and are not a stable timing claim. Host and target memory figures must not be mixed.

Internal changes: binary SHA256 keys for occurrence counters instead of cloned 64-character hex strings; fingerprint hex written once; disabled rules rejected before rule/identity cloning; replacement avoids cloning the discarded original; output vectors reserve exact eligible counts. Public owned schema, diagnostics, hashes and source indexes are unchanged. Integrated native checks passed 68 tests, including all 23 exact Go policy fixture cases, fmt and clippy with warnings denied.

The earlier authenticated diagnostic service peak of 416 KiB does not describe rule-operation memory. A real full backend must budget source input, effective policy, native compiler output, persistence buffers and verification children together. Native generated config is limited to 512 KiB and a draft to 256 KiB.

The one-shot processes finished; their isolated RAM artifacts were removed. Existing panel PID6642 and sing-box PID7858/generation9 were not stopped/reconfigured; the Go8788 entry remains memory-paused and capture remains observed off. No production rules were read or written by this benchmark.

Evidence: `/Users/nkanf/docs/miwifibe6500/live-inspection/embedded-performance-2026-10-04/policy-operation-benchmark/optimization-target-comparison.json`, raw before/after output and host comparison report.

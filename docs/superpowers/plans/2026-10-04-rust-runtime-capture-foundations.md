# Rust Runtime and Capture Foundations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port the fixed-service ownership foundations and pure routed-TUN capture intent without controlling production processes or network resources.

**Architecture:** Three independent Rust library modules: an exclusive private runtime store, an owned-child lifecycle wrapper, and a pure packet-capture planner. A later manager composes them with readiness, cleanup/restore and recovery. Do not expose runtime HTTP actions or claim ownership handover before that composition is implemented and tested.

**Tech Stack:** Rust std, existing serde/serde_json/sha2/libc. No async runtime, generic process runner, shell-command API, new guardian, IPC protocol or multi-user scheduler.

---

## Task 1: fixed-service runtime state store

**Files:** create rust/panel/src/runtime_store.rs and tests/runtime_store.rs; lib.rs export only; existing dependencies suffice.

- [ ] ServiceId is exactly SingBox/Frpc with native strings sing-box/frpc. Preserve existing services/.manager.lock and service/state.json/config-N.json or FRPC config-N.toml layouts. Options provide separate data/run roots; refuse overlapping real paths and second exclusive owner.
- [ ] Typed Artifact, ConfigRecord and DiskState preserve existing JSON names, generations, current/lastGood/artifact. Private artifact URLs/config contents are redacted in Debug and fixed errors. Read copied fixtures only; never silently reset invalid state. A legacy lastGood without ready=true is not a recovery candidate.
- [ ] Require private 0700 directories/0600 files, fd-relative/no-follow regular reads and bounded 16 KiB metadata. Accepted config reads preserve the legacy4 MiB ceiling; new writes max512 KiB. Verify SHA256, exact controlled config file names/generation references and stored bytes.
- [ ] Constructor loads but does not prune/start/cleanup network state. Config candidates stay private and temporary. Precommit verification that checker did not change candidate bytes is required. Commit monotonic generation, current record and proven-ready prior lastGood only; generation overflow fails.
- [ ] Atomic file fsync/rename/directory fsync; complete temporary growth measured against >=1 MiB free headroom. No new emergency file allocation or reclaim. Post-manifest-rename durability failure returns authoritative committed state and retains snapshots; precommit failure preserves old accepted state. MarkReady/restore update state only after caller supplies real readiness evidence, not on HTTP success.
- [ ] TDD fixture import, invalid references/digests, overlap/symlink/second lock, failed candidates, unchanged bytes, monotonic generations/overflow, ready-only rollback, private modes and injected write/sync/rename failures. Native Cargo checks and owned-file commit. No fake arbitrary PID adoption.

## Task 2: owned fixed-service process lifecycle

**Files:** create rust/panel/src/runtime_process.rs and tests/runtime_process.rs; lib.rs export; existing libc/sha2.

- [ ] Accept only a trusted local absolute artifact path with expected checksum and a controlled private config path. Fixed verbs: sing-box run/check -c; frpc -c/verify -c according to existing current launcher semantics. No shell or caller-defined argv/environment. Tests use executable fixtures under a temporary trusted root; production allowlist semantics stay fixed.
- [ ] Sanitize child environment to PATH/HOME/TMPDIR, do not inherit panel passwords or proxy variables. Create an owned process group. Linux retains an exited leader until descendants are killed and it is reaped, preventing PID reuse from authorizing signals. Observe only the Child object, never persisted PID or /api/status values.
- [ ] Linux parent-death behavior and stable creation-thread ownership must be deliberate. Do not recreate Go's creator-thread lifetime bug. A small fixed supervisor thread or an explicit single-owner event-loop implementation is acceptable; no per-request worker/thread or generic supervisor framework.
- [ ] Bounded stdout/stderr tail without exposing private output through Debug/errors/public status. Draining must not deadlock a noisy child. Poll/verify deadlines and termination have finite limits; verifier cancellation/timeout kills descendants and reaps the exact child. Native host tests may use real fake subprocesses, never actual sing-box/FRPC.
- [ ] Stop requires successful cleanup callback before TERM/KILL. If cleanup fails, retain the live child and its ownership for retry; do not drop the listener or pretend stopped. Natural exit and descendant cleanup preserve identity. No Drop operation may silently bypass required capture cleanup. The caller must explicitly finish lifecycle handling.
- [ ] TDD fixed argv/env/checksum/config validation, noisy output, verifier timeout, cleanup failure retains process, successful cleanup precedes signal, natural exit/descendants/PID ownership and repeated stop. Host tests and native checks; no router process actions.

## Task 3: pure routed-TUN capture planner

**Files:** create rust/panel/src/capture_plan.rs and tests/capture_plan.rs; lib.rs export only.

- [ ] Port current gateway and exact-device routed-TUN planning from internal/proxy/firewall.go and firewall_routed_tun.go. New TPROXY generation is rejected; old-journal cleanup compatibility is a separate later function, not a user option.
- [ ] Preserve mark/mask0x4000, table/priority16500, six exact owned chains, ordinary TUN route, declared-prefix or exact IP/MAC/interface hooks, management/bootstrap/safety bypass and DNS ordering. Factory ACL/guest isolation, OUTPUT, offload and unrelated routes remain untouched.
- [ ] Validate canonical disjoint RFC1918 gateway prefixes, bounded devices and maps, safe TUN/LAN names/address/ports/endpoint/router DNS intents; gateway derives no inventory authority in this pure function. IPv6 direct and fail-direct only.
- [ ] Output ordered argv vectors Apply/Cleanup/OnFailure plus typed Ownership, no shell strings and no execution/file/network I/O. Cleanup removes MARK hooks first, attempts all owned hooks/chains/rules/routes and never deletes core-created TUN interface. Stored identity is not proof resources are installed.
- [ ] Tests compare exact argv/order/ownership to synthetic Go reference fixtures; invalid namespace/overlap/bounds refused. No router commands or actual firewall checks. Full native checks and owned-file commit.

## Task 4: exact reference fixtures and root fan-in

- [ ] Generate public synthetic Go routed-TUN plan fixtures with direct PlanOwnedRules calls. Ordinary tests do not write fixtures; explicit output/compare env only. Pure input DTO preserves acronym fields, nil/empty semantics and ordered argv. Include gateway /24+/32, multiple prefixes, device MAC, DNS endpoint ordering, fake-IP and invalid bounds/collisions.
- [ ] Merge modules only after native checks; resolve lib exports narrowly. Compare exact plan argv/ownership and runtime copied-state semantics. Cross-build ARM but do not run runtime/control/capture operations on the router in this stage.
- [ ] Next manager stage must integrate exclusive store/process ownership, native checking, actual readiness, withdraw-before-stop, rollback and capture journal. Runtime HTTP/Apply stays unavailable until integrated fake-process/fake-runner recovery tests pass. A production owner replacement is a separate qualified operation; existing6642/7858/coregen9 and current capture-off intent stay unchanged.

# Rust Owned Native Readiness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement actual bounded native readiness checks without using production core processes, TUN devices or upstream DNS during validation.

**Architecture:** Small read-only library modules: accepted-config listener/DNS probes and owned routed-TUN process/interface/socket validation. Manager hooks call them later with actual retained process identity and accepted bytes. No daemon, interface repair, arbitrary process adoption or simulated readiness flags.

**Tech Stack:** Existing Rust std/serde/serde_json/libc/getrandom. Fixed local socket operations and bounded procfs/interface observations; no Tokio, shell pipelines or C application code.

---

## Task 1: bounded local listener and DNS readiness

**Files:** create rust/panel/src/readiness_dns.rs and tests/readiness_dns.rs; lib.rs export only.

- [ ] Port nativeReadinessTargets and bootstrap-domain selection from cmd/be6500panel/readiness.go. Current routed-TUN compiler has mixed and direct DNS listeners; retired TPROXY is not a new product configuration. Parse accepted config <=4 MiB without cloning the whole private JSON. Bound listener count and listable fields.
- [ ] Normalize loopback probes for wildcard listeners; literal IPs only, no target DNS lookup. Fixed TCP socket probe is not DNS success. If the route hijacks DNS for an inbound, require actual UDP/TCP A-query reply on the corresponding selected transports.
- [ ] DNS query uses OS random transaction, A/IN question, <=4096-byte packet. TCP framing and UDP reads obey one absolute deadline, not a renewed deadline per read. Reject wrong ID/question/opcode/rcode/TC/extended rcode, malformed labels/pointers/records/trailing bytes, bare or cyclic CNAME, unrelated/additional-only A answer.
- [ ] Wire parser work bounded by packet length; compressed name pointers strictly backwards and labels <=63/name <=254. Fixed safe errors, no config/probe/body values or private path in Debug.
- [ ] Public entry points select targets purely; one probe_once then bounded wait_until(deadline,cancel) if needed. No background loop or idle timer. Caller cancellation observed during active I/O at bounded granularity without unbounded helper threads.
- [ ] Tests pure query/response fixtures and fake loopback UDP/TCP servers only: actual answer, false port-open DNS, two transports, wrong IDs/questions, CNAME/OPT/malformed/oversize/truncation, deadline and cancellation. No Internet or router probes.
- [ ] Locked native tests/fmt/clippy/release and exact owned commit. Root target cross-build later; no production readiness claim.

## Task 2: owned TUN readiness and prestart observations

**Files:** create rust/panel/src/readiness_tun.rs and tests/readiness_tun.rs; lib.rs export only. If Task1 dependency is absent, expose a narrow final listener callback, not a fake successful readiness default.

- [ ] Parse the exact accepted tun-in shape: 11 controlled fields, owned b6p interface, private first-host IPv4 /30, MTU1500/system stack/DNS disabled/no auto-route/no auto-redirect/udp_timeout2m/udp_nat_max1024. Reject duplicate/unknown field or extra TUN, TPROXY conflict and missing unconditional direct IPv6 route. Preserve supported arbitrary non-TUN settings; no input rewriting.
- [ ] Caller supplies identity from a retained ProcessOwner: service, PID, generation, expected artifact path/inode metadata. A PID from disk/client input is never adopted or signalled by this read-only module. Bind /proc/PID/stat starttime, executable inode/path/mode and private directory identity before and after observation; reject zombie, reuse, deleted/replaced executable or generation drift. Do not hash a large artifact on every probe.
- [ ] Interface exists once, UP, MTU1500, exact expected address plus only IPv6 link-local metadata. rp_filter=2. The process has an fdinfo iff matching TUN and owns the unique non-DNS private TCP LISTEN socket on its local TUN address; ignore DNS53/other addresses. All proc reads/descriptor counts are bounded and deadline/cancellation checked.
- [ ] Pure observation structs/functions and bounded fixture filesystem readers permit tests without Linux core/TUN. Production native observation is read-only; Linux interface data via getifaddrs/ioctl/netlink as appropriate, with no sysctl/interface/route mutation. For prestart, inspect all IPv4 route tables via a bounded read-only route query; no new command executor or shell framework. If the native all-table adapter is not complete, return unavailable and report the gap, never fall back to main-table-only success.
- [ ] Prestart rejects occupied interface or overlapping interface/route prefixes; ordinary WAN default is not a private-prefix collision. No interface create/delete, route addition or rp_filter repair. Deadline-bounded one-shot observation for GET is separate from startup waiting/probing.
- [ ] Tests fake stat/fdinfo/socket/interface/route observations: starttime/exe/generation changes, wrong/duplicate interfaces, address/MTU/rpf/fd/private-listener mismatch, IPv6 metadata only, namespace collisions, exact /30 host parsing, malformed/bounded reads and timeout/cancel. No real core/proc identity from router or live network command.
- [ ] Locked native tests/fmt/clippy/release; Linux ARM cross-build. Private-safe errors and actual implementation gaps reported before integration.

## Root integration and safety boundary

- [ ] Merge after native commits, run existing164 tests and pure/fake readiness tests. Bind real readiness hooks into manager only after its fake rollback order is checked.
- [ ] ARM builds and host fake-loopback tests are not evidence of actual production native readiness. No household core stop/restart, capture command, DNS/upstream call or owner takeover is authorized by this plan.
- [ ] Capture current-kernel observation/executor, artifact admission, runtime HTTP and all remaining UCI/FRPC/history/device APIs remain separate migration tasks. Project Go deletion stays blocked until full qualified Rust ownership/data migration.

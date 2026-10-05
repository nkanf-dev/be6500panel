# Subscription Import and Node Selection Implementation Plan

**Goal:** Restore the authenticated import/select workflow while preserving independent drafts, accepted native settings and exact applied evidence.

**Architecture:** One RulesState source owns private parsed subscription/prepared rule fingerprints. Import validates whole bounded source and staged-response size before atomic source commit, moves parsed state once, preserves local draft and never modifies Runtime. Native HTTPS byte fetch reuses fixed SourcePolicy/direct boundedDNS/socket/TLS/framing; no customcrypto/defaultresolver/downloaderthread. Selection accepts exact nodeID, currentsettings/declaredports/routedTUNdirect/faildirect and omissionrevision, compiles bounded native config through the one Manager, then attests exact readback/generation before manifest/selection publication. First selection derives bind from actualcurrentLAN, not hardcoded192.168.31.1. No browserpaths/argv/PID or storedmetadata-as-ready.

## Source storage lane

- [x] Add `subscription_store.rs/tests/subscription_store.rs`: existingprivate0700root, subscription.yaml private0600,2MiBsourcecap; nofollow/pinnedinode/parent; read optional, atomictemporary/filefsync/rename/dirfsync; measured1MiBheadroom/fulltemporarygrowth. Save returns committed+durability uncertainty and retainsnewsourceauthority afterrename. No reset corruptfile or otherdraft/history/corewrite. Existing source read compatibility preserved. Source-only worker; root exports/builds.

## Root source workflow

- [x] Refactor qualifiedSourcePolicy's response loop into trusted callback/bodyconsumer for existing artifactStage and bounded subscriptionbytes<=2MiB/redirects3/45s. Retainexacterrorclasses/shareddeadline/TLSidentity and HTTPcontentencoding rules. Root/nativehelpersprivate, no generalHTTPRPC.
- [x] Strict `/api/proxy/import` bodycontent XOR url, requirednon-null/unique fields, max3MiBJSON torepresent2MiBraw; malformedorzero compatible nodes refused before storage. Nodes/public summary+PreparedSubscription bound once. Acceptnewsource afterrename evenuncertain, clearselectedmarkerbutoldcurrentcoreunchanged; drafts preserveorder/edits/orphanprovenance. Failedprecommitoldstate/readbackintact; response uncertainty truthful.

## Root pure selection

- [x] `rule_apply::compile_selection` reads currentsettings without assumingoldnode remainsin newlyimportedsubscription. Unknownacceptedsettings preservedraw; onlyproxycredentials/explicitports/routedTUN/policy arrays change. Preserve DNSauthority/localDNScache/log/telemetry/dialer; reject unsupportedtransport intent. First configuration compiles fromexplicitnode/currentLAN binds+resolvedendpoints+verifiedSRS, no arbitraryfirstnode.
- [x] StrictSelect DTO nodeId/ipv6/failure/ports, optionaldatapath/routedTUN/ack/currentgeneration; map-only nestedobjects/direct-only/caps. Resolveactualnodeendpoint through fixednativeSourcePolicybootstrap; verifiedSRS/readconfig/generation/omission gates beforecheck/cleanup. Localdraftrevision/effectivepolicy transferredwithoutkeepingpreviewgraph overchecker/fsync.
- [x] Configure sameManager, whileoffacceptconfigbutnotappliedready; whileonexactacceptedbytes/actualready resourceproof before appliedmanifest. KnownselectednodeID requiresacceptedproxycredentialidentity+selectionmanifest hash/generation, never volatilelabelalone. Manifestpersistfailure reportrealacceptedstate andselection/applieduncertain, no falseoldactive.
- [x] Hosttemporarysource/fakecore/actualHTTP tests import/noApply/draftpreserve/URLTLDsource errors/newsourceorphans/selectoldnodemissing/customsettings/firstLANbind/stalegeneration/ack/noRefs/checkfail/cleanupfail/successreadback. FrozenGofixturesnoGocommands. Fullserialstatic/native+completeARM aftersettledhost; no productionmigration/parity/resourcegain claim.

## Import qualification checkpoint

- Root observed 468 serialized host tests, fmt/clippy/diff all pass.
- Original existing0755 root and parent-symlink aliases refuse before either store; permissions/drafts unchanged. Red-to-green regression retained. Missing private roots create through relative mkdirat/openat, no canonicalization bypass.
- Complete native HTTPS/owner ARMv7 executable link passes with LLVM ar; ELF32machine40/static/noPT_INTERP. This is crossbuild evidence, not device execution or production deployment.
- Import content/URL/auth/origin/strictDTO/draft/currentcorePID/configmanifest/orphaned-edit evidence passes; no Apply on import.

## Selection and simplification checkpoint

- Actual authenticated fake-owner HTTP regressions pass for first currentLAN bind, off configuration versus applied, replacement subscription without oldnode, preserved private native settings, checker failure retaining oldPID, failed cleanup with explicit recovery, source/ack/generation refusal and local selection-evidence uncertainty.
- Removed duplicate owned selection-settings derivation, duplicate native-current config hash/TUN probe, source GET rehash and just-added durability latch/sync gate. Fixed selection evidence reuses two fixed private names with4096B metadata bound; no general storage framework.
- A checked new core now starts and restores resources after authoritative configuration rename even if directory durability is uncertain. The error remains truthful; no blind old-state rollback or healthy-core stop. Injected committed-sync regression passes.
- Capture selection reuses its installed-kernel proof instead of an immediate second full observation. Inconclusive post-Apply inspection remains local uncertainty, not automatic withdrawal. GET remains read-only.
- Host allocation diagnostic measured the removed duplicate derivation:684 allocation calls/47701 cumulative allocated bytes on fixed synthetic fixture. Exact repeated optimized config/hash match. No target RSS, timing or aggregate memory gain claim.
- Final settled serial full/static and complete native ARM chain passed:480 complete host tests plus23 focused cases; fmt/clippy/diff pass. Complete ELF32 ARMv7 static executable built with LLVM ar. Production bootstrap and original owner/core remain unchanged; no production deployment claim.

# Subscription Import and Node Selection Implementation Plan

**Goal:** Restore the authenticated import/select workflow while preserving independent drafts, accepted native settings and exact applied evidence.

**Architecture:** One RulesState source owns private parsed subscription/prepared rule fingerprints. Import validates whole bounded source and staged-response size before atomic source commit, moves parsed state once, preserves local draft and never modifies Runtime. Native HTTPS byte fetch reuses fixed SourcePolicy/direct boundedDNS/socket/TLS/framing; no customcrypto/defaultresolver/downloaderthread. Selection accepts exact nodeID, currentsettings/declaredports/routedTUNdirect/faildirect and omissionrevision, compiles bounded native config through the one Manager, then attests exact readback/generation before manifest/selection publication. First selection derives bind from actualcurrentLAN, not hardcoded192.168.31.1. No browserpaths/argv/PID or storedmetadata-as-ready.

## Source storage lane

- [ ] Add `subscription_store.rs/tests/subscription_store.rs`: existingprivate0700root, subscription.yaml private0600,2MiBsourcecap; nofollow/pinnedinode/parent; read optional, atomictemporary/filefsync/rename/dirfsync; measured1MiBheadroom/fulltemporarygrowth. Save returns committed+durability uncertainty and retainsnewsourceauthority afterrename. No reset corruptfile or otherdraft/history/corewrite. Existing source read compatibility preserved. Source-only worker; root exports/builds.

## Root source workflow

- [ ] Refactor qualifiedSourcePolicy's response loop into trusted callback/bodyconsumer for existing artifactStage and bounded subscriptionbytes<=2MiB/redirects3/45s. Retainexacterrorclasses/shareddeadline/TLSidentity and HTTPcontentencoding rules. Root/nativehelpersprivate, no generalHTTPRPC.
- [ ] Strict `/api/proxy/import` bodycontent XOR url, requirednon-null/unique fields, max3MiBJSON torepresent2MiBraw; malformedorzero compatible nodes refused before storage. Nodes/public summary+PreparedSubscription bound once. Acceptnewsource afterrename evenuncertain, clearselectedmarkerbutoldcurrentcoreunchanged; drafts preserveorder/edits/orphanprovenance. Failedprecommitoldstate/readbackintact; response uncertainty truthful.

## Root pure selection

- [ ] `rule_apply::compile_selection` reads currentsettings without assumingoldnode remainsin newlyimportedsubscription. Unknownacceptedsettings preservedraw; onlyproxycredentials/explicitports/routedTUN/policy arrays change. Preserve DNSauthority/localDNScache/log/telemetry/dialer; reject unsupportedtransport intent. First configuration compiles fromexplicitnode/currentLAN binds+resolvedendpoints+verifiedSRS, no arbitraryfirstnode.
- [ ] StrictSelect DTO nodeId/ipv6/failure/ports, optionaldatapath/routedTUN/ack/currentgeneration; map-only nestedobjects/direct-only/caps. Resolveactualnodeendpoint through fixednativeSourcePolicybootstrap; verifiedSRS/readconfig/generation/omission gates beforecheck/cleanup. Localdraftrevision/effectivepolicy transferredwithoutkeepingpreviewgraph overchecker/fsync.
- [ ] Configure sameManager, whileoffacceptconfigbutnotappliedready; whileonexactacceptedbytes/actualready resourceproof before appliedmanifest. KnownselectednodeID requiresacceptedproxycredentialidentity+selectionmanifest hash/generation, never volatilelabelalone. Manifestpersistfailure reportrealacceptedstate andselection/applieduncertain, no falseoldactive.
- [ ] Hosttemporarysource/fakecore/actualHTTP tests import/noApply/draftpreserve/URLTLDsource errors/newsourceorphans/selectoldnodemissing/customsettings/firstLANbind/stalegeneration/ack/noRefs/checkfail/cleanupfail/successreadback. FrozenGofixturesnoGocommands. Fullserialstatic/native+completeARM aftersettledhost; no productionmigration/parity/resourcegain claim.

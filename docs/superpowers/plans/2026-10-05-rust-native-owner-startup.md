# Native Owner Startup Implementation Plan

> **For agentic workers:** Execute root-owned binding in the retained integration tree. Root serialhost/ARM qualification. No production bootstrap/network change.

**Goal:** Assemble one exclusive nativeManager, its sharedcapture/readiness callbacks and authenticatedacquisition in a recoverable startup component.

**Architecture:** `NativeOwner::open` reads trustedstartupoptions and admittedsystemcommandbindings; it acquires existingservices/.manager.lock through Manager::open before any networkcleanup/restore. DefaultartifactsNone: only verifiedHTTPS acquisition/rebuilding creates executabletrust. It opens current capturedesired/validatedjournal and prepares mandatorycallbacks without actions. Separate explicit `initialize` withdraws retainedjournal first, then calls restore_saved onlywhenwithdrawalsucceeded. Failure leaves the owner borrowed/alive for cleanup/retry; no Drop-fallback kill, reconstructedPID, permanentGo fallback or bootfilehashadoption.

**Tech Stack:** ExistingRuststd/libc/nativeRuntime/Capture/SourcePolicy, no newframework/thread/crate.

## Files

Create `rust/panel/src/native_owner.rs`, tests `rust/panel/tests/native_owner.rs`; root adds module export. Modify RuntimeHttp only if required for existingtypedstatus/accessor. Main/bootstrap remainunchanged in this componentqualification.

- [x] Trustedoptions data/runroot/SourcePolicyBootstrap; fixedadmitted ip/iptables TableNames supplied from installedreleasemanifest, notHTTPfields. Data/run areabsolute/separateprivate; managed artifactsunderfixedrun sing-box/frpc private childdirs, executorunderfixedrun capture-exec. Rejectsymlink/public/foreignroot; neverrepairforeigndirectory or deleteunrelatedfiles.
- [x] Construct CaptureRuntime currentfreshNativeObserver andNativeReadiness samecancel flag, obtainmandatoryhooks+typedsharedhandle. Manager::open exclusivefixedstore precedes startupmutations, no process/artifactadoption. AttachSourcePolicy/fixedartifactroots,desiredintent/capture withoutimplicitactions. Any setupfailure beforechildactivation closes Manager safely, no cleanup/restore needed; openfailure returnsfixederrors no URL/path.
- [x] Explicit initialize -> sharedcapture.startup_withdraw<=30s -> runtime.restore_saved. Failedwithdrawal returnsblockingerror andzeroartifactfetch/check/corestart. SameNativeOwner canretry; preserveoffcapture/service intent. Missing/sourcefailed saved-on services returnprivateboundedoutcomes without droppingowner/API.
- [x] Borrowed runtime accessor serves currentAPIthroughsameownedlistenerlane; close attempts withdrawal andactualmanager close, retainsbothfailures/handles onfailure, repeatedsuccessfulcloseidempotent. No spawned observer/download/mainhelper.
- [x] Tempfile/fakecommand tests openzeroactions, duplicateexclusiveownerBusyzeroactions, validatedloadedjournalcleanupfail/retry orderingbeforeactualsource, missing/failedmetadata noreset, closewithoutRunwithdraws retainedjournal, public/symlinkrunrootsrefuse, data/service/rootcompatibility. Fullserialhost/static+ARMsourcecommit. No mainproductionactivation/Go removal/liveoperation/performanceclaim.

## Result

430 full serialized host tests, fmt, strict all-target Clippy, diff check and current-source ARMv7 build passed. Cancellation after startupwithdrawal preserves API gate; initialrestoredispatch runs once. Default main and bootstrap remain unchanged; qualification is native assembly component only, not deployment/full management parity.

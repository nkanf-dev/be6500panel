# Initial Verified Artifact Implementation Plan

> **For agentic workers:** Root-only fixed process/store integration. Serial host and ARM gates in the shared tree; no router/core operation.

**Goal:** Initialize a missing fixed-service owner from a typed verified Stage without trusting boot leftovers or starting a service implicitly.

**Architecture:** `Manager::initialize_staged_artifact` accepts only verifiedStage and expectedconfiggeneration. Stage's pinnedroot/file creates at most one fixedProcessOwner. If currentconfig exists, the sameowner verifies exactacceptedbytes with the newartifact; missingconfig means artifact admitted but NotConfigured, not checker/readinesssuccess. Off/constructor/GET never starts. Existing metadata transaction and explicitabort preserve authoritativefailure semantics. No URLfetch/HTTPAcquire or new manager/thread framework.

**Tech Stack:** ExistingRuststd/libc, Stage/RetainedStage, RuntimeStore, ProcessOwner.

- [ ] Add `run_root` to service so a no-artifact constructor retains its canonical controlledprocessroots without creating a supervisor. Reuse fixedowner creation/rootpinning for explicit initialadmission.
- [ ] Test absentowner/noConfig: typedStage admission writes boundedmetadata, returns notconfigured/noPID/desiredfalse, then laterconfigure/start uses exactadmittedartifact; close removes onlyownedstageafterreap.
- [ ] Test savedcurrentconfig/noowner: explicitadmission runs checker, preservesconfigbytes/generation, leaves stopped until explicitstart; mismatchedgeneration/invalidroot/checkfailure retainoldintent and acceptedconfig and leave nochild.
- [ ] Initialpendingstage retainschecker/cleanupauthority. Abort closes initialsupervisor onlyafterexactcheckerreap, thenownedfilecleanup. Metadata precommit error fully aborts withoutactivebinding; postrename uncertainty retainsnewintent/pendingstage/ownerfor explicitabort restoringpreviousmetadata. Cannot claimavailable/ready for uncommitteduncertainstage.
- [ ] Test failedmetadata/ownedfilecleanup canretry through the sameManager, no leaked service slot/new owner. Secondinitialization whilebinding/pending exists refused; processglobal serviceownershipslots respected. Acceptedreadonlybootmetadata alone never hashes/adoptsleftoverfile.
- [ ] Root focusedmanager/faultunits then fulltest/fmt/clippy/diff andARMv7. Commit narrowfiles/evidence; still noHTTPS/HTTPAcquire/productionowner or targetmemoryclaim.

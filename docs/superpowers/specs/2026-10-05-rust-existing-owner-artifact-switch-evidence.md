# Existing-owner verified artifact switch

## Source scope

`activate_staged_artifact` consumes only the exact typed stage alreadychecked by the same fixedservice ProcessOwner on unchanged acceptedconfig generation. URL fetching, initialunboundservice admission and HTTPAcquire are not enabled here. Constructor/GET does not start anything. Off service stays off after activation and requires explicitstart for readiness. Config bytes/generation are unchanged by artifact metadata transaction.

The current Run retains the exact launchedbinding/path/hash/file/directory stamps. Cleanup follows that Run, not newer acceptedbinding; cachedready requiresmatching config AND executableprovenance. One active, one retired and one pending typedstage per fixedservice; unresolvedchecker/recovery/retirement blocks another mutation, except safetyoff/exitcleanup.

## Ordering and failure authority

Before stop, verify exactstage/config/expectedgeneration/oldprovenreadiness and measuredmanifesttemporarygrowth. Cleanup-beforeTERM remains mandatory. After oldwithdrawal, repeatstageinode admission before authoritative metadata acceptance. Metadata precommitfailure restores provenoldbinding/currentconfig, with independent cleanup if oldrecovery readinessfails. Postrename directoryuncertainty retains committednewrequestmetadata, oldbinding and exactpendingstage; artifactdurabilityflag is distinct from acceptedconfigdurability. Explicitabort restoresoldmetadata durably before cleaning onlyownedstage. No newly applied/ready success from uncertain metadata.

Afterdurablemetadata, switchsameowner binding and stagehandles, then actualprestart/readiness/resource restore. Newreadinessfailure withdraws its actualchild first and restoresoldartifactmetadata/binding/currentconfig at unchangedgeneration. Newcleanupfailure keeps its exactchild/binding and bothstages for explicitretry. Oldready resourceRestore failure retains that sameoldPID; subsequentrecovery retriesactualrestore_resources withmatchingbinding/configreadiness instead of demandingactive beforependingrecovery clears. Durability failure doesnotauthorizeblindrollback.

Readynewcore alone doesnoterase failedoldstage retirement. Failure keeps boundedretiredhandle, blocks another switch and reports needsRecovery/notactive. Explicitretirementretry refuses any stage still usedby retainedRun using ownership-only path comparison, not freshfile admission (which must reject alreadyunlinked files). Exactforeignreplacement staysuntouched; once the knownforeignfile is removed, pinnedunlinkedfile+ENOENT may retrydirectorydurability. Successfulretry keeps samecurrentPID and clears its diagnostic.

Close cancels all launches and attempts both services. It removes successfullyclosed supervisors from manager state before potentiallyfailingownedstagecleanup, so filecleanupretry neednot call a closedowner. Onlymanagedownedstages areremoved afterchildren reap; preexistingTrustedLocal files arepreserved. Failedclose stillretains usablecleanupauthority and fixederrors.

## Evidence and remaining boundary

Root targetedmanager+HTTP/faultunits pass; latest fullserial/strictClippy gate remains authoritative. Stoppedactivation also rereads/hashchecks actualacceptedconfig beforemetadataacceptance; generation/checker success alone cannot authorize changedconfig. Two independentlyidentified source defects received regression-first fixes: retainedoldready restore retry and failedreadmission oldrecoverychild withdrawal. Latest fullhost/ARM qualification and committedresult are external `rust-existing-owner-artifact-switch-*` evidence; until readback of those results this is sourcework, not currentdeployment.

Sourceallocation adjustment drops temporaryFrozenReadyconfig before newartifactlaunch/readback; no targetRSS/latencygainclaim. GzipStage resourceadmission remains fullpotentialgrowth; metadata reserve is not consumed. Allcore/artifactfixtures areprivatefake/local, norouter/core/capture operation. Initialnoownerartifact, authenticatedHTTPSURLacquisition, startupadmittedartifact rebuilding, HTTPAcquire fullqualification, fullmanagementparity and exclusiveproductionhandover remain.

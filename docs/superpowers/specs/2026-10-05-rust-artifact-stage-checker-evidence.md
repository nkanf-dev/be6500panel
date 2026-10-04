# Verified artifact staging and same-owner checker

## Scope

`artifact_stage::Stage::from_reader` accepts an internal cooperating bounded Read, not a browser path or URL fetch. It validates encodedSHA256 before optional pureRust multigzip decoding, allmembers/CRC/ISIZE/trailers/trailingbytes, exact16MiBencoded/40MiBdecoded limits, empty rejection and the complete decoded output readbackhash. One unique0700ownedfile exists; no encodedtemporary duplicate. Admission pins directory/file identities and returnedread-only descriptors; metadata alone is not trust. fstatvfs and currentmemory admission charge fullpotentialdecodedgrowth+workingbuffers+1MiBheadroom, with no emergencyreserve consumption.

StageDrop removes only its exactinode/name. `into_retained` transfers nonClone explicitcleanup authority. RetainedDrop preserves the file. Unlink-success/directorysyncfailure can retry durability only when pinnedfilelinkcount0 and exactnameENOENT; foreignreplacement remains untouched and cleanuprefuses. All errors and Debug output are fixed/private-safe. SourceRead must cooperate with the absolutebudget; no workerthread can interrupt an arbitrary uncooperative callback.

`Manager::check_staged_artifact` admits only a typedverifiedStage under the same existing serviceartifactroot. It checks the exact accepted config/hash/generation using that newartifact in the SAME ProcessOwner while oldRun remains live. It does not persistartifactmetadata or switchbinding or create readinessproof. A checkedpendingStage blocks config/start/restart/anotherstage until explicitabort or eventualqualifiedswitch. Verifyfailure firstattempts abort/reap and cleans only exactownedstage; any unresolvedchecker orfilecleanup keeps the stage within manager. No new supervisor is created.

Explicitstop latches off first and attempts oldRun withdrawal even when stage/checkerabort fails. handle_exit likewise cannot leave deadcore capture solely because stagedcleanup is unresolved. Close retains unresolved manager/child/stages for explicitretry. `staged_artifact_checked` requires samegeneration and currentpinnedstageidentity; it is not applied/ready/active state.

## Acceptance boundaries

Stage-only fullhost317tests and same-owner verifier/off/foreignreplacement focusednativegates passed before final exitregression. Latest complete host/ARM evidence is saved externally as rust-artifact-stage-checker-*. No productiondevice/core/captureoperation. HTTPAcquire still unavailable; HTTPStransport, firstartifactadmission, metadata/switch/realreadiness/oldartifactrollback and fullproductionmigration remain work. Do not mark the broader artifactruntime-switchplan complete from checker acceptance.

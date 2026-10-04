# Verified Artifact Staging Implementation Plan

> **For agentic workers:** Independent source-only decoding lane; root serial gates and runtime ownership integration. No device operation or separate target tree.

**Goal:** Stage native core/FRPC bytes from an already bounded source, preserving compressed SHA semantics, extraction limits and exact cleanup authority.

**Architecture:** One private same-directory staged file; stream8KiB at a time. Digest fetched bytes before optional pure-Rust gzip decoding. Artifact metadata is a request, not authority: only source digest + extracted digest + pinned owned file produce admission. No download thread, URL/API, arbitrary argv, activation or filesystem scavenging is added. HTTPS transport and manager acquisition remain explicitly unavailable until separately implemented.

**Tech Stack:** Ruststd/libc,sha2,getrandom,flate2 pure-Rust backend (no nativezlib); existingfixed artifact metadata.

---

## Task1: Bounded immutable stage

Files:create `rust/panel/src/artifact_stage.rs`,tests `rust/panel/tests/artifact_stage.rs`; rootmod/dependencyexport.

- [ ] `Stage::from_reader(root:&Path, artifact:&runtime_store::Artifact, source:impl Read, budget:&readiness_tun::Budget)->Result<Stage,StageError>`. Source must honor provided absolutebudget; check before/after everyread/write, neither systemresolver nor timeoutthread hides unbounded callback. Readerfixture tests use no realnetwork.
- [ ] `Artifact` URL4096/version128/sha64hex/compressionnone|gzip bounds. URLmetadata neverusedaspathorcommand; fulltransportvalidation reservedforHTTPSlane.
- [ ] Compressed bytes16MiB/uncompressed40MiB. Exactlimitaccepted/probeoneextra; gzipallmembers/CRC/trailer/trailinggarbage validated, sourcehash covers wholeencodedstream. OSrandomunique `.artifact-32lowerhex`, nofollow regularfile0700/private0700directory/pinnedidentity, file+dirsync. Deadline/cancelerrorsfixedprivate.
- [ ] Admission accessor returns path, extractedbinarySHA256/length and requestmetadata onlyafter verifiedwholefetchdigest. StageDrop removes onlysameownedinode/name, neverreplacement/symlink; commit transferspreservation explicitly and no cleanup before realownedprocesswithdrawal.
- [ ] Measuredavailablememory/backingfilesystemspace before fulltemporarygrowth, no consuming1MiBpersistentheadroom/emergencyreserve. Stage defaultsuseexistinglimits; maxsourceReaderwork/capfixtures don'tallocate40MiBwholebuffer.

## Task2: Native checks

- [ ] Syntheticraw/gzipmulti/CRC/truncated/trailing/mismatch/exactcaps/cancel/deadline/privatepaths/changedinode/cleanupfixtures. Frozen Go artifactsemantics reference only, noGocommands.
- [ ] Rootcrateexport+pureRustflate2feature, onefulltest/fmt/clippy/diff insharedtree; ARMv7aftersettledhost. Source qualification not HTTPSfetch/manageractivation orproductionmigration.
- [ ] Commitexactfiles/logevidence, retirecleansourceworktree preservingbranch. Next manager acquisition must checkactualacceptedconfigwithnewartifactwhileoldRunlives, persistmetadataauthoritatively, withdrawbeforestop, retainoldartifactforqualifiedrollback. ThisstageAPI alone mustnot enableHTTPAcquire.

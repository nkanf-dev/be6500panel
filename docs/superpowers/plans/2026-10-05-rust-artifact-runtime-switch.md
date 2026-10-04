# Verified Artifact Runtime Switch Implementation Plan

> **For agentic workers:** Root-only process/store change; staged source lane may prepare Stage independently without builds. Serial qualification, no production operations.

**Goal:** Admit verified stage bytes to the same fixed-service owner, check with old Run live, and preserve exact cleanup/rollback authority through failures.

**Architecture:** Service keeps at most active/new/old-retained verified artifact handles; stage inputs are trusted local typed objects, not browser path/PID/argv. Existing ProcessOwner executes candidate checker in the same fixed artifactroot and same service slot. No additional supervisor/thread/manager. Store artifact-intent transaction retains accepted config generation and truthful durability result. This narrow lane does not enable URL fetch/HTTPAcquire until transport admission exists.

## Files

Modify `rust/panel/src/runtime_manager.rs`, `src/runtime_store.rs`, `tests/runtime_manager.rs`; depends on verified `artifact_stage.rs` contract, root owns integration and exports.

- [ ] Read root/path/sha/length through checked stage admission; reject wrong fixed service artifactroot and unresolved checker/cleanup before further mutation. Never hash a boot leftover to create trust. If firstartifact has no serviceowner, create one fixed owner only after roots pinned; preserve that owner on every failure.
- [ ] Keep stage in service while fixed accepted-config checker runs; use exact currentbytes/path/hash/generation with new extractedbinaryhash while oldRun remains live. Checkerfailure attempts abort; retained checker keeps its stage until explicitabort succeeds. No drop/removal of files potentially used by ownedchild.
- [ ] Metadata transaction precommit failure leaves oldbinding/Run/config/generation; postrename uncertainty reports authoritative newrequestmetadata but keeps oldRun and both recovery artifacts; no blindrollback. Do not report newly admittedmetadata as active.
- [ ] On durable newmetadata, cleanup-beforestop, switchbinding sameowner, actual prestart/readiness/resource restore. Old proven readybinding/config remains available until newready or boundedrollback completes. Rollback restoresoldmetadata/oldbinding and actualready unchangedconfig without inventinggeneration. Any cleanupfailure retainscorrectRunbinding/artifact identities for retry.
- [ ] Bounded retained artifacts: refuse another switch while pendingcleanup/metadatauncertainty/checker/oldartifact retirement unresolved. Explicit ownerclose reaps children first then removes onlyowned verifiedstageinodes; unrelated/preexistingtrustedlocal files neverdeleted. Readystatus needsbinding andacceptedconfig match; zero falseactive during mismatchedbinding.
- [ ] Fakeartifact A/B tests: checkerwhileoldalive, invalidnewbin/hash/root retainsRun, successfulswitch samegeneration, readinessfailed rollback, cleanupfailed retainsoldRun andstages, checkerabort retention, noownerinitialartifact, metadatafailure/uncertainty. Full roottest/fmt/clippy/ARM after staging+switch integrations. URLfetch/HTTPAcquire/productionmigration stay unclaimed.

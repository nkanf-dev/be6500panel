# Artifact Metadata Transaction Implementation Plan

> **For agentic workers:** Inline root implementation while source-only staging lane runs independently. One shared native target; no live operation.

**Goal:** Support atomic metadata replacement for a verified artifact without changing accepted config bytes/generation or claiming executable admission.

**Architecture:** Reuse RuntimeStore::save_state private fsync/rename transaction and existing fault injection. `set_artifact_intent(service,expected_generation,Option<Artifact>)` persists only bounded request metadata; caller must separately stage and admit binaries, check accepted config and preserve actual ownedprocess. Artifact intent does not create or authorize a ProcessOwner.

**Tech Stack:** Existing Rust serde store/sha validation/nativefaultfixtures.

- [ ] Add focused tests in `rust/panel/tests/runtime_store.rs`: unchangedgeneration/current/lastGood acrossmetadata set/restoreNone,reopen;wronggenrefuses;url/version/hash/compressionbounds leaveoldmanifest.
- [ ] Add method+fixedmetadatavalidator in `src/runtime_store.rs`, onlyknownNone|gzipcompressed intent,64hexnormalize. No download/path/command orrecordprune.
- [ ] Internal fault test exercises all premanifestwrite/sync/rename/spacefailures retainingoldstate; postrename syncerror returns authoritativecommittedmetadata while retainingconfigs and uncertainty. Storeerrors neverprivateURL/version/hashbody.
- [ ] Rootfocusedstoretests+fmt/clippy; include laterfullstaging/managergate and ARM. Do not enable HTTPAcquire from this method. Next rootmanagerownsfixedartifactroot/newcheckerwhileoldrunalive/metadataadmission/cleanup/restart/readyrollback.

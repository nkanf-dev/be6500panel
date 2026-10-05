# Native Production Handover Implementation Plan

**Goal:** Move the deployed management entry to the Rust owner with existing user data and native components preserved.

**Architecture:** One authenticated Rust process and one static tree. A trusted private release manifest binds exact locally deployed native assets; saved metadata cannot adopt a PID. The original runtime store is compatible by generation/config identity. Existing drafts/history/backups remain in place. Stop the original owner only at the explicit handover boundary after native checker qualification and rollback preparation. Keep capture off unless explicitly requested.

**Tech Stack:** Existing std Rust manager, native sing-box/FRPC, factory BusyBox/ip/iptables/UCI, serialized shared Cargo target.

## Files and tasks

- [ ] `rust/panel/src/runtime_bindings.rs`, `native_owner.rs`, `main.rs`: optional trusted fixed-service local artifact bindings from private release manifest. Default absent remains absent. Bind actual artifact hash/path with existing `ArtifactBinding::trusted_local`; never trust state metadata or adopt old processes. Root tests saved configuration real-check-before-start.
- [ ] Root migration fixture: privately save original state/config bytes and desired-services flags; generation9/current9/lastGood8 compatibility. Keep original artifact/config and data unchanged while checker runs against fresh RAM fixture. Fixed release manifest pins factory ip/iptables and actual native executable. No real captureApply or core restart during preparation.
- [ ] `rust/panel/src/observations.rs`, `tests/observations.rs` plus rootserver wiring: actual bounded system/router/network/devices/native-service observations for existing UI contracts. Read-only fixed commands; no per-request background collectors or fabricated values. Preserve single-thread request lane.
- [ ] Preserve existing frontend calls and migrate required active module handlers without permanent Go proxy/fallback server. Read/write operations use fixed bounded UCI/native contracts; source worker owns only explicit files, root serial qualifies.
- [ ] Root release/bootstrap: checksum-qualified compact package, rollback archive and SSH22/2222 preserved. Explicit original owner stop only after migration/checker and API qualification; native takes the same manager lock, checks restored current bytes, starts via retained child ownership, verifies DNS/TUN, listens8787 with samepassword/staticresources.
- [ ] Actual readback: original user data revisions preserved, one owner/API/static tree, native core configurationhash/readiness, captureinactive, auth/local+remote contract, browsererror/click regression, rescue22/2222, rollback. Test configured LinuxARMv7 resource snapshot, not empty-data extrapolation.

## Current preparation evidence

Legacy original native store wire format matches Rust `DiskState/ConfigRecord`:generation9,current9,lastGood8. Capture isoff. Native artifact is still the oldcreator-owned `/tmp/be6500panel/managed/.artifact-2427970713`, not a trusted boot adoption. Deployment will bind its exactbytes via trustedreleaseinput and reconstruct a retained newchild only at handover. Originaldata root0700 and subscription/draft/selection0600. No productionwrites at this checkpoint.

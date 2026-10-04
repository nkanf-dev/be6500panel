# Explicit Startup Artifact Rebuild Implementation Plan

**Goal:** Reconstruct missing verified volatile artifacts for fixed saved-on services before explicit restoration, without PID/file adoption or read-triggered start.

**Architecture:** RuntimeHttp uses already fixed SourcePolicy/privateartifactroots and the one Manager. Manager exposes only private bounded stored artifact request metadata, not executable trust. An explicit startup call examines saved desired flags: only saved-on AND configured services with missingqualifiedartifact enter fetch/stage/check/initialize. Acquisition occurs while runtime desired remainsoff; then existing explicitstart performsnative readiness/capture restoration. No constructor/GET download/start or unboundedboot retry. Calls share normalabsoluteacquisitioncutoffs; failure keepsmanager/APIalive with truthfulrecovery state.

- [x] Add internal manager requestmetadata accessor; no serializedURL or hashing/adoption of bootartifactfile. Existingacceptedconfig remains checkedbyrealstagedartifact nativeverifier.
- [x] Explicitrestore path invokes missingartifact reconstruction through SourcePolicy/fixedroot, not HTTPrequestbody/defaultprovider; skips savedoff/unconfigured services and alreadyqualifiedbinding. Keep existingSourceError/ManagerError typedprivatefailure, do not turn downloadfailure into notconfiguredreset.
- [x] Reuse configured8attempt2s->60s owner recovery lane for finite reconstruction retry, not another scheduler/thread. Attemptbudget counts failedfetch aswell asstart; explicitoff clearsretry first, cancelsanylater reconstruction. Do not refetch an acceptedlivecore or startfromGET.
- [x] Source-only fake tests emulate ownerclose/volatilefilesremoved/reopen/private metadata+config+savedintent: explicitstartup reconstructsbyteverifiedcore thenruns; savedoff zeroDNS/sourceconnections/children; corruptSHA/URL/unavailableconfiguredsource returnsrecoveryerror keepsacceptedmetadata; fixedretrycaps/canceloff; RSS/latency remainunclaimed.
- [ ] Capturestartup journal withdrawal is separate explicit trustedexecutor step before restoration; legacyGo journal/desiredmigration not inferred. No production CLI/ownerbinding until exclusivehandover/data/rootadmission and currentcaptureproof qualified.
- [x] Fullserialnative/fmt/clippy/diff+ARM, sourcecommit/evidence. Defaultmainremainsdiagnostic; remoteFRPC/tunnel/TLStermination/fullmoduleparity/productionmigration notclaimed.

## Result

390 full serialized host tests, fmt, strict all-target Clippy, diff check and current-source ARMv7 build passed. Completion-based retry and consistent nested deadline/cancel projection included. Mainnativebinding and explicitcapturejournalwithdrawal/migration remain separate boundaries; no productionnetwork actions.

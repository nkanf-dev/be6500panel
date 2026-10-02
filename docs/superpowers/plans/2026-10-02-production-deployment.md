# be6500panel Production Deployment Implementation Plan

**Goal:** Deploy and operate the professional panel on RN02 with actual observation and managed proxy/frpc modules.

**Architecture:** Keep one Go service, independent domain adapters and a managed external-core layer. Persistent settings small; artifacts and bounded telemetry in RAM. Frontend uses actual API observations.

## Tasks
- [ ] Deploy currentARMpanel isolatedLANport, verifyauth/UI/memory andsaveprivatecredentialsoutsidepublicrepo.
- [ ] Implement `internal/router` RN02 fixtures, bounded actualdevices/wifi/dns/firewall/routes/traffic snapshot; Go testsrace.
- [ ] Implement `internal/runtime` artifact/config/processmanager with supervised lifecycle, atomicstate, expectedhash andfault tests.
- [ ] Implement `internal/proxy` nativeVLESSsubscription/compiler/splitDNS/ownedTPROXYplans; synthetictests.
- [ ] Rootintegrate authenticatedrouter/runtime/configAPIs andtypedUIpages+actualcharts; testAPIcontracts/browser.
- [ ] Importprivate subscription,checkconfigurednativecore,startmixedproxyonrouter andverifyhandshake/dns/udp/RSS.
- [ ] Applyownedrules onlytestclient, verifycounters/IPv6/dns/stoprollback; thenLANcapture if tests pass.
- [ ] Implementpersistentlauncher/bootreconstruction usingfixedreachableartifacts; boundedretry/health andstoprollback.
- [ ] Faulttest invalidcandidate/update/checksum/coreexit/networkreload andconfirmfactoryWeb/SSH.
- [ ] Build/race/browser/CI, publishverifiedsourceinbatches, deployexactbuild andreportrealURL/status.

Implementation workers own distinct packages in isolatedworktrees. RootownsHTTP/main/UI/deploypaths. No livefixtures/credentials inpublicsource.

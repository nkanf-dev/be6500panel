# Independent strict maturity review — 2026-10-03

## Verdict

**Not accepted as more mature than mature LuCI/PandoraBox panels.** This is a useful, actively improving RN02 control-plane foundation. A modern React dashboard, editable UCI fields, real proxy metrics and bounded WAN history do not prove mature daily operations or whole-router parity. Dedicated firmware workflows remain missing. No blanket superiority claim is justified.

This review treats modernity as successful user tasks, short paths, scalable lists, preserved edits, clear states and recovery. It does not treat a visual theme or chart density as functional maturity.

## Final bounded release disposition

**Limited off-path readiness only. The maturity-superiority claim remains rejected.** The source fixes materially improve the original proxy/device/modern-UX/history work. They do not establish that transparent capture is actually usable on this router, that every native service is healthy, or that the product exceeds mature LuCI/PandoraBox workflows.

Late integration was independently read through `83a1258` and root guard/private-budget/selection-state commit `afe8e0c`; reviewed frontend dictionary/context commit `589c81a` now lands. Screenshot frontend hashes match that source. Root reported serialized final validation after cache cleanup: `go test ./...`, `go vet ./...`, full race13packages, frontend typecheck/build and588tests/46files passed. Guard/private-budget/selection-status commit `afe8e0c` was inspected by this reviewer. These are root-reported completed results, not a new independent rerun.

For a limited release, the safe boundary is:

- Capture stays disabled until explicit user consent. No actual or virtual activation was performed by this reviewer. Deployment/restart must not implicitly restore an old desired scope without that consent.
- Selected-device capture is MAC+IP scoped, with no automatic whole-LAN/OUTPUT fallback. Desired state, installed rules and data-path health remain separate claims. IPv6 direct and exact-address follow/block limits stay visible.
- Native network changes durably disable desired capture before writes. Capture Apply, runtime RestoreHook and periodic refresh now check control `Enabled`, `ErrorCode` and `PendingCommit`. Manual runtime Acquire/Configure/Restore/Start/node selection refuse pending or failed configuration recovery; Stop/Disable remain available. Automatic core-only restart is not a whole-device coordinator.
- Unsupported LAN management migration is blocked in production Stage and Commit validation. This is a safe restriction, not an implemented LAN-renumber workflow. Do not change to a WAN-exposed wildcard listener as a shortcut.
- Native configuration verification is still exact files plus generic reload outcomes and any explicit confirmation. There is no completed RN02 functional WAN/Wi-Fi/DHCP/firewall verification adapter. “Files restored” must not imply these services were functionally tested.
- Shared measured admission/reserve is now wired for history, native control, runtime, private subscription/selection/desired state and capture files. This protects panel-owned allocation paths but does not guarantee against arbitrary external flash writers, flash wear or an unmeasured target volume. A denied operation must remain explicit; history never silently shortens retention.
- Proven-ready runtime recovery restores old checked+locally-ready config/artifact and retries its owned resources. API errors retain accepted status/generation and recovery flags. The requested switch still failed even when old service recovered. Local readiness is not remote node/frps connectivity.
- The local-DNS compiler now preserves factory LAN suffixes, short lease names, RN02 aliases and private reverse queries through original loopback dnsmasq. Custom dnsmasq domains/ports/loopback bind and cyclic native forwarding are not auto-discovered by the production selector. State that limit; the exact target-core check and consented client UDP/TCP/private-name tests remain required.
- Do not offer arbitrary subscription conversion or silently imply unsupported rules were honored. `AcceptUnsupportedRules:true` remains a policy limitation. A limited accepted input must contain only the documented supported rules, or omissions must be explicitly reviewed before any activation. No “explicitly acknowledged” claim is valid without a real user action.

### Final finding dispositions

| Findings | Bounded disposition |
|---|---|
| R01–R02 capture drift and IP reuse | Source refresh/MAC guard inspected; offline regression suites previously passed. Live L2/bridge/offload and multi-device data path remain unverified. |
| R03 automatic ownership | Minimum activation guards/source checked. Accept bounded capture-off during provisional/recovery state. Whole-device coordinator and platform lifecycle remain limitations, not claimed solved. |
| R04–R06 file recovery/risk/native semantics | Snapshot compare inspected; own risk/include and DHCP native-semantic overlays passed after fixes. Functional platform health remains unverified. |
| R07 flash reserve | All named panel writers now use shared admission or held reservation. Source/test design inspected; root serialized final tests required. RN02 measured space/write budget/endurance not established. |
| R08 failed live runtime change | Proven-ready recovery/source regression tests inspected. Final root race result is separate evidence; remote path not verified. |
| R09 local DNS | Factory local path fixed in compiler; custom native DNS discovery and target/client proof remain bounded limitations. |
| R10 management migration | Production backend guard and UI note checked; unsupported by design in this release. |
| R11–R13 retained edits/node scalability | Own GET-only browser tests passed runtime/UCI retention and220-node search/filter/favorite/paging/mobile; accepted follow/custom-port readback passed. FRPC accepted-form loading and large device/section lists remain P2 scope gaps. |
| R14 routine sections/bundle | Own printer local add/delete/cancel passed; deferred references/full candidate-set source inspected after9c12b0b. Reviewer's original bundle was not independently rerun after Go pause; root should include it in serialized validation. Vendor-specific runtime operations are not parity. |
| R15 recovery UX | Recovery operation ID/global reconnect task remains incomplete. Recovery is not reported completed merely because `pendingCommit` is absent; guard checks unavailable/error state. Document maintenance fallback and do not claim finished high-risk native workflow. |
| R16–R17 unsupported policy/mixed exposure | Explicit product/input limitations. Checkboxes scope transparent ingress only; separate LAN mixed listener access can still be used by unselected LAN devices. Not accidental global interception. |
| R18 collection/history truth | Cheap WANSource and sync/loss-window/aggregate-source metadata inspected; counters/totals/gaps/corrupt-record tests passed earlier. Real year retention is a capacity claim, not already existing data. |

**Remaining mandatory before declaring capture fixed:** the exact deployed final config check (six synthetic local-DNS schema fixtures now passed); explicit user yes for named devices/test duration/rollback; fresh client UDP and TCP DNS including factory private names; ordinary proxied TCP/HTTPS and UDP/QUIC; untouched-device control; management/SSH/factory access; node switch/restart/Stop/Delete recovery; real counters/return path/offload evidence. Failure must withdraw owned resources and report incomplete state. No global conntrack flush.

## Evidence boundary

The reviewer independently read the source, plans and official capability matrix. Early worktrees were changing during review. Findings distinguish integration code, worker fixes and pending final evidence. All tests used local sources or synthetic/mocked data. No SSH, live router, TAP, actual capture, or synthetic capture activation was used. Explicit user consent remains required before any such activation.

A passing unit suite is not a functional network-path or physical flash endurance result. A synthetic one-year retention test is not an already collected year of measurements. The mature LuCI/PandoraBox comparison is against ordinary dedicated workflows, not an assertion every build has every package.

## Release blocker index

No P0 incident is demonstrated by this offline review. P1 means a high-impact release blocker or required limitation before production claims. P2 means a common task/scale/clarity gap. “Resolved in source” still does not mean router data-path verified.

| ID | Priority | Finding / source | Reproduction, impact and required remedy | Verified disposition / current boundary |
|---|---|---|---|---|
| R01 | P1 | Capture drift withdraws all devices and GET mutates rules. `internal/capture/desired.go`, `internal/httpapi/proxy_capture.go` | Select two MACs; one changes IP or disappears. Original reconcile cleaned the whole group and required manual Apply. Use server-owned bounded runtime/readiness lane, rebuild exact current identities, retain desired scope; GET observes only. | Source refresh fix `c9d1f66`; final re-test required |
| R02 | P1 | Compiled capture originally matched source IP without MAC. `internal/proxy/firewall.go`, `internal/capture/native.go` | Old selected IP reused by unrelated MAC before next refresh can be intercepted. Require IP + L2 MAC on each hook and exact cleanup/recovery. MAC guard is not anti-spoofing and only works at the expected L2 ingress. | Source fix `0b8a359`; real RN02 bridge/offload evidence still needed |
| R03 | P1 | Deadline/manual recovery does not share one runtime ownership boundary. `internal/control/manager.go:deadlineLoop`, `internal/httpapi/configuration.go`, `cmd/be6500panel/main.go` | HTTP commit/manual rollback use ResourceOperation, automatic rollback was internal and independent. Test expiry/retry racing Start, Configure, capture refresh/apply, process exit and shutdown; no deadlock and no stale rule reinstall. | Open at initial review |
| R04 | P1 | File accepted/reload exit success is not functional health. `cmd/be6500panel/main.go`, `internal/control/manager.go` | Production control manager has no Verify adapter hook. Prior rollback also failed to compare restored files. Restore exact snapshots; keep needs-recovery on mismatch. Add known native runtime checks or explicitly state files-only verification, never full recovery/health. | Snapshot compare source fix `e07cf75`; platform verification unavailable |
| R05 | P1 | Risk classifier misses exact management denial and permits vendor include deletion. `internal/control/validate.go` | Own overlays failed exact LAN admin→router:8787 DROP risk and factory include deletion. Such a commit could be final, without deadline. Treat management-impacting rules/membership as provisional; protect owned factory includes. | Independently PASS at5594856 for both exact deny and include deletion |
| R06 | P1 | New field semantics rejected by generic backend validation. `internal/control/validate.go`, `web/src/components/configuration/field-schema.ts` | Own overlay failed DHCP dnsmasq port=0, host dns=1, host ip=ignore. Section-aware native validation must match frontend. Preserve unknown vendor semantics; edit unrelated field in documents containing these values. | Independently PASS at a1cdabb for all three native semantic regressions |
| R07 | P1 | Package-local byte limits can exhaust flash needed for rollback. `internal/traffic/store.go`, `internal/control/store.go`, `internal/runtime/store.go`, proxy private files | History allocates 3,072,192 B (~2.93 MiB) of stated ~6 MiB free. State rewrite uses old+new plus candidate snapshots; runtime/subscription limits add further MiB. Global measured reserve/admission and recovery headroom, not per-package boundedness, is required. Inject low free space / ENOSPC during Stage, Commit, journal and rollback. | Shared budget source81982b4/339cf44/03a8189 plus capture83a1258/root private writes inspected; root-reported serialized final tests passed; physical RN02 headroom/endurance unmeasured |
| R08 | P1 | Check-valid new runtime can replace working accepted state then fail startup, disabling desired state instead of resuming prior runtime. `internal/runtime/manager.go:Configure/startProcess`, `internal/httpapi/runtime.go` | Fake core passes check; new readiness fails. Previous process/capture must be restored under lane, or authoritative accepted-but-not-running state + safe manual recovery must be explicit. Errors must carry accepted generation/state, including durability failure; no blind retry. | Proven-ready config/artifact recovery source041a898 and substantive offline tests inspected; root reports final race passed; health remains local readiness |
| R09 | P1/P2 | Local DNS compatibility lost through capture. `internal/proxy/native.go`, capture router-DNS rules | Selected router port53 redirects to core; lan/local suffix queries go public TLS dns-direct, not dnsmasq. Resolve DHCP hostnames/internal records via original LAN resolver with loop exemption. Test local names and direct/proxy public answers over UDP/TCP separately. | Factory LAN-name/shortname/privatePTR dns-local sourcef13f810 inspected; no live path test; custom native local DNS discovery not wired |
| R10 | P1 functional | Supported LAN renumbering cannot complete on default fixed listener. `scripts/bootstrap.sh`, `cmd/be6500panel/main.go`, proxySelect bind | Bootstrap binds192.168.31.1; one HTTP listener and default node binds do not follow new LAN address. Native field accepts renumbering but no new-address/reconnect job/global confirmation. Implement safe platform lifecycle or disable/declare this task unsupported. Do not call LAN workflow parity. | Production PreserveLANManagement guard source inspected; unsupported task rather than parity; root reports final guard tests passed |
| R11 | P1 edit loss | Runtime native config disappears on tab navigation. `web/src/modules/runtime/native-config-editor.tsx`, `proxy.tsx` | Independent mocked browser: edit marker → runtime→nodes→runtime returns empty textarea. Reload confirmation alone does not protect unmount. Preserve authenticated in-memory edits or block intentional navigation; clear on logout. Audit FRPC/subscription similarly. | Runtime subfinding independently browser-PASS after40c2361; other editors need audit |
| R12 | P2 destructive defaults | Node selection restores ID but not accepted IPv6/ports; FRPC form does not read accepted config. `node-selector.tsx`, `proxy_runtime.go`, `frpc.tsx` | Navigate away; return to direct/default ports though accepted config differs. Re-Commit changes unrelated policy. Display/load accepted policy + generation; explicit preserve/replace/clear credentials, not blank=drop. | Node follow/custom-port readback browser-PASS at20cf80c; FRPC accepted-form workflow still limited |
| R13 | P2 high-volume UX |220-node table renders all rows, with no search/filter/favorites/paging and distant Commit. `node-selector.tsx`; devices/capture/native sections also map full lists | Browser measured selected y13,883.84 and Commit y14,154.75 at1000px viewport,220 radios,0 searchboxes. Bound rows, searchable identifiable choice, visible current/pending summary, keyboard/mobile interaction. Add200-device/512-section fixtures, draft summaries. | Independently GET-only browser-PASS at20cf80c for220nodes→20cards/search/favorites/page/mobile/no writes; device/section scale remains unproven |
| R14 | P2 routine tasks | Fields cannot add/delete sections; cross-document dependent drafts cannot commit atomically. `NativeFields.tsx`, `NativeEditor.tsx`, `internal/control/manager.go` | Printer reservation/first forward/static route/guest needs raw UCI. Own bundle overlay: new guest network + DHCP reference marks DHCP invalid at Stage and Commit rejects before bundle validation. Typed section CRUD and candidate-set validation must avoid intermediate live commits. | Printer local add/delete/cancel browser-PASS6bc8f23; deferred reference candidate-set source9c12b0b inspected, root reports final candidate-set suite passed; vendor lifecycle not parity |
| R15 | P2 state truth | Recovery failure hides operation ID; pending UI only exists in configuration page. `control/types.go`, manager Status, `CommitControls.tsx`, `use-configuration.ts` | Expiry fails reload, reconnect from lost Commit response: phase rolling_back loses pending ID; Documents/Drafts blocked. Return recovery operation/phase/deadline/actions globally and provide authoritative terminal result. Unknown ≠ rolled back. | Open at initial review |
| R16 | P2 policy truth | API automatically acknowledges unsupported subscription rules. `internal/httpapi/proxy_runtime.go` | CompileNative has explicit acknowledgement guard; proxySelect hardcodes true, diagnostics say explicitly acknowledged when user never did. Require concrete omitted-rule impact preview and consent or reject. | Open at initial review |
| R17 | P2 access clarity | Transparent checkbox scope is not explicit mixed listener scope. `internal/proxy/native.go`, `proxy_runtime.go` | Mixed listener binds LAN without users. Unselected LAN can explicitly configure it; this is NOT automatic/global transparent capture. Show this separate access policy and offer loopback/authenticated scope. | Limitation required |
| R18 | P2 history metadata/resources | Whole router Snapshot collection couples WAN to firewall reads; persistence/source wording incomplete. `traffic/collector.go`, `router/adapter.go`, `TrafficHistoryPanel.tsx` | Full Snapshot spawns two firewall reads under1500ms collection deadline every5s; slow firewall can create WAN gaps. Separate cheap counter path and benchmark. UI says 已持久化 for up-to-minute unflushed data; show loss window/last flush, source-change aggregate semantics. | WANSource/sync metadata source fixes5594856/f3150c5 confirmed; physical budget proof remains |

### Do not confuse installation, readiness and health

The new DNS readiness parser sends bounded local UDP and TCP queries and rejects malformed/mismatched/no-answer packets. This is materially better than a socket dial. Its chosen managed bootstrap identity routes directly. It does not prove a selected client's NAT/TPROXY return path, proxy upstream TCP/UDP, QUIC/offload, DNS-over-HTTPS bypass, local DNS names, existing NAT conntrack cleanup, FRPC server/tunnel reachability or management reconnect.

The capture UI correctly states “rules applied, not internet connectivity verified,” partial MACs stay visible, and IPv6 direct is explicit. Keep that precision. IPv6 follow/block covers only one user-provided address, not privacy-address churn or every address of a selected MAC. Fail-direct removes owned rules but does not necessarily erase old DNS REDIRECT conntrack or cached fake-IP identities. These are required product limitations until explicitly tested/fixed; never use a global conntrack flush.


## Honest feature comparison

| User task | Mature LuCI/PandoraBox reference | Current be6500panel evidence | Decision |
|---|---|---|---|
| Edit WAN/LAN and keep management access | Interface-specific fields, apply/confirm, topology/state/reconnect | Six raw native documents plus field editor, generic reloads; fixed bind and missing platform verification | Not parity |
| Add static DHCP reservation or DNS host | Add/delete row; device integration and validation | Routine typed section add/delete now exists; native backend semantic defects pending | Not parity |
| Add forward, zone, static route | Dedicated Add/Remove and scope-aware forms | Routine section add/delete for forward/rule/route exists; generic UCI lifecycle and total-policy-only observation | Not parity |
| Manage radios, guests, wireless clients | Capability-specific options and radio/association status | Configured UCI observations; RN02 MLO/TWT/Mesh/guest lifecycle adapters absent | Not parity |
| QoS/SQM, VLAN/IPTV, multi-WAN/PBR | Dedicated pages where driver/packages support them | Planned matrix, no completed RN02 workflow | Not parity; do not offer unsupported CAKE/QSDK substitutions |
| VPN, DDNS, UPnP/NAT, parental control | Dedicated state/edit/service workflows where installed | Planned adapters; raw six-package access is not these operations | Missing |
| Configuration backup/restore, reboot/OTA | Dedicated maintenance jobs and validated recovery | Journal snapshots cover panel-owned edits only; no complete export/restore/OTA workflow | Missing |
| Proxy selection at 100+ nodes | Search/group/filter, selection status, current policy | 20-card search/filter/favorite/page workflow independently tested; accepted policy readback | Must measure scalable task completion |
| Observe actual proxy connections and rate | Optional installed core/telemetry modules | Local bounded Clash API collector, clear active-only scope and stale state; no HTTPS phase invention | Good scoped improvement, not whole-router superiority |
| Year WAN history | Optional collectd/rrd/vnStat tooling, retention varies | Fixed 400-day three-tier ring; central collector; byte totals/coverage/gaps | Good scoped feature once wired and resource proof passed |
| Custom homepage | Stock/custom theme varies | Browser-local visible/order/width edit/save/cancel, corruption recovery, keyboard buttons | Good modern interaction, one named layout only |

Wi-Fi7/MLO, Mesh, dual-WAN, VLAN/IPTV, QSDK QoS/ECM, UPnP, DDNS, VPN, NAT/DMZ, parental/security, backup/OTA and lifecycle completion must follow `docs/official-capability-matrix.md`. Raw editing never earns a dedicated-operation check mark.

## Minimum acceptance matrix

| Gate | Minimum independent test / user workflow | Pass condition / required evidence |
|---|---|---|
| Capture scope/consent | No selected devices; inspect pages/check boxes; explicit user-authorized activation only | No POST/no rules until explicit action. No OUTPUT/LAN-wide hooks. No broad prefix fallback. Virtual activation also needs explicit user yes. |
| Three-device lifecycle |A/B/C selected; removeB; changeA address; oldA address reused byD; C disappears/returns | Exact currentIP+MACA/C only;D never captured; desired scope retained; per-device pending visible. Rules restored server-side without browser, accepted core settings only. |
| Stop/delete/restart/crash | Core Stop→Start, Configure/node change, core crash/backoff, panel restart; explicit DELETE with cleanup failure | Stop suspends but retains; DELETE durable off prevents all future auto activation; journal only ownership, not arbitrary commands. No stale-generation cleanup or hidden current-live state. |
| Race ownership | Expired risky commit and retry recovery race Configure/Start/Stop/Apply/refresh/process exit/Close | One coordinator protects native mutation+rollback and runtime resources. Bounded completion/no deadlock; no capture reinstall during provisional/recovering state. |
| Recovery independence | Browser closes/current Mac network changes during apply/failure/deadline | Backend recovery completes without that browser/network. Exact prior file comparisons; actual platform checks or honest unverified state. Readable recoveryID/status remains available after reconnect. |
| DNS actual path | Local UDP/TCP protocol response tests; later consented client external and router-addressed UDP/TCP DNS, local names | Matching validated answers; public direct/proxy and private names correct; fresh queries succeed; listener-only success not accepted. |
| TCP/UDP actual path | Later consented ordinary HTTPS/TCP, UDP echo/QUIC, IPv4+explicit supported IPv6, management/SSH/stock UI | Packet/rule counters and real destination responses agree. Untouched device path unchanged. Offload on/off effect recorded. No fake HTTP/TLS stages. |
| Failed node switch | Old working node→check-valid bad-readiness node, cancellation/durability/selection-save failures | Old accepted healthy runtime+scope recovers or precise accepted-but-not-running state and reliable recovery. Config generation, selected metadata and desired boot state consistent. |
| One-year history math | Synthetic ≥400 days, tier rollover/cross-boundary sample, source changes, reset/reboot/outage/time regression | Byte sums exact (not rate sums), coverage exact, peaks bounded, no fabricated zeros or huge delta. Output≤maxPoints and no tier double counting. Real start date shown. |
| Crash/corruption | Kill between writes/Sync, torn record, previous valid slot, truncation, header/CRC fault, startup ENOSPC | Valid previous records recover, damaged regions visible as gaps/error, corrupt header not replaced silently. Fixed disk budget and max60s write-loss documented. |
| Flash/RAM/CPU budget | Start with~6MiB free and~20MiB volume plus existing panel/runtime assets; largest practical Stage/Commit/rollback while history runs | Measured total allocation/write amplification/free reserve; recovery survives admissions/full disk. No whole-year rewrite. Router RSS/CPU/cadence and tmpfs peaks measured, not guessed. |
| Forms native round trip | Booleans/selects/numbers/passwords/lists, unknown vendor fields, CRLF/quote/comments; DHCP port0/dns bool/ip ignore | Local buffer preserves unrelated values; Stage only private; precise diff and validation; final backend schema supports real native semantics. |
| Routine new object | New reservation/forward/static route/SSID; dependent network+DHCP/firewall bundle | No raw source required; add/remove controls and reference-aware preview; one atomic commit, not unsafe intermediate live configuration. |
| Draft/auth/navigation | Dirty UCI/runtime/FRPC/import edits across tabs/pages; refresh/Back/logout/401; API generation conflict | No silent edit loss, staged draft restoration possible, current/accepted/selected clear. Private memory cleared on logout; no credential browser storage. |
| LAN/high-risk workflow | Non-default management subnet; current client changes network; pending operation outside configuration page | Current effective topology drives binds/scope. Reachable new management origin and global deadline/reconnect task, or explicit unsupported before apply. Automatic recovery independent of Mac. |
| Modern scalable selection |220 nodes,200 devices,512 sections; search exact item, favorite/filter/page, keyboard and390px | Selected/pending choice and Commit visible; bounded rows; no page-long scroll; stable selection, keyboard focus and capability-aware policy readback. |
| Real telemetry | Exact target core with_clash_api; restart/node epoch/counter reset/error/oversize/stale/empty/fixed manual probe | Loopback only; finite limits; no credentials/raw config; actual active connections/rates only; direct-included totals and short-flow omissions stated. Failures never0ms. |
| Dashboard interaction | Visibility/order/size, save/cancel/reset, all hidden, corrupt/blocked storage, narrow/zoom/reduced motion | Save is explicit, Cancel never writes; recovery controls and focus/live announcements; real source/error/empty states; only presentation persisted. |
| Mature feature claims | Run named dedicated workflows in capability matrix, with platform discovery + runtime verify/restore | Wi-Fi7/MLO/QoS/Mesh/etc remain planned until actual operations pass. Raw editor, pointer existence, green ready or proposed plan never equals parity. |

## Modern functional UX standard

- Locate and select one of 220 nodes by name/server with no page-long scroll. Search, filter, favorite and selected-summary controls must be reachable on 390px screens. Bound rendered rows. Keep keyboard focus and selection across filters/pages.
- Show accepted node IPv6/listener policy and actual runtime generation separately from unsaved choice. Never reset policy because the user navigated away.
- Preserve dirty UCI, runtime JSON/TOML, subscription and FRPC edits through normal tab/page navigation. Stage must not apply. Logout must clear private in-memory edits. Refresh must warn or recover.
- Add/remove routine objects without editing source syntax. A printer reservation, forward, static route or additional SSID is not a mature workflow when raw UCI is required.
- Keep pending recovery visible globally. Give new management address, deadline, operation ID and authoritative terminal state. A green process badge or file-accepted state is not WAN/DNS/tunnel health.
- Show real source, timestamps, partial coverage, stale/error/empty distinctions and start-of-recording. Empty charts must offer an actionable reason. Never fabricate zero, request phases, historical hits or missing months.
- At 390/844, desktop and 200% zoom: no page overflow, usable field labels/actions, 44px-class primary touch controls, visible focus, logical tab order and actual tab keyboard behavior. Check contrast and reduced motion. CSS-only assertions do not establish these.
- Layout edit must expose visibility/order/width, Save/Cancel/Reset, recover all-hidden and corrupt/blocked local storage. Do not persist private router config or samples with layout data.

## Verification record

Independent reviewer-run evidence:

| Check | Result and boundary |
|---|---|
| Early worker Go race suites: control; traffic+HTTP; telemetry; capture/runtime/HTTP/router | Passed. These were local package tests, not the final root commit or live data-path proof. |
| Broad integration race: cmd/proxy/capture/router/traffic/telemetry/control/runtime/HTTP |9packages passed before later budget/recovery/guard integration. |
| Early integration TypeScript/Vitest | Typecheck passed;39files/435tests passed. |
| Own Go overlays: management DROP and factory include deletion | Initially failed; passed after integrated safety fixes. |
| Own Go overlay: DHCP dnsmasq port0, host dns1, host ipignore | Initially failed; all three passed at a1cdabb. |
| Own Go overlay: restored-file drift | Passed after verifyRestored fix. |
| Own dependent network+DHCP bundle overlay | Initially failed. Corrected candidate-set source9c12b0b independently inspected; not independently rerun after root compile pause. Sibling reports the exact overlay passed. Root final suite must include it. |
| Latest shared-budget race retry | Actual host ENOSPC during Go compilation/link and ring writes blocked the run. Storage library tests passed. Not a project regression or final suite pass. Log: `/var/folders/24/p92yf6ld797061scjvy681300000gn/T/be6500-strict-review-noa8ds8e/storage-race-output.txt`. |
| GET-only browser: runtime/UCI drafts | Runtime native text originally lost on tab switch; independently passed after40c2361. UCI dirtySSID retained across pages. |
| GET-only browser:220nodes | Initially220radio rows, no search and selected/Apply at~14,000px. After20cf80c:20cards,1searchbox, exact search/favorite/paging, accepted follow/custom-port readback, keyboard Tab, mobile0overflow; no writes. |
| GET-only browser: printer section | Add→local editable values→delete impact→cancel retains→local delete removes; no Stage/Commit/POST. |
| GET-only browser: dashboard |390px0overflow, Save/Cancel/Reset and6move controls, cancel retainedWAN. Final latest mock screenshots also0overflow and no page errors/writes. |

Earlier mock browser artifacts: `/var/folders/24/p92yf6ld797061scjvy681300000gn/T/be6500-strict-review-noa8ds8e/browser-workflows.json`, `node-workflows-final.json`, `section-workflows.json`. Latest screenshots and manifest are under `/tmp/be6500-final-review`.

The reviewer independently read source, but did not start final Go compiles after the root coordination pause. Root-reported final all-Go/vet/race13packages and frontend588tests/46files/typecheck/build are recorded below as root evidence. The reviewer model cannot view screenshots; a screenshot file or DOM result is not visual approval.

## Release claim permitted within the bounded scope

“RN02 panel source with editable native fields and routine section drafts, selected-device capture lifecycle safeguards, validated local DNS readiness checks, actual bounded core telemetry support, searchable paged node selection, customizable browser-local dashboard and 400-day bounded WAN-history storage. Transparent capture is not yet accepted as working on the live router; platform/recovery/resource limitations apply.”

Do not claim “full official parity,” “more mature than OpenWrt/PandoraBox,” “internet connectivity verified,” “all selected-device addresses covered,” or “a year of retained records” without the corresponding evidence.

## Latest mocked UI screenshots and DOM gates

`/tmp/be6500-final-review/manifest.json` records8screenshots from the latest root integration source. `source-snapshot.json` records selected frontend SHA256s. Images visibly label `UI TEST MOCK`, synthetic data and GET ONLY. The screenshots are not live-router measurement or visual approval.

- Node cards: desktop dark/light and390px mobile;220fixture nodes,20cards,1searchbox, accepted follow/custom ports, running-state action “保存并应用节点”.
- Custom dashboard: desktop dark/light,390px mobile, layout editor desktop/mobile;6widgets/6editor rows, Save/Cancel visible.
- All measured page overflow=0; page errors=[]; write requests=[]. No probe, apply or capture request occurred.
- Reviewer cannot see images with this model. Parent must relay these files to a vision-capable UX expert for visual review.

## Root-reported final validation stamp

Root reports `afe8e0c` includes late control guards, shared private storage admission and accepted-node-marker/state fixes. Serialized Go all-tests/vet and13-package race passed after host-cache cleanup. Frontend typecheck/build and588tests in46files passed. The reviewer independently inspected the source and GET-only UI gates, but did not rerun these final compile suites after the coordination pause.

Root also reports the router's installed ARM sing-box1.14.2 executed `check -c` on six synthetic local-DNS direct/follow/block and fake-IP variants, with no stderr and aggregate SSH exit0. The reviewer read root's local evidence file `/Users/nkanf/docs/miwifibe6500/live-inspection/proxy-lab/native-schema-check-results.json`: all six variants record `exitCode:0` and empty output. Root reports one SSH transport255 was retried successfully; that transport error is not a core check failure. This is config-parser admission only: no core `run`, transparent capture or traffic-path activation. Current live capture was reported inactive with0commands by root. These statements are root evidence, not this reviewer's live access. The per-case record should accompany release verification artifacts.

Root reports `make armv7` passed and a separate vision-capable copy/UX expert approved the latest mocked node/dashboard screenshots. A final module-status copy regression/hotfix was still awaiting its hash when this note was written. This reviewer does not claim independent visual approval. None of these results changes the limited off-path verdict into live connectivity or maturity-superiority approval.

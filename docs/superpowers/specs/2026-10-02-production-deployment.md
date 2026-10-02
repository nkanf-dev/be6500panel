# Production deployment: be6500panel RN02

## Outcome
Deploy the management panel to the user's actual RN02. Keep factory Web/SSH recovery, bind panel to LAN192.168.31.1 independent port8787, authenticate access. Support real observation, persistent small settings, managed external runtimes and native VLESS policy. No private configuration/public history.

## Stage order
1. Deploy read-only panel immediately and validate ARM execution/UI/auth/resources.
2. Add RN02 read-only observations and bounded traffic samples to actual charts.
3. Add atomic configuration store + operation coordinator + managed sing-box/frpc lifecycle.
4. Import private subscription locally, run explicit proxy and measure.
5. Single-client transparent IPv4/IPv6 TPROXY + DNS verification and owned-rule rollback before wider capture.
6. Boot reconstruction of panel/core from fixed source, supervision and upgrade/recovery tests.

## Ownership for parallel implementation

### router adapter package
`internal/router` owns read-only actual RN02 observations and parsers. Export `New(root string) *Adapter`, `(*Adapter).Snapshot(ctx) (Snapshot,error)`; root='' means actual /; fixtures choose temp root. Snapshot JSON `{platform:{model,firmware,kernel,architecture},devices:[{ip,mac,hostname,expiresAt,online}],wifi:[{name,ssid,band,channel,bandwidth,disabled,encryption}],dns:{resolvers:[string],leaseCount:number},firewall:{ipv4:{input,forward,output,rules},ipv6:{input,forward,output,rules}},traffic:[{interface,rxBytes,txBytes,rxBytesPerSecond,txBytesPerSecond}],routes:[{family,destination,gateway,interface,metric}],sampledAt,errors:[{module,code,message}]}`. Device identities only returned authenticated API, never fixtures copied from live. No credentials/WiFi passwords in observation. Read bounded proc/files; fixed exec uci/network when necessary use context timeout argv not shell. Snapshot initial previous counter may produce0rate but stated actual delta availability. Validate unsupported parsers and failing sources distinctly. Captures leases/proc ARP/safe UCI wireless fields/resolv/proc routes/iptables-save. Root integratesAPI /api/router and chart telemetry.

### runtime and settings package
`internal/runtime` package `runtime` is standalone process/artifact/settings manager. `New(Options{DataDir,RunDir,Logger}) (*Manager,error)`, `Close() error`; exported types and methods documented by worker in README. Needed: bounded allowed-service IDs sing-box/frpc; checksum verified HTTPS/local artifact acquisition intoRunDir; gzip extraction limit, no tar path traversal; atomic artifact activation; configure binary+config then fixed argv check before start; no shell strings/arbitrary user commands; process groups+TERM boundedKILL, crash bounded backoff, small log tail, live RSS/state, stop cleanup hook seam; serialize transitions, reject simultaneous incompatible mutation; config private0600 atomically persisted, lastaccepted snapshot restore, generation conflict. Artifacts onlyRAM, small manifests/data persistent. Manager can run fake executable testsnative; do notexecute downloaded unknown binary. Main/http integrates root later. Backend module runtime may be unavailable before artifact loaded, explicit state.

### proxy package
`internal/proxy` standalone native config compiler and owned rules planner. Parse boundedClashYAML subscription into VLESS+TCP REALITY/Vision/uTLS nodes; avoid public converters; keep credentials only config files0600, return redacted public views. Fixed sing-box1.14.2 native config, explicit SOCKS mixed and TPROXY DNS inputs; local domain overrides+boundedCNdomain/IP SRS references, domain vsIP/dns paths, management/private/linklocal/bootstrap endpoint bypass; strictUDP/IPv6/failure policy. Tests synthetic config fixtures, no realnodes. Export compilerInput/Output and documentedAPIto root. Target selected minimal core lacksclashapi/gvisor/quic. Optionalgeo rule sets run/tmp verified/hash. Publish pure intended firewall command list (`[][]string` exactargv), explicitownership chainmarktableconstants not collisions; no applying in worker. Cleanupownedonly, inputscopeoneclientfirst. Rootactual handshakes/commandtest.

## Root integration
Root ownsAPI/main/auth/TLS/deployscripts/UIintegration/realdevice tests. UI replaces unavailable pages with actual router snapshot, realtraffic trend; no mock data whenhost mode. Configplans+apply/lifecycle endpoints use typedoperation status+logs. FRPC nofrpscredentials available: support realconfig/start but donot invent serveror expose panel. Network mutations require candidate plan+verify+rollback andkeep adminreachability. Public commits smallverified; deploy exactverified binaries, privateassets outside repo.

## Runtime sizes
PanelARM6.0MiB; statics1.3MiB. Minimal sing-box32.25MiB/gzip11.55MiB. RAM available~171MiB. /data12.7MiB; panel cancompressedpersistent andcoreRAM/download. Sharedtmpfsnoextraspace; measure RSS/connectionspeak.

## Production API extension contract

All endpoints below require existing session authentication. Development demo mode without a runtime data directory keeps runtime controls disabled.

- GET /api/router -> router.Snapshot (fields in adapter contract above).
- GET /api/runtime -> {enabled:boolean,services:[Status]}, Status{service,state,generation,configured,artifactAvailable,version?,pid?,rssBytes,rssAvailable,desired,restarts,retryAt?,errorCode?,recoveryPlan?}.
- POST /api/runtime/acquire {service:"sing-box"|"frpc",artifact:{url,sha256,compression:"none"|"gzip",version}} -> Status.
- POST /api/runtime/configure {service,config:string,generation:number} -> Status. private nativeJSON/TOML is authenticated input; no logs/body capture.
- GET /api/runtime/config?service=... -> {service,config:string,generation:number}.
- POST /api/runtime/start and /api/runtime/stop {service} -> Status.
- GET /api/proxy/nodes -> {nodes:[{id,label,server,port,protocol,transport,reality,vision,utls,udp}],diagnostics:[{scope,index,code,message}],selectedNodeId:string}.
- POST /api/proxy/import {url?:string,content?:string} exactlyone, HTTPS boundeddownload orClashYAML <=2MiB. -> same nodes response. Native locallyparse, noexternalconverters.
- POST /api/proxy/select {nodeId:string,ipv6:"follow"|"direct"|"block",failure:"direct",ports:{mixed:number,tproxy:number,dns:number}} -> {status:Status,configSHA256:string,diagnostics:[]}. Compilesprivate nativeconfig andruntimeConfigure; coreartifactmustalreadyavailable forcheck.
- POST /api/proxy/capture {clientIPv4:string,clientIPv6?:string,ipv6:"follow"|"direct"|"block"} -> {active:boolean,clientIPv4,clientIPv6?,commands:number}; pureownermark/rulesapplyrootexecutor. Stopruntimecleanupundoowned.
- GET /api/proxy/capture -> {active:boolean,clientIPv4?,clientIPv6?,commands:number}.

UI: actualrouterpages dev/WiFi/DNS/firewall snapshot observations; traffic chart optional `samples` + source props (themeownercoordination) bounded recent interfaceRX/TX. Proxy tabs nodes/runtime/split/diagnostics; FRPC form compiled nativeTOML server/token/tls/tunnels viaruntimeConfigure, token isuserinput private notcommitted. Runtime artifactstage visiblehash/versiondownload; editor rawnativeconfig andcanvalidate-save/start/stop. Logs concise. Formoperations showcode/resultsnotfakeplans. Keep oldproxy/frpc plan separatepreview usefulbutnotonlyactions. NoautomaticbulkLANcapture: explicitIPscope.

## Full control and commit semantics (user clarification)

The target is a full control panel. Read-only observation is only the already-deployed bootstrap phase, not product scope. Router settings use draft -> diff/validate -> commit. Typing/editing/staging never modifies live UCI files or reloads services. Commit is the explicit application boundary, separate from Git commits.

`internal/control` owns RN02 configuration transactions for whitelisted native UCI documents network/wireless/dhcp/firewall/system/dropbear. Drafts store private copies under DataDir; raw native document editing provides complete section access in addition to typed UI forms. Validation uses fixed UCI `-c <isolated directory>` parsing (never import into live namespace), plus domain field bounds and cross-field checks. No generic shell/file execution surface. Atomic private snapshots and journal before live write; all changed docs install atomically and reload through fixed module-owned argv, rollback all if verification fails. Exclusive mutation gate + expected generation prevents competing commits. Restart recovery sees interrupted journal and restores accepted prior settings before marking ready.

Risk classification applies to management/LAN address/netmask, management service disable/port/auth, WAN-interface replacement, firewall default input/forwarding and broad deny policies, and disabling all primary radios. Commit receives explicit risks acknowledgment when required. Reachability-sensitive commits are provisional: deadline persisted, browser verifies through new/current address and sends confirm; on timeout rollback restores prior docs and reloads. Non-risk modifications can commit after validation with no repeated confirmation. Avoid unnecessary prompts or defensive banners; concise diff, risk code, operation logs.

Configuration endpoints, authenticated:
- GET /api/configuration -> {generation:number,documents:[{module:"network"|"wireless"|"dhcp"|"firewall"|"system"|"dropbear",content:string}],pendingCommit?:{id,deadline}}. Contains native private configuration only to authenticated admin, never logs/publicfixtures.
- POST /api/configuration/stage {module:string,content:string,generation:number} -> Draft {id,module,generation,diff:string,risks:[{code,message}],valid:boolean,errors:[{code,message}],createdAt:string}. Stage preserves previous pending drafts; no live apply.
- GET /api/configuration/drafts -> {drafts:[Draft]}; DELETE /api/configuration/drafts?id=... removes own draft.
- POST /api/configuration/commit {draftIds:[string],generation:number,acknowledgeRisks:boolean} -> {id,state:"committed"|"pending_confirmation"|"rolled_back",generation,deadline?,changedModules:[string]}.
- POST /api/configuration/confirm {id:string} -> same operation status.
- POST /api/configuration/rollback {id:string} -> same operation status.
- GET /api/configuration/status -> {enabled:boolean,generation:number,pendingCommit?:...}.

UI module configuration editors provide all native documents with section-aware navigation, diff inspection, staged count, commit action and risk acknowledgment dialog. Full expert raw editing is available; common forms can layer on same draft API. Modules show observed + configured separately, no forced read-only badge once write-enabled. GitHub source stays credential-free. Runtime Start/Stop remains explicit immediate process operation; runtime config Configure internally stages/checks/commits private service config, but router UCI edits all use the Commit boundary.

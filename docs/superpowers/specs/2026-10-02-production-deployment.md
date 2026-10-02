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

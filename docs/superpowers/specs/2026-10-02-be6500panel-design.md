# be6500panel modular router control platform

## Product

be6500panel is a self-hosted, professional router control plane built for our own Xiaomi BE6500 RN02. Proxy is one domain module, not the application identity. The long-term UI replaces the factory daily management surface; factory Web/SSH remain fallback until real-device acceptance. This first delivery is a runnable public source foundation, not production network control. No deployment to the router, no changes to its network, no public credentials or device snapshots.

## Runtime architecture

A single Go process serves typed JSON APIs, an SSE event stream, and prebuilt browser assets. No Node runtime, containers, database server, or periodic subprocess tree on the router. Linux ARMv7 static build is mandatory. React/TypeScript, Effect, Tailwind CSS, shadcn-style local UI components, and Motion execute in the browser. Motion respects reduced-motion; polling/stream work pauses or backs off when the browser is hidden/disconnected. Build dependencies exist only on developer machines.

## Module boundary

Core owns module registry, authentication/session boundary, HTTP/event transport, operation coordination and small persisted configuration. Modules own domain contracts, observation, validation, planning and their local lifecycle. Modules declare capabilities and availability; unsupported actions are visible but never fabricated. Built-in modules are linked at compile time with explicit registration; no dynamic shared-library loader or RPC microservices. UI modules explicitly register page, navigation, commands and summary widgets. Shared components and request transport are outside domain modules.

Initial domains: system, network, devices, wifi, dns, firewall, proxy. System observation is implemented for the actual host, and deterministic demo mode is distinctly labeled. The remaining first-delivery modules expose supported status/capabilities and structured unsupported operations rather than pretending to control a router. Proxy provides a credential-free plan/validation endpoint demonstrating coordination, not a running proxy. Observation adapters are distinct from domain contracts; Xiaomi QSDK specifics never leak into core.

Network-changing resources have one owner: network owns interfaces/routes/policy routing; firewall owns chains/marks/forwarding policy; dns owns resolver path; proxy requests contributions. Future apply uses serialized validate -> plan -> capture -> apply -> verify -> commit, with rollback and management reachability protected. First delivery only validates/plans and rejects apply with clear unsupported status. A generation identifier prevents stale-plan execution; this is runtime concurrency, not a public schema version.

## Backend HTTP contract

- GET /api/health: {status:"ok",mode:"demo"|"host",readOnly:true}
- GET /api/modules: {modules:[{id,title,description,state:"ready"|"unavailable",capabilities:[{id,title,supported,reason?}]}]}
- GET /api/system: {mode,hostname,os,arch,kernel,uptimeSeconds,cpuCount,memory:{totalBytes,availableBytes},load:[number,number,number],sampledAt}
- GET /api/network: {interfaces:[{name,addresses:[string],up:boolean,mtu:number}],routes:[],routeObservationSupported:false}
- GET /api/devices: {devices:[],supported:false,reason:string}
- GET /api/events: SSE status events. id integer, event snapshot, data {system:<system object>,sampledAt}. Single shared sampler, bounded queues/drop stale event, heartbeat, disconnect cleanup; no new expensive poller per client.
- POST /api/proxy/plan: {mode:"split"|"global"|"direct",dnsStrategy:"split"|"direct",ipv6Policy:"follow"|"direct"|"block",failurePolicy:"direct"|"block-proxy",nodeCount:number}. Returns {id,generation,readOnly:true,summary,steps:[{module,action,detail}],warnings:[string],canApply:false}. Input has no node credential fields; reject unknown fields, bounds and oversized bodies. Cannot infer reliable domestic-vs-foreign by IP alone. Default excludes management/LAN/node endpoint and coordinates DNS/IPv6.
- POST /api/operations/apply: 501 with {error:{code:"not_implemented",message}}. No system shell or writes.
- GET /api/session: {authenticated:boolean,authRequired:boolean}; POST /api/session/login {password} creates HttpOnly SameSite=Strict cookie; POST /api/session/logout.
- Other error envelope {error:{code,message}}. Unknown API path is JSON404, never SPA fallback.

Development default: --listen 127.0.0.1:8787 --demo. Authentication optional only on loopback; non-loopback requires BE6500PANEL_PASSWORD, otherwise startup fails. Password env never returned/logged, constant-time validation, bounded attempts, authenticated API/SSE when enabled, same-origin checks for unsafe methods; no arbitrary shell/file endpoint. Static UI may serve unauthenticated login. Graceful cancellation. Go standard library except narrowly justified tooling.

## Frontend

Professional dark/light capable control shell: left module navigation, top search/command palette, breadcrumbs, connection/demo/read-only state, right-side details. Dashboard is an overview, not the application. Dense, keyboard-friendly tables, grouped controls, diagnostic context and clear availability. Chinese primary copy with exact technical protocol names. Professional restrained palette, typography, spacing, real icons; no gratuitous gradient/card wall. Effect centralizes typed request failure/retry boundary; React owns presentation; Motion only helps orientation. Shared accessible UI primitives use Radix where appropriate. App must build without internet at runtime and never fetch public CDN JS/fonts.

System/dashboard views read live backend. Network table renders actual interface observations. Device/Wi-Fi/DNS/firewall views explain integration availability without faking connected clients/config. Proxy view builds the coordinated plan via API, explicitly says simulation, no deploy button with false success. Show plan steps, warnings and resource cost assumptions as estimates, not measurements. Cmd+K opens module command palette. Layout usable at desktop/tablet/mobile, contrast/reduced motion/keyboard focus respected.

## Proxy production direction (not shipped in foundation)

Custom sing-box ARMv7 with VLESS/REALITY/Vision/uTLS/UDP. Core/tmp cache reconstructed at boot from directly reachable fixed HTTPS artifact+SHA256; /data stores small settings. No ShellCrash dependency, no large full Geo databases by default. Native local subscription parsing/conversion; never third-party converters. Split policy combines explicit direct/proxy overrides, maintained domestic domain/IP sets, foreign/default routing, and split DNS; process-name rules cannot identify forwarded clients. Separate policy for IPv6 and failure behavior. Per-device canary before LAN-wide capture; verify TCP/UDP/DNS/IPv6 and QSDK acceleration interactions. Unclassified targets and CDN domains are explicit, testable policy, not perfect country inference.

## Verification and honesty

Go tests/race/vet; API/auth/SSE/oversize/invalid inputs/unsupported apply. Frontend typecheck, unit tests and production build; screenshots/browser smoke if supported. Static ELF32 ARM build measured. Endpoints only read host data; CI never touches devices. Public GitHub source creation is authorized, remote name be6500panel under authenticated account. No private source files copied; synthetic documentation data only. Reports state delivered/unsupported/untested exactly.

## Visualization and theme separation

Charts use Apache ECharts (tree-shaken component imports through one local wrapper). No homegrown canvas chart engine. Theme is a separate tokens/provider package inside web/src/theme with light/dark mode, CSS custom properties, font/spacing/radius/motion tokens and semantic chart palette; every module, chart, tooltip and shared UI component consumes the same tokens. Theme selection persists only localStorage preference, honors prefers-color-scheme and reduced motion. No domain CSS theme literals.

Initial visualizations: time-series RX/TX/latency, per-device x time heatmap, request-phase waterfall, and latency histogram/rule-hit breakdown. They must be responsive, accessible text/table alternatives, zoom or filtered views and coherent hover detail. Graph source metadata is mandatory; demo datasets explicitly labeled with no real-device claim. Unsupported real telemetry stays unavailable, never replaced with demo data silently. Chart inputs bound to limited recent points, lazy import heavy ECharts, ResizeObserver cleanup and hidden-view work suppression. Map/waterfall render from stable chronological buckets; not new backend polling endpoints per chart.

Visualization code lives web/src/components/visualizations; frontend shell/ui module code imports their public index. Theme and charts ownership independent of page/domain implementation. First delivery dashboard supports labeled demo data charts and real system status; host-mode charts requiring unimplemented data show empty availability messages. Professional UX consistent in every surface, no separate default chart theme.

## FRPC domain module (user-requested)

Add frpc as an independent domain module and UI navigation page. First-delivery capabilities: observation reports unavailable/no daemon; validates and plans tunnel settings without running frpc or exposing ports. Native artifact acquisition/supervision can reuse future runtime services; no coupling to proxy.

POST /api/frpc/plan input: {serverAddress:string,serverPort:number,tls:boolean,transport:"tcp"|"quic",proxies:[{name:string,type:"tcp"|"udp"|"http"|"https",localAddress:string,localPort:number,remotePort?:number,domains?:string[]}]}. No token field in public plan, no echo/logging credentials. Validate server/host syntax and port bounds; unique names, tcp/udp remotePort required, http/https nonempty domains required, max64 proxies. Structured response matches OperationPlan shape in proxy with readOnly:true/canApply:false and exposure warnings. GET /api/frpc: {supported:false,running:false,reason:string,proxies:[]}. Do not fabricate remote connectivity. First-delivery frpc UI can use user-supplied nonsecret settings for actual backend validation; token placement explained as private future runtime configuration, not collected for unsupported operation. Production future includes native TOML format generation, FRPC version-pinned runtime, tunnel health and unified staged apply/revert. TLS defaults enabled; LAN services published outside must be explicit and do not expose panel automatically.

## Professional copy and diagnostics

UI is concise: one mode badge, capability states, short empty text, structured errors and an expandable log/plan view. Do not repeat disclaimer paragraphs or apologies on every page. Demo sources remain labeled; unknown state does not become successful state.

Go slog writes structured stdout and a bounded in-memory log ring (up to512 entries). GET /api/logs?limit=100 (max500) returns {entries:[{sequence,time,level,code,module,message}],capacity}. Same authentication as other APIs. No request bodies, passwords or credential logs; no unbounded disk history. UI uses filtering by level/module/text and timestamp/code visibility. One observation-error entry per state transition, not one per sample.

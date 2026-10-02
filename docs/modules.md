# Module ownership

A module is a built-in domain component with an explicit ID, capability list, observation/plan contracts and a browser registration. Modules compile into one service; they are not separate daemons or a dynamic binary plugin system.

| Module | Owns | Requests from other modules |
| --- | --- | --- |
| system | Resource/OS observations and future safe runtime lifecycle | None |
| network | Interfaces, routes, policy routing, WAN/bridge state | Firewall/DNS coordination |
| devices | Client discovery, identity and per-device intent | Network/Wi-Fi observations |
| wifi | Radio/SSID/client configuration through QSDK adapters | Device/network coordination |
| dns | Resolver paths, caches and split-DNS policy | Network/firewall routing intent |
| firewall | Chains, marks, filter/forwarding policies | Network paths and device policy |
| proxy | Subscription parsing, nodes, selection and routing intent | DNS/firewall/network contributions |
| frpc | Tunnel configuration and runtime health | Optional explicit firewall/network contributions |

## Core

Core owns transport, session boundary, registry, bounded events, small settings and coordinated operations. No module gets an arbitrary shell execution endpoint. Device-specific adapters are separate from contracts. Capability support is explicit: present in navigation does not mean implemented on a particular router.

Future mutations use validate, plan, capture, apply, verify and commit. Changes are serialized; resources have one owner; cleanup removes only owned state. A stale plan cannot apply over a newer generation. Unsupported apply currently returns a structured error rather than a false success.

## Browser extensions

Each domain registers its navigation item, page and commands. Shared UI and API transport do not belong to proxy. Theme tokens and chart components are independent layers. New modules reuse existing registration paths rather than editing one giant dashboard.

## Efficient telemetry

One shared sampler feeds bounded event subscribers. Historical chart buffers are bounded in the browser. Real flow history needs an explicit collection/storage design; sample charts remain labeled demo and are unavailable in host mode until such collectors exist.

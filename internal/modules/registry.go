// Package modules contains built-in domain modules and read-only host adapters.
package modules

import "be6500panel/internal/core"

type Unavailable struct{ ID, Title, Description, Reason string }

func (m Unavailable) Descriptor() core.Module {
	return core.Module{ID: m.ID, Title: m.Title, Description: m.Description, State: "unavailable", Capabilities: []core.Capability{{ID: "observe", Title: "Device observation", Supported: false, Reason: m.Reason}, {ID: "apply", Title: "Configuration", Supported: false, Reason: "Real-device control is not implemented."}}}
}

const DeviceReason = "No router client/lease adapter is integrated; host interfaces are not connected-device observations."
const FRPCReason = "frpc is not installed, launched or observed by this foundation; plans never expose services."

func Builtins(system *System, network Network) (*core.Registry, error) {
	registry := &core.Registry{}
	providers := []core.ModuleProvider{
		system, network,
		Unavailable{ID: "devices", Title: "Devices", Description: "Connected-device integration.", Reason: DeviceReason},
		Unavailable{ID: "wifi", Title: "Wi-Fi", Description: "Wireless radio and SSID management.", Reason: "Xiaomi/QSDK radio adapter is not integrated."},
		Unavailable{ID: "dns", Title: "DNS", Description: "Resolver path ownership.", Reason: "Resolver observation and configuration adapter is not integrated."},
		Unavailable{ID: "firewall", Title: "Firewall", Description: "Forwarding rules and mark ownership.", Reason: "Firewall observation and configuration adapter is not integrated."},
		Proxy{}, FRPC{},
	}
	for _, provider := range providers {
		if err := registry.Register(provider); err != nil {
			return nil, err
		}
	}
	return registry, nil
}

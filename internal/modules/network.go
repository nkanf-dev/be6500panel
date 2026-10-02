package modules

import (
	"context"
	"errors"
	"net"
	"sort"

	"be6500panel/internal/core"
)

type Network struct{}

func (Network) Descriptor() core.Module {
	return core.Module{ID: "network", Title: "Network", Description: "Current-host interface observation only; not router LAN configuration.", State: "ready", Capabilities: []core.Capability{{ID: "interfaces", Title: "Host interfaces", Supported: true}, {ID: "routes", Title: "Route observation", Supported: false, Reason: "Router route adapter is not integrated."}, {ID: "apply", Title: "Network configuration", Supported: false, Reason: "No network changes are implemented."}}}
}
func (Network) Observe(ctx context.Context) (core.NetworkStatus, error) {
	if err := ctx.Err(); err != nil {
		return core.NetworkStatus{}, err
	}
	interfaces, err := net.Interfaces()
	if err != nil {
		return core.NetworkStatus{}, errors.New("cannot observe host interfaces")
	}
	result := core.NetworkStatus{Interfaces: []core.Interface{}, Routes: []any{}, RouteObservationSupported: false}
	for _, iface := range interfaces {
		addresses, err := iface.Addrs()
		if err != nil {
			return core.NetworkStatus{}, errors.New("cannot observe host interface addresses")
		}
		item := core.Interface{Name: iface.Name, Addresses: []string{}, Up: iface.Flags&net.FlagUp != 0, MTU: iface.MTU}
		for _, address := range addresses {
			item.Addresses = append(item.Addresses, address.String())
		}
		sort.Strings(item.Addresses)
		result.Interfaces = append(result.Interfaces, item)
	}
	sort.Slice(result.Interfaces, func(i, j int) bool { return result.Interfaces[i].Name < result.Interfaces[j].Name })
	return result, nil
}

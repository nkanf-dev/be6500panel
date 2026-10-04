package proxy

import (
	"fmt"
	"net/netip"
	"regexp"
)

var nativeRoutedTUNInterfaceName = regexp.MustCompile(`^b6p-[A-Za-z0-9_][A-Za-z0-9_-]{0,10}$`)

func defaultRoutedTUNConfig() RoutedTUNConfig {
	return RoutedTUNConfig{InterfaceName: "b6p-tun", Address: "172.31.255.253/30"}
}

// validateNativeDatapath accepts only the current original-packet main path.
// Capture preflight must also reject overlap with observed LAN prefixes, routes
// and occupied interfaces; compilation can check only known input addresses.
func validateNativeDatapath(in CompileInput) error {
	switch in.Datapath {
	case DatapathTPROXY:
		return fmt.Errorf("tproxy datapath is no longer supported; use routed-tun")
	case DatapathRoutedTUN:
		if in.IPv6 != IPv6Direct {
			return fmt.Errorf("routed-tun supports only IPv6 direct; IPv6 follow and block are not supported")
		}
	default:
		return fmt.Errorf("invalid datapath mode")
	}

	config := *in.RoutedTUN
	// The owned prefix excludes factory LAN/bridge/ethernet interfaces. The
	// restricted suffix also excludes aliases, wildcards and command syntax.
	if !nativeRoutedTUNInterfaceName.MatchString(config.InterfaceName) {
		return fmt.Errorf("routed-tun interface requires an owned b6p- name of 5 to 15 safe characters")
	}
	prefix, err := netip.ParsePrefix(config.Address)
	if err != nil || prefix.Bits() != 30 || !prefix.Addr().Is4() || !prefix.Addr().IsPrivate() || prefix.String() != config.Address {
		return fmt.Errorf("routed-tun address requires a literal RFC1918 IPv4 /30 first usable host with a usable next peer")
	}
	// A /30's first host and the next address are its two usable hosts. Keep
	// the host bits in the native address; never replace it with the network.
	network := prefix.Masked()
	if prefix.Addr() != network.Addr().Next() {
		return fmt.Errorf("routed-tun address requires the first usable /30 host with a usable next peer")
	}
	return nil
}

// validateNativeRoutedTUNCollisions runs after defaults and input limits. The
// connected /30 must not take over any known literal endpoint or router bind.
// Hostnames remain unresolved; compilation never performs network observation.
func validateNativeRoutedTUNCollisions(in CompileInput, localDNS LocalDNSConfig) error {
	prefix, _ := netip.ParsePrefix(in.RoutedTUN.Address) // validated above
	network := prefix.Masked()
	contains := func(value string) bool {
		address, err := netip.ParseAddr(value)
		return err == nil && network.Contains(address.Unmap())
	}
	for _, value := range in.ManagementIPs {
		if contains(value) {
			return fmt.Errorf("routed-tun address prefix overlaps router management")
		}
	}
	for _, value := range []string{in.Node.Server, in.DirectDNS.Server, in.ProxyDNS.Server, localDNS.Server, in.MixedListenAddress, in.DNSListenAddress} {
		if contains(value) {
			return fmt.Errorf("routed-tun address prefix overlaps a known endpoint or listener")
		}
	}
	for _, value := range in.Endpoints {
		if contains(value) {
			return fmt.Errorf("routed-tun address prefix overlaps a known endpoint or listener")
		}
	}
	return nil
}

// nativeRoutedTUNInbound admits original packets directly into the main core.
// Its system stack needs no gVisor build, automatic route ownership or SOCKS
// sidecar. The explicit route planner, not this inbound, captures client scope.
func nativeRoutedTUNInbound(config RoutedTUNConfig) map[string]any {
	return map[string]any{
		"type":           "tun",
		"tag":            "tun-in",
		"interface_name": config.InterfaceName,
		"address":        []string{config.Address},
		"mtu":            1500,
		"dns_mode":       "disabled",
		"auto_route":     false,
		"auto_redirect":  false,
		"stack":          "system",
		"udp_timeout":    "2m",
		"udp_nat_max":    1024,
	}
}

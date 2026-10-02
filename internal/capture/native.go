package capture

import (
	"context"
	"encoding/json"
	"errors"
	"net"
	"net/netip"
	"slices"

	"be6500panel/internal/proxy"
	"be6500panel/internal/router"
)

type EndpointResolver func(context.Context, string) ([]string, error)

// BuildFromAccepted extracts listener intent from the actual accepted native
// config. Selection metadata and journals never supply ports or endpoints.
func BuildFromAccepted(ctx context.Context, d Desired, raw []byte, observed router.CaptureObservation, resolve EndpointResolver) (proxy.RulesPlanInput, []Client, error) {
	clients := desiredClients(d)
	bad := func(code string) (proxy.RulesPlanInput, []Client, error) {
		return proxy.RulesPlanInput{}, clients, errors.New(code)
	}
	if len(observed.ManagementIPs) == 0 || len(observed.LANPrefixes) == 0 {
		return bad("capture_lan_unavailable")
	}
	addresses := []string{}
	pendingDevices := false
	if len(d.Devices) > 0 {
		unresolved := false
		for i, selection := range d.Devices {
			matches := []router.Device{}
			for _, device := range observed.Devices {
				if device.MAC == selection.MAC && device.Eligible {
					matches = append(matches, device)
				}
			}
			if len(matches) != 1 {
				unresolved = true
				continue
			}
			clients[i] = Client{MAC: selection.MAC, IP: matches[0].IP, Hostname: matches[0].Hostname}
			addresses = append(addresses, matches[0].IP)
		}
		if unresolved {
			if len(addresses) == 0 {
				return bad("capture_device_unresolved")
			}
			pendingDevices = true
		}
	} else {
		// A literal has no stable identity. Keep its input readable but never
		// restore it across a lifecycle without an explicit observed MAC.
		return bad("capture_client_identity_required")
	}
	for _, address := range addresses {
		if !insideLAN(address, observed.LANPrefixes) || slices.Contains(observed.ManagementIPs, address) {
			return bad("capture_client_not_lan")
		}
	}
	uniqueAddresses := slices.Clone(addresses)
	slices.Sort(uniqueAddresses)
	if len(slices.Compact(uniqueAddresses)) != len(addresses) {
		return bad("capture_device_conflict")
	}
	var native struct {
		Inbounds []struct {
			Type     string `json:"type"`
			Tag      string `json:"tag"`
			Listen   string `json:"listen"`
			Port     uint16 `json:"listen_port"`
			Network  string `json:"network"`
			IPv6Only bool   `json:"ipv6_only"`
		} `json:"inbounds"`
		Outbounds []struct {
			Server string `json:"server"`
		} `json:"outbounds"`
		DNS struct {
			Servers []struct {
				Type   string `json:"type"`
				Server string `json:"server"`
				Detour string `json:"detour"`
			} `json:"servers"`
		} `json:"dns"`
		Route struct {
			Rules []struct {
				Inbound   []string `json:"inbound"`
				Action    string   `json:"action"`
				Outbound  string   `json:"outbound"`
				IPVersion int      `json:"ip_version"`
			} `json:"rules"`
		} `json:"route"`
	}
	if json.Unmarshal(raw, &native) != nil {
		return bad("capture_native_invalid")
	}
	input := proxy.RulesPlanInput{LANInterface: "br-lan", IPv6: d.IPv6, Failure: proxy.FailureDirect, ManagementIPs: slices.Clone(observed.ManagementIPs), RouterDNSAddresses: slices.Clone(observed.LANAddresses), ClientIPv6: d.ClientIPv6}
	if len(addresses) == 1 {
		input.ClientIPv4 = addresses[0]
	} else {
		input.ClientIPv4s = addresses
	}
	dnsBind, tproxyBind := "", ""
	seen := map[string]bool{}
	for _, inbound := range native.Inbounds {
		key := ""
		switch {
		case inbound.Type == "mixed":
			key = "mixed"
		case inbound.Type == "tproxy":
			key = "tproxy"
		case inbound.Type == "direct" && inbound.Tag == "dns-in":
			key = "dns"
		default:
			continue
		}
		if seen[key] || inbound.Port == 0 || inbound.IPv6Only {
			return bad("capture_listener_invalid")
		}
		seen[key] = true
		if key != "mixed" && inbound.Network != "" {
			return bad("capture_listener_tcp_udp_required")
		}
		listen, err := netip.ParseAddr(inbound.Listen)
		if err != nil || listen.Zone() != "" || listen.Is4In6() {
			return bad("capture_listener_invalid")
		}
		switch key {
		case "mixed":
			input.Ports.Mixed = inbound.Port
		case "tproxy":
			input.Ports.TProxy = inbound.Port
			tproxyBind = listen.String()
		case "dns":
			input.Ports.DNS = inbound.Port
			dnsBind = listen.String()
		}
	}
	if len(seen) != 3 {
		return bad("capture_listener_missing")
	}
	if tproxyBind != "127.0.0.1" && tproxyBind != "0.0.0.0" && tproxyBind != "::" {
		return bad("capture_tproxy_bind_mismatch")
	}
	dnsAddress, _ := netip.ParseAddr(dnsBind)
	if dnsBind != "0.0.0.0" && dnsBind != "::" && (!dnsAddress.Is4() || !slices.Contains(observed.LANAddresses, dnsBind)) {
		return bad("capture_dns_bind_mismatch")
	}
	dnsHijack := false
	acceptedIPv6 := proxy.IPv6Follow
	for _, rule := range native.Route.Rules {
		if rule.Action == "hijack-dns" && slices.Contains(rule.Inbound, "dns-in") {
			dnsHijack = true
		}
		if rule.IPVersion == 6 && len(rule.Inbound) == 0 {
			if rule.Outbound == "direct" {
				acceptedIPv6 = proxy.IPv6Direct
			}
			if rule.Action == "reject" {
				acceptedIPv6 = proxy.IPv6Block
			}
		}
	}
	if !dnsHijack {
		return bad("capture_dns_route_missing")
	}
	if acceptedIPv6 != d.IPv6 {
		return bad("capture_ipv6_policy_mismatch")
	}
	if d.IPv6 == proxy.IPv6Follow && (tproxyBind != "::" || dnsBind != "::") {
		return bad("capture_ipv6_listener_mismatch")
	}
	endpoints := []string{}
	for _, outbound := range native.Outbounds {
		if outbound.Server != "" {
			endpoints = append(endpoints, outbound.Server)
		}
	}
	for _, server := range native.DNS.Servers {
		if server.Type == "fakeip" {
			input.FakeIP = true
		}
		if server.Detour == "direct" && server.Server != "" {
			endpoints = append(endpoints, server.Server)
		}
	}
	if len(endpoints) > 256 {
		return bad("capture_endpoint_limit")
	}
	for _, host := range endpoints {
		if address, err := netip.ParseAddr(host); err == nil && address.Zone() == "" && !address.Is4In6() {
			input.EndpointIPs = append(input.EndpointIPs, address.String())
			continue
		}
		if resolve == nil {
			return bad("capture_endpoint_unresolved")
		}
		ips, err := resolve(ctx, host)
		if err != nil || len(ips) == 0 {
			return bad("capture_endpoint_unresolved")
		}
		input.EndpointIPs = append(input.EndpointIPs, ips...)
	}
	slices.Sort(input.EndpointIPs)
	input.EndpointIPs = slices.Compact(input.EndpointIPs)
	if _, err := proxy.PlanOwnedRules(input); err != nil {
		return bad("capture_native_scope_invalid")
	}
	if pendingDevices {
		return input, clients, &PartialScopeError{}
	}
	return input, clients, nil
}

// PartialScopeError is a warning: input contains only current resolved devices.
// No stale address is substituted for an unresolved selected MAC.
type PartialScopeError struct{}

func (*PartialScopeError) Error() string { return "capture_devices_pending" }

func insideLAN(raw string, prefixes []string) bool {
	addr, err := netip.ParseAddr(raw)
	if err != nil || !addr.Is4() {
		return false
	}
	for _, rawPrefix := range prefixes {
		prefix, err := netip.ParsePrefix(rawPrefix)
		if err != nil || !prefix.Contains(addr) {
			continue
		}
		if prefix.Bits() <= 30 {
			network := prefix.Masked().Addr().As4()
			ip := addr.As4()
			n := uint32(network[0])<<24 | uint32(network[1])<<16 | uint32(network[2])<<8 | uint32(network[3])
			value := uint32(ip[0])<<24 | uint32(ip[1])<<16 | uint32(ip[2])<<8 | uint32(ip[3])
			broadcast := n | (^uint32(0) >> prefix.Bits())
			if value == n || value == broadcast {
				continue
			}
		}
		return true
	}
	return false
}
func ResolveEndpoints(ctx context.Context, host string) ([]string, error) {
	ips, err := net.DefaultResolver.LookupNetIP(ctx, "ip", host)
	if err != nil {
		return nil, err
	}
	result := []string{}
	for _, ip := range ips {
		if ip.Zone() == "" {
			result = append(result, ip.Unmap().String())
		}
	}
	return result, nil
}

package capture

import (
	"context"
	"encoding/json"
	"errors"
	"net"
	"net/netip"
	"regexp"
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
		Inbounds  []json.RawMessage `json:"inbounds"`
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
			Rules []json.RawMessage `json:"rules"`
		} `json:"route"`
	}
	if json.Unmarshal(raw, &native) != nil {
		return bad("capture_native_invalid")
	}
	input := proxy.RulesPlanInput{LANInterface: "br-lan", IPv6: d.IPv6, Failure: proxy.FailureDirect, ManagementIPs: slices.Clone(observed.ManagementIPs), RouterDNSAddresses: captureRouterDNSAddresses(observed.LANAddresses, d.IPv6), ClientIPv6: d.ClientIPv6}
	if len(addresses) == 1 {
		input.ClientIPv4 = addresses[0]
	} else {
		input.ClientIPv4s = addresses
	}
	// Enforce both current IP and authorized layer-2 identity at the hook.
	// A DHCP address reused between observations cannot intercept a new MAC.
	input.ClientMACs = make(map[string]string, len(addresses)+1)
	for _, client := range clients {
		if client.IP != "" {
			input.ClientMACs[client.IP] = client.MAC
		}
	}
	if d.IPv6 != proxy.IPv6Direct && len(d.Devices) == 1 {
		input.ClientMACs[d.ClientIPv6] = d.Devices[0].MAC
	}
	dnsBind, tproxyBind := "", ""
	seen := map[string]bool{}
	for _, rawInbound := range native.Inbounds {
		var inbound acceptedInbound
		if json.Unmarshal(rawInbound, &inbound) != nil {
			return bad("capture_native_invalid")
		}
		if inbound.Type == "tun" {
			if seen["tun"] || seen["tproxy"] {
				return bad("capture_datapath_ambiguous")
			}
			if err := acceptedTUN(rawInbound, inbound, observed); err != nil {
				return bad(err.Error())
			}
			seen["tun"] = true
			input.Datapath = proxy.DatapathRoutedTUN
			input.TUNInterface, input.TUNAddress = inbound.InterfaceName, inbound.Address[0]
			// This backend has no TPROXY listener. Reserve the legacy native
			// compiler port only for shared port-conflict validation.
			input.Ports.TProxy = 7893
			continue
		}
		key := ""
		switch {
		case inbound.Type == "mixed":
			key = "mixed"
		case inbound.Type == "tproxy":
			if seen["tun"] {
				return bad("capture_datapath_ambiguous")
			}
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
	if seen["tun"] && d.IPv6 != proxy.IPv6Direct {
		return bad("capture_tun_ipv6_unsupported")
	}
	if seen["tproxy"] && tproxyBind != "127.0.0.1" && tproxyBind != "0.0.0.0" && tproxyBind != "::" {
		return bad("capture_tproxy_bind_mismatch")
	}
	dnsAddress, _ := netip.ParseAddr(dnsBind)
	if dnsBind != "0.0.0.0" && dnsBind != "::" && (!dnsAddress.Is4() || !slices.Contains(observed.LANAddresses, dnsBind)) {
		return bad("capture_dns_bind_mismatch")
	}
	dnsHijack := false
	acceptedIPv6 := proxy.IPv6Follow
	acceptedIPv6Explicit := false
	for _, rawRule := range native.Route.Rules {
		var rule struct {
			Inbound   []string `json:"inbound"`
			Action    string   `json:"action"`
			Outbound  string   `json:"outbound"`
			IPVersion int      `json:"ip_version"`
		}
		if json.Unmarshal(rawRule, &rule) != nil {
			return bad("capture_native_invalid")
		}
		if rule.Action == "hijack-dns" && slices.Contains(rule.Inbound, "dns-in") {
			dnsHijack = true
		}
		if rule.IPVersion == 6 && len(rule.Inbound) == 0 {
			if seen["tun"] {
				var fields map[string]json.RawMessage
				if json.Unmarshal(rawRule, &fields) != nil || len(fields) != 2 || fields["outbound"] == nil || rule.Outbound != "direct" {
					return bad("capture_ipv6_policy_mismatch")
				}
			}
			if rule.Outbound == "direct" {
				acceptedIPv6 = proxy.IPv6Direct
				acceptedIPv6Explicit = true
			}
			if rule.Action == "reject" {
				acceptedIPv6 = proxy.IPv6Block
				acceptedIPv6Explicit = true
			}
		}
	}
	if seen["tun"] {
		prefix, _ := captureTUNPrefix(input.TUNAddress)
		// Keep proxy-DNS and mixed-listener semantics unchanged, but reject a
		// known accepted literal that the connected private /30 would steal.
		for _, inboundRaw := range native.Inbounds {
			var inbound acceptedInbound
			_ = json.Unmarshal(inboundRaw, &inbound)
			if addr, err := netip.ParseAddr(inbound.Listen); err == nil && prefix.Contains(addr.Unmap()) {
				return bad("capture_tun_prefix_collision")
			}
		}
		for _, server := range native.DNS.Servers {
			if addr, err := netip.ParseAddr(server.Server); err == nil && prefix.Contains(addr.Unmap()) {
				return bad("capture_tun_prefix_collision")
			}
		}
	}
	if !dnsHijack {
		return bad("capture_dns_route_missing")
	}
	if acceptedIPv6 != d.IPv6 || (seen["tun"] && !acceptedIPv6Explicit) {
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

// acceptedInbound retains only capture intent. TUN's raw object is also
// checked against a closed field set before trusting its interface or address.
type acceptedInbound struct {
	Type          string   `json:"type"`
	Tag           string   `json:"tag"`
	Listen        string   `json:"listen"`
	Port          uint16   `json:"listen_port"`
	Network       string   `json:"network"`
	IPv6Only      bool     `json:"ipv6_only"`
	InterfaceName string   `json:"interface_name"`
	Address       []string `json:"address"`
	MTU           int      `json:"mtu"`
	Stack         string   `json:"stack"`
	DNSMode       string   `json:"dns_mode"`
	AutoRoute     *bool    `json:"auto_route"`
	AutoRedirect  *bool    `json:"auto_redirect"`
	UDPTimeout    string   `json:"udp_timeout"`
	UDPNATMax     int      `json:"udp_nat_max"`
}

var captureTUNInterface = regexp.MustCompile(`^b6p-[A-Za-z0-9_][A-Za-z0-9_-]{0,10}$`)

func acceptedTUN(raw []byte, inbound acceptedInbound, observed router.CaptureObservation) error {
	var fields map[string]json.RawMessage
	if json.Unmarshal(raw, &fields) != nil {
		return errors.New("capture_tun_invalid")
	}
	for key := range fields {
		switch key {
		case "type", "tag", "interface_name", "address", "mtu", "stack", "dns_mode", "auto_route", "auto_redirect", "udp_timeout", "udp_nat_max":
		default:
			return errors.New("capture_tun_invalid")
		}
	}
	if inbound.Tag != "tun-in" || !captureTUNInterface.MatchString(inbound.InterfaceName) ||
		len(inbound.Address) != 1 || inbound.MTU != 1500 || inbound.Stack != "system" || inbound.DNSMode != "disabled" ||
		inbound.AutoRoute == nil || *inbound.AutoRoute || inbound.AutoRedirect == nil || *inbound.AutoRedirect ||
		inbound.UDPTimeout != "2m" || inbound.UDPNATMax != 1024 {
		return errors.New("capture_tun_invalid")
	}
	prefix, err := captureTUNPrefix(inbound.Address[0])
	if err != nil {
		return err
	}
	for _, value := range observed.LANPrefixes {
		lan, err := netip.ParsePrefix(value)
		if err != nil {
			return errors.New("capture_lan_unavailable")
		}
		if prefix.Overlaps(lan) {
			return errors.New("capture_tun_prefix_collision")
		}
	}
	for _, value := range observed.ManagementIPs {
		addr, err := netip.ParseAddr(value)
		if err != nil || addr.Zone() != "" || addr.Is4In6() {
			return errors.New("capture_lan_unavailable")
		}
		if prefix.Contains(addr) {
			return errors.New("capture_tun_prefix_collision")
		}
	}
	return nil
}

// captureTUNPrefix validates the private /30 host and its next usable peer.
// It returns the entire prefix: collision checks must not test just the host.
func captureTUNPrefix(value string) (netip.Prefix, error) {
	prefix, err := netip.ParsePrefix(value)
	if err != nil || !prefix.Addr().Is4() || prefix.Bits() != 30 || !prefix.Addr().IsPrivate() || prefix.String() != value {
		return netip.Prefix{}, errors.New("capture_tun_address_invalid")
	}
	network := prefix.Masked()
	if prefix.Addr() != network.Addr().Next() || !network.Contains(prefix.Addr().Next()) {
		return netip.Prefix{}, errors.New("capture_tun_address_invalid")
	}
	return network, nil
}

// PartialScopeError is a warning: input contains only current resolved devices.
// No stale address is substituted for an unresolved selected MAC.
type PartialScopeError struct{}

func (*PartialScopeError) Error() string { return "capture_devices_pending" }

// Router DNS exceptions are only for addresses supported by the active capture
// family. Link-local DNS stays on dnsmasq: scoped addresses cannot be safely
// expressed by the owned exact-address NAT plan.
func captureRouterDNSAddresses(addresses []string, ipv6 proxy.IPv6Mode) []string {
	out := []string{}
	for _, value := range addresses {
		addr, err := netip.ParseAddr(value)
		if err != nil || addr.Zone() != "" || addr.Is4In6() || !addr.IsGlobalUnicast() || addr.IsLoopback() || addr.IsLinkLocalUnicast() {
			continue
		}
		if addr.Is6() && ipv6 != proxy.IPv6Follow {
			continue
		}
		out = append(out, addr.String())
	}
	return out
}

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

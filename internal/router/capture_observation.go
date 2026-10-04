package router

import (
	"context"
	"errors"
	"net"
	"net/netip"
	"sort"
	"strings"
	"time"
)

var errCaptureSources = errors.New("Current capture identity sources are unavailable or invalid.")
var errCaptureScope = errors.New("Current br-lan IPv4 scope is unavailable.")

// CaptureObservation reads only current leases, ARP, IPv4 routes, and interface
// addresses. It bypasses both Snapshot caches and never reads firewall state.
// Any unavailable or invalid identity source makes every device ineligible and
// returns an error. Valid rows remain visible for diagnostics, not authorization.
func (a *Adapter) CaptureObservation(ctx context.Context) (CaptureObservation, error) {
	empty := CaptureObservation{Devices: []Device{}, LANPrefixes: []string{}, LANAddresses: []string{}, ManagementIPs: []string{}}
	if err := ctx.Err(); err != nil {
		return empty, err
	}
	a.mu.Lock()
	defer a.mu.Unlock()
	if err := ctx.Err(); err != nil {
		return empty, err
	}
	observation, _, _, sourceErrors := a.observeCaptureSources(ctx, a.now())
	if err := ctx.Err(); err != nil {
		return observation, err
	}
	if len(sourceErrors) != 0 {
		return observation, errCaptureSources
	}
	if len(observation.LANPrefixes) == 0 {
		return observation, errCaptureScope
	}
	return observation, nil
}

// observeCaptureSources is shared with Snapshot so UI eligibility and fresh
// capture decisions use the same identity rules. Snapshot retains partial rows
// and module errors while CaptureObservation fails closed.
func (a *Adapter) observeCaptureSources(ctx context.Context, now time.Time) (CaptureObservation, int, []Route, []ModuleError) {
	observation := CaptureObservation{Devices: []Device{}, LANPrefixes: []string{}, LANAddresses: []string{}, ManagementIPs: []string{}}
	leases, arp := []Device{}, []Device{}
	routes := []Route{}
	errs := []ModuleError{}
	if ctx.Err() != nil {
		return observation, 0, routes, errs
	}
	data, err := a.firstFile([]string{"/tmp/dhcp.leases", "/tmp/dnsmasq.leases", "/var/lib/misc/dnsmasq.leases"}, fileLimit)
	if err != nil {
		errs = append(errs, moduleError("devices.leases", sourceCode(err)))
	} else {
		var bad bool
		leases, bad = parseLeases(data, now)
		if bad {
			errs = append(errs, moduleError("devices.leases", "invalid"))
		}
	}
	data, err = a.readFile("/proc/net/arp", procLimit)
	if err != nil {
		errs = append(errs, moduleError("devices.arp", sourceCode(err)))
	} else {
		var bad bool
		arp, bad = parseARP(data)
		if bad {
			errs = append(errs, moduleError("devices.arp", "invalid"))
		}
	}
	data, err = a.readFile("/proc/net/route", procLimit)
	if err != nil {
		errs = append(errs, moduleError("routes.ipv4", sourceCode(err)))
	} else {
		var bad bool
		routes, bad = parseIPv4Routes(data)
		if bad {
			errs = append(errs, moduleError("routes.ipv4", "invalid"))
		}
	}
	addresses, err := a.captureInterfaceAddresses(ctx)
	if err != nil {
		code := sourceCode(err)
		if errors.Is(err, errCaptureSources) {
			code = "invalid"
		}
		errs = append(errs, moduleError("devices.interfaces", code))
	}
	observation.LANPrefixes, observation.LANAddresses, observation.ManagementIPs = captureScope(addresses, routes)
	for _, address := range addresses {
		observation.InterfaceAddresses = append(observation.InterfaceAddresses, CaptureInterfaceAddress{Interface: address.name, Address: address.prefix.String()})
	}
	observation.Devices = captureDevices(leases, arp, observation.LANPrefixes, observation.ManagementIPs)
	if len(errs) != 0 || ctx.Err() != nil {
		for i := range observation.Devices {
			observation.Devices[i].Eligible = false
		}
	}
	return observation, len(leases), routes, errs
}

type interfaceAddress struct {
	name   string
	prefix netip.Prefix
}

func (a *Adapter) captureInterfaceAddresses(ctx context.Context) ([]interfaceAddress, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	if !a.live {
		data, err := a.readFile("/etc/config/network", fileLimit)
		if err != nil {
			return nil, err
		}
		return fixtureInterfaceAddresses(data)
	}
	interfaces, err := net.Interfaces()
	if err != nil {
		return nil, err
	}
	out := []interfaceAddress{}
	for _, iface := range interfaces {
		if err := ctx.Err(); err != nil {
			return nil, err
		}
		addresses, err := iface.Addrs()
		if err != nil {
			return nil, err
		}
		for _, address := range addresses {
			prefix, err := netip.ParsePrefix(address.String())
			if err != nil {
				return nil, errCaptureSources
			}
			out = append(out, interfaceAddress{name: iface.Name, prefix: prefix})
		}
	}
	return out, nil
}

// Only fixture roots consult configured addresses. Live capture always uses
// actual interface addresses, not potentially stale UCI configuration.
func fixtureInterfaceAddresses(data []byte) ([]interfaceAddress, error) {
	fields := map[string]bool{"device": true, "ifname": true, "ipaddr": true, "netmask": true, "ip6addr": true}
	sections, bad := parseUCI(data, fields)
	if bad {
		return nil, errCaptureSources
	}
	out := []interfaceAddress{}
	for _, section := range sections {
		if section.kind != "interface" {
			continue
		}
		name := section.options["device"]
		if name == "" {
			name = section.options["ifname"]
		}
		if section.name == "lan" && section.options["device"] == "" {
			// Older OpenWrt fixtures name bridge members in ifname rather than
			// the effective bridge. Do not override an explicit runtime device.
			name = "br-lan"
		}
		if name == "" {
			name = section.name
		}
		if !validInterface(name) {
			return nil, errCaptureSources
		}
		for _, value := range fixtureAddressValues(section, "ipaddr") {
			prefix, err := fixtureIPv4Prefix(value, section.options["netmask"])
			if err != nil {
				return nil, err
			}
			out = append(out, interfaceAddress{name: name, prefix: prefix})
		}
		for _, value := range fixtureAddressValues(section, "ip6addr") {
			prefix, err := netip.ParsePrefix(value)
			if err != nil || !prefix.Addr().Is6() || prefix.Addr().IsUnspecified() || prefix.Addr().IsMulticast() {
				return nil, errCaptureSources
			}
			out = append(out, interfaceAddress{name: name, prefix: prefix})
		}
	}
	return out, nil
}

func fixtureAddressValues(section uciSection, field string) []string {
	out := strings.Fields(section.options[field])
	for _, value := range section.lists[field] {
		out = append(out, strings.Fields(value)...)
	}
	return out
}

func fixtureIPv4Prefix(value, maskValue string) (netip.Prefix, error) {
	prefix, err := netip.ParsePrefix(value)
	if err != nil {
		ip, ipErr := netip.ParseAddr(value)
		if ipErr != nil || !ip.Is4() {
			return netip.Prefix{}, errCaptureSources
		}
		// Missing masks do not invent a subnet. The address remains a
		// management identity; an actual connected route can supply scope.
		prefix = netip.PrefixFrom(ip, 32)
	}
	if !prefix.Addr().Is4() || prefix.Addr().IsUnspecified() || prefix.Addr().IsMulticast() {
		return netip.Prefix{}, errCaptureSources
	}
	if maskValue != "" {
		mask, err := netip.ParseAddr(maskValue)
		if err != nil || !mask.Is4() {
			return netip.Prefix{}, errCaptureSources
		}
		bits, ok := maskBits(mask.As4())
		if !ok || (strings.Contains(value, "/") && bits != prefix.Bits()) {
			return netip.Prefix{}, errCaptureSources
		}
		prefix = netip.PrefixFrom(prefix.Addr(), bits)
	}
	return prefix, nil
}

func captureScope(addresses []interfaceAddress, routes []Route) ([]string, []string, []string) {
	prefixes := map[string]bool{}
	lan := map[string]bool{}
	management := map[string]bool{}
	for _, address := range addresses {
		ip := address.prefix.Addr()
		if !ip.IsUnspecified() && !ip.IsMulticast() {
			management[ip.String()] = true
			if address.name == "br-lan" {
				lan[ip.String()] = true
			}
		}
		if address.name == "br-lan" && ip.Is4() && !ip.IsLoopback() && address.prefix.Bits() > 0 && address.prefix.Bits() < 32 {
			prefixes[address.prefix.Masked().String()] = true
		}
	}
	for _, route := range routes {
		if route.Family != "ipv4" || route.Interface != "br-lan" || route.Gateway != "0.0.0.0" {
			continue
		}
		prefix, err := netip.ParsePrefix(route.Destination)
		if err == nil && prefix.Addr().Is4() && !prefix.Addr().IsLoopback() && prefix.Bits() > 0 && prefix.Bits() < 32 {
			prefixes[prefix.Masked().String()] = true
		}
	}
	return sortedKeys(prefixes), sortedKeys(lan), sortedKeys(management)
}

func sortedKeys(values map[string]bool) []string {
	out := make([]string, 0, len(values))
	for value := range values {
		out = append(out, value)
	}
	sort.Strings(out)
	return out
}

func captureDevices(leases, arp []Device, scope, management []string) []Device {
	out := mergeDevices(leases, arp)
	leaseCount := map[string]int{}
	ipOwners := map[string]map[string]bool{}
	macIPs := map[string]map[string]bool{}
	interfaces := map[string]map[string]bool{}
	for _, d := range leases {
		leaseCount[d.MAC]++
	}
	// Ownership conflicts survive stale-ARP suppression. A current lease
	// does not authorize capturing an IP claimed by a different MAC.
	for _, rows := range [][]Device{leases, arp} {
		for _, d := range rows {
			if ipOwners[d.IP] == nil {
				ipOwners[d.IP] = map[string]bool{}
			}
			ipOwners[d.IP][d.MAC] = true
		}
	}
	for _, d := range arp {
		key := d.IP + "/" + d.MAC
		if interfaces[key] == nil {
			interfaces[key] = map[string]bool{}
		}
		interfaces[key][d.Interface] = true
	}
	for _, d := range out {
		if macIPs[d.MAC] == nil {
			macIPs[d.MAC] = map[string]bool{}
		}
		macIPs[d.MAC][d.IP] = true
	}
	local := map[string]bool{}
	for _, ip := range management {
		local[ip] = true
	}
	prefixes := []netip.Prefix{}
	for _, value := range scope {
		if prefix, err := netip.ParsePrefix(value); err == nil {
			prefixes = append(prefixes, prefix)
		}
	}
	for i := range out {
		d := &out[i]
		ip, err := netip.ParseAddr(d.IP)
		if err != nil || !ip.Is4() || !ip.IsGlobalUnicast() || local[d.IP] || !captureLANHost(ip, prefixes) || len(ipOwners[d.IP]) != 1 || len(macIPs[d.MAC]) != 1 || leaseCount[d.MAC] > 1 {
			continue
		}
		seenInterfaces := interfaces[d.IP+"/"+d.MAC]
		if len(seenInterfaces) != 0 && (len(seenInterfaces) != 1 || !seenInterfaces["br-lan"]) {
			continue
		}
		if !d.Lease && (d.Interface != "br-lan" || !d.Online) {
			continue
		}
		if d.Lease && d.Interface == "" {
			d.Interface = "br-lan"
		}
		d.Eligible = true
	}
	return out
}

func captureLANHost(ip netip.Addr, prefixes []netip.Prefix) bool {
	for _, prefix := range prefixes {
		if !prefix.Contains(ip) {
			continue
		}
		if prefix.Bits() >= 31 {
			return true
		}
		// Network and directed-broadcast addresses are not device identities.
		address := ip.As4()
		network := prefix.Masked().Addr().As4()
		value := uint32(address[0])<<24 | uint32(address[1])<<16 | uint32(address[2])<<8 | uint32(address[3])
		base := uint32(network[0])<<24 | uint32(network[1])<<16 | uint32(network[2])<<8 | uint32(network[3])
		broadcast := base | (^uint32(0) >> prefix.Bits())
		if value != base && value != broadcast {
			return true
		}
	}
	return false
}

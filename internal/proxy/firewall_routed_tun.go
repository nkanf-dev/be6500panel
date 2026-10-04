package proxy

import (
	"fmt"
	"net/netip"
	"regexp"
	"slices"
	"strconv"
)

// Routed TUN names belong to this product. Keep this contract in sync with the
// native compiler and accepted-native/capture validation. Linux interface names
// are at most 15 bytes; aliases, wildcard matches and foreign names are refused.
var ownedRoutedTUNInterface = regexp.MustCompile(`^b6p-[A-Za-z0-9_][A-Za-z0-9_-]{0,10}$`)

const (
	ownedTUNMark    = "B6P_V4_TUN_MARK"
	ownedTUNDNS     = "B6P_V4_DNS"
	ownedTUNForward = "B6P_V4_TUN_FORWARD"
	ownedTUNReturn  = "B6P_V4_TUN_RETURN"
	ownedTUNInput   = "B6P_V4_TUN_INPUT"
	ownedTUNOutput  = "B6P_V4_TUN_OUTPUT"
)

// planOwnedRoutedTUN receives canonical, bounded device or gateway inputs. Both
// scopes use the same proven packet chains and main-core-owned system TUN. It
// owns packet routing/firewall intent, never interface or main-core lifecycle.
func planOwnedRoutedTUN(input RulesPlanInput, v4, v6 []netip.Addr, bypass, management, endpoints, routerDNS []string) (OwnedRulesPlan, error) {
	if input.IPv6 != IPv6Direct {
		return OwnedRulesPlan{}, fmt.Errorf("routed-tun requires IPv6 direct in the initial qualified phase")
	}
	if input.Scope != CaptureScopeGateway && input.ClientMACs == nil {
		return OwnedRulesPlan{}, fmt.Errorf("routed-tun requires exact ClientMACs for every selected client")
	}
	if input.Scope != CaptureScopeGateway && input.Ports.TProxy != 7893 {
		return OwnedRulesPlan{}, fmt.Errorf("routed-tun requires the unused compatibility TProxy port 7893")
	}
	local, peer, err := ownedRoutedTUNAddresses(input, v4, management, endpoints)
	if err != nil {
		return OwnedRulesPlan{}, err
	}
	input.TUNAddress = netip.PrefixFrom(local, 30).String()
	plan := OwnedRulesPlan{
		Ownership: RulesOwnership{
			Datapath: DatapathRoutedTUN, TUNInterface: input.TUNInterface, TUNAddress: input.TUNAddress,
			Mark: CaptureMark, Mask: CaptureMask, RouteTable: CaptureTable, RulePriority: CapturePriority,
			LANInterface: input.LANInterface, ClientMACs: input.ClientMACs, RouteFamilies: []int{4},
		},
		Warnings: []string{
			"Intent only: verify unused mark 0x4000, chains, table 16500 and priority 16500; serialize generations before applying. No idempotence or rollback success is assumed.",
			"Exact source-IP/MAC and incoming-LAN-interface TCP/UDP capture only. No router OUTPUT capture or full-LAN hooks; declared-client address changes require a new coordinated plan.",
			"Require actual main-core-owned system TUN readiness, matching address/peer, return routing and firewall hook order before activation. No interface create/delete/address, sysctl, netns, offload or main-table route changes are planned.",
			"IPv6 direct deliberately installs no IPv6 capture, DNS redirect or block rules; routed-TUN follow/block are not qualified. Multicast, ESP, GRE, VPN and other transports are not claimed by this TCP/UDP path.",
			"DNS REDIRECT requires the direct DNS listener on the incoming LAN address or a suitable wildcard, not loopback only. Router-local, management and safety DNS retain their factory path except exact RouterDNSAddresses port53 opt-in; other selected-client TCP/UDP port53, including endpoint DNS, reaches managed DNS before private/endpoint bypass. Encrypted DNS needs separate policy.",
			"Private-stack INPUT accepts only TCP from the next peer to the local TUN address on the owned TUN. OUTPUT accepts only the reverse TCP tuple on that TUN; no blanket TUN or router-output acceptance is planned. Return forwarding accepts only TCP/UDP to installed declared clients on the original LAN interface, not all LAN/WAN/guest destinations.",
			"No ECM/PPE/SFE setting is changed. Verify counters and real TCP, UDP, DNS, QUIC, MTU and failure/return paths on this firmware; pure argv intent is not router acceptance.",
			"Fail-direct cleanup removes owned hooks, chains, policy rules and the ordinary TUN route, not core-created interfaces or existing DNS REDIRECT conntrack bindings. Coordinate scoped flow drain/expiry on stop or failure; no global conntrack flush is planned.",
		},
	}
	sources := make([]string, 0, len(v4))
	if input.Scope == CaptureScopeGateway {
		plan.Ownership.Scope = CaptureScopeGateway
		plan.Ownership.LANIPv4Prefixes = slices.Clone(input.LANIPv4Prefixes)
		sources = input.LANIPv4Prefixes
		plan.Warnings[1] = "Declared-prefix and incoming-LAN-interface TCP/UDP capture only. New sources within these prefixes follow automatically; no device inventory or router OUTPUT capture. Guest/IoT/WAN interfaces remain outside this scope."
	} else {
		// Retain the established exact-client singular/plural ownership and
		// byte shape for device intent and old-journal withdrawal.
		if len(input.ClientIPv4s) == 0 {
			plan.Ownership.ClientIPv4 = v4[0].String()
		} else {
			plan.Ownership.ClientIPv4s = ownedClientStrings(v4)
		}
		if len(input.ClientIPv6s) != 0 {
			plan.Ownership.ClientIPv6s = ownedClientStrings(v6)
		} else if len(v6) != 0 {
			plan.Ownership.ClientIPv6 = v6[0].String()
		}
		for _, client := range v4 {
			sources = append(sources, ownedHostPrefix(client))
		}
	}
	if input.FakeIP {
		plan.Warnings = append(plan.Warnings,
			"Fake-IP identities 198.18.0.0/15 take precedence over ordinary private-network bypass only because FakeIP is enabled; synchronize native policy. Explicit management and non-DNS endpoint exemptions still win.",
			"Fail-direct withdrawal cannot make cached fake-IP identities directly routable. Coordinate resolver/classifier lifetime and client DNS cache recovery; cleanup alone is not instant direct recovery.")
	}
	b := ownedRulesBuilder{plan: &plan, input: input, bypass: bypass, management: management, endpoints: endpoints, routerDNS: routerDNS}
	plan.Apply = append(plan.Apply, ownedRoutedTUNRoute(input.TUNInterface, "add"))
	for _, source := range sources {
		plan.Apply = append(plan.Apply, b.sourceRule(4, source, "add"))
	}
	b.chain(4, "mangle", ownedTUNMark, "PREROUTING")
	b.chain(4, "nat", ownedTUNDNS, "PREROUTING")
	b.chain(4, "filter", ownedTUNForward, "FORWARD")
	b.chain(4, "filter", ownedTUNReturn, "FORWARD")
	b.chain(4, "filter", ownedTUNInput, "INPUT")
	b.chain(4, "filter", ownedTUNOutput, "OUTPUT")
	ownedRoutedTUNPacketChains(&b)
	for _, chain := range []string{ownedTUNForward, ownedTUNReturn} {
		if input.Scope == CaptureScopeGateway && chain == ownedTUNReturn {
			// Management addresses can be inside the declaration. Keep their
			// factory forwarding path before the shared TCP/UDP acceptance.
			b.addressBypass(4, "filter", chain, management)
		}
		for _, protocol := range []string{"tcp", "udp"} {
			b.appendRule(4, "filter", chain, "-p", protocol, "-j", "ACCEPT")
		}
		b.appendRule(4, "filter", chain, "-j", "RETURN")
	}
	for _, chain := range []string{ownedTUNInput, ownedTUNOutput} {
		b.appendRule(4, "filter", chain, "-p", "tcp", "-j", "ACCEPT")
		b.appendRule(4, "filter", chain, "-j", "RETURN")
	}
	// Forward/return/private-stack permissions are exact hooks, not blanket TUN
	// ACCEPT. Prepare them and DNS before the MARK hooks admit original packets.
	for _, source := range sources {
		for _, protocol := range []string{"tcp", "udp"} {
			args := ownedRoutedTUNSourceMatch(input, source)
			args = append(args, "-o", input.TUNInterface, "-m", "mark", "--mark", ownedMarkMask(), "-p", protocol, "-j", ownedTUNForward)
			b.hooks = append(b.hooks, ownedRoutedTUNHook("filter", "FORWARD", args...))
			b.hooks = append(b.hooks, ownedRoutedTUNHook("filter", "FORWARD", "-i", input.TUNInterface, "-o", input.LANInterface, "-d", source, "-p", protocol, "-j", ownedTUNReturn))
		}
	}
	b.hooks = append(b.hooks,
		ownedRoutedTUNHook("filter", "INPUT", "-i", input.TUNInterface, "-s", ownedHostPrefix(peer), "-d", ownedHostPrefix(local), "-p", "tcp", "-j", ownedTUNInput),
		ownedRoutedTUNHook("filter", "OUTPUT", "-o", input.TUNInterface, "-s", ownedHostPrefix(local), "-d", ownedHostPrefix(peer), "-p", "tcp", "-j", ownedTUNOutput))
	for _, entry := range []struct{ table, chain string }{{"nat", ownedTUNDNS}, {"mangle", ownedTUNMark}} {
		for _, source := range sources {
			args := append(ownedRoutedTUNSourceMatch(input, source), "-j", entry.chain)
			b.hooks = append(b.hooks, ownedRoutedTUNHook(entry.table, "PREROUTING", args...))
		}
	}
	plan.Apply = append(plan.Apply, b.hooks...)
	// Reverse hook order removes MARK entry first, DNS next, then all scoped
	// permissions. All hook deletions are attempted before any chain or route
	// deletion; the caller must retain ownership if best-effort cleanup fails.
	for i := len(b.hooks) - 1; i >= 0; i-- {
		hook := slices.Clone(b.hooks[i])
		hook[5] = "-D"
		hook = append(hook[:7], hook[8:]...)
		plan.Cleanup = append(plan.Cleanup, hook)
	}
	for i := len(plan.Ownership.Chains) - 1; i >= 0; i-- {
		chain := plan.Ownership.Chains[i]
		plan.Cleanup = append(plan.Cleanup, ownedIPTables(4, chain.Table, "-F", chain.Name), ownedIPTables(4, chain.Table, "-X", chain.Name))
	}
	for i := len(sources) - 1; i >= 0; i-- {
		plan.Cleanup = append(plan.Cleanup, b.sourceRule(4, sources[i], "del"))
	}
	plan.Cleanup = append(plan.Cleanup, ownedRoutedTUNRoute(input.TUNInterface, "del"))
	plan.OnFailure = ownedCloneCommands(plan.Cleanup)
	return plan, nil
}

func ownedRoutedTUNAddresses(input RulesPlanInput, clients []netip.Addr, management, endpoints []string) (netip.Addr, netip.Addr, error) {
	if !ownedRoutedTUNInterface.MatchString(input.TUNInterface) || input.TUNInterface == input.LANInterface {
		return netip.Addr{}, netip.Addr{}, fmt.Errorf("TUNInterface must be a distinct owned b6p- interface name of 5 to 15 bytes (no alias or wildcard)")
	}
	prefix, err := netip.ParsePrefix(input.TUNAddress)
	if err != nil || !prefix.Addr().Is4() || prefix.Bits() != 30 || !prefix.Addr().IsPrivate() {
		return netip.Addr{}, netip.Addr{}, fmt.Errorf("TUNAddress must be a private RFC1918 IPv4 host /30 with a next usable peer")
	}
	local, network := prefix.Addr(), prefix.Masked()
	if local != network.Addr().Next() {
		return netip.Addr{}, netip.Addr{}, fmt.Errorf("TUNAddress must be the first usable /30 host so the next host is its usable peer")
	}
	for _, value := range input.LANIPv4Prefixes {
		if network.Overlaps(netip.MustParsePrefix(value)) {
			return netip.Addr{}, netip.Addr{}, fmt.Errorf("TUNAddress /30 overlaps a declared LAN prefix")
		}
	}
	for _, client := range clients {
		if network.Contains(client) {
			return netip.Addr{}, netip.Addr{}, fmt.Errorf("TUNAddress /30 collides with a selected client")
		}
	}
	for _, values := range [][]string{management, endpoints} {
		for _, value := range values {
			addr, _ := netip.ParseAddr(value)
			if network.Contains(addr) {
				return netip.Addr{}, netip.Addr{}, fmt.Errorf("TUNAddress /30 collides with management or endpoint addresses")
			}
		}
	}
	return local, local.Next(), nil
}

func ownedRoutedTUNPacketChains(b *ownedRulesBuilder) {
	b.prelude(4, "mangle", ownedTUNMark)
	// This is the established managed-DNS ordering (33149bf): explicit router
	// DNS opt-in precedes LOCAL; endpoint/private bypass never bypasses public
	// selected-client port53. Mangle excludes port53 for NAT's direct listener.
	for _, destination := range b.routerDNS {
		addr, _ := netip.ParseAddr(destination)
		if addr.Is4() {
			for _, protocol := range []string{"tcp", "udp"} {
				b.appendRule(4, "nat", ownedTUNDNS, "-d", ownedHostPrefix(addr), "-p", protocol, "--dport", "53", "-j", "REDIRECT", "--to-ports", strconv.Itoa(int(b.input.Ports.DNS)))
			}
		}
	}
	b.localBypass(4, "nat", ownedTUNDNS)
	b.addressBypass(4, "nat", ownedTUNDNS, b.management)
	b.safetyBypass(4, "nat", ownedTUNDNS)
	for _, protocol := range []string{"tcp", "udp"} {
		b.appendRule(4, "mangle", ownedTUNMark, "-p", protocol, "--dport", "53", "-j", "RETURN")
		b.appendRule(4, "nat", ownedTUNDNS, "-p", protocol, "--dport", "53", "-j", "REDIRECT", "--to-ports", strconv.Itoa(int(b.input.Ports.DNS)))
	}
	for _, destination := range b.endpoints {
		if !slices.Contains(b.management, destination) {
			b.addressBypass(4, "nat", ownedTUNDNS, []string{destination})
		}
	}
	if b.input.FakeIP {
		for _, protocol := range []string{"tcp", "udp"} {
			b.appendRule(4, "mangle", ownedTUNMark, "-p", protocol, "-d", "198.18.0.0/15", "-j", "MARK", "--set-xmark", ownedMarkMask())
		}
	}
	for _, destination := range ownedPrivateNetworks(4) {
		b.appendRule(4, "mangle", ownedTUNMark, "-d", destination, "-j", "RETURN")
		b.appendRule(4, "nat", ownedTUNDNS, "-d", destination, "-j", "RETURN")
	}
	for _, protocol := range []string{"tcp", "udp"} {
		b.appendRule(4, "mangle", ownedTUNMark, "-p", protocol, "-j", "MARK", "--set-xmark", ownedMarkMask())
	}
	b.appendRule(4, "mangle", ownedTUNMark, "-j", "RETURN")
	b.appendRule(4, "nat", ownedTUNDNS, "-j", "RETURN")
}

func ownedRoutedTUNSourceMatch(input RulesPlanInput, source string) []string {
	args := []string{"-i", input.LANInterface, "-s", source}
	if input.Scope != CaptureScopeGateway {
		client := netip.MustParsePrefix(source).Addr().String()
		args = append(args, "-m", "mac", "--mac-source", input.ClientMACs[client])
	}
	return args
}

func ownedRoutedTUNHook(table, hook string, args ...string) []string {
	return ownedIPTables(4, table, append([]string{"-I", hook, "1"}, args...)...)
}

func ownedRoutedTUNRoute(iface, operation string) []string {
	return []string{"ip", "-4", "route", operation, "default", "dev", iface, "table", strconv.Itoa(CaptureTable)}
}

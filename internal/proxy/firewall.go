package proxy

import (
	"fmt"
	"net"
	"net/netip"
	"regexp"
	"slices"
	"strconv"
)

const (
	// CaptureMark is the only free bit between the known QSDK QoS mask
	// 0xffff8000, mwan mask 0x3f00, parent mask 0x0f and UU mask 0xf0.
	// 0x40000000 is NOT free: the QoS mask includes that bit.
	CaptureMark     uint32 = 0x4000
	CaptureMask     uint32 = 0x4000
	CaptureTable           = 16500
	CapturePriority        = 16500
	// MaxCaptureClientsPerFamily bounds raw client entries, including a legacy
	// singular address when it is combined with a plural list.
	MaxCaptureClientsPerFamily = 64
)

// RulesPlanInput selects a bounded set of exact IPv4 clients, and optionally
// exact IPv6 clients. It is not a LAN-wide policy. Singular legacy fields remain
// valid and are merged with plural fields when both are supplied. All client
// entries are literal addresses, never hostnames, prefixes, comma-separated
// lists or interface-zone-qualified addresses.
// Optional ClientMACs binds exact sources to their layer-2 identities. A supplied
// map must cover every IPv4 and active IPv6 source, and cannot contain any other
// address. IP and unicast six-byte MAC literals are canonicalized. A nil map
// preserves legacy IP-only intent for standalone
// compiler callers. Mapped hooks require both exact source IP and source MAC.
// ManagementIPs and EndpointIPs bypass capture except for explicitly selected
// RouterDNSAddresses on TCP/UDP port 53. RouterDNSAddresses must be a subset
// of ManagementIPs sourced from actual router LAN addresses by the owner.
// Other router-addressed DNS remains on the original dnsmasq path.
// IPv6Follow and IPv6Block require at least one IPv6 client; IPv6Direct installs
// no IPv6 rules. Each family has shared dedicated chains and one local route,
// with exact per-client policy rules and interface-scoped hooks.
// Empty policy fields default to direct; all-zero Ports uses native compiler
// defaults (2080/7893/1053), but partially specified ports are invalid.
// FakeIP must match the compiler; false preserves ordinary private destinations.
// FailureBlockProxy is unsupported without a surviving flow classifier.
type RulesPlanInput struct {
	ClientIPv4         string
	ClientIPv6         string
	ClientIPv4s        []string          `json:",omitempty"`
	ClientIPv6s        []string          `json:",omitempty"`
	ClientMACs         map[string]string `json:",omitempty"`
	LANInterface       string
	Ports              Ports
	IPv6               IPv6Mode
	Failure            FailurePolicy
	EndpointIPs        []string
	ManagementIPs      []string
	RouterDNSAddresses []string
	FakeIP             bool
}

// OwnedChain identifies a dedicated chain, not an existing system chain.
// Hook is the system chain containing the exact, client-scoped jump.
type OwnedChain struct {
	Family int
	Table  string
	Name   string
	Hook   string
}

// RulesOwnership describes reserved resources. The caller must verify that the
// mark, chains, route table and rule priority are unused before applying a plan,
// serialize generations, and retain this metadata until cleanup completes.
type RulesOwnership struct {
	Mark          uint32
	Mask          uint32
	RouteTable    int
	RulePriority  int
	LANInterface  string
	ClientIPv4    string
	ClientIPv6    string
	ClientIPv4s   []string          `json:",omitempty"`
	ClientIPv6s   []string          `json:",omitempty"`
	ClientMACs    map[string]string `json:",omitempty"`
	RouteFamilies []int
	Chains        []OwnedChain
}

// OwnedRulesPlan is pure argv intent. Each command includes its executable and
// must be run directly, never joined into a shell command. Apply is ordered;
// stop on its first error, then attempt EVERY Cleanup command best-effort.
// Cleanup can report absent resources after partial Apply. It is not an
// idempotent transaction and must not run against another active generation.
// OnFailure implements only fail-direct and has independent argv storage.
type OwnedRulesPlan struct {
	Apply     [][]string
	Cleanup   [][]string
	OnFailure [][]string
	Ownership RulesOwnership
	Warnings  []string
}

var ownedLANInterface = regexp.MustCompile(`^[A-Za-z0-9_][A-Za-z0-9_.:-]{0,14}$`)

// PlanOwnedRules builds exact-client transparent TCP/UDP capture, DNS REDIRECT
// and optional IPv6 intent. No command is run and no router state is inspected.
// TPROXY modifies only CaptureMask, preserving QoS/mwan/parent/UU mark bits.
// Routes and complete dedicated chains are prepared before any hook is added.
// DNS must use its own NAT REDIRECT: the native direct DNS listener cannot use
// TPROXY's original destination. Mangle explicitly returns port 53 so it can
// reach NAT; other TCP/UDP uses the transparent listener on Ports.TProxy.
func PlanOwnedRules(input RulesPlanInput) (OwnedRulesPlan, error) {
	if input.IPv6 == "" {
		input.IPv6 = IPv6Direct
	}
	if input.Failure == "" {
		input.Failure = FailureDirect
	}
	if input.Ports == (Ports{}) {
		input.Ports = Ports{Mixed: 2080, TProxy: 7893, DNS: 1053}
	}
	if len(input.EndpointIPs) > 256 || len(input.ManagementIPs) > 128 || len(input.RouterDNSAddresses) > 16 {
		return OwnedRulesPlan{}, fmt.Errorf("firewall input limit exceeded: at most 256 EndpointIPs, 128 ManagementIPs and 16 RouterDNSAddresses")
	}
	v4, err := ownedClientAddresses(input.ClientIPv4, input.ClientIPv4s, 4)
	if err != nil {
		return OwnedRulesPlan{}, fmt.Errorf("ClientIPv4/ClientIPv4s: %w", err)
	}
	if len(v4) == 0 {
		return OwnedRulesPlan{}, fmt.Errorf("at least one exact IPv4 client is required")
	}
	v6, err := ownedClientAddresses(input.ClientIPv6, input.ClientIPv6s, 6)
	if err != nil {
		return OwnedRulesPlan{}, fmt.Errorf("ClientIPv6/ClientIPv6s: %w", err)
	}
	if !ownedLANInterface.MatchString(input.LANInterface) {
		return OwnedRulesPlan{}, fmt.Errorf("LANInterface must be a safe, exact interface name of 1 to 15 bytes (no wildcard)")
	}
	if input.Ports.Mixed == 0 || input.Ports.TProxy == 0 || input.Ports.DNS == 0 || input.Ports.TProxy == input.Ports.DNS {
		return OwnedRulesPlan{}, fmt.Errorf("listener ports must be nonzero and distinct")
	}
	if input.Ports.Mixed == input.Ports.TProxy || input.Ports.Mixed == input.Ports.DNS {
		return OwnedRulesPlan{}, fmt.Errorf("Mixed port must not share a TProxy or DNS listener port")
	}
	switch input.IPv6 {
	case IPv6Direct:
	case IPv6Follow, IPv6Block:
		if len(v6) == 0 {
			return OwnedRulesPlan{}, fmt.Errorf("IPv6 %s requires exact ClientIPv6 or ClientIPv6s addresses; refusing broader capture", input.IPv6)
		}
	default:
		return OwnedRulesPlan{}, fmt.Errorf("invalid IPv6 mode")
	}
	clientMACs, err := ownedClientMACs(input.ClientMACs, v4, v6, input.IPv6)
	if err != nil {
		return OwnedRulesPlan{}, fmt.Errorf("ClientMACs: %w", err)
	}
	input.ClientMACs = clientMACs
	switch input.Failure {
	case FailureDirect:
	case FailureBlockProxy:
		return OwnedRulesPlan{}, fmt.Errorf("block-proxy requires a surviving stateful DNS/flow classifier; dropping all selected-client traffic is not selective blocking")
	default:
		return OwnedRulesPlan{}, fmt.Errorf("invalid failure policy")
	}
	endpoints, err := ownedAddressList(input.EndpointIPs)
	if err != nil {
		return OwnedRulesPlan{}, fmt.Errorf("EndpointIPs: %w", err)
	}
	management, err := ownedAddressList(input.ManagementIPs)
	if err != nil {
		return OwnedRulesPlan{}, fmt.Errorf("ManagementIPs: %w", err)
	}
	routerDNS, err := ownedAddressList(input.RouterDNSAddresses)
	if err != nil {
		return OwnedRulesPlan{}, fmt.Errorf("RouterDNSAddresses: %w", err)
	}
	for _, destination := range routerDNS {
		addr, _ := netip.ParseAddr(destination)
		if addr.IsUnspecified() || addr.IsLoopback() || addr.IsMulticast() || addr.IsLinkLocalUnicast() || destination == "255.255.255.255" || !slices.Contains(management, destination) {
			return OwnedRulesPlan{}, fmt.Errorf("RouterDNSAddresses must be unicast router LAN addresses also present in ManagementIPs")
		}
	}
	bypass := append(endpoints, management...)
	slices.Sort(bypass)
	bypass = slices.Compact(bypass)

	plan := OwnedRulesPlan{
		Ownership: RulesOwnership{
			Mark: CaptureMark, Mask: CaptureMask, RouteTable: CaptureTable, RulePriority: CapturePriority,
			LANInterface: input.LANInterface, ClientMACs: clientMACs,
		},
		Warnings: []string{
			"Intent only: verify unused mark 0x4000, chains, table 16500 and priority 16500; serialize generations before applying. No idempotence or rollback success is assumed.",
			"Single-client TCP/UDP capture only; no OUTPUT or full-LAN hooks. Client address changes require a new coordinated plan.",
			"Validate kernel TPROXY, policy routing, NAT REDIRECT, firewall hook order and return routing on the router before activation.",
			"DNS REDIRECT requires a DNS listener on the incoming LAN address or a suitable wildcard, not loopback only. Restrict listener access in the coordinated firewall; keep mixed authentication/listener scope separate.",
			"Router-local, management and endpoint traffic bypasses even DNS capture unless an exact ManagementIPs subset is opted into RouterDNSAddresses for selected-client TCP/UDP port53 only, preserving factory SSH management without a global port-22 exemption. Other router-addressed DNS stays on dnsmasq; TCP and UDP DNS to other unicast destinations is redirected before private-network bypass. Encrypted DNS needs separate policy.",
			"No ECM/PPE/SFE setting is changed. Verify rule counters and real TCP, UDP, DNS, QUIC and failure/return paths with hardware offload on this firmware; argv alone does not prove capture works.",
			"Fail-direct cleanup removes owned rules, not existing DNS REDIRECT conntrack bindings. Coordinate scoped flow drain/expiry on stop or failure; no global conntrack flush is planned.",
		},
	}
	// Keep legacy singular-only ownership and cleanup exactly reproducible.
	// Plural intent uses only plural ownership for the matching family.
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
	if len(v4) > 1 || len(v6) > 1 {
		plan.Warnings[1] = "Exact-client TCP/UDP capture only; no OUTPUT or full-LAN hooks. Client address changes require a new coordinated plan."
	}
	if input.FakeIP {
		plan.Warnings = append(plan.Warnings,
			"Fake-IP identities 198.18.0.0/15 and fc00::/18 take precedence over ordinary private-network bypass only because FakeIP is enabled; synchronize this setting with the native compiler. Explicit management/endpoint exemptions still win.",
			"Fail-direct rule withdrawal cannot make cached fake-IP identities directly routable. Coordinate resolver/classifier lifetime and client DNS cache recovery before enabling FakeIP; cleanup alone is not instant direct recovery.")
	}
	builder := ownedRulesBuilder{plan: &plan, input: input, bypass: bypass, routerDNS: routerDNS}
	builder.captureFamily(4, v4)
	switch input.IPv6 {
	case IPv6Follow:
		builder.captureFamily(6, v6)
		warning := "IPv6 follow requires ip6tables TPROXY/NAT support, local IPv6 routing and transparent listeners accepting ::1 and 127.0.0.1; verify dual-stack behavior. Only the selected IPv6 address is covered."
		if len(v6) > 1 {
			warning = "IPv6 follow requires ip6tables TPROXY/NAT support, local IPv6 routing and transparent listeners accepting ::1 and 127.0.0.1; verify dual-stack behavior. Only the selected exact IPv6 addresses are covered."
		}
		plan.Warnings = append(plan.Warnings, warning)
	case IPv6Block:
		builder.blockIPv6(v6)
		warning := "IPv6 block rejects only the selected client's forwarded non-exempt IPv6 traffic, including public DNS and fake-IP identities when enabled. Router-local/private/management/endpoint traffic is preserved, including factory SSH management. Other client IPv6 addresses are not covered."
		if len(v6) > 1 {
			warning = "IPv6 block rejects only the selected exact clients' forwarded non-exempt IPv6 traffic, including public DNS and fake-IP identities when enabled. Router-local/private/management/endpoint traffic is preserved, including factory SSH management. Unselected IPv6 addresses are not covered."
		}
		plan.Warnings = append(plan.Warnings, warning)
	case IPv6Direct:
		plan.Warnings = append(plan.Warnings, "IPv6 direct deliberately installs no IPv6 capture, DNS redirect or block rules; it is not IPv6 split routing.")
	}

	// Every family, route and owned chain is ready before the first externally
	// reachable hook. DNS hooks precede capture hooks; mangle still runs first.
	plan.Apply = append(plan.Apply, builder.hooks...)
	for i := len(builder.hooks) - 1; i >= 0; i-- {
		hook := slices.Clone(builder.hooks[i])
		// Fixed prefix: executable, -w, seconds, -t, table, -I, hook, 1.
		hook[5] = "-D"
		hook = append(hook[:7], hook[8:]...)
		plan.Cleanup = append(plan.Cleanup, hook)
	}
	for i := len(plan.Ownership.Chains) - 1; i >= 0; i-- {
		chain := plan.Ownership.Chains[i]
		plan.Cleanup = append(plan.Cleanup,
			ownedIPTables(chain.Family, chain.Table, "-F", chain.Name),
			ownedIPTables(chain.Family, chain.Table, "-X", chain.Name))
	}
	for i := len(builder.routes) - 1; i >= 0; i-- {
		family := builder.routes[i]
		for j := len(family.clients) - 1; j >= 0; j-- {
			plan.Cleanup = append(plan.Cleanup, builder.rule(family.family, family.clients[j], "del"))
		}
		plan.Cleanup = append(plan.Cleanup, ownedLocalRoute(family.family, "del"))
	}
	plan.OnFailure = ownedCloneCommands(plan.Cleanup)
	return plan, nil
}

func ownedClientAddresses(singular string, plural []string, family int) ([]netip.Addr, error) {
	count := len(plural)
	if singular != "" {
		count++
	}
	if count > MaxCaptureClientsPerFamily {
		return nil, fmt.Errorf("at most %d exact clients per family", MaxCaptureClientsPerFamily)
	}
	raw := make([]string, 0, count)
	if singular != "" {
		raw = append(raw, singular)
	}
	raw = append(raw, plural...)
	canonical := make([]string, 0, count)
	for i, value := range raw {
		addr, err := ownedClientAddress(value, family)
		if err != nil {
			return nil, fmt.Errorf("address %d: %w", i, err)
		}
		canonical = append(canonical, addr.String())
	}
	slices.Sort(canonical)
	canonical = slices.Compact(canonical)
	clients := make([]netip.Addr, 0, len(canonical))
	for _, value := range canonical {
		clients = append(clients, netip.MustParseAddr(value))
	}
	return clients, nil
}

func ownedClientMACs(raw map[string]string, v4, v6 []netip.Addr, mode IPv6Mode) (map[string]string, error) {
	if raw == nil {
		return nil, nil
	}
	if len(raw) > 2*MaxCaptureClientsPerFamily {
		return nil, fmt.Errorf("at most %d exact source IP/MAC pairs", 2*MaxCaptureClientsPerFamily)
	}
	selected := make(map[string]bool, len(v4)+len(v6))
	for _, addr := range v4 {
		selected[addr.String()] = true
	}
	if mode != IPv6Direct {
		for _, addr := range v6 {
			selected[addr.String()] = true
		}
	}
	canonical := make(map[string]string, len(raw))
	for source, value := range raw {
		addr, err := ownedLiteralAddress(source)
		if err != nil || !selected[addr.String()] {
			return nil, fmt.Errorf("keys must be selected exact IPv4 or active IPv6 client addresses")
		}
		key := addr.String()
		if _, exists := canonical[key]; exists {
			return nil, fmt.Errorf("duplicate canonical source address %s", key)
		}
		mac, err := net.ParseMAC(value)
		if err != nil || len(mac) != 6 || mac[0]&1 != 0 || slices.Equal([]byte(mac), []byte{0, 0, 0, 0, 0, 0}) {
			return nil, fmt.Errorf("source %s requires a nonzero unicast six-byte MAC address", key)
		}
		canonical[key] = mac.String()
	}
	for source := range selected {
		if _, exists := canonical[source]; !exists {
			return nil, fmt.Errorf("every selected IPv4 and active IPv6 client requires an exact source MAC")
		}
	}
	return canonical, nil
}

func ownedClientStrings(clients []netip.Addr) []string {
	out := make([]string, len(clients))
	for i, client := range clients {
		out[i] = client.String()
	}
	return out
}

func ownedClientAddress(raw string, family int) (netip.Addr, error) {
	addr, err := ownedLiteralAddress(raw)
	if err != nil {
		return netip.Addr{}, err
	}
	if addr.Is4() != (family == 4) {
		return netip.Addr{}, fmt.Errorf("requires one IPv%d address", family)
	}
	if addr.IsUnspecified() || addr.IsLoopback() || addr.IsMulticast() || addr.String() == "255.255.255.255" {
		return netip.Addr{}, fmt.Errorf("requires a unicast client address, not unspecified, loopback, multicast or broadcast")
	}
	return addr, nil
}

func ownedLiteralAddress(raw string) (netip.Addr, error) {
	addr, err := netip.ParseAddr(raw)
	if err != nil || addr.Zone() != "" || addr.Is4In6() {
		return netip.Addr{}, fmt.Errorf("requires a single literal IP address without prefix, zone or mapped IPv4")
	}
	return addr, nil
}

func ownedAddressList(raw []string) ([]string, error) {
	out := make([]string, 0, len(raw))
	for i, value := range raw {
		addr, err := ownedLiteralAddress(value)
		if err != nil {
			return nil, fmt.Errorf("address %d: %w", i, err)
		}
		out = append(out, addr.String())
	}
	slices.Sort(out)
	return slices.Compact(out), nil
}

type ownedFamilyRoute struct {
	family  int
	clients []netip.Addr
}

type ownedRulesBuilder struct {
	plan      *OwnedRulesPlan
	input     RulesPlanInput
	bypass    []string
	routerDNS []string
	hooks     [][]string
	routes    []ownedFamilyRoute
}

func (b *ownedRulesBuilder) captureFamily(family int, clients []netip.Addr) {
	b.plan.Apply = append(b.plan.Apply, ownedLocalRoute(family, "add"))
	for _, client := range clients {
		b.plan.Apply = append(b.plan.Apply, b.rule(family, client, "add"))
	}
	b.routes = append(b.routes, ownedFamilyRoute{family, clients})
	b.plan.Ownership.RouteFamilies = append(b.plan.Ownership.RouteFamilies, family)
	capture := "B6P_V" + strconv.Itoa(family) + "_CAPTURE"
	dns := "B6P_V" + strconv.Itoa(family) + "_DNS"
	b.chain(family, "mangle", capture, "PREROUTING")
	b.chain(family, "nat", dns, "PREROUTING")
	b.prelude(family, "mangle", capture)
	// This narrow opt-in must precede LOCAL and management bypass in NAT only.
	// The chain hook already scopes source client/interface. No global dnsmasq
	// or INPUT mutation is generated, and non-DNS management remains exempt.
	for _, destination := range b.routerDNS {
		addr, _ := netip.ParseAddr(destination)
		if addr.Is4() != (family == 4) {
			continue
		}
		for _, protocol := range []string{"tcp", "udp"} {
			b.appendRule(family, "nat", dns, "-d", ownedHostPrefix(addr), "-p", protocol, "--dport", "53", "-j", "REDIRECT", "--to-ports", strconv.Itoa(int(b.input.Ports.DNS)))
		}
	}
	b.prelude(family, "nat", dns)
	for _, protocol := range []string{"tcp", "udp"} {
		b.appendRule(family, "mangle", capture, "-p", protocol, "--dport", "53", "-j", "RETURN")
		b.appendRule(family, "nat", dns, "-p", protocol, "--dport", "53", "-j", "REDIRECT", "--to-ports", strconv.Itoa(int(b.input.Ports.DNS)))
	}
	fake := "198.18.0.0/15"
	if family == 6 {
		fake = "fc00::/18"
	}
	if b.input.FakeIP {
		for _, protocol := range []string{"tcp", "udp"} {
			b.tproxy(family, capture, protocol, fake)
		}
	}
	for _, destination := range ownedPrivateNetworks(family) {
		b.appendRule(family, "mangle", capture, "-d", destination, "-j", "RETURN")
		b.appendRule(family, "nat", dns, "-d", destination, "-j", "RETURN")
	}
	for _, protocol := range []string{"tcp", "udp"} {
		b.tproxy(family, capture, protocol, "")
	}
	b.appendRule(family, "mangle", capture, "-j", "RETURN")
	b.appendRule(family, "nat", dns, "-j", "RETURN")
	// Prepare one shared chain per table, then queue all exact DNS hooks before
	// capture hooks. Hooks are installed only after every family is prepared.
	for _, client := range clients {
		b.hook(family, "nat", dns, "PREROUTING", client)
	}
	for _, client := range clients {
		b.hook(family, "mangle", capture, "PREROUTING", client)
	}
}

func (b *ownedRulesBuilder) blockIPv6(clients []netip.Addr) {
	const chain = "B6P_V6_BLOCK"
	b.chain(6, "filter", chain, "FORWARD")
	b.prelude(6, "filter", chain)
	if b.input.FakeIP {
		b.appendRule(6, "filter", chain, "-d", "fc00::/18", "-j", "REJECT", "--reject-with", "icmp6-adm-prohibited")
	}
	for _, destination := range ownedPrivateNetworks(6) {
		b.appendRule(6, "filter", chain, "-d", destination, "-j", "RETURN")
	}
	b.appendRule(6, "filter", chain, "-j", "REJECT", "--reject-with", "icmp6-adm-prohibited")
	for _, client := range clients {
		b.hook(6, "filter", chain, "FORWARD", client)
	}
}

func (b *ownedRulesBuilder) chain(family int, table, name, hook string) {
	b.plan.Ownership.Chains = append(b.plan.Ownership.Chains, OwnedChain{Family: family, Table: table, Name: name, Hook: hook})
	b.plan.Apply = append(b.plan.Apply, ownedIPTables(family, table, "-N", name))
}

func (b *ownedRulesBuilder) hook(family int, table, name, hook string, client netip.Addr) {
	args := []string{"-I", hook, "1", "-i", b.input.LANInterface, "-s", ownedHostPrefix(client)}
	if mac, exists := b.input.ClientMACs[client.String()]; exists {
		args = append(args, "-m", "mac", "--mac-source", mac)
	}
	args = append(args, "-j", name)
	b.hooks = append(b.hooks, ownedIPTables(family, table, args...))
}

func (b *ownedRulesBuilder) prelude(family int, table, chain string) {
	// All router interface addresses are management, even if the caller did
	// not enumerate them. Only explicit RouterDNSAddresses port53 exceptions
	// installed earlier in the NAT chain may override this LOCAL bypass.
	b.appendRule(family, table, chain, "-m", "addrtype", "--dst-type", "LOCAL", "-j", "RETURN")
	for _, destination := range b.bypass {
		addr, _ := netip.ParseAddr(destination) // already validated and canonical
		if addr.Is4() == (family == 4) {
			b.appendRule(family, table, chain, "-d", ownedHostPrefix(addr), "-j", "RETURN")
		}
	}
	for _, destination := range ownedSafetyNetworks(family) {
		b.appendRule(family, table, chain, "-d", destination, "-j", "RETURN")
	}
}

func (b *ownedRulesBuilder) appendRule(family int, table, chain string, args ...string) {
	all := append([]string{"-A", chain}, args...)
	b.plan.Apply = append(b.plan.Apply, ownedIPTables(family, table, all...))
}

func (b *ownedRulesBuilder) tproxy(family int, chain, protocol, destination string) {
	args := []string{"-p", protocol}
	if destination != "" {
		args = append(args, "-d", destination)
	}
	onIP := "127.0.0.1"
	if family == 6 {
		onIP = "::1"
	}
	args = append(args, "-j", "TPROXY", "--on-ip", onIP, "--on-port", strconv.Itoa(int(b.input.Ports.TProxy)), "--tproxy-mark", ownedMarkMask())
	b.appendRule(family, "mangle", chain, args...)
}

func (b *ownedRulesBuilder) rule(family int, client netip.Addr, operation string) []string {
	return []string{"ip", "-" + strconv.Itoa(family), "rule", operation, "priority", strconv.Itoa(CapturePriority),
		"from", ownedHostPrefix(client), "iif", b.input.LANInterface, "fwmark", ownedMarkMask(), "lookup", strconv.Itoa(CaptureTable)}
}

func ownedLocalRoute(family int, operation string) []string {
	prefix := "0.0.0.0/0"
	if family == 6 {
		prefix = "::/0"
	}
	return []string{"ip", "-" + strconv.Itoa(family), "route", operation, "local", prefix, "dev", "lo", "table", strconv.Itoa(CaptureTable)}
}

func ownedIPTables(family int, table string, args ...string) []string {
	tool := "iptables"
	if family == 6 {
		tool = "ip6tables"
	}
	return append([]string{tool, "-w", "5", "-t", table}, args...)
}

func ownedHostPrefix(addr netip.Addr) string {
	bits := 128
	if addr.Is4() {
		bits = 32
	}
	return netip.PrefixFrom(addr, bits).String()
}

func ownedMarkMask() string {
	return fmt.Sprintf("0x%x/0x%x", CaptureMark, CaptureMask)
}

func ownedSafetyNetworks(family int) []string {
	if family == 6 {
		return []string{"::/128", "::1/128", "fe80::/10", "ff00::/8"}
	}
	return []string{"0.0.0.0/8", "127.0.0.0/8", "169.254.0.0/16", "192.0.0.0/24", "224.0.0.0/4", "240.0.0.0/4"}
}

func ownedPrivateNetworks(family int) []string {
	if family == 6 {
		return []string{"fc00::/7"}
	}
	return []string{"10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16", "100.64.0.0/10", "198.18.0.0/15"}
}

func ownedCloneCommands(commands [][]string) [][]string {
	out := make([][]string, len(commands))
	for i, command := range commands {
		out[i] = slices.Clone(command)
	}
	return out
}

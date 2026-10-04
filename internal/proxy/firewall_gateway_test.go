package proxy

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"reflect"
	"slices"
	"strings"
	"testing"
)

func gatewayTestInput() RulesPlanInput {
	in := routedTUNTestInput()
	in.Scope = CaptureScopeGateway
	in.ClientIPv4 = ""
	in.ClientMACs = nil
	in.LANIPv4Prefixes = []string{"192.168.31.0/24"}
	return in
}

func TestGatewayCanonicalPrefixes(t *testing.T) {
	raw := []string{"192.168.31.243/32", "10.0.0.0/8", "172.16.0.0/12"}
	original := slices.Clone(raw)
	got, err := CanonicalGatewayPrefixes(raw)
	want := []string{"10.0.0.0/8", "172.16.0.0/12", "192.168.31.243/32"}
	if err != nil || !slices.Equal(got, want) || !slices.Equal(raw, original) {
		t.Fatalf("canonical prefixes got=%v err=%v input=%v", got, err, raw)
	}
	got[0] = "changed"
	if !slices.Equal(raw, original) {
		t.Fatal("canonical helper aliases input")
	}
	for _, tc := range []struct {
		name string
		raw  []string
	}{
		{"absent", nil}, {"empty", []string{}},
		{"too-many", []string{"10.0.0.1/32", "10.0.0.2/32", "10.0.0.3/32", "10.0.0.4/32", "10.0.0.5/32", "10.0.0.6/32", "10.0.0.7/32", "10.0.0.8/32", "10.0.0.9/32"}},
		{"host-bits", []string{"192.168.31.1/24"}},
		{"noncanonical-bits", []string{"192.168.31.0/024"}},
		{"noncanonical-address", []string{"192.168.031.0/24"}},
		{"duplicate", []string{"192.168.31.0/24", "192.168.31.0/24"}},
		{"overlap", []string{"192.168.0.0/16", "192.168.31.0/24"}},
		{"overlap-reverse", []string{"192.168.31.243/32", "192.168.31.0/24"}},
		{"mapped", []string{"::ffff:192.168.31.0/120"}},
		{"ipv6", []string{"fd00::/64"}},
		{"zone", []string{"192.168.31.0%br-lan/24"}},
		{"public", []string{"203.0.113.0/24"}},
		{"cgnat", []string{"100.64.0.0/10"}},
		{"fake-ip", []string{"198.18.0.0/15"}},
		{"link-local", []string{"169.254.0.0/16"}},
		{"multicast", []string{"224.0.0.0/4"}},
		{"zero-prefix", []string{"0.0.0.0/0"}},
		{"private-zero-prefix", []string{"10.0.0.0/0"}},
		{"too-broad", []string{"10.0.0.0/7"}},
		{"partly-private-172", []string{"172.0.0.0/8"}},
		{"partly-private-192", []string{"192.0.0.0/8"}},
		{"no-prefix", []string{"192.168.31.0"}},
		{"comma", []string{"192.168.31.0/24,192.168.32.0/24"}},
		{"padded", []string{" 192.168.31.0/24"}},
		{"injection", []string{"192.168.31.0/24; reboot"}},
	} {
		t.Run(tc.name, func(t *testing.T) {
			if got, err := CanonicalGatewayPrefixes(tc.raw); err == nil || got != nil {
				t.Fatalf("invalid declaration accepted: got=%v err=%v", got, err)
			}
		})
	}
}

func TestGatewayRejectsMixedScopeAndUnsafeInputs(t *testing.T) {
	for _, tc := range []struct {
		name string
		edit func(*RulesPlanInput)
	}{
		{"invalid-scope", func(in *RulesPlanInput) { in.Scope = "all" }},
		{"devices-with-prefix", func(in *RulesPlanInput) { in.Scope = CaptureScopeDevices }},
		{"empty-scope-with-prefix", func(in *RulesPlanInput) { in.Scope = "" }},
		{"literal-v4", func(in *RulesPlanInput) { in.ClientIPv4 = "192.168.31.42" }},
		{"literal-v6", func(in *RulesPlanInput) { in.ClientIPv6 = "2001:db8::42" }},
		{"plural-v4", func(in *RulesPlanInput) { in.ClientIPv4s = []string{"192.168.31.42"} }},
		{"empty-plural-v4", func(in *RulesPlanInput) { in.ClientIPv4s = []string{} }},
		{"plural-v6", func(in *RulesPlanInput) { in.ClientIPv6s = []string{"2001:db8::42"} }},
		{"empty-plural-v6", func(in *RulesPlanInput) { in.ClientIPv6s = []string{} }},
		{"mac-map", func(in *RulesPlanInput) { in.ClientMACs = map[string]string{"192.168.31.42": "02:11:22:33:44:42"} }},
		{"empty-mac-map", func(in *RulesPlanInput) { in.ClientMACs = map[string]string{} }},
		{"default-datapath", func(in *RulesPlanInput) { in.Datapath = "" }},
		{"tproxy", func(in *RulesPlanInput) { in.Datapath = DatapathTPROXY }},
		{"unknown-datapath", func(in *RulesPlanInput) { in.Datapath = "other" }},
		{"ipv6-follow", func(in *RulesPlanInput) { in.IPv6 = IPv6Follow }},
		{"ipv6-block", func(in *RulesPlanInput) { in.IPv6 = IPv6Block }},
		{"ipv6-invalid", func(in *RulesPlanInput) { in.IPv6 = "other" }},
		{"failure-block", func(in *RulesPlanInput) { in.Failure = FailureBlockProxy }},
		{"failure-invalid", func(in *RulesPlanInput) { in.Failure = "other" }},
		{"lan-absent", func(in *RulesPlanInput) { in.LANInterface = "" }},
		{"lan-wildcard", func(in *RulesPlanInput) { in.LANInterface = "br-lan+" }},
		{"lan-option", func(in *RulesPlanInput) { in.LANInterface = "--help" }},
		{"lan-injection", func(in *RulesPlanInput) { in.LANInterface = "br-lan;reboot" }},
		{"lan-long", func(in *RulesPlanInput) { in.LANInterface = strings.Repeat("a", 16) }},
		{"no-prefixes", func(in *RulesPlanInput) { in.LANIPv4Prefixes = nil }},
		{"duplicate", func(in *RulesPlanInput) { in.LANIPv4Prefixes = []string{"192.168.31.0/24", "192.168.31.0/24"} }},
		{"overlap", func(in *RulesPlanInput) { in.LANIPv4Prefixes = []string{"192.168.31.0/24", "192.168.31.243/32"} }},
		{"host-bits", func(in *RulesPlanInput) { in.LANIPv4Prefixes = []string{"192.168.31.1/24"} }},
		{"unsafe-default-ports", func(in *RulesPlanInput) { in.Ports = Ports{} }},
		{"zero-mixed", func(in *RulesPlanInput) { in.Ports.Mixed = 0 }},
		{"zero-dns", func(in *RulesPlanInput) { in.Ports.DNS = 0 }},
		{"shared-listeners", func(in *RulesPlanInput) { in.Ports.Mixed = in.Ports.DNS }},
		{"tun-wildcard", func(in *RulesPlanInput) { in.TUNInterface = "b6p-tun+" }},
		{"foreign-tun", func(in *RulesPlanInput) { in.TUNInterface = "tun0" }},
		{"tun-not-private", func(in *RulesPlanInput) { in.TUNAddress = "203.0.113.1/30" }},
		{"tun-not-first-host", func(in *RulesPlanInput) { in.TUNAddress = "172.31.255.254/30" }},
		{"tun-in-lan", func(in *RulesPlanInput) { in.LANIPv4Prefixes = []string{"172.31.255.0/24"} }},
		{"tun-containing-lan32", func(in *RulesPlanInput) { in.LANIPv4Prefixes = []string{"172.31.255.254/32"} }},
		{"tun-overlap-second-segment", func(in *RulesPlanInput) { in.LANIPv4Prefixes = append(in.LANIPv4Prefixes, "172.16.0.0/12") }},
		{"management-tun-collision", func(in *RulesPlanInput) { in.ManagementIPs = append(in.ManagementIPs, "172.31.255.254") }},
		{"endpoint-tun-collision", func(in *RulesPlanInput) { in.EndpointIPs = append(in.EndpointIPs, "172.31.255.253") }},
		{"invalid-endpoint", func(in *RulesPlanInput) { in.EndpointIPs = []string{"203.0.113.0/24"} }},
		{"invalid-management", func(in *RulesPlanInput) { in.ManagementIPs = []string{"router.local"} }},
		{"invalid-router-dns", func(in *RulesPlanInput) { in.RouterDNSAddresses = []string{"192.168.31.2"} }},
		{"bounded-endpoints", func(in *RulesPlanInput) { in.EndpointIPs = make([]string, 257) }},
		{"bounded-management", func(in *RulesPlanInput) { in.ManagementIPs = make([]string, 129) }},
		{"bounded-router-dns", func(in *RulesPlanInput) { in.RouterDNSAddresses = make([]string, 17) }},
	} {
		t.Run(tc.name, func(t *testing.T) {
			in := gatewayTestInput()
			tc.edit(&in)
			plan, err := PlanOwnedRules(in)
			if err == nil || !reflect.DeepEqual(plan, OwnedRulesPlan{}) {
				t.Fatalf("unsafe gateway input produced argv: err=%v plan=%+v", err, plan)
			}
		})
	}
}

func TestGatewayDeclaredPrefixRequiresNoDeviceInventory(t *testing.T) {
	in := gatewayTestInput()
	in.LANIPv4Prefixes = []string{"192.168.50.0/24", "192.168.31.0/24"}
	in.ManagementIPs = append(in.ManagementIPs, "192.168.31.9")
	in.IPv6, in.Failure = "", ""
	original := slices.Clone(in.LANIPv4Prefixes)
	plan := ownedTestPlan(t, in)
	if plan.Ownership.Scope != CaptureScopeGateway || len(plan.Ownership.Chains) != 6 ||
		!slices.Equal(plan.Ownership.LANIPv4Prefixes, []string{"192.168.31.0/24", "192.168.50.0/24"}) ||
		plan.Ownership.ClientIPv4 != "" || plan.Ownership.ClientIPv6 != "" || plan.Ownership.ClientIPv4s != nil || plan.Ownership.ClientIPv6s != nil || plan.Ownership.ClientMACs != nil {
		t.Fatalf("wrong declared-prefix ownership: %+v", plan.Ownership)
	}
	for _, prefix := range plan.Ownership.LANIPv4Prefixes {
		for _, command := range [][]string{
			{"ip", "-4", "rule", "add", "priority", "16500", "from", prefix, "iif", "br-lan", "fwmark", "0x4000/0x4000", "lookup", "16500"},
			ownedRoutedTUNHook("nat", "PREROUTING", "-i", "br-lan", "-s", prefix, "-j", ownedTUNDNS),
			ownedRoutedTUNHook("mangle", "PREROUTING", "-i", "br-lan", "-s", prefix, "-j", ownedTUNMark),
			ownedRoutedTUNHook("filter", "FORWARD", "-i", "br-lan", "-s", prefix, "-o", "b6p-tun", "-m", "mark", "--mark", "0x4000/0x4000", "-p", "tcp", "-j", ownedTUNForward),
			ownedRoutedTUNHook("filter", "FORWARD", "-i", "b6p-tun", "-o", "br-lan", "-d", prefix, "-p", "udp", "-j", ownedTUNReturn),
		} {
			index := ownedFindCommand(plan.Apply, command...)
			if index < 0 || !slices.Equal(plan.Apply[index], command) {
				t.Fatalf("missing exact declared-prefix argv: %v", command)
			}
		}
	}
	if !slices.Equal(plan.Apply[0], ownedRoutedTUNRoute("b6p-tun", "add")) {
		t.Fatal("gateway did not reuse ordinary owned TUN route")
	}
	second := ownedTestPlan(t, in)
	plan.Ownership.LANIPv4Prefixes[0] = "changed"
	if !slices.Equal(in.LANIPv4Prefixes, original) || second.Ownership.LANIPv4Prefixes[0] != "192.168.31.0/24" {
		t.Fatal("gateway ownership aliases caller or another plan")
	}
	for _, value := range []uint16{0, 2081, 7893, in.Ports.Mixed, in.Ports.DNS} {
		in.Ports.TProxy = value
		if got := ownedTestPlan(t, in); !reflect.DeepEqual(got, second) {
			t.Fatalf("unused TProxy port %d changed gateway plan", value)
		}
	}
	in.Ports.Mixed = 7893
	ownedTestPlan(t, in) // Unused TProxy is not a reservation against real ports.
	in.Ports.Mixed, in.Ports.DNS = 2080, 7893
	ownedTestPlan(t, in)
}

// These tests walk pure planner argv. They do not send packets, emulate kernel
// routing/conntrack or claim live TUN/offload qualification.
func TestGatewayNewSourcesMatchOnlyDeclaredInterfaceAndPrefixes(t *testing.T) {
	in := gatewayTestInput()
	in.LANIPv4Prefixes = append(in.LANIPv4Prefixes, "192.168.50.0/24")
	plan := ownedTestPlan(t, in)
	for _, source := range []string{"192.168.31.42", "192.168.31.199", "192.168.50.8"} {
		for _, protocol := range []string{"tcp", "udp"} {
			packet := routedTUNPacket{source: source, destination: "8.8.8.8", protocol: protocol, port: 443, incoming: "br-lan", outgoing: "b6p-tun", mac: "02:ff:ff:ff:ff:ff", mark: 0xfabc123f}
			original := packet
			routedTUNWalkHook(t, plan, "mangle", "PREROUTING", &packet)
			if routedTUNPacketPolicyRoute(plan, packet) != "b6p-tun" || routedTUNWalkHook(t, plan, "filter", "FORWARD", &packet) != "ACCEPT" {
				t.Fatalf("new source within declaration did not match: %+v", packet)
			}
			if packet.mark&^CaptureMask != original.mark&^CaptureMask || packet.source != original.source || packet.destination != original.destination || packet.port != original.port || packet.protocol != original.protocol {
				t.Fatal("gateway changed original packet headers or unrelated mark bits")
			}
			packet.port = 53
			routedTUNWalkHook(t, plan, "nat", "PREROUTING", &packet)
			if packet.redirect != int(in.Ports.DNS) {
				t.Fatal("new declared source did not reach managed DNS")
			}
			ret := routedTUNPacket{source: "8.8.8.8", destination: source, protocol: protocol, incoming: "b6p-tun", outgoing: "br-lan"}
			if routedTUNWalkHook(t, plan, "filter", "FORWARD", &ret) != "ACCEPT" {
				t.Fatal("new declared source missing narrow return permission")
			}
		}
	}
	for _, tc := range []struct{ source, incoming string }{
		{"192.168.32.199", "br-lan"}, {"192.168.31.199", "guest"}, {"192.168.31.199", "iot"}, {"192.168.31.199", "wan"},
	} {
		packet := routedTUNPacket{source: tc.source, destination: "8.8.8.8", protocol: "udp", port: 53, incoming: tc.incoming, outgoing: "b6p-tun"}
		routedTUNWalkHook(t, plan, "mangle", "PREROUTING", &packet)
		if packet.mark != 0 || routedTUNWalkHook(t, plan, "nat", "PREROUTING", &packet) != "FACTORY" {
			t.Fatalf("outside/guest source captured: %+v", packet)
		}
		packet.mark = CaptureMark
		if routedTUNWalkHook(t, plan, "filter", "FORWARD", &packet) != "FACTORY" {
			t.Fatal("outside/guest source received forwarding permission")
		}
	}
	for _, packet := range []routedTUNPacket{
		{source: "8.8.8.8", destination: "192.168.32.8", protocol: "tcp", incoming: "b6p-tun", outgoing: "br-lan"},
		{source: "8.8.8.8", destination: "192.168.31.8", protocol: "tcp", incoming: "b6p-tun", outgoing: "guest"},
		{source: "8.8.8.8", destination: "192.168.31.8", protocol: "tcp", incoming: "foreign-tun", outgoing: "br-lan"},
		{source: "8.8.8.8", destination: "192.168.31.8", protocol: "icmp", incoming: "b6p-tun", outgoing: "br-lan"},
		{source: "192.168.31.8", destination: "8.8.8.8", protocol: "udp", incoming: "br-lan", outgoing: "b6p-tun"},
		{source: "192.168.31.8", destination: "8.8.8.8", protocol: "tcp", incoming: "br-lan", outgoing: "wan", mark: CaptureMark},
	} {
		if routedTUNWalkHook(t, plan, "filter", "FORWARD", &packet) != "FACTORY" {
			t.Fatalf("gateway forwarding scope widened: %+v", packet)
		}
	}
}

func TestGatewayManagementReturnBypassAndSharedDNSOrdering(t *testing.T) {
	in := gatewayTestInput()
	in.ManagementIPs = append(in.ManagementIPs, "192.168.31.9", "2001:db8::1")
	in.RouterDNSAddresses = []string{"192.168.31.1"}
	plan := ownedTestPlan(t, in)
	for _, destination := range []string{"192.168.31.1", "192.168.31.9"} {
		bypass := ownedFindCommand(plan.Apply, "-A", ownedTUNReturn, "-d", destination+"/32", "-j", "RETURN")
		accept := ownedFindCommand(plan.Apply, "-A", ownedTUNReturn, "-p", "tcp", "-j", "ACCEPT")
		if bypass < 0 || bypass >= accept {
			t.Fatal("management return bypass must precede shared ACCEPT")
		}
		packet := routedTUNPacket{source: "8.8.8.8", destination: destination, protocol: "tcp", incoming: "b6p-tun", outgoing: "br-lan"}
		if routedTUNWalkHook(t, plan, "filter", "FORWARD", &packet) != "FACTORY" {
			t.Fatal("management destination received gateway return ACCEPT")
		}
	}
	for _, tc := range []struct {
		destination      string
		port             int
		local, mark, dns bool
	}{
		{"8.8.8.8", 443, false, true, false},
		{"192.168.31.1", 22, true, false, false},
		{"192.168.31.9", 8787, false, false, false},
		{"10.1.2.3", 443, false, false, false},
		{"203.0.113.20", 443, false, false, false},
		{"169.254.2.3", 443, false, false, false},
		{"8.8.8.8", 53, false, false, true},
		{"192.168.31.1", 53, true, false, true},
		{"192.168.31.9", 53, true, false, false},
		{"192.168.31.10", 53, true, false, false},
		{"10.1.2.3", 53, false, false, true},
		{"203.0.113.20", 53, false, false, true},
		{"169.254.2.3", 53, false, false, false},
	} {
		for _, protocol := range []string{"tcp", "udp"} {
			t.Run(fmt.Sprintf("%s-%s-%d", tc.destination, protocol, tc.port), func(t *testing.T) {
				packet := routedTUNPacket{source: "192.168.31.199", destination: tc.destination, protocol: protocol, port: tc.port, incoming: "br-lan", local: tc.local}
				routedTUNWalkHook(t, plan, "mangle", "PREROUTING", &packet)
				routedTUNWalkHook(t, plan, "nat", "PREROUTING", &packet)
				if (packet.mark == CaptureMark) != tc.mark || (packet.redirect == int(in.Ports.DNS)) != tc.dns {
					t.Fatalf("shared DNS/bypass order changed: %+v", packet)
				}
			})
		}
	}
	in.FakeIP = true
	fake := ownedTestPlan(t, in)
	packet := routedTUNPacket{source: "192.168.31.199", destination: "198.18.1.1", protocol: "tcp", port: 443, incoming: "br-lan"}
	routedTUNWalkHook(t, fake, "mangle", "PREROUTING", &packet)
	if routedTUNPacketPolicyRoute(fake, packet) != "b6p-tun" {
		t.Fatal("shared FakeIP ordering changed")
	}
}

func TestGatewaySynthetic32CanaryAndEightSegments(t *testing.T) {
	in := gatewayTestInput()
	in.LANIPv4Prefixes = []string{"192.168.31.243/32"}
	plan := ownedTestPlan(t, in)
	for _, tc := range []struct {
		source  string
		capture bool
	}{{"192.168.31.243", true}, {"192.168.31.242", false}, {"192.168.31.244", false}} {
		packet := routedTUNPacket{source: tc.source, destination: "8.8.8.8", protocol: "tcp", incoming: "br-lan"}
		routedTUNWalkHook(t, plan, "mangle", "PREROUTING", &packet)
		if (routedTUNPacketPolicyRoute(plan, packet) == "b6p-tun") != tc.capture {
			t.Fatalf("synthetic /32 canary scope changed: %+v", packet)
		}
	}
	in.LANIPv4Prefixes = nil
	for i := 0; i < 8; i++ {
		in.LANIPv4Prefixes = append(in.LANIPv4Prefixes, fmt.Sprintf("10.%d.0.0/16", i))
	}
	plan = ownedTestPlan(t, in)
	slices.Reverse(in.LANIPv4Prefixes)
	if got := ownedTestPlan(t, in); !reflect.DeepEqual(got, plan) || len(got.Ownership.LANIPv4Prefixes) != 8 || len(got.Ownership.Chains) != 6 {
		t.Fatal("maximum declared scope changed or is nondeterministic")
	}
}

func TestGatewayApplyCleanupInverseAndBoundedCommands(t *testing.T) {
	in := gatewayTestInput()
	in.LANIPv4Prefixes = append(in.LANIPv4Prefixes, "192.168.50.0/24")
	plan := ownedTestPlan(t, in)
	firstHook := -1
	for i, argv := range plan.Apply {
		if argv[0] == "iptables" && argv[5] == "-I" {
			if firstHook == -1 {
				firstHook = i
			}
		} else if firstHook != -1 {
			t.Fatalf("resource preparation follows external hook: %v", argv)
		}
	}
	if firstHook < 0 {
		t.Fatal("missing external hooks")
	}
	for i, argv := range plan.Apply[firstHook:] {
		want := slices.Clone(argv)
		want[5] = "-D"
		want = append(want[:7], want[8:]...)
		if !slices.Equal(plan.Cleanup[len(plan.Apply)-firstHook-1-i], want) {
			t.Fatalf("prefix hook cleanup not exact inverse: %v", argv)
		}
	}
	if plan.Cleanup[0][4] != "mangle" || plan.Cleanup[1][4] != "mangle" || plan.Cleanup[2][4] != "nat" || plan.Cleanup[3][4] != "nat" {
		t.Fatal("MARK entry must withdraw first, then DNS")
	}
	for stop := 0; stop <= len(plan.Apply); stop++ {
		runner := newRoutedTUNFakeRunner()
		for _, argv := range plan.Apply[:stop] {
			if err := runner.run(argv); err != nil {
				t.Fatalf("apply prefix %d: %v", stop, err)
			}
		}
		for _, argv := range plan.Cleanup {
			if err := runner.run(argv); stop == len(plan.Apply) && err != nil {
				t.Fatalf("cleanup: %v", err)
			}
		}
		if !runner.empty() {
			t.Fatalf("owned resource remained after apply prefix %d", stop)
		}
	}
	if !reflect.DeepEqual(plan.Cleanup, plan.OnFailure) {
		t.Fatal("gateway fail-direct is not exact withdrawal")
	}
	for _, commands := range [][][]string{plan.Apply, plan.Cleanup, plan.OnFailure} {
		for _, argv := range commands {
			if argv[0] == "ip" {
				if argv[1] != "-4" || argv[2] != "rule" && argv[2] != "route" || argv[3] != "add" && argv[3] != "del" || !slices.Contains(argv, "16500") || slices.Contains(argv, "main") {
					t.Fatalf("unowned routing or lifecycle operation: %v", argv)
				}
			} else if argv[0] != "iptables" {
				t.Fatalf("unexpected executable: %v", argv)
			} else if argv[5] == "-I" || argv[5] == "-D" {
				if argv[6] == "OUTPUT" && (argv[4] != "filter" || !ownedHasArgs(argv, "-o", "b6p-tun", "-s", "172.31.255.253/32", "-d", "172.31.255.254/32", "-p", "tcp", "-j", ownedTUNOutput)) {
					t.Fatalf("router OUTPUT capture or blanket TUN permission: %v", argv)
				}
			} else if !strings.HasPrefix(argv[6], "B6P_") {
				t.Fatalf("factory chain mutation: %v", argv)
			}
			for _, token := range argv {
				if slices.Contains([]string{"--mac-source", "TPROXY", "CONNMARK", "SNAT", "MASQUERADE", "DROP", "REJECT", "POSTROUTING", "flush", "--flush"}, token) {
					t.Fatalf("inventory dependency or unrequested side effect: %v", argv)
				}
			}
		}
	}
	for i := range plan.Cleanup {
		before := plan.OnFailure[i][0]
		plan.Cleanup[i][0] = "changed"
		if plan.OnFailure[i][0] != before {
			t.Fatal("OnFailure argv aliases Cleanup")
		}
	}
}

func TestGatewayLegacyRoutedTUNPlanFixtures(t *testing.T) {
	// Complete pre-change hashes pin both cleanup argv and old device journal
	// metadata. Gateway must not alter either singular or plural device intent.
	for _, tc := range []struct {
		plural bool
		want   string
	}{
		{false, "baa881269b93f60e4cd480d71febdcad7d4183fee82e03e28557a362a3ac3fbe"},
		{true, "d3283a63cc46791428c0ccc092b8711e51f99e4cb242acd31235a085fa9e3652"},
	} {
		in := routedTUNTestInput()
		if tc.plural {
			in.ClientIPv4 = ""
			in.ClientIPv4s = []string{"192.168.31.43", "192.168.31.42"}
			in.ClientMACs["192.168.31.43"] = "02:11:22:33:44:43"
		}
		plan := ownedTestPlan(t, in)
		raw, err := json.Marshal(plan)
		if err != nil {
			t.Fatal(err)
		}
		hash := sha256.Sum256(raw)
		if hex.EncodeToString(hash[:]) != tc.want {
			t.Fatalf("old device plan bytes changed: %x", hash)
		}
		in.Scope = CaptureScopeDevices
		if !reflect.DeepEqual(ownedTestPlan(t, in), plan) {
			t.Fatal("explicit devices scope changed old withdrawal shape")
		}
		var journal OwnedRulesPlan
		if err := json.Unmarshal(raw, &journal); err != nil {
			t.Fatal(err)
		}
		if !reflect.DeepEqual(journal.Ownership, plan.Ownership) || !reflect.DeepEqual(journal.Cleanup, plan.Cleanup) {
			t.Fatal("old journal cleanup changed")
		}
	}
}

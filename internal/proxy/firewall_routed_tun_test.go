package proxy

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	stdmaps "maps"
	"net/netip"
	"reflect"
	"slices"
	"strings"
	"testing"
)

func routedTUNTestInput() RulesPlanInput {
	in := ownedTestInput()
	in.Datapath = DatapathRoutedTUN
	in.TUNInterface = "b6p-tun"
	in.TUNAddress = "172.31.255.253/30"
	in.FakeIP = false
	in.Ports.TProxy = 7893
	in.ClientMACs = map[string]string{in.ClientIPv4: "02:11:22:33:44:42"}
	return in
}

func TestOwnedRoutedTUNOrdinaryRouteAndOwnedChainContract(t *testing.T) {
	in := routedTUNTestInput()
	plan := ownedTestPlan(t, in)
	wantChains := []OwnedChain{
		{4, "mangle", "B6P_V4_TUN_MARK", "PREROUTING"},
		{4, "nat", "B6P_V4_DNS", "PREROUTING"},
		{4, "filter", "B6P_V4_TUN_FORWARD", "FORWARD"},
		{4, "filter", "B6P_V4_TUN_RETURN", "FORWARD"},
		{4, "filter", "B6P_V4_TUN_INPUT", "INPUT"},
		{4, "filter", "B6P_V4_TUN_OUTPUT", "OUTPUT"},
	}
	if !reflect.DeepEqual(plan.Ownership.Chains, wantChains) || plan.Ownership.Datapath != DatapathRoutedTUN ||
		plan.Ownership.TUNInterface != in.TUNInterface || plan.Ownership.TUNAddress != in.TUNAddress ||
		!slices.Equal(plan.Ownership.RouteFamilies, []int{4}) || !stdmaps.Equal(plan.Ownership.ClientMACs, in.ClientMACs) {
		t.Fatalf("wrong routed-TUN ownership: %+v", plan.Ownership)
	}
	if ownedFindCommand(plan.Apply, "ip", "-4", "route", "add", "default", "dev", "b6p-tun", "table", "16500") != 0 {
		t.Fatal("ordinary default dev TUN must be prepared first")
	}
	for _, command := range plan.Apply {
		if slices.Contains(command, "TPROXY") || slices.Contains(command, "--on-port") || slices.Contains(command, "CONNMARK") || ownedHasArgs(command, "dev", "lo") {
			t.Fatalf("routed original packet unexpectedly uses TPROXY/local loopback routing: %v", command)
		}
		if ownedHasArgs(command, "-j", "MARK") && !ownedHasArgs(command, "--set-xmark", "0x4000/0x4000") {
			t.Fatalf("MARK changes unrelated bits: %v", command)
		}
		if command[0] == "ip" && command[2] == "rule" && !ownedHasArgs(command, "from", "192.168.31.42/32", "iif", "br-lan", "fwmark", "0x4000/0x4000", "lookup", "16500") {
			t.Fatalf("policy route is not source+interface+mark scoped: %v", command)
		}
	}
	warnings := strings.Join(plan.Warnings, "\n")
	for _, limit := range []string{"IPv6 direct", "TCP/UDP", "conntrack", "readiness", "No interface", "QUIC", "ECM/PPE/SFE"} {
		if !strings.Contains(warnings, limit) {
			t.Errorf("missing explicit acceptance limit %q", limit)
		}
	}
}

func TestOwnedRoutedTUNValidatesExplicitFields(t *testing.T) {
	for _, tc := range []struct {
		name string
		edit func(*RulesPlanInput)
	}{
		{"unknown-datapath", func(in *RulesPlanInput) { in.Datapath = "routed-tun; reboot" }},
		{"absent-tun-interface", func(in *RulesPlanInput) { in.TUNInterface = "" }},
		{"absent-tun-address", func(in *RulesPlanInput) { in.TUNAddress = "" }},
		{"lan-as-tun", func(in *RulesPlanInput) { in.TUNInterface = in.LANInterface }},
		{"owned-lan-as-tun", func(in *RulesPlanInput) { in.LANInterface = in.TUNInterface }},
		{"loopback-as-tun", func(in *RulesPlanInput) { in.TUNInterface = "lo" }},
		{"foreign-name", func(in *RulesPlanInput) { in.TUNInterface = "tun0" }},
		{"bare-owned-prefix", func(in *RulesPlanInput) { in.TUNInterface = "b6p-" }},
		{"long-name", func(in *RulesPlanInput) { in.TUNInterface = "b6p-" + strings.Repeat("a", 12) }},
		{"wildcard-name", func(in *RulesPlanInput) { in.TUNInterface = "b6p-tun+" }},
		{"option-name", func(in *RulesPlanInput) { in.TUNInterface = "--help" }},
		{"alias-name", func(in *RulesPlanInput) { in.TUNInterface = "b6p-tun:1" }},
		{"dot-name", func(in *RulesPlanInput) { in.TUNInterface = "b6p-tun.1" }},
		{"compatibility-port", func(in *RulesPlanInput) { in.Ports.TProxy = 2081 }},
		{"command-name", func(in *RulesPlanInput) { in.TUNInterface = "b6p-tun;reboot" }},
		{"newline-name", func(in *RulesPlanInput) { in.TUNInterface = "b6p-tun\n" }},
		{"prefixless-address", func(in *RulesPlanInput) { in.TUNAddress = "172.31.255.253" }},
		{"public-address", func(in *RulesPlanInput) { in.TUNAddress = "203.0.113.1/30" }},
		{"cgnat-address", func(in *RulesPlanInput) { in.TUNAddress = "100.64.0.1/30" }},
		{"loopback-address", func(in *RulesPlanInput) { in.TUNAddress = "127.0.0.1/30" }},
		{"linklocal-address", func(in *RulesPlanInput) { in.TUNAddress = "169.254.0.1/30" }},
		{"ipv6-address", func(in *RulesPlanInput) { in.TUNAddress = "fd00::1/126" }},
		{"mapped-address", func(in *RulesPlanInput) { in.TUNAddress = "::ffff:172.31.255.253/126" }},
		{"subnet-address", func(in *RulesPlanInput) { in.TUNAddress = "172.31.255.252/30" }},
		{"second-host-address", func(in *RulesPlanInput) { in.TUNAddress = "172.31.255.254/30" }},
		{"broadcast-address", func(in *RulesPlanInput) { in.TUNAddress = "172.31.255.255/30" }},
		{"wrong-prefix-size", func(in *RulesPlanInput) { in.TUNAddress = "172.31.255.253/29" }},
		{"command-address", func(in *RulesPlanInput) { in.TUNAddress = "172.31.255.253/30; reboot" }},
		{"padded-address", func(in *RulesPlanInput) { in.TUNAddress = " 172.31.255.253/30" }},
		{"management-collision", func(in *RulesPlanInput) { in.ManagementIPs = append(in.ManagementIPs, "172.31.255.254") }},
		{"client-collision", func(in *RulesPlanInput) { in.TUNAddress = "192.168.31.41/30" }},
		{"endpoint-collision", func(in *RulesPlanInput) { in.EndpointIPs = append(in.EndpointIPs, "172.31.255.253") }},
		{"follow", func(in *RulesPlanInput) {
			in.IPv6, in.ClientIPv6 = IPv6Follow, "2001:db8::42"
			in.ClientMACs[in.ClientIPv6] = "02:11:22:33:44:42"
		}},
		{"block", func(in *RulesPlanInput) {
			in.IPv6, in.ClientIPv6 = IPv6Block, "2001:db8::42"
			in.ClientMACs[in.ClientIPv6] = "02:11:22:33:44:42"
		}},
		{"partial-ports", func(in *RulesPlanInput) { in.Ports.Mixed = 0 }},
		{"conflicting-ports", func(in *RulesPlanInput) { in.Ports.TProxy = in.Ports.DNS }},
		{"missing-mac-binding", func(in *RulesPlanInput) { in.ClientMACs = nil }},
		{"wrong-mac-binding", func(in *RulesPlanInput) { in.ClientMACs = map[string]string{"192.168.31.43": "02:11:22:33:44:43"} }},
		{"mac-command", func(in *RulesPlanInput) { in.ClientMACs[in.ClientIPv4] = "02:11:22:33:44:42; reboot" }},
	} {
		t.Run(tc.name, func(t *testing.T) {
			in := routedTUNTestInput()
			tc.edit(&in)
			plan, err := PlanOwnedRules(in)
			if err == nil || !reflect.DeepEqual(plan, OwnedRulesPlan{}) {
				t.Fatalf("invalid fields yielded executable plan: err=%v plan=%+v", err, plan)
			}
		})
	}
	for _, name := range []string{"b6p-tun", "b6p-tun_1", "b6p-" + strings.Repeat("a", 11)} {
		in := routedTUNTestInput()
		in.TUNInterface = name
		in.Ports, in.IPv6, in.Failure = Ports{}, "", ""
		plan := ownedTestPlan(t, in)
		if ownedFindCommand(plan.Apply, "--to-ports", "1053") < 0 || !reflect.DeepEqual(plan.Cleanup, plan.OnFailure) {
			t.Fatal("routed-TUN default ports/policy do not match existing defaults")
		}
	}
}

func TestOwnedRoutedTUNDefaultPlannerBytesAndLegacyJournalUnchanged(t *testing.T) {
	// These hashes cover the complete pre-dispatch planner result at e394cb5,
	// including Apply, ownership, warnings and recovery command ordering.
	// This is old-journal compatibility by exact content, not a version switch.
	for _, tc := range []struct {
		mode IPv6Mode
		want string
	}{
		{IPv6Direct, "59b63f4cf04d5ff5446869bf1358342ebd6ae2f44d1e0817c558186739d5f0ad"}, {IPv6Follow, "d101654c4a4268a9581f2d999e9648ed09db674dc669756378c2c4646b1b81b8"}, {IPv6Block, "f437895ee6efd83b83c5c5bc1cf22273775c85c7c36729deb0da7976e8b75888"},
	} {
		in := ownedTestInput()
		in.IPv6, in.ClientIPv6 = tc.mode, "2001:db8::42"
		empty := ownedTestPlan(t, in)
		raw, err := json.Marshal(empty)
		if err != nil {
			t.Fatal(err)
		}
		hash := sha256.Sum256(raw)
		if hex.EncodeToString(hash[:]) != tc.want {
			t.Fatalf("legacy %s plan bytes changed: %x", tc.mode, hash)
		}
		in.Datapath = DatapathTPROXY
		explicit := ownedTestPlan(t, in)
		explicitRaw, _ := json.Marshal(explicit)
		if !slices.Equal(raw, explicitRaw) {
			t.Fatal("explicit TPROXY changed default ownership/intent")
		}
		var journal OwnedRulesPlan
		if err := json.Unmarshal(raw, &journal); err != nil {
			t.Fatal(err)
		}
		if !reflect.DeepEqual(empty.Ownership, journal.Ownership) || !reflect.DeepEqual(empty.Cleanup, journal.Cleanup) {
			t.Fatal("old journal recovery no longer exact")
		}
		if empty.Ownership.Datapath != "" || empty.Ownership.TUNInterface != "" || empty.Ownership.TUNAddress != "" {
			t.Fatal("legacy journal gained backend ownership")
		}
	}
	for _, mode := range []DatapathMode{"", DatapathTPROXY} {
		in := ownedTestInput()
		in.Datapath, in.TUNInterface, in.TUNAddress = mode, "b6p-tun", "172.31.255.253/30"
		if _, err := PlanOwnedRules(in); err == nil {
			t.Fatal("inferred backend from fields without explicit routed-tun opt-in")
		}
	}
}

// This walker checks generated match/target intent only. It does not emulate
// kernel routing, NAT conntrack, system-TUN internals or router offload.
type routedTUNPacket struct {
	source, destination, protocol, incoming, outgoing, mac string
	port, redirect                                         int
	mark                                                   uint32
	local                                                  bool
}

func routedTUNPacketMatch(argv []string, packet routedTUNPacket) bool {
	for i := 7; i < len(argv); i++ {
		switch argv[i] {
		case "-i":
			i++
			if argv[i] != packet.incoming {
				return false
			}
		case "-o":
			i++
			if argv[i] != packet.outgoing {
				return false
			}
		case "-s":
			i++
			if !netip.MustParsePrefix(argv[i]).Contains(netip.MustParseAddr(packet.source)) {
				return false
			}
		case "-d":
			i++
			if !netip.MustParsePrefix(argv[i]).Contains(netip.MustParseAddr(packet.destination)) {
				return false
			}
		case "-p":
			i++
			if argv[i] != packet.protocol {
				return false
			}
		case "--dport":
			i++
			if argv[i] != fmt.Sprint(packet.port) {
				return false
			}
		case "--mac-source":
			i++
			if argv[i] != packet.mac {
				return false
			}
		case "--mark":
			i++
			if argv[i] != "0x4000/0x4000" || packet.mark&CaptureMask != CaptureMark {
				return false
			}
		case "--dst-type":
			i++
			if argv[i] != "LOCAL" || !packet.local {
				return false
			}
		}
	}
	return true
}

func routedTUNWalkChain(t *testing.T, commands [][]string, table, chain string, packet *routedTUNPacket) string {
	t.Helper()
	for _, argv := range commands {
		if argv[0] != "iptables" || argv[4] != table || argv[5] != "-A" || argv[6] != chain || !routedTUNPacketMatch(argv, *packet) {
			continue
		}
		index := slices.Index(argv, "-j")
		if index < 0 {
			t.Fatalf("rule has no target: %v", argv)
		}
		switch target := argv[index+1]; target {
		case "MARK":
			if !ownedHasArgs(argv, "--set-xmark", "0x4000/0x4000") {
				t.Fatalf("unqualified mark mutation: %v", argv)
			}
			packet.mark = packet.mark&^CaptureMask | CaptureMark
		case "REDIRECT":
			fmt.Sscan(argv[len(argv)-1], &packet.redirect)
			return target
		case "RETURN", "ACCEPT":
			return target
		default:
			t.Fatalf("unrecognized target %q", target)
		}
	}
	return "RETURN"
}

func routedTUNWalkHook(t *testing.T, plan OwnedRulesPlan, table, hook string, packet *routedTUNPacket) string {
	t.Helper()
	// Every -I hook is inserted at position 1, so runtime order is reversed.
	for i := len(plan.Apply) - 1; i >= 0; i-- {
		argv := plan.Apply[i]
		if argv[0] != "iptables" || argv[4] != table || argv[5] != "-I" || argv[6] != hook || !routedTUNPacketMatch(argv, *packet) {
			continue
		}
		target := routedTUNWalkChain(t, plan.Apply, table, argv[len(argv)-1], packet)
		if target != "RETURN" {
			return target
		}
	}
	return "FACTORY"
}

func routedTUNPacketPolicyRoute(plan OwnedRulesPlan, packet routedTUNPacket) string {
	for _, argv := range plan.Apply {
		if argv[0] == "ip" && argv[2] == "rule" && argv[3] == "add" &&
			netip.MustParsePrefix(argv[7]).Contains(netip.MustParseAddr(packet.source)) &&
			argv[9] == packet.incoming && packet.mark&CaptureMask == CaptureMark {
			return plan.Ownership.TUNInterface
		}
	}
	return "factory"
}

func TestOwnedRoutedTUNOriginalPacketAndDNSPrelude(t *testing.T) {
	in := routedTUNTestInput()
	in.RouterDNSAddresses = []string{"192.168.31.1"}
	in.ManagementIPs = append(in.ManagementIPs, "192.168.31.9")
	plan := ownedTestPlan(t, in)
	for _, tc := range []struct {
		destination, protocol string
		port                  int
		local, capture, dns   bool
	}{
		{"8.8.8.8", "tcp", 443, false, true, false},
		{"1.1.1.1", "udp", 444, false, true, false},
		{"8.8.8.8", "tcp", 22, false, true, false},
		{"203.0.113.20", "tcp", 443, false, false, false},
		{"192.168.31.1", "tcp", 22, true, false, false},
		{"192.168.31.1", "tcp", 8787, true, false, false},
		{"10.1.2.3", "tcp", 443, false, false, false},
		{"172.16.2.3", "udp", 444, false, false, false},
		{"192.168.50.2", "tcp", 443, false, false, false},
		{"100.64.2.3", "tcp", 443, false, false, false},
		{"198.18.1.1", "tcp", 443, false, false, false},
		{"169.254.2.3", "udp", 444, false, false, false},
		{"224.0.0.1", "udp", 443, false, false, false},
		{"240.0.0.1", "udp", 443, false, false, false},
		{"8.8.8.8", "icmp", 0, false, false, false},
		{"8.8.8.8", "esp", 0, false, false, false},
		{"8.8.8.8", "gre", 0, false, false, false},
		{"8.8.8.8", "tcp", 53, false, false, true},
		{"8.8.8.8", "udp", 53, false, false, true},
		{"10.1.2.3", "udp", 53, false, false, true},
		{"203.0.113.20", "tcp", 53, false, false, true},
		{"203.0.113.20", "udp", 53, false, false, true},
		{"192.168.31.1", "udp", 53, true, false, true},
		{"192.168.31.1", "tcp", 53, true, false, true},
		{"192.168.31.9", "udp", 53, true, false, false},
		{"192.168.31.10", "udp", 53, true, false, false},
		{"169.254.2.3", "udp", 53, false, false, false},
		{"224.0.0.1", "udp", 53, false, false, false},
	} {
		t.Run(fmt.Sprintf("%s-%s-%d", tc.destination, tc.protocol, tc.port), func(t *testing.T) {
			packet := routedTUNPacket{source: in.ClientIPv4, destination: tc.destination, protocol: tc.protocol, port: tc.port, incoming: in.LANInterface, mac: in.ClientMACs[in.ClientIPv4], mark: 0xfabc123f, local: tc.local}
			original := packet
			routedTUNWalkHook(t, plan, "mangle", "PREROUTING", &packet)
			if got := routedTUNPacketPolicyRoute(plan, packet) == in.TUNInterface; got != tc.capture {
				t.Fatalf("marked original-packet route capture=%v want=%v mark=%x", got, tc.capture, packet.mark)
			}
			if packet.mark&^CaptureMask != original.mark&^CaptureMask {
				t.Fatal("mark changed QoS/mwan/UU/parent bits")
			}
			if packet.source != original.source || packet.destination != original.destination || packet.port != original.port || packet.protocol != original.protocol {
				t.Fatal("TUN route changed original packet headers")
			}
			routedTUNWalkHook(t, plan, "nat", "PREROUTING", &packet)
			if (packet.redirect == int(in.Ports.DNS)) != tc.dns {
				t.Fatalf("managed DNS=%d wantRedirect=%v", packet.redirect, tc.dns)
			}
		})
	}
	in.FakeIP = true
	fake := ownedTestPlan(t, in)
	packet := routedTUNPacket{source: in.ClientIPv4, destination: "198.18.1.1", protocol: "tcp", port: 443, incoming: in.LANInterface, mac: in.ClientMACs[in.ClientIPv4]}
	routedTUNWalkHook(t, fake, "mangle", "PREROUTING", &packet)
	if routedTUNPacketPolicyRoute(fake, packet) != in.TUNInterface {
		t.Fatal("opt-in fake-IP lost mark before private RETURN")
	}
}

func TestOwnedRoutedTUNExactClientMACReturnAndPrivateStackScope(t *testing.T) {
	in := routedTUNTestInput()
	in.ClientIPv4 = ""
	in.ClientIPv4s = []string{"192.168.31.43", "192.168.31.42"}
	in.ClientMACs["192.168.31.43"] = "02:11:22:33:44:43"
	plan := ownedTestPlan(t, in)
	for source, mac := range in.ClientMACs {
		for _, protocol := range []string{"tcp", "udp"} {
			base := routedTUNPacket{source: source, destination: "8.8.8.8", protocol: protocol, port: 443, incoming: "br-lan", outgoing: "b6p-tun", mac: mac, mark: CaptureMark}
			if routedTUNWalkHook(t, plan, "filter", "FORWARD", &base) != "ACCEPT" {
				t.Fatal("declared marked outgoing client missing scoped permission")
			}
			for _, bad := range []routedTUNPacket{
				{source: source, destination: "8.8.8.8", protocol: protocol, incoming: "br-lan", outgoing: "b6p-tun", mac: "02:ff:ff:ff:ff:ff", mark: CaptureMark},
				{source: source, destination: "8.8.8.8", protocol: protocol, incoming: "br-lan", outgoing: "b6p-tun", mac: mac},
				{source: source, destination: "8.8.8.8", protocol: protocol, incoming: "guest", outgoing: "b6p-tun", mac: mac, mark: CaptureMark},
				{source: source, destination: "8.8.8.8", protocol: protocol, incoming: "br-lan", outgoing: "wan", mac: mac, mark: CaptureMark},
				{source: "192.168.31.99", destination: "8.8.8.8", protocol: protocol, incoming: "br-lan", outgoing: "b6p-tun", mac: mac, mark: CaptureMark},
			} {
				if routedTUNWalkHook(t, plan, "filter", "FORWARD", &bad) != "FACTORY" {
					t.Fatalf("foreign outgoing traffic received permission: %+v", bad)
				}
				bad.mark = 0
				routedTUNWalkHook(t, plan, "mangle", "PREROUTING", &bad)
				if bad.mac != mac || bad.incoming != "br-lan" || bad.source != source {
					if bad.mark != 0 {
						t.Fatal("reused IP/wrong MAC/foreign ingress marked")
					}
					if routedTUNWalkHook(t, plan, "nat", "PREROUTING", &bad) != "FACTORY" {
						t.Fatal("foreign client reached managed DNS hook")
					}
				}
			}
			for _, remote := range []string{"8.8.8.8", "203.0.113.25", "192.168.50.1"} {
				ret := routedTUNPacket{source: remote, destination: source, protocol: protocol, incoming: "b6p-tun", outgoing: "br-lan"}
				if routedTUNWalkHook(t, plan, "filter", "FORWARD", &ret) != "ACCEPT" {
					t.Fatal("normal remote return cannot reach its declared client")
				}
			}
		}
	}
	for _, bad := range []routedTUNPacket{
		{source: "8.8.8.8", destination: "192.168.31.99", protocol: "tcp", incoming: "b6p-tun", outgoing: "br-lan"},
		{source: "8.8.8.8", destination: "192.168.31.42", protocol: "tcp", incoming: "foreign-tun", outgoing: "br-lan"},
		{source: "8.8.8.8", destination: "192.168.31.42", protocol: "tcp", incoming: "b6p-tun", outgoing: "guest"},
		{source: "8.8.8.8", destination: "192.168.31.42", protocol: "icmp", incoming: "b6p-tun", outgoing: "br-lan"},
		{source: "192.168.31.42", destination: "8.8.8.8", protocol: "esp", incoming: "br-lan", outgoing: "b6p-tun", mac: in.ClientMACs["192.168.31.42"], mark: CaptureMark},
	} {
		if routedTUNWalkHook(t, plan, "filter", "FORWARD", &bad) != "FACTORY" {
			t.Fatalf("return scope widened: %+v", bad)
		}
	}
	stackIn := routedTUNPacket{source: "172.31.255.254", destination: "172.31.255.253", protocol: "tcp", incoming: "b6p-tun"}
	stackOut := routedTUNPacket{source: "172.31.255.253", destination: "172.31.255.254", protocol: "tcp", outgoing: "b6p-tun"}
	if routedTUNWalkHook(t, plan, "filter", "INPUT", &stackIn) != "ACCEPT" || routedTUNWalkHook(t, plan, "filter", "OUTPUT", &stackOut) != "ACCEPT" {
		t.Fatal("private TCP stack peer tuple missing")
	}
	for _, tc := range []struct {
		hook   string
		packet routedTUNPacket
	}{
		{"INPUT", routedTUNPacket{source: "172.31.255.254", destination: "172.31.255.253", protocol: "tcp", incoming: "br-lan"}},
		{"INPUT", routedTUNPacket{source: "8.8.8.8", destination: "172.31.255.253", protocol: "tcp", incoming: "b6p-tun"}},
		{"INPUT", routedTUNPacket{source: "172.31.255.254", destination: "192.168.31.1", protocol: "tcp", incoming: "b6p-tun"}},
		{"INPUT", routedTUNPacket{source: "172.31.255.254", destination: "172.31.255.253", protocol: "udp", incoming: "b6p-tun"}},
		{"OUTPUT", routedTUNPacket{source: "172.31.255.253", destination: "172.31.255.254", protocol: "tcp", outgoing: "br-lan"}},
		{"OUTPUT", routedTUNPacket{source: "192.168.31.1", destination: "172.31.255.254", protocol: "tcp", outgoing: "b6p-tun"}},
		{"OUTPUT", routedTUNPacket{source: "172.31.255.253", destination: "8.8.8.8", protocol: "tcp", outgoing: "b6p-tun"}},
		{"OUTPUT", routedTUNPacket{source: "172.31.255.253", destination: "172.31.255.254", protocol: "udp", outgoing: "b6p-tun"}},
	} {
		if routedTUNWalkHook(t, plan, "filter", tc.hook, &tc.packet) != "FACTORY" {
			t.Fatalf("private stack widened %s: %+v", tc.hook, tc.packet)
		}
	}
}

// A stateful fake argv runner checks owned resource creation/removal without
// invoking iptables/ip. Apply is one-way argv intent, not an atomic transaction.
type routedTUNFakeRunner struct {
	chains               map[string][]string
	hooks, routes, rules map[string]bool
}

func newRoutedTUNFakeRunner() *routedTUNFakeRunner {
	return &routedTUNFakeRunner{chains: map[string][]string{}, hooks: map[string]bool{}, routes: map[string]bool{}, rules: map[string]bool{}}
}

func (runner *routedTUNFakeRunner) run(argv []string) error {
	if argv[0] == "ip" {
		key := strings.Join(append(slices.Clone(argv[:3]), argv[4:]...), " ")
		resources := runner.routes
		if argv[2] == "rule" {
			resources = runner.rules
		}
		switch argv[3] {
		case "add":
			if resources[key] {
				return fmt.Errorf("duplicate owned ip resource %s", key)
			}
			resources[key] = true
		case "del":
			if !resources[key] {
				return fmt.Errorf("absent owned ip resource %s", key)
			}
			delete(resources, key)
		default:
			return fmt.Errorf("unexpected ip command %v", argv)
		}
		return nil
	}
	if argv[0] != "iptables" {
		return fmt.Errorf("unexpected executable %v", argv)
	}
	key := argv[4] + "/" + argv[6]
	switch argv[5] {
	case "-N":
		if _, exists := runner.chains[key]; exists {
			return fmt.Errorf("chain exists %s", key)
		}
		runner.chains[key] = nil
	case "-A":
		if _, exists := runner.chains[key]; !exists {
			return fmt.Errorf("chain absent %s", key)
		}
		runner.chains[key] = append(runner.chains[key], strings.Join(argv[7:], " "))
	case "-I", "-D":
		args := argv[7:]
		if argv[5] == "-I" {
			args = argv[8:]
		}
		if _, exists := runner.chains[argv[4]+"/"+argv[len(argv)-1]]; !exists {
			return fmt.Errorf("hook target absent %v", argv)
		}
		hookKey := key + "/" + strings.Join(args, " ")
		if argv[5] == "-I" {
			if runner.hooks[hookKey] {
				return fmt.Errorf("duplicate hook %s", hookKey)
			}
			runner.hooks[hookKey] = true
		} else {
			if !runner.hooks[hookKey] {
				return fmt.Errorf("absent hook %s", hookKey)
			}
			delete(runner.hooks, hookKey)
		}
	case "-F", "-X":
		if _, exists := runner.chains[key]; !exists {
			return fmt.Errorf("absent chain %s", key)
		}
		for hookKey := range runner.hooks {
			if strings.HasSuffix(hookKey, "-j "+argv[6]) {
				return fmt.Errorf("chain still externally hooked: %s", key)
			}
		}
		if argv[5] == "-F" {
			runner.chains[key] = nil
		} else {
			if len(runner.chains[key]) != 0 {
				return fmt.Errorf("chain not empty %s", key)
			}
			delete(runner.chains, key)
		}
	default:
		return fmt.Errorf("unexpected iptables command %v", argv)
	}
	return nil
}

func (runner *routedTUNFakeRunner) empty() bool {
	return len(runner.chains) == 0 && len(runner.hooks) == 0 && len(runner.routes) == 0 && len(runner.rules) == 0
}

func TestOwnedRoutedTUNApplyCleanupInverseAndPartialFailure(t *testing.T) {
	in := routedTUNTestInput()
	in.ClientIPv4 = ""
	in.ClientIPv4s = []string{"192.168.31.42", "192.168.31.43"}
	in.ClientMACs["192.168.31.43"] = "02:11:22:33:44:43"
	plan := ownedTestPlan(t, in)
	firstHook := -1
	for i, argv := range plan.Apply {
		if argv[0] == "iptables" && argv[5] == "-I" {
			if firstHook == -1 {
				firstHook = i
			}
		} else if firstHook != -1 {
			t.Fatalf("resource preparation after externally reachable hook: %v", argv)
		}
	}
	if firstHook < 0 {
		t.Fatal("missing hooks")
	}
	for i, argv := range plan.Apply[firstHook:] {
		want := slices.Clone(argv)
		want[5] = "-D"
		want = append(want[:7], want[8:]...)
		if !slices.Equal(plan.Cleanup[len(plan.Apply)-firstHook-1-i], want) {
			t.Fatalf("hook cleanup not exact inverse: %v", argv)
		}
	}
	if plan.Cleanup[0][4] != "mangle" || plan.Cleanup[1][4] != "mangle" || plan.Cleanup[2][4] != "nat" || plan.Cleanup[3][4] != "nat" {
		t.Fatal("MARK entry then DNS hooks must withdraw first")
	}
	for stop := 0; stop <= len(plan.Apply); stop++ {
		runner := newRoutedTUNFakeRunner()
		for _, argv := range plan.Apply[:stop] {
			if err := runner.run(argv); err != nil {
				t.Fatalf("apply at prefix %d: %v", stop, err)
			}
		}
		for _, argv := range plan.Cleanup {
			err := runner.run(argv)
			if stop == len(plan.Apply) && err != nil {
				t.Fatalf("full apply cleanup is not inverse: %v", err)
			}
		}
		if !runner.empty() {
			t.Fatalf("best-effort cleanup left owned resources after apply prefix %d: %+v", stop, runner)
		}
	}
	if !reflect.DeepEqual(plan.OnFailure, plan.Cleanup) {
		t.Fatal("FailureDirect does not withdraw exact owned intent")
	}
	for i, argv := range plan.Cleanup {
		if len(argv) == 0 {
			t.Fatal("empty cleanup argv")
		}
		failureArg := plan.OnFailure[i][0]
		plan.Cleanup[i][0] = "mutated"
		if plan.OnFailure[i][0] != failureArg {
			t.Fatal("OnFailure argv shares storage with Cleanup")
		}
	}
}

func TestOwnedRoutedTUNNoLifecycleGlobalOrOutputCaptureCommands(t *testing.T) {
	plan := ownedTestPlan(t, routedTUNTestInput())
	for _, commands := range [][][]string{plan.Apply, plan.Cleanup, plan.OnFailure} {
		for _, argv := range commands {
			if argv[0] != "ip" && argv[0] != "iptables" {
				t.Fatalf("non-owned executable %v", argv)
			}
			if argv[0] == "ip" {
				if argv[1] != "-4" || argv[2] != "route" && argv[2] != "rule" || argv[3] != "add" && argv[3] != "del" {
					t.Fatalf("interface/main-table/sysctl lifecycle operation %v", argv)
				}
				if !slices.Contains(argv, "16500") || slices.Contains(argv, "main") || slices.Contains(argv, "local") || slices.Contains(argv, "lo") {
					t.Fatalf("unowned routing %v", argv)
				}
				continue
			}
			if !ownedHasArgs(argv, "-w", "5", "-t") {
				t.Fatalf("unbounded iptables operation %v", argv)
			}
			if argv[5] == "-I" || argv[5] == "-D" {
				switch argv[6] {
				case "OUTPUT":
					if argv[4] != "filter" || !ownedHasArgs(argv, "-o", "b6p-tun", "-s", "172.31.255.253/32", "-d", "172.31.255.254/32", "-p", "tcp", "-j", ownedTUNOutput) {
						t.Fatalf("OUTPUT capture or broad permission %v", argv)
					}
				case "INPUT":
					if !ownedHasArgs(argv, "-i", "b6p-tun", "-s", "172.31.255.254/32", "-d", "172.31.255.253/32", "-p", "tcp", "-j", ownedTUNInput) {
						t.Fatalf("INPUT permission widened %v", argv)
					}
				case "FORWARD", "PREROUTING":
				default:
					t.Fatalf("unrequested system chain hook %v", argv)
				}
			} else if !strings.HasPrefix(argv[6], "B6P_") {
				t.Fatalf("factory chain changed %v", argv)
			}
			for _, token := range argv {
				if token == "CONNMARK" || token == "MASQUERADE" || token == "SNAT" || token == "TPROXY" || token == "POSTROUTING" || token == "flush" || token == "--flush" {
					t.Fatalf("unrequested global/header/mark side effect %v", argv)
				}
			}
		}
	}
}

func TestOwnedRoutedTUNMaximumClientsCanonicalAndIndependent(t *testing.T) {
	in := routedTUNTestInput()
	in.ClientIPv4 = ""
	in.ClientMACs = map[string]string{}
	for i := 1; i <= MaxCaptureClientsPerFamily; i++ {
		source := fmt.Sprintf("192.0.2.%d", i)
		in.ClientIPv4s = append(in.ClientIPv4s, source)
		in.ClientMACs[source] = fmt.Sprintf("02:11:22:33:44:%02x", i)
	}
	in.ClientIPv6s = []string{"2001:db8::42"}
	originalClients, originalMACs := slices.Clone(in.ClientIPv4s), stdmaps.Clone(in.ClientMACs)
	first := ownedTestPlan(t, in)
	slices.Reverse(in.ClientIPv4s)
	second := ownedTestPlan(t, in)
	if !reflect.DeepEqual(first, second) || len(first.Ownership.Chains) != 6 || len(first.Ownership.ClientIPv4s) != MaxCaptureClientsPerFamily {
		t.Fatal("64 exact clients were broadened, truncated or nondeterministic")
	}
	hooks, rules, marks := 0, 0, 0
	for _, argv := range first.Apply {
		if argv[0] == "ip" && argv[2] == "rule" {
			rules++
		}
		if argv[0] == "iptables" && argv[5] == "-I" {
			hooks++
		}
		if ownedHasArgs(argv, "-j", "MARK") {
			marks++
		}
		if argv[0] == "ip6tables" || ownedHasArgs(argv, "ip", "-6") {
			t.Fatal("IPv6Direct installed IPv6 intent")
		}
	}
	if rules != MaxCaptureClientsPerFamily || hooks != 6*MaxCaptureClientsPerFamily+2 || marks != 2 {
		t.Fatalf("shared exact scope changed: rules=%d hooks=%d marks=%d", rules, hooks, marks)
	}
	slices.Reverse(in.ClientIPv4s)
	if !slices.Equal(in.ClientIPv4s, originalClients) || !stdmaps.Equal(in.ClientMACs, originalMACs) {
		t.Fatal("planner mutated caller-owned clients/MACs")
	}
	first.Ownership.ClientMACs[in.ClientIPv4s[0]] = "changed"
	first.Ownership.ClientIPv4s[0] = "changed"
	if !stdmaps.Equal(in.ClientMACs, originalMACs) || !stdmaps.Equal(second.Ownership.ClientMACs, originalMACs) || !slices.Equal(in.ClientIPv4s, originalClients) {
		t.Fatal("ownership aliases input/another plan")
	}
	in.ClientIPv4s = append(in.ClientIPv4s, "192.0.2.99")
	in.ClientMACs["192.0.2.99"] = "02:11:22:33:44:99"
	if _, err := PlanOwnedRules(in); err == nil {
		t.Fatal("routed-TUN exceeds existing 64-client limit")
	}
}

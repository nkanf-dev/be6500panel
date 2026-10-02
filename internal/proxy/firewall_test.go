package proxy

import (
	"fmt"
	"net/netip"
	"reflect"
	"slices"
	"strings"
	"testing"
)

func ownedTestInput() RulesPlanInput {
	return RulesPlanInput{
		ClientIPv4: "192.168.31.42", LANInterface: "br-lan",
		Ports: Ports{Mixed: 2080, TProxy: 2081, DNS: 2082},
		IPv6:  IPv6Direct, Failure: FailureDirect, FakeIP: true,
		EndpointIPs: []string{"203.0.113.20"}, ManagementIPs: []string{"192.168.31.1"},
	}
}

func ownedTestPlan(t *testing.T, in RulesPlanInput) OwnedRulesPlan {
	t.Helper()
	plan, err := PlanOwnedRules(in)
	if err != nil {
		t.Fatal(err)
	}
	return plan
}

func ownedHasArgs(command []string, args ...string) bool {
	for i := 0; i+len(args) <= len(command); i++ {
		if slices.Equal(command[i:i+len(args)], args) {
			return true
		}
	}
	return false
}

func ownedFindCommand(commands [][]string, args ...string) int {
	for i, command := range commands {
		if ownedHasArgs(command, args...) {
			return i
		}
	}
	return -1
}

func TestOwnedRulesMarkDoesNotOverlapFirmware(t *testing.T) {
	for name, mask := range map[string]uint32{"qos": 0xffff8000, "mwan": 0x3f00, "parent": 0x0f, "uu": 0xf0} {
		if CaptureMark&mask != 0 || CaptureMask&mask != 0 {
			t.Fatalf("capture mark/mask overlaps %s: %#x", name, mask)
		}
	}
	if CaptureMark != 0x4000 || CaptureMask != 0x4000 {
		t.Fatalf("unexpected capture mark/mask: %#x/%#x", CaptureMark, CaptureMask)
	}
	if uint32(0x40000000)&uint32(0xffff8000) == 0 {
		t.Fatal("regression: earlier proposed 0x40000000 is inside the QoS mask")
	}
	in := ownedTestInput()
	in.IPv6, in.ClientIPv6 = IPv6Follow, "2001:db8::42"
	plan := ownedTestPlan(t, in)
	if plan.Ownership.Mark != CaptureMark || plan.Ownership.Mask != CaptureMask || plan.Ownership.RouteTable != 16500 || plan.Ownership.RulePriority != 16500 {
		t.Fatalf("wrong ownership: %+v", plan.Ownership)
	}
	nTProxy, nPolicy := 0, 0
	for _, commands := range [][][]string{plan.Apply, plan.Cleanup, plan.OnFailure} {
		for _, command := range commands {
			for _, arg := range command {
				if arg == "MARK" || arg == "CONNMARK" || arg == "--set-mark" || arg == "--set-xmark" || arg == "--save-mark" || arg == "--restore-mark" {
					t.Fatalf("unrequested mark operation: %v", command)
				}
			}
			if ownedHasArgs(command, "-j", "TPROXY") {
				nTProxy++
				if !ownedHasArgs(command, "--tproxy-mark", "0x4000/0x4000") {
					t.Fatalf("TPROXY must preserve other mark bits: %v", command)
				}
			}
			if command[0] == "ip" && command[2] == "rule" {
				nPolicy++
				if !ownedHasArgs(command, "fwmark", "0x4000/0x4000", "lookup", "16500") {
					t.Fatalf("wrong policy rule: %v", command)
				}
			}
		}
	}
	if nTProxy != 8 || nPolicy != 6 {
		t.Fatalf("unexpected TPROXY/policy command counts: %d/%d", nTProxy, nPolicy)
	}
	// The kernel's masked TPROXY update must retain every outside bit.
	original := uint32(0xfabc123f)
	updated := (original &^ CaptureMask) | CaptureMark
	if updated&^CaptureMask != original&^CaptureMask {
		t.Fatal("masked mark update changed unrelated bits")
	}
}

func TestOwnedRulesValidateLiteralInputsAndRejectInjection(t *testing.T) {
	cases := []struct {
		name string
		edit func(*RulesPlanInput)
	}{
		{"missing-client", func(in *RulesPlanInput) { in.ClientIPv4 = "" }},
		{"client-prefix", func(in *RulesPlanInput) { in.ClientIPv4 = "192.168.31.0/24" }},
		{"client-hostname", func(in *RulesPlanInput) { in.ClientIPv4 = "client.example" }},
		{"client-list", func(in *RulesPlanInput) { in.ClientIPv4 = "192.168.31.42,192.168.31.43" }},
		{"client-command", func(in *RulesPlanInput) { in.ClientIPv4 = "192.168.31.42; reboot" }},
		{"client-spaces", func(in *RulesPlanInput) { in.ClientIPv4 = " 192.168.31.42" }},
		{"client-loopback", func(in *RulesPlanInput) { in.ClientIPv4 = "127.0.0.1" }},
		{"client-zero", func(in *RulesPlanInput) { in.ClientIPv4 = "0.0.0.0" }},
		{"client-multicast", func(in *RulesPlanInput) { in.ClientIPv4 = "224.0.0.1" }},
		{"client-broadcast", func(in *RulesPlanInput) { in.ClientIPv4 = "255.255.255.255" }},
		{"client-wrong-family", func(in *RulesPlanInput) { in.ClientIPv4 = "2001:db8::42" }},
		{"missing-interface", func(in *RulesPlanInput) { in.LANInterface = "" }},
		{"interface-long", func(in *RulesPlanInput) { in.LANInterface = strings.Repeat("a", 16) }},
		{"interface-wildcard", func(in *RulesPlanInput) { in.LANInterface = "br+" }},
		{"interface-command", func(in *RulesPlanInput) { in.LANInterface = "br;reboot" }},
		{"interface-space", func(in *RulesPlanInput) { in.LANInterface = "br lan" }},
		{"interface-newline", func(in *RulesPlanInput) { in.LANInterface = "br-lan\n" }},
		{"interface-option", func(in *RulesPlanInput) { in.LANInterface = "--help" }},
		{"missing-mixed-port", func(in *RulesPlanInput) { in.Ports.Mixed = 0 }},
		{"missing-tproxy-port", func(in *RulesPlanInput) { in.Ports.TProxy = 0 }},
		{"missing-dns-port", func(in *RulesPlanInput) { in.Ports.DNS = 0 }},
		{"dns-tproxy-collision", func(in *RulesPlanInput) { in.Ports.DNS = in.Ports.TProxy }},
		{"mixed-tproxy-collision", func(in *RulesPlanInput) { in.Ports.Mixed = in.Ports.TProxy }},
		{"mixed-dns-collision", func(in *RulesPlanInput) { in.Ports.Mixed = in.Ports.DNS }},
		{"unknown-ipv6-mode", func(in *RulesPlanInput) { in.IPv6 = "disable-everything" }},
		{"missing-ipv6-follow-address", func(in *RulesPlanInput) { in.IPv6 = IPv6Follow }},
		{"missing-ipv6-block-address", func(in *RulesPlanInput) { in.IPv6 = IPv6Block }},
		{"ipv6-prefix", func(in *RulesPlanInput) { in.ClientIPv6 = "2001:db8::/64" }},
		{"ipv6-zone", func(in *RulesPlanInput) { in.ClientIPv6 = "fe80::42%br-lan" }},
		{"ipv6-mapped-v4", func(in *RulesPlanInput) { in.ClientIPv6 = "::ffff:192.168.31.42" }},
		{"ipv6-wrong-family", func(in *RulesPlanInput) { in.ClientIPv6 = "192.168.31.42" }},
		{"ipv6-loopback", func(in *RulesPlanInput) { in.ClientIPv6 = "::1" }},
		{"ipv6-multicast", func(in *RulesPlanInput) { in.ClientIPv6 = "ff02::1" }},
		{"unknown-failure", func(in *RulesPlanInput) { in.Failure = "drop-all" }},
		{"endpoint-name", func(in *RulesPlanInput) { in.EndpointIPs = []string{"proxy.example"} }},
		{"endpoint-prefix", func(in *RulesPlanInput) { in.EndpointIPs = []string{"203.0.113.0/24"} }},
		{"endpoint-command", func(in *RulesPlanInput) { in.EndpointIPs = []string{"203.0.113.20; reboot"} }},
		{"endpoint-zone", func(in *RulesPlanInput) { in.EndpointIPs = []string{"fe80::20%br-lan"} }},
		{"management-option", func(in *RulesPlanInput) { in.ManagementIPs = []string{"--jump ACCEPT"} }},
		{"management-prefix", func(in *RulesPlanInput) { in.ManagementIPs = []string{"192.168.31.0/24"} }},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			in := ownedTestInput()
			tc.edit(&in)
			plan, err := PlanOwnedRules(in)
			if err == nil {
				t.Fatalf("accepted invalid input: %+v", in)
			}
			if !reflect.DeepEqual(plan, OwnedRulesPlan{}) {
				t.Fatalf("error returned an executable partial plan: %+v", plan)
			}
		})
	}
	for _, iface := range []string{"br-lan", "eth0.100", "lan_1", "br:1", strings.Repeat("a", 15)} {
		in := ownedTestInput()
		in.LANInterface = iface
		ownedTestPlan(t, in)
	}
}

func TestOwnedRulesDeterministicAndNoInputMutation(t *testing.T) {
	in := ownedTestInput()
	in.IPv6, in.ClientIPv6 = IPv6Follow, "2001:0db8:0000::0042"
	in.EndpointIPs = []string{"2001:0db8::0020", "203.0.113.20", "203.0.113.20"}
	in.ManagementIPs = []string{"2001:0db8::0001", "192.168.31.1", "203.0.113.20"}
	originalEndpoints := slices.Clone(in.EndpointIPs)
	originalManagement := slices.Clone(in.ManagementIPs)
	first := ownedTestPlan(t, in)
	second := ownedTestPlan(t, in)
	if !reflect.DeepEqual(first, second) {
		t.Fatal("same input produced different argv")
	}
	slices.Reverse(in.EndpointIPs)
	slices.Reverse(in.ManagementIPs)
	third := ownedTestPlan(t, in)
	if !reflect.DeepEqual(first, third) {
		t.Fatal("IP list order changed the plan")
	}
	slices.Reverse(in.EndpointIPs)
	slices.Reverse(in.ManagementIPs)
	if !slices.Equal(in.EndpointIPs, originalEndpoints) || !slices.Equal(in.ManagementIPs, originalManagement) {
		t.Fatal("planner changed caller-owned input slices")
	}
	if first.Ownership.ClientIPv6 != "2001:db8::42" {
		t.Fatalf("IPv6 scope not canonical: %+v", first.Ownership)
	}
	if !reflect.DeepEqual(first.OnFailure, first.Cleanup) {
		t.Fatal("fail-direct does not withdraw all owned resources")
	}
	failureOriginal := ownedCloneCommands(first.OnFailure)
	first.Cleanup[0][0] = "changed"
	if !reflect.DeepEqual(first.OnFailure, failureOriginal) {
		t.Fatal("OnFailure shares command slices with Cleanup")
	}
	first.OnFailure[0][1] = "changed"
	if !reflect.DeepEqual(second, third) {
		t.Fatal("plans share mutable slices across calls")
	}
}

func TestOwnedRulesHooksAndPolicyAreExactSingleClientScope(t *testing.T) {
	in := ownedTestInput()
	in.IPv6, in.ClientIPv6 = IPv6Follow, "2001:db8::42"
	plan := ownedTestPlan(t, in)
	nHooks := 0
	for _, command := range plan.Apply {
		if command[0] == "iptables" || command[0] == "ip6tables" {
			if !ownedHasArgs(command, "-w", "5", "-t") {
				t.Fatalf("missing fixed wait/table argv: %v", command)
			}
			if command[5] != "-N" && command[5] != "-A" && command[5] != "-I" {
				t.Fatalf("unexpected apply operation: %v", command)
			}
			if command[5] == "-I" {
				nHooks++
				prefix := "192.168.31.42/32"
				if command[0] == "ip6tables" {
					prefix = "2001:db8::42/128"
				}
				if !ownedHasArgs(command, "-I", "PREROUTING", "1", "-i", "br-lan", "-s", prefix) {
					t.Fatalf("hook widened client scope: %v", command)
				}
				if !strings.HasPrefix(command[len(command)-1], "B6P_") {
					t.Fatalf("hook does not jump to owned chain: %v", command)
				}
			} else if !strings.HasPrefix(command[6], "B6P_") {
				t.Fatalf("unowned chain changed: %v", command)
			}
		}
		if command[0] == "ip" && command[2] == "rule" {
			prefix := "192.168.31.42/32"
			if command[1] == "-6" {
				prefix = "2001:db8::42/128"
			}
			if !ownedHasArgs(command, "from", prefix, "iif", "br-lan", "fwmark", "0x4000/0x4000") {
				t.Fatalf("policy rule widened client scope: %v", command)
			}
		}
		if slices.Contains(command, "OUTPUT") || slices.Contains(command, "POSTROUTING") {
			t.Fatalf("unexpected output capture: %v", command)
		}
	}
	if nHooks != 4 {
		t.Fatalf("unexpected hooks: %d", nHooks)
	}
	for _, chain := range plan.Ownership.Chains {
		if !strings.HasPrefix(chain.Name, "B6P_") || chain.Hook != "PREROUTING" {
			t.Fatalf("unexpected chain metadata: %+v", chain)
		}
	}
}

func TestOwnedRulesPrepareAllFamiliesBeforeCaptureAndUnhookFirst(t *testing.T) {
	in := ownedTestInput()
	in.IPv6, in.ClientIPv6 = IPv6Follow, "2001:db8::42"
	plan := ownedTestPlan(t, in)
	firstHook := -1
	for i, command := range plan.Apply {
		if ownedHasArgs(command, "-I", "PREROUTING") {
			if firstHook == -1 {
				firstHook = i
			}
		} else if firstHook != -1 {
			t.Fatalf("preparation after a live hook: %v", command)
		}
	}
	if firstHook != len(plan.Apply)-4 {
		t.Fatalf("capture hooks were not the final four operations: %d/%d", firstHook, len(plan.Apply))
	}
	for _, family := range []struct{ flag, client, prefix string }{
		{"-4", "192.168.31.42/32", "0.0.0.0/0"}, {"-6", "2001:db8::42/128", "::/0"},
	} {
		route := []string{"ip", family.flag, "route", "add", "local", family.prefix, "dev", "lo", "table", "16500"}
		rule := []string{"ip", family.flag, "rule", "add", "priority", "16500", "from", family.client, "iif", "br-lan", "fwmark", "0x4000/0x4000", "lookup", "16500"}
		ri, pi := ownedFindCommand(plan.Apply, route...), ownedFindCommand(plan.Apply, rule...)
		if ri < 0 || pi <= ri || pi >= firstHook {
			t.Fatalf("local routing not ready before hooks: route=%d rule=%d hook=%d", ri, pi, firstHook)
		}
	}
	for i := 0; i < 4; i++ {
		apply := plan.Apply[len(plan.Apply)-1-i]
		cleanup := plan.Cleanup[i]
		expected := slices.Clone(apply)
		expected[5] = "-D"
		expected = append(expected[:7], expected[8:]...)
		if !slices.Equal(cleanup, expected) {
			t.Fatalf("cleanup does not first remove exact hook: got %v want %v", cleanup, expected)
		}
	}
	if plan.Apply[firstHook][4] != "nat" || plan.Apply[firstHook+1][4] != "mangle" || plan.Apply[firstHook+2][4] != "nat" || plan.Apply[firstHook+3][4] != "mangle" {
		t.Fatal("DNS hooks must be installed before capture hooks")
	}
}

func TestOwnedRulesDNSAndBypassOrdering(t *testing.T) {
	in := ownedTestInput()
	in.IPv6, in.ClientIPv6 = IPv6Follow, "2001:db8::42"
	in.ManagementIPs = append(in.ManagementIPs, "2001:db8::1")
	in.EndpointIPs = append(in.EndpointIPs, "2001:db8::20")
	plan := ownedTestPlan(t, in)
	for _, family := range []struct {
		tool, suffix, mgmt, endpoint, private, fake, onIP string
	}{
		{"iptables", "4", "192.168.31.1/32", "203.0.113.20/32", "192.168.0.0/16", "198.18.0.0/15", "127.0.0.1"},
		{"ip6tables", "6", "2001:db8::1/128", "2001:db8::20/128", "fc00::/7", "fc00::/18", "::1"},
	} {
		t.Run(family.tool, func(t *testing.T) {
			capture := "B6P_V" + family.suffix + "_CAPTURE"
			dns := "B6P_V" + family.suffix + "_DNS"
			for _, chain := range []string{capture, dns} {
				local := ownedFindCommand(plan.Apply, "-A", chain, "-m", "addrtype", "--dst-type", "LOCAL", "-j", "RETURN")
				mgmt := ownedFindCommand(plan.Apply, "-A", chain, "-d", family.mgmt, "-j", "RETURN")
				endpoint := ownedFindCommand(plan.Apply, "-A", chain, "-d", family.endpoint, "-j", "RETURN")
				link := "169.254.0.0/16"
				if family.suffix == "6" {
					link = "fe80::/10"
				}
				linkIndex := ownedFindCommand(plan.Apply, "-A", chain, "-d", link, "-j", "RETURN")
				private := ownedFindCommand(plan.Apply, "-A", chain, "-d", family.private, "-j", "RETURN")
				for _, protocol := range []string{"tcp", "udp"} {
					var dnsIndex int
					if chain == capture {
						dnsIndex = ownedFindCommand(plan.Apply, "-A", chain, "-p", protocol, "--dport", "53", "-j", "RETURN")
					} else {
						dnsIndex = ownedFindCommand(plan.Apply, "-A", chain, "-p", protocol, "--dport", "53", "-j", "REDIRECT", "--to-ports", "2082")
					}
					if local < 0 || mgmt <= local || endpoint < local || linkIndex < 0 || dnsIndex <= mgmt || dnsIndex <= endpoint || dnsIndex <= linkIndex || private <= dnsIndex {
						t.Fatalf("DNS/bypass order invalid in %s/%s: local=%d mgmt=%d endpoint=%d link=%d dns=%d private=%d", chain, protocol, local, mgmt, endpoint, linkIndex, dnsIndex, private)
					}
					if chain == capture {
						fake := ownedFindCommand(plan.Apply, "-A", chain, "-p", protocol, "-d", family.fake, "-j", "TPROXY")
						general := ownedFindCommand(plan.Apply, "-A", chain, "-p", protocol, "-j", "TPROXY")
						if fake <= dnsIndex || fake >= private || general <= private {
							t.Fatalf("fake/private/general capture order invalid: dns=%d fake=%d private=%d general=%d", dnsIndex, fake, private, general)
						}
						for _, index := range []int{fake, general} {
							if !ownedHasArgs(plan.Apply[index], "--on-ip", family.onIP, "--on-port", "2081", "--tproxy-mark", "0x4000/0x4000") {
								t.Fatalf("wrong transparent listener: %v", plan.Apply[index])
							}
						}
					}
				}
			}
		})
	}
	for _, command := range plan.Apply {
		if ownedHasArgs(command, "-j", "TPROXY") && slices.Contains(command, "2082") {
			t.Fatalf("direct DNS listener must never receive TPROXY: %v", command)
		}
	}
}

// This small packet walker checks actual first-match behavior of the generated
// chains, in addition to checking argv shape. It does not emulate a kernel or
// claim that hardware offload, routing or native listeners work on the router.
func ownedTestVerdict(t *testing.T, commands [][]string, chain, destination, protocol string, port int, local bool) string {
	t.Helper()
	dst := netip.MustParseAddr(destination)
	for _, command := range commands {
		if len(command) < 7 || command[5] != "-A" || command[6] != chain {
			continue
		}
		match, target := true, ""
		for i := 7; i < len(command); i++ {
			switch command[i] {
			case "-d":
				i++
				match = match && netip.MustParsePrefix(command[i]).Contains(dst)
			case "-p":
				i++
				match = match && command[i] == protocol
			case "--dport":
				i++
				match = match && command[i] == fmt.Sprint(port)
			case "--sport":
				i++
				match = false // synthetic client source port is never SSH
			case "--dst-type":
				i++
				match = match && command[i] == "LOCAL" && local
			case "-j":
				i++
				target = command[i]
			}
		}
		if match {
			return target
		}
	}
	return "RETURN"
}

func TestOwnedRulesFirstMatchPacketIntent(t *testing.T) {
	in := ownedTestInput()
	in.IPv6, in.ClientIPv6 = IPv6Follow, "2001:db8::42"
	in.ManagementIPs = append(in.ManagementIPs, "2001:db8::1")
	in.EndpointIPs = append(in.EndpointIPs, "2001:db8::20")
	plan := ownedTestPlan(t, in)
	cases := []struct {
		chain, destination, protocol string
		port                         int
		local                        bool
		want                         string
	}{
		{"B6P_V4_CAPTURE", "203.0.113.20", "tcp", 443, false, "RETURN"},
		{"B6P_V4_CAPTURE", "192.168.31.1", "tcp", 443, false, "RETURN"},
		{"B6P_V4_CAPTURE", "10.0.0.2", "tcp", 443, false, "RETURN"},
		{"B6P_V4_CAPTURE", "169.254.1.1", "udp", 443, false, "RETURN"},
		{"B6P_V4_CAPTURE", "8.8.8.8", "tcp", 22, false, "TPROXY"},
		{"B6P_V4_CAPTURE", "192.168.31.1", "tcp", 22, false, "RETURN"},
		{"B6P_V4_CAPTURE", "192.0.0.2", "udp", 443, false, "RETURN"},
		{"B6P_V4_CAPTURE", "8.8.8.8", "udp", 443, false, "TPROXY"},
		{"B6P_V4_CAPTURE", "198.18.1.1", "tcp", 443, false, "TPROXY"},
		{"B6P_V4_CAPTURE", "8.8.8.8", "udp", 53, false, "RETURN"},
		{"B6P_V4_DNS", "8.8.8.8", "udp", 53, false, "REDIRECT"},
		{"B6P_V4_DNS", "10.0.0.2", "tcp", 53, false, "REDIRECT"},
		{"B6P_V4_DNS", "192.168.31.1", "udp", 53, false, "RETURN"},
		{"B6P_V4_DNS", "192.168.31.9", "udp", 53, true, "RETURN"},
		{"B6P_V4_DNS", "203.0.113.20", "tcp", 53, false, "RETURN"},
		{"B6P_V4_DNS", "224.0.0.1", "udp", 53, false, "RETURN"},
		{"B6P_V4_DNS", "8.8.8.8", "udp", 443, false, "RETURN"},
		{"B6P_V6_CAPTURE", "2001:db8::20", "tcp", 443, false, "RETURN"},
		{"B6P_V6_CAPTURE", "2001:db8::1", "tcp", 443, false, "RETURN"},
		{"B6P_V6_CAPTURE", "fd12::2", "tcp", 443, false, "RETURN"},
		{"B6P_V6_CAPTURE", "fe80::2", "udp", 443, false, "RETURN"},
		{"B6P_V6_CAPTURE", "ff02::1", "udp", 443, false, "RETURN"},
		{"B6P_V6_CAPTURE", "fc00::2", "tcp", 443, false, "TPROXY"},
		{"B6P_V6_CAPTURE", "2001:4860:4860::8888", "udp", 443, false, "TPROXY"},
		{"B6P_V6_CAPTURE", "2001:4860:4860::8888", "udp", 53, false, "RETURN"},
		{"B6P_V6_DNS", "2001:4860:4860::8888", "udp", 53, false, "REDIRECT"},
		{"B6P_V6_DNS", "fd12::2", "tcp", 53, false, "REDIRECT"},
		{"B6P_V6_DNS", "fe80::2", "udp", 53, false, "RETURN"},
		{"B6P_V6_DNS", "2001:db8::1", "udp", 53, false, "RETURN"},
	}
	for _, tc := range cases {
		name := fmt.Sprintf("%s-%s-%s-%d", tc.chain, tc.destination, tc.protocol, tc.port)
		t.Run(name, func(t *testing.T) {
			if got := ownedTestVerdict(t, plan.Apply, tc.chain, tc.destination, tc.protocol, tc.port, tc.local); got != tc.want {
				t.Fatalf("got %s, want %s", got, tc.want)
			}
		})
	}
}

func TestOwnedRulesIPv6DirectAndBlock(t *testing.T) {
	in := ownedTestInput()
	in.ClientIPv6 = "2001:db8::42"
	direct := ownedTestPlan(t, in)
	for _, commands := range [][][]string{direct.Apply, direct.Cleanup, direct.OnFailure} {
		for _, command := range commands {
			if command[0] == "ip6tables" || (command[0] == "ip" && command[1] == "-6") {
				t.Fatalf("IPv6 direct changes IPv6 state: %v", command)
			}
		}
	}
	if !slices.Equal(direct.Ownership.RouteFamilies, []int{4}) {
		t.Fatal("direct claims IPv6 route ownership")
	}
	in.IPv6 = IPv6Block
	in.ManagementIPs = append(in.ManagementIPs, "2001:db8::1")
	in.EndpointIPs = append(in.EndpointIPs, "2001:db8::20")
	block := ownedTestPlan(t, in)
	if !slices.Equal(block.Ownership.RouteFamilies, []int{4}) {
		t.Fatal("block should not install IPv6 TPROXY routing")
	}
	if ownedFindCommand(block.Apply, "ip6tables", "-w", "5", "-t", "filter", "-I", "FORWARD", "1", "-i", "br-lan", "-s", "2001:db8::42/128", "-j", "B6P_V6_BLOCK") < 0 {
		t.Fatal("IPv6 block is not a scoped FORWARD hook")
	}
	fake := ownedFindCommand(block.Apply, "-A", "B6P_V6_BLOCK", "-d", "fc00::/18", "-j", "REJECT", "--reject-with", "icmp6-adm-prohibited")
	private := ownedFindCommand(block.Apply, "-A", "B6P_V6_BLOCK", "-d", "fc00::/7", "-j", "RETURN")
	if fake < 0 || private <= fake {
		t.Fatal("fake-IP identities incorrectly bypass IPv6 block")
	}
	for _, tc := range []struct {
		destination, protocol string
		port                  int
		want                  string
	}{
		{"2001:db8::1", "tcp", 443, "RETURN"},
		{"2001:db8::20", "tcp", 443, "RETURN"},
		{"2001:4860:4860::8888", "tcp", 22, "REJECT"},
		{"fd12::2", "tcp", 443, "RETURN"},
		{"fe80::2", "udp", 53, "RETURN"},
		{"fc00::2", "tcp", 443, "REJECT"},
		{"2001:4860:4860::8888", "udp", 53, "REJECT"},
		{"2001:4860:4860::8888", "udp", 443, "REJECT"},
		{"2001:4860:4860::8888", "icmp", 0, "REJECT"},
	} {
		if got := ownedTestVerdict(t, block.Apply, "B6P_V6_BLOCK", tc.destination, tc.protocol, tc.port, false); got != tc.want {
			t.Errorf("IPv6 block %s/%s/%d: got %s, want %s", tc.destination, tc.protocol, tc.port, got, tc.want)
		}
	}
	for _, command := range block.Apply {
		if command[0] == "ip6tables" && command[4] != "filter" {
			t.Fatalf("IPv6 block installs IPv6 capture/DNS redirect: %v", command)
		}
		if command[0] == "ip" && command[1] == "-6" {
			t.Fatalf("IPv6 block installs IPv6 policy routing: %v", command)
		}
	}
}

func TestOwnedRulesRejectSelectiveFailureWithoutClassifier(t *testing.T) {
	in := ownedTestInput()
	in.Failure = FailureBlockProxy
	plan, err := PlanOwnedRules(in)
	if err == nil || !strings.Contains(err.Error(), "stateful") || !strings.Contains(err.Error(), "not selective") {
		t.Fatalf("block-proxy must explain missing classifier, not drop everything: %v", err)
	}
	if !reflect.DeepEqual(plan, OwnedRulesPlan{}) {
		t.Fatal("unsupported failure policy returned capture or drop intent")
	}
}

func TestOwnedRulesCleanupOnlyOwnedResourcesAndBothFamilies(t *testing.T) {
	in := ownedTestInput()
	in.IPv6, in.ClientIPv6 = IPv6Follow, "2001:db8::42"
	plan := ownedTestPlan(t, in)
	nUnhook, nFlush, nDeleteChain, nRule, nRoute := 0, 0, 0, 0, 0
	for _, command := range plan.Cleanup {
		if command[0] == "iptables" || command[0] == "ip6tables" {
			if !ownedHasArgs(command, "-w", "5", "-t") {
				t.Fatalf("missing bounded wait: %v", command)
			}
			switch command[5] {
			case "-D":
				nUnhook++
				if command[6] != "PREROUTING" || !strings.HasPrefix(command[len(command)-1], "B6P_") || !ownedHasArgs(command, "-i", "br-lan", "-s") {
					t.Fatalf("cleanup widened hook deletion: %v", command)
				}
			case "-F", "-X":
				if len(command) != 7 || !strings.HasPrefix(command[6], "B6P_") {
					t.Fatalf("cleanup flushes an unowned/system chain: %v", command)
				}
				if command[5] == "-F" {
					nFlush++
				} else {
					nDeleteChain++
				}
			default:
				t.Fatalf("unexpected cleanup operation: %v", command)
			}
		} else if command[0] == "ip" {
			if command[3] != "del" {
				t.Fatalf("cleanup must delete exact routes/rules, never flush: %v", command)
			}
			switch command[2] {
			case "rule":
				nRule++
				if !ownedHasArgs(command, "priority", "16500", "from") || !ownedHasArgs(command, "fwmark", "0x4000/0x4000", "lookup", "16500") {
					t.Fatalf("cleanup policy rule is not exact: %v", command)
				}
			case "route":
				nRoute++
				prefix := "0.0.0.0/0"
				if command[1] == "-6" {
					prefix = "::/0"
				}
				if !slices.Equal(command, []string{"ip", command[1], "route", "del", "local", prefix, "dev", "lo", "table", "16500"}) {
					t.Fatalf("cleanup route is not exact: %v", command)
				}
			default:
				t.Fatalf("unexpected ip cleanup command: %v", command)
			}
		} else {
			t.Fatalf("unexpected executable: %v", command)
		}
		for _, arg := range command {
			if arg == "flush" || arg == "--flush" || strings.Contains(arg, "sysctl") || strings.Contains(arg, "ecm") || strings.Contains(arg, "/sys/") {
				t.Fatalf("global cleanup side effect: %v", command)
			}
		}
	}
	if nUnhook != 4 || nFlush != 4 || nDeleteChain != 4 || nRule != 2 || nRoute != 2 {
		t.Fatalf("incomplete cleanup: hooks=%d F=%d X=%d rules=%d routes=%d", nUnhook, nFlush, nDeleteChain, nRule, nRoute)
	}
	if !reflect.DeepEqual(plan.OnFailure, plan.Cleanup) {
		t.Fatal("direct failure does not use full cleanup")
	}
}

func TestOwnedRulesWarningsStateUnverifiedRouterRequirements(t *testing.T) {
	in := ownedTestInput()
	in.IPv6, in.ClientIPv6 = IPv6Follow, "2001:db8::42"
	plan := ownedTestPlan(t, in)
	warnings := strings.Join(plan.Warnings, "\n")
	for _, requirement := range []string{"unused mark", "No idempotence", "Single-client", "return routing", "not loopback only", "dnsmasq", "Encrypted DNS", "ECM/PPE/SFE", "real TCP, UDP, DNS, QUIC", "dual-stack", "fc00::/18", "conntrack bindings", "cached fake-IP"} {
		if !strings.Contains(warnings, requirement) {
			t.Errorf("missing warning about %q", requirement)
		}
	}
	for _, command := range plan.Apply {
		if command[0] != "iptables" && command[0] != "ip6tables" && command[0] != "ip" {
			t.Fatalf("planner includes non-rule side effect: %v", command)
		}
	}
}

func TestOwnedRulesDefaultsMatchCompilerAndBoundInput(t *testing.T) {
	in := RulesPlanInput{ClientIPv4: "192.168.31.42", LANInterface: "br-lan"}
	plan := ownedTestPlan(t, in)
	if ownedFindCommand(plan.Apply, "--on-port", "7893") < 0 || ownedFindCommand(plan.Apply, "--to-ports", "1053") < 0 {
		t.Fatal("default firewall listeners do not match compiler defaults")
	}
	if !reflect.DeepEqual(plan.Cleanup, plan.OnFailure) || !slices.Equal(plan.Ownership.RouteFamilies, []int{4}) {
		t.Fatal("empty policies must default to direct")
	}
	for _, tc := range []struct {
		endpoints, management int
		valid                 bool
	}{
		{256, 128, true}, {257, 0, false}, {0, 129, false},
	} {
		bounded := in
		bounded.EndpointIPs = make([]string, tc.endpoints)
		bounded.ManagementIPs = make([]string, tc.management)
		for i := range bounded.EndpointIPs {
			bounded.EndpointIPs[i] = "203.0.113.20"
		}
		for i := range bounded.ManagementIPs {
			bounded.ManagementIPs[i] = "192.168.31.1"
		}
		result, err := PlanOwnedRules(bounded)
		if tc.valid != (err == nil) {
			t.Fatalf("limits %d/%d valid=%v: %v", tc.endpoints, tc.management, tc.valid, err)
		}
		if err != nil && !reflect.DeepEqual(result, OwnedRulesPlan{}) {
			t.Fatal("limit error returned a partial plan")
		}
	}
}

func TestOwnedRulesFakeIPOptInPreservesRealPrivateAddresses(t *testing.T) {
	for _, mode := range []IPv6Mode{IPv6Follow, IPv6Block} {
		for _, fake := range []bool{false, true} {
			t.Run(fmt.Sprintf("%s/fake=%v", mode, fake), func(t *testing.T) {
				in := ownedTestInput()
				in.IPv6, in.ClientIPv6, in.FakeIP = mode, "2001:db8::42", fake
				plan := ownedTestPlan(t, in)
				v4Want, v6Want, v6Chain := "RETURN", "RETURN", "B6P_V6_CAPTURE"
				if mode == IPv6Block {
					v6Chain = "B6P_V6_BLOCK"
				}
				if fake {
					v4Want, v6Want = "TPROXY", "TPROXY"
					if mode == IPv6Block {
						v6Want = "REJECT"
					}
				}
				if got := ownedTestVerdict(t, plan.Apply, "B6P_V4_CAPTURE", "198.18.1.1", "tcp", 443, false); got != v4Want {
					t.Fatalf("IPv4 benchmark/fake identity got %s want %s", got, v4Want)
				}
				if got := ownedTestVerdict(t, plan.Apply, v6Chain, "fc00::2", "udp", 443, false); got != v6Want {
					t.Fatalf("IPv6 ULA/fake identity got %s want %s", got, v6Want)
				}
				if got := ownedTestVerdict(t, plan.Apply, v6Chain, "fd12::2", "tcp", 443, false); got != "RETURN" {
					t.Fatal("ordinary ULA must always bypass")
				}
				for _, command := range plan.Apply {
					if ownedHasArgs(command, "--dport", "22") || ownedHasArgs(command, "--sport", "22") {
						t.Fatalf("global SSH exemption changes selected foreign policy: %v", command)
					}
					if !fake && ownedHasArgs(command, "-d", "fc00::/18") {
						t.Fatalf("fake-IP exception installed without opt-in: %v", command)
					}
				}
			})
		}
	}
}

func TestOwnedRulesRouterDNSOptInRedirectsBeforeLocalReturn(t *testing.T) {
	for _, follow := range []bool{false, true} {
		t.Run(fmt.Sprintf("follow=%v", follow), func(t *testing.T) {
			input := ownedTestInput()
			input.Ports = Ports{Mixed: 2080, TProxy: 7893, DNS: 1053}
			input.FakeIP = false
			input.RouterDNSAddresses = []string{"192.168.31.1", "192.168.31.1"}
			if follow {
				input.IPv6 = IPv6Follow
				input.ClientIPv6 = "2001:db8::42"
				input.ManagementIPs = append(input.ManagementIPs, "fd12::1")
				input.RouterDNSAddresses = append(input.RouterDNSAddresses, "fd12:0:0:0:0:0:0:1")
			}
			plan := ownedTestPlan(t, input)
			for _, family := range []int{4, 6} {
				tool, address, chain := "iptables", "192.168.31.1/32", "B6P_V4_DNS"
				if family == 6 {
					if !follow {
						continue
					}
					tool, address, chain = "ip6tables", "fd12::1/128", "B6P_V6_DNS"
				}
				local := ownedFindCommand(plan.Apply, tool, "-w", "5", "-t", "nat", "-A", chain, "-m", "addrtype", "--dst-type", "LOCAL", "-j", "RETURN")
				for _, protocol := range []string{"tcp", "udp"} {
					prefix := []string{tool, "-w", "5", "-t", "nat", "-A", chain, "-d", address, "-p", protocol, "--dport", "53", "-j", "REDIRECT", "--to-ports", "1053"}
					redirect := ownedFindCommand(plan.Apply, prefix...)
					if redirect < 0 || local <= redirect {
						t.Fatalf("explicit router DNS redirect does not precede LOCAL bypass: family=%d protocol=%s", family, protocol)
					}
					count := 0
					for _, command := range plan.Apply {
						if slices.Equal(command, prefix) {
							count++
						}
					}
					if count != 1 {
						t.Fatal("duplicate RouterDNSAddresses generated duplicate redirect rules")
					}
				}
			}
			for _, tc := range []struct {
				chain, address, protocol string
				port                     int
				local                    bool
				want                     string
			}{
				{"B6P_V4_DNS", "192.168.31.1", "udp", 53, true, "REDIRECT"},
				{"B6P_V4_DNS", "192.168.31.1", "tcp", 53, true, "REDIRECT"},
				{"B6P_V4_DNS", "192.168.31.1", "tcp", 22, true, "RETURN"},
				{"B6P_V4_DNS", "192.168.31.1", "tcp", 8787, true, "RETURN"},
				{"B6P_V4_DNS", "192.168.31.9", "udp", 53, true, "RETURN"},
				{"B6P_V4_CAPTURE", "192.168.31.1", "udp", 53, true, "RETURN"},
				{"B6P_V4_CAPTURE", "192.168.31.1", "tcp", 22, true, "RETURN"},
			} {
				if got := ownedTestVerdict(t, plan.Apply, tc.chain, tc.address, tc.protocol, tc.port, tc.local); got != tc.want {
					t.Fatalf("router DNS/control %s:%d got %s want %s", tc.address, tc.port, got, tc.want)
				}
			}
			if follow {
				if got := ownedTestVerdict(t, plan.Apply, "B6P_V6_DNS", "fd12::1", "udp", 53, true); got != "REDIRECT" {
					t.Fatal("IPv6 router DNS missing opt-in")
				}
				if got := ownedTestVerdict(t, plan.Apply, "B6P_V6_DNS", "fd12::2", "udp", 53, true); got != "RETURN" {
					t.Fatal("other IPv6 router address was redirected")
				}
				if got := ownedTestVerdict(t, plan.Apply, "B6P_V6_CAPTURE", "fc00::2", "tcp", 443, false); got != "RETURN" {
					t.Fatal("router DNS opt-in changed fake-off ULA bypass")
				}
			}
			// There is no rule widening DNS to another client. Every NAT PREROUTING
			// hook still has the exact selected client's source and LAN interface.
			for _, command := range plan.Apply {
				if command[0] == "iptables" || command[0] == "ip6tables" {
					if command[5] == "-I" {
						source := "192.168.31.42/32"
						if command[0] == "ip6tables" {
							source = "2001:db8::42/128"
						}
						if !ownedHasArgs(command, "-i", "br-lan", "-s", source) {
							t.Fatalf("router DNS hook widened selected client: %v", command)
						}
					}
				}
			}
			without := input
			without.RouterDNSAddresses = nil
			original := ownedTestPlan(t, without)
			if !reflect.DeepEqual(original.Cleanup, plan.Cleanup) || !reflect.DeepEqual(original.Ownership, plan.Ownership) {
				t.Fatal("router DNS opt-in changed owned cleanup or scope")
			}
			if got := ownedTestVerdict(t, original.Apply, "B6P_V4_DNS", "192.168.31.1", "udp", 53, true); got != "RETURN" {
				t.Fatal("router DNS redirected without opt-in")
			}
		})
	}
}

func TestOwnedRulesRouterDNSRequiresExplicitRouterManagementSubset(t *testing.T) {
	for _, addresses := range [][]string{
		{"example.com"}, {"192.168.31.1/32"}, {"192.168.31.9"}, {"0.0.0.0"}, {"127.0.0.1"},
		{"224.0.0.1"}, {"255.255.255.255"}, {"fe80::1"}, {"fe80::1%br-lan"}, {"::ffff:192.168.31.1"},
	} {
		input := ownedTestInput()
		input.RouterDNSAddresses = addresses
		// The unsafe literal addresses must be refused even if also listed as
		// management. Literal parsing still rejects hostnames/prefix/zone/mapped.
		if addresses[0] != "192.168.31.9" {
			input.ManagementIPs = append(input.ManagementIPs, addresses[0])
		}
		plan, err := PlanOwnedRules(input)
		if err == nil || !reflect.DeepEqual(plan, OwnedRulesPlan{}) {
			t.Fatalf("unsafe router DNS input accepted: %v", addresses)
		}
	}
	input := ownedTestInput()
	input.RouterDNSAddresses = make([]string, 17)
	for i := range input.RouterDNSAddresses {
		input.RouterDNSAddresses[i] = "192.168.31.1"
	}
	if _, err := PlanOwnedRules(input); err == nil {
		t.Fatal("router DNS address limit not enforced")
	}
	// IPv6 direct and block do not install a NAT IPv6 redirect even if the
	// future owner enumerates router IPv6 addresses ahead of a later follow.
	for _, mode := range []IPv6Mode{IPv6Direct, IPv6Block} {
		input = ownedTestInput()
		input.IPv6 = mode
		input.ClientIPv6 = "2001:db8::42"
		input.ManagementIPs = append(input.ManagementIPs, "fd12::1")
		input.RouterDNSAddresses = []string{"fd12::1"}
		plan := ownedTestPlan(t, input)
		for _, command := range plan.Apply {
			if command[0] == "ip6tables" && command[4] == "nat" {
				t.Fatal("non-follow mode redirected router IPv6 DNS")
			}
		}
	}
}

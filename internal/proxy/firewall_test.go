package proxy

import (
	"encoding/json"
	"fmt"
	stdmaps "maps"
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

func TestOwnedRulesMultipleClientsSharePreparationAndKeepExactScope(t *testing.T) {
	for _, mode := range []IPv6Mode{IPv6Direct, IPv6Follow, IPv6Block} {
		t.Run(string(mode), func(t *testing.T) {
			in := ownedTestInput()
			in.ClientIPv4 = ""
			in.ClientIPv4s = []string{"192.168.31.43", "192.168.31.42", "192.168.31.43"}
			in.ClientIPv6s = []string{"2001:db8::43", "2001:0db8:0000::0042", "2001:db8::42"}
			in.IPv6 = mode
			plan := ownedTestPlan(t, in)
			if plan.Ownership.ClientIPv4 != "" || plan.Ownership.ClientIPv6 != "" ||
				!slices.Equal(plan.Ownership.ClientIPv4s, []string{"192.168.31.42", "192.168.31.43"}) ||
				!slices.Equal(plan.Ownership.ClientIPv6s, []string{"2001:db8::42", "2001:db8::43"}) {
				t.Fatalf("noncanonical plural ownership: %+v", plan.Ownership)
			}
			wantChains, wantRoutes, wantRules, wantHooks := 2, 1, 2, 4
			if mode == IPv6Follow {
				wantChains, wantRoutes, wantRules, wantHooks = 4, 2, 4, 8
			} else if mode == IPv6Block {
				wantChains, wantHooks = 3, 6
			}
			if len(plan.Ownership.Chains) != wantChains || len(plan.Ownership.RouteFamilies) != wantRoutes {
				t.Fatalf("shared ownership: %+v", plan.Ownership)
			}
			countCommands := func(commands [][]string, operation string) (chains, routes, rules, hooks int) {
				for _, command := range commands {
					if command[0] == "ip" {
						if command[3] != operation {
							t.Fatalf("unexpected routing operation: %v", command)
						}
						if command[2] == "route" {
							routes++
						} else if command[2] == "rule" {
							rules++
							prefixes := []string{"192.168.31.42/32", "192.168.31.43/32"}
							if command[1] == "-6" {
								prefixes = []string{"2001:db8::42/128", "2001:db8::43/128"}
							}
							if !slices.Contains(prefixes, command[7]) || !ownedHasArgs(command, "iif", "br-lan", "fwmark", "0x4000/0x4000", "lookup", "16500") {
								t.Fatalf("non-exact policy rule: %v", command)
							}
						}
						continue
					}
					switch command[5] {
					case "-N", "-X":
						chains++
					case "-I", "-D":
						hooks++
						prefixes := []string{"192.168.31.42/32", "192.168.31.43/32"}
						if command[0] == "ip6tables" {
							prefixes = []string{"2001:db8::42/128", "2001:db8::43/128"}
						}
						if !ownedHasArgs(command, "-i", "br-lan", "-s") || !slices.Contains(prefixes, command[len(command)-3]) {
							t.Fatalf("non-exact hook: %v", command)
						}
					}
				}
				return
			}
			for _, commands := range []struct {
				argv [][]string
				op   string
			}{{plan.Apply, "add"}, {plan.Cleanup, "del"}} {
				chains, routes, rules, hooks := countCommands(commands.argv, commands.op)
				if chains != wantChains || routes != wantRoutes || rules != wantRules || hooks != wantHooks {
					t.Fatalf("%s counts chains/routes/rules/hooks=%d/%d/%d/%d", commands.op, chains, routes, rules, hooks)
				}
			}
			countExact := func(want []string) int {
				count := 0
				for _, command := range plan.Apply {
					if slices.Equal(command, want) {
						count++
					}
				}
				return count
			}
			for _, family := range []int{4, 6} {
				clients := plan.Ownership.ClientIPv4s
				tables := []string{"nat", "mangle"}
				hook, names := "PREROUTING", []string{"B6P_V4_DNS", "B6P_V4_CAPTURE"}
				if family == 6 {
					if mode == IPv6Direct {
						continue
					}
					clients = plan.Ownership.ClientIPv6s
					names = []string{"B6P_V6_DNS", "B6P_V6_CAPTURE"}
					if mode == IPv6Block {
						tables, hook, names = []string{"filter"}, "FORWARD", []string{"B6P_V6_BLOCK"}
					}
				}
				if family == 4 || mode == IPv6Follow {
					if countExact(ownedLocalRoute(family, "add")) != 1 {
						t.Fatalf("family %d must have exactly one shared route", family)
					}
				}
				for _, raw := range clients {
					client := netip.MustParseAddr(raw)
					for i, table := range tables {
						want := ownedIPTables(family, table, "-I", hook, "1", "-i", "br-lan", "-s", ownedHostPrefix(client), "-j", names[i])
						if countExact(want) != 1 {
							t.Fatalf("each client requires one exact hook per shared table: %v", want)
						}
					}
					if family == 4 || mode == IPv6Follow {
						want := []string{"ip", fmt.Sprintf("-%d", family), "rule", "add", "priority", "16500", "from", ownedHostPrefix(client), "iif", "br-lan", "fwmark", "0x4000/0x4000", "lookup", "16500"}
						if countExact(want) != 1 {
							t.Fatalf("each captured client requires exactly one policy rule: %v", want)
						}
					}
				}
			}
			for i, command := range plan.Apply {
				if (i >= len(plan.Apply)-wantHooks) != (command[5] == "-I") {
					t.Fatalf("hooks precede complete shared preparation: %v", command)
				}
			}
			for i := 0; i < wantHooks; i++ {
				check := slices.Clone(plan.Apply[len(plan.Apply)-1-i])
				check[5] = "-D"
				check = append(check[:7], check[8:]...)
				if !slices.Equal(plan.Cleanup[i], check) {
					t.Fatalf("cleanup does not unhook every client first: %v", plan.Cleanup[i])
				}
			}
			for _, command := range plan.Apply {
				if slices.Contains(command, "OUTPUT") || slices.Contains(command, "POSTROUTING") || slices.Contains(command, "0.0.0.0/0") && command[2] != "route" {
					t.Fatalf("multiple clients widened capture: %v", command)
				}
			}
			// Increasing clients must not duplicate chain bodies or routing tables.
			single := in
			single.ClientIPv4s, single.ClientIPv6s = in.ClientIPv4s[1:2], in.ClientIPv6s[1:2]
			one := ownedTestPlan(t, single)
			chainBodies := func(commands [][]string) [][]string {
				var out [][]string
				for _, command := range commands {
					if command[0] != "ip" && command[5] == "-A" {
						out = append(out, command)
					}
				}
				return out
			}
			if !reflect.DeepEqual(chainBodies(plan.Apply), chainBodies(one.Apply)) {
				t.Fatal("shared chain bodies changed with the number of clients")
			}
		})
	}
}

func TestOwnedRulesPluralClientsAreBoundedValidatedAndCanonical(t *testing.T) {
	for _, tc := range []struct {
		name string
		edit func(*RulesPlanInput)
	}{
		{"missing-all-v4", func(in *RulesPlanInput) { in.ClientIPv4 = "" }},
		{"v4-prefix", func(in *RulesPlanInput) { in.ClientIPv4s = []string{"192.168.31.0/24"} }},
		{"v4-hostname", func(in *RulesPlanInput) { in.ClientIPv4s = []string{"client.example"} }},
		{"v4-family", func(in *RulesPlanInput) { in.ClientIPv4s = []string{"2001:db8::42"} }},
		{"v4-loopback", func(in *RulesPlanInput) { in.ClientIPv4s = []string{"127.0.0.1"} }},
		{"v4-empty-entry", func(in *RulesPlanInput) { in.ClientIPv4s = []string{""} }},
		{"v6-prefix", func(in *RulesPlanInput) { in.ClientIPv6s = []string{"2001:db8::/64"} }},
		{"v6-zone", func(in *RulesPlanInput) { in.ClientIPv6s = []string{"fe80::42%br-lan"} }},
		{"v6-mapped", func(in *RulesPlanInput) { in.ClientIPv6s = []string{"::ffff:192.168.31.42"} }},
		{"v6-family", func(in *RulesPlanInput) { in.ClientIPv6s = []string{"192.168.31.42"} }},
		{"v6-multicast", func(in *RulesPlanInput) { in.ClientIPv6s = []string{"ff02::1"} }},
		{"v6-command", func(in *RulesPlanInput) { in.ClientIPv6s = []string{"2001:db8::42; reboot"} }},
		{"v4-over-limit", func(in *RulesPlanInput) {
			in.ClientIPv4 = ""
			in.ClientIPv4s = make([]string, MaxCaptureClientsPerFamily+1)
		}},
		{"v6-over-limit", func(in *RulesPlanInput) { in.ClientIPv6s = make([]string, MaxCaptureClientsPerFamily+1) }},
		{"mixed-over-limit", func(in *RulesPlanInput) { in.ClientIPv4s = make([]string, MaxCaptureClientsPerFamily) }},
	} {
		t.Run(tc.name, func(t *testing.T) {
			in := ownedTestInput()
			tc.edit(&in)
			plan, err := PlanOwnedRules(in)
			if err == nil || !reflect.DeepEqual(plan, OwnedRulesPlan{}) {
				t.Fatalf("invalid plural input produced executable intent: %v %+v", err, plan)
			}
		})
	}
	in := ownedTestInput()
	in.ClientIPv4s = []string{"192.168.31.43", in.ClientIPv4, "192.168.31.43"}
	in.ClientIPv6, in.IPv6 = "2001:db8::42", IPv6Follow
	in.ClientIPv6s = []string{"2001:db8::43", "2001:0db8:0000::0042"}
	original4, original6 := slices.Clone(in.ClientIPv4s), slices.Clone(in.ClientIPv6s)
	first := ownedTestPlan(t, in)
	if !slices.Equal(in.ClientIPv4s, original4) || !slices.Equal(in.ClientIPv6s, original6) {
		t.Fatal("compiler mutated input client lists")
	}
	slices.Reverse(in.ClientIPv4s)
	slices.Reverse(in.ClientIPv6s)
	second := ownedTestPlan(t, in)
	if !reflect.DeepEqual(first, second) || len(first.Ownership.ClientIPv4s) != 2 || len(first.Ownership.ClientIPv6s) != 2 {
		t.Fatal("mixed plural/singular input was not sorted and deduplicated")
	}
	in.ClientIPv4s[0], in.ClientIPv6s[0] = "192.168.31.99", "2001:db8::99"
	if !reflect.DeepEqual(first, second) {
		t.Fatal("plan aliases caller-owned clients")
	}
	for _, count := range []int{MaxCaptureClientsPerFamily, MaxCaptureClientsPerFamily + 1} {
		bounded := ownedTestInput()
		bounded.ClientIPv4 = ""
		bounded.IPv6 = IPv6Follow
		for i := 1; i <= count; i++ {
			bounded.ClientIPv4s = append(bounded.ClientIPv4s, fmt.Sprintf("192.0.2.%d", i))
			bounded.ClientIPv6s = append(bounded.ClientIPv6s, fmt.Sprintf("2001:db8::%x", i))
		}
		plan, err := PlanOwnedRules(bounded)
		if (err == nil) != (count == MaxCaptureClientsPerFamily) {
			t.Fatalf("bound %d err=%v", count, err)
		}
		if err == nil && (len(plan.Ownership.ClientIPv4s) != count || len(plan.Ownership.ClientIPv6s) != count || len(plan.Ownership.Chains) != 4) {
			t.Fatal("maximum exact clients did not retain shared chains")
		}
	}
}

func TestOwnedRulesLegacyOwnershipJSONAndCleanupRemainExact(t *testing.T) {
	for _, mode := range []IPv6Mode{IPv6Direct, IPv6Follow, IPv6Block} {
		t.Run(string(mode), func(t *testing.T) {
			in := ownedTestInput()
			in.ClientIPv6, in.IPv6 = "2001:db8::42", mode
			plan := ownedTestPlan(t, in)
			if plan.Ownership.ClientIPv4s != nil || plan.Ownership.ClientIPv6s != nil || plan.Ownership.ClientMACs != nil {
				t.Fatal("legacy singular plan gained plural or MAC ownership")
			}
			var expected [][]string
			if mode == IPv6Follow {
				expected = append(expected,
					[]string{"ip6tables", "-w", "5", "-t", "mangle", "-D", "PREROUTING", "-i", "br-lan", "-s", "2001:db8::42/128", "-j", "B6P_V6_CAPTURE"},
					[]string{"ip6tables", "-w", "5", "-t", "nat", "-D", "PREROUTING", "-i", "br-lan", "-s", "2001:db8::42/128", "-j", "B6P_V6_DNS"})
			} else if mode == IPv6Block {
				expected = append(expected, []string{"ip6tables", "-w", "5", "-t", "filter", "-D", "FORWARD", "-i", "br-lan", "-s", "2001:db8::42/128", "-j", "B6P_V6_BLOCK"})
			}
			expected = append(expected,
				[]string{"iptables", "-w", "5", "-t", "mangle", "-D", "PREROUTING", "-i", "br-lan", "-s", "192.168.31.42/32", "-j", "B6P_V4_CAPTURE"},
				[]string{"iptables", "-w", "5", "-t", "nat", "-D", "PREROUTING", "-i", "br-lan", "-s", "192.168.31.42/32", "-j", "B6P_V4_DNS"})
			chains := []struct{ tool, table, chain string }{}
			if mode == IPv6Follow {
				chains = append(chains, struct{ tool, table, chain string }{"ip6tables", "nat", "B6P_V6_DNS"}, struct{ tool, table, chain string }{"ip6tables", "mangle", "B6P_V6_CAPTURE"})
			} else if mode == IPv6Block {
				chains = append(chains, struct{ tool, table, chain string }{"ip6tables", "filter", "B6P_V6_BLOCK"})
			}
			chains = append(chains, struct{ tool, table, chain string }{"iptables", "nat", "B6P_V4_DNS"}, struct{ tool, table, chain string }{"iptables", "mangle", "B6P_V4_CAPTURE"})
			for _, chain := range chains {
				for _, op := range []string{"-F", "-X"} {
					expected = append(expected, []string{chain.tool, "-w", "5", "-t", chain.table, op, chain.chain})
				}
			}
			if mode == IPv6Follow {
				expected = append(expected,
					[]string{"ip", "-6", "rule", "del", "priority", "16500", "from", "2001:db8::42/128", "iif", "br-lan", "fwmark", "0x4000/0x4000", "lookup", "16500"},
					[]string{"ip", "-6", "route", "del", "local", "::/0", "dev", "lo", "table", "16500"})
			}
			expected = append(expected,
				[]string{"ip", "-4", "rule", "del", "priority", "16500", "from", "192.168.31.42/32", "iif", "br-lan", "fwmark", "0x4000/0x4000", "lookup", "16500"},
				[]string{"ip", "-4", "route", "del", "local", "0.0.0.0/0", "dev", "lo", "table", "16500"})
			if !reflect.DeepEqual(plan.Cleanup, expected) {
				t.Fatalf("legacy cleanup order/shape changed:\ngot  %v\nwant %v", plan.Cleanup, expected)
			}
			for _, value := range []any{in, plan.Ownership} {
				raw, err := json.Marshal(value)
				if err != nil {
					t.Fatal(err)
				}
				if strings.Contains(string(raw), "ClientIPv4s") || strings.Contains(string(raw), "ClientIPv6s") || strings.Contains(string(raw), "ClientMACs") {
					t.Fatalf("legacy JSON gained plural or MAC fields: %s", raw)
				}
			}
		})
	}
}

func ownedMACInput(mode IPv6Mode) RulesPlanInput {
	in := ownedTestInput()
	in.ClientIPv4 = ""
	in.ClientIPv4s = []string{"192.168.31.44", "192.168.31.42", "192.168.31.43"}
	in.IPv6 = mode
	in.ClientMACs = map[string]string{
		"192.168.31.42": "02:11:22:33:44:42",
		"192.168.31.43": "02:11:22:33:44:43",
		"192.168.31.44": "02:11:22:33:44:44",
	}
	if mode != IPv6Direct {
		in.ClientIPv6s = []string{"2001:db8::44", "2001:db8::42", "2001:db8::43"}
		for i := 42; i <= 44; i++ {
			in.ClientMACs[fmt.Sprintf("2001:db8::%d", i)] = in.ClientMACs[fmt.Sprintf("192.168.31.%d", i)]
		}
	}
	return in
}

func TestOwnedRulesThreeClientsRequireExactIPMACPairsOnEveryHook(t *testing.T) {
	for _, mode := range []IPv6Mode{IPv6Direct, IPv6Follow, IPv6Block} {
		t.Run(string(mode), func(t *testing.T) {
			in := ownedMACInput(mode)
			plan := ownedTestPlan(t, in)
			if !stdmaps.Equal(plan.Ownership.ClientMACs, in.ClientMACs) {
				t.Fatalf("MAC ownership changed: %+v", plan.Ownership)
			}
			without := in
			without.ClientMACs = nil
			legacy := ownedTestPlan(t, without)
			if legacy.Ownership.ClientMACs != nil {
				t.Fatal("legacy plural plan gained MAC ownership")
			}
			for _, value := range []any{without, legacy.Ownership} {
				raw, err := json.Marshal(value)
				if err != nil || strings.Contains(string(raw), "ClientMACs") {
					t.Fatalf("legacy plural JSON changed: %s %v", raw, err)
				}
			}
			for _, commands := range []struct{ paired, old [][]string }{
				{plan.Apply, legacy.Apply}, {plan.Cleanup, legacy.Cleanup}, {plan.OnFailure, legacy.OnFailure},
			} {
				if len(commands.paired) != len(commands.old) {
					t.Fatal("adding source MAC changed shared preparation or cleanup count")
				}
				seen := make(map[string]int)
				for i, argv := range commands.paired {
					if argv[0] == "ip" || argv[5] != "-I" && argv[5] != "-D" {
						if !slices.Equal(argv, commands.old[i]) {
							t.Fatalf("source MAC changed route or shared chain body: %v", argv)
						}
						continue
					}
					idx := slices.Index(argv, "-s")
					source := netip.MustParsePrefix(argv[idx+1]).Addr().String()
					mac, exists := in.ClientMACs[source]
					if !exists || !ownedHasArgs(argv, "-i", "br-lan", "-s", argv[idx+1], "-m", "mac", "--mac-source", mac, "-j") {
						t.Fatalf("hook does not bind one exact IP/MAC pair: %v", argv)
					}
					seen[source]++
					stripped := append(slices.Clone(argv[:idx+2]), argv[idx+6:]...)
					if !slices.Equal(stripped, commands.old[i]) {
						t.Fatalf("legacy hook differs beyond exact MAC matcher: %v", argv)
					}
				}
				for source := range in.ClientMACs {
					want := 2 // IPv4 and IPv6 follow both require NAT and mangle hooks.
					if mode == IPv6Block && strings.Contains(source, ":") {
						want = 1
					}
					if seen[source] != want {
						t.Fatalf("source %s has %d hooks, want %d", source, seen[source], want)
					}
				}
			}
		})
	}
}

func TestOwnedRulesClientMACsAreBoundedValidatedAndCanonical(t *testing.T) {
	for _, tc := range []struct {
		name string
		edit func(*RulesPlanInput)
	}{
		{"empty-map", func(in *RulesPlanInput) { in.ClientMACs = map[string]string{} }},
		{"missing-v4", func(in *RulesPlanInput) { delete(in.ClientMACs, "192.168.31.43") }},
		{"missing-v6", func(in *RulesPlanInput) { delete(in.ClientMACs, "2001:db8::43") }},
		{"extra-v4", func(in *RulesPlanInput) { in.ClientMACs["192.168.31.99"] = "02:11:22:33:44:99" }},
		{"extra-v6", func(in *RulesPlanInput) { in.ClientMACs["2001:db8::99"] = "02:11:22:33:44:99" }},
		{"inactive-v6", func(in *RulesPlanInput) { in.IPv6 = IPv6Direct }},
		{"prefix-key", func(in *RulesPlanInput) { in.ClientMACs["192.168.31.42/32"] = "02:11:22:33:44:42" }},
		{"hostname-key", func(in *RulesPlanInput) { in.ClientMACs["client.example"] = "02:11:22:33:44:42" }},
		{"mapped-v4-key", func(in *RulesPlanInput) { in.ClientMACs["::ffff:192.168.31.42"] = "02:11:22:33:44:42" }},
		{"zone-key", func(in *RulesPlanInput) { in.ClientMACs["fe80::42%br-lan"] = "02:11:22:33:44:42" }},
		{"duplicate-canonical-key", func(in *RulesPlanInput) { in.ClientMACs["2001:0db8:0000::0042"] = "02:11:22:33:44:42" }},
		{"map-over-limit", func(in *RulesPlanInput) {
			for i := 0; i <= 2*MaxCaptureClientsPerFamily; i++ {
				in.ClientMACs[fmt.Sprintf("192.0.2.%d", i)] = "02:11:22:33:44:42"
			}
		}},
	} {
		t.Run(tc.name, func(t *testing.T) {
			in := ownedMACInput(IPv6Follow)
			tc.edit(&in)
			plan, err := PlanOwnedRules(in)
			if err == nil || !reflect.DeepEqual(plan, OwnedRulesPlan{}) {
				t.Fatalf("invalid source MAC scope produced executable intent: %v %+v", err, plan)
			}
		})
	}
	for _, mac := range []string{"", "00:00:00:00:00:00", "01:11:22:33:44:55", "ff:ff:ff:ff:ff:ff", "02:11:22:33:44", "02:11:22:33:44:55:66:77", "garbage", " 02:11:22:33:44:55", "02:11:22:33:44:55; reboot", "02:11:22:33:44:55\n"} {
		t.Run("MAC/"+mac, func(t *testing.T) {
			in := ownedMACInput(IPv6Direct)
			in.ClientMACs[in.ClientIPv4s[0]] = mac
			plan, err := PlanOwnedRules(in)
			if err == nil || !reflect.DeepEqual(plan, OwnedRulesPlan{}) {
				t.Fatalf("invalid MAC produced executable intent: %q %v", mac, err)
			}
		})
	}
	in := ownedMACInput(IPv6Follow)
	in.ClientMACs["192.168.31.42"] = "02:AA:BB:CC:DD:42"
	in.ClientMACs["192.168.31.43"] = "02-AA-BB-CC-DD-43"
	in.ClientMACs["192.168.31.44"] = "02aa.bbcc.dd44"
	in.ClientMACs["2001:0db8:0000::0042"] = in.ClientMACs["2001:db8::42"]
	delete(in.ClientMACs, "2001:db8::42")
	original := stdmaps.Clone(in.ClientMACs)
	first, second := ownedTestPlan(t, in), ownedTestPlan(t, in)
	if !stdmaps.Equal(in.ClientMACs, original) || !reflect.DeepEqual(first, second) {
		t.Fatal("compiler changed caller map or produced nondeterministic MAC intent")
	}
	for source, want := range map[string]string{"192.168.31.42": "02:aa:bb:cc:dd:42", "192.168.31.43": "02:aa:bb:cc:dd:43", "192.168.31.44": "02:aa:bb:cc:dd:44", "2001:db8::42": "02:11:22:33:44:42"} {
		if first.Ownership.ClientMACs[source] != want {
			t.Fatalf("noncanonical IP/MAC ownership for %s: %v", source, first.Ownership.ClientMACs)
		}
	}
	in.ClientMACs["192.168.31.43"] = "02:00:00:00:00:99"
	first.Ownership.ClientMACs["192.168.31.42"] = "02:00:00:00:00:99"
	if second.Ownership.ClientMACs["192.168.31.42"] != "02:aa:bb:cc:dd:42" || second.Ownership.ClientMACs["192.168.31.43"] != "02:aa:bb:cc:dd:43" {
		t.Fatal("compiled ownership aliases input or another plan map")
	}
	// Both family bounds are supported together without increasing map scope.
	bounded := ownedTestInput()
	bounded.ClientIPv4, bounded.IPv6 = "", IPv6Follow
	bounded.ClientMACs = map[string]string{}
	for i := 1; i <= MaxCaptureClientsPerFamily; i++ {
		v4, v6 := fmt.Sprintf("192.0.2.%d", i), fmt.Sprintf("2001:db8::%x", i)
		bounded.ClientIPv4s = append(bounded.ClientIPv4s, v4)
		bounded.ClientIPv6s = append(bounded.ClientIPv6s, v6)
		bounded.ClientMACs[v4], bounded.ClientMACs[v6] = "02:11:22:33:44:42", "02:11:22:33:44:42"
	}
	if len(ownedTestPlan(t, bounded).Ownership.ClientMACs) != 2*MaxCaptureClientsPerFamily {
		t.Fatal("maximum bounded paired families lost client identity")
	}
}

// This hook matcher checks only generated argv, not kernel/offload behavior.
func ownedTestHookTarget(commands [][]string, table, source, iface, mac string) string {
	addr := netip.MustParseAddr(source)
	for _, argv := range commands {
		if argv[0] == "ip" || argv[4] != table || argv[5] != "-I" {
			continue
		}
		match, target := true, ""
		for i := 8; i < len(argv); i++ {
			switch argv[i] {
			case "-i":
				i++
				match = match && argv[i] == iface
			case "-s":
				i++
				match = match && netip.MustParsePrefix(argv[i]).Contains(addr)
			case "--mac-source":
				i++
				match = match && argv[i] == mac
			case "-j":
				i++
				target = argv[i]
			}
		}
		if match {
			return target
		}
	}
	return ""
}

func TestOwnedRulesReusedIPUnknownMACNeverReachesNATOrMangleHooks(t *testing.T) {
	in := ownedMACInput(IPv6Follow)
	plan := ownedTestPlan(t, in)
	for source, mac := range in.ClientMACs {
		for _, table := range []string{"nat", "mangle"} {
			if ownedTestHookTarget(plan.Apply, table, source, "br-lan", mac) == "" {
				t.Fatalf("authorized exact pair missing %s hook: %s %s", table, source, mac)
			}
			for _, otherMAC := range []string{"02:ff:ff:ff:ff:99", "02:11:22:33:44:42", "02:11:22:33:44:43", "02:11:22:33:44:44"} {
				if otherMAC != mac && ownedTestHookTarget(plan.Apply, table, source, "br-lan", otherMAC) != "" {
					t.Fatalf("reused source IP reaches %s with unknown/wrong MAC: %s %s", table, source, otherMAC)
				}
			}
			if ownedTestHookTarget(plan.Apply, table, source, "other-lan", mac) != "" || ownedTestHookTarget(plan.Apply, table, "192.168.31.99", "br-lan", mac) != "" {
				t.Fatal("MAC matching widened source IP or interface scope")
			}
		}
	}
}

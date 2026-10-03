package proxy

import (
	"fmt"
	"net/netip"
	"reflect"
	"slices"
	"strings"
	"testing"
)

func TestOwnedRulesEndpointResolverDNSRedirectPrecedesBypass(t *testing.T) {
	for _, follow := range []bool{false, true} {
		for _, fake := range []bool{false, true} {
			t.Run(fmt.Sprintf("follow=%v/fake=%v", follow, fake), func(t *testing.T) {
				input := ownedTestInput()
				input.ClientIPv4 = "192.168.31.250"
				input.FakeIP = fake
				input.EndpointIPs = []string{"223.5.5.5", "192.168.31.1", "192.168.31.53", "169.254.1.53"}
				if follow {
					input.IPv6, input.ClientIPv6 = IPv6Follow, "2001:db8::250"
					input.EndpointIPs = append(input.EndpointIPs, "2001:4860:4860::8888", "fd12::53", "fe80::53")
				}
				plan := ownedTestPlan(t, input)
				families := []struct{ dns, capture, resolver, prefix string }{
					{"B6P_V4_DNS", "B6P_V4_CAPTURE", "223.5.5.5", "223.5.5.5/32"},
				}
				if follow {
					families = append(families, struct{ dns, capture, resolver, prefix string }{
						"B6P_V6_DNS", "B6P_V6_CAPTURE", "2001:4860:4860::8888", "2001:4860:4860::8888/128",
					})
				}
				for _, family := range families {
					endpoint := ownedFindCommand(plan.Apply, "-A", family.dns, "-d", family.prefix, "-j", "RETURN")
					for _, protocol := range []string{"tcp", "udp"} {
						redirect := ownedFindCommand(plan.Apply, "-A", family.dns, "-p", protocol, "--dport", "53", "-j", "REDIRECT", "--to-ports", "2082")
						if redirect < 0 || endpoint <= redirect {
							t.Fatalf("%s %s DNS resolver endpoint bypass precedes redirect: redirect=%d endpoint=%d", family.dns, protocol, redirect, endpoint)
						}
						if got := ownedTestVerdict(t, plan.Apply, family.capture, family.resolver, protocol, 53, false); got != "RETURN" {
							t.Fatalf("%s %s endpoint DNS must reach NAT, got %s", family.capture, protocol, got)
						}
						if got := ownedTestVerdict(t, plan.Apply, family.dns, family.resolver, protocol, 53, false); got != "REDIRECT" {
							t.Fatalf("%s %s endpoint DNS got %s, want REDIRECT", family.dns, protocol, got)
						}
						for _, chain := range []string{family.capture, family.dns} {
							if got := ownedTestVerdict(t, plan.Apply, chain, family.resolver, protocol, 443, false); got != "RETURN" {
								t.Fatalf("%s %s non-DNS endpoint transport got %s, want RETURN", chain, protocol, got)
							}
						}
					}
				}
			})
		}
	}
}

func TestOwnedRulesEndpointDNSKeepsManagementLocalSafetyAndPrivateIntent(t *testing.T) {
	input := ownedTestInput()
	input.IPv6, input.ClientIPv6 = IPv6Follow, "2001:db8::250"
	input.ManagementIPs = []string{"192.168.31.1", "192.168.31.2", "203.0.113.2", "fd12::1", "fd12::2"}
	input.RouterDNSAddresses = []string{"192.168.31.1", "fd12::1"}
	input.EndpointIPs = []string{
		"223.5.5.5", "192.168.31.1", "192.168.31.2", "203.0.113.2", "192.168.31.53",
		"0.0.0.0", "127.0.0.53", "169.254.1.53", "192.0.0.53", "224.0.0.53", "255.255.255.255",
		"fd12::1", "fd12::2", "fd12::53", "::", "::1", "fe80::53", "ff02::53",
	}
	plan := ownedTestPlan(t, input)
	for _, family := range []struct{ dns, router, prefix string }{
		{"B6P_V4_DNS", "192.168.31.1", "192.168.31.1/32"},
		{"B6P_V6_DNS", "fd12::1", "fd12::1/128"},
	} {
		local := ownedFindCommand(plan.Apply, "-A", family.dns, "-m", "addrtype", "--dst-type", "LOCAL", "-j", "RETURN")
		for _, protocol := range []string{"tcp", "udp"} {
			redirect := ownedFindCommand(plan.Apply, "-A", family.dns, "-d", family.prefix, "-p", protocol, "--dport", "53", "-j", "REDIRECT")
			if redirect < 0 || local <= redirect {
				t.Fatalf("%s %s router DNS opt-in must precede LOCAL", family.dns, protocol)
			}
		}
	}
	for _, protocol := range []string{"tcp", "udp"} {
		for _, tc := range []struct {
			chain, destination string
			local              bool
			want               string
		}{
			{"B6P_V4_DNS", "192.168.31.1", true, "REDIRECT"},
			{"B6P_V6_DNS", "fd12::1", true, "REDIRECT"},
			{"B6P_V4_DNS", "192.168.31.2", true, "RETURN"},
			{"B6P_V4_DNS", "192.168.31.2", false, "RETURN"},
			{"B6P_V4_DNS", "203.0.113.2", false, "RETURN"},
			{"B6P_V6_DNS", "fd12::2", false, "RETURN"},
			{"B6P_V4_DNS", "223.5.5.5", true, "RETURN"},
			{"B6P_V4_DNS", "192.168.31.9", true, "RETURN"},
			{"B6P_V6_DNS", "fd12::9", true, "RETURN"},
			{"B6P_V4_DNS", "0.0.0.0", false, "RETURN"},
			{"B6P_V4_DNS", "127.0.0.53", false, "RETURN"},
			{"B6P_V4_DNS", "169.254.1.53", false, "RETURN"},
			{"B6P_V4_DNS", "192.0.0.53", false, "RETURN"},
			{"B6P_V4_DNS", "224.0.0.53", false, "RETURN"},
			{"B6P_V4_DNS", "255.255.255.255", false, "RETURN"},
			{"B6P_V6_DNS", "::", false, "RETURN"},
			{"B6P_V6_DNS", "::1", false, "RETURN"},
			{"B6P_V6_DNS", "fe80::53", false, "RETURN"},
			{"B6P_V6_DNS", "ff02::53", false, "RETURN"},
			// Ordinary private DNS was already redirected before private bypass.
			{"B6P_V4_DNS", "192.168.31.53", false, "REDIRECT"},
			{"B6P_V4_DNS", "10.0.0.53", false, "REDIRECT"},
			{"B6P_V6_DNS", "fd12::53", false, "REDIRECT"},
			{"B6P_V6_DNS", "fd12::54", false, "REDIRECT"},
		} {
			if got := ownedTestVerdict(t, plan.Apply, tc.chain, tc.destination, protocol, 53, tc.local); got != tc.want {
				t.Errorf("%s %s %s:53 local=%v got %s, want %s", tc.chain, tc.destination, protocol, tc.local, got, tc.want)
			}
		}
		for _, destination := range []string{"192.168.31.1", "192.168.31.2"} {
			for _, port := range []int{22, 8787, 443} {
				for _, chain := range []string{"B6P_V4_DNS", "B6P_V4_CAPTURE"} {
					if got := ownedTestVerdict(t, plan.Apply, chain, destination, protocol, port, true); got != "RETURN" {
						t.Errorf("management %s %s %s:%d got %s, want RETURN", chain, protocol, destination, port, got)
					}
				}
			}
		}
	}
	without := input
	without.RouterDNSAddresses = nil
	factory := ownedTestPlan(t, without)
	for _, protocol := range []string{"tcp", "udp"} {
		if got := ownedTestVerdict(t, factory.Apply, "B6P_V4_DNS", "192.168.31.1", protocol, 53, true); got != "RETURN" {
			t.Fatalf("router DNS without opt-in got %s, want factory RETURN", got)
		}
	}
}

func TestOwnedRulesEndpointDNSKeepsExactMACScopeAndDeterministicOwnership(t *testing.T) {
	for _, mode := range []IPv6Mode{IPv6Direct, IPv6Follow, IPv6Block} {
		t.Run(string(mode), func(t *testing.T) {
			input := ownedMACInput(mode)
			input.EndpointIPs = []string{"223.5.5.5", "203.0.113.20", "223.5.5.5", "2001:4860:4860::8888"}
			plan := ownedTestPlan(t, input)
			reordered := input
			reordered.EndpointIPs = []string{"2001:4860:4860::8888", "223.5.5.5", "203.0.113.20"}
			if !reflect.DeepEqual(plan, ownedTestPlan(t, reordered)) {
				t.Fatal("endpoint order/duplicates changed deterministic argv intent")
			}
			without := input
			without.EndpointIPs = nil
			other := ownedTestPlan(t, without)
			if !reflect.DeepEqual(plan.Ownership, other.Ownership) || !reflect.DeepEqual(plan.Cleanup, other.Cleanup) || !reflect.DeepEqual(plan.OnFailure, other.OnFailure) {
				t.Fatal("DNS endpoint change altered exact ownership or cleanup")
			}
			seen := make(map[string]int)
			for _, command := range plan.Apply {
				if command[0] == "ip" {
					continue
				}
				if ownedHasArgs(command, "OUTPUT") {
					t.Fatalf("endpoint DNS must not change router-originated traffic: %v", command)
				}
				if mode == IPv6Direct && command[0] == "ip6tables" {
					t.Fatalf("direct IPv6 acquired a capture rule: %v", command)
				}
				if command[5] != "-I" {
					continue
				}
				idx := slices.Index(command, "-s")
				if idx < 0 || idx+1 >= len(command) {
					t.Fatalf("missing exact client source: %v", command)
				}
				source := command[idx+1]
				address := netip.MustParsePrefix(source).Addr().String()
				mac, exists := input.ClientMACs[address]
				if !exists || !ownedHasArgs(command, "-i", "br-lan", "-s", source, "-m", "mac", "--mac-source", mac, "-j") {
					t.Fatalf("DNS endpoint change widened IP/MAC hook: %v", command)
				}
				seen[address]++
			}
			for source := range input.ClientMACs {
				want := 2
				if mode == IPv6Block && strings.Contains(source, ":") {
					want = 1
				}
				if seen[source] != want {
					t.Fatalf("source %s got %d exact hooks, want %d", source, seen[source], want)
				}
			}
		})
	}
}

package proxy

import (
	"bytes"
	"reflect"
	"testing"
)

// These fixtures only compile native JSON. They do not open sockets, read rule
// sets, or simulate sing-box metadata, QUIC parsing, or transparent delivery.
func nativeSniffFixture(t *testing.T) CompileInput {
	t.Helper()
	node := testNode(t)
	node.Server, node.ServerName = "192.0.2.2", "node.example"
	return CompileInput{
		Node:             node,
		DirectDNS:        DNSEndpoint{Server: "192.0.2.53", Port: 853, ServerName: "direct-resolver.example"},
		ProxyDNS:         DNSEndpoint{Server: "198.51.100.53", Port: 853, ServerName: "proxy-resolver.example"},
		ManagementIPs:    []string{"10.20.30.1"},
		Endpoints:        []string{"192.0.2.3"},
		BootstrapDomains: []string{"bootstrap.example"},
		RuleSets:         stagedRefs(),
		Overrides:        []Rule{{Kind: RuleDomain, Value: "override.example", Target: TargetBlock}},
		Rules: []Rule{
			{Kind: RuleDomain, Value: "direct.example", Target: TargetDirect},
			{Kind: RuleDomain, Value: "block.example", Target: TargetBlock},
			{Kind: RuleDomainSuffix, Value: "proxy.example", Target: TargetProxy},
			{Kind: RuleIPCIDR, Value: "203.0.113.0/24", Target: TargetDirect, NoResolve: true},
			{Kind: RuleMatch, Target: TargetProxy},
		},
	}
}

func nativeSniffPosition(t *testing.T, rules []map[string]any) int {
	t.Helper()
	position := -1
	for i, rule := range rules {
		if rule["action"] != "sniff" {
			continue
		}
		if position >= 0 {
			t.Fatal("multiple native sniff actions")
		}
		position = i
	}
	if position < 0 {
		t.Fatal("missing native sniff action")
	}
	return position
}

func TestNativeSniffIncludesQUICWithoutTransportFeature(t *testing.T) {
	for _, mode := range []IPv6Mode{IPv6Follow, IPv6Direct, IPv6Block} {
		for _, fake := range []bool{false, true} {
			name := string(mode)
			if fake {
				name += "-fake"
			}
			t.Run(name, func(t *testing.T) {
				in := nativeSniffFixture(t)
				in.IPv6, in.FakeIP = mode, fake
				out, err := CompileNative(in)
				if err != nil {
					t.Fatal(err)
				}
				rules := maps(decodeConfig(t, out)["route"].(map[string]any)["rules"])
				sniff := rules[nativeSniffPosition(t, rules)]
				want := map[string]any{
					"inbound": []any{"mixed-in", "tproxy-in"},
					"action":  "sniff",
					"sniffer": []any{"http", "tls", "dns", "quic"},
					"timeout": "300ms",
				}
				if !reflect.DeepEqual(sniff, want) {
					t.Fatalf("native ingress sniff must include QUIC alongside HTTP/TLS/DNS: got %v, want %v", sniff, want)
				}
				if out.CoreVersion != "1.14.2" {
					t.Fatal("QUIC sniff regression no longer targets sing-box 1.14.2")
				}
				wantFeatures := []string{"with_utls", "badlinkname", "tcp_fast_open", "tproxy_tcp_udp", "tls_dns"}
				if !reflect.DeepEqual(out.RequiredFeatures, wantFeatures) {
					t.Fatalf("packet sniffing must not add a QUIC transport build requirement: %v", out.RequiredFeatures)
				}
			})
		}
	}
}

func TestNativeSniffPreservesBypassAndClassificationOrder(t *testing.T) {
	for _, mode := range []IPv6Mode{IPv6Follow, IPv6Direct, IPv6Block} {
		t.Run(string(mode), func(t *testing.T) {
			in := nativeSniffFixture(t)
			in.IPv6 = mode
			out, err := CompileNative(in)
			if err != nil {
				t.Fatal(err)
			}
			rules := maps(decodeConfig(t, out)["route"].(map[string]any)["rules"])
			sniff := nativeSniffPosition(t, rules)
			position := func(key, value string) int {
				for i, rule := range rules {
					if nativeRuleMatchesValue(rule, key, value) {
						return i
					}
				}
				t.Fatalf("missing native match %s=%s", key, value)
				return -1
			}
			previous := -1
			for _, match := range []struct{ key, value string }{
				{"inbound", "dns-in"},
				{"ip_cidr", "10.20.30.1/32"},
				{"domain", "bootstrap.example"},
				{"ip_cidr", "192.168.0.0/16"},
				{"domain_suffix", "lan"},
			} {
				p := position(match.key, match.value)
				if p <= previous || p >= sniff {
					t.Fatalf("bypass/local-DNS priority changed before sniff: %s=%s at %d, previous=%d, sniff=%d", match.key, match.value, p, previous, sniff)
				}
				previous = p
			}
			clientDNS := -1
			for i, rule := range rules {
				if rule["action"] == "hijack-dns" && reflect.DeepEqual(rule["port"], []any{float64(53)}) {
					clientDNS = i
					if !reflect.DeepEqual(rule["inbound"], []any{"mixed-in", "tproxy-in"}) {
						t.Fatal("client DNS hijack lost its ingress scope")
					}
				}
			}
			if clientDNS <= position("domain", "bootstrap.example") || clientDNS >= position("ip_cidr", "192.168.0.0/16") {
				t.Fatal("client DNS hijack priority changed")
			}
			for _, value := range []string{"192.0.2.2/32", "192.0.2.3/32", "192.0.2.53/32"} {
				if p := position("ip_cidr", value); p >= sniff || rules[p]["outbound"] != "direct" {
					t.Fatalf("endpoint/resolver bypass moved behind sniff: %s", value)
				}
			}
			ipv6Policy := -1
			for i, rule := range rules {
				if rule["ip_version"] == float64(6) {
					ipv6Policy = i
				}
			}
			if mode == IPv6Follow {
				if ipv6Policy >= 0 {
					t.Fatal("IPv6 follow unexpectedly gained a catch-all")
				}
			} else if ipv6Policy <= previous || ipv6Policy >= sniff ||
				(mode == IPv6Direct && rules[ipv6Policy]["outbound"] != "direct") ||
				(mode == IPv6Block && rules[ipv6Policy]["action"] != "reject") {
				t.Fatal("IPv6 catch-all policy moved or changed")
			}
			previous = sniff
			for _, match := range []struct{ key, value string }{
				{"domain", "override.example"},
				{"domain", "direct.example"},
				{"domain", "block.example"},
				{"domain_suffix", "proxy.example"},
				{"ip_cidr", "203.0.113.0/24"},
				{"rule_set", "cn-domain"},
				{"rule_set", "proxy-domain"},
				{"rule_set", "cn-ip"},
			} {
				p := position(match.key, match.value)
				if p <= previous {
					t.Fatalf("classification order changed after sniff: %s=%s at %d, previous=%d", match.key, match.value, p, previous)
				}
				previous = p
			}
			for _, match := range []struct{ key, value, actionKey, action string }{
				{"domain", "override.example", "action", "reject"},
				{"domain", "direct.example", "outbound", "direct"},
				{"domain", "block.example", "action", "reject"},
				{"domain_suffix", "proxy.example", "outbound", "proxy"},
				{"ip_cidr", "203.0.113.0/24", "outbound", "direct"},
			} {
				if rules[position(match.key, match.value)][match.actionKey] != match.action {
					t.Fatalf("classification target changed for %s", match.value)
				}
			}
			ip := position("ip_cidr", "203.0.113.0/24")
			if rules[ip-1]["action"] == "resolve" {
				t.Fatal("no-resolve original-IP rule gained eager resolution")
			}
			cnIP := position("rule_set", "cn-ip")
			if rules[cnIP-1]["action"] != "resolve" || rules[cnIP-1]["server"] != "dns-proxy" {
				t.Fatal("CN-IP fallback lost its existing resolve action")
			}
			if rules[len(rules)-1]["outbound"] != "proxy" || len(rules[len(rules)-1]) != 1 {
				t.Fatal("terminal MATCH proxy policy changed")
			}
		})
	}
}

func TestNativeSniffDoesNotEmitDestinationOverrides(t *testing.T) {
	out, err := CompileNative(nativeSniffFixture(t))
	if err != nil {
		t.Fatal(err)
	}
	// A sniffed domain is classification metadata, not permission to rewrite an
	// original IP. This asserts compiler intent only, not runtime Destination.
	for _, key := range []string{"override_destination", "sniff_override_destination", "override_address", "override_port"} {
		if bytes.Contains(out.Config, []byte(`"`+key+`":`)) {
			t.Fatalf("sniffing must not override the original destination: %s", key)
		}
	}
	config := decodeConfig(t, out)
	inbounds := maps(config["inbounds"])
	if inbounds[1]["type"] != "tproxy" || inbounds[1]["network"] != nil {
		t.Fatal("transparent original-destination TCP/UDP ingress changed")
	}
	proxy := maps(config["outbounds"])[1]
	if proxy["type"] != "vless" || proxy["packet_encoding"] != "xudp" || proxy["transport"] != nil || proxy["network"] != nil {
		t.Fatal("VLESS TCP/XUDP transport changed for QUIC packet sniffing")
	}
}

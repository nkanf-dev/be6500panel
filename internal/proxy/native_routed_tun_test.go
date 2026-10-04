package proxy

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"reflect"
	"strings"
	"testing"
)

func routedTUNFixture(t *testing.T) CompileInput {
	t.Helper()
	in := nativeSniffFixture(t)
	in.Datapath = DatapathRoutedTUN
	in.RoutedTUN = &RoutedTUNConfig{InterfaceName: "b6p-tun", Address: "172.31.255.253/30"}
	return in
}

// These hashes pin bytes from the pre-TUN compiler, not just JSON semantics.
func TestNativeRoutedTUNLegacyGolden(t *testing.T) {
	cases := []struct {
		name string
		mode IPv6Mode
		fake bool
		want string
	}{
		{"default", "", false, "3b3d731238b2f6a1bf2029123c1ed0279edde4413663f2e684ccd48ac5c0bb8a"},
		{"direct", IPv6Direct, false, "a7233b42561a9071a3f61d277b038f6fee953c365285fdcfbf607693c69cfdfb"},
		{"direct-fake", IPv6Direct, true, "c3b2714e5fc154d50ba540647dc84ede36abd588a489b16aa3c909b56a388301"},
		{"follow", IPv6Follow, false, "3b4319c215849541fafac60166986c79119aee3a639ef48edb81f3d31b581aa0"},
		{"follow-fake", IPv6Follow, true, "8000114d13b1effa68ed3e1a96053e6317c9f4de05f9386d2be27e9b95253bb4"},
		{"block", IPv6Block, false, "650cf28acb52b4a98252186f2020a417d91fa5415dccf181ef7039eeb4445f75"},
		{"block-fake", IPv6Block, true, "0db950f37096909ddc3897e2e114833494bea8c25007b0eaf47b9529cf460e7a"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			in := nativeSniffFixture(t)
			if tc.name == "default" {
				in = CompileInput{Node: testNode(t)}
			}
			in.IPv6, in.FakeIP = tc.mode, tc.fake
			for _, mode := range []DatapathMode{"", DatapathTPROXY} {
				in.Datapath = mode
				out, err := CompileNative(in)
				if err != nil {
					t.Fatal(err)
				}
				hash := sha256.Sum256(out.Config)
				if got := hex.EncodeToString(hash[:]); got != tc.want || got != out.SHA256 {
					t.Errorf("legacy byte hash mode=%q: got %s, want %s", mode, got, tc.want)
				}
				features := []string{"with_utls", "badlinkname", "tcp_fast_open", "tproxy_tcp_udp", "tls_dns"}
				if !reflect.DeepEqual(out.RequiredFeatures, features) {
					t.Fatal("legacy feature requirements changed")
				}
			}
		})
	}
}

func TestNativeRoutedTUNInboundAndRequiredFeatures(t *testing.T) {
	in := routedTUNFixture(t)
	out, err := CompileNative(in)
	if err != nil {
		t.Fatal(err)
	}
	config := decodeConfig(t, out)
	inbounds := maps(config["inbounds"])
	want := []map[string]any{
		{"type": "mixed", "tag": "mixed-in", "listen": "127.0.0.1", "listen_port": float64(2080)},
		{"type": "tun", "tag": "tun-in", "interface_name": "b6p-tun", "address": []any{"172.31.255.253/30"}, "mtu": float64(1500), "dns_mode": "disabled", "auto_route": false, "auto_redirect": false, "stack": "system", "udp_timeout": "2m", "udp_nat_max": float64(1024)},
		{"type": "direct", "tag": "dns-in", "listen": "127.0.0.1", "listen_port": float64(1053)},
	}
	if !reflect.DeepEqual(inbounds, want) {
		t.Fatalf("routed TUN must replace only the transparent inbound: got %v, want %v", inbounds, want)
	}
	hash := sha256.Sum256(out.Config)
	if out.SHA256 != hex.EncodeToString(hash[:]) || !bytes.HasSuffix(out.Config, []byte("\n")) {
		t.Fatal("TUN hash does not cover the exact native bytes and final newline")
	}
	features := []string{"with_utls", "badlinkname", "tcp_fast_open", "system_tun_tcp_udp", "tls_dns"}
	if !reflect.DeepEqual(out.RequiredFeatures, features) || out.CoreVersion != "1.14.2" || out.IPv6 != IPv6Direct || out.Failure != FailureDirect {
		t.Fatal("routed system-TUN requirements or policy changed")
	}
}

func TestNativeRoutedTUNPreservesPolicyAndOriginalPacketIntent(t *testing.T) {
	for _, fake := range []bool{false, true} {
		in := routedTUNFixture(t)
		in.FakeIP = fake
		out, err := CompileNative(in)
		if err != nil {
			t.Fatal(err)
		}
		legacy := in
		legacy.Datapath, legacy.RoutedTUN = "", nil
		old, err := CompileNative(legacy)
		if err != nil {
			t.Fatal(err)
		}
		config, oldConfig := decodeConfig(t, out), decodeConfig(t, old)
		// Replace the one expected listener change and ingress tag. All other
		// route order, local DNS authority, staged rules and outbounds are exact.
		config["inbounds"].([]any)[1] = oldConfig["inbounds"].([]any)[1]
		rules := maps(config["route"].(map[string]any)["rules"])
		clientDNS, sniff := 0, 0
		for _, rule := range rules {
			if rule["action"] != "sniff" && (rule["action"] != "hijack-dns" || rule["port"] == nil) {
				continue
			}
			if !reflect.DeepEqual(rule["inbound"], []any{"mixed-in", "tun-in"}) {
				t.Fatalf("client policy does not include TUN ingress: %v", rule)
			}
			if rule["action"] == "sniff" {
				sniff++
				if !reflect.DeepEqual(rule["sniffer"], []any{"http", "tls", "dns", "quic"}) || rule["timeout"] != "300ms" {
					t.Fatal("TUN sniff protocol intent changed")
				}
			} else {
				clientDNS++
				if !reflect.DeepEqual(rule["port"], []any{float64(53)}) {
					t.Fatal("TUN client DNS port changed")
				}
			}
			rule["inbound"] = []any{"mixed-in", "tproxy-in"}
		}
		if clientDNS != 1 || sniff != 1 || !reflect.DeepEqual(config, oldConfig) {
			t.Fatal("routed TUN changed policy, node identity, resolver authority or native fields beyond its ingress")
		}
		if !reflect.DeepEqual(out.Diagnostics, old.Diagnostics) || !reflect.DeepEqual(out.EndpointHosts, old.EndpointHosts) || out.Failure != old.Failure || out.IPv6 != old.IPv6 {
			t.Fatal("routed TUN changed public compiler metadata beyond required features")
		}
		for _, key := range []string{
			"sniff", "sniff_override_destination", "override_destination", "override_address", "override_port",
			"source_ip_cidr", "source_port", "source_port_range", "routing_mark", "default_mark",
			"inet4_address", "inet6_address", "iproute2_table_index", "iproute2_rule_index", "strict_route",
		} {
			// The route action value "sniff" is current; an inbound sniff key is not.
			if bytes.Contains(out.Config, []byte(`"`+key+`":`)) {
				t.Fatalf("unexpected source/destination rewrite, automatic routing or deprecated key: %s", key)
			}
		}
		for _, value := range []string{`"socks"`, `"tproxy"`, `"gvisor"`, `"with_gvisor"`} {
			if bytes.Contains(out.Config, []byte(value)) {
				t.Fatalf("TUN must be a main-core system inbound without sidecars: %s", value)
			}
		}
	}
}

func TestNativeRoutedTUNRejectsInvalidSelectionAndConfiguration(t *testing.T) {
	cases := []struct {
		name   string
		change func(*CompileInput)
		want   string
	}{
		{"unknown-datapath", func(in *CompileInput) { in.Datapath = "private-token" }, "invalid datapath"},
		{"datapath-whitespace", func(in *CompileInput) { in.Datapath = " routed-tun" }, "invalid datapath"},
		{"datapath-case", func(in *CompileInput) { in.Datapath = "Routed-TUN" }, "invalid datapath"},
		{"missing-config", func(in *CompileInput) { in.RoutedTUN = nil }, "requires explicit configuration"},
		{"default-with-config", func(in *CompileInput) { in.Datapath = "" }, "requires routed-tun datapath"},
		{"tproxy-with-config", func(in *CompileInput) { in.Datapath = DatapathTPROXY }, "requires routed-tun datapath"},
		{"empty-config", func(in *CompileInput) { in.RoutedTUN = &RoutedTUNConfig{} }, "interface"},
		{"ipv6-follow", func(in *CompileInput) { in.IPv6 = IPv6Follow }, "routed-tun supports only IPv6 direct"},
		{"ipv6-block", func(in *CompileInput) { in.IPv6 = IPv6Block }, "routed-tun supports only IPv6 direct"},
		{"management-host", func(in *CompileInput) { in.ManagementIPs = []string{"172.31.255.253"} }, "overlaps router management"},
		{"management-peer", func(in *CompileInput) { in.ManagementIPs = []string{"172.31.255.254"} }, "overlaps router management"},
		{"management-network", func(in *CompileInput) { in.ManagementIPs = []string{"172.31.255.252"} }, "overlaps router management"},
		{"management-broadcast", func(in *CompileInput) { in.ManagementIPs = []string{"172.31.255.255"} }, "overlaps router management"},
		{"mixed-zero", func(in *CompileInput) { in.Ports = Ports{Mixed: 0, TProxy: 7893, DNS: 1053} }, "listener ports"},
		{"tproxy-zero", func(in *CompileInput) { in.Ports = Ports{Mixed: 2080, TProxy: 0, DNS: 1053} }, "listener ports"},
		{"dns-zero", func(in *CompileInput) { in.Ports = Ports{Mixed: 2080, TProxy: 7893, DNS: 0} }, "listener ports"},
		{"duplicate-mixed-tproxy", func(in *CompileInput) { in.Ports = Ports{Mixed: 2080, TProxy: 2080, DNS: 1053} }, "listener ports"},
		{"duplicate-mixed-DNS", func(in *CompileInput) { in.Ports = Ports{Mixed: 2080, TProxy: 7893, DNS: 2080} }, "listener ports"},
		{"duplicate-tproxy-DNS", func(in *CompileInput) { in.Ports = Ports{Mixed: 2080, TProxy: 7893, DNS: 7893} }, "listener ports"},
		{"failure-block", func(in *CompileInput) { in.Failure = FailureBlockProxy }, "not supported"},
		{"failure-unknown", func(in *CompileInput) { in.Failure = "private-token" }, "invalid failure policy"},
		{"unstaged-ruleset", func(in *CompileInput) {
			in.Rules = []Rule{{Kind: RuleSet, Value: "cn-domain", Target: TargetProxy}}
			in.RuleSets = nil
		}, "unstaged controlled set"},
		{"local-DNS-loop", func(in *CompileInput) { in.LocalDNS = &LocalDNSConfig{Port: 1053} }, "core listener port"},
		{"node-no-UDP", func(in *CompileInput) { in.Node.UDP = false }, "UDP/XUDP must be enabled"},
		{"unsupported-rule", func(in *CompileInput) {
			in.Diagnostics = []Diagnostic{{Scope: "rule", Index: 7, Message: "private-token"}}
		}, "explicit acknowledgement"},
		{"management-limit", func(in *CompileInput) { in.ManagementIPs = make([]string, 129) }, "compiler input limit"},
		{"endpoint-limit", func(in *CompileInput) { in.Endpoints = make([]string, 257) }, "compiler input limit"},
		{"mixed-address", func(in *CompileInput) { in.MixedListenAddress = "private-token" }, "listener address"},
		{"dns-address", func(in *CompileInput) { in.DNSListenAddress = "private-token" }, "listener address"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			in := routedTUNFixture(t)
			tc.change(&in)
			out, err := CompileNative(in)
			if err == nil || len(out.Config) != 0 {
				t.Fatal("unsafe or unsupported routed TUN input accepted")
			}
			if !strings.Contains(err.Error(), tc.want) || strings.Contains(err.Error(), "private-token") {
				t.Fatalf("imprecise or input-leaking rejection: %v", err)
			}
		})
	}
}

func TestNativeRoutedTUNRejectsUnsafeInterface(t *testing.T) {
	for _, value := range []string{"", "b6p-", "tun0", "lo", "eth0", "br-lan", "lan", "b6p-.tun", "b6p-tun.1", "b6p-:tun", "b6p-/tun", "b6p- tun", "b6p-tun\n", "b6p-tun\x00", "b6p-tun;private-token", "b6p-tün", "b6p-123456789012"} {
		t.Run(value, func(t *testing.T) {
			in := routedTUNFixture(t)
			in.RoutedTUN.InterfaceName = value
			out, err := CompileNative(in)
			if err == nil || len(out.Config) != 0 || !strings.Contains(err.Error(), "interface") || strings.Contains(err.Error(), "private-token") {
				t.Fatalf("unsafe interface not rejected safely: %v", err)
			}
		})
	}
}

func TestNativeRoutedTUNRejectsUnsafeAddress(t *testing.T) {
	for _, value := range []string{
		"", "172.31.255.253", "private-token/30", "172.31.255.253/29", "172.31.255.253/31", "172.31.255.253/32",
		"172.31.255.252/30", "172.31.255.254/30", "172.31.255.255/30", "192.168.1.0/30", "192.168.1.3/30",
		"0.0.0.0/30", "0.0.0.1/30", "127.0.0.1/30", "169.254.1.1/30", "224.0.0.1/30", "240.0.0.1/30",
		"100.64.0.1/30", "198.18.0.1/30", "192.0.2.1/30", "8.8.8.9/30", "255.255.255.253/30",
		"::/30", "fd00::1/30", "::ffff:172.31.255.253/126", "::ffff:172.31.255.253/30", "172.031.255.253/30",
		"172.31.255.253%private-token/30", "172.31.255.253/30\n", "172.31.255.253/30;private-token",
	} {
		t.Run(value, func(t *testing.T) {
			in := routedTUNFixture(t)
			in.RoutedTUN.Address = value
			out, err := CompileNative(in)
			if err == nil || len(out.Config) != 0 || !strings.Contains(err.Error(), "address") || strings.Contains(err.Error(), "private-token") {
				t.Fatalf("unsafe address not rejected safely: %v", err)
			}
		})
	}
}

func TestNativeRoutedTUNAcceptsOwnedInterfaceAndPrivateHost(t *testing.T) {
	for _, tc := range []struct{ name, address string }{
		{"b6p-tun", "172.31.255.253/30"},
		{"b6p-12345678901", "10.0.0.1/30"},
		{"b6p-tun_2", "192.168.255.253/30"},
		{"b6p-TUN-2", "172.16.0.1/30"},
	} {
		in := routedTUNFixture(t)
		in.RoutedTUN = &RoutedTUNConfig{InterfaceName: tc.name, Address: tc.address}
		in.ManagementIPs = []string{"10.20.30.1", "172.31.255.249", "2001:db8::1"}
		out, err := CompileNative(in)
		if err != nil {
			t.Fatalf("valid owned interface/address rejected: %v", err)
		}
		listener := maps(decodeConfig(t, out)["inbounds"])[1]
		if listener["interface_name"] != tc.name || !reflect.DeepEqual(listener["address"], []any{tc.address}) {
			t.Fatal("configured TUN interface/host address ignored or masked to its network")
		}
	}
}

func TestNativeRoutedTUNIgnoresUnusedTProxyBindOnly(t *testing.T) {
	in := routedTUNFixture(t)
	in.Ports = Ports{Mixed: 12080, TProxy: 17893, DNS: 11053}
	in.MixedListenAddress, in.DNSListenAddress = "127.0.0.2", "::1"
	out, err := CompileNative(in)
	if err != nil {
		t.Fatal(err)
	}
	in.TProxyListenAddress = "unused-private-token"
	again, err := CompileNative(in)
	if err != nil || !bytes.Equal(out.Config, again.Config) {
		t.Fatal("nonexistent TPROXY listener affected TUN compilation")
	}
	inbounds := maps(decodeConfig(t, out)["inbounds"])
	if inbounds[0]["listen"] != "127.0.0.2" || inbounds[0]["listen_port"] != float64(12080) || inbounds[2]["listen"] != "::1" || inbounds[2]["listen_port"] != float64(11053) {
		t.Fatal("TUN changed actual mixed/DNS listener binds")
	}
	in.Datapath, in.RoutedTUN = DatapathTPROXY, nil
	if _, err := CompileNative(in); err == nil {
		t.Fatal("legacy TPROXY listener address validation removed")
	}
}

func TestNativeRoutedTUNDeterministicWithoutMutation(t *testing.T) {
	in := routedTUNFixture(t)
	in.BootstrapDomains = []string{"b.example", "a.example", "b.example"}
	in.Endpoints = []string{"192.0.2.9", "192.0.2.8", "192.0.2.9"}
	in.LocalDNS = &LocalDNSConfig{Server: "10.20.30.1", Domains: []string{"Office.Home."}, Hostnames: []string{"Router.Office.Home."}}
	configBefore := *in.RoutedTUN
	refsBefore := append([]RuleSetReference{}, in.RuleSets...)
	domainsBefore := append([]string{}, in.BootstrapDomains...)
	first, err := CompileNative(in)
	if err != nil {
		t.Fatal(err)
	}
	second, err := CompileNative(in)
	if err != nil || !bytes.Equal(first.Config, second.Config) || first.SHA256 != second.SHA256 {
		t.Fatal("nondeterministic routed TUN compilation")
	}
	if !reflect.DeepEqual(*in.RoutedTUN, configBefore) || !reflect.DeepEqual(in.RuleSets, refsBefore) || !reflect.DeepEqual(in.BootstrapDomains, domainsBefore) || in.LocalDNS.Domains[0] != "Office.Home." {
		t.Fatal("routed TUN compiler mutated caller configuration")
	}
	in.Datapath, in.IPv6, in.Failure = DatapathRoutedTUN, IPv6Direct, FailureDirect
	in.Endpoints, in.BootstrapDomains = []string{"192.0.2.8", "192.0.2.9"}, []string{"a.example", "b.example"}
	in.RuleSets[0], in.RuleSets[2] = in.RuleSets[2], in.RuleSets[0]
	third, err := CompileNative(in)
	if err != nil || !bytes.Equal(first.Config, third.Config) {
		t.Fatal("equivalent unordered settings or explicit policy defaults changed TUN bytes")
	}
}

func TestNativeRoutedTUNRejectsKnownLiteralPrefixCollisions(t *testing.T) {
	cases := []struct {
		name   string
		change func(*CompileInput, string)
	}{
		{"node", func(in *CompileInput, address string) { in.Node.Server = address }},
		{"endpoint", func(in *CompileInput, address string) { in.Endpoints = []string{address} }},
		{"direct-DNS", func(in *CompileInput, address string) { in.DirectDNS.Server = address }},
		{"proxy-DNS", func(in *CompileInput, address string) { in.ProxyDNS.Server = address }},
		{"mixed-listener", func(in *CompileInput, address string) { in.MixedListenAddress = address }},
		{"DNS-listener", func(in *CompileInput, address string) { in.DNSListenAddress = address }},
		{"common-listener", func(in *CompileInput, address string) { in.ListenAddress = address }},
		{"local-DNS", func(in *CompileInput, address string) {
			in.LocalDNS = &LocalDNSConfig{Server: address}
			in.ManagementIPs = []string{address}
		}},
		{"mapped-DNS", func(in *CompileInput, address string) { in.ProxyDNS.Server = "::ffff:" + address }},
		{"mapped-listener", func(in *CompileInput, address string) { in.MixedListenAddress = "::ffff:" + address }},
	}
	for _, tc := range cases {
		for _, address := range []string{"172.31.255.252", "172.31.255.253", "172.31.255.254", "172.31.255.255"} {
			t.Run(tc.name+"/"+address, func(t *testing.T) {
				in := routedTUNFixture(t)
				tc.change(&in, address)
				out, err := CompileNative(in)
				if err == nil || len(out.Config) != 0 || !strings.Contains(err.Error(), "address prefix overlaps") || strings.Contains(err.Error(), address) {
					t.Fatalf("known literal address colliding with the connected TUN prefix accepted or leaked: %v", err)
				}
				in.Datapath, in.RoutedTUN = DatapathTPROXY, nil
				if _, err := CompileNative(in); err != nil {
					t.Fatalf("TUN-only collision check changed accepted TPROXY configuration: %v", err)
				}
			})
		}
	}
}

func TestNativeRoutedTUNAllowsAdjacentLiteralAddressesAndWildcardBinds(t *testing.T) {
	in := routedTUNFixture(t)
	// Adjacent /30s, public endpoints, unrelated private addresses and wildcard
	// listener semantics must not be mistaken for TUN-prefix collisions.
	in.Node.Server = "172.31.255.251"
	in.Endpoints = []string{"172.31.255.248", "172.31.255.249"}
	in.DirectDNS.Server, in.ProxyDNS.Server = "172.31.255.250", "172.31.255.247"
	in.ManagementIPs = []string{"172.31.255.246"}
	in.LocalDNS = &LocalDNSConfig{Server: "172.31.255.246"}
	in.MixedListenAddress, in.DNSListenAddress = "0.0.0.0", "::"
	if _, err := CompileNative(in); err != nil {
		t.Fatalf("non-colliding known literals or wildcard listener rejected: %v", err)
	}
}

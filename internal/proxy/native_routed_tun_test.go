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

func TestNativeRoutedTUNDefaultsMatchExplicitConfiguration(t *testing.T) {
	in := CompileInput{Node: testNode(t)}
	first, err := CompileNative(in)
	if err != nil {
		t.Fatal(err)
	}
	if in.Datapath != "" || in.RoutedTUN != nil {
		t.Fatal("compiler changed caller datapath or allocated caller configuration")
	}
	for _, tc := range []struct {
		name string
		mode DatapathMode
		tun  *RoutedTUNConfig
	}{
		{"explicit-default", DatapathRoutedTUN, nil},
		{"explicit-config", DatapathRoutedTUN, &RoutedTUNConfig{InterfaceName: "b6p-tun", Address: "172.31.255.253/30"}},
		{"default-with-config", "", &RoutedTUNConfig{InterfaceName: "b6p-tun", Address: "172.31.255.253/30"}},
	} {
		t.Run(tc.name, func(t *testing.T) {
			in.Datapath, in.RoutedTUN = tc.mode, tc.tun
			out, err := CompileNative(in)
			if err != nil {
				t.Fatal(err)
			}
			if !reflect.DeepEqual(first, out) {
				t.Fatal("default and explicit routed TUN differ")
			}
		})
	}
	config := decodeConfig(t, first)
	tun := maps(config["inbounds"])[1]
	if tun["type"] != "tun" || tun["interface_name"] != "b6p-tun" || !reflect.DeepEqual(tun["address"], []any{"172.31.255.253/30"}) {
		t.Fatal("default must be the managed routed-TUN main path")
	}
	if in.IPv6 != "" || in.Failure != "" || in.Ports != (Ports{}) {
		t.Fatal("compiler defaults mutated caller input")
	}
	in.Ports.TProxy = 65535
	again, err := CompileNative(in)
	if err != nil || !reflect.DeepEqual(first, again) {
		t.Fatalf("unused TPROXY port changed default listener configuration: %v", err)
	}
}

func TestNativeRoutedTUNRejectsRetiredTPROXYBackend(t *testing.T) {
	for _, config := range []*RoutedTUNConfig{nil, {InterfaceName: "b6p-tun", Address: "172.31.255.253/30"}} {
		out, err := CompileNative(CompileInput{Node: testNode(t), Datapath: DatapathTPROXY, RoutedTUN: config})
		if err == nil || len(out.Config) != 0 || !strings.Contains(err.Error(), "tproxy datapath is no longer supported") {
			t.Fatalf("retired unsafe backend must not compile: %v", err)
		}
	}
}

func TestNativeRoutedTUNValidatesOnlyActualListenerPorts(t *testing.T) {
	in := CompileInput{Node: testNode(t), Ports: Ports{Mixed: 2080, DNS: 1053}}
	baseline, err := CompileNative(in)
	if err != nil {
		t.Fatal(err)
	}
	for _, unused := range []uint16{0, 7893, 2080, 1053, 53, 65535} {
		in.Ports.TProxy = unused
		out, err := CompileNative(in)
		if err != nil || !reflect.DeepEqual(baseline, out) {
			t.Fatalf("unused TPROXY port affected compilation: %v", err)
		}
		if in.Ports.TProxy != unused {
			t.Fatal("unused port normalization mutated caller input")
		}
	}
	for _, ports := range []Ports{{Mixed: 7893, DNS: 1053}, {Mixed: 2080, DNS: 7893}} {
		in.Ports = ports
		if _, err := CompileNative(in); err != nil {
			t.Fatalf("unused reserved port conflicts with an actual listener: %v", err)
		}
	}
	in.Ports, in.LocalDNS = Ports{Mixed: 2080, DNS: 1053}, &LocalDNSConfig{Port: 7893}
	if _, err := CompileNative(in); err != nil {
		t.Fatalf("unused port must not reserve a nonexistent core listener: %v", err)
	}
	in.Ports.Mixed = 7893
	if _, err := CompileNative(in); err == nil || !strings.Contains(err.Error(), "core listener port") {
		t.Fatalf("local DNS may not loop into an actual listener on 7893: %v", err)
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
		defaultInput := in
		defaultInput.Datapath, defaultInput.RoutedTUN = "", nil
		defaultOutput, err := CompileNative(defaultInput)
		if err != nil {
			t.Fatal(err)
		}
		if !reflect.DeepEqual(out, defaultOutput) {
			t.Fatal("explicit routed TUN changed current default policy or metadata")
		}
		config := decodeConfig(t, out)
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
		}
		if clientDNS != 1 || sniff != 1 {
			t.Fatal("routed TUN must have one client DNS hijack and one sniff action")
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
		{"tproxy-with-config", func(in *CompileInput) { in.Datapath = DatapathTPROXY }, "tproxy datapath is no longer supported"},
		{"empty-config", func(in *CompileInput) { in.RoutedTUN = &RoutedTUNConfig{} }, "interface"},
		{"ipv6-follow", func(in *CompileInput) { in.IPv6 = IPv6Follow }, "routed-tun supports only IPv6 direct"},
		{"ipv6-block", func(in *CompileInput) { in.IPv6 = IPv6Block }, "routed-tun supports only IPv6 direct"},
		{"management-host", func(in *CompileInput) { in.ManagementIPs = []string{"172.31.255.253"} }, "overlaps router management"},
		{"management-peer", func(in *CompileInput) { in.ManagementIPs = []string{"172.31.255.254"} }, "overlaps router management"},
		{"management-network", func(in *CompileInput) { in.ManagementIPs = []string{"172.31.255.252"} }, "overlaps router management"},
		{"management-broadcast", func(in *CompileInput) { in.ManagementIPs = []string{"172.31.255.255"} }, "overlaps router management"},
		{"mixed-zero", func(in *CompileInput) { in.Ports = Ports{Mixed: 0, TProxy: 7893, DNS: 1053} }, "listener ports"},
		{"dns-zero", func(in *CompileInput) { in.Ports = Ports{Mixed: 2080, TProxy: 7893, DNS: 0} }, "listener ports"},
		{"duplicate-mixed-DNS", func(in *CompileInput) { in.Ports = Ports{Mixed: 2080, TProxy: 7893, DNS: 2080} }, "listener ports"},
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
		in.Datapath = ""
		defaultOutput, err := CompileNative(in)
		if err != nil || !reflect.DeepEqual(out, defaultOutput) {
			t.Fatalf("default datapath ignored explicit TUN configuration: %v", err)
		}
	}
}

func TestNativeRoutedTUNIgnoresUnusedTProxyBind(t *testing.T) {
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
	if _, err := CompileNative(in); err == nil || !strings.Contains(err.Error(), "tproxy datapath is no longer supported") {
		t.Fatalf("retired backend accepted or rejected for the wrong reason: %v", err)
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
				in.Datapath, in.RoutedTUN = "", nil
				if _, err := CompileNative(in); err == nil || !strings.Contains(err.Error(), "address prefix overlaps") {
					t.Fatalf("default routed TUN missed a known prefix collision: %v", err)
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

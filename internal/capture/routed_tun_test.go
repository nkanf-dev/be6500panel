package capture

import (
	"context"
	"encoding/json"
	"errors"
	"net/netip"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"strings"
	"testing"

	"be6500panel/internal/proxy"
	"be6500panel/internal/router"
)

const acceptedRoutedTUN = `{"inbounds":[{"type":"mixed","tag":"mixed-in","listen":"192.0.2.1","listen_port":2081},{"type":"tun","tag":"tun-in","interface_name":"b6p-tun","address":["172.31.255.253/30"],"mtu":1500,"dns_mode":"disabled","auto_route":false,"auto_redirect":false,"stack":"system","udp_timeout":"2m","udp_nat_max":1024},{"type":"direct","tag":"dns-in","listen":"192.0.2.1","listen_port":1054}],"outbounds":[{"type":"vless","server":"node.example"}],"dns":{"servers":[{"type":"tls","server":"203.0.113.53","detour":"direct"},{"type":"udp","server":"127.0.0.1","detour":"direct"},{"type":"tls","server":"1.1.1.1","detour":"proxy"},{"type":"fakeip"}]},"route":{"rules":[{"inbound":["dns-in"],"action":"hijack-dns"},{"ip_version":6,"outbound":"direct"}]}}`

// This fixture is accepted-config intent, not a native compile bypass. All
// kernel, command and endpoint observations below are fake and offline.
func routedTUNInput() proxy.RulesPlanInput {
	in := testInput()
	in.Datapath, in.TUNInterface, in.TUNAddress = proxy.DatapathRoutedTUN, "b6p-tun", "172.31.255.253/30"
	in.ClientMACs = map[string]string{in.ClientIPv4: "02:00:00:00:00:10"}
	return in
}

func routedTUNPlan(t *testing.T) proxy.OwnedRulesPlan {
	t.Helper()
	plan, err := proxy.PlanOwnedRules(routedTUNInput())
	if err != nil {
		t.Fatal(err)
	}
	if plan.Ownership.Datapath != proxy.DatapathRoutedTUN || !reflect.DeepEqual(plan.Ownership.RouteFamilies, []int{4}) || len(plan.Ownership.Chains) != 6 {
		t.Fatalf("routed TUN planner dispatch missing: %+v", plan.Ownership)
	}
	return plan
}

func tunAddressFixture() []byte {
	return []byte(`[{"ifname":"b6p-tun","flags":["POINTOPOINT","MULTICAST","NOARP","UP","LOWER_UP"],"mtu":1500,"addr_info":[{"family":"inet","local":"172.31.255.253","prefixlen":30}]}]`)
}

const tunFactoryRoutes = "default via 192.0.2.1 dev eth0 proto static\n192.0.2.0/24 dev br-lan proto kernel scope link src 192.0.2.1\n172.31.255.252/30 dev b6p-tun proto kernel scope link src 172.31.255.253\nlocal 172.31.255.253 dev b6p-tun table local proto kernel scope host src 172.31.255.253\nbroadcast 172.31.255.252 dev b6p-tun table local proto kernel scope link src 172.31.255.253\nbroadcast 172.31.255.255 dev b6p-tun table local proto kernel scope link src 172.31.255.253\n"

func tunIdleRunner(ctx context.Context, argv []string) ([]byte, error) {
	if slices.Equal(argv, tunAddressShow("b6p-tun")) {
		return tunAddressFixture(), nil
	}
	if slices.Equal(argv, tunRPFilterShow("b6p-tun")) {
		return []byte("2\n"), nil
	}
	if slices.Equal(argv, []string{"ip", "-4", "route", "show", "table", "all"}) {
		return []byte(tunFactoryRoutes), nil
	}
	return idleRunner(ctx, argv)
}

func TestAcceptedRoutedTUNUsesActualBackendAndObservedIdentity(t *testing.T) {
	observed := deviceObservation()
	observed.LANAddresses = append(observed.LANAddresses, "fe80::1", "fd00::1")
	observed.ManagementIPs = append(observed.ManagementIPs, "fe80::1", "fd00::1")
	input, clients, err := BuildFromAccepted(context.Background(), desiredDevices(), []byte(acceptedRoutedTUN), observed, fakeResolve)
	if err != nil {
		t.Fatal(err)
	}
	if input.Datapath != proxy.DatapathRoutedTUN || input.TUNInterface != "b6p-tun" || input.TUNAddress != "172.31.255.253/30" || input.Ports != (proxy.Ports{Mixed: 2081, TProxy: 7893, DNS: 1054}) {
		t.Fatalf("accepted TUN extraction mismatch: %+v", input)
	}
	if !input.FakeIP || !reflect.DeepEqual(input.EndpointIPs, []string{"127.0.0.1", "203.0.113.4", "203.0.113.53"}) || !reflect.DeepEqual(input.RouterDNSAddresses, []string{"192.0.2.1"}) || !reflect.DeepEqual(input.ManagementIPs, observed.ManagementIPs) {
		t.Fatalf("accepted DNS/endpoints/local authority changed: %+v", input)
	}
	if !reflect.DeepEqual(input.ClientIPv4s, []string{"192.0.2.10", "192.0.2.11"}) || !reflect.DeepEqual(input.ClientMACs, map[string]string{"192.0.2.10": "02:00:00:00:00:10", "192.0.2.11": "02:00:00:00:00:11"}) || clients[0].Hostname != "mac" {
		t.Fatalf("observed scope mismatch: %+v %v", input, clients)
	}
	plan, err := proxy.PlanOwnedRules(input)
	if err != nil || plan.Ownership.Datapath != proxy.DatapathRoutedTUN {
		t.Fatalf("actual backend did not reach planner: %+v %v", plan.Ownership, err)
	}
	legacy, _, err := BuildFromAccepted(context.Background(), desiredDevices(), []byte(acceptedNative), observed, fakeResolve)
	if err != nil || legacy.Datapath != "" || legacy.TUNInterface != "" || legacy.TUNAddress != "" || legacy.Ports.TProxy != 7894 {
		t.Fatalf("legacy extraction migrated backend: %+v %v", legacy, err)
	}
}

func TestAcceptedRoutedTUNRejectsUnsafeNativeFields(t *testing.T) {
	for _, tc := range []struct{ name, old, replacement string }{
		{"wrong-tag", `"tag":"tun-in"`, `"tag":"foreign"`},
		{"missing-tag", `"tag":"tun-in",`, ``},
		{"unsafe-interface", `"interface_name":"b6p-tun"`, `"interface_name":"br-lan"`},
		{"alias-interface", `"interface_name":"b6p-tun"`, `"interface_name":"b6p-tun:1"`},
		{"wildcard-interface", `"interface_name":"b6p-tun"`, `"interface_name":"b6p-+"`},
		{"auto-route", `"auto_route":false`, `"auto_route":true`},
		{"missing-auto-route", `"auto_route":false,`, ``},
		{"null-auto-route", `"auto_route":false`, `"auto_route":null`},
		{"auto-redirect", `"auto_redirect":false`, `"auto_redirect":true`},
		{"missing-auto-redirect", `"auto_redirect":false,`, ``},
		{"unknown-auto-field", `"auto_route":false`, `"auto_route":false,"auto_redirect_input_mark":16384`},
		{"route-address", `"auto_route":false`, `"auto_route":false,"route_address":["0.0.0.0/0"]`},
		{"unknown-field", `"stack":"system"`, `"stack":"system","platform":{}`},
		{"unknown-netns", `"stack":"system"`, `"stack":"system","netns":""`},
		{"unknown-uid", `"stack":"system"`, `"stack":"system","include_uid":[]`},
		{"unknown-mapping", `"stack":"system"`, `"stack":"system","udp_mapping":"endpoint-independent"`},
		{"gvisor", `"stack":"system"`, `"stack":"gvisor"`},
		{"dns-enabled", `"dns_mode":"disabled"`, `"dns_mode":"native"`},
		{"wrong-mtu", `"mtu":1500`, `"mtu":9000`},
		{"unbounded-udp", `"udp_timeout":"2m"`, `"udp_timeout":"24h"`},
		{"wrong-udp-capacity", `"udp_nat_max":1024`, `"udp_nat_max":0`},
		{"multi-address", `"address":["172.31.255.253/30"]`, `"address":["172.31.255.253/30","172.31.255.249/30"]`},
		{"ipv6-address", `"address":["172.31.255.253/30"]`, `"address":["fd00::1/126"]`},
		{"network-address", `172.31.255.253/30`, `172.31.255.252/30`},
		{"second-host", `172.31.255.253/30`, `172.31.255.254/30`},
		{"broadcast-address", `172.31.255.253/30`, `172.31.255.255/30`},
		{"wrong-prefix", `172.31.255.253/30`, `172.31.255.253/29`},
		{"public-prefix", `172.31.255.253/30`, `203.0.113.253/30`},
		{"missing-dns-route", `"action":"hijack-dns"`, `"action":"route"`},
		{"missing-explicit-ipv6", `,{"ip_version":6,"outbound":"direct"}`, ``},
		{"scoped-ipv6-direct", `"ip_version":6,"outbound":"direct"`, `"ip_version":6,"ip_cidr":["2001:db8::/32"],"outbound":"direct"`},
		{"reject-ipv6", `"ip_version":6,"outbound":"direct"`, `"ip_version":6,"action":"reject"`},
		{"loopback-dns", `"listen":"192.0.2.1","listen_port":1054`, `"listen":"127.0.0.1","listen_port":1054`},
		{"reserved-port-conflict", `"listen_port":1054`, `"listen_port":7893`},
		{"mixed-dns-port-conflict", `"listen_port":1054`, `"listen_port":2081`},
		{"unknown-datapath", `"type":"tun"`, `"type":"unknown"`},
		{"mixed-bind-in-private-prefix", `"listen":"192.0.2.1","listen_port":2081`, `"listen":"172.31.255.254","listen_port":2081`},
		{"proxy-dns-in-private-prefix", `"server":"1.1.1.1"`, `"server":"172.31.255.254"`},
	} {
		t.Run(tc.name, func(t *testing.T) {
			raw := strings.Replace(acceptedRoutedTUN, tc.old, tc.replacement, 1)
			if raw == acceptedRoutedTUN {
				t.Fatal("fixture replacement missed")
			}
			if _, _, err := BuildFromAccepted(context.Background(), desiredDevices(), []byte(raw), deviceObservation(), fakeResolve); err == nil {
				t.Fatal("unsafe native TUN accepted")
			}
		})
	}
	for _, before := range []bool{true, false} {
		tproxy := `{"type":"tproxy","listen":"127.0.0.1","listen_port":7894},`
		needle := `{"type":"tun"`
		if !before {
			needle = `{"type":"direct"`
		}
		raw := strings.Replace(acceptedRoutedTUN, needle, tproxy+needle, 1)
		if _, _, err := BuildFromAccepted(context.Background(), desiredDevices(), []byte(raw), deviceObservation(), fakeResolve); err == nil || err.Error() != "capture_datapath_ambiguous" {
			t.Fatalf("both TPROXY/TUN accepted (before=%v): %v", before, err)
		}
	}
}

func TestAcceptedRoutedTUNRejectsUnsupportedFamilyAndWholePrefixCollision(t *testing.T) {
	for _, mode := range []proxy.IPv6Mode{proxy.IPv6Follow, proxy.IPv6Block} {
		d := desiredDevices()
		d.Devices, d.IPv6, d.ClientIPv6 = d.Devices[:1], mode, "2001:db8::10"
		if _, _, err := BuildFromAccepted(context.Background(), d, []byte(acceptedRoutedTUN), deviceObservation(), fakeResolve); err == nil || err.Error() != "capture_tun_ipv6_unsupported" {
			t.Fatalf("unqualified IPv6 accepted: %v", err)
		}
	}
	for _, tc := range []struct{ lan, management string }{
		{lan: "172.16.0.0/12"}, {lan: "172.31.255.252/30"}, {lan: "172.31.255.254/31"}, {lan: "172.31.255.255/32"},
		{management: "172.31.255.252"}, {management: "172.31.255.254"}, {management: "172.31.255.255"},
	} {
		observed := deviceObservation()
		if tc.lan != "" {
			observed.LANPrefixes = append(observed.LANPrefixes, tc.lan)
		}
		if tc.management != "" {
			observed.ManagementIPs = append(observed.ManagementIPs, tc.management)
		}
		if _, _, err := BuildFromAccepted(context.Background(), desiredDevices(), []byte(acceptedRoutedTUN), observed, fakeResolve); err == nil || err.Error() != "capture_tun_prefix_collision" {
			t.Fatalf("whole prefix collision accepted: %+v %v", tc, err)
		}
	}
}

func TestAcceptedRoutedTUNKeepsDeviceEligibilityAndPendingIdentity(t *testing.T) {
	observed := deviceObservation()
	observed.Devices[0].Eligible = false
	input, clients, err := BuildFromAccepted(context.Background(), desiredDevices(), []byte(acceptedRoutedTUN), observed, fakeResolve)
	var partial *PartialScopeError
	if !errors.As(err, &partial) || input.ClientIPv4 != "192.0.2.11" || clients[0].IP != "" || clients[0].MAC != "02:00:00:00:00:10" || len(input.ClientMACs) != 1 {
		t.Fatalf("stale identity substituted: %+v %+v %v", input, clients, err)
	}
	observed.Devices[1].IP = "198.51.100.10"
	if _, _, err = BuildFromAccepted(context.Background(), desiredDevices(), []byte(acceptedRoutedTUN), observed, fakeResolve); err == nil {
		t.Fatal("upstream IP accepted")
	}
	observed = deviceObservation()
	observed.Devices = append(observed.Devices, observed.Devices[0])
	input, clients, err = BuildFromAccepted(context.Background(), desiredDevices(), []byte(acceptedRoutedTUN), observed, fakeResolve)
	if !errors.As(err, &partial) || input.ClientIPv4 != "192.0.2.11" || clients[0].IP != "" {
		t.Fatal("ambiguous device admitted", input, clients, err)
	}
}

func TestRoutedTUNFixedReadCommandsAndCompiledIntent(t *testing.T) {
	plan := routedTUNPlan(t)
	c := testController(t, tunIdleRunner)
	c.plan = &plan
	for _, command := range append(plan.Apply, plan.Cleanup...) {
		if err := c.approvedCommand(command); err != nil {
			t.Fatalf("compiled command rejected: %v: %v", command, err)
		}
	}
	for _, argv := range [][]string{tunAddressShow("b6p-tun"), tunRPFilterShow("b6p-tun")} {
		if validate(argv) != nil || !isReadCommand(argv) {
			t.Fatalf("fixed TUN read rejected: %v", argv)
		}
	}
	for _, argv := range [][]string{
		{"cat", "/proc/1/fdinfo/1"}, {"sysctl", "-w", "net.ipv4.conf.b6p-tun.rp_filter=2"},
		{"sysctl", "-n", "net.ipv4.conf.br-lan.rp_filter"}, {"sysctl", "-n", "net.ipv4.conf.b6p-tun.rp_filter", "kernel.hostname"},
		tunAddressShow("br-lan"), tunAddressShow("b6p-tun+"), tunAddressShow("b6p-tun:1"),
		{"ip", "-j", "-4", "address", "add", "172.31.255.253/30", "dev", "b6p-tun"},
		{"iptables", "-w", "5", "-t", "filter", "-S", "FORWARD"},
		{"iptables", "-w", "5", "-t", "filter", "-S", "B6P_V4_TUN_FOREIGN"},
		{"iptables", "-w", "5", "-t", "filter", "-F", "B6P_V4_TUN_FORWARD"},
	} {
		if validate(argv) == nil {
			t.Fatalf("arbitrary command treated as inspection: %v", argv)
		}
	}
}

func TestRoutedTUNPreflightAllowsCoreInterfaceAndRejectsStateDrift(t *testing.T) {
	for _, tc := range []struct {
		name, address, rpf, extraRoute string
		failure                        error
		wantError                      bool
	}{
		{name: "owned-core-interface", address: string(tunAddressFixture()), rpf: "2"},
		{name: "interface-absent", address: `[]`, rpf: "2", wantError: true},
		{name: "wrong-interface", address: strings.Replace(string(tunAddressFixture()), `"b6p-tun"`, `"b6p-other"`, 1), rpf: "2", wantError: true},
		{name: "wrong-address", address: strings.Replace(string(tunAddressFixture()), `172.31.255.253`, `172.31.255.254`, 1), rpf: "2", wantError: true},
		{name: "down", address: strings.Replace(string(tunAddressFixture()), `"UP",`, ``, 1), rpf: "2", wantError: true},
		{name: "wrong-mask", address: strings.Replace(string(tunAddressFixture()), `"prefixlen":30`, `"prefixlen":24`, 1), rpf: "2", wantError: true},
		{name: "wrong-mtu", address: strings.Replace(string(tunAddressFixture()), `"mtu":1500`, `"mtu":1400`, 1), rpf: "2", wantError: true},
		{name: "foreign-private-route", address: string(tunAddressFixture()), rpf: "2", extraRoute: "172.16.0.0/12 dev br-lan proto kernel scope link", wantError: true},
		{name: "peer-route-other-interface", address: string(tunAddressFixture()), rpf: "2", extraRoute: "172.31.255.254 dev eth0 proto static scope link", wantError: true},
		{name: "own-device-broad-route", address: string(tunAddressFixture()), rpf: "2", extraRoute: "172.31.0.0/16 dev b6p-tun proto kernel scope link", wantError: true},
		{name: "strict-rpf", address: string(tunAddressFixture()), rpf: "1", wantError: true},
		{name: "disabled-rpf", address: string(tunAddressFixture()), rpf: "0", wantError: true},
		{name: "read-refused", failure: errors.New("permission denied"), wantError: true},
	} {
		t.Run(tc.name, func(t *testing.T) {
			mutations := 0
			c := testController(t, func(ctx context.Context, argv []string) ([]byte, error) {
				if slices.Equal(argv, tunAddressShow("b6p-tun")) {
					return []byte(tc.address), tc.failure
				}
				if slices.Equal(argv, tunRPFilterShow("b6p-tun")) {
					return []byte(tc.rpf), nil
				}
				if slices.Equal(argv, []string{"ip", "-4", "route", "show", "table", "all"}) {
					return []byte(tunFactoryRoutes + tc.extraRoute), nil
				}
				if !isReadCommand(argv) {
					mutations++
				}
				return idleRunner(ctx, argv)
			})
			_, err := c.Apply(context.Background(), routedTUNInput())
			if (err != nil) != tc.wantError {
				t.Fatalf("preflight=%v", err)
			}
			if tc.wantError && (mutations != 0 || c.Status().Commands != 0) {
				t.Fatal("mutated after failed interface preflight")
			}
		})
	}
}

func TestRoutedTUNPreflightStillRejectsOccupiedMarkTableAndChains(t *testing.T) {
	for _, tc := range []struct{ name, route, rule, mangle, chain string }{
		{name: "ordinary-table", route: "default dev b6p-other"},
		{name: "legacy-local-table", route: "local default dev lo"},
		{name: "foreign-named-table", rule: "12000: from all lookup capture"},
		{name: "connmark", mangle: "-A PREROUTING -j CONNMARK --restore-mark --nfmask 0xffffffff --ctmask 0xffffffff"},
		{name: "occupied-private-chain", chain: "B6P_V4_TUN_INPUT"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			mutations := 0
			c := testController(t, func(ctx context.Context, argv []string) ([]byte, error) {
				if slices.Equal(argv, routeShow(4)) {
					return []byte(tc.route), nil
				}
				if slices.Equal(argv, ruleShow(4)) {
					return []byte(tc.rule), nil
				}
				if len(argv) == 6 && argv[5] == "-S" {
					return []byte(tc.mangle), nil
				}
				if len(argv) == 7 && argv[5] == "-S" && argv[6] == tc.chain {
					return nil, nil
				}
				if !isReadCommand(argv) {
					mutations++
				}
				return tunIdleRunner(ctx, argv)
			})
			if _, err := c.Apply(context.Background(), routedTUNInput()); err == nil || mutations != 0 {
				t.Fatalf("occupied TUN resource admitted: %v mutations=%d", err, mutations)
			}
		})
	}
}

func TestTUNRouteCollisionReadRejectsMalformedAndForeignAllocation(t *testing.T) {
	own := routedTUNPlan(t).Ownership
	prefix := netip.MustParsePrefix("172.31.255.252/30")
	for _, line := range []string{
		"Dump terminated", "172.31.255.252/30 dev", "172.31.255.252/30 dev br-lan proto kernel scope link",
		"local 172.31.255.254 dev b6p-tun table local proto kernel scope host", "172.31.255.252/30 dev b6p-tun proto static scope link",
		"172.31.255.252/30 via 172.31.255.254 dev b6p-tun proto kernel scope link", "172.31.255.252/30 dev b6p-tun table 100 proto kernel scope link",
		"default dev b6p-tun table main",
	} {
		if err := tunRouteCollisions([]byte(line), prefix, own); err == nil {
			t.Fatalf("foreign route accepted: %s", line)
		}
	}
	if err := tunRouteCollisions([]byte(tunFactoryRoutes), prefix, own); err != nil {
		t.Fatal(err)
	}
}

func TestRoutedTUNJournalRecompilesInputAndOwnershipFallbackNeverStoredApply(t *testing.T) {
	in := routedTUNInput()
	in.ManagementIPs, in.RouterDNSAddresses = []string{"192.0.2.1"}, []string{"192.0.2.1"}
	in.FakeIP = true
	plan, err := proxy.PlanOwnedRules(in)
	if err != nil {
		t.Fatal(err)
	}
	for _, withInput := range []bool{true, false} {
		t.Run(map[bool]string{true: "with-input", false: "ownership-fallback"}[withInput], func(t *testing.T) {
			stored := journal{OwnedRulesPlan: clonePlan(plan)}
			stored.Apply = [][]string{{"sh", "-c", "do-not-replay"}}
			if withInput {
				stored.Input = &in
			}
			raw, err := json.Marshal(stored)
			if err != nil {
				t.Fatal(err)
			}
			dir := t.TempDir()
			if err = os.WriteFile(filepath.Join(dir, "capture-journal.json"), raw, 0600); err != nil {
				t.Fatal(err)
			}
			var calls [][]string
			c, err := New(dir, func(_ context.Context, argv []string) ([]byte, error) {
				calls = append(calls, slices.Clone(argv))
				return nil, nil
			})
			if err != nil {
				t.Fatal(err)
			}
			if len(calls) != 0 || c.Status().Active || !c.Status().CleanupPending || c.plan.Ownership.Datapath != proxy.DatapathRoutedTUN {
				t.Fatal("journal inferred activity or executed")
			}
			if len(c.plan.Apply) == 0 || c.plan.Apply[0][0] == "sh" || !reflect.DeepEqual(c.plan.Cleanup, plan.Cleanup) || !reflect.DeepEqual(c.plan.Ownership, plan.Ownership) {
				t.Fatal("journal not recompiled")
			}
			if withInput && !reflect.DeepEqual(*c.plan, plan) {
				t.Fatal("journal lost TUN input")
			}
			if err = c.Cleanup(context.Background()); err != nil {
				t.Fatal(err)
			}
			if !reflect.DeepEqual(calls, plan.Cleanup) {
				t.Fatal("cleanup did not use exact compiled intent", calls)
			}
			for _, argv := range calls {
				if slices.Contains(argv, "link") || argv[0] == "sysctl" {
					t.Fatal("cleanup changed core interface", argv)
				}
			}
		})
	}
}

func TestRoutedTUNJournalRejectsBackendInterfaceAddressAndForeignCleanup(t *testing.T) {
	for _, mode := range []string{"datapath", "interface", "address", "ownership-interface", "ownership-address", "cleanup", "missing-mac", "ipv6", "route-family"} {
		t.Run(mode, func(t *testing.T) {
			in := routedTUNInput()
			stored := journal{OwnedRulesPlan: routedTUNPlan(t), Input: &in}
			switch mode {
			case "datapath":
				in.Datapath = "unknown"
			case "interface":
				in.TUNInterface = "b6p-other"
			case "address":
				in.TUNAddress = "172.31.255.249/30"
			case "ownership-interface":
				stored.Ownership.TUNInterface = "b6p-other"
			case "ownership-address":
				stored.Ownership.TUNAddress = "172.31.255.249/30"
			case "cleanup":
				stored.Cleanup[0] = []string{"ip", "link", "delete", "b6p-tun"}
			case "missing-mac":
				in.ClientMACs = nil
			case "ipv6":
				in.IPv6, in.ClientIPv6 = proxy.IPv6Follow, "2001:db8::10"
			case "route-family":
				stored.Ownership.RouteFamilies = []int{4, 6}
			}
			if _, err := recoveredPlan(stored); err == nil {
				t.Fatal("unsupported or mismatched TUN journal accepted")
			}
		})
	}
}

func tunObservedRunner(t *testing.T, plan proxy.OwnedRulesPlan, route string, missing string) Runner {
	t.Helper()
	return func(_ context.Context, argv []string) ([]byte, error) {
		if !isReadCommand(argv) {
			t.Fatalf("observation mutated: %v", argv)
		}
		if slices.Equal(argv, tunAddressShow("b6p-tun")) {
			return tunAddressFixture(), nil
		}
		if slices.Equal(argv, tunRPFilterShow("b6p-tun")) {
			return []byte("2"), nil
		}
		if slices.Equal(argv, routeShow(4)) {
			return []byte(route), nil
		}
		if slices.Equal(argv, ruleShow(4)) {
			return []byte("16500: from 192.0.2.10 iif br-lan fwmark 0x4000/0x4000 lookup capture\n"), nil
		}
		if len(argv) > 6 && (argv[5] == "-S" || argv[5] == "-C") {
			if argv[6] == missing || (argv[5] == "-C" && slices.Contains(argv, missing)) {
				return []byte("iptables: Bad rule (does a matching rule exist in that chain?)."), errors.New("missing")
			}
			if argv[5] == "-S" {
				lines := []string{"-N " + argv[6]}
				for _, command := range plan.Apply {
					if len(command) > 6 && command[4] == argv[4] && command[5] == "-A" && command[6] == argv[6] {
						lines = append(lines, strings.Join(command[5:], " "))
					}
				}
				return []byte(strings.Join(lines, "\n")), nil
			}
			return nil, nil
		}
		t.Fatalf("unapproved observed argv: %v", argv)
		return nil, nil
	}
}

func TestRoutedTUNObservationRequiresOrdinaryOwnedDefaultAndExactHooks(t *testing.T) {
	plan := routedTUNPlan(t)
	for _, tc := range []struct {
		name, route, missing string
		want                 bool
	}{
		{name: "ordinary", route: "default dev b6p-tun scope link", want: true},
		{name: "prefix-spelling", route: "0.0.0.0/0 dev b6p-tun proto boot scope link", want: true},
		{name: "missing-route"}, {name: "legacy-local", route: "local default dev lo scope host"},
		{name: "local-tun", route: "local default dev b6p-tun scope host"}, {name: "wrong-interface", route: "default dev b6p-other"},
		{name: "via", route: "default via 172.31.255.254 dev b6p-tun"}, {name: "link-down", route: "default dev b6p-tun linkdown"},
		{name: "extra-route", route: "default dev b6p-tun\n203.0.113.0/24 dev b6p-other"},
		{name: "missing-private-rule", route: "default dev b6p-tun", missing: "B6P_V4_TUN_INPUT"},
		{name: "missing-mark-hook", route: "default dev b6p-tun", missing: "B6P_V4_TUN_MARK"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			c := testController(t, tunObservedRunner(t, plan, tc.route, tc.missing))
			c.plan, c.active = &plan, true
			state, err := c.Reconcile(context.Background())
			if state.Active != tc.want || (err == nil) != tc.want || state.CleanupPending == tc.want {
				t.Fatalf("observation=%+v %v", state, err)
			}
		})
	}
}

func TestRoutedTUNGETScopeChangeNeverMigratesAcceptedBackend(t *testing.T) {
	c := testController(t, tunIdleRunner)
	observed := deviceObservation()
	d := desiredDevices()
	d.Devices = d.Devices[:1]
	raw := acceptedRoutedTUN
	c.SetBuilder(func(ctx context.Context, d Desired) (proxy.RulesPlanInput, []Client, error) {
		return BuildFromAccepted(ctx, d, []byte(raw), observed, fakeResolve)
	})
	if _, err := c.Select(context.Background(), d); err != nil {
		t.Fatal(err)
	}
	before, err := os.ReadFile(c.path)
	if err != nil {
		t.Fatal(err)
	}
	plan := clonePlan(*c.plan)
	c.runner = tunObservedRunner(t, plan, "default dev b6p-tun", "")
	raw = acceptedNative
	state, err := c.ReconcileDesired(context.Background())
	if err == nil || !state.Active || state.ScopeState != "changed" || state.Error != "capture_scope_changed_apply_required" || c.plan.Ownership.Datapath != proxy.DatapathRoutedTUN {
		t.Fatalf("GET silently migrated: %+v %v", state, err)
	}
	after, err := os.ReadFile(c.path)
	if err != nil || !slices.Equal(before, after) {
		t.Fatal("GET changed journal", err)
	}
}

func TestRoutedTUNDiagnosticsReadOnlyStagesAndRouteEvidence(t *testing.T) {
	plan := routedTUNPlan(t)
	for _, route := range []string{"default dev b6p-tun", "local default dev lo scope host", "default dev b6p-other", ""} {
		var calls [][]string
		c := testController(t, func(_ context.Context, argv []string) ([]byte, error) {
			calls = append(calls, slices.Clone(argv))
			if !diagnosticReadAllowed(argv) {
				t.Fatalf("unsafe diagnostics command: %v", argv)
			}
			if slices.Equal(argv, routeShow(4)) {
				return []byte(route), nil
			}
			if slices.Equal(argv, ruleShow(4)) {
				return []byte("16500: from 192.0.2.10 iif br-lan fwmark 0x4000/0x4000 lookup 16500"), nil
			}
			target := "ACCEPT"
			if argv[6] == "B6P_V4_TUN_MARK" {
				target = "MARK"
			}
			if argv[6] == "B6P_V4_DNS" {
				target = "REDIRECT"
			}
			return diagnosticFixture(argv[6], "12 900 "+target+" tcp -- * * 192.0.2.10 0.0.0.0/0"), nil
		})
		c.plan, c.active = &plan, true
		before := c.Status()
		report := c.Diagnostics(context.Background())
		if len(calls) != 8 || len(report.Chains) != 6 || !reflect.DeepEqual(before, c.Status()) {
			t.Fatalf("diagnostics incomplete or mutable: %+v", report)
		}
		present := route == "default dev b6p-tun"
		if len(report.Routing) != 1 || report.Routing[0].LocalRoute.Present == nil || *report.Routing[0].LocalRoute.Present != present {
			t.Fatalf("wrong route evidence: %+v", report.Routing)
		}
		for _, entry := range report.Chains {
			if entry.State != "ok" || len(entry.Counters) != 1 {
				t.Fatalf("missing TUN counters: %+v", entry)
			}
			if entry.Chain == "B6P_V4_TUN_MARK" && (entry.Role != "original-ingress" || entry.Counters[0].Target != "MARK" || entry.Counters[0].ListenerPort != nil) {
				t.Fatalf("ingress claimed listener delivery: %+v", entry)
			}
		}
	}
}

func TestRoutedTUNDiagnosticReadFailuresRemainUnavailable(t *testing.T) {
	plan := routedTUNPlan(t)
	c := testController(t, func(context.Context, []string) ([]byte, error) {
		return []byte("permission denied"), errors.New("refused")
	})
	c.plan = &plan
	report := c.Diagnostics(context.Background())
	if report.State != "partial" || report.Routing[0].LocalRoute.Present != nil || report.Routing[0].PolicyRules.Present != nil {
		t.Fatal("failed read became false proof", report)
	}
	for _, entry := range report.Chains {
		if entry.Counters != nil || entry.State != "unavailable" {
			t.Fatalf("failed counters guessed: %+v", entry)
		}
	}
	for _, chain := range []proxy.OwnedChain{{Family: 4, Table: "filter", Name: "FORWARD"}, {Family: 6, Table: "filter", Name: "B6P_V6_BLOCK"}, {Family: 4, Table: "filter", Name: "B6P_V4_TUN_FOREIGN"}} {
		if diagnosticOwnedChain(chain) || diagnosticReadAllowed(diagnosticChainArgs(chain)) {
			t.Fatal("foreign diagnostic chain allowed", chain)
		}
	}
}

func TestAcceptedRoutedTUNResolverFailuresAndObservedScopeRemainFailClosed(t *testing.T) {
	for _, resolver := range []EndpointResolver{nil, func(context.Context, string) ([]string, error) { return nil, errors.New("crypto resolver refused") }, func(context.Context, string) ([]string, error) { return []string{"not-an-ip"}, nil }} {
		if _, _, err := BuildFromAccepted(context.Background(), desiredDevices(), []byte(acceptedRoutedTUN), deviceObservation(), resolver); err == nil {
			t.Fatal("unresolved or invalid endpoint accepted")
		}
	}
	observed := router.CaptureObservation{Devices: deviceObservation().Devices}
	if _, _, err := BuildFromAccepted(context.Background(), desiredDevices(), []byte(acceptedRoutedTUN), observed, fakeResolve); err == nil {
		t.Fatal("missing LAN authority accepted")
	}
}

func TestRoutedTUNChainShapeRejectsAdditionalDuplicateAndMissingRows(t *testing.T) {
	plan := routedTUNPlan(t)
	chain := plan.Ownership.Chains[0]
	lines := []string{"-N " + chain.Name}
	for _, argv := range plan.Apply {
		if len(argv) > 6 && argv[4] == chain.Table && argv[5] == "-A" && argv[6] == chain.Name {
			lines = append(lines, strings.Join(argv[5:], " "))
		}
	}
	if len(lines) < 2 || tunChainShape([]byte(strings.Join(lines, "\n")), chain, plan.Apply) != nil {
		t.Fatal("generated chain shape rejected")
	}
	for _, listing := range []string{
		strings.Join(lines[1:], "\n"), strings.Join(lines[:len(lines)-1], "\n"),
		strings.Join(append(slices.Clone(lines), lines[1]), "\n"),
		strings.Join(append(slices.Clone(lines), "-A "+chain.Name+" -j ACCEPT"), "\n"),
		strings.Join(append(slices.Clone(lines), "-A FOREIGN -j ACCEPT"), "\n"),
	} {
		if tunChainShape([]byte(listing), chain, plan.Apply) == nil {
			t.Fatalf("changed owned chain admitted: %s", listing)
		}
	}
}

func TestRoutedTUNChainShapeProvesOrderAndKnownKernelSpelling(t *testing.T) {
	plan := routedTUNPlan(t)
	for _, chain := range plan.Ownership.Chains {
		lines := []string{"-N " + chain.Name}
		for _, argv := range plan.Apply {
			if len(argv) > 6 && argv[4] == chain.Table && argv[5] == "-A" && argv[6] == chain.Name {
				lines = append(lines, strings.Join(argv[5:], " "))
			}
		}
		if len(lines) < 3 {
			t.Fatal("insufficient fixture rows", chain)
		}
		swapped := slices.Clone(lines)
		swapped[1], swapped[len(swapped)-1] = swapped[len(swapped)-1], swapped[1]
		if tunChainShape([]byte(strings.Join(swapped, "\n")), chain, plan.Apply) == nil {
			t.Fatal("permuted chain proved Active", chain)
		}
		listing := strings.Join(lines, "\n")
		listing = strings.ReplaceAll(listing, "-p tcp ", "-p tcp -m tcp ")
		listing = strings.ReplaceAll(listing, "-p udp ", "-p udp -m udp ")
		listing = strings.ReplaceAll(listing, "192.0.2.1/32", "192.0.2.1")
		listing = strings.ReplaceAll(listing, "--set-xmark 0x4000/0x4000", "--set-xmark 16384/16384")
		if err := tunChainShape([]byte(listing), chain, plan.Apply); err != nil {
			t.Fatal("known kernel spelling rejected", chain, err)
		}
	}
}

func TestCompiledRoutedTUNAcceptedNativeRoundTrip(t *testing.T) {
	node := proxy.Node{Server: "node.example", Port: 443, UUID: "00000000-1111-4222-8333-444444444444", ServerName: "www.example.com", RealityPublicKey: "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE", RealityShortID: "aabbccdd", Fingerprint: "chrome", Flow: "xtls-rprx-vision", UDP: true}
	for _, fake := range []bool{false, true} {
		compiled, err := proxy.CompileNative(proxy.CompileInput{Node: node, Datapath: proxy.DatapathRoutedTUN, RoutedTUN: &proxy.RoutedTUNConfig{InterfaceName: "b6p-tun", Address: "172.31.255.253/30"}, ManagementIPs: deviceObservation().ManagementIPs, MixedListenAddress: "127.0.0.1", DNSListenAddress: "192.0.2.1", IPv6: proxy.IPv6Direct, FakeIP: fake, LocalDNS: &proxy.LocalDNSConfig{Server: "127.0.0.1", Port: 53, Domains: []string{"home.lan"}}})
		if err != nil {
			t.Fatal(err)
		}
		input, clients, err := BuildFromAccepted(context.Background(), desiredDevices(), compiled.Config, deviceObservation(), fakeResolve)
		if err != nil {
			t.Fatal("production compiled native rejected", err)
		}
		if input.Datapath != proxy.DatapathRoutedTUN || input.FakeIP != fake || input.Ports != (proxy.Ports{Mixed: 2080, TProxy: 7893, DNS: 1053}) || len(clients) != 2 {
			t.Fatal("compiled extraction changed contract", input, clients)
		}
		if !reflect.DeepEqual(input.EndpointIPs, []string{"127.0.0.1", "203.0.113.4", "223.5.5.5"}) || !reflect.DeepEqual(input.RouterDNSAddresses, []string{"192.0.2.1"}) {
			t.Fatal("compiled local/direct DNS source changed", input)
		}
	}
}

func TestRoutedTUNObservationRejectsInterfaceStateDriftReadOnly(t *testing.T) {
	plan := routedTUNPlan(t)
	for _, tc := range []struct {
		name, address, rpf string
		failure            error
	}{
		{name: "missing", address: `[]`, rpf: "2"},
		{name: "address", address: strings.Replace(string(tunAddressFixture()), "172.31.255.253", "172.31.255.254", 1), rpf: "2"},
		{name: "down", address: strings.Replace(string(tunAddressFixture()), `"UP",`, ``, 1), rpf: "2"},
		{name: "mtu", address: strings.Replace(string(tunAddressFixture()), `"mtu":1500`, `"mtu":1400`, 1), rpf: "2"},
		{name: "strict-rpf", address: string(tunAddressFixture()), rpf: "1"},
		{name: "unavailable", failure: errors.New("permission denied")},
	} {
		t.Run(tc.name, func(t *testing.T) {
			base := tunObservedRunner(t, plan, "default dev b6p-tun", "")
			c := testController(t, func(ctx context.Context, argv []string) ([]byte, error) {
				if slices.Equal(argv, tunAddressShow("b6p-tun")) {
					return []byte(tc.address), tc.failure
				}
				if slices.Equal(argv, tunRPFilterShow("b6p-tun")) {
					return []byte(tc.rpf), nil
				}
				return base(ctx, argv)
			})
			c.plan, c.active = &plan, true
			state, err := c.Reconcile(context.Background())
			if err == nil || state.Active || !state.CleanupPending || c.plan == nil {
				t.Fatalf("state drift claimed active or discarded ownership: %+v %v", state, err)
			}
		})
	}
}

func TestAcceptedRoutedTUNDoesNotCollideWithOwnObservedAddress(t *testing.T) {
	observed := deviceObservation()
	observed.ManagementIPs = append(observed.ManagementIPs, "172.31.255.253")
	observed.InterfaceAddresses = []router.CaptureInterfaceAddress{{Interface: "br-lan", Address: "192.0.2.1/24"}, {Interface: "b6p-tun", Address: "172.31.255.253/30"}}
	input, _, err := BuildFromAccepted(context.Background(), desiredDevices(), []byte(acceptedRoutedTUN), observed, fakeResolve)
	if err != nil {
		t.Fatal("core-created own TUN was rejected", err)
	}
	if slices.Contains(input.ManagementIPs, "172.31.255.253") {
		t.Fatal("own address leaked into planner collision list")
	}
	if _, err = proxy.PlanOwnedRules(input); err != nil {
		t.Fatal(err)
	}
	for _, entry := range []router.CaptureInterfaceAddress{{Interface: "foreign", Address: "172.31.255.253/30"}, {Interface: "b6p-tun", Address: "172.31.255.254/30"}, {Interface: "b6p-tun", Address: "172.31.255.253/24"}} {
		wrong := observed
		wrong.InterfaceAddresses = []router.CaptureInterfaceAddress{entry}
		if _, _, err = BuildFromAccepted(context.Background(), desiredDevices(), []byte(acceptedRoutedTUN), wrong, fakeResolve); err == nil {
			t.Fatal("foreign allocation exempted", entry)
		}
	}
	unproved := observed
	unproved.InterfaceAddresses = nil
	if _, _, err = BuildFromAccepted(context.Background(), desiredDevices(), []byte(acceptedRoutedTUN), unproved, fakeResolve); err == nil {
		t.Fatal("bare management address exempted without provenance")
	}
}

package proxy

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"strings"
	"testing"
)

const (
	rustCaptureGoldenOutputEnv  = "CAPTURE_GO_GOLDEN_OUTPUT"
	rustCaptureGoldenCompareEnv = "CAPTURE_GO_GOLDEN_COMPARE"
)

// The explicit fixture-only DTO is not RulesPlanInput's JSON representation:
// that production type uses Pascal/acronym field names and several omitempty
// tags. Every input field below is emitted, including null versus []/{}.
// In particular, gateway rejects a present empty client list or MAC map.
// This file calls only the pure planner. Its argv output is never executed.
type rustCaptureGoldenInput struct {
	Scope              CaptureScope           `json:"scope"`
	LANIPv4Prefixes    []string               `json:"lanIPv4Prefixes"`
	Datapath           DatapathMode           `json:"datapath"`
	TUNInterface       string                 `json:"tunInterface"`
	TUNAddress         string                 `json:"tunAddress"`
	ClientIPv4         string                 `json:"clientIPv4"`
	ClientIPv6         string                 `json:"clientIPv6"`
	ClientIPv4s        []string               `json:"clientIPv4s"`
	ClientIPv6s        []string               `json:"clientIPv6s"`
	ClientMACs         map[string]string      `json:"clientMACs"`
	LANInterface       string                 `json:"lanInterface"`
	Ports              rustCaptureGoldenPorts `json:"ports"`
	IPv6               IPv6Mode               `json:"ipv6"`
	Failure            FailurePolicy          `json:"failure"`
	EndpointIPs        []string               `json:"endpointIPs"`
	ManagementIPs      []string               `json:"managementIPs"`
	RouterDNSAddresses []string               `json:"routerDNSAddresses"`
	FakeIP             bool                   `json:"fakeIP"`
}

type rustCaptureGoldenPorts struct {
	Mixed  uint16 `json:"mixed"`
	TProxy uint16 `json:"tProxy"`
	DNS    uint16 `json:"dns"`
}

func (in rustCaptureGoldenInput) rules() RulesPlanInput {
	return RulesPlanInput{
		Scope: in.Scope, LANIPv4Prefixes: in.LANIPv4Prefixes, Datapath: in.Datapath,
		TUNInterface: in.TUNInterface, TUNAddress: in.TUNAddress,
		ClientIPv4: in.ClientIPv4, ClientIPv6: in.ClientIPv6,
		ClientIPv4s: in.ClientIPv4s, ClientIPv6s: in.ClientIPv6s, ClientMACs: in.ClientMACs,
		LANInterface: in.LANInterface,
		Ports:        Ports{Mixed: in.Ports.Mixed, TProxy: in.Ports.TProxy, DNS: in.Ports.DNS},
		IPv6:         in.IPv6, Failure: in.Failure, EndpointIPs: in.EndpointIPs,
		ManagementIPs: in.ManagementIPs, RouterDNSAddresses: in.RouterDNSAddresses,
		FakeIP: in.FakeIP,
	}
}

type rustCaptureGoldenCase struct {
	Name  string                 `json:"name"`
	Input rustCaptureGoldenInput `json:"input"`
	Valid bool                   `json:"valid"`
	// Marshal the actual Go output unchanged, including Pascal/acronym keys,
	// ownership omitempty behavior, ordered full argv and verbatim warnings.
	Plan  *OwnedRulesPlan `json:"plan,omitempty"`
	Error string          `json:"error,omitempty"`
}

type rustCaptureGoldenDocument struct {
	Version int                     `json:"version"`
	Cases   []rustCaptureGoldenCase `json:"cases"`
}

type rustCaptureGoldenSource struct {
	name  string
	input rustCaptureGoldenInput
	error string // Fixed safe Go error; empty means valid.
}

func rustCaptureGoldenDeviceInput() rustCaptureGoldenInput {
	return rustCaptureGoldenInput{
		Scope: CaptureScopeDevices, Datapath: DatapathRoutedTUN,
		TUNInterface: "b6p-tun", TUNAddress: "172.31.255.253/30",
		ClientIPv4: "192.168.50.42", ClientMACs: map[string]string{"192.168.50.42": "02:11:22:33:44:42"},
		LANInterface: "br-lan", Ports: rustCaptureGoldenPorts{Mixed: 2080, TProxy: 7893, DNS: 1053},
		IPv6: IPv6Direct, Failure: FailureDirect,
	}
}

func rustCaptureGoldenGatewayInput() rustCaptureGoldenInput {
	in := rustCaptureGoldenDeviceInput()
	in.Scope, in.ClientIPv4, in.ClientMACs = CaptureScopeGateway, "", nil
	in.LANIPv4Prefixes = []string{"192.168.50.0/24"}
	return in
}

func rustCaptureGoldenInputs() []rustCaptureGoldenSource {
	sources := []rustCaptureGoldenSource{}
	add := func(name string, gateway bool, change func(*rustCaptureGoldenInput), errorText string) {
		in := rustCaptureGoldenDeviceInput()
		if gateway {
			in = rustCaptureGoldenGatewayInput()
		}
		if change != nil {
			change(&in)
		}
		sources = append(sources, rustCaptureGoldenSource{name: name, input: in, error: errorText})
	}

	// All addresses and MACs are synthetic public test intent. Gateway contains
	// no device inventory: new addresses within the declaration follow its hooks.
	add("gateway-24-inventory-free-unused-tproxy-zero", true, func(in *rustCaptureGoldenInput) {
		in.Ports.TProxy = 0 // Unused in gateway; not a listener reservation.
	}, "")
	add("gateway-32", true, func(in *rustCaptureGoldenInput) {
		in.LANIPv4Prefixes = []string{"192.168.50.243/32"}
	}, "")
	add("gateway-multiple-rfc1918-prefixes-sorted", true, func(in *rustCaptureGoldenInput) {
		in.LANIPv4Prefixes = []string{"192.168.50.0/24", "172.20.0.0/16", "10.20.0.0/16"}
		in.Ports.TProxy = in.Ports.Mixed // Also ignored for gateway.
	}, "")
	add("devices-one-mac", false, nil, "")
	add("devices-two-normalized-macs", false, func(in *rustCaptureGoldenInput) {
		in.ClientIPv4 = ""
		in.ClientIPv4s = []string{"192.168.50.43", "192.168.50.42"}
		in.ClientMACs = map[string]string{"192.168.50.43": "02-AA-BB-CC-DD-43", "192.168.50.42": "02:AA:BB:CC:DD:42"}
	}, "")
	add("devices-many-merged-deduplicated", false, func(in *rustCaptureGoldenInput) {
		in.ClientIPv4s = []string{"192.168.50.44", "192.168.50.42", "192.168.50.43", "192.168.50.45", "192.168.50.44"}
		in.ClientMACs = map[string]string{
			"192.168.50.42": "02:11:22:33:44:42", "192.168.50.43": "02:11:22:33:44:43",
			"192.168.50.44": "02:11:22:33:44:44", "192.168.50.45": "02:11:22:33:44:45",
		}
		in.ClientIPv6 = "2001:db8:50::42"
		in.ClientIPv6s = []string{"2001:0db8:0050::0043", "2001:db8:50::42"}
	}, "")
	add("devices-endpoint-dns-router-opt-in-order", false, func(in *rustCaptureGoldenInput) {
		in.EndpointIPs = []string{"203.0.113.20", "10.20.30.40", "203.0.113.10", "203.0.113.20"}
		in.ManagementIPs = []string{"192.168.50.9", "2001:db8:50::1", "192.168.50.1", "203.0.113.10"}
		in.RouterDNSAddresses = []string{"192.168.50.9", "192.168.50.1", "192.168.50.9"}
	}, "")
	add("gateway-fakeip-management-return-bypass", true, func(in *rustCaptureGoldenInput) {
		in.FakeIP = true
		in.EndpointIPs = []string{"198.18.1.20", "203.0.113.20"}
		in.ManagementIPs = []string{"192.168.50.9", "198.18.1.10", "192.168.50.1"}
		in.RouterDNSAddresses = []string{"192.168.50.1"}
	}, "")
	add("devices-empty-policies-zero-ports-default", false, func(in *rustCaptureGoldenInput) {
		in.Scope, in.IPv6, in.Failure = "", "", ""
		in.Ports = rustCaptureGoldenPorts{}
		in.EndpointIPs, in.ManagementIPs, in.RouterDNSAddresses = []string{}, []string{}, []string{}
	}, "")

	// Invalid cases use routed-TUN input only. Do not turn the retired Go
	// TPROXY backend into a Rust success/parity requirement.
	add("gateway-prefix-overlap", true, func(in *rustCaptureGoldenInput) {
		in.LANIPv4Prefixes = []string{"192.168.50.0/24", "192.168.50.42/32"}
	}, "LANIPv4Prefixes must be disjoint without duplicate or overlapping prefixes")
	add("gateway-prefix-host-bits", true, func(in *rustCaptureGoldenInput) {
		in.LANIPv4Prefixes = []string{"192.168.50.1/24"}
	}, "LANIPv4Prefixes entry 0 requires a canonical IPv4 network prefix /8 through /32 without host bits")
	add("gateway-prefix-not-rfc1918", true, func(in *rustCaptureGoldenInput) {
		in.LANIPv4Prefixes = []string{"203.0.113.0/24"}
	}, "LANIPv4Prefixes entry 0 must be wholly inside RFC1918 space")
	add("gateway-prefix-count-limit", true, func(in *rustCaptureGoldenInput) {
		in.LANIPv4Prefixes = []string{"10.20.0.1/32", "10.20.0.2/32", "10.20.0.3/32", "10.20.0.4/32", "10.20.0.5/32", "10.20.0.6/32", "10.20.0.7/32", "10.20.0.8/32", "10.20.0.9/32"}
	}, "gateway requires 1 to 8 declared IPv4 LAN prefixes")
	add("gateway-present-empty-client-list", true, func(in *rustCaptureGoldenInput) {
		in.ClientIPv4s = []string{}
	}, "gateway forbids exact client addresses, client lists and MAC maps")
	add("devices-mac-coverage-missing", false, func(in *rustCaptureGoldenInput) {
		in.ClientIPv4s = []string{"192.168.50.43"}
	}, "ClientMACs: every selected IPv4 and active IPv6 client requires an exact source MAC")
	add("devices-wildcard-lan", false, func(in *rustCaptureGoldenInput) {
		in.LANInterface = "br-lan+"
	}, "LANInterface must be a safe, exact interface name of 1 to 15 bytes (no wildcard)")
	add("devices-unowned-tun", false, func(in *rustCaptureGoldenInput) {
		in.TUNInterface = "tun0"
	}, "TUNInterface must be a distinct owned b6p- interface name of 5 to 15 bytes (no alias or wildcard)")
	add("devices-tun-not-first-host", false, func(in *rustCaptureGoldenInput) {
		in.TUNAddress = "172.31.255.254/30"
	}, "TUNAddress must be the first usable /30 host so the next host is its usable peer")
	add("gateway-tun-overlaps-lan", true, func(in *rustCaptureGoldenInput) {
		in.LANIPv4Prefixes = []string{"172.31.255.0/24"}
	}, "TUNAddress /30 overlaps a declared LAN prefix")
	add("devices-tun-collides-client", false, func(in *rustCaptureGoldenInput) {
		in.TUNAddress = "192.168.50.41/30"
	}, "TUNAddress /30 collides with a selected client")
	add("devices-tun-collides-management", false, func(in *rustCaptureGoldenInput) {
		in.ManagementIPs = []string{"172.31.255.254"}
	}, "TUNAddress /30 collides with management or endpoint addresses")
	add("devices-tun-collides-endpoint", false, func(in *rustCaptureGoldenInput) {
		in.EndpointIPs = []string{"172.31.255.253"}
	}, "TUNAddress /30 collides with management or endpoint addresses")
	add("devices-ipv6-follow-unqualified", false, func(in *rustCaptureGoldenInput) {
		in.IPv6, in.ClientIPv6 = IPv6Follow, "2001:db8:50::42"
		in.ClientMACs[in.ClientIPv6] = "02:11:22:33:44:42"
	}, "routed-tun requires IPv6 direct in the initial qualified phase")
	add("devices-ipv6-block-unqualified", false, func(in *rustCaptureGoldenInput) {
		in.IPv6, in.ClientIPv6 = IPv6Block, "2001:db8:50::42"
		in.ClientMACs[in.ClientIPv6] = "02:11:22:33:44:42"
	}, "routed-tun requires IPv6 direct in the initial qualified phase")
	add("devices-failure-block-proxy", false, func(in *rustCaptureGoldenInput) {
		in.Failure = FailureBlockProxy
	}, "block-proxy requires a surviving stateful DNS/flow classifier; dropping all selected-client traffic is not selective blocking")
	add("gateway-zero-ports", true, func(in *rustCaptureGoldenInput) {
		in.Ports = rustCaptureGoldenPorts{}
	}, "gateway Mixed and DNS listener ports must be actual, nonzero and distinct")
	add("devices-listener-port-collision", false, func(in *rustCaptureGoldenInput) {
		in.Ports.DNS = in.Ports.Mixed
	}, "Mixed port must not share a TProxy or DNS listener port")
	add("devices-client-count-limit", false, func(in *rustCaptureGoldenInput) {
		in.ClientIPv4s = make([]string, 64) // Plus the singular client = 65 raw entries.
		for i := range in.ClientIPv4s {
			in.ClientIPv4s[i] = fmt.Sprintf("192.168.50.%d", i+1)
		}
	}, "ClientIPv4/ClientIPv4s: at most 64 exact clients per family")
	add("devices-unused-tproxy-noncanonical", false, func(in *rustCaptureGoldenInput) {
		in.Ports.TProxy = 7894
	}, "routed-tun requires the unused compatibility TProxy port 7893")
	add("devices-router-dns-not-management", false, func(in *rustCaptureGoldenInput) {
		in.RouterDNSAddresses = []string{"192.168.50.1"}
	}, "RouterDNSAddresses must be unicast router LAN addresses also present in ManagementIPs")
	return sources
}

func rustCaptureGoldenBuild(t *testing.T) rustCaptureGoldenDocument {
	t.Helper()
	document := rustCaptureGoldenDocument{Version: 1, Cases: []rustCaptureGoldenCase{}}
	names := map[string]bool{}
	for _, source := range rustCaptureGoldenInputs() {
		if names[source.name] {
			t.Fatal("duplicate capture golden case", source.name)
		}
		names[source.name] = true
		original, err := json.Marshal(source.input)
		if err != nil {
			t.Fatal(err)
		}
		plan, planErr := PlanOwnedRules(source.input.rules())
		after, err := json.Marshal(source.input)
		if err != nil || !bytes.Equal(original, after) {
			t.Fatal("reference planner changed caller input", source.name, err)
		}
		fixture := rustCaptureGoldenCase{Name: source.name, Input: source.input, Valid: source.error == ""}
		if fixture.Valid {
			if planErr != nil {
				t.Fatal(source.name, planErr)
			}
			rustCaptureGoldenCheckPlan(t, source.name, plan)
			fixture.Plan = &plan
		} else {
			if planErr == nil || planErr.Error() != source.error {
				t.Fatalf("%s: fixed Go error changed: got %v, want %q", source.name, planErr, source.error)
			}
			if !reflect.DeepEqual(plan, OwnedRulesPlan{}) {
				t.Fatal("invalid input returned command intent", source.name)
			}
			fixture.Error = planErr.Error()
		}
		document.Cases = append(document.Cases, fixture)
	}
	return document
}

func rustCaptureGoldenCheckPlan(t *testing.T, name string, plan OwnedRulesPlan) {
	t.Helper()
	ownership := plan.Ownership
	wantChains := []OwnedChain{
		{Family: 4, Table: "mangle", Name: "B6P_V4_TUN_MARK", Hook: "PREROUTING"},
		{Family: 4, Table: "nat", Name: "B6P_V4_DNS", Hook: "PREROUTING"},
		{Family: 4, Table: "filter", Name: "B6P_V4_TUN_FORWARD", Hook: "FORWARD"},
		{Family: 4, Table: "filter", Name: "B6P_V4_TUN_RETURN", Hook: "FORWARD"},
		{Family: 4, Table: "filter", Name: "B6P_V4_TUN_INPUT", Hook: "INPUT"},
		{Family: 4, Table: "filter", Name: "B6P_V4_TUN_OUTPUT", Hook: "OUTPUT"},
	}
	if ownership.Datapath != DatapathRoutedTUN || ownership.Mark != 0x4000 || ownership.Mask != 0x4000 ||
		ownership.RouteTable != 16500 || ownership.RulePriority != 16500 ||
		!slices.Equal(ownership.RouteFamilies, []int{4}) || !reflect.DeepEqual(ownership.Chains, wantChains) {
		t.Fatal("capture ownership escaped fixed routed-TUN resources", name)
	}
	if !reflect.DeepEqual(plan.Cleanup, plan.OnFailure) || len(plan.Warnings) < 8 {
		t.Fatal("fail-direct cleanup or actual intent warnings missing", name)
	}
	if len(plan.Apply) == 0 || len(plan.Cleanup) == 0 ||
		!reflect.DeepEqual(plan.Apply[0], ownedRoutedTUNRoute(ownership.TUNInterface, "add")) ||
		!reflect.DeepEqual(plan.Cleanup[len(plan.Cleanup)-1], ownedRoutedTUNRoute(ownership.TUNInterface, "del")) {
		t.Fatal("ordinary owned TUN route ordering changed", name)
	}
	for _, commands := range [][][]string{plan.Apply, plan.Cleanup, plan.OnFailure} {
		for _, command := range commands {
			if len(command) < 7 || slices.Contains(command, "TPROXY") || slices.Contains(command, "ip6tables") {
				t.Fatal("command is not routed IPv4 argv", name, command)
			}
			switch command[0] {
			case "ip":
				if command[1] != "-4" || (command[2] != "route" && command[2] != "rule") ||
					command[len(command)-1] != "16500" {
					t.Fatal("command escaped owned policy route/table", name, command)
				}
			case "iptables":
				if !slices.Equal(command[1:4], []string{"-w", "5", "-t"}) {
					t.Fatal("command lost bounded iptables prefix", name, command)
				}
				switch command[5] {
				case "-N", "-A", "-F", "-X":
					if !strings.HasPrefix(command[6], "B6P_V4_") {
						t.Fatal("command mutates an unowned chain", name, command)
					}
				case "-I", "-D":
					if !slices.Contains([]string{"PREROUTING", "FORWARD", "INPUT", "OUTPUT"}, command[6]) ||
						!strings.HasPrefix(command[len(command)-1], "B6P_V4_") {
						t.Fatal("command lost a narrow owned-chain hook", name, command)
					}
				default:
					t.Fatal("unexpected firewall operation", name, command)
				}
			default:
				t.Fatal("command intent includes a foreign executable", name, command)
			}
		}
	}
}

func rustCaptureGoldenBytes(t *testing.T) []byte {
	t.Helper()
	raw, err := json.MarshalIndent(rustCaptureGoldenBuild(t), "", "  ")
	if err != nil {
		t.Fatal(err)
	}
	return append(raw, '\n')
}

func TestRustCaptureGoldenSourceCases(t *testing.T) {
	document := rustCaptureGoldenBuild(t)
	if len(document.Cases) != 30 {
		t.Fatal("review the compact 30-case capture matrix if it changes")
	}
	raw := rustCaptureGoldenBytes(t)
	if !bytes.Equal(raw, rustCaptureGoldenBytes(t)) {
		t.Fatal("capture generation is not byte-deterministic")
	}
	var decoded rustCaptureGoldenDocument
	if err := json.Unmarshal(raw, &decoded); err != nil || !reflect.DeepEqual(decoded, document) {
		t.Fatal("explicit DTO round-trip lost nil/empty intent or actual Go plan", err)
	}
	cases := map[string]rustCaptureGoldenCase{}
	for _, fixture := range document.Cases {
		cases[fixture.Name] = fixture
	}
	gateway := cases["gateway-24-inventory-free-unused-tproxy-zero"]
	if gateway.Input.ClientIPv4s != nil || gateway.Input.ClientIPv6s != nil || gateway.Input.ClientMACs != nil ||
		gateway.Plan.Ownership.ClientMACs != nil || gateway.Plan.Ownership.ClientIPv4 != "" {
		t.Fatal("gateway gained device-inventory authority")
	}
	if cases["gateway-present-empty-client-list"].Input.ClientIPv4s == nil {
		t.Fatal("present empty gateway client list lost its refusal semantics")
	}
	if !slices.Equal(cases["gateway-multiple-rfc1918-prefixes-sorted"].Plan.Ownership.LANIPv4Prefixes,
		[]string{"10.20.0.0/16", "172.20.0.0/16", "192.168.50.0/24"}) {
		t.Fatal("gateway prefix canonical sort changed")
	}
	if cases["devices-two-normalized-macs"].Plan.Ownership.ClientMACs["192.168.50.43"] != "02:aa:bb:cc:dd:43" {
		t.Fatal("canonical source MAC changed")
	}
	if !reflect.DeepEqual(cases["devices-empty-policies-zero-ports-default"].Plan, cases["devices-one-mac"].Plan) {
		t.Fatal("explicit default input changed actual Go plan")
	}
	// OnFailure equals Cleanup by value but has independent argv storage.
	plan := *cases["devices-one-mac"].Plan
	before := plan.Cleanup[0][0]
	plan.OnFailure[0][0] = "synthetic-mutation"
	if plan.Cleanup[0][0] != before {
		t.Fatal("failure argv aliases cleanup")
	}
}

func TestRustCaptureGoldenFixtures(t *testing.T) {
	outputPath, comparePath := os.Getenv(rustCaptureGoldenOutputEnv), os.Getenv(rustCaptureGoldenCompareEnv)
	if outputPath == "" && comparePath == "" {
		t.Skip("explicit golden output or compare path is required; ordinary tests do not write fixtures")
	}
	if outputPath != "" && comparePath != "" {
		t.Fatal("choose either explicit output or read-only compare, not both")
	}
	selectedPath := outputPath
	if selectedPath == "" {
		selectedPath = comparePath
	}
	if !filepath.IsAbs(selectedPath) {
		t.Fatal("golden output and compare paths must be absolute")
	}
	raw := rustCaptureGoldenBytes(t)
	if outputPath != "" {
		if err := os.WriteFile(outputPath, raw, 0o644); err != nil {
			t.Fatal(err)
		}
		return
	}
	existing, err := os.ReadFile(comparePath)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(existing, raw) {
		t.Fatal("Go capture golden differs; regenerate explicitly and review the fixture change")
	}
}

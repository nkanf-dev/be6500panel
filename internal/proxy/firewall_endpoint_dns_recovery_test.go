package proxy_test

import (
	"context"
	"encoding/json"
	"fmt"
	"net/netip"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"testing"

	"be6500panel/internal/capture"
	"be6500panel/internal/proxy"
)

// Captured from the unmodified compiler before the endpoint DNS fix. All data
// is synthetic. Keeping this static proves old journals, not newly generated
// journals, retain exact validated cleanup despite the changed NAT rule order.
const oldEndpointDNSJournal = `{"Apply":[["ip","-4","route","add","local","0.0.0.0/0","dev","lo","table","16500"],["ip","-4","rule","add","priority","16500","from","192.168.31.250/32","iif","br-lan","fwmark","0x4000/0x4000","lookup","16500"],["iptables","-w","5","-t","mangle","-N","B6P_V4_CAPTURE"],["iptables","-w","5","-t","nat","-N","B6P_V4_DNS"],["iptables","-w","5","-t","mangle","-A","B6P_V4_CAPTURE","-m","addrtype","--dst-type","LOCAL","-j","RETURN"],["iptables","-w","5","-t","mangle","-A","B6P_V4_CAPTURE","-d","192.168.31.1/32","-j","RETURN"],["iptables","-w","5","-t","mangle","-A","B6P_V4_CAPTURE","-d","203.0.113.20/32","-j","RETURN"],["iptables","-w","5","-t","mangle","-A","B6P_V4_CAPTURE","-d","223.5.5.5/32","-j","RETURN"],["iptables","-w","5","-t","mangle","-A","B6P_V4_CAPTURE","-d","0.0.0.0/8","-j","RETURN"],["iptables","-w","5","-t","mangle","-A","B6P_V4_CAPTURE","-d","127.0.0.0/8","-j","RETURN"],["iptables","-w","5","-t","mangle","-A","B6P_V4_CAPTURE","-d","169.254.0.0/16","-j","RETURN"],["iptables","-w","5","-t","mangle","-A","B6P_V4_CAPTURE","-d","192.0.0.0/24","-j","RETURN"],["iptables","-w","5","-t","mangle","-A","B6P_V4_CAPTURE","-d","224.0.0.0/4","-j","RETURN"],["iptables","-w","5","-t","mangle","-A","B6P_V4_CAPTURE","-d","240.0.0.0/4","-j","RETURN"],["iptables","-w","5","-t","nat","-A","B6P_V4_DNS","-d","192.168.31.1/32","-p","tcp","--dport","53","-j","REDIRECT","--to-ports","6450"],["iptables","-w","5","-t","nat","-A","B6P_V4_DNS","-d","192.168.31.1/32","-p","udp","--dport","53","-j","REDIRECT","--to-ports","6450"],["iptables","-w","5","-t","nat","-A","B6P_V4_DNS","-m","addrtype","--dst-type","LOCAL","-j","RETURN"],["iptables","-w","5","-t","nat","-A","B6P_V4_DNS","-d","192.168.31.1/32","-j","RETURN"],["iptables","-w","5","-t","nat","-A","B6P_V4_DNS","-d","203.0.113.20/32","-j","RETURN"],["iptables","-w","5","-t","nat","-A","B6P_V4_DNS","-d","223.5.5.5/32","-j","RETURN"],["iptables","-w","5","-t","nat","-A","B6P_V4_DNS","-d","0.0.0.0/8","-j","RETURN"],["iptables","-w","5","-t","nat","-A","B6P_V4_DNS","-d","127.0.0.0/8","-j","RETURN"],["iptables","-w","5","-t","nat","-A","B6P_V4_DNS","-d","169.254.0.0/16","-j","RETURN"],["iptables","-w","5","-t","nat","-A","B6P_V4_DNS","-d","192.0.0.0/24","-j","RETURN"],["iptables","-w","5","-t","nat","-A","B6P_V4_DNS","-d","224.0.0.0/4","-j","RETURN"],["iptables","-w","5","-t","nat","-A","B6P_V4_DNS","-d","240.0.0.0/4","-j","RETURN"],["iptables","-w","5","-t","mangle","-A","B6P_V4_CAPTURE","-p","tcp","--dport","53","-j","RETURN"],["iptables","-w","5","-t","nat","-A","B6P_V4_DNS","-p","tcp","--dport","53","-j","REDIRECT","--to-ports","6450"],["iptables","-w","5","-t","mangle","-A","B6P_V4_CAPTURE","-p","udp","--dport","53","-j","RETURN"],["iptables","-w","5","-t","nat","-A","B6P_V4_DNS","-p","udp","--dport","53","-j","REDIRECT","--to-ports","6450"],["iptables","-w","5","-t","mangle","-A","B6P_V4_CAPTURE","-d","10.0.0.0/8","-j","RETURN"],["iptables","-w","5","-t","nat","-A","B6P_V4_DNS","-d","10.0.0.0/8","-j","RETURN"],["iptables","-w","5","-t","mangle","-A","B6P_V4_CAPTURE","-d","172.16.0.0/12","-j","RETURN"],["iptables","-w","5","-t","nat","-A","B6P_V4_DNS","-d","172.16.0.0/12","-j","RETURN"],["iptables","-w","5","-t","mangle","-A","B6P_V4_CAPTURE","-d","192.168.0.0/16","-j","RETURN"],["iptables","-w","5","-t","nat","-A","B6P_V4_DNS","-d","192.168.0.0/16","-j","RETURN"],["iptables","-w","5","-t","mangle","-A","B6P_V4_CAPTURE","-d","100.64.0.0/10","-j","RETURN"],["iptables","-w","5","-t","nat","-A","B6P_V4_DNS","-d","100.64.0.0/10","-j","RETURN"],["iptables","-w","5","-t","mangle","-A","B6P_V4_CAPTURE","-d","198.18.0.0/15","-j","RETURN"],["iptables","-w","5","-t","nat","-A","B6P_V4_DNS","-d","198.18.0.0/15","-j","RETURN"],["iptables","-w","5","-t","mangle","-A","B6P_V4_CAPTURE","-p","tcp","-j","TPROXY","--on-ip","127.0.0.1","--on-port","7893","--tproxy-mark","0x4000/0x4000"],["iptables","-w","5","-t","mangle","-A","B6P_V4_CAPTURE","-p","udp","-j","TPROXY","--on-ip","127.0.0.1","--on-port","7893","--tproxy-mark","0x4000/0x4000"],["iptables","-w","5","-t","mangle","-A","B6P_V4_CAPTURE","-j","RETURN"],["iptables","-w","5","-t","nat","-A","B6P_V4_DNS","-j","RETURN"],["iptables","-w","5","-t","nat","-I","PREROUTING","1","-i","br-lan","-s","192.168.31.250/32","-m","mac","--mac-source","02:00:00:00:00:fa","-j","B6P_V4_DNS"],["iptables","-w","5","-t","mangle","-I","PREROUTING","1","-i","br-lan","-s","192.168.31.250/32","-m","mac","--mac-source","02:00:00:00:00:fa","-j","B6P_V4_CAPTURE"]],"Cleanup":[["iptables","-w","5","-t","mangle","-D","PREROUTING","-i","br-lan","-s","192.168.31.250/32","-m","mac","--mac-source","02:00:00:00:00:fa","-j","B6P_V4_CAPTURE"],["iptables","-w","5","-t","nat","-D","PREROUTING","-i","br-lan","-s","192.168.31.250/32","-m","mac","--mac-source","02:00:00:00:00:fa","-j","B6P_V4_DNS"],["iptables","-w","5","-t","nat","-F","B6P_V4_DNS"],["iptables","-w","5","-t","nat","-X","B6P_V4_DNS"],["iptables","-w","5","-t","mangle","-F","B6P_V4_CAPTURE"],["iptables","-w","5","-t","mangle","-X","B6P_V4_CAPTURE"],["ip","-4","rule","del","priority","16500","from","192.168.31.250/32","iif","br-lan","fwmark","0x4000/0x4000","lookup","16500"],["ip","-4","route","del","local","0.0.0.0/0","dev","lo","table","16500"]],"OnFailure":[["iptables","-w","5","-t","mangle","-D","PREROUTING","-i","br-lan","-s","192.168.31.250/32","-m","mac","--mac-source","02:00:00:00:00:fa","-j","B6P_V4_CAPTURE"],["iptables","-w","5","-t","nat","-D","PREROUTING","-i","br-lan","-s","192.168.31.250/32","-m","mac","--mac-source","02:00:00:00:00:fa","-j","B6P_V4_DNS"],["iptables","-w","5","-t","nat","-F","B6P_V4_DNS"],["iptables","-w","5","-t","nat","-X","B6P_V4_DNS"],["iptables","-w","5","-t","mangle","-F","B6P_V4_CAPTURE"],["iptables","-w","5","-t","mangle","-X","B6P_V4_CAPTURE"],["ip","-4","rule","del","priority","16500","from","192.168.31.250/32","iif","br-lan","fwmark","0x4000/0x4000","lookup","16500"],["ip","-4","route","del","local","0.0.0.0/0","dev","lo","table","16500"]],"Ownership":{"Mark":16384,"Mask":16384,"RouteTable":16500,"RulePriority":16500,"LANInterface":"br-lan","ClientIPv4":"192.168.31.250","ClientIPv6":"","ClientMACs":{"192.168.31.250":"02:00:00:00:00:fa"},"RouteFamilies":[4],"Chains":[{"Family":4,"Table":"mangle","Name":"B6P_V4_CAPTURE","Hook":"PREROUTING"},{"Family":4,"Table":"nat","Name":"B6P_V4_DNS","Hook":"PREROUTING"}]},"Warnings":["Intent only: verify unused mark 0x4000, chains, table 16500 and priority 16500; serialize generations before applying. No idempotence or rollback success is assumed.","Single-client TCP/UDP capture only; no OUTPUT or full-LAN hooks. Client address changes require a new coordinated plan.","Validate kernel TPROXY, policy routing, NAT REDIRECT, firewall hook order and return routing on the router before activation.","DNS REDIRECT requires a DNS listener on the incoming LAN address or a suitable wildcard, not loopback only. Restrict listener access in the coordinated firewall; keep mixed authentication/listener scope separate.","Router-local, management and endpoint traffic bypasses even DNS capture unless an exact ManagementIPs subset is opted into RouterDNSAddresses for selected-client TCP/UDP port53 only, preserving factory SSH management without a global port-22 exemption. Other router-addressed DNS stays on dnsmasq; TCP and UDP DNS to other unicast destinations is redirected before private-network bypass. Encrypted DNS needs separate policy.","No ECM/PPE/SFE setting is changed. Verify rule counters and real TCP, UDP, DNS, QUIC and failure/return paths with hardware offload on this firmware; argv alone does not prove capture works.","Fail-direct cleanup removes owned rules, not existing DNS REDIRECT conntrack bindings. Coordinate scoped flow drain/expiry on stop or failure; no global conntrack flush is planned.","IPv6 direct deliberately installs no IPv6 capture, DNS redirect or block rules; it is not IPv6 split routing."],"input":{"ClientIPv4":"192.168.31.250","ClientIPv6":"","ClientMACs":{"192.168.31.250":"02:00:00:00:00:fa"},"LANInterface":"br-lan","Ports":{"Mixed":2080,"TProxy":7893,"DNS":6450},"IPv6":"direct","Failure":"direct","EndpointIPs":["223.5.5.5","203.0.113.20"],"ManagementIPs":["192.168.31.1"],"RouterDNSAddresses":["192.168.31.1"],"FakeIP":false}}`

type endpointDNSJournal struct {
	proxy.OwnedRulesPlan
	Input *proxy.RulesPlanInput `json:"input,omitempty"`
}

func endpointDNSStoredJournal(t *testing.T) endpointDNSJournal {
	t.Helper()
	var stored endpointDNSJournal
	if err := json.Unmarshal([]byte(oldEndpointDNSJournal), &stored); err != nil {
		t.Fatal(err)
	}
	return stored
}

// This first-match model checks native argv intent and counters, not kernel
// routing or listener behavior. The packets have already reached exact-client
// PREROUTING chains; LOCAL and explicit safety checks remain in the real plan.
func endpointDNSFirstMatch(t *testing.T, commands [][]string, chain, destination, protocol string, port int) (int, string) {
	t.Helper()
	dst := netip.MustParseAddr(destination)
	for index, command := range commands {
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
			case "--dst-type":
				i++
				match = false // This synthetic resolver destination is not router-local.
			case "-j":
				i++
				target = command[i]
			}
		}
		if match {
			return index, target
		}
	}
	return -1, "RETURN"
}

func TestOwnedRulesEndpointDNSOldAndNewFirstMatchCounters(t *testing.T) {
	stored := endpointDNSStoredJournal(t)
	current, err := proxy.PlanOwnedRules(*stored.Input)
	if err != nil {
		t.Fatal(err)
	}
	for _, tc := range []struct {
		name    string
		plan    proxy.OwnedRulesPlan
		wantNAT string
	}{
		{"old", stored.OwnedRulesPlan, "RETURN"},
		{"fixed", current, "REDIRECT"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			counters := make(map[string]int)
			for _, protocol := range []string{"tcp", "udp"} {
				_, mangle := endpointDNSFirstMatch(t, tc.plan.Apply, "B6P_V4_CAPTURE", "223.5.5.5", protocol, 53)
				if mangle != "RETURN" {
					t.Fatalf("%s DNS mangle must return to NAT, got %s", protocol, mangle)
				}
				counters["mangle/"+mangle]++
				index, nat := endpointDNSFirstMatch(t, tc.plan.Apply, "B6P_V4_DNS", "223.5.5.5", protocol, 53)
				if nat != tc.wantNAT || index < 0 {
					t.Fatalf("%s endpoint DNS got %s at %d, want %s", protocol, nat, index, tc.wantNAT)
				}
				counters["nat/"+nat]++
				if nat == "RETURN" && !slices.Contains(tc.plan.Apply[index], "223.5.5.5/32") {
					t.Fatalf("old bypass did not hit measured resolver endpoint: %v", tc.plan.Apply[index])
				}
				if nat == "REDIRECT" && !slices.Contains(tc.plan.Apply[index], "6450") {
					t.Fatalf("fixed DNS did not reach managed listener: %v", tc.plan.Apply[index])
				}
			}
			want := map[string]int{"mangle/RETURN": 2, "nat/" + tc.wantNAT: 2}
			if !reflect.DeepEqual(counters, want) {
				t.Fatalf("first-match DNS counters got %v, want %v", counters, want)
			}
		})
	}
	// The other tables and cleanup must not acquire unrelated changes.
	for _, table := range []string{"mangle", "filter"} {
		commands := func(plan proxy.OwnedRulesPlan) [][]string {
			var result [][]string
			for _, command := range plan.Apply {
				if len(command) > 4 && command[4] == table {
					result = append(result, command)
				}
			}
			return result
		}
		if !reflect.DeepEqual(commands(stored.OwnedRulesPlan), commands(current)) {
			t.Fatalf("endpoint DNS fix changed %s intent", table)
		}
	}
}

func TestOwnedRulesEndpointDNSOldJournalKeepsValidatedCleanup(t *testing.T) {
	for _, withInput := range []bool{true, false} {
		t.Run(fmt.Sprintf("input=%v", withInput), func(t *testing.T) {
			stored := endpointDNSStoredJournal(t)
			current, err := proxy.PlanOwnedRules(*stored.Input)
			if err != nil {
				t.Fatal(err)
			}
			if reflect.DeepEqual(stored.Apply, current.Apply) {
				t.Fatal("static old journal does not exercise changed rule order")
			}
			if !reflect.DeepEqual(stored.Ownership, current.Ownership) || !reflect.DeepEqual(stored.Cleanup, current.Cleanup) || !reflect.DeepEqual(stored.OnFailure, current.OnFailure) {
				t.Fatal("endpoint DNS fix broke exact old journal ownership/cleanup")
			}
			if !withInput {
				stored.Input = nil
			}
			raw, err := json.Marshal(stored)
			if err != nil {
				t.Fatal(err)
			}
			dir := t.TempDir()
			path := filepath.Join(dir, "capture-journal.json")
			if err = os.WriteFile(path, raw, 0600); err != nil {
				t.Fatal(err)
			}
			var calls [][]string
			controller, err := capture.New(dir, func(_ context.Context, argv []string) ([]byte, error) {
				calls = append(calls, slices.Clone(argv))
				return nil, nil
			})
			if err != nil {
				t.Fatalf("exact pre-fix journal was rejected: %v", err)
			}
			if len(calls) != 0 {
				t.Fatal("journal recovery executed stored Apply or other commands")
			}
			if status := controller.Status(); status.Active || !status.CleanupPending || status.Desired {
				t.Fatalf("old journal claimed live/desired capture: %+v", status)
			}
			if err = controller.Cleanup(context.Background()); err != nil {
				t.Fatal(err)
			}
			if !reflect.DeepEqual(calls, stored.Cleanup) {
				t.Fatalf("old journal cleanup changed exact owned argv: got %v want %v", calls, stored.Cleanup)
			}
			if _, err = os.Stat(path); !os.IsNotExist(err) {
				t.Fatalf("successful fake cleanup retained journal: %v", err)
			}
		})
	}
}

func TestOwnedRulesEndpointDNSOldJournalStillRejectsForeignCleanup(t *testing.T) {
	for _, change := range []string{"mark", "hook-mac", "extra-cleanup"} {
		t.Run(change, func(t *testing.T) {
			stored := endpointDNSStoredJournal(t)
			switch change {
			case "mark":
				stored.Ownership.Mark = 0x8000
			case "hook-mac":
				index := slices.Index(stored.Cleanup[0], "--mac-source")
				if index < 0 {
					t.Fatal("static old journal lost exact MAC hook")
				}
				stored.Cleanup[0][index+1] = "02:00:00:00:00:fb"
			case "extra-cleanup":
				stored.Cleanup = append(stored.Cleanup, []string{"iptables", "-w", "5", "-t", "nat", "-F", "PREROUTING"})
			}
			raw, err := json.Marshal(stored)
			if err != nil {
				t.Fatal(err)
			}
			dir := t.TempDir()
			if err = os.WriteFile(filepath.Join(dir, "capture-journal.json"), raw, 0600); err != nil {
				t.Fatal(err)
			}
			calls := 0
			if _, err = capture.New(dir, func(context.Context, []string) ([]byte, error) { calls++; return nil, nil }); err == nil {
				t.Fatal("foreign ownership/cleanup accepted as an old journal")
			}
			if calls != 0 {
				t.Fatal("rejected journal executed commands")
			}
		})
	}
}

package capture

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"maps"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"strconv"
	"strings"
	"testing"

	"be6500panel/internal/proxy"
)

func multiClientInput(mode proxy.IPv6Mode) proxy.RulesPlanInput {
	in := testInput()
	in.ClientIPv4 = ""
	in.ClientIPv4s = []string{"192.0.2.11", "192.0.2.10", "192.0.2.11"}
	in.ClientIPv6s = []string{"2001:db8::11", "2001:0db8:0000::0010"}
	in.IPv6 = mode
	return in
}

func multiClientPlan(t *testing.T, mode proxy.IPv6Mode) proxy.OwnedRulesPlan {
	t.Helper()
	p, err := proxy.PlanOwnedRules(multiClientInput(mode))
	if err != nil {
		t.Fatal(err)
	}
	return p
}

func multiClientRuleLine(client string) string {
	return "16500: from " + client + " iif br-lan fwmark 0x4000/0x4000 lookup capture\n"
}

func TestMultiClientJournalRoundTripAndCompiledRecovery(t *testing.T) {
	for _, mode := range []proxy.IPv6Mode{proxy.IPv6Direct, proxy.IPv6Follow, proxy.IPv6Block} {
		for _, legacy := range []bool{false, true} {
			t.Run(fmt.Sprintf("%s/input-omitted=%v", mode, legacy), func(t *testing.T) {
				in := multiClientInput(mode)
				if !legacy {
					in.Ports = proxy.Ports{Mixed: 3080, TProxy: 3893, DNS: 2053}
					in.ManagementIPs = []string{"192.0.2.1", "2001:db8::1"}
					in.RouterDNSAddresses = []string{"192.0.2.1"}
					in.EndpointIPs = []string{"203.0.113.1"}
					in.FakeIP = true
				}
				p, err := proxy.PlanOwnedRules(in)
				if err != nil {
					t.Fatal(err)
				}
				stored := journal{OwnedRulesPlan: clonePlan(p), Input: &in}
				if legacy {
					stored.Input = nil
				}
				stored.Apply = [][]string{{"sh", "-c", "never execute stored apply"}}
				stored.OnFailure = [][]string{{"sh", "-c", "never execute stored failure"}}
				raw, err := json.Marshal(stored)
				if err != nil {
					t.Fatal(err)
				}
				dir := t.TempDir()
				if err = os.WriteFile(filepath.Join(dir, "capture-journal.json"), raw, 0600); err != nil {
					t.Fatal(err)
				}
				var calls [][]string
				c, err := New(dir, func(_ context.Context, a []string) ([]byte, error) {
					calls = append(calls, slices.Clone(a))
					return nil, nil
				})
				if err != nil {
					t.Fatal(err)
				}
				if len(calls) != 0 || !reflect.DeepEqual(*c.plan, p) || !c.Status().CleanupPending || c.Status().Active {
					t.Fatal("multi-client journal did not recompile into unproven recovery state")
				}
				if err = c.Cleanup(context.Background()); err != nil {
					t.Fatal(err)
				}
				if !reflect.DeepEqual(calls, p.Cleanup) {
					t.Fatalf("recovery cleanup changed exact clients/shared ownership: %v", calls)
				}
			})
		}
	}
}

func TestMultiClientJournalRejectsIncompleteOrChangedScope(t *testing.T) {
	for _, mode := range []string{"missing-v4", "changed-v6", "unsorted-v4", "duplicate-v4", "singular-alias", "missing-hook", "missing-policy", "widened-policy", "extra-route", "input-mismatch"} {
		t.Run(mode, func(t *testing.T) {
			in := multiClientInput(proxy.IPv6Follow)
			p := multiClientPlan(t, proxy.IPv6Follow)
			switch mode {
			case "missing-v4":
				p.Ownership.ClientIPv4s = p.Ownership.ClientIPv4s[:1]
			case "changed-v6":
				p.Ownership.ClientIPv6s[1] = "2001:db8::99"
			case "unsorted-v4":
				slices.Reverse(p.Ownership.ClientIPv4s)
			case "duplicate-v4":
				p.Ownership.ClientIPv4s = append(p.Ownership.ClientIPv4s, p.Ownership.ClientIPv4s[0])
			case "singular-alias":
				p.Ownership.ClientIPv4 = p.Ownership.ClientIPv4s[0]
			case "missing-hook":
				p.Cleanup = p.Cleanup[1:]
			case "missing-policy", "widened-policy":
				for i, a := range p.Cleanup {
					if a[0] != "ip" || a[2] != "rule" {
						continue
					}
					if mode == "missing-policy" {
						p.Cleanup = append(p.Cleanup[:i], p.Cleanup[i+1:]...)
					} else {
						a[7] = "2001:db8::/64"
					}
					break
				}
			case "extra-route":
				p.Cleanup = append(p.Cleanup, []string{"ip", "-4", "route", "flush", "table", "main"})
			case "input-mismatch":
				in.ClientIPv4s[0] = "192.0.2.99"
			}
			if _, err := recoveredPlan(journal{OwnedRulesPlan: p, Input: &in}); err == nil {
				t.Fatal("accepted journal not matching every exact owned client")
			}
		})
	}
}

func TestMultiClientClonePlanOwnsClientSlices(t *testing.T) {
	p, err := proxy.PlanOwnedRules(multiClientMACInput(proxy.IPv6Follow))
	if err != nil {
		t.Fatal(err)
	}
	original := clonePlan(p)
	cloned := clonePlan(p)
	cloned.Ownership.ClientIPv4s[0] = "192.0.2.99"
	cloned.Ownership.ClientIPv6s[0] = "2001:db8::99"
	cloned.Ownership.ClientMACs["192.0.2.10"] = "02:ff:ff:ff:ff:99"
	cloned.Apply[0][0] = "changed"
	cloned.Cleanup[0][0] = "changed"
	cloned.OnFailure[0][0] = "changed"
	cloned.Ownership.Chains[0].Name = "changed"
	cloned.Ownership.RouteFamilies[0] = 99
	cloned.Warnings[0] = "changed"
	if !reflect.DeepEqual(p, original) {
		t.Fatal("clonePlan shares plural ownership or command slices")
	}
}

func TestMultiClientPreflightSharedResourcesAndCompleteCleanup(t *testing.T) {
	for _, mode := range []proxy.IPv6Mode{proxy.IPv6Direct, proxy.IPv6Follow, proxy.IPv6Block} {
		t.Run(string(mode), func(t *testing.T) {
			var reads, applied, cleaned [][]string
			cleanup := false
			c := testController(t, func(ctx context.Context, a []string) ([]byte, error) {
				if isReadCommand(a) {
					reads = append(reads, slices.Clone(a))
				} else if cleanup {
					cleaned = append(cleaned, slices.Clone(a))
				} else {
					applied = append(applied, slices.Clone(a))
				}
				return idleRunner(ctx, a)
			})
			p := multiClientPlan(t, mode)
			if _, err := c.Apply(context.Background(), multiClientInput(mode)); err != nil {
				t.Fatal(err)
			}
			if !reflect.DeepEqual(applied, p.Apply) {
				t.Fatal("apply omitted an exact client or duplicated shared preparation")
			}
			count := func(want []string) int {
				n := 0
				for _, a := range reads {
					if slices.Equal(a, want) {
						n++
					}
				}
				return n
			}
			for _, family := range p.Ownership.RouteFamilies {
				for _, a := range [][]string{routeShow(family), ruleShow(family), {ipTables(family), "-w", "5", "-t", "mangle", "-S"}} {
					if count(a) != 1 {
						t.Fatalf("preflight did not inspect shared family resource once: %v", a)
					}
				}
			}
			for _, chain := range p.Ownership.Chains {
				if count([]string{ipTables(chain.Family), "-w", "5", "-t", chain.Table, "-S", chain.Name}) != 1 {
					t.Fatalf("shared chain checked once per client instead of once: %+v", chain)
				}
			}
			if len(reads) != 3*len(p.Ownership.RouteFamilies)+len(p.Ownership.Chains) {
				t.Fatalf("unexpected preflight reads: %v", reads)
			}
			cleanup = true
			if err := c.Cleanup(context.Background()); err != nil {
				t.Fatal(err)
			}
			if !reflect.DeepEqual(cleaned, p.Cleanup) {
				t.Fatal("cleanup omitted per-client ownership")
			}
		})
	}
}

func TestMultiClientPolicyObservationRequiresEveryExactClient(t *testing.T) {
	p := multiClientPlan(t, proxy.IPv6Follow)
	names := map[string]int{"capture": proxy.CaptureTable, "main": 254}
	for _, family := range []int{4, 6} {
		clients := p.Ownership.ClientIPv4s
		bits := 32
		if family == 6 {
			clients, bits = p.Ownership.ClientIPv6s, 128
		}
		t.Run(strconv.Itoa(family), func(t *testing.T) {
			first, second := multiClientRuleLine(clients[0]), multiClientRuleLine(clients[1])
			for _, tc := range []struct {
				name, lines string
				want        bool
			}{
				{"both", first + second, true},
				{"host-prefixes", multiClientRuleLine(clients[0]+"/"+strconv.Itoa(bits)) + multiClientRuleLine(clients[1]+"/"+strconv.Itoa(bits)), true},
				{"missing-first", second, false},
				{"missing-second", first, false},
				{"duplicate-first", first + first, false},
				{"wrong-interface", first + strings.ReplaceAll(second, "br-lan", "other"), false},
				{"broader-second", first + multiClientRuleLine(clients[1]+"/"+strconv.Itoa(bits-1)), false},
				{"wrong-mark", first + strings.ReplaceAll(second, "0x4000/0x4000", "0x4000"), false},
				{"wrong-table", first + strings.ReplaceAll(second, "lookup capture", "lookup main"), false},
				{"negated", first + strings.ReplaceAll(second, "from ", "not from "), false},
				{"destination-restricted", first + strings.ReplaceAll(second, " iif ", " to default iif "), false},
				{"extra-selector", first + strings.TrimSpace(second) + " oif other\n", false},
			} {
				t.Run(tc.name, func(t *testing.T) {
					if got := hasOwnedRule([]byte(tc.lines), p.Ownership, family, names); got != tc.want {
						t.Fatalf("observed ready=%v want=%v for %q", got, tc.want, tc.lines)
					}
				})
			}
		})
	}
	ipv6 := multiClientRuleLine("2001:0db8:0000:0000:0000:0000:0000:0010/128") + multiClientRuleLine("2001:db8::11")
	if !hasOwnedRule([]byte(ipv6), p.Ownership, 6, names) {
		t.Fatal("canonical-equivalent exact IPv6 source was rejected")
	}
}

func TestMultiClientReconcileMissingOnePolicyOrHookRetainsJournal(t *testing.T) {
	for _, missing := range []string{"none", "v4-policy", "v6-policy", "v4-dns-hook", "v6-capture-hook"} {
		t.Run(missing, func(t *testing.T) {
			c := testController(t, idleRunner)
			if _, err := c.Apply(context.Background(), multiClientInput(proxy.IPv6Follow)); err != nil {
				t.Fatal(err)
			}
			p := clonePlan(*c.plan)
			checked := make(map[string]bool)
			c.runner = func(_ context.Context, a []string) ([]byte, error) {
				if !isReadCommand(a) {
					t.Fatalf("reconcile mutates multi-client state: %v", a)
				}
				if a[0] == "ip" {
					if a[2] == "route" {
						return []byte("local default dev lo scope host\n"), nil
					}
					clients := p.Ownership.ClientIPv4s
					if a[1] == "-6" {
						clients = p.Ownership.ClientIPv6s
					}
					var lines string
					for i, client := range clients {
						if i == 1 && (missing == "v4-policy" && a[1] == "-4" || missing == "v6-policy" && a[1] == "-6") {
							continue
						}
						lines += multiClientRuleLine(client)
					}
					return []byte(lines), nil
				}
				if a[5] == "-C" {
					checked[strings.Join(a, " ")] = true
					if missing == "v4-dns-hook" && a[0] == "iptables" && a[4] == "nat" && a[6] == "PREROUTING" && slices.Contains(a, "192.0.2.11/32") ||
						missing == "v6-capture-hook" && a[0] == "ip6tables" && a[4] == "mangle" && a[6] == "PREROUTING" && slices.Contains(a, "2001:db8::11/128") {
						return []byte(a[0] + ": Bad rule (does a matching rule exist in that chain?)."), errors.New("exit status 1")
					}
				}
				return nil, nil
			}
			s, err := c.Reconcile(context.Background())
			if missing == "none" {
				if err != nil || !s.Active || s.CleanupPending {
					t.Fatalf("every client present: %+v %v", s, err)
				}
			} else if err == nil || s.Active || !s.CleanupPending {
				t.Fatalf("missing one exact client resource was overlooked: %+v %v", s, err)
			}
			for _, a := range p.Apply {
				if a[0] == "ip" || a[5] != "-A" && a[5] != "-I" {
					continue
				}
				check := slices.Clone(a)
				check[5] = "-C"
				if a[5] == "-I" {
					check = append(check[:7], check[8:]...)
				}
				if !checked[strings.Join(check, " ")] {
					t.Fatalf("not every shared rule and exact hook was checked: %v", check)
				}
			}
			if _, err := os.Stat(c.path); err != nil {
				t.Fatalf("reconcile lost multi-client recovery journal: %v", err)
			}
		})
	}
}

func TestMultiClientCleanupAttemptsAllAfterOneClientFailure(t *testing.T) {
	c := testController(t, idleRunner)
	if _, err := c.Apply(context.Background(), multiClientInput(proxy.IPv6Follow)); err != nil {
		t.Fatal(err)
	}
	p := clonePlan(*c.plan)
	var calls [][]string
	fail := true
	c.runner = func(_ context.Context, a []string) ([]byte, error) {
		calls = append(calls, slices.Clone(a))
		if fail && a[0] == "iptables" && a[5] == "-D" && slices.Contains(a, "192.0.2.11/32") {
			return []byte("xtables lock busy"), errors.New("exit status 4")
		}
		return nil, nil
	}
	if err := c.Cleanup(context.Background()); err == nil || !c.Status().CleanupPending {
		t.Fatal("one failed client cleanup lost recovery state")
	}
	if !reflect.DeepEqual(calls, p.Cleanup) {
		t.Fatal("one failed client cleanup stopped remaining clients/shared resources")
	}
	fail, calls = false, nil
	if err := c.Cleanup(context.Background()); err != nil || c.Status().CleanupPending || c.Status().Active {
		t.Fatalf("retry did not clear exact multi-client state: %v", err)
	}
	if !reflect.DeepEqual(calls, p.Cleanup) {
		t.Fatal("retry skipped cleanup of some exact clients")
	}
}

func multiClientMACInput(mode proxy.IPv6Mode) proxy.RulesPlanInput {
	in := multiClientInput(mode)
	in.ClientMACs = map[string]string{"192.0.2.10": "02:11:22:33:44:10", "192.0.2.11": "02:11:22:33:44:11"}
	if mode != proxy.IPv6Direct {
		in.ClientMACs["2001:db8::10"] = "02:11:22:33:44:10"
		in.ClientMACs["2001:db8::11"] = "02:11:22:33:44:11"
	}
	return in
}

func TestClientMACJournalRoundTripWithAndWithoutInput(t *testing.T) {
	for _, mode := range []proxy.IPv6Mode{proxy.IPv6Direct, proxy.IPv6Follow, proxy.IPv6Block} {
		for _, omitted := range []bool{false, true} {
			t.Run(fmt.Sprintf("%s/input-omitted=%v", mode, omitted), func(t *testing.T) {
				in := multiClientMACInput(mode)
				p, err := proxy.PlanOwnedRules(in)
				if err != nil {
					t.Fatal(err)
				}
				stored := journal{OwnedRulesPlan: clonePlan(p), Input: &in}
				if omitted {
					stored.Input = nil
				}
				stored.Apply = [][]string{{"sh", "-c", "never execute stored apply"}}
				raw, err := json.Marshal(stored)
				if err != nil {
					t.Fatal(err)
				}
				dir := t.TempDir()
				if err = os.WriteFile(filepath.Join(dir, "capture-journal.json"), raw, 0600); err != nil {
					t.Fatal(err)
				}
				var calls [][]string
				c, err := New(dir, func(_ context.Context, a []string) ([]byte, error) {
					calls = append(calls, slices.Clone(a))
					return nil, nil
				})
				if err != nil || len(calls) != 0 || !reflect.DeepEqual(*c.plan, p) {
					t.Fatalf("MAC journal did not recompile exact pairs without execution: %v", err)
				}
				if err = c.Cleanup(context.Background()); err != nil || !reflect.DeepEqual(calls, p.Cleanup) {
					t.Fatalf("recovery did not clean exact IP/MAC hooks: %v %v", calls, err)
				}
				// Recovery must not expose caller-owned journal map storage either.
				recovered, err := recoveredPlan(stored)
				if err != nil {
					t.Fatal(err)
				}
				recovered.Ownership.ClientMACs["192.0.2.10"] = "02:ff:ff:ff:ff:99"
				if !maps.Equal(stored.Ownership.ClientMACs, p.Ownership.ClientMACs) || in.ClientMACs["192.0.2.10"] != "02:11:22:33:44:10" {
					t.Fatal("recovery shares ownership or input MAC map")
				}
			})
		}
	}
}

func TestClientMACJournalRejectsChangedPairsOrBroadenedHooks(t *testing.T) {
	for _, omitted := range []bool{false, true} {
		for _, change := range []string{"missing-map", "missing-v4", "missing-v6", "changed-mac", "extra-key", "noncanonical-key", "noncanonical-mac", "missing-match", "wrong-hook-mac", "input-mismatch"} {
			t.Run(fmt.Sprintf("%s/input-omitted=%v", change, omitted), func(t *testing.T) {
				in := multiClientMACInput(proxy.IPv6Follow)
				p, err := proxy.PlanOwnedRules(in)
				if err != nil {
					t.Fatal(err)
				}
				switch change {
				case "missing-map":
					p.Ownership.ClientMACs = nil
				case "missing-v4":
					delete(p.Ownership.ClientMACs, "192.0.2.10")
				case "missing-v6":
					delete(p.Ownership.ClientMACs, "2001:db8::10")
				case "changed-mac":
					p.Ownership.ClientMACs["192.0.2.10"] = "02:ff:ff:ff:ff:99"
				case "extra-key":
					p.Ownership.ClientMACs["192.0.2.99"] = "02:ff:ff:ff:ff:99"
				case "noncanonical-key":
					p.Ownership.ClientMACs["2001:0db8::0010"] = p.Ownership.ClientMACs["2001:db8::10"]
					delete(p.Ownership.ClientMACs, "2001:db8::10")
				case "noncanonical-mac":
					p.Ownership.ClientMACs["192.0.2.10"] = "02-11-22-33-44-10"
				case "missing-match", "wrong-hook-mac":
					for i, argv := range p.Cleanup {
						if idx := slices.Index(argv, "--mac-source"); idx >= 0 {
							if change == "missing-match" {
								p.Cleanup[i] = append(slices.Clone(argv[:idx-2]), argv[idx+2:]...)
							} else {
								argv[idx+1] = "02:ff:ff:ff:ff:99"
							}
							break
						}
					}
				case "input-mismatch":
					if omitted {
						return // No input is stored in ownership-only journals.
					}
					in.ClientMACs["192.0.2.10"] = "02:ff:ff:ff:ff:99"
				}
				stored := journal{OwnedRulesPlan: p, Input: &in}
				if omitted {
					stored.Input = nil
				}
				if _, err = recoveredPlan(stored); err == nil {
					t.Fatal("accepted journal not matching compiled exact IP/MAC ownership and cleanup")
				}
			})
		}
	}
}

func TestClientMACApprovalsRequireExactApplyCleanupAndObservationHooks(t *testing.T) {
	p, err := proxy.PlanOwnedRules(multiClientMACInput(proxy.IPv6Follow))
	if err != nil {
		t.Fatal(err)
	}
	c := &Controller{plan: &p}
	var hooks [][]string
	for _, argv := range p.Apply {
		if argv[0] != "ip" && argv[5] == "-I" {
			hooks = append(hooks, slices.Clone(argv))
			check := slices.Clone(argv)
			check[5] = "-C"
			check = append(check[:7], check[8:]...)
			hooks = append(hooks, check)
		}
	}
	for _, argv := range p.Cleanup {
		if argv[0] != "ip" && argv[5] == "-D" {
			hooks = append(hooks, slices.Clone(argv))
		}
	}
	if len(hooks) != 24 {
		t.Fatalf("unexpected paired hook count: %d", len(hooks))
	}
	for _, argv := range hooks {
		if err = c.approvedCommand(argv); err != nil {
			t.Fatalf("compiled exact MAC hook rejected: %v %v", argv, err)
		}
		idx := slices.Index(argv, "--mac-source")
		if idx < 0 {
			t.Fatalf("compiled hook has no exact MAC match: %v", argv)
		}
		wrongMAC := slices.Clone(argv)
		wrongMAC[idx+1] = "02:ff:ff:ff:ff:99"
		withoutMAC := append(slices.Clone(argv[:idx-2]), argv[idx+2:]...)
		if c.approvedCommand(wrongMAC) == nil || c.approvedCommand(withoutMAC) == nil {
			t.Fatalf("hook approval allowed reused IP with unknown MAC or no identity: %v", argv)
		}
	}
}

func TestClientMACReconcileChecksExactPairsDespiteCompleteIPPolicy(t *testing.T) {
	for _, missing := range []string{"none", "nat", "mangle"} {
		t.Run(missing, func(t *testing.T) {
			c := testController(t, idleRunner)
			if _, err := c.Apply(context.Background(), multiClientMACInput(proxy.IPv6Follow)); err != nil {
				t.Fatal(err)
			}
			p := clonePlan(*c.plan)
			checks := make(map[string]int)
			c.runner = func(_ context.Context, argv []string) ([]byte, error) {
				if !isReadCommand(argv) {
					t.Fatalf("MAC reconcile mutated resources: %v", argv)
				}
				if argv[0] == "ip" {
					if argv[2] == "route" {
						return []byte("local default dev lo scope host\n"), nil
					}
					clients := p.Ownership.ClientIPv4s
					if argv[1] == "-6" {
						clients = p.Ownership.ClientIPv6s
					}
					var lines string
					for _, client := range clients {
						lines += multiClientRuleLine(client)
					}
					return []byte(lines), nil
				}
				if argv[5] == "-C" && argv[6] == "PREROUTING" {
					idx := slices.Index(argv, "--mac-source")
					if idx < 0 || argv[idx+1] != p.Ownership.ClientMACs[strings.Split(argv[slices.Index(argv, "-s")+1], "/")[0]] {
						t.Fatalf("observation lost exact IP/MAC pair: %v", argv)
					}
					checks[argv[4]]++
					if argv[4] == missing && slices.Contains(argv, "192.0.2.11/32") {
						return []byte(argv[0] + ": Bad rule (does a matching rule exist in that chain?)."), errors.New("exit status 1")
					}
				}
				return nil, nil
			}
			s, err := c.Reconcile(context.Background())
			if missing == "none" {
				if err != nil || !s.Active || s.CleanupPending {
					t.Fatalf("complete exact pairs not active: %+v %v", s, err)
				}
			} else if err == nil || s.Active || !s.CleanupPending {
				t.Fatalf("complete IP policies concealed missing exact MAC hook: %+v %v", s, err)
			}
			if checks["nat"] != 4 || checks["mangle"] != 4 {
				t.Fatalf("not every client/family exact MAC hook observed: %v", checks)
			}
		})
	}
}

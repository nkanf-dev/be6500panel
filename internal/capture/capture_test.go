package capture

import (
	"context"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	"be6500panel/internal/proxy"
)

func testInput() proxy.RulesPlanInput {
	return proxy.RulesPlanInput{ClientIPv4: "192.0.2.10", LANInterface: "br-lan", Ports: proxy.Ports{Mixed: 2080, TProxy: 7893, DNS: 1053}, IPv6: proxy.IPv6Direct, Failure: proxy.FailureDirect}
}
func testPlan(t *testing.T) proxy.OwnedRulesPlan {
	t.Helper()
	p, err := proxy.PlanOwnedRules(testInput())
	if err != nil {
		t.Fatal(err)
	}
	return p
}
func absentChain() ([]byte, error) {
	return []byte("iptables: No chain/target/match by that name.\n"), errors.New("exit status 1")
}
func idleRunner(_ context.Context, a []string) ([]byte, error) {
	if len(a) == 7 && a[5] == "-S" {
		return []byte(a[0] + ": No chain/target/match by that name.\n"), errors.New("exit status 1")
	}
	return nil, nil
}
func testController(t *testing.T, runner Runner) *Controller {
	t.Helper()
	c, err := New(t.TempDir(), runner)
	if err != nil {
		t.Fatal(err)
	}
	c.tableNames = func() (map[string]int, error) {
		return map[string]int{"main": 254, "local": 255, "default": 253, "capture": proxy.CaptureTable}, nil
	}
	return c
}
func TestApplyCleanupAndRecover(t *testing.T) {
	var calls [][]string
	runner := func(ctx context.Context, a []string) ([]byte, error) {
		calls = append(calls, append([]string(nil), a...))
		return idleRunner(ctx, a)
	}
	dir := t.TempDir()
	c, err := New(dir, runner)
	if err != nil {
		t.Fatal(err)
	}
	s, err := c.Apply(context.Background(), testInput())
	if err != nil || !s.Active || s.State != "active" || s.CleanupPending {
		t.Fatalf("apply=%+v %v", s, err)
	}
	if _, err = c.Apply(context.Background(), testInput()); err == nil {
		t.Fatal("duplicate allowed")
	}
	recovered, err := New(dir, runner)
	if err != nil {
		t.Fatal(err)
	}
	if s := recovered.Status(); s.Active || !s.CleanupPending {
		t.Fatalf("restart cannot assume live capture: %+v", s)
	}
	if err = recovered.Cleanup(context.Background()); err != nil {
		t.Fatal(err)
	}
	if s := recovered.Status(); s.Commands != 0 || s.CleanupPending || s.Active || s.State != "inactive" {
		t.Fatalf("cleanup retained state: %+v", s)
	}
	if len(calls) < len(testPlan(t).Apply)+len(testPlan(t).Cleanup) {
		t.Fatal("missing execution")
	}
}
func TestPreflightAbsentAndUnavailable(t *testing.T) {
	for _, tc := range []struct {
		name, output string
		wantError    bool
	}{
		{"missing-chain", "iptables: No chain/target/match by that name.\n", false},
		{"unavailable", "exec: iptables: executable file not found in $PATH", true},
		{"permission", "iptables: Permission denied (you must be root)", true},
		{"table-module", "iptables: can't initialize iptables table `mangle': Table does not exist", true},
		{"timeout", "", true},
	} {
		t.Run(tc.name, func(t *testing.T) {
			mutations := 0
			c := testController(t, func(_ context.Context, a []string) ([]byte, error) {
				if len(a) >= 6 && a[5] == "-S" && (tc.wantError || len(a) == 7) {
					return []byte(tc.output), errors.New(tc.name)
				}
				if !isReadCommand(a) {
					mutations++
				}
				return nil, nil
			})
			_, err := c.Apply(context.Background(), testInput())
			if (err != nil) != tc.wantError {
				t.Fatalf("err=%v", err)
			}
			if tc.wantError && mutations != 0 {
				t.Fatal("mutated after preflight failure")
			}
		})
	}
}
func TestEmptyUncreatedFIBAccepted(t *testing.T) {
	for _, family := range []string{"ipv4", "ipv6"} {
		t.Run(family, func(t *testing.T) {
			c := testController(t, func(ctx context.Context, a []string) ([]byte, error) {
				if a[0] == "ip" && a[2] == "route" && a[3] == "show" {
					return []byte("Error: " + family + ": FIB table does not exist.\nDump terminated\n"), errors.New("exit status 2")
				}
				return idleRunner(ctx, a)
			})
			if _, err := c.Apply(context.Background(), testInput()); err != nil {
				t.Fatal(err)
			}
		})
	}
}
func TestPreflightRejectsOccupiedResources(t *testing.T) {
	for _, tc := range []struct {
		name, route, rule, mangle string
		chain                     bool
	}{
		{name: "table", route: "local default dev lo"},
		{name: "priority", rule: "16500: from all lookup main"},
		{name: "numeric-table", rule: "12000: from all lookup 16500"},
		{name: "named-table", rule: "12000: from all lookup capture"},
		{name: "mark", rule: "12000: from all fwmark 0x4000/0x4000 lookup main"},
		{name: "zero-mark", rule: "12000: from all fwmark 0x0/0x4000 lookup main"},
		{name: "mark-default-mask", rule: "12000: from all fwmark 0x1 lookup main"},
		{name: "chain", chain: true},
		{name: "iptables-mark", mangle: "-A PREROUTING -j MARK --set-xmark 0x4000/0x4000"},
		{name: "iptables-or", mangle: "-A PREROUTING -j MARK --or-mark 0x4000"},
		{name: "iptables-and", mangle: "-A PREROUTING -j MARK --and-mark 0xffffbfff"},
		{name: "iptables-xor", mangle: "-A PREROUTING -j MARK --xor-mark 0x4000"},
		{name: "connmark", mangle: "-A PREROUTING -j CONNMARK --restore-mark --nfmask 0xffffffff --ctmask 0xffffffff"},
		{name: "tproxy", mangle: "-A PREROUTING -j TPROXY --tproxy-mark 0x4000/0x4000"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			c := testController(t, func(ctx context.Context, a []string) ([]byte, error) {
				if a[0] == "ip" && a[2] == "route" {
					return []byte(tc.route), nil
				}
				if a[0] == "ip" && a[2] == "rule" {
					return []byte(tc.rule), nil
				}
				if len(a) == 6 && a[5] == "-S" {
					return []byte(tc.mangle), nil
				}
				if len(a) == 7 && a[5] == "-S" && tc.chain {
					return nil, nil
				}
				return idleRunner(ctx, a)
			})
			if _, err := c.Apply(context.Background(), testInput()); err == nil {
				t.Fatal("collision accepted")
			}
		})
	}
}
func TestPreflightAllowsDisjointFirmwareMarks(t *testing.T) {
	c := testController(t, func(ctx context.Context, a []string) ([]byte, error) {
		if a[0] == "ip" && a[2] == "rule" {
			return []byte("12000: from all fwmark 0x100/0x3f00 lookup main\n"), nil
		}
		if len(a) == 6 && a[5] == "-S" {
			return []byte("-A PREROUTING -j MARK --set-xmark 0x10000/0xffff8000\n-A PREROUTING -j MARK --or-mark 0xf0\n-A PREROUTING -j MARK --and-mark 0xffffff0f\n-A PREROUTING -j CONNMARK --restore-mark --nfmask 0xffff8000 --ctmask 0xffff8000\n"), nil
		}
		return idleRunner(ctx, a)
	})
	if _, err := c.Apply(context.Background(), testInput()); err != nil {
		t.Fatal(err)
	}
}
func TestCleanupFailureRetainsLiveStateAndRetries(t *testing.T) {
	fail := false
	c := testController(t, func(ctx context.Context, a []string) ([]byte, error) {
		if fail && len(a) > 5 && a[5] == "-D" {
			return []byte("xtables lock busy"), errors.New("exit status 4")
		}
		return idleRunner(ctx, a)
	})
	if _, err := c.Apply(context.Background(), testInput()); err != nil {
		t.Fatal(err)
	}
	fail = true
	if err := c.Cleanup(context.Background()); err == nil {
		t.Fatal("failure hidden")
	}
	if s := c.Status(); !s.Active || !s.CleanupPending || s.State != "cleanup-pending" {
		t.Fatalf("false inactive state: %+v", s)
	}
	if _, err := os.Stat(c.path); err != nil {
		t.Fatal("lost recovery journal")
	}
	fail = false
	if err := c.Cleanup(context.Background()); err != nil {
		t.Fatal(err)
	}
	if c.Status().Active || c.Status().CleanupPending {
		t.Fatal("successful retry retained state")
	}
}
func TestPartialApplyRollsBackEveryCommand(t *testing.T) {
	for _, pending := range []bool{false, true} {
		t.Run(map[bool]string{false: "clean", true: "pending"}[pending], func(t *testing.T) {
			cleaned := 0
			c := testController(t, func(ctx context.Context, a []string) ([]byte, error) {
				if len(a) > 5 && a[5] == "-N" {
					return []byte("TPROXY kernel unavailable"), errors.New("apply failure")
				}
				if len(a) > 5 && (a[5] == "-D" || a[5] == "-F" || a[5] == "-X") {
					cleaned++
					if pending {
						return []byte("permission denied"), errors.New("exit status 1")
					}
				}
				return idleRunner(ctx, a)
			})
			s, err := c.Apply(context.Background(), testInput())
			var diagnostic *CommandError
			if err == nil || !errors.As(err, &diagnostic) || !strings.Contains(err.Error(), "TPROXY kernel unavailable") {
				t.Fatalf("lost diagnostic: %v", err)
			}
			if s.Active || s.CleanupPending != pending {
				t.Fatalf("status=%+v", s)
			}
			if cleaned != len(testPlan(t).Ownership.Chains)*2+len(testPlan(t).Ownership.Chains) {
				t.Fatalf("not every owned iptables cleanup attempted: %d", cleaned)
			}
		})
	}
}
func TestFirstRouteFailureDoesNotLeaveAbsentJournal(t *testing.T) {
	c := testController(t, func(ctx context.Context, a []string) ([]byte, error) {
		if a[0] == "ip" && a[2] == "route" && a[3] == "add" {
			return []byte("RTNETLINK answers: Operation not permitted"), errors.New("exit status 2")
		}
		if a[0] == "ip" && a[3] == "del" {
			return []byte("RTNETLINK answers: No such process"), errors.New("exit status 2")
		}
		if len(a) > 5 && (a[5] == "-D" || a[5] == "-F" || a[5] == "-X") {
			return absentChain()
		}
		return idleRunner(ctx, a)
	})
	if _, err := c.Apply(context.Background(), testInput()); err == nil {
		t.Fatal("failure expected")
	}
	if c.Status().Commands != 0 || c.Status().CleanupPending {
		t.Fatal("safe partial rollback retained journal")
	}
}
func TestAbsentIsCommandSpecific(t *testing.T) {
	for _, tc := range []struct {
		a    []string
		out  string
		want bool
	}{
		{[]string{"ip", "-4", "route", "del", "local", "0.0.0.0/0", "dev", "lo", "table", "16500"}, "RTNETLINK answers: No such process", true},
		{[]string{"ip", "-4", "rule", "del", "priority", "16500"}, "RTNETLINK answers: No such file or directory", true},
		{[]string{"iptables", "-w", "5", "-t", "mangle", "-F", "B6P_V4_CAPTURE"}, "iptables: No chain/target/match by that name.", true},
		{[]string{"iptables", "-w", "5", "-t", "mangle", "-D", "PREROUTING"}, "iptables: Bad rule (does a matching rule exist in that chain?).", true},
		{[]string{"iptables", "-w", "5", "-t", "mangle", "-F", "B6P_V4_CAPTURE"}, "iptables: Could not load match `TPROXY': No such file or directory", false},
		{[]string{"ip", "-4", "route", "del", "local", "0.0.0.0/0"}, "Cannot find device lo", false},
		{[]string{"ip", "-4", "route", "add", "local", "0.0.0.0/0"}, "RTNETLINK answers: No such process", false},
	} {
		if got := resourceAbsent(tc.a, []byte(tc.out), errors.New("exit")); got != tc.want {
			t.Errorf("%v %q: %v", tc.a, tc.out, got)
		}
	}
}
func TestJournalRejectsUnownedCleanupAndOwnership(t *testing.T) {
	for _, mode := range []string{"global-route", "global-policy", "numeric-hook", "other-chain", "mark", "table", "priority", "empty", "family", "input-mismatch"} {
		t.Run(mode, func(t *testing.T) {
			p := testPlan(t)
			switch mode {
			case "global-route":
				p.Cleanup[0] = []string{"ip", "-4", "route", "flush", "table", "main"}
			case "global-policy":
				p.Cleanup[0] = []string{"iptables", "-P", "FORWARD", "DROP"}
			case "numeric-hook":
				p.Cleanup[0] = []string{"iptables", "-w", "5", "-t", "nat", "-D", "PREROUTING", "1"}
			case "other-chain":
				p.Cleanup[0] = []string{"iptables", "-w", "5", "-t", "nat", "-F", "B6P_FOREIGN"}
			case "mark":
				p.Ownership.Mark = 0x8000
			case "table":
				p.Ownership.RouteTable++
			case "priority":
				p.Ownership.RulePriority++
			case "empty":
				p.Cleanup = nil
			case "family":
				p.Ownership.RouteFamilies = []int{6}
			}
			dir := t.TempDir()
			var value any = p
			if mode == "input-mismatch" {
				value = journal{OwnedRulesPlan: p, Input: func() *proxy.RulesPlanInput { in := testInput(); in.ClientIPv4 = "192.0.2.11"; return &in }()}
			}
			raw, err := json.Marshal(value)
			if err != nil {
				t.Fatal(err)
			}
			if err = os.WriteFile(filepath.Join(dir, "capture-journal.json"), raw, 0600); err != nil {
				t.Fatal(err)
			}
			calls := 0
			if _, err = New(dir, func(context.Context, []string) ([]byte, error) { calls++; return nil, nil }); err == nil {
				t.Fatal("unsafe journal accepted")
			}
			if calls != 0 {
				t.Fatal("journal validation executed commands")
			}
		})
	}
}
func TestLegacyJournalIsRecompiledNeverReplaysStoredApply(t *testing.T) {
	dir := t.TempDir()
	p := testPlan(t)
	p.Apply = [][]string{{"sh", "-c", "bad"}} // legacy Apply is discarded; only exact compiled cleanup is recoverable.
	raw, _ := json.Marshal(p)
	if err := os.WriteFile(filepath.Join(dir, "capture-journal.json"), raw, 0600); err != nil {
		t.Fatal(err)
	}
	c, err := New(dir, idleRunner)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(c.plan, testPlanPointer(t)) {
		t.Fatal("legacy journal was not recompiled")
	}
	if err := c.Cleanup(context.Background()); err != nil {
		t.Fatal(err)
	}
}
func testPlanPointer(t *testing.T) *proxy.OwnedRulesPlan { p := testPlan(t); return &p }
func TestCompilerCommandsValidate(t *testing.T) {
	for _, mode := range []proxy.IPv6Mode{proxy.IPv6Direct, proxy.IPv6Follow, proxy.IPv6Block} {
		in := testInput()
		in.IPv6 = mode
		in.ClientIPv6 = "2001:db8::10"
		in.EndpointIPs = []string{"203.0.113.4", "2001:db8::4"}
		in.ManagementIPs = []string{"192.0.2.1"}
		in.FakeIP = true
		p, err := proxy.PlanOwnedRules(in)
		if err != nil {
			t.Fatal(err)
		}
		c := testController(t, idleRunner)
		c.plan = &p
		for _, a := range append(p.Apply, p.Cleanup...) {
			if err := c.approvedCommand(a); err != nil {
				t.Fatalf("%v: %v", a, err)
			}
		}
	}
}
func TestRejectExecutableAndUnownedMutations(t *testing.T) {
	for _, a := range [][]string{{"sh", "-c", "id"}, {"iptables", "-F", "FORWARD"}, {"ip", "x\n"}, {"iptables"}, {"ip", "-4", "route", "flush", "table", "main"}, {"iptables", "-w", "5", "-t", "filter", "-P", "FORWARD", "DROP"}, {"iptables", "-w", "5", "-t", "nat", "-D", "PREROUTING", "1"}, {"iptables", "-w", "5", "-t", "mangle", "-F", "B6P_FOREIGN"}} {
		if validate(a) == nil {
			t.Errorf("unsafe %v", a)
		}
	}
}
func TestRunnerCannotMutateOwnedIntent(t *testing.T) {
	mutated := false
	c := testController(t, func(ctx context.Context, a []string) ([]byte, error) {
		out, err := idleRunner(ctx, a)
		if len(a) > 5 && a[5] == "-N" {
			a[6] = "B6P_FOREIGN"
			mutated = true
		}
		return out, err
	})
	if _, err := c.Apply(context.Background(), testInput()); err != nil {
		t.Fatal(err)
	}
	if !mutated || !reflect.DeepEqual(c.plan, testPlanPointer(t)) {
		t.Fatal("runner changed held intent")
	}
}
func TestReconcileObservesHooksRoutesAndMissingResources(t *testing.T) {
	missing := false
	c := testController(t, idleRunner)
	p := testPlan(t)
	c.plan = &p
	c.cleanupPending = true
	// Reconcile is read-only and checks generated rules plus exact client hooks.
	c.runner = func(_ context.Context, a []string) ([]byte, error) {
		if a[0] == "ip" && a[2] == "route" {
			if missing {
				return nil, nil
			}
			return []byte("local default dev lo scope host\n"), nil
		}
		if a[0] == "ip" && a[2] == "rule" {
			if missing {
				return nil, nil
			}
			return []byte("16500: from 192.0.2.10 iif br-lan fwmark 0x4000/0x4000 lookup 16500\n"), nil
		}
		if a[5] == "-C" {
			if missing {
				return []byte("iptables: Bad rule (does a matching rule exist in that chain?)."), errors.New("exit status 1")
			}
			return nil, nil
		}
		if a[5] == "-S" {
			if missing {
				return absentChain()
			}
			var lines []string
			for _, cmd := range p.Apply {
				if len(cmd) > 6 && cmd[5] == "-A" && cmd[6] == a[6] {
					lines = append(lines, strings.Join(cmd[5:], " "))
				}
			}
			return []byte(strings.Join(lines, "\n")), nil
		}
		t.Fatalf("reconcile mutates: %v", a)
		return nil, nil
	}
	s, err := c.Reconcile(context.Background())
	if err != nil || !s.Active || s.CleanupPending {
		t.Fatalf("ready: %+v %v", s, err)
	}
	missing = true
	s, err = c.Reconcile(context.Background())
	if err == nil || s.Active || !s.CleanupPending {
		t.Fatalf("missing: %+v %v", s, err)
	}
}
func TestDiagnosticOutputBounded(t *testing.T) {
	c := testController(t, func(context.Context, []string) ([]byte, error) {
		return []byte(strings.Repeat("x", 100000)), errors.New("failure")
	})
	_, err := c.Apply(context.Background(), testInput())
	var commandErr *CommandError
	if !errors.As(err, &commandErr) || len(commandErr.Output) > 65536 {
		t.Fatalf("unbounded diagnostic: %T", err)
	}
}

func TestCleanupJournalRemovalFailureRetainsPending(t *testing.T) {
	c := testController(t, idleRunner)
	if _, err := c.Apply(context.Background(), testInput()); err != nil {
		t.Fatal(err)
	}
	if err := os.Remove(c.path); err != nil {
		t.Fatal(err)
	}
	if err := os.Mkdir(c.path, 0700); err != nil {
		t.Fatal(err)
	}
	child := filepath.Join(c.path, "not-empty")
	if err := os.WriteFile(child, []byte("x"), 0600); err != nil {
		t.Fatal(err)
	}
	if err := c.Cleanup(context.Background()); err == nil {
		t.Fatal("journal removal failed but accepted")
	}
	if s := c.Status(); !s.CleanupPending || s.Commands == 0 {
		t.Fatalf("lost retry state: %+v", s)
	}
	if err := os.Remove(child); err != nil {
		t.Fatal(err)
	}
	if err := c.Cleanup(context.Background()); err != nil {
		t.Fatal(err)
	}
	if c.Status().Active || c.Status().CleanupPending {
		t.Fatal("retry retained state")
	}
}
func TestCanceledApplyUsesFreshRollbackContext(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cleaned := 0
	c := testController(t, func(callCtx context.Context, a []string) ([]byte, error) {
		if len(a) > 5 && a[5] == "-N" {
			cancel()
			return nil, context.Canceled
		}
		if !isReadCommand(a) && (len(a) > 5 && (a[5] == "-D" || a[5] == "-F" || a[5] == "-X") || a[0] == "ip" && a[3] == "del") {
			if callCtx.Err() != nil {
				t.Fatal("rollback inherited canceled apply context")
			}
			cleaned++
		}
		return idleRunner(callCtx, a)
	})
	_, err := c.Apply(ctx, testInput())
	if !errors.Is(err, context.Canceled) || cleaned != len(testPlan(t).Cleanup) {
		t.Fatalf("rollback=%d err=%v", cleaned, err)
	}
	if c.Status().CleanupPending {
		t.Fatal("successful canceled rollback retained pending")
	}
}
func TestAlreadyCanceledApplyDoesNotExecute(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	calls := 0
	c := testController(t, func(context.Context, []string) ([]byte, error) { calls++; return nil, nil })
	if _, err := c.Apply(ctx, testInput()); !errors.Is(err, context.Canceled) {
		t.Fatalf("err=%v", err)
	}
	if calls != 0 {
		t.Fatal("canceled context executed runner")
	}
}
func TestCanceledAbsenceIsNotIgnored(t *testing.T) {
	if resourceAbsent([]string{"iptables", "-w", "5", "-t", "mangle", "-F", "B6P_V4_CAPTURE"}, []byte("iptables: No chain/target/match by that name."), context.DeadlineExceeded) {
		t.Fatal("deadline treated as absence")
	}
	if emptyFIB([]byte("Error: ipv4: FIB table does not exist."), context.Canceled) {
		t.Fatal("cancellation treated as empty table")
	}
}
func TestPreflightReadsIPv6WhenFollowOnly(t *testing.T) {
	for _, mode := range []proxy.IPv6Mode{proxy.IPv6Follow, proxy.IPv6Block} {
		t.Run(string(mode), func(t *testing.T) {
			var ipv6Route, ipv6Mangle, ipv6Chain bool
			c := testController(t, func(ctx context.Context, a []string) ([]byte, error) {
				if a[0] == "ip" && a[1] == "-6" && a[2] == "route" && a[3] == "show" {
					ipv6Route = true
				}
				if a[0] == "ip6tables" && len(a) == 6 && a[5] == "-S" {
					ipv6Mangle = true
				}
				if a[0] == "ip6tables" && len(a) == 7 && a[5] == "-S" {
					ipv6Chain = true
				}
				return idleRunner(ctx, a)
			})
			in := testInput()
			in.IPv6 = mode
			in.ClientIPv6 = "2001:db8::10"
			if _, err := c.Apply(context.Background(), in); err != nil {
				t.Fatal(err)
			}
			follow := mode == proxy.IPv6Follow
			if ipv6Route != follow || ipv6Mangle != follow || !ipv6Chain {
				t.Fatalf("read route=%v mark=%v chain=%v", ipv6Route, ipv6Mangle, ipv6Chain)
			}
		})
	}
}
func TestJournalRoundTripEveryPolicy(t *testing.T) {
	for _, mode := range []proxy.IPv6Mode{proxy.IPv6Direct, proxy.IPv6Follow, proxy.IPv6Block} {
		t.Run(string(mode), func(t *testing.T) {
			dir := t.TempDir()
			c, err := New(dir, idleRunner)
			if err != nil {
				t.Fatal(err)
			}
			in := testInput()
			in.IPv6 = mode
			in.ClientIPv6 = "2001:db8::10"
			in.Ports = proxy.Ports{Mixed: 3080, TProxy: 3893, DNS: 2053}
			in.FakeIP = true
			in.EndpointIPs = []string{"203.0.113.2", "2001:db8::2"}
			if _, err = c.Apply(context.Background(), in); err != nil {
				t.Fatal(err)
			}
			recovered, err := New(dir, idleRunner)
			if err != nil {
				t.Fatal(err)
			}
			if !reflect.DeepEqual(c.plan, recovered.plan) {
				t.Fatal("restart lost compiled input")
			}
			if err = recovered.Cleanup(context.Background()); err != nil {
				t.Fatal(err)
			}
		})
	}
}
func TestUnownedMutationRejectedEvenWhileActive(t *testing.T) {
	c := testController(t, idleRunner)
	p := testPlan(t)
	c.plan = &p
	for _, a := range [][]string{{"ip", "-4", "route", "flush", "table", "main"}, {"iptables", "-w", "5", "-t", "nat", "-D", "PREROUTING", "1"}, {"iptables", "-w", "5", "-t", "mangle", "-F", "B6P_FOREIGN"}} {
		if c.approvedCommand(a) == nil {
			t.Errorf("unowned mutation accepted: %v", a)
		}
	}
}
func TestReconcileReadErrorRetainsUnknownLiveState(t *testing.T) {
	c := testController(t, idleRunner)
	p := testPlan(t)
	c.plan = &p
	c.active = true
	c.runner = func(context.Context, []string) ([]byte, error) {
		return []byte("permission denied"), errors.New("exit status 1")
	}
	s, err := c.Reconcile(context.Background())
	if err == nil || !s.Active || !s.CleanupPending || s.State != "cleanup-pending" {
		t.Fatalf("false clean inactivity: %+v %v", s, err)
	}
}
func TestPolicyRuleReadyRequiresExactScope(t *testing.T) {
	own := testPlan(t).Ownership
	names := map[string]int{"capture": 16500, "main": 254}
	for _, tc := range []struct {
		line string
		want bool
	}{
		{"16500: from 192.0.2.10 iif br-lan fwmark 0x4000/0x4000 lookup capture", true},
		{"16500: from 192.0.2.10/32 iif br-lan fwmark 0x4000/0x4000 lookup 16500", true},
		{"16500: from 192.0.2.0/24 iif br-lan fwmark 0x4000/0x4000 lookup 16500", false},
		{"16500: from 192.0.2.10 iif br-lan fwmark 0x4000 lookup 16500", false},
		{"16500: from 192.0.2.10 iif br-lan fwmark 0x4000/0x4000 lookup main", false},
		{"16500: not from 192.0.2.10 iif br-lan fwmark 0x4000/0x4000 lookup 16500", false},
	} {
		if got := hasOwnedRule([]byte(tc.line), own, 4, names); got != tc.want {
			t.Errorf("%q: %v", tc.line, got)
		}
	}
}
func TestMarkListingQuotedCommentsAndMalformedNumbers(t *testing.T) {
	if err := markCollisions([]byte(`-A PREROUTING -m comment --comment "ignore --set-xmark 0x4000/0x4000" -j ACCEPT`)); err != nil {
		t.Fatal(err)
	}
	for _, line := range []string{`-A PREROUTING -j MARK --set-xmark bad`, `-A PREROUTING -j MARK --set-xmark`, `-A PREROUTING -g MARK --set-xmark 0x4000/0x4000`, `-A PREROUTING -j MARK --set-xmark 0x4000/0x0`} {
		if err := markCollisions([]byte(line)); err == nil {
			t.Errorf("unsafe mark accepted: %s", line)
		}
	}
	if err := ruleCollisions([]byte("12000: from all fwmark bad lookup main"), map[string]int{"main": 254}); err == nil {
		t.Fatal("malformed policy mark accepted")
	}
	if err := ruleCollisions([]byte("12000: from all lookup unknown"), map[string]int{}); err == nil {
		t.Fatal("unknown table alias accepted")
	}
}

func TestRouterDNSIntentRoundTripAndOwnedCommands(t *testing.T) {
	dir := t.TempDir()
	c, err := New(dir, idleRunner)
	if err != nil {
		t.Fatal(err)
	}
	in := testInput()
	in.ManagementIPs = []string{"192.0.2.1"}
	in.RouterDNSAddresses = []string{"192.0.2.1"}
	if _, err = c.Apply(context.Background(), in); err != nil {
		t.Fatal(err)
	}
	recovered, err := New(dir, idleRunner)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(c.plan, recovered.plan) {
		t.Fatal("restart lost selected-client router DNS intent")
	}
	for _, a := range recovered.plan.Apply {
		if err = recovered.approvedCommand(a); err != nil {
			t.Fatalf("compiler DNS argv rejected: %v", err)
		}
	}
	if err = recovered.Cleanup(context.Background()); err != nil {
		t.Fatal(err)
	}
}
func TestInspectionCannotSilentlyTruncate(t *testing.T) {
	c := testController(t, func(context.Context, []string) ([]byte, error) { return []byte(strings.Repeat("x", 64<<10)), nil })
	if _, err := c.execute(context.Background(), []string{"iptables", "-w", "5", "-t", "mangle", "-S"}); err == nil {
		t.Fatal("truncated inspection could hide colliding mark")
	}
}

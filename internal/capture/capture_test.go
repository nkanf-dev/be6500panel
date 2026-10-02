package capture

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"testing"

	"be6500panel/internal/proxy"
)

func testPlan(t *testing.T) proxy.OwnedRulesPlan {
	t.Helper()
	p, e := proxy.PlanOwnedRules(proxy.RulesPlanInput{ClientIPv4: "192.0.2.10", LANInterface: "br-lan", Ports: proxy.Ports{Mixed: 2080, TProxy: 7893, DNS: 1053}, IPv6: proxy.IPv6Direct, Failure: proxy.FailureDirect})
	if e != nil {
		t.Fatal(e)
	}
	return p
}
func TestApplyCleanupAndRecover(t *testing.T) {
	var calls [][]string
	runner := func(_ context.Context, args []string) ([]byte, error) {
		calls = append(calls, append([]string{}, args...))
		for _, a := range args {
			if a == "-S" {
				return []byte("No chain/target/match by that name"), errors.New("absent")
			}
		}
		return nil, nil
	}
	dir := t.TempDir()
	c, e := New(dir, runner)
	if e != nil {
		t.Fatal(e)
	}
	p := testPlan(t)
	s, e := c.Apply(context.Background(), p)
	if e != nil || !s.Active {
		t.Fatalf("apply=%+v %v", s, e)
	}
	if _, e = c.Apply(context.Background(), p); e == nil {
		t.Fatal("duplicate allowed")
	}
	recovered, e := New(dir, runner)
	if e != nil {
		t.Fatal(e)
	}
	if recovered.Status().Active {
		t.Fatal("restart cannot assume live capture")
	}
	if e = recovered.Cleanup(context.Background()); e != nil {
		t.Fatal(e)
	}
	if recovered.Status().Commands != 0 {
		t.Fatal("cleanup retained state")
	}
	if len(calls) < len(p.Apply)+len(p.Cleanup) {
		t.Fatal("missing command execution")
	}
}
func TestPartialApplyRollsBackAllOwnedCommands(t *testing.T) {
	count := 0
	clean := 0
	runner := func(_ context.Context, a []string) ([]byte, error) {
		for _, v := range a {
			if v == "-S" {
				return nil, errors.New("absent")
			}
		}
		if a[0] == "ip" && a[2] == "route" && a[3] == "show" {
			return nil, nil
		}
		if a[0] == "ip" && a[2] == "rule" && a[3] == "show" {
			return nil, nil
		}
		if strings.Contains(strings.Join(a, " "), " -N ") {
			count++
			if count == 2 {
				return nil, errors.New("fail")
			}
		}
		if strings.Contains(strings.Join(a, " "), " -D ") || strings.Contains(strings.Join(a, " "), " -F ") || strings.Contains(strings.Join(a, " "), " -X ") {
			clean++
		}
		return nil, nil
	}
	c, e := New(t.TempDir(), runner)
	if e != nil {
		t.Fatal(e)
	}
	s, e := c.Apply(context.Background(), testPlan(t))
	if e == nil || s.Active {
		t.Fatal("partial apply was accepted")
	}
	if clean == 0 {
		t.Fatal("no cleanup")
	}
}
func TestPreflightRejectsOccupiedResources(t *testing.T) {
	for _, mode := range []string{"table", "priority", "chain"} {
		t.Run(mode, func(t *testing.T) {
			runner := func(_ context.Context, a []string) ([]byte, error) {
				s := strings.Join(a, " ")
				if mode == "table" && strings.Contains(s, "route show") {
					return []byte("local default dev lo"), nil
				}
				if mode == "priority" && strings.Contains(s, "rule show") {
					return []byte(fmt.Sprintf("%d: from all lookup main", proxy.CapturePriority)), nil
				}
				if strings.Contains(s, " -S ") {
					if mode == "chain" {
						return nil, nil
					}
					return nil, errors.New("absent")
				}
				return nil, nil
			}
			c, e := New(t.TempDir(), runner)
			if e != nil {
				t.Fatal(e)
			}
			if _, e = c.Apply(context.Background(), testPlan(t)); e == nil {
				t.Fatal("collision accepted")
			}
		})
	}
}
func TestRejectExecutableAndGlobalFlush(t *testing.T) {
	for _, args := range [][]string{{"sh", "-c", "id"}, {"iptables", "-F", "FORWARD"}, {"ip", "x\n"}, {"iptables"}} {
		if validate(args) == nil {
			t.Fatal("unsafe", args)
		}
	}
}

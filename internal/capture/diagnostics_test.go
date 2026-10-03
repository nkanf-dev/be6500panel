package capture

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os/exec"
	"reflect"
	"slices"
	"strings"
	"sync"
	"testing"
	"time"

	"be6500panel/internal/proxy"
)

// All diagnostic fixtures run through a fake Runner. No test invokes a firewall,
// opens a socket, or captures packets.
func diagnosticFixture(chain string, rows ...string) []byte {
	return []byte("Chain " + chain + " (1 references)\n" +
		" pkts bytes target prot opt in out source destination\n" + strings.Join(rows, "\n") + "\n")
}

func diagnosticRunner(ctx context.Context, argv []string) ([]byte, error) {
	if argv[0] == "ip" {
		if argv[2] == "route" {
			return []byte("local default dev lo scope host\n"), nil
		}
		source := "192.0.2.10/32"
		if argv[1] == "-6" {
			source = "2001:db8::10/128"
		}
		return []byte("0: from all lookup local\n16500: from " + source + " iif br-lan fwmark 0x4000/0x4000 lookup 16500\n"), nil
	}
	return diagnosticFixture(argv[6], "0 0 RETURN all -- * * 0.0.0.0/0 0.0.0.0/0"), nil
}

func diagnosticController(t *testing.T, runner Runner) *Controller {
	t.Helper()
	c := testController(t, runner)
	plan := testPlan(t)
	c.plan, c.active = &plan, true
	return c
}

func TestDiagnosticsFixedReadsQSDKCountersAndSavedScope(t *testing.T) {
	var calls [][]string
	c := diagnosticController(t, func(ctx context.Context, argv []string) ([]byte, error) {
		calls = append(calls, slices.Clone(argv))
		if argv[0] == "iptables" && argv[4] == "mangle" {
			return diagnosticFixture(argv[6],
				" 12 900 RETURN tcp -- * * 0.0.0.0/0 0.0.0.0/0 tcp dpt:53",
				" 18446744073709551615 18446744073709551615 TPROXY udp -- * * 0.0.0.0/0 0.0.0.0/0 TPROXY redirect 127.0.0.1:7893 mark 0x4000/0x4000"), nil
		}
		if argv[0] == "iptables" && argv[4] == "nat" {
			return diagnosticFixture(argv[6], "2 180 REDIRECT udp -- * * 0.0.0.0/0 0.0.0.0/0 udp dpt:53 redir ports 1053"), nil
		}
		return diagnosticRunner(ctx, argv)
	})
	c.clients = []Client{{IP: "192.0.2.99", Hostname: "fresh-not-installed"}}
	before := c.Status()
	d := c.Diagnostics(context.Background())
	if d.State != "complete" || d.Error != "" || d.MeasuredAt.IsZero() || d.DurationMS < 0 {
		t.Fatalf("report=%+v", d)
	}
	if !reflect.DeepEqual(d.InstalledClients, installedClients(c.plan.Ownership)) || !reflect.DeepEqual(before, c.Status()) {
		t.Fatal("diagnostics used desired observations or changed controller state")
	}
	want := [][]string{
		{"iptables", "-w", "1", "-t", "mangle", "-L", "B6P_V4_CAPTURE", "-n", "-v", "-x"},
		{"iptables", "-w", "1", "-t", "nat", "-L", "B6P_V4_DNS", "-n", "-v", "-x"},
		routeShow(4), ruleShow(4),
	}
	if !reflect.DeepEqual(calls, want) {
		t.Fatalf("unexpected command contract: %v", calls)
	}
	rows := d.Chains[0].Counters
	if len(rows) != 2 || rows[0].Packets != 12 || rows[0].Bytes != 900 || rows[0].Target != "RETURN" || rows[0].Protocol != "tcp" || rows[0].DestinationPort == nil || *rows[0].DestinationPort != 53 || rows[0].ListenerPort != nil {
		t.Fatalf("DNS return counters=%+v", rows)
	}
	if rows[1].Packets != ^uint64(0) || rows[1].Bytes != ^uint64(0) || rows[1].ListenerPort == nil || *rows[1].ListenerPort != 7893 {
		t.Fatalf("TPROXY counters=%+v", rows[1])
	}
	nat := d.Chains[1].Counters[0]
	if nat.ListenerPort == nil || *nat.ListenerPort != 1053 || nat.DestinationPort == nil || *nat.DestinationPort != 53 {
		t.Fatalf("NAT counters=%+v", nat)
	}
	if len(d.Routing) != 1 || d.Routing[0].LocalRoute.Present == nil || !*d.Routing[0].LocalRoute.Present || d.Routing[0].PolicyRules.Present == nil || !*d.Routing[0].PolicyRules.Present {
		t.Fatalf("routing=%+v", d.Routing)
	}
}

func TestDiagnosticsIPv6AndFilterIsExplicitlyUnsupported(t *testing.T) {
	for _, mode := range []proxy.IPv6Mode{proxy.IPv6Follow, proxy.IPv6Block} {
		t.Run(string(mode), func(t *testing.T) {
			calls := 0
			c := diagnosticController(t, func(ctx context.Context, argv []string) ([]byte, error) {
				calls++
				if len(argv) > 4 && argv[4] == "filter" {
					t.Fatal("filter read is outside approved scope")
				}
				if argv[0] == "ip6tables" {
					if argv[4] == "nat" {
						// QSDK ip6tables can omit the empty opt field in rows.
						return diagnosticFixture(argv[6], "3 240 REDIRECT udp * * ::/0 ::/0 udp dpt:53 redir ports 1053"), nil
					}
					return diagnosticFixture(argv[6], "7 560 TPROXY tcp * * ::/0 ::/0 TPROXY redirect ::1:7893 mark 0x4000/0x4000"), nil
				}
				return diagnosticRunner(ctx, argv)
			})
			in := testInput()
			in.ClientIPv6, in.IPv6 = "2001:db8::10", mode
			plan, err := proxy.PlanOwnedRules(in)
			if err != nil {
				t.Fatal(err)
			}
			c.plan = &plan
			d := c.Diagnostics(context.Background())
			if mode == proxy.IPv6Block {
				if d.State != "partial" || calls != 4 || len(d.Chains) != 3 || d.Chains[2].State != "unsupported" || d.Chains[2].Counters != nil {
					t.Fatalf("block=%+v calls=%d", d, calls)
				}
			} else if d.State != "complete" || calls != 8 || len(d.Chains[2].Counters) != 1 || d.Chains[2].Counters[0].ListenerPort == nil || *d.Chains[2].Counters[0].ListenerPort != 7893 {
				t.Fatalf("follow=%+v calls=%d", d, calls)
			}
		})
	}
}

func TestDiagnosticsFailuresAndNoRawOutputLeakage(t *testing.T) {
	secret := "private.example.invalid 192.0.2.200 UUID=do-not-project"
	valid := "4 300 TPROXY tcp -- * * 0.0.0.0/0 0.0.0.0/0 TPROXY redirect 127.0.0.1:7893"
	for _, tc := range []struct {
		name, state, code string
		output            []byte
		err               error
		rows              int
	}{
		{"missing", "missing", "capture_diagnostic_chain_missing", []byte("iptables: No chain/target/match by that name.\n"), errors.New(secret), 0},
		{"unavailable", "unavailable", "capture_diagnostic_command_failed", []byte(secret), errors.New(secret), 0},
		{"executable", "unavailable", "capture_diagnostic_unavailable", nil, &exec.Error{Name: secret, Err: exec.ErrNotFound}, 0},
		{"warning", "partial", "capture_diagnostic_warning", append([]byte("Warning: "+secret+"\n"), diagnosticFixture("B6P_V4_CAPTURE", valid)...), nil, 1},
		{"partial-exit", "partial", "capture_diagnostic_command_failed", diagnosticFixture("B6P_V4_CAPTURE", valid), errors.New(secret), 1},
		{"malformed-count", "partial", "capture_diagnostic_malformed", diagnosticFixture("B6P_V4_CAPTURE", valid, "1K 600 RETURN tcp -- * * 0.0.0.0/0 0.0.0.0/0 tcp dpt:53"), nil, 1},
		{"overflow-count", "partial", "capture_diagnostic_malformed", diagnosticFixture("B6P_V4_CAPTURE", valid, "18446744073709551616 1 RETURN all -- * * 0.0.0.0/0 0.0.0.0/0"), nil, 1},
		{"private-target", "unavailable", "capture_diagnostic_malformed", diagnosticFixture("B6P_V4_CAPTURE", "1 1 "+secret), nil, 0},
		{"wrong-header", "unavailable", "capture_diagnostic_malformed", []byte("Chain B6P_OTHER (1 references)\n pkts bytes target prot opt in out source destination\n" + valid), nil, 0},
		{"no-header", "unavailable", "capture_diagnostic_malformed", []byte(valid), nil, 0},
		{"negative", "unavailable", "capture_diagnostic_malformed", diagnosticFixture("B6P_V4_CAPTURE", "-1 1 RETURN all -- * * 0.0.0.0/0 0.0.0.0/0"), nil, 0},
		{"bad-port", "unavailable", "capture_diagnostic_malformed", diagnosticFixture("B6P_V4_CAPTURE", "1 1 RETURN tcp -- * * 0.0.0.0/0 0.0.0.0/0 tcp dpt:999999"), nil, 0},
	} {
		t.Run(tc.name, func(t *testing.T) {
			c := diagnosticController(t, func(ctx context.Context, argv []string) ([]byte, error) {
				if argv[0] == "iptables" && argv[4] == "mangle" {
					return tc.output, tc.err
				}
				return diagnosticRunner(ctx, argv)
			})
			d := c.Diagnostics(context.Background())
			chain := d.Chains[0]
			if d.State != "partial" || chain.State != tc.state || chain.Error != tc.code || len(chain.Counters) != tc.rows {
				t.Fatalf("chain=%+v report=%+v", chain, d)
			}
			if tc.rows == 0 && chain.Counters != nil {
				t.Fatal("unavailable counts became empty/zero counts")
			}
			raw, err := json.Marshal(d)
			if err != nil || strings.Contains(string(raw), secret) || strings.Contains(string(raw), "argv") || strings.Contains(string(raw), "0.0.0.0/0") {
				t.Fatalf("private command data leaked: %s %v", raw, err)
			}
		})
	}
}

func TestDiagnosticsEmptyAndDuplicateRowsHaveHonestCounts(t *testing.T) {
	for _, duplicate := range []bool{false, true} {
		c := diagnosticController(t, func(ctx context.Context, argv []string) ([]byte, error) {
			if argv[0] == "iptables" && argv[4] == "mangle" {
				if !duplicate {
					return diagnosticFixture(argv[6]), nil
				}
				row := "3 180 RETURN tcp -- * * 0.0.0.0/0 0.0.0.0/0 tcp dpt:53"
				return diagnosticFixture(argv[6], row, row), nil
			}
			return diagnosticRunner(ctx, argv)
		})
		d := c.Diagnostics(context.Background())
		if d.State != "complete" || d.Chains[0].Counters == nil {
			t.Fatalf("known empty chain is not missing: %+v", d)
		}
		if duplicate && (len(d.Chains[0].Counters) != 2 || d.Chains[0].Counters[0].Row != 1 || d.Chains[0].Counters[1].Row != 2) {
			t.Fatal("distinct identical rules lost their row identity")
		}
	}
}

func TestDiagnosticsFiniteOutputAndCardinality(t *testing.T) {
	row := "1 1 RETURN all -- * * 0.0.0.0/0 0.0.0.0/0"
	for _, tc := range []struct{ name, output, code string }{
		{"rows", string(diagnosticFixture("B6P_V4_CAPTURE")) + strings.Repeat(row+"\n", diagnosticMaxRows+1), "capture_diagnostic_row_limit"},
		{"bytes", string(diagnosticFixture("B6P_V4_CAPTURE", row)) + strings.Repeat(" ", diagnosticMaxOutput), "capture_diagnostic_output_limit"},
		{"line", string(diagnosticFixture("B6P_V4_CAPTURE", row)) + strings.Repeat(" ", diagnosticMaxLine+1), "capture_diagnostic_malformed"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			c := diagnosticController(t, func(ctx context.Context, argv []string) ([]byte, error) {
				if argv[0] == "iptables" && argv[4] == "mangle" {
					return []byte(tc.output), nil
				}
				return diagnosticRunner(ctx, argv)
			})
			d := c.Diagnostics(context.Background())
			if d.State != "partial" || d.Chains[0].Error != tc.code || len(d.Chains[0].Counters) > diagnosticMaxRows {
				t.Fatalf("limits=%+v", d.Chains[0])
			}
		})
	}
}

func TestDiagnosticsTimeoutCancellationAndPartialRead(t *testing.T) {
	for _, partial := range []bool{false, true} {
		c := diagnosticController(t, func(ctx context.Context, argv []string) ([]byte, error) {
			if argv[0] == "iptables" && argv[4] == "mangle" {
				deadline, ok := ctx.Deadline()
				if !ok || time.Until(deadline) > diagnosticCommandTimeout {
					t.Error("read has no bounded deadline")
				}
				<-ctx.Done()
				if partial {
					return diagnosticFixture(argv[6], "5 300 RETURN all -- * * 0.0.0.0/0 0.0.0.0/0"), ctx.Err()
				}
				return nil, ctx.Err()
			}
			return diagnosticRunner(ctx, argv)
		})
		ctx, cancel := context.WithTimeout(context.Background(), 15*time.Millisecond)
		d := c.Diagnostics(ctx)
		cancel()
		if d.State != "partial" || d.Chains[0].Error != "capture_diagnostic_timeout" || (len(d.Chains[0].Counters) == 1) != partial {
			t.Fatalf("deadline=%+v", d)
		}
	}
	calls := 0
	c := diagnosticController(t, func(context.Context, []string) ([]byte, error) { calls++; return nil, nil })
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	d := c.Diagnostics(ctx)
	if calls != 0 || d.Chains[0].Error != "capture_diagnostic_canceled" {
		t.Fatalf("canceled calls=%d report=%+v", calls, d)
	}
}

func TestDiagnosticsDoesNotBlockDisableAndDiscardsChangedScope(t *testing.T) {
	for _, change := range []string{"disable", "replace-same", "mutate-in-place"} {
		t.Run(change, func(t *testing.T) {
			started, release := make(chan struct{}), make(chan struct{})
			var once sync.Once
			c := diagnosticController(t, func(ctx context.Context, argv []string) ([]byte, error) {
				if len(argv) == 10 && argv[5] == "-L" {
					once.Do(func() { close(started); <-release })
					return diagnosticRunner(ctx, argv)
				}
				if argv[0] == "ip" && argv[3] == "show" {
					return diagnosticRunner(ctx, argv)
				}
				return nil, nil // Only Disable runs cleanup, never Diagnostics.
			})
			done := make(chan DatapathDiagnostics, 1)
			go func() { done <- c.Diagnostics(context.Background()) }()
			<-started
			changed := make(chan error, 1)
			go func() {
				if change == "disable" {
					changed <- c.Disable(context.Background())
					return
				}
				c.mu.Lock()
				if change == "replace-same" {
					plan := clonePlan(*c.plan)
					c.plan = &plan
				} else {
					c.plan.Ownership.ClientIPv4 = "192.0.2.11"
				}
				c.mu.Unlock()
				changed <- nil
			}()
			select {
			case err := <-changed:
				if err != nil {
					t.Fatal(err)
				}
			case <-time.After(time.Second):
				close(release)
				t.Fatal("slow diagnostics blocked Disable or saved-plan change")
			}
			close(release)
			d := <-done
			if d.State != "scope-changed" || d.Error != "capture_diagnostic_scope_changed" || d.InstalledClients != nil || d.Chains != nil || d.Routing != nil {
				t.Fatalf("stale counters attached to another scope: %+v", d)
			}
		})
	}
}

func TestDiagnosticsRoutingFailureIsNotAbsence(t *testing.T) {
	for _, tc := range []struct {
		name, output, state string
		err                 error
		present             *bool
	}{
		{"missing", "", "missing", nil, diagnosticBool(false)},
		{"uncreated-fib", "Error: ipv4: FIB table does not exist.\nDump terminated", "missing", errors.New("exit 2"), diagnosticBool(false)},
		{"failure", "private raw route diagnostic", "unavailable", errors.New("private failure"), nil},
		{"malformed", "warning: private route", "unavailable", nil, nil},
	} {
		t.Run(tc.name, func(t *testing.T) {
			c := diagnosticController(t, func(ctx context.Context, argv []string) ([]byte, error) {
				if argv[0] == "ip" && argv[2] == "route" {
					return []byte(tc.output), tc.err
				}
				return diagnosticRunner(ctx, argv)
			})
			d := c.Diagnostics(context.Background())
			evidence := d.Routing[0].LocalRoute
			if evidence.State != tc.state || !reflect.DeepEqual(evidence.Present, tc.present) {
				t.Fatalf("route evidence=%+v", evidence)
			}
			raw, _ := json.Marshal(d)
			if strings.Contains(string(raw), "private") {
				t.Fatal("route output leaked")
			}
		})
	}
	c := diagnosticController(t, func(ctx context.Context, argv []string) ([]byte, error) {
		if argv[0] == "ip" && argv[2] == "rule" {
			return []byte(fmt.Sprintf("16500: from 192.0.2.10 iif br-lan fwmark 0x4000/0x4000 lookup %s\n", "private-alias")), nil
		}
		return diagnosticRunner(ctx, argv)
	})
	d := c.Diagnostics(context.Background())
	if d.Routing[0].PolicyRules.Present != nil || d.Routing[0].PolicyRules.Error != "capture_diagnostic_table_alias_unresolved" {
		t.Fatalf("unresolved alias reported missing: %+v", d.Routing)
	}
}

func TestDiagnosticsNoPlanDoesNotRead(t *testing.T) {
	calls := 0
	c := testController(t, func(context.Context, []string) ([]byte, error) { calls++; return nil, nil })
	d := c.Diagnostics(context.Background())
	if calls != 0 || d.State != "no-plan" || d.Chains != nil || d.InstalledClients != nil {
		t.Fatalf("no-plan=%+v calls=%d", d, calls)
	}
}

func TestDiagnosticsStrictCommandAdmission(t *testing.T) {
	for _, argv := range [][]string{
		{"iptables", "-w", "1", "-t", "mangle", "-L", "PREROUTING", "-n", "-v", "-x"},
		{"iptables", "-w", "1", "-t", "mangle", "-L", "B6P_V6_CAPTURE", "-n", "-v", "-x"},
		{"iptables", "-w", "5", "-t", "mangle", "-L", "B6P_V4_CAPTURE", "-n", "-v", "-x"},
		{"iptables", "-w", "1", "-t", "mangle", "-L", "B6P_V4_CAPTURE", "-n", "-v", "-x", "-Z"},
		{"ip6tables", "-w", "1", "-t", "filter", "-L", "B6P_V6_BLOCK", "-n", "-v", "-x"},
		{"ip", "-4", "route", "show", "table", "all"},
		{"ip", "-4", "route", "flush", "table", "16500"},
		{"sh", "-c", "iptables -L"},
	} {
		calls := 0
		_, code := diagnosticRead(context.Background(), func(context.Context, []string) ([]byte, error) { calls++; return nil, nil }, argv)
		if calls != 0 || code != "capture_diagnostic_command_unapproved" {
			t.Fatalf("unapproved read admitted %v calls=%d code=%s", argv, calls, code)
		}
	}
}

func TestDiagnosticsBusyMutexCannotExtendBudget(t *testing.T) {
	c := diagnosticController(t, diagnosticRunner)
	c.mu.Lock()
	d := c.Diagnostics(context.Background())
	c.mu.Unlock()
	if d.State != "unavailable" || d.Error != "capture_diagnostic_busy" || d.Chains != nil {
		t.Fatalf("busy report=%+v", d)
	}

	started, release, locked, unlock := make(chan struct{}), make(chan struct{}), make(chan struct{}), make(chan struct{})
	var once sync.Once
	c = diagnosticController(t, func(ctx context.Context, argv []string) ([]byte, error) {
		once.Do(func() { close(started); <-release })
		return diagnosticRunner(ctx, argv)
	})
	done := make(chan DatapathDiagnostics, 1)
	go func() { done <- c.Diagnostics(context.Background()) }()
	<-started
	go func() {
		c.mu.Lock()
		close(locked)
		<-unlock
		c.mu.Unlock()
	}()
	<-locked
	close(release)
	select {
	case d = <-done:
		close(unlock)
	case <-time.After(time.Second):
		close(unlock)
		t.Fatal("diagnostics waited for final scope lock")
	}
	if d.State != "scope-unverified" || d.Error != "capture_diagnostic_scope_unverified" || d.InstalledClients != nil || d.Chains != nil || d.Routing != nil {
		t.Fatalf("unverified scope retained counts: %+v", d)
	}
}

func TestDiagnosticsTotalTimeBudget(t *testing.T) {
	calls := 0
	c := diagnosticController(t, func(ctx context.Context, argv []string) ([]byte, error) {
		calls++
		<-ctx.Done()
		return nil, ctx.Err()
	})
	started := time.Now()
	d := c.Diagnostics(context.Background())
	if time.Since(started) > diagnosticTimeout+time.Second || calls > 3 || d.State != "partial" || d.Routing[0].PolicyRules.Error != "capture_diagnostic_timeout" {
		t.Fatalf("unbounded report=%+v calls=%d", d, calls)
	}
}

func TestDiagnosticsTimedOutAbsentOutputIsUnknown(t *testing.T) {
	c := diagnosticController(t, func(ctx context.Context, argv []string) ([]byte, error) {
		if argv[0] == "iptables" && argv[4] == "mangle" {
			return []byte("iptables: No chain/target/match by that name."), context.DeadlineExceeded
		}
		return diagnosticRunner(ctx, argv)
	})
	d := c.Diagnostics(context.Background())
	if d.Chains[0].State != "unavailable" || d.Chains[0].Error != "capture_diagnostic_timeout" || d.Chains[0].Counters != nil {
		t.Fatalf("timed out missing output treated as absence: %+v", d)
	}
}

func TestDiagnosticsUnrelatedTableAliasesAndDuplicateRules(t *testing.T) {
	for _, candidate := range []string{
		"16500: from 192.0.2.10 iif br-lan fwmark 0x4000/0x4000 lookup 16500\n",
		"16500: from 192.0.2.10 iif br-lan fwmark 0x4000/0x4000 lookup capture_alias\n",
	} {
		c := diagnosticController(t, func(ctx context.Context, argv []string) ([]byte, error) {
			if argv[0] == "ip" && argv[2] == "rule" {
				return []byte("12000: from all lookup private_firmware_alias\n" + candidate + "16500: from 192.0.2.11 iif br-lan fwmark 0x4000/0x4000 lookup unresolved_duplicate\n" + candidate), nil
			}
			return diagnosticRunner(ctx, argv)
		})
		d := c.Diagnostics(context.Background())
		evidence := d.Routing[0].PolicyRules
		if strings.Contains(candidate, "16500\n") {
			if evidence.Present == nil || !*evidence.Present {
				t.Fatalf("unrelated aliases hid proved owned rule: %+v", evidence)
			}
		} else if evidence.Present != nil || evidence.Error != "capture_diagnostic_table_alias_unresolved" {
			t.Fatalf("unknown owned alias reported absence: %+v", evidence)
		}
	}
	c := diagnosticController(t, func(ctx context.Context, argv []string) ([]byte, error) {
		if argv[0] == "ip" && argv[2] == "rule" {
			row := "16500: from 192.0.2.10 iif br-lan fwmark 0x4000/0x4000 lookup 16500\n"
			return []byte(row + row), nil
		}
		return diagnosticRunner(ctx, argv)
	})
	c.plan.Ownership.ClientIPv4, c.plan.Ownership.ClientIPv4s = "", []string{"192.0.2.10", "192.0.2.11"}
	d := c.Diagnostics(context.Background())
	if d.Routing[0].PolicyRules.Present == nil || *d.Routing[0].PolicyRules.Present {
		t.Fatalf("duplicate policy rule proved another client: %+v", d.Routing)
	}
}

func TestDiagnosticsCommentsAndSignedCounts(t *testing.T) {
	c := diagnosticController(t, func(ctx context.Context, argv []string) ([]byte, error) {
		if argv[0] == "iptables" && argv[4] == "mangle" {
			return append([]byte("# Warning: private backend details\n"), diagnosticFixture(argv[6], "0 0 RETURN all -- * * 0.0.0.0/0 0.0.0.0/0", "+1 1 RETURN all -- * * 0.0.0.0/0 0.0.0.0/0")...), nil
		}
		return diagnosticRunner(ctx, argv)
	})
	d := c.Diagnostics(context.Background())
	if d.Chains[0].State != "partial" || d.Chains[0].Error != "capture_diagnostic_malformed" || len(d.Chains[0].Counters) != 1 {
		t.Fatalf("warning/signed count=%+v", d.Chains[0])
	}
}

func TestDiagnosticsTruncatedAndMalformedRoutingIsUnknown(t *testing.T) {
	for _, tc := range []struct{ kind, row string }{
		{"route", "local default"},
		{"route", "local default dev"},
		{"route", "local default dev lo scope"},
		{"route", "local default via private.invalid"},
		{"route", "local private.invalid dev lo"},
		{"rule", "16500: from 192.0.2.10 iif br-lan fwmark"},
		{"rule", "16500: from private.invalid iif br-lan fwmark 0x4000/0x4000 lookup 16500"},
		{"rule", "16500: from 192.0.2.10 iif br-lan fwmark bad-mark lookup 16500"},
		{"rule", "16500: from 192.0.2.10 iif br-lan fwmark 0x4000/0x4000"},
		{"rule", "16500: from 192.0.2.10 iif br-lan fwmark 0x4000/0x4000 lookup"},
		{"rule", "16500: from 192.0.2.10 iif br-lan fwmark 0x4000/0x4000 lookup 16500 not"},
		{"rule", "16500: from 192.0.2.10 not iif br-lan fwmark 0x4000/0x4000 lookup 16500"},
	} {
		t.Run(tc.kind+"/"+tc.row, func(t *testing.T) {
			c := diagnosticController(t, func(ctx context.Context, argv []string) ([]byte, error) {
				if argv[0] == "ip" && argv[2] == tc.kind {
					return []byte(tc.row), nil
				}
				return diagnosticRunner(ctx, argv)
			})
			d := c.Diagnostics(context.Background())
			evidence := d.Routing[0].LocalRoute
			if tc.kind == "rule" {
				evidence = d.Routing[0].PolicyRules
			}
			if evidence.State != "unavailable" || evidence.Present != nil || evidence.Error != "capture_diagnostic_malformed" {
				t.Fatalf("malformed dump became absence: %+v", evidence)
			}
		})
	}
}

func TestDiagnosticsOversizedRuleKeepsPhysicalRowIdentity(t *testing.T) {
	c := diagnosticController(t, func(ctx context.Context, argv []string) ([]byte, error) {
		if argv[0] == "iptables" && argv[4] == "mangle" {
			long := "5 500 RETURN tcp -- * * 0.0.0.0/0 0.0.0.0/0 " + strings.Repeat("private", diagnosticMaxLine)
			return diagnosticFixture(argv[6], long, "3 180 RETURN tcp -- * * 0.0.0.0/0 0.0.0.0/0 tcp dpt:53"), nil
		}
		return diagnosticRunner(ctx, argv)
	})
	d := c.Diagnostics(context.Background())
	if d.Chains[0].State != "partial" || len(d.Chains[0].Counters) != 1 || d.Chains[0].Counters[0].Row != 2 {
		t.Fatalf("oversized row changed subsequent identity: %+v", d.Chains[0])
	}
}

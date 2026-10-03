package capture

import (
	"context"
	"errors"
	"fmt"
	"os"
	"reflect"
	"slices"
	"strings"
	"testing"

	"be6500panel/internal/proxy"
	"be6500panel/internal/router"
	"be6500panel/internal/storage"
)

// These fixtures prove only saved compiler intent against a synthetic kernel.
// They never execute ip/iptables, resolve a public hostname, or test connectivity.
func savedPlanReads(plan proxy.OwnedRulesPlan) [][]string {
	var reads [][]string
	for _, chain := range plan.Ownership.Chains {
		reads = append(reads, []string{ipTables(chain.Family), "-w", "5", "-t", chain.Table, "-S", chain.Name})
		for _, command := range plan.Apply {
			if len(command) > 6 && command[0] == ipTables(chain.Family) && command[4] == chain.Table && command[5] == "-A" && command[6] == chain.Name {
				check := slices.Clone(command)
				check[5] = "-C"
				reads = append(reads, check)
			}
		}
	}
	for _, command := range plan.Apply {
		if len(command) > 5 && command[5] == "-I" {
			check := slices.Clone(command)
			check[5] = "-C"
			reads = append(reads, append(check[:7], check[8:]...))
		}
	}
	for _, family := range plan.Ownership.RouteFamilies {
		reads = append(reads, routeShow(family), ruleShow(family))
	}
	return reads
}

func savedPlanRunner(t *testing.T, plan proxy.OwnedRulesPlan) Runner {
	t.Helper()
	reads := savedPlanReads(plan)
	return func(_ context.Context, args []string) ([]byte, error) {
		if !slices.ContainsFunc(reads, func(read []string) bool { return slices.Equal(read, args) }) {
			t.Fatalf("observation used mutation or non-saved scope: %v", args)
		}
		if args[0] != "ip" {
			return nil, nil // Every exact -S/-C fixture above exists.
		}
		if args[2] == "route" {
			return []byte("local default dev lo scope host\n"), nil
		}
		clients, singular := plan.Ownership.ClientIPv4s, plan.Ownership.ClientIPv4
		if args[1] == "-6" {
			clients, singular = plan.Ownership.ClientIPv6s, plan.Ownership.ClientIPv6
		}
		clients = slices.Clone(clients)
		if singular != "" {
			clients = append(clients, singular)
		}
		var lines []string
		for _, client := range clients {
			lines = append(lines, fmt.Sprintf("16500: from %s iif %s fwmark 0x4000/0x4000 lookup 16500", client, plan.Ownership.LANInterface))
		}
		return []byte(strings.Join(lines, "\n")), nil
	}
}

func observationFixture(t *testing.T, observe *router.CaptureObservation, resolve EndpointResolver) *Controller {
	t.Helper()
	c := testController(t, idleRunner)
	c.SetBuilder(func(ctx context.Context, desired Desired) (proxy.RulesPlanInput, []Client, error) {
		return BuildFromAccepted(ctx, desired, []byte(acceptedNative), *observe, resolve)
	})
	if state, err := c.Select(context.Background(), desiredDevices()); err != nil || !state.Active {
		t.Fatalf("synthetic selection: %+v %v", state, err)
	}
	return c
}

func TestReconcileDesiredObservesSavedPlanDespiteScopeDrift(t *testing.T) {
	for _, cause := range []string{"dhcp-ip", "ipv4-rrset", "aaaa-only", "lookup-error", "partial-reused-ip", "all-devices-unresolved", "no-builder", "invalid-plan"} {
		t.Run(cause, func(t *testing.T) {
			observation := deviceObservation()
			answers := []string{"203.0.113.4"}
			var lookupErr error
			c := observationFixture(t, &observation, func(ctx context.Context, host string) ([]string, error) {
				if host != "node.example" {
					t.Fatalf("unexpected resolver hostname: %s", host)
				}
				return slices.Clone(answers), lookupErr
			})
			plan := clonePlan(*c.plan)
			journalBefore, err := os.ReadFile(c.path)
			if err != nil {
				t.Fatal(err)
			}
			desiredBefore, err := os.ReadFile(c.desiredPath)
			if err != nil {
				t.Fatal(err)
			}
			wantScope, wantError := "changed", "capture_scope_changed_apply_required"
			switch cause {
			case "dhcp-ip":
				observation.Devices[0].IP = "192.0.2.20"
			case "ipv4-rrset":
				answers = []string{"203.0.113.5"}
			case "aaaa-only":
				answers = append(answers, "2001:db8::4")
				wantScope, wantError = "current", ""
			case "lookup-error":
				lookupErr = errors.New("synthetic lookup failed")
				wantScope, wantError = "unresolved", "capture_endpoint_unresolved"
			case "partial-reused-ip":
				observation.Devices[0] = router.Device{MAC: "02:00:00:00:00:99", IP: "192.0.2.10", Eligible: true}
			case "all-devices-unresolved":
				observation.Devices = nil
				wantScope, wantError = "unresolved", "capture_device_unresolved"
			case "no-builder":
				c.SetBuilder(nil)
				wantScope, wantError = "unresolved", "capture_configuration_unavailable"
			case "invalid-plan":
				c.SetBuilder(func(context.Context, Desired) (proxy.RulesPlanInput, []Client, error) {
					return proxy.RulesPlanInput{}, desiredClients(c.desired), nil
				})
				wantScope, wantError = "unresolved", "capture_native_scope_invalid"
			}
			healthy := savedPlanRunner(t, plan)
			var commands [][]string
			c.runner = func(ctx context.Context, args []string) ([]byte, error) {
				commands = append(commands, slices.Clone(args))
				return healthy(ctx, args)
			}
			state, reconcileErr := c.ReconcileDesired(context.Background())
			wantState := "scope-changed"
			if wantScope == "current" {
				wantState = "active"
			}
			if !state.Active || state.CleanupPending || state.ScopeState != wantScope || state.State != wantState || state.Error != wantError || (reconcileErr != nil) != (wantError != "") {
				t.Fatalf("saved healthy plan was hidden: %+v %v", state, reconcileErr)
			}
			if !reflect.DeepEqual(commands, savedPlanReads(plan)) {
				t.Fatalf("saved plan was not observed exactly once: %v", commands)
			}
			wantInstalled := []Client{{MAC: "02:00:00:00:00:10", IP: "192.0.2.10"}, {MAC: "02:00:00:00:00:11", IP: "192.0.2.11"}}
			if !reflect.DeepEqual(state.InstalledClients, wantInstalled) || state.Commands != len(plan.Apply) || !reflect.DeepEqual(*c.plan, plan) {
				t.Fatalf("saved ownership changed on GET: %+v", state)
			}
			if cause == "dhcp-ip" && (state.Clients[0].IP != "192.0.2.20" || state.ClientIPv4 != "192.0.2.10") {
				t.Fatalf("fresh desired observations confused with installed IP: %+v", state)
			}
			if cause == "partial-reused-ip" && (state.Clients[0].MAC != "02:00:00:00:00:10" || state.Clients[0].IP != "" || state.Clients[1].IP != "192.0.2.11") {
				t.Fatalf("wrong MAC substituted for unresolved identity: %+v", state)
			}
			journalAfter, err := os.ReadFile(c.path)
			if err != nil || !slices.Equal(journalBefore, journalAfter) {
				t.Fatal("GET changed ownership journal", err)
			}
			desiredAfter, err := os.ReadFile(c.desiredPath)
			if err != nil || !slices.Equal(desiredBefore, desiredAfter) {
				t.Fatal("GET changed desired storage", err)
			}
			state.InstalledClients[0].IP = "198.51.100.99"
			if c.Status().InstalledClients[0].IP != "192.0.2.10" {
				t.Fatal("installed status aliases controller ownership")
			}
		})
	}
}

func TestReconcileDesiredMissingOrUncertainSavedKernel(t *testing.T) {
	for _, scope := range []string{"current", "changed", "unresolved"} {
		for _, failed := range []string{"chain", "exact-rule", "mac-hook", "route", "policy", "permission", "table-names"} {
			t.Run(scope+"/"+failed, func(t *testing.T) {
				observation := deviceObservation()
				lookupErr := error(nil)
				c := observationFixture(t, &observation, func(ctx context.Context, host string) ([]string, error) {
					if lookupErr != nil {
						return nil, lookupErr
					}
					return fakeResolve(ctx, host)
				})
				if scope == "changed" {
					observation.Devices[0].IP = "192.0.2.20"
				}
				if scope == "unresolved" {
					lookupErr = errors.New("synthetic lookup failed")
				}
				plan := clonePlan(*c.plan)
				healthy := savedPlanRunner(t, plan)
				if failed == "table-names" {
					c.tableNames = func() (map[string]int, error) { return nil, errors.New("synthetic alias read failed") }
				}
				c.runner = func(ctx context.Context, args []string) ([]byte, error) {
					output, err := healthy(ctx, args)
					if failed == "permission" {
						return []byte("permission denied"), errors.New("synthetic read failure")
					}
					if args[0] == "ip" {
						if args[2] == failed || (failed == "policy" && args[2] == "rule") {
							return nil, nil
						}
						return output, err
					}
					if failed == "chain" && args[5] == "-S" {
						return absentChain()
					}
					if args[5] == "-C" && ((failed == "exact-rule" && args[6] != "PREROUTING" && args[6] != "FORWARD") || (failed == "mac-hook" && slices.Contains(args, "--mac-source"))) {
						return []byte("iptables: Bad rule (does a matching rule exist in that chain?)."), errors.New("synthetic absent rule")
					}
					return output, err
				}
				state, err := c.ReconcileDesired(context.Background())
				if err == nil || state.Active || !state.CleanupPending || state.State != "cleanup-pending" || state.ScopeState != scope || state.Commands != len(plan.Apply) || !reflect.DeepEqual(*c.plan, plan) {
					t.Fatalf("failed proof hid possibly-live resources: %+v %v", state, err)
				}
				wantError := "capture_observation_failed"
				if scope == "changed" {
					wantError = "capture_scope_changed_apply_required"
				}
				if scope == "unresolved" {
					wantError = "capture_endpoint_unresolved"
				}
				if state.Error != wantError {
					t.Fatalf("scope warning lost: %+v", state)
				}
				// A later complete proof clears observation uncertainty, not intent.
				c.tableNames = func() (map[string]int, error) { return map[string]int{"capture": proxy.CaptureTable}, nil }
				c.runner = healthy
				state, _ = c.ReconcileDesired(context.Background())
				if !state.Active || state.CleanupPending {
					t.Fatalf("fresh proof did not resolve uncertainty: %+v", state)
				}
			})
		}
	}
}

func TestReconcileDesiredRetainsFailedCleanupAndOffWarning(t *testing.T) {
	for _, off := range []bool{false, true} {
		t.Run(fmt.Sprint("off=", off), func(t *testing.T) {
			observation := deviceObservation()
			c := observationFixture(t, &observation, fakeResolve)
			plan := clonePlan(*c.plan)
			c.runner = func(context.Context, []string) ([]byte, error) { return nil, errors.New("synthetic cleanup failure") }
			if off {
				c.SetStorageAdmission(func(context.Context, string, int64, bool) (func(), error) { return nil, storage.ErrInsufficientSpace })
				if err := c.Disable(context.Background()); !errors.Is(err, storage.ErrInsufficientSpace) {
					t.Fatal(err)
				}
			} else if err := c.Cleanup(context.Background()); err == nil {
				t.Fatal("missing cleanup failure")
			}
			c.runner = savedPlanRunner(t, plan)
			state, err := c.ReconcileDesired(context.Background())
			if err != nil || !state.Active || !state.CleanupPending || state.State != "cleanup-pending" || state.Desired == off {
				t.Fatalf("GET erased actual failed cleanup: %+v %v", state, err)
			}
			if off && state.Error != "capture_disable_not_persisted" {
				t.Fatalf("GET cleared sticky off warning: %+v", state)
			}
			c.runner = idleRunner
			if err := c.Cleanup(context.Background()); err != nil {
				t.Fatal(err)
			}
			state = c.Status()
			if state.Active || state.CleanupPending || len(state.InstalledClients) != 0 || state.ScopeState != "" {
				t.Fatalf("successful cleanup retained installed projection: %+v", state)
			}
		})
	}
}

func TestReconcileDesiredWithoutSavedPlanKeepsPartialWarning(t *testing.T) {
	observation := deviceObservation()
	c := observationFixture(t, &observation, fakeResolve)
	if err := c.Cleanup(context.Background()); err != nil {
		t.Fatal(err)
	}
	observation.Devices = observation.Devices[1:]
	c.runner = func(context.Context, []string) ([]byte, error) {
		t.Fatal("no saved plan may inspect or mutate kernel resources")
		return nil, nil
	}
	state, err := c.ReconcileDesired(context.Background())
	var partial *PartialScopeError
	if !errors.As(err, &partial) || state.Active || state.CleanupPending || state.State != "suspended" || state.ScopeState != "" || state.Error != "capture_devices_pending" || len(state.InstalledClients) != 0 || state.Clients[0].IP != "" {
		t.Fatalf("no-plan pending selection changed contract: %+v %v", state, err)
	}
}

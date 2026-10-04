package capture

import (
	"context"
	"errors"
	"os"
	"reflect"
	"slices"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"be6500panel/internal/proxy"
)

// All work here uses synthetic accepted intent and fake runners. No subprocess,
// network, core process, router or kernel state is used by these fixtures.
type desiredObservationResult struct {
	state Status
	err   error
}

type desiredObservationContextKey struct{}

func startDesiredObservation(c *Controller, desired bool) <-chan desiredObservationResult {
	result := make(chan desiredObservationResult, 1)
	go func() {
		ctx := context.WithValue(context.Background(), desiredObservationContextKey{}, true)
		var state Status
		var err error
		if desired {
			state, err = c.ReconcileDesired(ctx)
		} else {
			state, err = c.Reconcile(ctx)
		}
		result <- desiredObservationResult{state, err}
	}()
	return result
}

func receiveDesiredObservation(t *testing.T, result <-chan desiredObservationResult) desiredObservationResult {
	t.Helper()
	select {
	case got := <-result:
		return got
	case <-time.After(5 * time.Second):
		t.Fatal("cooperative observation did not finish")
		return desiredObservationResult{}
	}
}

func waitDesiredObservation(t *testing.T, entered <-chan struct{}) {
	t.Helper()
	select {
	case <-entered:
	case <-time.After(2 * time.Second):
		t.Fatal("observation did not enter fake callback")
	}
}

func TestDesiredObservationBlockedBuilderAllowsDisableAndDiscardsStaleResult(t *testing.T) {
	c, _, _ := disableFixture(t)
	plan := clonePlan(*c.plan)
	healthy := savedPlanRunner(t, plan)
	c.runner = func(ctx context.Context, args []string) ([]byte, error) {
		if ctx.Value(desiredObservationContextKey{}) != nil {
			if !isReadCommand(args) {
				t.Errorf("GET mutated resources: %v", args)
			}
			return healthy(ctx, args)
		}
		return idleRunner(ctx, args)
	}
	entered, release := make(chan struct{}), make(chan struct{})
	var releaseOnce sync.Once
	defer releaseOnce.Do(func() { close(release) })
	c.SetBuilder(func(ctx context.Context, d Desired) (proxy.RulesPlanInput, []Client, error) {
		close(entered)
		select {
		case <-release:
		case <-ctx.Done():
			return proxy.RulesPlanInput{}, desiredClients(d), ctx.Err()
		}
		input, clients, err := BuildFromAccepted(ctx, d, []byte(acceptedNative), deviceObservation(), fakeResolve)
		clients[0].Hostname = "stale GET observation"
		return input, clients, err
	})
	result := startDesiredObservation(c, true)
	waitDesiredObservation(t, entered)
	disabled := make(chan error, 1)
	go func() { disabled <- c.Disable(context.Background()) }()
	select {
	case err := <-disabled:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(500 * time.Millisecond):
		releaseOnce.Do(func() { close(release) })
		<-disabled
		t.Fatal("Disable waited for the GET builder")
	}
	want := c.Status()
	releaseOnce.Do(func() { close(release) })
	got := receiveDesiredObservation(t, result)
	if got.err != nil || !reflect.DeepEqual(got.state, want) || !reflect.DeepEqual(c.Status(), want) || got.state.Desired || got.state.Active || got.state.CleanupPending {
		t.Fatalf("stale builder overwrote Disable: got=%+v want=%+v err=%v", got.state, want, got.err)
	}
	if _, err := os.Stat(c.path); !os.IsNotExist(err) {
		t.Fatal("GET recreated the cleaned journal", err)
	}
}

func TestDesiredObservationStaleNilPlanSelectABAIsDiscarded(t *testing.T) {
	c := testController(t, func(context.Context, []string) ([]byte, error) {
		t.Error("no saved plan may read or mutate resources")
		return nil, nil
	})
	d := Desired{Enabled: true, ClientIPv4: "192.0.2.10", IPv6: proxy.IPv6Direct}
	offline := errors.New("capture_configuration_unavailable")
	c.SetBuilder(func(_ context.Context, d Desired) (proxy.RulesPlanInput, []Client, error) {
		return proxy.RulesPlanInput{}, desiredClients(d), offline
	})
	if _, err := c.Select(context.Background(), d); !errors.Is(err, offline) {
		t.Fatal(err)
	}
	entered, release := make(chan struct{}), make(chan struct{})
	var releaseOnce sync.Once
	defer releaseOnce.Do(func() { close(release) })
	var calls atomic.Int32
	c.SetBuilder(func(ctx context.Context, d Desired) (proxy.RulesPlanInput, []Client, error) {
		clients := desiredClients(d)
		if calls.Add(1) == 1 {
			close(entered)
			select {
			case <-release:
			case <-ctx.Done():
				return proxy.RulesPlanInput{}, clients, ctx.Err()
			}
			clients[0].Hostname = "obsolete observation"
		}
		return proxy.RulesPlanInput{}, clients, offline
	})
	result := startDesiredObservation(c, true)
	waitDesiredObservation(t, entered)
	if err := c.DisableRetainingSelection(context.Background()); err != nil {
		t.Fatal(err)
	}
	if _, err := c.Select(context.Background(), d); !errors.Is(err, offline) {
		t.Fatal(err)
	}
	want := c.Status() // Same desired, nil plan, clients and warning as the GET snapshot.
	releaseOnce.Do(func() { close(release) })
	got := receiveDesiredObservation(t, result)
	if got.err != nil || !reflect.DeepEqual(got.state, want) || !reflect.DeepEqual(c.Status(), want) {
		t.Fatalf("nil-plan ABA published stale data: got=%+v want=%+v err=%v", got.state, want, got.err)
	}
}

func TestDesiredObservationBuilderTimeoutReservesInstalledResourceBudget(t *testing.T) {
	c, _, wantClients := disableFixture(t)
	plan := clonePlan(*c.plan)
	journalBefore, err := os.ReadFile(c.path)
	if err != nil {
		t.Fatal(err)
	}
	var commands [][]string
	healthy := savedPlanRunner(t, plan)
	c.runner = func(ctx context.Context, args []string) ([]byte, error) {
		deadline, ok := ctx.Deadline()
		if !ok || time.Until(deadline) <= time.Second || time.Until(deadline) > 2*time.Second {
			t.Errorf("installed proof did not receive its reserved budget: %v %v", deadline, ok)
		}
		if !isReadCommand(args) {
			t.Errorf("timeout GET mutated resources: %v", args)
		}
		commands = append(commands, slices.Clone(args))
		return healthy(ctx, args)
	}
	c.SetBuilder(func(ctx context.Context, _ Desired) (proxy.RulesPlanInput, []Client, error) {
		deadline, ok := ctx.Deadline()
		if !ok || time.Until(deadline) > time.Second {
			t.Error("builder did not receive a one-second deadline")
		}
		<-ctx.Done()
		// Even a callback that forgets its error cannot turn an expired build
		// into fresh scope proof or erase the last observed selected clients.
		return proxy.RulesPlanInput{}, nil, nil
	})
	started := time.Now()
	state, err := c.ReconcileDesired(context.Background())
	if !errors.Is(err, context.DeadlineExceeded) || !state.Active || state.CleanupPending || state.ScopeState != "unresolved" || state.State != "scope-changed" || !reflect.DeepEqual(state.Clients, wantClients) || !reflect.DeepEqual(state.InstalledClients, installedClients(plan.Ownership)) {
		t.Fatalf("builder timeout hid installed truth: %+v %v", state, err)
	}
	if elapsed := time.Since(started); elapsed > 3*time.Second {
		t.Fatalf("cooperative builder exceeded GET budget: %v", elapsed)
	}
	if !reflect.DeepEqual(commands, savedPlanReads(plan)) {
		t.Fatalf("timeout skipped or widened saved-plan proof: %v", commands)
	}
	journalAfter, readErr := os.ReadFile(c.path)
	if readErr != nil || !slices.Equal(journalBefore, journalAfter) {
		t.Fatal("timeout changed installed journal", readErr)
	}
}

func TestDesiredObservationBlockedResourceReadAllowsDisable(t *testing.T) {
	for _, desired := range []bool{false, true} {
		t.Run(map[bool]string{false: "plain", true: "desired"}[desired], func(t *testing.T) {
			c, _, _ := disableFixture(t)
			plan := clonePlan(*c.plan)
			healthy := savedPlanRunner(t, plan)
			entered, release := make(chan struct{}), make(chan struct{})
			var releaseOnce sync.Once
			defer releaseOnce.Do(func() { close(release) })
			var first atomic.Bool
			c.runner = func(ctx context.Context, args []string) ([]byte, error) {
				if ctx.Value(desiredObservationContextKey{}) == nil {
					return idleRunner(ctx, args)
				}
				if !isReadCommand(args) {
					t.Errorf("GET emitted mutation: %v", args)
				}
				if first.CompareAndSwap(false, true) {
					deadline, ok := ctx.Deadline()
					if !ok || time.Until(deadline) > 2*time.Second {
						t.Error("resource callback did not receive a bounded context")
					}
					close(entered)
					select {
					case <-release:
					case <-ctx.Done():
						return nil, ctx.Err()
					}
				}
				return healthy(ctx, args)
			}
			result := startDesiredObservation(c, desired)
			waitDesiredObservation(t, entered)
			disabled := make(chan error, 1)
			go func() { disabled <- c.Disable(context.Background()) }()
			select {
			case err := <-disabled:
				if err != nil {
					t.Fatal(err)
				}
			case <-time.After(500 * time.Millisecond):
				releaseOnce.Do(func() { close(release) })
				<-disabled
				t.Fatal("Disable waited for the GET resource runner")
			}
			want := c.Status()
			releaseOnce.Do(func() { close(release) })
			got := receiveDesiredObservation(t, result)
			if got.err != nil || !reflect.DeepEqual(got.state, want) || !reflect.DeepEqual(c.Status(), want) {
				t.Fatalf("stale resource proof overwrote Disable: %+v %v", got.state, got.err)
			}
		})
	}
}

func TestDesiredObservationResourceTimeoutRetainsJournalAndUncertainty(t *testing.T) {
	c, _, _ := disableFixture(t)
	plan := clonePlan(*c.plan)
	before, err := os.ReadFile(c.path)
	if err != nil {
		t.Fatal(err)
	}
	reads := 0
	c.runner = func(ctx context.Context, args []string) ([]byte, error) {
		if !isReadCommand(args) {
			t.Errorf("GET mutated after timeout: %v", args)
		}
		reads++
		<-ctx.Done()
		return []byte("iptables: No chain/target/match by that name."), ctx.Err()
	}
	started := time.Now()
	state, err := c.ReconcileDesired(context.Background())
	if !errors.Is(err, context.DeadlineExceeded) || state.Active || !state.CleanupPending || state.State != "cleanup-pending" || state.Error != "capture_observation_failed" || reads != 1 || !reflect.DeepEqual(state.InstalledClients, installedClients(plan.Ownership)) {
		t.Fatalf("timeout claimed clean absence: %+v reads=%d %v", state, reads, err)
	}
	if elapsed := time.Since(started); elapsed > 3*time.Second {
		t.Fatalf("cooperative resource read exceeded budget: %v", elapsed)
	}
	after, readErr := os.ReadFile(c.path)
	if readErr != nil || !slices.Equal(before, after) || c.cleanupFailed {
		t.Fatal("observation changed journal or actual-cleanup failure flag", readErr)
	}
}

func TestDesiredObservationBusyReturnsUncertainWithoutWaiting(t *testing.T) {
	c, _, _ := disableFixture(t)
	c.mu.Lock()
	defer c.mu.Unlock()
	before := c.statusLocked()
	for _, desired := range []bool{false, true} {
		result := startDesiredObservation(c, desired)
		select {
		case got := <-result:
			if got.err == nil || got.state.Active || !got.state.CleanupPending || got.state.Error != "capture_observation_busy" {
				t.Fatalf("busy observation claimed resource proof: %+v %v", got.state, got.err)
			}
		case <-time.After(500 * time.Millisecond):
			t.Error("GET waited for the controller mutation lock")
			return
		}
	}
	if !reflect.DeepEqual(before, c.statusLocked()) || c.cleanupFailed {
		t.Fatal("busy observation mutated controller state")
	}
}

func TestDesiredObservationRoutedTUNDriftKeepsJournalAndPending(t *testing.T) {
	for _, failure := range []string{"interface-down", "address-read", "rpf"} {
		t.Run(failure, func(t *testing.T) {
			c := testController(t, tunIdleRunner)
			d := desiredDevices()
			d.Devices = d.Devices[:1]
			c.SetBuilder(func(ctx context.Context, d Desired) (proxy.RulesPlanInput, []Client, error) {
				return BuildFromAccepted(ctx, d, []byte(acceptedRoutedTUN), deviceObservation(), fakeResolve)
			})
			if _, err := c.Select(context.Background(), d); err != nil {
				t.Fatal(err)
			}
			plan := clonePlan(*c.plan)
			before, err := os.ReadFile(c.path)
			if err != nil {
				t.Fatal(err)
			}
			base := tunObservedRunner(t, plan, "default dev b6p-tun", "")
			c.runner = func(ctx context.Context, args []string) ([]byte, error) {
				if !isReadCommand(args) {
					t.Errorf("drift observation mutated: %v", args)
				}
				if slices.Equal(args, tunAddressShow("b6p-tun")) {
					if failure == "address-read" {
						return nil, errors.New("synthetic interface read failure")
					}
					if failure == "interface-down" {
						return []byte(strings.Replace(string(tunAddressFixture()), `"UP",`, "", 1)), nil
					}
				}
				if failure == "rpf" && slices.Equal(args, tunRPFilterShow("b6p-tun")) {
					return []byte("1"), nil
				}
				return base(ctx, args)
			}
			state, err := c.ReconcileDesired(context.Background())
			if err == nil || state.Active || !state.CleanupPending || state.ScopeState != "current" || !reflect.DeepEqual(*c.plan, plan) {
				t.Fatalf("interface drift lost ownership uncertainty: %+v %v", state, err)
			}
			after, readErr := os.ReadFile(c.path)
			if readErr != nil || !slices.Equal(before, after) {
				t.Fatal("drift observation changed recovery journal", readErr)
			}
		})
	}
}

func TestDesiredObservationStaleBuilderCannotOverwriteNewInstalledSelection(t *testing.T) {
	c, _, _ := disableFixture(t)
	oldPlan := clonePlan(*c.plan)
	healthy := savedPlanRunner(t, oldPlan)
	c.runner = func(ctx context.Context, args []string) ([]byte, error) {
		if ctx.Value(desiredObservationContextKey{}) != nil {
			return healthy(ctx, args)
		}
		return idleRunner(ctx, args)
	}
	entered, release := make(chan struct{}), make(chan struct{})
	var releaseOnce sync.Once
	defer releaseOnce.Do(func() { close(release) })
	var calls atomic.Int32
	c.SetBuilder(func(ctx context.Context, d Desired) (proxy.RulesPlanInput, []Client, error) {
		if calls.Add(1) == 1 {
			close(entered)
			select {
			case <-release:
			case <-ctx.Done():
				return proxy.RulesPlanInput{}, desiredClients(d), ctx.Err()
			}
		}
		return BuildFromAccepted(ctx, d, []byte(acceptedNative), deviceObservation(), fakeResolve)
	})
	result := startDesiredObservation(c, true)
	waitDesiredObservation(t, entered)
	d := desiredDevices()
	d.Devices = d.Devices[1:]
	selected := make(chan desiredObservationResult, 1)
	go func() {
		state, err := c.Select(context.Background(), d)
		selected <- desiredObservationResult{state, err}
	}()
	var want Status
	select {
	case got := <-selected:
		if got.err != nil || !got.state.Active || len(got.state.InstalledClients) != 1 {
			t.Fatalf("new selection failed: %+v %v", got.state, got.err)
		}
		want = got.state
	case <-time.After(500 * time.Millisecond):
		releaseOnce.Do(func() { close(release) })
		<-selected
		t.Fatal("new selection waited for the GET builder")
	}
	releaseOnce.Do(func() { close(release) })
	got := receiveDesiredObservation(t, result)
	if got.err != nil || !reflect.DeepEqual(got.state, want) || !reflect.DeepEqual(c.Status(), want) || c.plan == nil || plansEqual(*c.plan, oldPlan) {
		t.Fatalf("stale builder overwrote new installed scope: got=%+v want=%+v err=%v", got.state, want, got.err)
	}
}

func TestDesiredObservationDoesNotClearStickyOffCleanupFailures(t *testing.T) {
	c, _, _ := disableFixture(t)
	plan := clonePlan(*c.plan)
	c.runner = func(context.Context, []string) ([]byte, error) {
		return nil, errors.New("synthetic deletion failed")
	}
	if err := c.DisableRetainingSelection(context.Background()); err == nil {
		t.Fatal("fixture did not fail cleanup")
	}
	if c.Status().Error != "capture_cleanup_failed" || !c.cleanupFailed {
		t.Fatal("fixture lacks sticky cleanup failure")
	}
	before, err := os.ReadFile(c.path)
	if err != nil {
		t.Fatal(err)
	}
	c.SetBuilder(func(ctx context.Context, d Desired) (proxy.RulesPlanInput, []Client, error) {
		<-ctx.Done()
		return proxy.RulesPlanInput{}, desiredClients(d), ctx.Err()
	})
	c.runner = savedPlanRunner(t, plan)
	state, err := c.ReconcileDesired(context.Background())
	if err != nil || state.Desired || !state.Active || !state.CleanupPending || state.Error != "capture_cleanup_failed" || !c.cleanupFailed {
		t.Fatalf("scope timeout erased sticky off/cleanup failure: %+v %v", state, err)
	}
	after, readErr := os.ReadFile(c.path)
	if readErr != nil || !slices.Equal(before, after) {
		t.Fatal("observation changed cleanup recovery journal", readErr)
	}
}

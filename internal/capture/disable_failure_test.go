package capture

import (
	"context"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"strings"
	"testing"
	"time"

	"be6500panel/internal/proxy"
	"be6500panel/internal/storage"
)

// All identities and addresses in these tests are synthetic fixtures. Every
// network operation uses a fake runner; no host or router rules are changed.
func disableFixture(t *testing.T) (*Controller, Desired, []Client) {
	t.Helper()
	c := testController(t, idleRunner)
	c.SetBuilder(func(ctx context.Context, d Desired) (proxy.RulesPlanInput, []Client, error) {
		return BuildFromAccepted(ctx, d, []byte(acceptedNative), deviceObservation(), fakeResolve)
	})
	desired := desiredDevices()
	state, err := c.Select(context.Background(), desired)
	if err != nil || !state.Active {
		t.Fatalf("initial fake selection: %+v %v", state, err)
	}
	return c, desired, state.Clients
}

func checkDisableFailureState(t *testing.T, c *Controller, desired Desired, clients []Client, pending bool) {
	t.Helper()
	state := c.Status()
	if state.Desired || state.Error != "capture_disable_not_persisted" || state.CleanupPending != pending || state.Active != pending {
		t.Fatalf("unsafe failed off switch: %+v", state)
	}
	desired.Enabled = false
	if !reflect.DeepEqual(c.Desired(), desired) || !reflect.DeepEqual(state.Clients, clients) {
		t.Fatalf("failed off switch lost identities/policy/observations: desired=%+v clients=%+v", c.Desired(), state.Clients)
	}
	if pending && (state.State != "cleanup-pending" || state.ClientIPv4 != clients[0].IP || state.Commands == 0) {
		t.Fatalf("pending owned scope was hidden: %+v", state)
	}
}

func assertNoOffIntentApply(t *testing.T, c *Controller) {
	t.Helper()
	priorRunner := c.runner
	c.runner = func(ctx context.Context, args []string) ([]byte, error) {
		if !isReadCommand(args) && !(args[0] == "ip" && args[3] == "del") && !(len(args) > 5 && slices.Contains([]string{"-D", "-F", "-X"}, args[5])) {
			t.Fatalf("off intent applied a command: %v", args)
		}
		return priorRunner(ctx, args)
	}
	defer func() { c.runner = priorRunner }()
	for range 3 {
		if state, err := c.Refresh(context.Background()); err != nil || state.Desired || state.Error != "capture_disable_not_persisted" {
			t.Fatalf("refresh cleared off intent/error: %+v %v", state, err)
		}
		// Restore may retry owned cleanup, but must never rebuild or apply.
		state, _ := c.Restore(context.Background())
		if state.Desired || state.Error != "capture_disable_not_persisted" {
			t.Fatalf("restore cleared off intent/error: %+v", state)
		}
		before := 0
		original := c.runner
		c.runner = func(ctx context.Context, args []string) ([]byte, error) {
			if !isReadCommand(args) {
				before++
			}
			if c.plan != nil {
				return savedPlanRunner(t, *c.plan)(ctx, args)
			}
			return original(ctx, args)
		}
		state, err := c.ReconcileDesired(context.Background())
		c.runner = original
		if err != nil || before != 0 || state.Desired || state.Error != "capture_disable_not_persisted" {
			t.Fatalf("GET changed off intent/error/resources: %+v mutations=%d %v", state, before, err)
		}
		_ = c.Suspend(context.Background(), "capture_listener_unavailable")
		if state := c.Status(); state.Desired || state.Error != "capture_disable_not_persisted" {
			t.Fatalf("suspend obscured unsaved off switch: %+v", state)
		}
	}
}

func TestDisableFailureLatchesOffBeforeAdmissionAndKeepsRestartWarning(t *testing.T) {
	for _, retain := range []bool{false, true} {
		for _, fault := range []string{"admission", "atomic-rename", "already-canceled", "canceled-admission", "nil-context"} {
			t.Run(map[bool]string{false: "delete", true: "retained"}[retain]+"/"+fault, func(t *testing.T) {
				c, desired, clients := disableFixture(t)
				originalPath := c.desiredPath
				priorBytes, err := os.ReadFile(originalPath)
				if err != nil {
					t.Fatal(err)
				}
				ctx, cancel := context.WithCancel(context.Background())
				defer cancel()
				admissions, releases, cleaned := 0, 0, 0
				c.SetStorageAdmission(func(ctx context.Context, _ string, _ int64, recovery bool) (func(), error) {
					admissions++
					if c.desired.Enabled || !c.disableNotPersisted || !recovery {
						t.Fatal("off intent was not latched before recovery admission")
					}
					release := func() { releases++ }
					if fault == "admission" {
						return release, storage.ErrInsufficientSpace
					}
					if fault == "canceled-admission" {
						cancel()
					}
					return release, nil
				})
				if fault == "atomic-rename" {
					// Rename over a directory fails after the temporary file is
					// written. The real previously saved enabled intent remains.
					c.desiredPath = filepath.Join(t.TempDir(), "rename-target")
					if err := os.Mkdir(c.desiredPath, 0700); err != nil {
						t.Fatal(err)
					}
				}
				if fault == "already-canceled" {
					cancel()
				}
				var disableCtx context.Context = ctx
				if fault == "nil-context" {
					disableCtx = nil
				}
				c.runner = func(callCtx context.Context, args []string) ([]byte, error) {
					deadline, bounded := callCtx.Deadline()
					if callCtx.Err() != nil || !bounded || time.Until(deadline) > 30*time.Second {
						t.Fatal("disable cleanup did not use a fresh bounded context")
					}
					cleaned++
					return idleRunner(callCtx, args)
				}
				if retain {
					err = c.DisableRetainingSelection(disableCtx)
				} else {
					err = c.Disable(disableCtx)
				}
				if err == nil || !strings.Contains(err.Error(), "capture_disable_not_persisted: retry disable before process restart") || cleaned == 0 {
					t.Fatalf("failed write/cleanup warning hidden: cleaned=%d %v", cleaned, err)
				}
				if fault == "admission" && !errors.Is(err, storage.ErrInsufficientSpace) {
					t.Fatal("lost storage failure", err)
				}
				if strings.Contains(fault, "canceled") && !errors.Is(err, context.Canceled) {
					t.Fatal("lost cancellation", err)
				}
				expectedAdmissions := 1
				if fault == "already-canceled" || fault == "nil-context" {
					expectedAdmissions = 0
				}
				if admissions != expectedAdmissions || releases != expectedAdmissions || c.storageReserved {
					t.Fatalf("leaked/incorrect admission: admissions=%d releases=%d reserved=%v", admissions, releases, c.storageReserved)
				}
				checkDisableFailureState(t, c, desired, clients, false)
				assertNoOffIntentApply(t, c)
				checkDisableFailureState(t, c, desired, clients, false)
				afterBytes, err := os.ReadFile(originalPath)
				if err != nil || !slices.Equal(afterBytes, priorBytes) {
					t.Fatal("failed off write changed old saved intent", err)
				}
				// In-memory safety is not durable storage. Do not promise that
				// a fresh process is off until an explicit disable retry saves.
				reopened, err := New(filepath.Dir(originalPath), idleRunner)
				if err != nil || !reopened.Status().Desired {
					t.Fatal("fixture did not preserve old enabled restart risk", err)
				}
				c.desiredPath = originalPath
				c.SetStorageAdmission(nil)
				if retain {
					err = c.DisableRetainingSelection(context.Background())
				} else {
					err = c.Disable(context.Background())
				}
				if err != nil || c.Status().Desired || c.Status().Error != "" {
					t.Fatal("explicit off retry did not clear warning", c.Status(), err)
				}
				want := Desired{IPv6: desired.IPv6}
				if retain {
					want = desired
					want.Enabled = false
				}
				wantBytes, _ := json.Marshal(want)
				saved, err := os.ReadFile(originalPath)
				if err != nil || !slices.Equal(saved, wantBytes) {
					t.Fatalf("retry did not save exact off intent: saved=%s want=%s %v", saved, wantBytes, err)
				}
				reopened, err = New(filepath.Dir(originalPath), idleRunner)
				if err != nil || reopened.Status().Desired {
					t.Fatal("successful retry was not saved off", err)
				}
			})
		}
	}
}

func TestDisableWriteAndCleanupFailureRetainsExactJournalAndOffLatch(t *testing.T) {
	c, desired, clients := disableFixture(t)
	priorJournal, err := os.ReadFile(c.path)
	if err != nil {
		t.Fatal(err)
	}
	c.SetStorageAdmission(func(context.Context, string, int64, bool) (func(), error) {
		return nil, storage.ErrInsufficientSpace
	})
	busy := errors.New("synthetic owned hook deletion busy")
	c.runner = func(ctx context.Context, args []string) ([]byte, error) {
		if len(args) > 5 && args[5] == "-D" {
			return nil, busy
		}
		return idleRunner(ctx, args)
	}
	if err = c.Disable(context.Background()); !errors.Is(err, storage.ErrInsufficientSpace) || !errors.Is(err, busy) {
		t.Fatal("failed disable lost save or cleanup error", err)
	}
	checkDisableFailureState(t, c, desired, clients, true)
	assertNoOffIntentApply(t, c)
	checkDisableFailureState(t, c, desired, clients, true)
	afterJournal, err := os.ReadFile(c.path)
	if err != nil || !slices.Equal(afterJournal, priorJournal) {
		t.Fatal("failed cleanup lost original exact-client ownership journal", err)
	}
	c.SetStorageAdmission(nil)
	if err = c.DisableRetainingSelection(context.Background()); !errors.Is(err, busy) {
		t.Fatal("saved-off cleanup failure hidden", err)
	}
	state := c.Status()
	if state.Desired || !state.Active || !state.CleanupPending || state.Error != "capture_cleanup_failed" || state.State != "cleanup-pending" {
		t.Fatal("saved off switch lost truth about remaining owned hooks", state)
	}
	saved, err := os.ReadFile(c.desiredPath)
	want := desired
	want.Enabled = false
	wantBytes, _ := json.Marshal(want)
	if err != nil || !slices.Equal(saved, wantBytes) {
		t.Fatalf("cleanup failure changed saved off intent: saved=%s want=%s %v", saved, wantBytes, err)
	}
	for range 3 {
		if state, err = c.Refresh(context.Background()); err != nil || state.Desired {
			t.Fatal("saved off refresh did not stay off", state, err)
		}
		if state, err = c.Restore(context.Background()); !errors.Is(err, busy) || state.Desired || !state.CleanupPending {
			t.Fatal("saved off restore hid cleanup failure", state, err)
		}
	}
	c.runner = idleRunner
	if err = c.DisableRetainingSelection(context.Background()); err != nil || c.Status().Desired || c.Status().CleanupPending || c.Status().Active {
		t.Fatal("cleanup retry did not stay off", c.Status(), err)
	}
}

func TestOnlySuccessfulExplicitSelectionCanResumeAfterFailedOffWrite(t *testing.T) {
	c, desired, _ := disableFixture(t)
	c.SetStorageAdmission(func(context.Context, string, int64, bool) (func(), error) {
		return nil, storage.ErrInsufficientSpace
	})
	if err := c.DisableRetainingSelection(context.Background()); !errors.Is(err, storage.ErrInsufficientSpace) {
		t.Fatal(err)
	}
	state, err := c.Select(context.Background(), desired)
	if !errors.Is(err, storage.ErrInsufficientSpace) || state.Desired || state.Active || state.Error != "capture_disable_not_persisted" {
		t.Fatal("denied explicit selection cleared the off latch", state, err)
	}
	assertNoOffIntentApply(t, c)
	c.SetStorageAdmission(nil)
	state, err = c.Select(context.Background(), desired)
	if err != nil || !state.Desired || !state.Active || state.Error != "" || !reflect.DeepEqual(c.Desired(), desired) {
		t.Fatal("successful explicit selection did not resume chosen scope", state, err)
	}
}

func TestDisableCleanupSurvivesCancellationAfterOffIntentWasSaved(t *testing.T) {
	c, _, _ := disableFixture(t)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	wantCleanup := len(c.plan.Cleanup)
	cleaned := 0
	c.runner = func(callCtx context.Context, args []string) ([]byte, error) {
		cancel()
		if callCtx.Err() != nil {
			t.Fatal("off-switch cleanup inherited late request cancellation")
		}
		cleaned++
		return idleRunner(callCtx, args)
	}
	if err := c.Disable(ctx); err != nil || cleaned != wantCleanup {
		t.Fatalf("late cancellation abandoned cleanup: cleaned=%d want=%d %v", cleaned, wantCleanup, err)
	}
	state := c.Status()
	if state.Desired || state.Active || state.CleanupPending || state.Error != "" {
		t.Fatal("saved off intent did not finish cleanup", state)
	}
	reopened, err := New(filepath.Dir(c.path), idleRunner)
	if err != nil || reopened.Status().Desired {
		t.Fatal("late cancellation lost saved off intent", err)
	}
}

package capture

import (
	"context"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"testing"

	"be6500panel/internal/proxy"
	"be6500panel/internal/storage"
)

func TestStorageAdmissionDeniedApplyBeforeAnyKernelCommand(t *testing.T) {
	commands, released := 0, 0
	controller := testController(t, func(context.Context, []string) ([]byte, error) { commands++; return nil, nil })
	input := testInput()
	plan, err := proxy.PlanOwnedRules(input)
	if err != nil {
		t.Fatal(err)
	}
	raw, _ := json.Marshal(journal{OwnedRulesPlan: plan, Input: &input})
	controller.SetStorageAdmission(func(ctx context.Context, path string, size int64, recovery bool) (func(), error) {
		if path != filepath.Dir(controller.path) || size != int64(len(raw))+4096 || recovery {
			t.Fatalf("wrong full temp admission: %s %d %v", path, size, recovery)
		}
		return func() { released++ }, storage.ErrInsufficientSpace
	})
	state, err := controller.Apply(context.Background(), input)
	if !errors.Is(err, storage.ErrInsufficientSpace) || commands != 0 || released != 1 || state.Active || state.Desired {
		t.Fatalf("denied apply changed state: %+v commands=%d release=%d err=%v", state, commands, released, err)
	}
	if _, err = os.Stat(controller.path); !os.IsNotExist(err) {
		t.Fatal("denied apply wrote journal", err)
	}
}

func TestStorageSelectionAdmitsDesiredAndJournalBeforeChangingScope(t *testing.T) {
	commands := 0
	controller := testController(t, func(ctx context.Context, args []string) ([]byte, error) { commands++; return idleRunner(ctx, args) })
	controller.SetBuilder(func(ctx context.Context, d Desired) (proxy.RulesPlanInput, []Client, error) {
		return BuildFromAccepted(ctx, d, []byte(acceptedNative), deviceObservation(), fakeResolve)
	})
	original := desiredDevices()
	original.Devices = original.Devices[:1]
	if _, err := controller.Select(context.Background(), original); err != nil {
		t.Fatal(err)
	}
	priorBytes, err := os.ReadFile(controller.desiredPath)
	if err != nil {
		t.Fatal(err)
	}
	baseline := commands
	released := 0
	requested, _ := normalizeDesired(desiredDevices())
	input, _, err := BuildFromAccepted(context.Background(), requested, []byte(acceptedNative), deviceObservation(), fakeResolve)
	if err != nil {
		t.Fatal(err)
	}
	plan, err := proxy.PlanOwnedRules(input)
	if err != nil {
		t.Fatal(err)
	}
	desiredRaw, _ := json.Marshal(requested)
	journalRaw, _ := json.Marshal(journal{OwnedRulesPlan: plan, Input: &input})
	controller.SetStorageAdmission(func(ctx context.Context, path string, size int64, recovery bool) (func(), error) {
		if size != int64(len(desiredRaw)+len(journalRaw))+8192 || recovery {
			t.Fatalf("selection not admitted together: %d %v", size, recovery)
		}
		return func() { released++ }, storage.ErrInsufficientSpace
	})
	_, err = controller.Select(context.Background(), requested)
	if !errors.Is(err, storage.ErrInsufficientSpace) || released != 1 || commands != baseline || !reflect.DeepEqual(controller.Desired(), original) {
		t.Fatalf("denied selection changed live/desired scope: commands=%d baseline=%d desired=%+v err=%v", commands, baseline, controller.Desired(), err)
	}
	afterBytes, _ := os.ReadFile(controller.desiredPath)
	if !reflect.DeepEqual(priorBytes, afterBytes) {
		t.Fatal("denied selection replaced saved state")
	}
}

func TestStorageReserveAllowsRecoveryDisableAndReleaseAfterCleanup(t *testing.T) {
	for _, retain := range []bool{false, true} {
		t.Run(map[bool]string{false: "delete", true: "native-retained"}[retain], func(t *testing.T) {
			controller := testController(t, idleRunner)
			controller.SetBuilder(func(ctx context.Context, d Desired) (proxy.RulesPlanInput, []Client, error) {
				return BuildFromAccepted(ctx, d, []byte(acceptedNative), deviceObservation(), fakeResolve)
			})
			if _, err := controller.Select(context.Background(), desiredDevices()); err != nil {
				t.Fatal(err)
			}
			available := int64(128 << 10)
			budget, err := storage.New(storage.Options{ReserveBytes: 256 << 10, Measure: func(string) (storage.Space, error) { return storage.Space{Volume: 1, Available: available}, nil }})
			if err != nil {
				t.Fatal(err)
			}
			held, released := false, 0
			controller.SetStorageAdmission(func(ctx context.Context, path string, size int64, recovery bool) (func(), error) {
				if !recovery {
					t.Fatal("off switch spent ordinary admission")
				}
				release, err := budget.Admit(ctx, path, size, recovery)
				if err != nil {
					return release, err
				}
				held = true
				return func() { held = false; released++; release() }, nil
			})
			cleanupCommands := 0
			controller.runner = func(ctx context.Context, args []string) ([]byte, error) {
				if !held {
					t.Fatal("recovery reservation released before cleanup")
				}
				cleanupCommands++
				return idleRunner(ctx, args)
			}
			if retain {
				err = controller.DisableRetainingSelection(context.Background())
			} else {
				err = controller.Disable(context.Background())
			}
			if err != nil || held || released != 1 || cleanupCommands == 0 || controller.Status().Active || controller.Status().Desired {
				t.Fatalf("disable recovery failed: held=%v release=%d cleanup=%d status=%+v err=%v", held, released, cleanupCommands, controller.Status(), err)
			}
			reopened, err := New(filepath.Dir(controller.path), nil)
			if err != nil || reopened.Status().Desired {
				t.Fatal("off switch was not persisted", err)
			}
			if retain && len(reopened.Status().Clients) != 2 {
				t.Fatal("native disabled selection not retained")
			}
			if !retain && len(reopened.Status().Clients) != 0 {
				t.Fatal("DELETE did not clear selection")
			}
			if _, err = os.Stat(controller.path); !os.IsNotExist(err) {
				t.Fatal("cleanup kept journal", err)
			}
			available = 512 << 10
			release, err := budget.Admit(context.Background(), filepath.Dir(controller.path), 1, false)
			if err != nil {
				t.Fatal("recovery leaked shared reservation", err)
			}
			release()
		})
	}
}

func TestStorageAdmissionReleasesSuccessFailureCancellationAndCleanup(t *testing.T) {
	for _, mode := range []string{"success", "preflight", "save", "partial-apply", "canceled-admission"} {
		t.Run(mode, func(t *testing.T) {
			held, released, calls := false, 0, 0
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			controller := testController(t, func(ctx context.Context, args []string) ([]byte, error) {
				calls++
				if !held {
					t.Fatal("journal admission was released before command/rollback")
				}
				if mode == "preflight" {
					return nil, errors.New("read failed")
				}
				if mode == "partial-apply" && len(args) > 5 && args[5] == "-N" {
					return nil, errors.New("mutation failed")
				}
				return idleRunner(ctx, args)
			})
			if mode == "save" {
				if err := os.Mkdir(controller.path, 0700); err != nil {
					t.Fatal(err)
				}
			}
			controller.SetStorageAdmission(func(ctx context.Context, path string, size int64, recovery bool) (func(), error) {
				held = true
				if mode == "canceled-admission" {
					cancel()
				}
				return func() { held = false; released++ }, nil
			})
			state, err := controller.Apply(ctx, testInput())
			if held || released != 1 {
				t.Fatalf("reservation leaked mode=%s held=%v released=%d", mode, held, released)
			}
			if mode == "success" {
				if err != nil || !state.Active {
					t.Fatal(err, state)
				}
			} else if err == nil {
				t.Fatal("failure hidden")
			}
			if mode == "canceled-admission" && (!errors.Is(err, context.Canceled) || calls != 0) {
				t.Fatal("canceled admission ran command", calls, err)
			}
			if mode == "success" {
				controller.SetStorageAdmission(func(context.Context, string, int64, bool) (func(), error) {
					t.Fatal("cleanup removal must not allocate")
					return nil, nil
				})
				controller.runner = idleRunner
				if err := controller.Cleanup(context.Background()); err != nil {
					t.Fatal(err)
				}
			}
		})
	}
}

func TestStorageUnresolvedDesiredOnlyAdmissionBeforeSaving(t *testing.T) {
	controller := testController(t, func(context.Context, []string) ([]byte, error) {
		t.Fatal("unresolved selection ran command")
		return nil, nil
	})
	desired := desiredDevices()
	controller.SetBuilder(func(context.Context, Desired) (proxy.RulesPlanInput, []Client, error) {
		return proxy.RulesPlanInput{}, desiredClients(desired), errors.New("capture_device_unresolved")
	})
	raw, _ := json.Marshal(desired)
	releases := 0
	controller.SetStorageAdmission(func(ctx context.Context, path string, size int64, recovery bool) (func(), error) {
		if size != int64(len(raw))+4096 || recovery {
			t.Fatal(size, recovery)
		}
		return func() { releases++ }, nil
	})
	state, err := controller.Select(context.Background(), desired)
	if err == nil || state.Active || !state.Desired || releases != 1 {
		t.Fatal("pending saved scope admission failed", state, err, releases)
	}
}

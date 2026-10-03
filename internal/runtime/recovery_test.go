package runtime

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"sync/atomic"
	"syscall"
	"testing"
	"time"

	"be6500panel/internal/storage"
)

func TestFailedLiveConfigRestoresProvenReadyAndOwnedResources(t *testing.T) {
	var manager *Manager
	var ready atomic.Bool
	var restores atomic.Int32
	manager, opts := testManager(t, func(o *Options) {
		o.ReadyHook = func(ctx context.Context, id string) error {
			raw, _, err := manager.Config(id)
			if err != nil {
				return err
			}
			if string(raw) != "old-private-node" {
				return errors.New("candidate-private-listener-detail")
			}
			ready.Store(true)
			return nil
		}
		o.CleanupHook = func(context.Context, string) error { ready.Store(false); return nil }
		o.RestoreHook = func(ctx context.Context, id string) error {
			if !ready.Load() {
				t.Fatal("owned restore ran before readiness")
			}
			raw, _, err := manager.Config(id)
			if err != nil {
				return err
			}
			if string(raw) != "old-private-node" {
				t.Fatalf("wrong owned restore config: %q", raw)
			}
			restores.Add(1)
			return nil
		}
	})
	acquireFixture(t, manager, opts, SingBox, fixture)
	accepted(t, manager, SingBox, "old-private-node", 0)
	first, err := manager.Start(context.Background(), SingBox)
	if err != nil {
		t.Fatal(err)
	}
	status, err := manager.Configure(context.Background(), SingBox, []byte("new-private-node"), 1)
	if !errors.Is(err, ErrReadiness) || errors.Is(err, ErrRecovery) || status.State != Running || !status.Desired || !status.Restored || status.NeedsRecovery || status.Generation != 3 || status.PID == 0 || status.PID == first.PID || status.ErrorCode != "readiness_failed" {
		t.Fatalf("failed change did not recover: %+v %v", status, err)
	}
	assertGone(t, first.PID)
	if restores.Load() != 2 {
		t.Fatalf("owned restoration not resumed: %d", restores.Load())
	}
	raw, generation, err := manager.Config(SingBox)
	if err != nil || generation != 3 || string(raw) != "old-private-node" {
		t.Fatalf("rollback bytes/generation: %q %d %v", raw, generation, err)
	}
	loaded, err := loadState(manager.opts, SingBox)
	if err != nil || loaded.Generation != 3 || !loaded.Current.Ready || loaded.LastGood == nil || !loaded.LastGood.Ready {
		t.Fatalf("proof not durable: %+v %v", loaded, err)
	}
	public, _ := json.Marshal(status)
	if strings.Contains(string(public), "private") || strings.Contains(errString(err), "private") {
		t.Fatalf("private recovery diagnostic leaked: %s %v", public, err)
	}
	if _, err := manager.Configure(context.Background(), SingBox, []byte("stale"), 2); !errors.Is(err, ErrGeneration) {
		t.Fatal("rollback allowed stale generation", err)
	}
	if status, _ := manager.Status(SingBox); status.Generation != 3 || !status.Restored {
		t.Fatal(status)
	}
}

func TestCheckOnlyConfigurationsAreNotLastGoodOrAutoStarted(t *testing.T) {
	var readiness atomic.Int32
	manager, opts := testManager(t, func(o *Options) {
		o.ReadyHook = func(context.Context, string) error { readiness.Add(1); return errors.New("not ready") }
	})
	acquireFixture(t, manager, opts, SingBox, fixture)
	accepted(t, manager, SingBox, "never-ran-first", 0)
	status := accepted(t, manager, SingBox, "never-ran-second", 1)
	if status.State != Stopped || status.PID != 0 || status.Desired || readiness.Load() != 0 {
		t.Fatal("check-only edit auto-started", status)
	}
	manager.mu.Lock()
	disk := manager.services[SingBox].disk
	manager.mu.Unlock()
	if disk.LastGood != nil || disk.Current.Ready {
		t.Fatal("check-only config became proven-ready", disk)
	}
	if _, err := manager.Restore(context.Background(), SingBox, 2); err == nil {
		t.Fatal("restored never-ready config")
	}
	status, err := manager.Start(context.Background(), SingBox)
	if !errors.Is(err, ErrReadiness) || status.Restored || status.Generation != 2 || status.PID != 0 {
		t.Fatalf("first failed start incorrectly recovered: %+v %v", status, err)
	}
}

func TestFailedRecoveryReportsOriginalFailureAndNeedsRecovery(t *testing.T) {
	var manager *Manager
	var rejectOld atomic.Bool
	manager, opts := testManager(t, func(o *Options) {
		o.ReadyHook = func(ctx context.Context, id string) error {
			raw, _, err := manager.Config(id)
			if err != nil {
				return err
			}
			if string(raw) == "new" || rejectOld.Load() {
				return errors.New("private rejection")
			}
			return nil
		}
	})
	acquireFixture(t, manager, opts, SingBox, fixture)
	accepted(t, manager, SingBox, "old", 0)
	if _, err := manager.Start(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
	rejectOld.Store(true)
	status, err := manager.Configure(context.Background(), SingBox, []byte("new"), 1)
	if !errors.Is(err, ErrReadiness) || !errors.Is(err, ErrRecovery) || status.State != Error || status.Restored || !status.NeedsRecovery || status.PID != 0 || status.Desired || status.Generation != 3 || status.ErrorCode != "readiness_failed" {
		t.Fatalf("recovery failure hidden: %+v %v", status, err)
	}
	raw, generation, err := manager.Config(SingBox)
	if err != nil || generation != 3 || string(raw) != "old" {
		t.Fatalf("recovery lost proven bytes: %s %d %v", raw, generation, err)
	}
	rejectOld.Store(false)
	status, err = manager.Start(context.Background(), SingBox)
	if err != nil || status.State != Running || status.NeedsRecovery || status.ErrorCode != "" {
		t.Fatalf("explicit recovery failed: %+v %v", status, err)
	}
}

func TestFailedLiveChangeDoesNotMaskOrRetryCleanupFailure(t *testing.T) {
	for _, failedAfterReadiness := range []bool{false, true} {
		t.Run(map[bool]string{false: "old-stop", true: "candidate-stop"}[failedAfterReadiness], func(t *testing.T) {
			var manager *Manager
			var failCleanup atomic.Bool
			var cleanups atomic.Int32
			var restores atomic.Int32
			manager, opts := testManager(t, func(o *Options) {
				o.CleanupHook = func(context.Context, string) error {
					cleanups.Add(1)
					if failCleanup.Load() {
						return errors.New("private cleanup detail")
					}
					return nil
				}
				o.ReadyHook = func(ctx context.Context, id string) error {
					raw, _, err := manager.Config(id)
					if err != nil {
						return err
					}
					if string(raw) == "new" {
						failCleanup.Store(true)
						return errors.New("private listener detail")
					}
					return nil
				}
				o.RestoreHook = func(context.Context, string) error { restores.Add(1); return nil }
			})
			acquireFixture(t, manager, opts, SingBox, fixture)
			accepted(t, manager, SingBox, "old", 0)
			first, err := manager.Start(context.Background(), SingBox)
			if err != nil {
				t.Fatal(err)
			}
			t.Cleanup(func() { failCleanup.Store(false) })
			before := cleanups.Load()
			if !failedAfterReadiness {
				failCleanup.Store(true)
			}
			status, err := manager.Configure(context.Background(), SingBox, []byte("new"), 1)
			if err == nil || errors.Is(err, ErrRecovery) || status.ErrorCode != "cleanup_failed" || status.PID <= 0 || !status.Desired || status.Restored || !status.NeedsRecovery || status.Generation != 2 || restores.Load() != 1 {
				t.Fatalf("cleanup failure masked: %+v %v", status, err)
			}
			if !failedAfterReadiness && status.PID != first.PID {
				t.Fatal("old-stop lost retained original PID", status)
			}
			if failedAfterReadiness && status.PID == first.PID {
				t.Fatal("candidate-stop did not retain actual candidate PID", status)
			}
			if syscall.Kill(status.PID, 0) != nil {
				t.Fatal("retained process is gone", status)
			}
			if failedAfterReadiness {
				assertGone(t, first.PID)
			}
			if failedAfterReadiness && !errors.Is(err, ErrReadiness) {
				t.Fatal("original readiness error discarded", err)
			}
			want := int32(1)
			if failedAfterReadiness {
				want = 2
			}
			if cleanups.Load() != before+want {
				t.Fatalf("cleanup retried implicitly: %d -> %d", before, cleanups.Load())
			}
			raw, _, err := manager.Config(SingBox)
			if err != nil || string(raw) != "new" {
				t.Fatal("refused withdrawal rewrote accepted bytes", err)
			}
			failCleanup.Store(false)
			if stopped, err := manager.Stop(context.Background(), SingBox); err != nil || stopped.PID != 0 || stopped.NeedsRecovery {
				t.Fatalf("cleanup retry failed: %+v %v", stopped, err)
			}
			if _, err := manager.Restore(context.Background(), SingBox, 2); err != nil {
				t.Fatal(err)
			}
			if status, err := manager.Start(context.Background(), SingBox); err != nil || status.NeedsRecovery || status.State != Running {
				t.Fatalf("explicit previous-config recovery failed: %+v %v", status, err)
			}
		})
	}
}

func TestCanceledLiveConfigRecoversOnManagerContext(t *testing.T) {
	var manager *Manager
	entered := make(chan struct{})
	manager, opts := testManager(t, func(o *Options) {
		o.ReadyHook = func(ctx context.Context, id string) error {
			raw, _, err := manager.Config(id)
			if err != nil {
				return err
			}
			if string(raw) == "new" {
				close(entered)
				<-ctx.Done()
				return ctx.Err()
			}
			return ctx.Err()
		}
	})
	acquireFixture(t, manager, opts, SingBox, fixture)
	accepted(t, manager, SingBox, "old", 0)
	if _, err := manager.Start(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	type outcome struct {
		status Status
		err    error
	}
	completed := make(chan outcome, 1)
	go func() {
		status, err := manager.Configure(ctx, SingBox, []byte("new"), 1)
		completed <- outcome{status, err}
	}()
	<-entered
	status, _ := manager.Status(SingBox)
	candidatePID := status.PID
	cancel()
	select {
	case result := <-completed:
		if !errors.Is(result.err, context.Canceled) || errors.Is(result.err, ErrRecovery) || !result.status.Restored || result.status.NeedsRecovery || result.status.State != Running || !result.status.Desired || result.status.Generation != 3 {
			t.Fatalf("request cancellation stranded old service: %+v %v", result.status, result.err)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("cancellation recovery was not bounded")
	}
	assertGone(t, candidatePID)
}

func TestOwnedRestoreFailureDoesNotClaimFullAutomaticRecovery(t *testing.T) {
	var manager *Manager
	var failRestore atomic.Bool
	manager, opts := testManager(t, func(o *Options) {
		o.ReadyHook = func(ctx context.Context, id string) error {
			raw, _, err := manager.Config(id)
			if err != nil {
				return err
			}
			if string(raw) == "new" {
				return ErrReadiness
			}
			return nil
		}
		o.RestoreHook = func(context.Context, string) error {
			if failRestore.Load() {
				return errors.New("private owned detail")
			}
			return nil
		}
	})
	acquireFixture(t, manager, opts, SingBox, fixture)
	accepted(t, manager, SingBox, "old", 0)
	if _, err := manager.Start(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
	failRestore.Store(true)
	status, err := manager.Configure(context.Background(), SingBox, []byte("new"), 1)
	if !errors.Is(err, ErrReadiness) || !errors.Is(err, ErrRecovery) || !status.Restored || !status.NeedsRecovery || status.State != Running || !status.Desired || status.ErrorCode != "readiness_failed" {
		t.Fatalf("owned recovery failure hidden: %+v %v", status, err)
	}
	failRestore.Store(false)
	status, err = manager.Start(context.Background(), SingBox)
	if err != nil || status.NeedsRecovery {
		t.Fatalf("owned restore did not retry: %+v %v", status, err)
	}
}

func TestFailedArtifactActivationRecoversPreviousExecutable(t *testing.T) {
	var manager *Manager
	manager, opts := testManager(t, func(o *Options) {
		o.ReadyHook = func(ctx context.Context, id string) error {
			manager.mu.Lock()
			binary := manager.services[id].binary
			manager.mu.Unlock()
			raw, err := os.ReadFile(binary)
			if err != nil {
				return err
			}
			if strings.Contains(string(raw), "# failing-activation") {
				return errors.New("private listener rejection")
			}
			return nil
		}
	})
	original := acquireFixture(t, manager, opts, SingBox, fixture)
	accepted(t, manager, SingBox, "old", 0)
	first, err := manager.Start(context.Background(), SingBox)
	if err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(opts.LocalSourceRoot, "failing-upgrade")
	body := []byte(fixture + "\n# failing-activation\n")
	if err := os.WriteFile(path, body, 0700); err != nil {
		t.Fatal(err)
	}
	hash := sha256.Sum256(body)
	status, err := manager.Acquire(context.Background(), SingBox, Artifact{URL: "file://" + path, SHA256: hex.EncodeToString(hash[:]), Compression: "none", Version: "failed-upgrade"})
	if !errors.Is(err, ErrReadiness) || errors.Is(err, ErrRecovery) || status.State != Running || !status.Restored || status.NeedsRecovery || status.Generation != 2 || status.Version != original.Version || status.PID == 0 || status.PID == first.PID {
		t.Fatalf("artifact activation stranded old core: %+v %v", status, err)
	}
	assertGone(t, first.PID)
	entries, err := os.ReadDir(opts.RunDir)
	if err != nil {
		t.Fatal(err)
	}
	binaries := 0
	for _, entry := range entries {
		if strings.HasPrefix(entry.Name(), ".artifact-") {
			binaries++
		}
	}
	if binaries > 1 {
		t.Fatalf("abandoned artifact remains: %v", entries)
	}
}

func TestFailedExplicitRestoreRecoversPreviouslyRunningConfig(t *testing.T) {
	var manager *Manager
	var rejectFirst atomic.Bool
	manager, opts := testManager(t, func(o *Options) {
		o.ReadyHook = func(ctx context.Context, id string) error {
			raw, _, err := manager.Config(id)
			if err != nil {
				return err
			}
			if string(raw) == "first" && rejectFirst.Load() {
				return ErrReadiness
			}
			return nil
		}
	})
	acquireFixture(t, manager, opts, SingBox, fixture)
	accepted(t, manager, SingBox, "first", 0)
	if _, err := manager.Start(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
	accepted(t, manager, SingBox, "second", 1)
	rejectFirst.Store(true)
	status, err := manager.Restore(context.Background(), SingBox, 2)
	if !errors.Is(err, ErrReadiness) || errors.Is(err, ErrRecovery) || status.Generation != 4 || !status.Restored || status.NeedsRecovery || status.State != Running {
		t.Fatalf("explicit restore failure stranded working config: %+v %v", status, err)
	}
	raw, _, err := manager.Config(SingBox)
	if err != nil || string(raw) != "second" {
		t.Fatal("explicit restore did not recover working bytes", err)
	}
}

func TestLargerReadyConfigRecoverySpaceIsReservedBeforeCommit(t *testing.T) {
	manager, opts := testManager(t, func(o *Options) {
		o.ReadyHook = func(context.Context, string) error { return nil }
	})
	acquireFixture(t, manager, opts, SingBox, fixture)
	old := append([]byte("old-"), bytes.Repeat([]byte{'x'}, 128<<10)...)
	status, err := manager.Configure(context.Background(), SingBox, old, 0)
	if err != nil {
		t.Fatal(err)
	}
	status, err = manager.Start(context.Background(), SingBox)
	if err != nil {
		t.Fatal(err)
	}
	pid := status.PID
	free := int64(storage.DefaultReserveBytes + 64<<10)
	budget := runtimeBudget(t, &free)
	manager.opts.StorageAdmission = budget.Admit
	status, err = manager.Configure(context.Background(), SingBox, []byte("new"), 1)
	if !errors.Is(err, storage.ErrInsufficientSpace) || status.Generation != 1 || status.PID != pid || status.State != Running || status.Restored || status.ErrorCode != "storage_insufficient" {
		t.Fatalf("recovery scratch denied after accepted commit: %+v %v", status, err)
	}
	raw, generation, err := manager.Config(SingBox)
	if err != nil || generation != 1 || !bytes.Equal(raw, old) {
		t.Fatal("preflight changed accepted bytes", generation, err)
	}
}

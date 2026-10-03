package runtime

import (
	"context"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"sync/atomic"
	"syscall"
	"testing"
	"time"
)

func restartRunning(t *testing.T, m *Manager, opts Options, id string) Status {
	t.Helper()
	acquireFixture(t, m, opts, id, fixture)
	accepted(t, m, id, "good-private-config", 0)
	status, err := m.Start(context.Background(), id)
	if err != nil {
		t.Fatal(err)
	}
	return status
}

func restartReadback(t *testing.T, m *Manager, id string, expected Status) {
	t.Helper()
	actual, err := m.Status(id)
	if err != nil {
		t.Fatal(err)
	}
	if actual.Service != expected.Service || actual.State != expected.State || actual.PID != expected.PID || actual.Generation != expected.Generation || actual.Desired != expected.Desired || actual.Restarts != expected.Restarts || !actual.RetryAt.Equal(expected.RetryAt) || actual.ErrorCode != expected.ErrorCode || actual.Restored != expected.Restored || actual.NeedsRecovery != expected.NeedsRecovery {
		t.Fatalf("restart result disagrees with status: result %+v readback %+v", expected, actual)
	}
}

func TestRestartReplacesProcessWhileStartIsIdempotent(t *testing.T) {
	for _, id := range []string{SingBox, FRPC} {
		t.Run(id, func(t *testing.T) {
			var cleaned, restored, ready atomic.Int32
			m, opts := testManager(t, func(o *Options) {
				o.CleanupHook = func(context.Context, string) error { cleaned.Add(1); return nil }
				o.RestoreHook = func(context.Context, string) error { restored.Add(1); return nil }
				o.ReadyHook = func(context.Context, string) error { ready.Add(1); return nil }
			})
			original := restartRunning(t, m, opts, id)
			same, err := m.Start(context.Background(), id)
			if err != nil || same.PID != original.PID {
				t.Fatalf("Start not idempotent %+v %v", same, err)
			}
			cleanBefore, restoreBefore, readyBefore := cleaned.Load(), restored.Load(), ready.Load()
			m.mu.Lock()
			s := m.services[id]
			epoch := s.epoch
			s.restarts = 2
			s.retryAt = time.Now().Add(time.Hour)
			m.mu.Unlock()
			next, err := m.Restart(context.Background(), id)
			if err != nil || next.State != Running || next.PID <= 0 || next.PID == original.PID || !next.Desired || next.Restarts != 0 || !next.RetryAt.IsZero() || next.Generation != original.Generation || next.ErrorCode != "" {
				t.Fatalf("restart %+v %v", next, err)
			}
			restartReadback(t, m, id, next)
			assertGone(t, original.PID)
			if cleaned.Load() != cleanBefore+1 || restored.Load() != restoreBefore+1 || ready.Load() != readyBefore+1 {
				t.Fatalf("owned lifecycle cleanup%d restore%d ready%d", cleaned.Load(), restored.Load(), ready.Load())
			}
			m.mu.Lock()
			newEpoch := s.epoch
			m.mu.Unlock()
			if newEpoch <= epoch {
				t.Fatal("old watcher epoch reused")
			}
			raw, generation, err := m.Config(id)
			if err != nil || generation != 1 || string(raw) != "good-private-config" {
				t.Fatalf("accepted changed %q %d %v", raw, generation, err)
			}
		})
	}
}
func TestRestartInvalidAcceptedConfigPreservesWorkingProcess(t *testing.T) {
	for _, kind := range []string{"invalid", "missing"} {
		t.Run(kind, func(t *testing.T) {
			var cleaned atomic.Int32
			m, opts := testManager(t, func(o *Options) { o.CleanupHook = func(context.Context, string) error { cleaned.Add(1); return nil } })
			original := restartRunning(t, m, opts, SingBox)
			baseline := cleaned.Load()
			m.mu.Lock()
			record := m.services[SingBox].disk.Current
			m.mu.Unlock()
			path := configPath(opts, SingBox, record)
			if kind == "missing" {
				if err := os.Remove(path); err != nil {
					t.Fatal(err)
				}
			} else {
				if err := os.WriteFile(path, []byte("bad-private-secret"), 0600); err != nil {
					t.Fatal(err)
				}
			}
			status, err := m.Restart(context.Background(), SingBox)
			if err == nil || status.State != Running || status.PID != original.PID || !status.Desired || status.Generation != original.Generation || status.ErrorCode != "config_check_failed" || cleaned.Load() != baseline {
				t.Fatalf("invalid config killed core %+v %v", status, err)
			}
			public, _ := json.Marshal(status)
			if strings.Contains(string(public), "secret") || strings.Contains(errString(err), "credential") {
				t.Fatal("private checker output leaked")
			}
		})
	}
}
func TestRestartCancellationBeforeStopKeepsWorkingProcess(t *testing.T) {
	m, opts := testManager(t, nil)
	original := restartRunning(t, m, opts, FRPC)
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	status, err := m.Restart(ctx, FRPC)
	if !errors.Is(err, context.Canceled) || status.PID != original.PID || status.State != Running {
		t.Fatalf("canceled %+v %v", status, err)
	}
	m.mu.Lock()
	record := m.services[FRPC].disk.Current
	m.mu.Unlock()
	if err := os.WriteFile(configPath(opts, FRPC, record), []byte("slow-private"), 0600); err != nil {
		t.Fatal(err)
	}
	ctx, cancel = context.WithCancel(context.Background())
	defer cancel()
	finished := make(chan error, 1)
	go func() { _, err := m.Restart(ctx, FRPC); finished <- err }()
	waitStatus(t, m, FRPC, func(s Status) bool { return s.State == Checking })
	waitFile(t, filepath.Join(opts.RunDir, "check.pid"))
	cancel()
	select {
	case err := <-finished:
		if !errors.Is(err, context.Canceled) {
			t.Fatal(err)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("preflight cancellation did not finish")
	}
	status, _ = m.Status(FRPC)
	if status.PID != original.PID || status.State != Running || !status.Desired || status.ErrorCode != "operation_cancelled" {
		t.Fatalf("preflight cancellation killed core %+v", status)
	}
}
func TestRestartCancellationAfterCleanupDoesNotStartReplacement(t *testing.T) {
	var cancelOperation atomic.Bool
	var cancel context.CancelFunc
	m, opts := testManager(t, func(o *Options) {
		o.CleanupHook = func(context.Context, string) error {
			if cancelOperation.Load() {
				cancel()
			}
			return nil
		}
	})
	original := restartRunning(t, m, opts, SingBox)
	ctx, stop := context.WithCancel(context.Background())
	cancel = stop
	defer stop()
	cancelOperation.Store(true)
	status, err := m.Restart(ctx, SingBox)
	if !errors.Is(err, context.Canceled) || status.PID != 0 || status.State != Stopped || status.Desired || status.Restarts != 0 || !status.RetryAt.IsZero() || status.ErrorCode != "operation_cancelled" {
		t.Fatalf("poststop cancellation %+v %v", status, err)
	}
	assertGone(t, original.PID)
	cancelOperation.Store(false)
}
func TestRestartSingleLaneCoversStopAndReadiness(t *testing.T) {
	cleanupEntered := make(chan struct{}, 1)
	releaseCleanup := make(chan struct{})
	readyEntered := make(chan struct{}, 1)
	releaseReady := make(chan struct{})
	var restarting atomic.Bool
	m, opts := testManager(t, func(o *Options) {
		o.CleanupHook = func(ctx context.Context, id string) error {
			if restarting.Load() {
				cleanupEntered <- struct{}{}
				select {
				case <-releaseCleanup:
				case <-ctx.Done():
					return ctx.Err()
				}
			}
			return nil
		}
		o.ReadyHook = func(ctx context.Context, id string) error {
			if restarting.Load() {
				readyEntered <- struct{}{}
				select {
				case <-releaseReady:
				case <-ctx.Done():
					return ctx.Err()
				}
			}
			return nil
		}
	})
	original := restartRunning(t, m, opts, SingBox)
	var cleanupRelease, readyRelease sync.Once
	unblockCleanup := func() { cleanupRelease.Do(func() { close(releaseCleanup) }) }
	unblockReady := func() { readyRelease.Do(func() { close(releaseReady) }) }
	t.Cleanup(func() {
		restarting.Store(false)
		unblockCleanup()
		unblockReady()
	})
	restarting.Store(true)
	finished := make(chan error, 1)
	go func() { _, err := m.Restart(context.Background(), SingBox); finished <- err }()
	select {
	case <-cleanupEntered:
	case <-time.After(2 * time.Second):
		t.Fatal("cleanup not reached")
	}
	for _, id := range []string{SingBox, FRPC} {
		if _, err := m.Restart(context.Background(), id); !errors.Is(err, ErrBusy) {
			t.Fatalf("nested restart %s %v", id, err)
		}
		if _, err := m.Stop(context.Background(), id); !errors.Is(err, ErrBusy) {
			t.Fatalf("interleaved stop %s %v", id, err)
		}
	}
	if _, err := m.Configure(context.Background(), SingBox, []byte("other"), 1); !errors.Is(err, ErrBusy) {
		t.Fatal("configuration interleaved", err)
	}
	assertGone(t, original.PID)
	unblockCleanup()
	select {
	case <-readyEntered:
	case <-time.After(2 * time.Second):
		t.Fatal("readiness not reached")
	}
	status, _ := m.Status(SingBox)
	if status.State != Starting || status.PID <= 0 || status.PID == original.PID {
		t.Fatalf("not actual starting %+v", status)
	}
	if _, err := m.Start(context.Background(), FRPC); !errors.Is(err, ErrBusy) {
		t.Fatal("lane ended before readiness", err)
	}
	unblockReady()
	select {
	case err := <-finished:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("restart did not finish after readiness")
	}
	restarting.Store(false)
}
func TestRestartCleanupReadinessAndLaunchFailuresRemainActual(t *testing.T) {
	for _, kind := range []string{"cleanup", "readiness", "launch"} {
		t.Run(kind, func(t *testing.T) {
			var failing atomic.Bool
			m, opts := testManager(t, func(o *Options) {
				o.CleanupHook = func(context.Context, string) error {
					if failing.Load() && kind == "cleanup" {
						return errors.New("private cleanup")
					}
					return nil
				}
				o.ReadyHook = func(context.Context, string) error {
					if failing.Load() && kind == "readiness" {
						return errors.New("private readiness")
					}
					return nil
				}
				if kind == "launch" {
					o.CleanupHook = func(context.Context, string) error {
						if failing.Load() {
							mPath := filepath.Join(o.RunDir, "remove-binary")
							raw, err := os.ReadFile(mPath)
							if err != nil {
								return err
							}
							return os.Remove(string(raw))
						}
						return nil
					}
				}
			})
			original := restartRunning(t, m, opts, FRPC)
			if kind == "launch" {
				m.mu.Lock()
				binary := m.services[FRPC].binary
				m.mu.Unlock()
				if err := os.WriteFile(filepath.Join(opts.RunDir, "remove-binary"), []byte(binary), 0600); err != nil {
					t.Fatal(err)
				}
			}
			failing.Store(true)
			status, err := m.Restart(context.Background(), FRPC)
			expected := map[string]string{"cleanup": "cleanup_failed", "readiness": "readiness_failed", "launch": "start_failed"}[kind]
			if err == nil || status.PID != 0 || status.State != Error || status.Desired || status.ErrorCode != expected || !status.RetryAt.IsZero() {
				t.Fatalf("failure actual %+v %v", status, err)
			}
			if kind == "cleanup" && !status.NeedsRecovery {
				t.Fatal("cleanup recovery hidden")
			}
			restartReadback(t, m, FRPC, status)
			raw, generation, configErr := m.Config(FRPC)
			if configErr != nil || generation != original.Generation || string(raw) != "good-private-config" {
				t.Fatalf("failed restart changed accepted config %q %d %v", raw, generation, configErr)
			}
			public, _ := json.Marshal(status)
			if strings.Contains(string(public), "private") || strings.Contains(errString(err), "private") {
				t.Fatalf("private lifecycle diagnostic leaked: %s %v", public, err)
			}
			assertGone(t, original.PID)
			failing.Store(false)
		})
	}
}
func TestRestartFixedServiceAndPrerequisiteChecks(t *testing.T) {
	m, _ := testManager(t, nil)
	for _, id := range []string{"", "../frpc", "rescue", "be6500-rescue", "dnsmasq", "sing-box --help"} {
		if _, err := m.Restart(context.Background(), id); !errors.Is(err, ErrService) {
			t.Fatalf("allowed %q %v", id, err)
		}
	}
	if _, err := m.Restart(nil, FRPC); err == nil {
		t.Fatal("nil context accepted")
	}
	if status, err := m.Restart(context.Background(), FRPC); !errors.Is(err, ErrNotConfigured) || status.State != NotConfigured || status.PID != 0 || status.Desired {
		t.Fatalf("unconfigured restart %+v %v", status, err)
	}
	m.mu.Lock()
	m.services[FRPC].disk.Current = &configRecord{}
	m.mu.Unlock()
	if _, err := m.Restart(context.Background(), FRPC); !errors.Is(err, ErrNoArtifact) {
		t.Fatal(err)
	}
	if err := m.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := m.Restart(context.Background(), SingBox); !errors.Is(err, ErrClosed) {
		t.Fatal(err)
	}
}

func TestRestartStartsStoppedAcceptedService(t *testing.T) {
	m, opts := testManager(t, nil)
	original := restartRunning(t, m, opts, FRPC)
	if _, err := m.Stop(context.Background(), FRPC); err != nil {
		t.Fatal(err)
	}
	assertGone(t, original.PID)
	next, err := m.Restart(context.Background(), FRPC)
	if err != nil || next.State != Running || next.PID <= 0 || !next.Desired || next.Generation != original.Generation {
		t.Fatalf("stopped restart %+v %v", next, err)
	}
}
func TestRestartVerifierCannotRewriteAcceptedConfigOrStopGoodCore(t *testing.T) {
	m, opts := testManager(t, nil)
	mutableFixture := strings.Replace(fixture, "  exit 0 ;;", "  [ -f \"$TMPDIR/rewrite-check\" ] && echo 'modified-secret' > \"$3\"\n  exit 0 ;;", 1)
	acquireFixture(t, m, opts, SingBox, mutableFixture)
	accepted(t, m, SingBox, "good-private-config", 0)
	original, err := m.Start(context.Background(), SingBox)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(opts.RunDir, "rewrite-check"), []byte("yes"), 0600); err != nil {
		t.Fatal(err)
	}
	status, err := m.Restart(context.Background(), SingBox)
	if err == nil || status.PID != original.PID || status.State != Running || !status.Desired || status.ErrorCode != "config_check_failed" {
		t.Fatalf("checker mutation disturbed process %+v %v", status, err)
	}
	raw, generation, err := m.Config(SingBox)
	if err != nil || generation != 1 || string(raw) != "good-private-config" {
		t.Fatalf("checker changed accepted %q %d %v", raw, generation, err)
	}
}

func TestRestartCancellationDuringReadinessStopsReplacement(t *testing.T) {
	for _, id := range []string{SingBox, FRPC} {
		t.Run(id, func(t *testing.T) {
			var restarting atomic.Bool
			var restores atomic.Int32
			entered := make(chan int, 1)
			var manager *Manager
			manager, opts := testManager(t, func(o *Options) {
				o.ReadyHook = func(ctx context.Context, service string) error {
					if !restarting.Load() {
						return nil
					}
					status, err := manager.Status(service)
					if err != nil {
						return err
					}
					entered <- status.PID
					<-ctx.Done()
					return ctx.Err()
				}
				o.RestoreHook = func(context.Context, string) error { restores.Add(1); return nil }
			})
			original := restartRunning(t, manager, opts, id)
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			restarting.Store(true)
			t.Cleanup(func() { restarting.Store(false) })
			type outcome struct {
				status Status
				err    error
			}
			finished := make(chan outcome, 1)
			go func() {
				status, err := manager.Restart(ctx, id)
				finished <- outcome{status, err}
			}()
			var replacementPID int
			select {
			case replacementPID = <-entered:
			case <-time.After(2 * time.Second):
				t.Fatal("replacement readiness not reached")
			}
			if replacementPID <= 0 || replacementPID == original.PID {
				t.Fatalf("invalid replacement PID %d", replacementPID)
			}
			cancel()
			select {
			case result := <-finished:
				if !errors.Is(result.err, context.Canceled) || result.status.State != Error || result.status.PID != 0 || result.status.Desired || result.status.Restarts != 0 || !result.status.RetryAt.IsZero() || result.status.ErrorCode != "readiness_failed" {
					t.Fatalf("canceled readiness left replacement active: %+v %v", result.status, result.err)
				}
				restartReadback(t, manager, id, result.status)
			case <-time.After(2 * time.Second):
				t.Fatal("readiness cancellation did not finish")
			}
			assertGone(t, original.PID)
			assertGone(t, replacementPID)
			if restores.Load() != 1 {
				t.Fatal("owned resources restored before replacement readiness")
			}
		})
	}
}

func TestRestartOwnedRestoreFailureReportsSuspendedResources(t *testing.T) {
	var ready, failRestore atomic.Bool
	var restores atomic.Int32
	m, opts := testManager(t, func(o *Options) {
		o.CleanupHook = func(context.Context, string) error { ready.Store(false); return nil }
		o.ReadyHook = func(context.Context, string) error { ready.Store(true); return nil }
		o.RestoreHook = func(context.Context, string) error {
			if !ready.Load() {
				return errors.New("restore before readiness")
			}
			restores.Add(1)
			if failRestore.Load() {
				return errors.New("private owned resource detail")
			}
			return nil
		}
	})
	original := restartRunning(t, m, opts, SingBox)
	failRestore.Store(true)
	status, err := m.Restart(context.Background(), SingBox)
	if err != nil || status.State != Running || status.PID <= 0 || status.PID == original.PID || !status.Desired || !status.NeedsRecovery || status.ErrorCode != "resource_restore_failed" || status.Generation != original.Generation || restores.Load() != 2 {
		t.Fatalf("owned restore failure hid running core or suspended resources: %+v %v", status, err)
	}
	restartReadback(t, m, SingBox, status)
	assertGone(t, original.PID)
	public, _ := json.Marshal(status)
	if strings.Contains(string(public), "private") {
		t.Fatalf("private resource diagnostic leaked: %s", public)
	}
	failRestore.Store(false)
	recovered, err := m.Restart(context.Background(), SingBox)
	if err != nil || recovered.State != Running || recovered.PID <= 0 || recovered.PID == status.PID || !recovered.Desired || recovered.NeedsRecovery || recovered.ErrorCode != "" || restores.Load() != 3 {
		t.Fatalf("explicit restart did not restore saved resources: %+v %v", recovered, err)
	}
	restartReadback(t, m, SingBox, recovered)
	assertGone(t, status.PID)
}

func TestRestartCanceledAfterCleanupDisablesExistingBackoff(t *testing.T) {
	var restarting atomic.Bool
	cleanupEntered := make(chan struct{}, 1)
	releaseCleanup := make(chan struct{})
	m, opts := testManager(t, func(o *Options) {
		o.BackoffInitial = 500 * time.Millisecond
		o.BackoffMax = 500 * time.Millisecond
		o.CleanupHook = func(ctx context.Context, _ string) error {
			if !restarting.Load() {
				return nil
			}
			cleanupEntered <- struct{}{}
			select {
			case <-releaseCleanup:
				return nil
			case <-ctx.Done():
				return ctx.Err()
			}
		}
	})
	original := restartRunning(t, m, opts, SingBox)
	m.mu.Lock()
	process := m.services[SingBox].proc
	m.mu.Unlock()
	process.signal(syscall.SIGKILL)
	backoff := waitStatus(t, m, SingBox, func(s Status) bool { return s.State == Backoff })
	if !backoff.Desired || backoff.Restarts != 1 || backoff.RetryAt.IsZero() {
		t.Fatalf("fixture did not enter actual backoff: %+v", backoff)
	}
	assertGone(t, original.PID)
	var releaseOnce sync.Once
	unblockCleanup := func() { releaseOnce.Do(func() { close(releaseCleanup) }) }
	t.Cleanup(func() {
		restarting.Store(false)
		unblockCleanup()
	})
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	restarting.Store(true)
	type outcome struct {
		status Status
		err    error
	}
	finished := make(chan outcome, 1)
	go func() {
		status, err := m.Restart(ctx, SingBox)
		finished <- outcome{status, err}
	}()
	select {
	case <-cleanupEntered:
	case <-time.After(2 * time.Second):
		t.Fatal("restart cleanup not reached from backoff")
	}
	// Let the previous retry deadline pass while Restart still owns the lane.
	<-time.After(time.Until(backoff.RetryAt) + 30*time.Millisecond)
	cancel()
	unblockCleanup()
	select {
	case result := <-finished:
		if !errors.Is(result.err, context.Canceled) || result.status.State != Stopped || result.status.PID != 0 || result.status.Desired || result.status.Restarts != 0 || !result.status.RetryAt.IsZero() || result.status.ErrorCode != "operation_cancelled" {
			t.Fatalf("canceled restart retained backoff: %+v %v", result.status, result.err)
		}
		restartReadback(t, m, SingBox, result.status)
	case <-time.After(2 * time.Second):
		t.Fatal("restart did not finish after backoff cleanup cancellation")
	}
	restarting.Store(false)
	// A stale watcher blocked on the lane must not start after lane release.
	<-time.After(80 * time.Millisecond)
	status, err := m.Status(SingBox)
	if err != nil || status.State != Stopped || status.PID != 0 || status.Desired || !status.RetryAt.IsZero() {
		t.Fatalf("stale backoff watcher relaunched: %+v %v", status, err)
	}
}

func TestRestartGuardRunsInsideLaneBeforeAnyStateOrProcessChange(t *testing.T) {
	var cleanup, readiness, restore atomic.Int32
	m, opts := testManager(t, func(o *Options) {
		o.CleanupHook = func(context.Context, string) error { cleanup.Add(1); return nil }
		o.ReadyHook = func(context.Context, string) error { readiness.Add(1); return nil }
		o.RestoreHook = func(context.Context, string) error { restore.Add(1); return nil }
	})
	original := restartRunning(t, m, opts, SingBox)
	m.mu.Lock()
	epoch := m.services[SingBox].epoch
	m.mu.Unlock()
	cleanBefore, readyBefore, restoreBefore := cleanup.Load(), readiness.Load(), restore.Load()
	refusal := errors.New("pending native configuration")
	calls := 0
	state, err := m.RestartGuarded(context.Background(), SingBox, func(ctx context.Context) error {
		calls++
		if err := m.ResourceOperation(ctx, FRPC, func(context.Context) error { return nil }); !errors.Is(err, ErrBusy) {
			t.Fatalf("guard outside shared lane: %v", err)
		}
		return refusal
	})
	if !errors.Is(err, refusal) || calls != 1 {
		t.Fatal(state, err, calls)
	}
	restartReadback(t, m, SingBox, original)
	if state.PID != original.PID || state.State != original.State || cleanup.Load() != cleanBefore || readiness.Load() != readyBefore || restore.Load() != restoreBefore {
		t.Fatal(state)
	}
	m.mu.Lock()
	afterEpoch := m.services[SingBox].epoch
	m.mu.Unlock()
	if epoch != afterEpoch {
		t.Fatal("guard refusal changed watcher epoch")
	}
	next, err := m.RestartGuarded(context.Background(), SingBox, func(context.Context) error { return nil })
	if err != nil || next.PID == original.PID || next.State != Running {
		t.Fatal(next, err)
	}
}

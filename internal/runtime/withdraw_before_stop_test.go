package runtime

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"os"
	"path/filepath"
	goruntime "runtime"
	"strings"
	"sync/atomic"
	"syscall"
	"testing"
	"time"
)

// All withdrawal failures below are synthetic. The only children are local
// fixture processes; these tests never install or remove network resources.
func TestWithdrawBeforeStopRetainsListenerAndOriginalWatcher(t *testing.T) {
	for _, operation := range []string{"stop", "restart"} {
		t.Run(operation, func(t *testing.T) {
			var m *Manager
			var inspect, reject atomic.Bool
			var calls atomic.Int32
			var original *managedProcess
			var epoch uint64
			m, opts := testManager(t, func(o *Options) {
				o.CleanupHook = func(ctx context.Context, id string) error {
					if id != SingBox {
						return nil
					}
					if inspect.Load() {
						calls.Add(1)
						status, _ := m.Status(id) // No manager lock may cover the hook.
						m.mu.Lock()
						s := m.services[id]
						retained := s.proc == original && s.epoch == epoch && s.cancelWatch != nil && s.desired
						m.mu.Unlock()
						if !retained || status.PID != original.cmd.Process.Pid || syscall.Kill(status.PID, 0) != nil {
							t.Errorf("withdrawal lost live identity before hook: %+v retained=%v", status, retained)
						}
					} else if reject.Load() {
						calls.Add(1)
					}
					if reject.Load() {
						return errors.New("private synthetic withdrawal failure")
					}
					return nil
				}
			})
			t.Cleanup(func() { inspect.Store(false); reject.Store(false) })
			first := restartRunning(t, m, opts, SingBox)
			m.mu.Lock()
			original, epoch = m.services[SingBox].proc, m.services[SingBox].epoch
			m.mu.Unlock()
			reject.Store(true)
			inspect.Store(true)
			var status Status
			var err error
			if operation == "stop" {
				status, err = m.Stop(context.Background(), SingBox)
			} else {
				status, err = m.Restart(context.Background(), SingBox)
			}
			if err == nil || strings.Contains(err.Error(), "private") || status.PID != first.PID || status.State != Error || !status.Desired || !status.NeedsRecovery || status.ErrorCode != "cleanup_failed" || status.Generation != first.Generation || calls.Load() != 1 {
				t.Fatalf("blocked withdrawal did not retain actual listener: %+v %v calls=%d", status, err, calls.Load())
			}
			m.mu.Lock()
			retained := m.services[SingBox].proc == original && m.services[SingBox].epoch == epoch && m.services[SingBox].cancelWatch != nil
			m.mu.Unlock()
			if !retained {
				t.Fatal("failed withdrawal replaced process or watcher")
			}
			if operation == "stop" {
				reject.Store(false)
				status, err = m.Stop(context.Background(), SingBox)
				if err != nil || status.PID != 0 || status.State != Stopped || status.Desired || status.NeedsRecovery || calls.Load() != 2 {
					t.Fatalf("explicit retry: %+v %v", status, err)
				}
				assertGone(t, first.PID)
			} else {
				// The retained original supervisor must still withdraw resources on
				// unexpected exit. Keep failure fixed so it cannot launch a retry.
				inspect.Store(false)
				before := calls.Load()
				original.signal(syscall.SIGKILL)
				waitStatus(t, m, SingBox, func(s Status) bool { return s.PID == 0 && s.State == Error && !s.Desired && calls.Load() > before })
				assertGone(t, first.PID)
				reject.Store(false)
			}
		})
	}
}

func TestWithdrawBlockedAcceptedChangesKeepActualRunningIdentity(t *testing.T) {
	for _, operation := range []string{"configure", "acquire", "restore"} {
		t.Run(operation, func(t *testing.T) {
			var reject atomic.Bool
			var cleanups, readiness atomic.Int32
			m, opts := testManager(t, func(o *Options) {
				o.ReadyHook = func(context.Context, string) error { readiness.Add(1); return nil }
				o.CleanupHook = func(context.Context, string) error {
					cleanups.Add(1)
					if reject.Load() {
						return errors.New("fixed refusal")
					}
					return nil
				}
			})
			t.Cleanup(func() { reject.Store(false) })
			originalArtifact := acquireFixture(t, m, opts, SingBox, fixture)
			accepted(t, m, SingBox, "first", 0)
			if _, err := m.Start(context.Background(), SingBox); err != nil {
				t.Fatal(err)
			}
			first := accepted(t, m, SingBox, "second", 1)
			m.mu.Lock()
			process, epoch, binary := m.services[SingBox].proc, m.services[SingBox].epoch, m.services[SingBox].binary
			m.mu.Unlock()
			before := cleanups.Load()
			reject.Store(true)
			var status Status
			var err error
			wantRaw, wantGeneration := "third", uint64(3)
			switch operation {
			case "configure":
				status, err = m.Configure(context.Background(), SingBox, []byte(wantRaw), 2)
			case "restore":
				wantRaw = "first"
				status, err = m.Restore(context.Background(), SingBox, 2)
			case "acquire":
				wantRaw, wantGeneration = "second", 2
				path := filepath.Join(opts.LocalSourceRoot, "upgrade")
				body := []byte(fixture + "\n# new accepted executable\n")
				if err := os.WriteFile(path, body, 0700); err != nil {
					t.Fatal(err)
				}
				digest := sha256.Sum256(body)
				status, err = m.Acquire(context.Background(), SingBox, Artifact{URL: "file://" + path, SHA256: hex.EncodeToString(digest[:]), Compression: "none", Version: "new-accepted"})
			}
			if err == nil || status.PID != first.PID || status.State != Error || !status.Desired || !status.NeedsRecovery || status.Restored || status.ErrorCode != "cleanup_failed" || status.Generation != wantGeneration || cleanups.Load() != before+1 {
				t.Fatalf("blocked accepted change hid retained listener: %+v %v cleanups=%d", status, err, cleanups.Load()-before)
			}
			m.mu.Lock()
			retained := m.services[SingBox].proc == process && m.services[SingBox].epoch == epoch && m.services[SingBox].cancelWatch != nil
			m.mu.Unlock()
			if !retained {
				t.Fatal("blocked accepted change replaced live identity")
			}
			if _, err := os.Stat(binary); err != nil {
				t.Fatal("retained executable unlinked", err)
			}
			raw, generation, err := m.Config(SingBox)
			if err != nil || string(raw) != wantRaw || generation != wantGeneration {
				t.Fatalf("accepted manifest rolled back: %q %d %v", raw, generation, err)
			}
			if operation == "acquire" && (status.Version == originalArtifact.Version || status.Version != "new-accepted") {
				t.Fatal("accepted artifact not accounted for", status)
			}
			readyBefore := readiness.Load()
			if err := m.ReadyOperation(context.Background(), SingBox, func(context.Context) error { t.Error("ReadyOperation ran mutation for retained drift"); return nil }); !errors.Is(err, ErrReadiness) {
				t.Fatal("ReadyOperation admitted retained drift", err)
			}
			if readiness.Load() != readyBefore {
				t.Fatal("ReadyOperation called readiness for retained drift")
			}
			// Start must not silently report the requested accepted change applied
			// while the retained process still uses its old config/executable.
			still, err := m.Start(context.Background(), SingBox)
			if err == nil || still.PID != first.PID || still.State != Error || !still.NeedsRecovery {
				t.Fatalf("Start disguised retained old listener as new config: %+v %v", still, err)
			}
			reject.Store(false)
			stopped, err := m.Stop(context.Background(), SingBox)
			if err != nil || stopped.PID != 0 {
				t.Fatal(stopped, err)
			}
			assertGone(t, first.PID)
			started, err := m.Start(context.Background(), SingBox)
			if err != nil || started.State != Running || started.PID == 0 || started.PID == first.PID || started.Generation != wantGeneration || started.NeedsRecovery {
				t.Fatalf("explicit reconciliation failed: %+v %v", started, err)
			}
		})
	}
}

func TestWithdrawReadinessFailureRetainsSupervisedCandidate(t *testing.T) {
	var m *Manager
	var reject atomic.Bool
	var candidate atomic.Int32
	m, opts := testManager(t, func(o *Options) {
		o.ReadyHook = func(_ context.Context, id string) error {
			if !reject.Load() {
				return nil
			}
			status, _ := m.Status(id)
			candidate.Store(int32(status.PID))
			return ErrReadiness
		}
		o.CleanupHook = func(context.Context, string) error {
			if reject.Load() {
				return errors.New("fixed refusal")
			}
			return nil
		}
	})
	t.Cleanup(func() { reject.Store(false) })
	acquireFixture(t, m, opts, SingBox, fixture)
	accepted(t, m, SingBox, "good", 0)
	if _, err := m.Start(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
	if _, err := m.Stop(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
	reject.Store(true)
	status, err := m.Start(context.Background(), SingBox)
	if !errors.Is(err, ErrReadiness) || status.PID <= 0 || status.PID != int(candidate.Load()) || status.State != Error || !status.Desired || !status.NeedsRecovery {
		t.Fatalf("unready retained PID: %+v %v", status, err)
	}
	m.mu.Lock()
	process := m.services[SingBox].proc
	watched := m.services[SingBox].cancelWatch != nil
	m.mu.Unlock()
	if !watched || process == nil {
		t.Fatal("retained candidate has no supervisor")
	}
	process.signal(syscall.SIGKILL)
	waitStatus(t, m, SingBox, func(s Status) bool { return s.PID == 0 && !s.Desired && s.ErrorCode == "cleanup_failed" })
	assertGone(t, int(candidate.Load()))
}

func TestWithdrawCloseFailureRetainsExecutableAndReportsBoundary(t *testing.T) {
	var m *Manager
	var reject atomic.Bool
	m, opts := testManager(t, func(o *Options) {
		o.ResourceTimeout = 30 * time.Millisecond
		o.CleanupHook = func(ctx context.Context, id string) error {
			if id != SingBox || !reject.Load() {
				return nil
			}
			status, _ := m.Status(id)
			if status.PID <= 0 {
				t.Error("Close stopped listener before withdrawal")
			}
			<-ctx.Done()
			return ctx.Err()
		}
	})
	first := restartRunning(t, m, opts, SingBox)
	m.mu.Lock()
	process, binary := m.services[SingBox].proc, m.services[SingBox].binary
	m.mu.Unlock()
	// Close is terminal, not a persistent guardian. Explicit test teardown
	// reaps the retained fixture after checking the bounded failure boundary.
	t.Cleanup(func() { reject.Store(false); process.terminate(opts.TermGrace) })
	reject.Store(true)
	before := time.Now()
	err := m.Close()
	if err == nil || time.Since(before) > time.Second {
		t.Fatal("Close did not report bounded withdrawal failure", err)
	}
	status, _ := m.Status(SingBox)
	if status.PID != first.PID || status.State != Error || status.ErrorCode != "cleanup_failed" || !status.NeedsRecovery {
		t.Fatalf("Close hid live listener: %+v", status)
	}
	if _, err := os.Stat(binary); err != nil {
		t.Fatal("Close unlinked retained executable", err)
	}
	otherOpts := opts
	otherOpts.RunDir = filepath.Join(t.TempDir(), "separate-run")
	if other, err := New(otherOpts); err == nil {
		_ = other.Close()
		t.Fatal("failed Close released live store ownership")
	}
	if _, err := m.Stop(context.Background(), SingBox); !errors.Is(err, ErrClosed) {
		t.Fatal("closed boundary was not explicit", err)
	}
}

func TestWithdrawUnreadyRetainedConfigRejectsFurtherAcceptedChanges(t *testing.T) {
	var m *Manager
	var reject atomic.Bool
	m, opts := testManager(t, func(o *Options) {
		o.RestoreHook = func(context.Context, string) error {
			t.Error("unready retained process restored resources")
			return nil
		}
		o.ReadyHook = func(context.Context, string) error {
			if reject.Load() {
				return ErrReadiness
			}
			return nil
		}
		o.CleanupHook = func(context.Context, string) error {
			if reject.Load() {
				return errors.New("fixed refusal")
			}
			return nil
		}
	})
	t.Cleanup(func() { reject.Store(false) })
	acquireFixture(t, m, opts, SingBox, fixture)
	accepted(t, m, SingBox, "never-ready", 0)
	// Clear boot leftovers without ever marking Current ready.
	if _, err := m.Stop(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
	reject.Store(true)
	first, err := m.Start(context.Background(), SingBox)
	if !errors.Is(err, ErrReadiness) || first.PID <= 0 {
		t.Fatal(first, err)
	}
	m.mu.Lock()
	s := m.services[SingBox]
	process, binary, record := s.proc, s.running.binary, s.running.disk.Current
	m.mu.Unlock()
	if record.Ready {
		t.Fatal("reproducer requires a never-ready retained config")
	}
	path := configPath(opts, SingBox, record)
	beforeEntries, err := os.ReadDir(opts.RunDir)
	if err != nil {
		t.Fatal(err)
	}
	for _, operation := range []string{"configure", "acquire", "restore"} {
		var status Status
		var err error
		switch operation {
		case "configure":
			status, err = m.Configure(context.Background(), SingBox, []byte("later"), 1)
		case "acquire":
			status, err = m.Acquire(context.Background(), SingBox, Artifact{URL: "file:///must-not-read", SHA256: strings.Repeat("0", 64), Compression: "none"})
		case "restore":
			status, err = m.Restore(context.Background(), SingBox, 1)
		}
		if err == nil || status.Generation != first.Generation || status.PID != first.PID || status.State != Error || status.ErrorCode != "cleanup_failed" {
			t.Fatalf("%s mutated retained runtime: %+v %v", operation, status, err)
		}
		if raw, err := os.ReadFile(path); err != nil || string(raw) != "never-ready" {
			t.Fatalf("%s pruned retained config: %q %v", operation, raw, err)
		}
		if _, err := os.Stat(binary); err != nil {
			t.Fatalf("%s pruned retained executable: %v", operation, err)
		}
	}
	afterEntries, err := os.ReadDir(opts.RunDir)
	if err != nil || len(afterEntries) != len(beforeEntries) {
		t.Fatal("refused updates allocated additional runtime files", err)
	}
	m.mu.Lock()
	retained := s.proc == process && s.running.disk.Current == record && s.running.binary == binary
	m.mu.Unlock()
	if !retained {
		t.Fatal("refused updates aliased retained live identity")
	}
}

func TestWithdrawFailedClosePinsOwnershipUntilExactProcessReaps(t *testing.T) {
	base := t.TempDir()
	opts := Options{DataDir: filepath.Join(base, "data"), RunDir: filepath.Join(base, "run"), LocalSourceRoot: filepath.Join(base, "source"), TermGrace: 40 * time.Millisecond}
	if err := os.Mkdir(opts.LocalSourceRoot, 0700); err != nil {
		t.Fatal(err)
	}
	var reject atomic.Bool
	var cleanups atomic.Int32
	opts.CleanupHook = func(context.Context, string) error {
		cleanups.Add(1)
		if reject.Load() {
			return errors.New("fixed refusal")
		}
		return nil
	}
	m, err := New(opts)
	if err != nil {
		t.Fatal(err)
	}
	acquireFixture(t, m, opts, SingBox, fixture)
	accepted(t, m, SingBox, "good", 0)
	first, err := m.Start(context.Background(), SingBox)
	if err != nil {
		t.Fatal(err)
	}
	m.mu.Lock()
	process, binary := m.services[SingBox].proc, m.services[SingBox].binary
	m.mu.Unlock()
	t.Cleanup(func() { process.terminate(opts.TermGrace) })
	reject.Store(true)
	before := cleanups.Load()
	if err := m.Close(); err == nil {
		t.Fatal("failed withdrawal claimed successful Close")
	}
	if err := m.Close(); err == nil || cleanups.Load() != before+2 {
		t.Fatal("repeated Close retried withdrawal", err, cleanups.Load())
	}
	m = nil // No t.Cleanup closure or hook retains this Manager.
	goruntime.GC()
	otherOpts := opts
	otherOpts.RunDir = filepath.Join(base, "other-run")
	otherOpts.CleanupHook = nil // independent new owner cannot use the refused hook
	if other, err := New(otherOpts); err == nil {
		_ = other.Close()
		t.Fatal("GC finalized ownership lock while exact child remains")
	}
	if syscall.Kill(first.PID, 0) != nil {
		t.Fatal("lifetime pin signaled listener")
	}
	if cleanups.Load() != before+2 {
		t.Fatal("lifetime pin retried cleanup")
	}
	// Only this test-owned process object terminates the child. The pin just
	// waits for done (owned process-group cleanup plus reap), then releases fd.
	process.terminate(opts.TermGrace)
	assertGone(t, first.PID)
	timer := time.NewTimer(time.Second)
	defer timer.Stop()
	tick := time.NewTicker(5 * time.Millisecond)
	defer tick.Stop()
	for {
		other, err := New(otherOpts)
		if err == nil {
			if err := other.Close(); err != nil {
				t.Fatal(err)
			}
			break
		}
		select {
		case <-tick.C:
		case <-timer.C:
			t.Fatal("reaped child retained store lock", err)
		}
	}
	if _, err := os.Stat(binary); !os.IsNotExist(err) {
		t.Fatal("reaped failed-Close artifact retained", err)
	}
	if cleanups.Load() != before+2 {
		t.Fatal("lifetime pin emitted cleanup/retry", cleanups.Load(), before)
	}
}

func TestWithdrawCanceledReadinessRetainsSupervisedReplacement(t *testing.T) {
	var m *Manager
	var restarting, reject atomic.Bool
	entered := make(chan int, 1)
	var cleanups atomic.Int32
	m, opts := testManager(t, func(o *Options) {
		o.ReadyHook = func(ctx context.Context, id string) error {
			if !restarting.Load() {
				return nil
			}
			status, _ := m.Status(id)
			entered <- status.PID
			<-ctx.Done()
			reject.Store(true)
			return ctx.Err()
		}
		o.CleanupHook = func(context.Context, string) error {
			cleanups.Add(1)
			if reject.Load() {
				return errors.New("fixed refusal")
			}
			return nil
		}
	})
	t.Cleanup(func() { restarting.Store(false); reject.Store(false) })
	first := restartRunning(t, m, opts, SingBox)
	restarting.Store(true)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	type outcome struct {
		status Status
		err    error
	}
	finished := make(chan outcome, 1)
	go func() { status, err := m.Restart(ctx, SingBox); finished <- outcome{status, err} }()
	var replacement int
	select {
	case replacement = <-entered:
	case <-time.After(time.Second):
		t.Fatal("replacement readiness was not reached")
	}
	cancel()
	select {
	case result := <-finished:
		if !errors.Is(result.err, context.Canceled) || result.status.PID != replacement || result.status.PID == first.PID || result.status.State != Error || !result.status.Desired || result.status.ErrorCode != "cleanup_failed" || !result.status.NeedsRecovery {
			t.Fatalf("cancellation detached retained candidate: %+v %v", result.status, result.err)
		}
	case <-time.After(time.Second):
		t.Fatal("canceled replacement was not bounded")
	}
	assertGone(t, first.PID)
	m.mu.Lock()
	process := m.services[SingBox].proc
	watched := m.services[SingBox].cancelWatch != nil
	m.mu.Unlock()
	if !watched {
		t.Fatal("canceled retained process lost supervision")
	}
	before := cleanups.Load()
	process.signal(syscall.SIGKILL)
	waitStatus(t, m, SingBox, func(s Status) bool { return s.PID == 0 && !s.Desired && cleanups.Load() > before })
	assertGone(t, replacement)
}

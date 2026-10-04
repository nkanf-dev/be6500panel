package runtime

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"log/slog"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"sync/atomic"
	"syscall"
	"testing"
	"time"
)

// The only executable is the existing local shell fixture. The extra marker
// records run invocations, not verification. No test touches network resources.
var preStartFixture = strings.Replace(fixture, "case \"$(cat \"$config\")\" in", "echo launch >> \"$TMPDIR/prestart-events\"\ncase \"$(cat \"$config\")\" in", 1)

func preStartEvents(t *testing.T, runDir string) []string {
	t.Helper()
	raw, err := os.ReadFile(filepath.Join(runDir, "prestart-events"))
	if os.IsNotExist(err) {
		return nil
	}
	if err != nil {
		t.Fatal(err)
	}
	return strings.Fields(string(raw))
}

func preStartLaunches(t *testing.T, runDir string) int {
	t.Helper()
	n := 0
	for _, event := range preStartEvents(t, runDir) {
		if event == "launch" {
			n++
		}
	}
	return n
}

func preStartEvent(t *testing.T, runDir, event string) {
	t.Helper()
	f, err := os.OpenFile(filepath.Join(runDir, "prestart-events"), os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0600)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := f.WriteString(event + "\n"); err != nil {
		_ = f.Close()
		t.Fatal(err)
	}
	if err := f.Close(); err != nil {
		t.Fatal(err)
	}
}

func waitPreStartLaunches(t *testing.T, runDir string, expected int) {
	t.Helper()
	deadline := time.NewTimer(time.Second)
	defer deadline.Stop()
	tick := time.NewTicker(5 * time.Millisecond)
	defer tick.Stop()
	for {
		if actual := preStartLaunches(t, runDir); actual == expected {
			return
		}
		select {
		case <-tick.C:
		case <-deadline.C:
			t.Fatalf("fixture launch count: got %d want %d", preStartLaunches(t, runDir), expected)
		}
	}
}

func assertPreStartNoProcess(t *testing.T, m *Manager, id string) {
	t.Helper()
	m.mu.Lock()
	s := m.services[id]
	clear := s.proc == nil && s.running == nil && !s.cleanupPending && m.startingProc == nil
	m.mu.Unlock()
	if !clear {
		t.Fatal("pre-start ran before owned runtime withdrawal")
	}
	status, err := m.Status(id)
	if err != nil || status.State != Starting || status.PID != 0 {
		t.Fatalf("pre-start has a process or wrong state: %+v %v", status, err)
	}
}

func TestPreStartOwnedOrderingAndAcceptedPrivateCopy(t *testing.T) {
	for _, id := range []string{SingBox, FRPC} {
		t.Run(id, func(t *testing.T) {
			var m *Manager
			var runDir string
			var preStarts, readyCalls int
			var previousPID int
			const body = "good-private-accepted-config"
			m, opts := testManager(t, func(o *Options) {
				runDir = o.RunDir
				o.ReadyTimeout = time.Second
				o.CleanupHook = func(context.Context, string) error {
					preStartEvent(t, runDir, "cleanup")
					return nil
				}
				o.PreStartHook = func(ctx context.Context, service string, raw []byte) error {
					if service != id {
						t.Fatal("wrong pre-start service", service)
					}
					assertPreStartNoProcess(t, m, id)
					if previousPID != 0 {
						assertGone(t, previousPID)
					}
					deadline, ok := ctx.Deadline()
					if !ok || time.Until(deadline) <= 0 || time.Until(deadline) > time.Second {
						t.Fatal("pre-start context is not finitely bounded")
					}
					acceptedRaw, generation, err := m.Config(id)
					m.mu.Lock()
					record := *m.services[id].disk.Current
					m.mu.Unlock()
					sum := sha256.Sum256(raw)
					if err != nil || generation != 1 || record.Generation != generation || string(raw) != body || !bytes.Equal(raw, acceptedRaw) || hex.EncodeToString(sum[:]) != record.SHA256 {
						t.Fatal("pre-start did not receive exact accepted generation and checksum")
					}
					if _, err := m.Stop(ctx, id); !errors.Is(err, ErrBusy) {
						t.Fatal("pre-start did not own the mutation lane", err)
					}
					preStarts++
					preStartEvent(t, runDir, "prestart")
					raw[0] = 'X' // A private hook copy must not rewrite the accepted file.
					return nil
				}
				o.ReadyHook = func(context.Context, string) error {
					readyCalls++
					waitPreStartLaunches(t, runDir, readyCalls)
					status, _ := m.Status(id)
					if status.PID <= 0 || status.State != Starting {
						t.Fatal("readiness moved before launch", status)
					}
					preStartEvent(t, runDir, "ready")
					return nil
				}
				o.RestoreHook = func(context.Context, string) error {
					preStartEvent(t, runDir, "restore")
					return nil
				}
			})
			acquireFixture(t, m, opts, id, preStartFixture)
			accepted(t, m, id, body, 0)
			first, err := m.Start(context.Background(), id)
			if err != nil {
				t.Fatal(err)
			}
			previousPID = first.PID
			next, err := m.Restart(context.Background(), id)
			if err != nil || next.State != Running || next.PID == first.PID || preStarts != 2 {
				t.Fatalf("restart did not recheck pre-start: %+v %v", next, err)
			}
			want := []string{"cleanup", "prestart", "launch", "ready", "restore", "cleanup", "prestart", "launch", "ready", "restore"}
			if got := preStartEvents(t, runDir); !reflect.DeepEqual(got, want) {
				t.Fatalf("owned lifecycle ordering: %v", got)
			}
			raw, generation, err := m.Config(id)
			if err != nil || generation != 1 || string(raw) != body {
				t.Fatal("hook changed accepted private config")
			}
		})
	}
}

func TestPreStartFailureNeverLaunchesOrLeaksPrivateData(t *testing.T) {
	for _, kind := range []string{"refused", "timeout", "canceled_success"} {
		t.Run(kind, func(t *testing.T) {
			const private = "{malformed-private-credential"
			var logs bytes.Buffer
			var preStarts, ready, restored, cleanup atomic.Int32
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			var m *Manager
			m, opts := testManager(t, func(o *Options) {
				o.Logger = slog.New(slog.NewTextHandler(&logs, &slog.HandlerOptions{Level: slog.LevelDebug}))
				o.ReadyTimeout = 25 * time.Millisecond
				o.CleanupHook = func(context.Context, string) error { cleanup.Add(1); return nil }
				o.ReadyHook = func(context.Context, string) error { ready.Add(1); return nil }
				o.RestoreHook = func(context.Context, string) error { restored.Add(1); return nil }
				o.PreStartHook = func(hookCtx context.Context, id string, raw []byte) error {
					preStarts.Add(1)
					assertPreStartNoProcess(t, m, id)
					if string(raw) != private {
						t.Fatal("private accepted bytes were not passed directly")
					}
					switch kind {
					case "timeout":
						<-hookCtx.Done()
					case "canceled_success":
						cancel()
						return nil // Success must not bypass the post-hook cancellation check.
					}
					return errors.New(private + " raw hook diagnostic")
				}
			})
			acquireFixture(t, m, opts, SingBox, preStartFixture)
			accepted(t, m, SingBox, private, 0) // The fixture check accepts synthetic bytes.
			status, err := m.Start(ctx, SingBox)
			wantErr := ErrReadiness
			if kind == "canceled_success" {
				wantErr = context.Canceled
			}
			if !errors.Is(err, wantErr) || status.State != Error || status.PID != 0 || status.Desired || status.ErrorCode != "prestart_failed" || status.Generation != 1 || !status.RetryAt.IsZero() {
				t.Fatalf("pre-start refusal state: %+v %v", status, err)
			}
			if preStartLaunches(t, opts.RunDir) != 0 || preStarts.Load() != 1 || ready.Load() != 0 || restored.Load() != 0 || cleanup.Load() != 1 {
				t.Fatal("failed pre-start launched a child or touched post-launch resources")
			}
			m.mu.Lock()
			s := m.services[SingBox]
			clear := s.proc == nil && s.running == nil && s.cancelWatch == nil && !s.cleanupPending && !s.disk.Current.Ready
			m.mu.Unlock()
			if !clear {
				t.Fatal("failed pre-start left process/watch/readiness proof")
			}
			raw, generation, configErr := m.Config(SingBox)
			if configErr != nil || generation != 1 || string(raw) != private {
				t.Fatal("pre-start failure changed the accepted snapshot")
			}
			public, _ := json.Marshal(status)
			if strings.Contains(string(public)+errString(err)+logs.String(), private) || strings.Contains(errString(err)+logs.String(), "raw hook diagnostic") {
				t.Fatal("private pre-start bytes or hook error leaked")
			}
		})
	}
}

func TestPreStartCancellationAndCloseHaveNoChild(t *testing.T) {
	for _, closeManager := range []bool{false, true} {
		name := "request"
		if closeManager {
			name = "manager_close"
		}
		t.Run(name, func(t *testing.T) {
			entered := make(chan struct{})
			var ready, restored atomic.Int32
			m, opts := testManager(t, func(o *Options) {
				o.PreStartHook = func(ctx context.Context, _ string, _ []byte) error {
					close(entered)
					<-ctx.Done()
					return nil
				}
				o.ReadyHook = func(context.Context, string) error { ready.Add(1); return nil }
				o.RestoreHook = func(context.Context, string) error { restored.Add(1); return nil }
			})
			acquireFixture(t, m, opts, FRPC, preStartFixture)
			accepted(t, m, FRPC, "good-private-config", 0)
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			finished := make(chan error, 1)
			go func() { _, err := m.Start(ctx, FRPC); finished <- err }()
			select {
			case <-entered:
			case <-time.After(time.Second):
				t.Fatal("pre-start not reached")
			}
			assertPreStartNoProcess(t, m, FRPC)
			if closeManager {
				if err := m.Close(); err != nil {
					t.Fatal(err)
				}
			} else {
				cancel()
			}
			select {
			case err := <-finished:
				if !errors.Is(err, context.Canceled) && !(closeManager && errors.Is(err, ErrClosed)) {
					t.Fatal("canceled pre-start succeeded", err)
				}
			case <-time.After(time.Second):
				t.Fatal("pre-start cancellation did not finish")
			}
			status, _ := m.Status(FRPC)
			if preStartLaunches(t, opts.RunDir) != 0 || status.PID != 0 || status.Desired || ready.Load() != 0 || restored.Load() != 0 {
				t.Fatal("cancellation launched a child or restored resources", status)
			}
		})
	}
}

func TestPreStartRefusedWithdrawalKeepsOriginalListener(t *testing.T) {
	var preStarts, ready, restored atomic.Int32
	var refuse atomic.Bool
	m, opts := testManager(t, func(o *Options) {
		o.PreStartHook = func(context.Context, string, []byte) error { preStarts.Add(1); return nil }
		o.ReadyHook = func(context.Context, string) error { ready.Add(1); return nil }
		o.RestoreHook = func(context.Context, string) error { restored.Add(1); return nil }
		o.CleanupHook = func(context.Context, string) error {
			if refuse.Load() {
				return errors.New("private refused withdrawal")
			}
			return nil
		}
	})
	t.Cleanup(func() { refuse.Store(false) })
	acquireFixture(t, m, opts, SingBox, preStartFixture)
	accepted(t, m, SingBox, "good-first", 0)
	refuse.Store(true)
	if status, err := m.Start(context.Background(), SingBox); err == nil || status.PID != 0 || preStarts.Load() != 0 || preStartLaunches(t, opts.RunDir) != 0 {
		t.Fatal("boot cleanup refusal reached pre-start or launch", status, err)
	}
	refuse.Store(false)
	first, err := m.Start(context.Background(), SingBox)
	if err != nil {
		t.Fatal(err)
	}
	waitPreStartLaunches(t, opts.RunDir, 1)
	refuse.Store(true)
	status, err := m.Configure(context.Background(), SingBox, []byte("good-new-accepted"), 1)
	if err == nil || status.PID != first.PID || !status.Desired || status.ErrorCode != "cleanup_failed" || status.Generation != 2 || preStarts.Load() != 1 || ready.Load() != 1 || restored.Load() != 1 || preStartLaunches(t, opts.RunDir) != 1 {
		t.Fatalf("refused withdrawal stranded or replaced original listener: %+v %v", status, err)
	}
	if syscall.Kill(first.PID, 0) != nil {
		t.Fatal("refused withdrawal signaled the old listener")
	}
}

func TestPreStartRejectsChangedAcceptedFileBeforeHookOrExec(t *testing.T) {
	for _, kind := range []string{"checksum", "missing", "oversized"} {
		t.Run(kind, func(t *testing.T) {
			var mutate atomic.Bool
			var preStarts, ready, restored atomic.Int32
			var path string
			m, opts := testManager(t, func(o *Options) {
				o.PreStartHook = func(context.Context, string, []byte) error { preStarts.Add(1); return nil }
				o.ReadyHook = func(context.Context, string) error { ready.Add(1); return nil }
				o.RestoreHook = func(context.Context, string) error { restored.Add(1); return nil }
				o.CleanupHook = func(context.Context, string) error {
					if !mutate.Load() {
						return nil
					}
					switch kind {
					case "missing":
						return os.Remove(path)
					case "oversized":
						return os.WriteFile(path, bytes.Repeat([]byte{'x'}, (4<<20)+1), 0600)
					default:
						return os.WriteFile(path, []byte("different-private-accepted-config"), 0600)
					}
				}
			})
			acquireFixture(t, m, opts, FRPC, preStartFixture)
			accepted(t, m, FRPC, "good-private-config", 0)
			first, err := m.Start(context.Background(), FRPC)
			if err != nil {
				t.Fatal(err)
			}
			waitPreStartLaunches(t, opts.RunDir, 1)
			m.mu.Lock()
			path = configPath(m.opts, FRPC, m.services[FRPC].disk.Current)
			m.mu.Unlock()
			mutate.Store(true) // Change the file only after Restart's successful verifier.
			status, err := m.Restart(context.Background(), FRPC)
			mutate.Store(false)
			if !errors.Is(err, ErrReadiness) || status.State != Error || status.ErrorCode != "prestart_failed" || status.PID != 0 || status.Desired || status.Generation != 1 || preStarts.Load() != 1 || ready.Load() != 1 || restored.Load() != 1 || preStartLaunches(t, opts.RunDir) != 1 {
				t.Fatalf("changed accepted file reached hook or exec: %+v %v", status, err)
			}
			assertGone(t, first.PID)
		})
	}
}

func TestPreStartLiveFailureRestoresPreviousReadySnapshot(t *testing.T) {
	var m *Manager
	var runDir string
	var ready, restored int
	var generations []uint64
	var seen []string
	m, opts := testManager(t, func(o *Options) {
		runDir = o.RunDir
		o.PreStartHook = func(_ context.Context, id string, raw []byte) error {
			assertPreStartNoProcess(t, m, id)
			m.mu.Lock()
			record := *m.services[id].disk.Current
			generation := m.services[id].disk.Generation
			m.mu.Unlock()
			sum := sha256.Sum256(raw)
			if record.Generation != generation || hex.EncodeToString(sum[:]) != record.SHA256 {
				t.Fatal("rollback pre-start lost accepted generation or checksum")
			}
			generations = append(generations, generation)
			seen = append(seen, string(raw))
			if string(raw) == "good-but-prestart-refused" {
				return errors.New("private candidate diagnostic")
			}
			return nil
		}
		o.CleanupHook = func(context.Context, string) error { return nil }
		o.ReadyHook = func(context.Context, string) error {
			ready++
			waitPreStartLaunches(t, runDir, ready)
			return nil
		}
		o.RestoreHook = func(context.Context, string) error { restored++; return nil }
	})
	acquireFixture(t, m, opts, SingBox, preStartFixture)
	accepted(t, m, SingBox, "good-ready-private", 0)
	first, err := m.Start(context.Background(), SingBox)
	if err != nil {
		t.Fatal(err)
	}
	status, err := m.Configure(context.Background(), SingBox, []byte("good-but-prestart-refused"), 1)
	if !errors.Is(err, ErrReadiness) || status.State != Running || status.PID <= 0 || status.PID == first.PID || !status.Desired || !status.Restored || status.NeedsRecovery || status.Generation != 3 || status.ErrorCode != "prestart_failed" || ready != 2 || restored != 2 || preStartLaunches(t, opts.RunDir) != 2 {
		t.Fatalf("previous proven-ready snapshot not restored: %+v %v", status, err)
	}
	assertGone(t, first.PID)
	if !reflect.DeepEqual(generations, []uint64{1, 2, 3}) || !reflect.DeepEqual(seen, []string{"good-ready-private", "good-but-prestart-refused", "good-ready-private"}) {
		t.Fatal("pre-start did not check candidate and rollback generations", generations)
	}
	raw, generation, configErr := m.Config(SingBox)
	if configErr != nil || generation != 3 || string(raw) != "good-ready-private" || strings.Contains(errString(err), "private") {
		t.Fatal("rollback body/generation changed or private error leaked")
	}
}

func TestPreStartSupervisorRetryAlsoRequiresHook(t *testing.T) {
	var preStarts, ready, restored atomic.Int32
	m, opts := testManager(t, func(o *Options) {
		o.PreStartHook = func(context.Context, string, []byte) error {
			if preStarts.Add(1) > 1 {
				return errors.New("private retry refusal")
			}
			return nil
		}
		o.ReadyHook = func(context.Context, string) error { ready.Add(1); return nil }
		o.RestoreHook = func(context.Context, string) error { restored.Add(1); return nil }
	})
	acquireFixture(t, m, opts, SingBox, preStartFixture)
	accepted(t, m, SingBox, "good-private-config", 0)
	first, err := m.Start(context.Background(), SingBox)
	if err != nil {
		t.Fatal(err)
	}
	waitPreStartLaunches(t, opts.RunDir, 1)
	m.mu.Lock()
	p := m.services[SingBox].proc
	m.mu.Unlock()
	p.signal(syscall.SIGKILL)
	status := waitStatus(t, m, SingBox, func(s Status) bool { return s.State == Error && s.ErrorCode == "prestart_failed" })
	if status.PID != 0 || status.Desired || preStarts.Load() != 2 || ready.Load() != 1 || restored.Load() != 1 || preStartLaunches(t, opts.RunDir) != 1 {
		t.Fatal("supervisor launched without pre-start approval", status)
	}
	assertGone(t, first.PID)
}

func TestPreStartReadsPriorFourMiBAcceptedConfigWithLowerWriteLimit(t *testing.T) {
	const prefix, suffix = `{"private":"`, `"}`
	body := prefix + strings.Repeat("x", (4<<20)-len(prefix)-len(suffix)) + suffix
	m, opts := testManager(t, func(o *Options) { o.MaxConfigBytes = 4 << 20 })
	acquireFixture(t, m, opts, SingBox, preStartFixture)
	accepted(t, m, SingBox, body, 0)
	if _, err := m.Start(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
	if err := m.Close(); err != nil {
		t.Fatal(err)
	}
	opts.MaxConfigBytes = 32 << 10
	calls := 0
	opts.PreStartHook = func(_ context.Context, id string, raw []byte) error {
		calls++
		if id != SingBox || len(raw) != 4<<20 || !bytes.Equal(raw, []byte(body)) {
			t.Fatal("prior accepted-config ceiling not preserved")
		}
		return nil
	}
	reopened, err := New(opts)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		if err := reopened.Close(); err != nil {
			t.Error(err)
		}
	})
	acquireFixture(t, reopened, opts, SingBox, preStartFixture)
	status, err := reopened.Start(context.Background(), SingBox)
	if err != nil || status.State != Running || status.Generation != 1 || calls != 1 || reopened.opts.MaxConfigBytes != 32<<10 {
		t.Fatalf("old 4 MiB accepted config stranded by write limit: %+v %v", status, err)
	}
}

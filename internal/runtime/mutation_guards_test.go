package runtime

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"sync/atomic"
	"syscall"
	"testing"
	"time"
)

type mutationGuardCase struct {
	name string
	run  func(context.Context, *Manager, string, Artifact, uint64, func(context.Context) error) (Status, error)
}

func mutationGuardCases() []mutationGuardCase {
	return []mutationGuardCase{
		{"acquire", func(ctx context.Context, m *Manager, id string, artifact Artifact, _ uint64, guard func(context.Context) error) (Status, error) {
			return m.AcquireGuarded(ctx, id, artifact, guard)
		}},
		{"configure", func(ctx context.Context, m *Manager, id string, _ Artifact, generation uint64, guard func(context.Context) error) (Status, error) {
			return m.ConfigureGuarded(ctx, id, []byte("new-private-config"), generation, guard)
		}},
		{"restore", func(ctx context.Context, m *Manager, id string, _ Artifact, generation uint64, guard func(context.Context) error) (Status, error) {
			return m.RestoreGuarded(ctx, id, generation, guard)
		}},
		{"start", func(ctx context.Context, m *Manager, id string, _ Artifact, _ uint64, guard func(context.Context) error) (Status, error) {
			return m.StartGuarded(ctx, id, guard)
		}},
	}
}

type mutationGuardTransport struct {
	calls atomic.Int32
	body  string
}

func (transport *mutationGuardTransport) RoundTrip(*http.Request) (*http.Response, error) {
	transport.calls.Add(1)
	return &http.Response{StatusCode: http.StatusOK, Body: io.NopCloser(strings.NewReader(transport.body)), ContentLength: int64(len(transport.body)), Header: make(http.Header)}, nil
}

type mutationGuardHooks struct {
	cleanup, ready, restore, admission, writes, syncs atomic.Int32
	transport                                         mutationGuardTransport
}

func (hooks *mutationGuardHooks) counts() [7]int32 {
	return [7]int32{hooks.cleanup.Load(), hooks.ready.Load(), hooks.restore.Load(), hooks.admission.Load(), hooks.writes.Load(), hooks.syncs.Load(), hooks.transport.calls.Load()}
}

func mutationGuardManager(t *testing.T) (*Manager, Options, *mutationGuardHooks, Artifact) {
	t.Helper()
	// Only local shell fixtures run. The HTTPS source is an in-memory transport,
	// not a network server. A check marker also exposes unwanted verification.
	body := strings.Replace(fixture, "  exit 0 ;;", "  echo check >> \"$TMPDIR/guard-checks\"\n  exit 0 ;;", 1)
	hooks := &mutationGuardHooks{transport: mutationGuardTransport{body: body}}
	m, opts := testManager(t, func(o *Options) {
		o.CleanupHook = func(context.Context, string) error { hooks.cleanup.Add(1); return nil }
		o.ReadyHook = func(context.Context, string) error { hooks.ready.Add(1); return nil }
		o.RestoreHook = func(context.Context, string) error { hooks.restore.Add(1); return nil }
		o.StorageAdmission = func(context.Context, string, int64, bool) (func(), error) {
			hooks.admission.Add(1)
			return func() {}, nil
		}
		o.storageWrite = func(f *os.File, data []byte) (int, error) { hooks.writes.Add(1); return f.Write(data) }
		o.syncDirectory = func(path string) error { hooks.syncs.Add(1); return syncDir(path) }
		o.HTTPClient = &http.Client{Transport: &hooks.transport}
	})
	digest := sha256.Sum256([]byte(body))
	artifact := Artifact{URL: "https://fixture.invalid/core", SHA256: hex.EncodeToString(digest[:]), Compression: "none", Version: "guard-fixture"}
	return m, opts, hooks, artifact
}

func mutationGuardRunning(t *testing.T, m *Manager, opts Options, id string, artifact Artifact) Status {
	t.Helper()
	if _, err := m.Acquire(context.Background(), id, artifact); err != nil {
		t.Fatal(err)
	}
	accepted(t, m, id, "last-good-private-config", 0)
	accepted(t, m, id, "current-private-config", 1)
	status, err := m.Start(context.Background(), id)
	if err != nil || status.State != Running || status.PID <= 0 {
		t.Fatalf("fixture not running: %+v %v", status, err)
	}
	waitFile(t, filepath.Join(opts.RunDir, "run.pid"))
	return status
}

type mutationGuardFile struct {
	mode    os.FileMode
	modTime time.Time
	size    int64
	body    string
}

type mutationGuardSnapshot struct {
	status                         Status
	epoch                          uint64
	binary                         string
	process                        *managedProcess
	current, lastGood              *configRecord
	disk                           string
	cleanupPending, restorePending bool
	watching                       bool
	files                          map[string]mutationGuardFile
	hooks                          [7]int32
}

func mutationGuardStatus(status Status) Status {
	// RSS is a live observation, not stored service state.
	status.RSSBytes, status.RSSAvailable = 0, false
	return status
}

func snapshotMutationGuard(t *testing.T, m *Manager, opts Options, id string, hooks *mutationGuardHooks) mutationGuardSnapshot {
	t.Helper()
	m.mu.Lock()
	s := m.services[id]
	disk, err := json.Marshal(s.disk)
	out := mutationGuardSnapshot{
		status: mutationGuardStatus(m.statusLocked(id)), epoch: s.epoch,
		binary: s.binary, process: s.proc, current: s.disk.Current, lastGood: s.disk.LastGood,
		disk: string(disk), cleanupPending: s.cleanupPending, restorePending: s.restorePending,
		watching: s.cancelWatch != nil, files: make(map[string]mutationGuardFile), hooks: hooks.counts(),
	}
	m.mu.Unlock()
	if err != nil {
		t.Fatal(err)
	}
	for _, root := range []string{opts.DataDir, opts.RunDir} {
		err := filepath.WalkDir(root, func(path string, entry os.DirEntry, walkErr error) error {
			if walkErr != nil {
				return walkErr
			}
			info, err := entry.Info()
			if err != nil {
				return err
			}
			file := mutationGuardFile{mode: info.Mode(), modTime: info.ModTime(), size: info.Size()}
			if info.Mode().IsRegular() {
				body, err := os.ReadFile(path)
				if err != nil {
					return err
				}
				file.body = string(body)
			}
			out.files[path] = file
			return nil
		})
		if err != nil {
			t.Fatal(err)
		}
	}
	return out
}

func assertMutationGuardPreserved(t *testing.T, m *Manager, opts Options, id string, hooks *mutationGuardHooks, before mutationGuardSnapshot, returned Status) {
	t.Helper()
	after := snapshotMutationGuard(t, m, opts, id, hooks)
	if !reflect.DeepEqual(before, after) {
		t.Fatalf("guard changed runtime, private files, verifier or hooks: before %+v after %+v", before, after)
	}
	if !reflect.DeepEqual(mutationGuardStatus(returned), before.status) {
		t.Fatalf("guard returned synthetic status: got %+v want %+v", returned, before.status)
	}
	if before.status.PID > 0 {
		if err := syscall.Kill(before.status.PID, 0); err != nil {
			t.Fatalf("guard stopped the original process: %v", err)
		}
	}
	if err := m.ResourceOperation(context.Background(), id, func(context.Context) error { return nil }); err != nil {
		t.Fatalf("guard leaked the mutation lane: %v", err)
	}
}

func assertMutationGuardLane(t *testing.T, ctx context.Context, m *Manager) {
	t.Helper()
	for _, id := range []string{SingBox, FRPC} {
		if err := m.ResourceOperation(ctx, id, func(context.Context) error { return nil }); !errors.Is(err, ErrBusy) {
			t.Fatalf("guard ran outside the shared mutation lane: %s %v", id, err)
		}
		if _, err := m.Status(id); err != nil {
			t.Fatalf("read-only status failed in guard: %v", err)
		}
	}
}

func TestMutationGuardsRefusalPreservesRunningRuntimeAndPrivateFiles(t *testing.T) {
	for _, id := range []string{SingBox, FRPC} {
		for _, operation := range mutationGuardCases() {
			t.Run(id+"/"+operation.name, func(t *testing.T) {
				m, opts, hooks, artifact := mutationGuardManager(t)
				original := mutationGuardRunning(t, m, opts, id, artifact)
				before := snapshotMutationGuard(t, m, opts, id, hooks)
				refusal := errors.New("private caller-owned pending configuration")
				calls := 0
				status, err := operation.run(context.Background(), m, id, artifact, original.Generation, func(ctx context.Context) error {
					calls++
					assertMutationGuardLane(t, ctx, m)
					return refusal
				})
				if !errors.Is(err, refusal) || calls != 1 {
					t.Fatalf("guard refusal not returned once: %+v %v calls=%d", status, err, calls)
				}
				assertMutationGuardPreserved(t, m, opts, id, hooks, before, status)
			})
		}
	}
}

func TestMutationGuardsCancellationAfterGuardPreservesRunningRuntime(t *testing.T) {
	for _, id := range []string{SingBox, FRPC} {
		for _, operation := range mutationGuardCases() {
			t.Run(id+"/"+operation.name, func(t *testing.T) {
				m, opts, hooks, artifact := mutationGuardManager(t)
				original := mutationGuardRunning(t, m, opts, id, artifact)
				before := snapshotMutationGuard(t, m, opts, id, hooks)
				ctx, cancel := context.WithCancel(context.Background())
				defer cancel()
				calls := 0
				status, err := operation.run(ctx, m, id, artifact, original.Generation, func(ctx context.Context) error {
					calls++
					assertMutationGuardLane(t, ctx, m)
					cancel()
					return nil
				})
				if !errors.Is(err, context.Canceled) || calls != 1 {
					t.Fatalf("postguard cancellation ignored: %+v %v calls=%d", status, err, calls)
				}
				assertMutationGuardPreserved(t, m, opts, id, hooks, before, status)
			})
		}
	}
}

func TestMutationGuardsRunBeforeUnconfiguredPrerequisites(t *testing.T) {
	for _, id := range []string{SingBox, FRPC} {
		for _, operation := range mutationGuardCases() {
			t.Run(id+"/"+operation.name, func(t *testing.T) {
				m, opts, hooks, _ := mutationGuardManager(t)
				before := snapshotMutationGuard(t, m, opts, id, hooks)
				refusal := errors.New("caller configuration not applied")
				calls := 0
				// Invalid artifact and stale generation must not bypass the guard.
				status, err := operation.run(context.Background(), m, id, Artifact{}, 99, func(ctx context.Context) error {
					calls++
					assertMutationGuardLane(t, ctx, m)
					return refusal
				})
				if !errors.Is(err, refusal) || calls != 1 || status.State != NotConfigured {
					t.Fatalf("prerequisite ran before guard: %+v %v calls=%d", status, err, calls)
				}
				assertMutationGuardPreserved(t, m, opts, id, hooks, before, status)
			})
		}
	}
}

func TestMutationGuardsDoNotRunBeforeAdmission(t *testing.T) {
	m, _, _, artifact := mutationGuardManager(t)
	calls := 0
	guard := func(context.Context) error { calls++; return nil }
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	for _, operation := range mutationGuardCases() {
		for _, id := range []string{"", "../frpc", "rescue", "sing-box --help"} {
			if _, err := operation.run(context.Background(), m, id, artifact, 0, guard); !errors.Is(err, ErrService) {
				t.Fatalf("%s admitted unsupported service %q: %v", operation.name, id, err)
			}
		}
		for _, id := range []string{SingBox, FRPC} {
			if _, err := operation.run(nil, m, id, artifact, 0, guard); err == nil {
				t.Fatalf("%s admitted nil context", operation.name)
			}
			if _, err := operation.run(ctx, m, id, artifact, 0, guard); !errors.Is(err, context.Canceled) {
				t.Fatalf("%s admitted canceled context: %v", operation.name, err)
			}
		}
	}
	_, done, err := m.begin(context.Background(), SingBox)
	if err != nil {
		t.Fatal(err)
	}
	for _, operation := range mutationGuardCases() {
		for _, id := range []string{SingBox, FRPC} {
			if _, err := operation.run(context.Background(), m, id, artifact, 0, guard); !errors.Is(err, ErrBusy) {
				done()
				t.Fatalf("%s admitted an occupied lane: %v", operation.name, err)
			}
		}
	}
	done()
	if err := m.Close(); err != nil {
		t.Fatal(err)
	}
	for _, operation := range mutationGuardCases() {
		for _, id := range []string{SingBox, FRPC} {
			if _, err := operation.run(context.Background(), m, id, artifact, 0, guard); !errors.Is(err, ErrClosed) {
				t.Fatalf("%s admitted closed manager: %v", operation.name, err)
			}
		}
	}
	if calls != 0 {
		t.Fatalf("guard ran %d times without admission", calls)
	}
}

func TestMutationGuardsAllowWorkflowOnBothServices(t *testing.T) {
	for _, id := range []string{SingBox, FRPC} {
		t.Run(id, func(t *testing.T) {
			m, opts, hooks, artifact := mutationGuardManager(t)
			calls := 0
			guard := func(ctx context.Context) error {
				calls++
				assertMutationGuardLane(t, ctx, m)
				return nil
			}
			status, err := m.AcquireGuarded(context.Background(), id, artifact, guard)
			if err != nil || !status.ArtifactAvailable || status.State != NotConfigured || hooks.transport.calls.Load() != 1 {
				t.Fatalf("guarded acquire failed: %+v %v", status, err)
			}
			status, err = m.ConfigureGuarded(context.Background(), id, []byte("first-private-config"), 0, guard)
			if err != nil || status.Generation != 1 || status.State != Stopped {
				t.Fatalf("guarded configure failed: %+v %v", status, err)
			}
			status, err = m.StartGuarded(context.Background(), id, guard)
			if err != nil || status.State != Running || status.PID <= 0 {
				t.Fatalf("guarded start failed: %+v %v", status, err)
			}
			firstPID := status.PID
			waitFile(t, filepath.Join(opts.RunDir, "run.pid"))
			status, err = m.ConfigureGuarded(context.Background(), id, []byte("second-private-config"), 1, guard)
			if err != nil || status.Generation != 2 || status.State != Running || status.PID <= 0 || status.PID == firstPID {
				t.Fatalf("guarded live configure failed: %+v %v", status, err)
			}
			assertGone(t, firstPID)
			secondPID := status.PID
			status, err = m.RestoreGuarded(context.Background(), id, 2, guard)
			if err != nil || status.Generation != 3 || status.State != Running || status.PID <= 0 || status.PID == secondPID {
				t.Fatalf("guarded restore failed: %+v %v", status, err)
			}
			assertGone(t, secondPID)
			raw, generation, err := m.Config(id)
			if err != nil || generation != 3 || string(raw) != "first-private-config" {
				t.Fatalf("guarded restore wrong accepted config: %q %d %v", raw, generation, err)
			}
			restoredPID := status.PID
			status, err = m.StartGuarded(context.Background(), id, guard)
			if err != nil || status.PID != restoredPID || status.State != Running {
				t.Fatalf("guarded start lost idempotence: %+v %v", status, err)
			}
			artifact.Version = "guard-upgrade"
			status, err = m.AcquireGuarded(context.Background(), id, artifact, guard)
			if err != nil || status.Generation != 3 || status.State != Running || status.PID <= 0 || status.PID == restoredPID || status.Version != artifact.Version || hooks.transport.calls.Load() != 2 {
				t.Fatalf("guarded live acquire failed: %+v %v", status, err)
			}
			assertGone(t, restoredPID)
			upgradePID := status.PID
			status, err = m.Stop(context.Background(), id)
			if err != nil || status.State != Stopped || status.PID != 0 || status.Desired || calls != 7 {
				t.Fatalf("emergency stop changed guard semantics: %+v %v calls=%d", status, err, calls)
			}
			assertGone(t, upgradePID)
			status, err = m.StartGuarded(context.Background(), id, guard)
			if err != nil || status.State != Running || status.PID <= 0 || calls != 8 {
				t.Fatalf("guarded stopped start failed: %+v %v calls=%d", status, err, calls)
			}
		})
	}
}

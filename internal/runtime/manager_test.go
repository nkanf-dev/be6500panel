package runtime

import (
	"bytes"
	"compress/gzip"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"syscall"
	"testing"
	"time"
)

// Only these locally created shell fixtures are executed. No downloaded core
// or router command is executed by this test suite.
const fixture = `#!/bin/sh
case "$1" in
 check|verify)
  [ "$2" = "-c" ] || exit 44
  case "$(cat "$3")" in
   bad*) echo 'credential-from-config' >&2; exit 9 ;;
   slow*) echo $$ > "$TMPDIR/check.pid"; trap '' TERM; sleep 20 ;;
  esac
  exit 0 ;;
 run) [ "$2" = "-c" ] || exit 44; config="$3" ;;
 -c) config="$2" ;;
 *) exit 45 ;;
esac
case "$(cat "$config")" in
 crash*) exit 8 ;;
 stubborn*) trap '' TERM ;;
 *) trap 'exit 0' TERM ;;
esac
echo 'credential-from-config' >&2
echo $$ > "$TMPDIR/run.pid"
while :; do sleep 1; done
`

func testManager(t *testing.T, change func(*Options)) (*Manager, Options) {
	t.Helper()
	base := t.TempDir()
	source := filepath.Join(base, "source")
	if err := os.Mkdir(source, 0700); err != nil {
		t.Fatal(err)
	}
	opts := Options{DataDir: filepath.Join(base, "data"), RunDir: filepath.Join(base, "run"), LocalSourceRoot: source, TermGrace: 40 * time.Millisecond, CheckTimeout: 5 * time.Second, BackoffInitial: 80 * time.Millisecond, BackoffMax: 150 * time.Millisecond, StableAfter: time.Second, MaxRestarts: 2}
	if change != nil {
		change(&opts)
	}
	m, err := New(opts)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		if err := m.Close(); err != nil && opts.CleanupHook == nil {
			t.Error(err)
		}
	})
	return m, opts
}
func acquireFixture(t *testing.T, m *Manager, opts Options, id, body string) Artifact {
	t.Helper()
	path := filepath.Join(opts.LocalSourceRoot, "fixture-"+id+"-"+strconv.FormatInt(time.Now().UnixNano(), 10))
	if err := os.WriteFile(path, []byte(body), 0700); err != nil {
		t.Fatal(err)
	}
	hash := sha256.Sum256([]byte(body))
	artifact := Artifact{URL: "file://" + path, SHA256: hex.EncodeToString(hash[:]), Compression: "none", Version: "local-test"}
	if _, err := m.Acquire(context.Background(), id, artifact); err != nil {
		t.Fatal(err)
	}
	return artifact
}
func accepted(t *testing.T, m *Manager, id, body string, generation uint64) Status {
	t.Helper()
	status, err := m.Configure(context.Background(), id, []byte(body), generation)
	if err != nil {
		t.Fatal(err)
	}
	return status
}
func waitStatus(t *testing.T, m *Manager, id string, predicate func(Status) bool) Status {
	t.Helper()
	deadline := time.NewTimer(3 * time.Second)
	defer deadline.Stop()
	tick := time.NewTicker(5 * time.Millisecond)
	defer tick.Stop()
	for {
		status, err := m.Status(id)
		if err != nil {
			t.Fatal(err)
		}
		if predicate(status) {
			return status
		}
		select {
		case <-tick.C:
		case <-deadline.C:
			t.Fatalf("status condition not reached: %+v", status)
		}
	}
}
func assertGone(t *testing.T, pid int) {
	t.Helper()
	if pid <= 0 {
		t.Fatal("missing fixture PID")
	}
	if err := syscall.Kill(pid, 0); err != syscall.ESRCH {
		t.Fatalf("process %d remains: %v", pid, err)
	}
}

func TestConfigureFailedCheckKeepsAcceptedAndRunning(t *testing.T) {
	m, opts := testManager(t, nil)
	acquireFixture(t, m, opts, SingBox, fixture)
	status := accepted(t, m, SingBox, "good-private-password", 0)
	if status.Generation != 1 {
		t.Fatal(status)
	}
	status, err := m.Start(context.Background(), SingBox)
	if err != nil {
		t.Fatal(err)
	}
	pid := status.PID
	status, err = m.Configure(context.Background(), SingBox, []byte("bad-secret"), 1)
	if !errors.Is(err, ErrCheck) || status.Generation != 1 || status.PID != pid || status.State != Running {
		t.Fatalf("old process/config not preserved: %+v %v", status, err)
	}
	m.mu.Lock()
	record := m.services[SingBox].disk.Current
	m.mu.Unlock()
	body, err := os.ReadFile(configPath(opts, SingBox, record))
	if err != nil || string(body) != "good-private-password" {
		t.Fatalf("accepted config changed: %q %v", body, err)
	}
	public, _ := json.Marshal(status)
	if strings.Contains(string(public), "secret") || strings.Contains(string(public), "credential") {
		t.Fatalf("private body leaked: %s", public)
	}
	if strings.Contains(errString(err), "credential") {
		t.Fatal("core stderr leaked")
	}
	if _, err = m.Configure(context.Background(), SingBox, []byte("good"), 0); !errors.Is(err, ErrGeneration) {
		t.Fatal(err)
	}
	if _, err = m.Stop(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
	assertGone(t, pid)
}
func errString(err error) string {
	if err == nil {
		return ""
	}
	return err.Error()
}

func TestStoreAtomicGenerationLastGoodAndRestart(t *testing.T) {
	m, opts := testManager(t, nil)
	acquireFixture(t, m, opts, FRPC, fixture)
	accepted(t, m, FRPC, "first", 0)
	if _, err := m.Start(context.Background(), FRPC); err != nil {
		t.Fatal(err)
	}
	if _, err := m.Stop(context.Background(), FRPC); err != nil {
		t.Fatal(err)
	}
	accepted(t, m, FRPC, "second", 1)
	if _, err := m.Restore(context.Background(), FRPC, 2); err != nil {
		t.Fatal(err)
	}
	m.mu.Lock()
	state := m.services[FRPC].disk
	m.mu.Unlock()
	if state.Generation != 3 {
		t.Fatal(state)
	}
	body, err := os.ReadFile(configPath(opts, FRPC, state.Current))
	if err != nil || string(body) != "first" {
		t.Fatalf("restore %q %v", body, err)
	}
	entries, err := os.ReadDir(filepath.Join(opts.DataDir, FRPC))
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 3 {
		t.Fatalf("unbounded private snapshots: %+v", entries)
	}
	for _, e := range entries {
		info, _ := e.Info()
		if info.Mode().Perm() != 0600 {
			t.Fatalf("unsafe private permissions: %s %o", e.Name(), info.Mode().Perm())
		}
	}
	if err = m.Close(); err != nil {
		t.Fatal(err)
	}
	reopened, err := New(opts)
	if err != nil {
		t.Fatal(err)
	}
	defer reopened.Close()
	status, _ := reopened.Status(FRPC)
	if status.Generation != 3 || !status.Configured || status.ArtifactAvailable || status.State != Rebuilding {
		t.Fatalf("restart restoration: %+v", status)
	}
	acquireFixture(t, reopened, opts, FRPC, fixture)
	if _, err = reopened.Start(context.Background(), FRPC); err != nil {
		t.Fatal(err)
	}
}

func TestStopCancelsBackoffAndBoundedCrashRetries(t *testing.T) {
	m, opts := testManager(t, func(o *Options) { o.BackoffInitial = 100 * time.Millisecond; o.BackoffMax = 100 * time.Millisecond })
	acquireFixture(t, m, opts, SingBox, fixture)
	accepted(t, m, SingBox, "crash", 0)
	if _, err := m.Start(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
	waitStatus(t, m, SingBox, func(s Status) bool { return s.State == Backoff })
	if _, err := m.Stop(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
	// A bounded test timer checks the canceled retry; not agent-side polling.
	<-time.After(180 * time.Millisecond)
	status, _ := m.Status(SingBox)
	if status.State != Stopped || status.PID != 0 || status.Desired {
		t.Fatal(status)
	}
	if _, err := m.Start(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
	status = waitStatus(t, m, SingBox, func(s Status) bool { return s.State == Error })
	if status.ErrorCode != "restart_limit" || status.Desired || status.Restarts != 3 {
		t.Fatal(status)
	}
}

func TestCheckTimeoutCancellationBusyAndClose(t *testing.T) {
	m, opts := testManager(t, func(o *Options) { o.CheckTimeout = time.Second })
	acquireFixture(t, m, opts, SingBox, fixture)
	accepted(t, m, SingBox, "good", 0)
	result := make(chan error, 1)
	go func() { _, err := m.Configure(context.Background(), SingBox, []byte("slow"), 1); result <- err }()
	waitStatus(t, m, SingBox, func(s Status) bool { return s.State == Checking })
	if _, err := m.Stop(context.Background(), FRPC); !errors.Is(err, ErrBusy) {
		t.Fatal(err)
	}
	if err := <-result; err == nil {
		t.Fatal("slow verifier accepted")
	}
	checkPIDBytes, err := os.ReadFile(filepath.Join(opts.RunDir, "check.pid"))
	if err != nil {
		t.Fatal(err)
	}
	pid, _ := strconv.Atoi(strings.TrimSpace(string(checkPIDBytes)))
	assertGone(t, pid)
	status, _ := m.Status(SingBox)
	if status.Generation != 1 {
		t.Fatal(status)
	}
	// Close must cancel an in-flight verifier rather than wait CheckTimeout.
	go func() { _, err := m.Configure(context.Background(), SingBox, []byte("slow"), 1); result <- err }()
	waitStatus(t, m, SingBox, func(s Status) bool { return s.State == Checking })
	before := time.Now()
	if err = m.Close(); err != nil {
		t.Fatal(err)
	}
	if time.Since(before) > 500*time.Millisecond {
		t.Fatal("Close did not cancel verifier")
	}
	if err = <-result; !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
	if _, err = m.Start(context.Background(), SingBox); !errors.Is(err, ErrClosed) {
		t.Fatal(err)
	}
	if err = m.Close(); err != nil {
		t.Fatal(err)
	}
}

func TestProcessGroupCleanupAndUnrelatedPID(t *testing.T) {
	m, opts := testManager(t, nil)
	marker := filepath.Join(opts.RunDir, "child.pid")
	childFixture := strings.Replace(fixture, "echo 'credential-from-config' >&2\necho $$", fmt.Sprintf("sleep 20 &\necho $! > '%s'\necho 'credential-from-config' >&2\necho $$", marker), 1)
	acquireFixture(t, m, opts, SingBox, childFixture)
	accepted(t, m, SingBox, "stubborn", 0)
	unrelated := exec.Command("/bin/sleep", "20")
	if err := unrelated.Start(); err != nil {
		t.Fatal(err)
	}
	defer func() { unrelated.Process.Kill(); unrelated.Wait() }()
	status, err := m.Start(context.Background(), SingBox)
	if err != nil {
		t.Fatal(err)
	}
	waitFile(t, marker)
	raw, _ := os.ReadFile(marker)
	child, _ := strconv.Atoi(strings.TrimSpace(string(raw)))
	before := time.Now()
	if _, err = m.Stop(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
	if time.Since(before) > time.Second {
		t.Fatal("TERM/KILL deadline exceeded")
	}
	assertGone(t, status.PID)
	// init may reap adopted descendants asynchronously, so test disappearance.
	waitGone(t, child)
	if err = syscall.Kill(unrelated.Process.Pid, 0); err != nil {
		t.Fatalf("unrelated PID killed: %v", err)
	}
	if _, err = m.Stop(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
	if err = syscall.Kill(unrelated.Process.Pid, 0); err != nil {
		t.Fatal("idempotent stop killed unrelated process")
	}
}
func waitFile(t *testing.T, path string) {
	t.Helper()
	timer := time.NewTimer(time.Second)
	defer timer.Stop()
	ticker := time.NewTicker(5 * time.Millisecond)
	defer ticker.Stop()
	for {
		if _, err := os.Stat(path); err == nil {
			return
		}
		select {
		case <-ticker.C:
		case <-timer.C:
			t.Fatal("fixture did not create marker")
		}
	}
}
func waitGone(t *testing.T, pid int) {
	t.Helper()
	timer := time.NewTimer(time.Second)
	defer timer.Stop()
	ticker := time.NewTicker(5 * time.Millisecond)
	defer ticker.Stop()
	for {
		if err := syscall.Kill(pid, 0); err == syscall.ESRCH {
			return
		}
		select {
		case <-ticker.C:
		case <-timer.C:
			t.Fatalf("fixture child remains %d", pid)
		}
	}
}

func TestRejectUnownedIDsLockAndLimits(t *testing.T) {
	m, opts := testManager(t, nil)
	for _, id := range []string{"", "../evil", "sh", "sing-box --help"} {
		if _, err := m.Status(id); !errors.Is(err, ErrService) {
			t.Fatal(id, err)
		}
	}
	if _, err := New(opts); err == nil {
		t.Fatal("duplicate store owner allowed")
	}
	if _, err := m.Configure(context.Background(), FRPC, []byte("x"), 0); !errors.Is(err, ErrNoArtifact) {
		t.Fatal(err)
	}
	if _, err := m.Start(context.Background(), FRPC); !errors.Is(err, ErrNotConfigured) {
		t.Fatal(err)
	}
	if _, err := New(Options{DataDir: opts.DataDir, RunDir: opts.DataDir}); err == nil {
		t.Fatal("persistent artifacts allowed")
	}
}
func TestCleanupHookAndTailBound(t *testing.T) {
	var calls atomic.Int32
	m, opts := testManager(t, func(o *Options) {
		o.CleanupHook = func(ctx context.Context, id string) error { calls.Add(1); return nil }
	})
	acquireFixture(t, m, opts, FRPC, fixture)
	accepted(t, m, FRPC, "good", 0)
	if _, err := m.Start(context.Background(), FRPC); err != nil {
		t.Fatal(err)
	}
	if _, err := m.Stop(context.Background(), FRPC); err != nil {
		t.Fatal(err)
	}
	if calls.Load() != 2 {
		t.Fatal(calls.Load())
	}
	tail := &tailWriter{limit: 8}
	var wg sync.WaitGroup
	for n := 0; n < 20; n++ {
		wg.Add(1)
		go func() { defer wg.Done(); tail.Write([]byte("secret-long-line")) }()
	}
	wg.Wait()
	if len(tail.data) != 8 || string(tail.data) != "ong-line" {
		t.Fatalf("tail not bounded: %q", tail.data)
	}
}

func TestArtifactFailedUpgradePreservesCore(t *testing.T) {
	m, opts := testManager(t, nil)
	artifact := acquireFixture(t, m, opts, SingBox, fixture)
	accepted(t, m, SingBox, "good", 0)
	status, err := m.Start(context.Background(), SingBox)
	if err != nil {
		t.Fatal(err)
	}
	artifact.SHA256 = strings.Repeat("0", 64)
	next, err := m.Acquire(context.Background(), SingBox, artifact)
	if err == nil || next.PID != status.PID || next.State != Running {
		t.Fatalf("failed upgrade disturbed core: %+v %v", next, err)
	}
}

func TestConfigReturnsPrivateCopyAndMetadataBound(t *testing.T) {
	m, opts := testManager(t, nil)
	acquireFixture(t, m, opts, FRPC, fixture)
	accepted(t, m, FRPC, "serverAddr = 'private.example'", 0)
	raw, generation, err := m.Config(FRPC)
	if err != nil || generation != 1 {
		t.Fatalf("config read: %d %v", generation, err)
	}
	raw[0] = 'X'
	again, _, err := m.Config(FRPC)
	if err != nil || string(again) != "serverAddr = 'private.example'" {
		t.Fatal("mutable config alias", err)
	}
	m.mu.Lock()
	record := m.services[FRPC].disk.Current
	m.mu.Unlock()
	if filepath.Ext(record.File) != ".toml" {
		t.Fatal("frpc TOML format lost", record.File)
	}
	if _, err := m.Acquire(context.Background(), FRPC, Artifact{URL: "https://example.invalid/" + strings.Repeat("x", 4096), SHA256: strings.Repeat("0", 64), Compression: "none"}); err == nil {
		t.Fatal("unbounded metadata accepted")
	}
}

func TestFRPCFormatAwareVerifier(t *testing.T) {
	m, opts := testManager(t, nil)
	formatFixture := strings.Replace(fixture, "[ \"$2\" = \"-c\" ] || exit 44\n  case", "[ \"$2\" = \"-c\" ] || exit 44\n  case \"$(cat \"$3\")\" in\n   {*) case \"$3\" in *.json) ;; *) exit 77 ;; esac ;;\n   *) case \"$3\" in *.toml) ;; *) exit 78 ;; esac ;;\n  esac\n  case", 1)
	acquireFixture(t, m, opts, FRPC, formatFixture)
	accepted(t, m, FRPC, "serverAddr = 'private.example'", 0)
	accepted(t, m, FRPC, `{"serverAddr":"private.example"}`, 1)
	if _, err := m.Start(context.Background(), FRPC); err != nil {
		t.Fatal(err)
	}
}

func TestCommitFailureKeepsAcceptedFile(t *testing.T) {
	m, opts := testManager(t, nil)
	acquireFixture(t, m, opts, SingBox, fixture)
	accepted(t, m, SingBox, `{"accepted":true}`, 0)
	m.mu.Lock()
	disk := m.services[SingBox].disk
	m.mu.Unlock()
	// A directory cannot be atomically replaced by the manifest temp file.
	stateFile := statePath(opts, SingBox)
	saved := stateFile + ".saved"
	if err := os.Rename(stateFile, saved); err != nil {
		t.Fatal(err)
	}
	if err := os.Mkdir(stateFile, 0700); err != nil {
		t.Fatal(err)
	}
	_, err := m.Configure(context.Background(), SingBox, []byte(`{"new":true}`), 1)
	if err == nil {
		t.Fatal("failed manifest write accepted")
	}
	raw, generation, err := m.Config(SingBox)
	if err != nil || generation != 1 || string(raw) != `{"accepted":true}` {
		t.Fatalf("failed atomic commit lost config: %s %d %v", raw, generation, err)
	}
	if _, err := os.Stat(filepath.Join(opts.DataDir, SingBox, "config-2.json")); !os.IsNotExist(err) {
		t.Fatal("orphan accepted file", err)
	}
	if err := os.Remove(stateFile); err != nil {
		t.Fatal(err)
	}
	if err := os.Rename(saved, stateFile); err != nil {
		t.Fatal(err)
	}
	loaded, err := loadState(m.opts, SingBox)
	if err != nil || loaded.Generation != disk.Generation {
		t.Fatal("old manifest lost", err)
	}
}

func TestNewRejectsPhysicallyAliasedDirectories(t *testing.T) {
	root := t.TempDir()
	data := filepath.Join(root, "data")
	if err := os.Mkdir(data, 0700); err != nil {
		t.Fatal(err)
	}
	alias := filepath.Join(root, "alias")
	if err := os.Symlink(data, alias); err != nil {
		t.Fatal(err)
	}
	if _, err := New(Options{DataDir: data, RunDir: filepath.Join(alias, "run")}); err == nil {
		t.Fatal("artifact alias into persistent data accepted")
	}
}

func TestExitedCoreCleanupBeforeExplicitStart(t *testing.T) {
	var cleaned atomic.Int32
	m, opts := testManager(t, func(o *Options) {
		o.CleanupHook = func(ctx context.Context, id string) error { cleaned.Add(1); return nil }
	})
	acquireFixture(t, m, opts, SingBox, fixture)
	accepted(t, m, SingBox, "good", 0)
	status, err := m.Start(context.Background(), SingBox)
	if err != nil {
		t.Fatal(err)
	}
	baselineCleanups := cleaned.Load()
	// Deterministically hold the mutation lane so the original exit watcher cannot
	// consume the exit; simulate a scheduling race before explicit Start.
	m.gate <- struct{}{}
	m.mu.Lock()
	p := m.services[SingBox].proc
	m.services[SingBox].cancelWatch()
	m.mu.Unlock()
	p.signal(syscall.SIGKILL)
	<-p.done
	<-m.gate
	if err = syscall.Kill(status.PID, 0); err != syscall.ESRCH {
		t.Fatal("old leader not reaped", err)
	}
	if _, err = m.Start(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
	if cleaned.Load() != baselineCleanups+1 {
		t.Fatalf("old exit cleanup skipped: %d", cleaned.Load())
	}
}

func TestCleanupFailureNeverStartsReplacement(t *testing.T) {
	var fail atomic.Bool
	m, opts := testManager(t, func(o *Options) {
		o.CleanupHook = func(ctx context.Context, id string) error {
			if fail.Load() {
				return errors.New("private failure detail")
			}
			return nil
		}
	})
	acquireFixture(t, m, opts, SingBox, fixture)
	accepted(t, m, SingBox, "good", 0)
	if _, err := m.Start(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
	fail.Store(true)
	status, err := m.Configure(context.Background(), SingBox, []byte("new-good"), 1)
	if err == nil || status.PID != 0 || status.State != Error || status.ErrorCode != "cleanup_failed" || status.Desired {
		t.Fatalf("cleanup failure ignored: %+v %v", status, err)
	}
	fail.Store(false)
}

func TestDirectorySyncWarningKeepsCommittedAndPreviousSnapshots(t *testing.T) {
	m, opts := testManager(t, nil)
	acquireFixture(t, m, opts, SingBox, fixture)
	accepted(t, m, SingBox, "first", 0)
	if _, err := m.Start(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
	if _, err := m.Stop(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
	accepted(t, m, SingBox, "second", 1)
	// Fail only manifest's post-rename fsync, not candidate's pre-commit fsync.
	calls := 0
	m.opts.syncDirectory = func(path string) error {
		calls++
		if calls == 2 {
			return errors.New("disk failure")
		}
		return syncDir(path)
	}
	status, err := m.Configure(context.Background(), SingBox, []byte("third"), 2)
	if !errors.Is(err, ErrDurability) || status.Generation != 3 || status.ErrorCode != "state_not_durable" {
		t.Fatalf("committed warning not represented: %+v %v", status, err)
	}
	raw, generation, err := m.Config(SingBox)
	if err != nil || generation != 3 || string(raw) != "third" {
		t.Fatalf("warning generation/body lost: %d %s %v", generation, raw, err)
	}
	for _, name := range []string{"config-1.json", "config-2.json", "config-3.json"} {
		if _, err := os.Stat(filepath.Join(opts.DataDir, SingBox, name)); err != nil {
			t.Fatal("snapshot pruned before confirmed durability", name, err)
		}
	}
	m.opts.syncDirectory = nil
	if err := m.Close(); err != nil {
		t.Fatal(err)
	}
	reopened, err := New(opts)
	if err != nil {
		t.Fatal(err)
	}
	defer reopened.Close()
	raw, generation, err = reopened.Config(SingBox)
	if err != nil || generation != 3 || string(raw) != "third" {
		t.Fatal("committed warning lost across reopen", err)
	}
}

func TestCandidateSyncFailureDoesNotCommit(t *testing.T) {
	m, opts := testManager(t, nil)
	acquireFixture(t, m, opts, SingBox, fixture)
	accepted(t, m, SingBox, "first", 0)
	m.opts.syncDirectory = func(path string) error { return errors.New("directory unavailable") }
	status, err := m.Configure(context.Background(), SingBox, []byte("second"), 1)
	if err == nil || status.Generation != 1 {
		t.Fatalf("pre-commit failure accepted: %+v %v", status, err)
	}
	raw, _, err := m.Config(SingBox)
	if err != nil || string(raw) != "first" {
		t.Fatal("prior body lost", err)
	}
	m.opts.syncDirectory = nil
}

func TestRetainLeaderFailureIsBoundedAndReaped(t *testing.T) {
	m, opts := testManager(t, nil)
	acquireFixture(t, m, opts, SingBox, fixture)
	m.opts.waitExitedLeader = func(pid int) bool { return false }
	before := time.Now()
	if _, err := m.Configure(context.Background(), SingBox, []byte("slow"), 0); err == nil {
		t.Fatal("waitid failure verified")
	}
	if time.Since(before) > time.Second {
		t.Fatal("waitid failure made termination hang")
	}
	if err := m.Close(); err != nil {
		t.Fatal(err)
	}
}

func TestStartRetriesRequiredCleanup(t *testing.T) {
	var fail atomic.Bool
	m, opts := testManager(t, func(o *Options) {
		o.CleanupHook = func(ctx context.Context, id string) error {
			if fail.Load() {
				return errors.New("cleanup refused")
			}
			return nil
		}
	})
	acquireFixture(t, m, opts, SingBox, fixture)
	accepted(t, m, SingBox, "good", 0)
	if _, err := m.Start(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
	fail.Store(true)
	if _, err := m.Stop(context.Background(), SingBox); err == nil {
		t.Fatal("cleanup failure missing")
	}
	status, err := m.Start(context.Background(), SingBox)
	if err == nil || status.PID != 0 || status.ErrorCode != "cleanup_failed" {
		t.Fatalf("started on stale owned resources: %+v %v", status, err)
	}
	fail.Store(false)
	if _, err := m.Start(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
}

func TestDelayedExitDispatchDoesNotResetCrashBudget(t *testing.T) {
	m, opts := testManager(t, func(o *Options) {
		o.StableAfter = 20 * time.Millisecond
		o.BackoffInitial = 100 * time.Millisecond
		o.BackoffMax = 100 * time.Millisecond
	})
	acquireFixture(t, m, opts, SingBox, fixture)
	accepted(t, m, SingBox, "good", 0)
	if _, err := m.Start(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
	m.gate <- struct{}{}
	m.mu.Lock()
	p := m.services[SingBox].proc
	m.services[SingBox].restarts = m.opts.MaxRestarts
	m.mu.Unlock()
	p.signal(syscall.SIGKILL)
	<-p.done
	// Model a download holding the lane longer than StableAfter after the exit.
	<-time.After(40 * time.Millisecond)
	<-m.gate
	status := waitStatus(t, m, SingBox, func(s Status) bool { return s.State == Error })
	if status.ErrorCode != "restart_limit" || status.Restarts != 3 {
		t.Fatal("dispatch delay reset crash budget", status)
	}
}

func TestCleanupPendingSurvivesOtherOperations(t *testing.T) {
	var fail atomic.Bool
	m, opts := testManager(t, func(o *Options) {
		o.CleanupHook = func(ctx context.Context, id string) error {
			if fail.Load() {
				return errors.New("cleanup refused")
			}
			return nil
		}
	})
	acquireFixture(t, m, opts, SingBox, fixture)
	accepted(t, m, SingBox, "good", 0)
	if _, err := m.Start(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
	fail.Store(true)
	if _, err := m.Stop(context.Background(), SingBox); err == nil {
		t.Fatal("cleanup failure missing")
	}
	accepted(t, m, SingBox, "new-good", 1)
	status, err := m.Start(context.Background(), SingBox)
	if err == nil || status.PID != 0 || status.ErrorCode != "cleanup_failed" {
		t.Fatalf("accepted config erased required cleanup: %+v %v", status, err)
	}
	fail.Store(false)
	if _, err := m.Start(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
}

func TestVerifierCannotChangeAcceptedCandidate(t *testing.T) {
	m, opts := testManager(t, nil)
	changing := strings.Replace(fixture, "check|verify)", "check|verify)\n  echo 'rewritten-by-verifier' > \"$3\"", 1)
	acquireFixture(t, m, opts, SingBox, changing)
	status, err := m.Configure(context.Background(), SingBox, []byte("good"), 0)
	if err == nil || status.Generation != 0 || status.Configured {
		t.Fatalf("mutated config accepted: %+v %v", status, err)
	}
}

func TestSuccessfulLiveConfigAndArtifactReplacement(t *testing.T) {
	m, opts := testManager(t, nil)
	acquireFixture(t, m, opts, SingBox, fixture)
	accepted(t, m, SingBox, "good", 0)
	first, err := m.Start(context.Background(), SingBox)
	if err != nil {
		t.Fatal(err)
	}
	next := accepted(t, m, SingBox, "new-good", 1)
	if next.State != Running || next.PID == 0 || next.PID == first.PID || next.Generation != 2 {
		t.Fatalf("live replacement failed: %+v", next)
	}
	assertGone(t, first.PID)
	acquireFixture(t, m, opts, SingBox, fixture+"\n# verified upgrade\n")
	upgraded, _ := m.Status(SingBox)
	if upgraded.State != Running || upgraded.PID == 0 || upgraded.PID == next.PID {
		t.Fatalf("artifact live replacement failed: %+v", upgraded)
	}
	assertGone(t, next.PID)
	if _, err := m.Stop(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
	assertGone(t, upgraded.PID)
}

func TestExitPreemptsOtherServiceSlowDownloadForCleanup(t *testing.T) {
	var cleanupCalls atomic.Int32
	m, opts := testManager(t, func(o *Options) {
		o.AllowLoopbackHTTP = true
		o.CleanupHook = func(ctx context.Context, id string) error {
			if id == SingBox {
				cleanupCalls.Add(1)
			}
			return nil
		}
	})
	acquireFixture(t, m, opts, SingBox, fixture)
	accepted(t, m, SingBox, "good", 0)
	if _, err := m.Start(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
	initialCleanups := cleanupCalls.Load()
	requestStarted := make(chan struct{})
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
		w.(http.Flusher).Flush()
		close(requestStarted)
		<-r.Context().Done()
	}))
	defer server.Close()
	acquired := make(chan error, 1)
	go func() {
		_, err := m.Acquire(context.Background(), FRPC, Artifact{URL: server.URL, SHA256: strings.Repeat("0", 64), Compression: "none"})
		acquired <- err
	}()
	<-requestStarted
	m.mu.Lock()
	p := m.services[SingBox].proc
	m.mu.Unlock()
	before := time.Now()
	p.signal(syscall.SIGKILL)
	select {
	case err := <-acquired:
		if !errors.Is(err, context.Canceled) {
			t.Fatal("exit did not cancel slow download", err)
		}
	case <-time.After(time.Second):
		t.Fatal("download blocked exit cleanup")
	}
	waitStatus(t, m, SingBox, func(s Status) bool {
		return cleanupCalls.Load() > initialCleanups && (s.State == Backoff || s.State == Running)
	})
	if cleanupCalls.Load() <= initialCleanups {
		t.Fatal("crash cleanup did not run")
	}
	if time.Since(before) > time.Second {
		t.Fatal("core exit cleanup waited for download timeout")
	}
}

func TestArtifactLimitsAreTypedAndPreserveStableCore(t *testing.T) {
	m, opts := testManager(t, func(o *Options) { o.MaxCompressedBytes = 4096; o.MaxUncompressedBytes = 4096 })
	acquireFixture(t, m, opts, SingBox, fixture)
	accepted(t, m, SingBox, "good", 0)
	stable, err := m.Start(context.Background(), SingBox)
	if err != nil {
		t.Fatal(err)
	}
	cases := []struct {
		name, compression, code string
		data                    []byte
		want                    error
	}{
		{"compressed", "none", "artifact_compressed_limit", bytes.Repeat([]byte("x"), 4097), ErrArtifactCompressedLimit},
	}
	var zipped bytes.Buffer
	writer := gzip.NewWriter(&zipped)
	if _, err = writer.Write(bytes.Repeat([]byte("x"), 4097)); err != nil {
		t.Fatal(err)
	}
	if err = writer.Close(); err != nil {
		t.Fatal(err)
	}
	cases = append(cases, struct {
		name, compression, code string
		data                    []byte
		want                    error
	}{"uncompressed", "gzip", "artifact_uncompressed_limit", zipped.Bytes(), ErrArtifactUncompressedLimit})
	for _, test := range cases {
		t.Run(test.name, func(t *testing.T) {
			path := filepath.Join(opts.LocalSourceRoot, test.name)
			if err := os.WriteFile(path, test.data, 0600); err != nil {
				t.Fatal(err)
			}
			digest := sha256.Sum256(test.data)
			status, err := m.Acquire(context.Background(), SingBox, Artifact{URL: "file://" + path, SHA256: hex.EncodeToString(digest[:]), Compression: test.compression})
			if !errors.Is(err, test.want) || status.ErrorCode != test.code || status.PID != stable.PID || status.State != Running || status.Generation != stable.Generation {
				t.Fatalf("artifact bound lost diagnostic or stable core: %+v %v", status, err)
			}
		})
	}
}

func TestStartingWaitsForLocalReadiness(t *testing.T) {
	entered := make(chan struct{})
	release := make(chan struct{})
	var m *Manager
	m, opts := testManager(t, func(o *Options) {
		o.ReadyTimeout = time.Second
		o.ReadyHook = func(ctx context.Context, id string) error {
			raw, generation, err := m.Config(id)
			if err != nil || generation != 1 || string(raw) != "good" {
				return errors.New("cannot read private accepted config")
			}
			close(entered)
			select {
			case <-release:
				return nil
			case <-ctx.Done():
				return ctx.Err()
			}
		}
	})
	acquireFixture(t, m, opts, SingBox, fixture)
	accepted(t, m, SingBox, "good", 0)
	completed := make(chan error, 1)
	go func() { _, err := m.Start(context.Background(), SingBox); completed <- err }()
	<-entered
	status, _ := m.Status(SingBox)
	if status.State != Starting || status.PID == 0 || !status.Desired {
		t.Fatalf("running reported before readiness: %+v", status)
	}
	select {
	case err := <-completed:
		t.Fatal("start completed before readiness", err)
	default:
	}
	close(release)
	if err := <-completed; err != nil {
		t.Fatal(err)
	}
	status, _ = m.Status(SingBox)
	if status.State != Running || status.PID == 0 {
		t.Fatal(status)
	}
}

func TestReadinessCancellationTimeoutAndCloseCleanProcesses(t *testing.T) {
	for _, mode := range []string{"request-cancel", "timeout", "close"} {
		t.Run(mode, func(t *testing.T) {
			entered := make(chan struct{})
			var cleanups atomic.Int32
			m, opts := testManager(t, func(o *Options) {
				o.ReadyTimeout = 100 * time.Millisecond
				o.ReadyHook = func(ctx context.Context, id string) error {
					close(entered)
					<-ctx.Done()
					return errors.New("private config detail")
				}
				o.CleanupHook = func(ctx context.Context, id string) error { cleanups.Add(1); return nil }
			})
			acquireFixture(t, m, opts, SingBox, fixture)
			accepted(t, m, SingBox, "good", 0)
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			completed := make(chan error, 1)
			go func() { _, err := m.Start(ctx, SingBox); completed <- err }()
			<-entered
			status, _ := m.Status(SingBox)
			pid := status.PID
			switch mode {
			case "request-cancel":
				cancel()
			case "close":
				if err := m.Close(); err != nil {
					t.Fatal(err)
				}
			}
			var err error
			select {
			case err = <-completed:
			case <-time.After(time.Second):
				t.Fatal("readiness did not finish bounded")
			}
			if mode == "timeout" && !errors.Is(err, ErrReadiness) {
				t.Fatal(err)
			}
			if mode != "timeout" && !errors.Is(err, context.Canceled) {
				t.Fatal(err)
			}
			if strings.Contains(err.Error(), "private") {
				t.Fatal("readiness hook details leaked", err)
			}
			assertGone(t, pid)
			status, _ = m.Status(SingBox)
			if status.PID != 0 || status.Desired {
				t.Fatalf("readiness failure left process desired/live: %+v", status)
			}
			if mode != "close" && status.ErrorCode != "readiness_failed" {
				t.Fatal(status)
			}
			if cleanups.Load() < 2 {
				t.Fatal("readiness failure skipped owned cleanup", cleanups.Load())
			}
		})
	}
}

func TestLeaderExitCancelsReadinessHookPromptly(t *testing.T) {
	m, opts := testManager(t, func(o *Options) {
		o.ReadyTimeout = 5 * time.Second
		o.ReadyHook = func(ctx context.Context, id string) error { <-ctx.Done(); return ctx.Err() }
	})
	acquireFixture(t, m, opts, SingBox, fixture)
	accepted(t, m, SingBox, "crash", 0)
	before := time.Now()
	status, err := m.Start(context.Background(), SingBox)
	if !errors.Is(err, ErrReadiness) || status.PID != 0 || status.State != Error || status.ErrorCode != "readiness_failed" {
		t.Fatalf("crash readiness accepted: %+v %v", status, err)
	}
	if time.Since(before) > time.Second {
		t.Fatal("leader exit waited for full readiness timeout")
	}
}

func TestRestartReadinessUsesManagerNotOldRequestContext(t *testing.T) {
	var checks atomic.Int32
	m, opts := testManager(t, func(o *Options) {
		o.ReadyHook = func(ctx context.Context, id string) error {
			if err := ctx.Err(); err != nil {
				return err
			}
			checks.Add(1)
			return nil
		}
	})
	acquireFixture(t, m, opts, SingBox, fixture)
	accepted(t, m, SingBox, "good", 0)
	requestCtx, cancel := context.WithCancel(context.Background())
	first, err := m.Start(requestCtx, SingBox)
	if err != nil {
		t.Fatal(err)
	}
	cancel()
	m.mu.Lock()
	p := m.services[SingBox].proc
	m.mu.Unlock()
	p.signal(syscall.SIGKILL)
	restarted := waitStatus(t, m, SingBox, func(s Status) bool {
		return s.State == Running && s.PID != 0 && s.PID != first.PID && checks.Load() >= 2
	})
	if restarted.ErrorCode != "" || !restarted.Desired {
		t.Fatal(restarted)
	}
	assertGone(t, first.PID)
}

func TestFRPCReadinessMayMeanAliveNotConnected(t *testing.T) {
	m, opts := testManager(t, func(o *Options) {
		o.ReadyHook = func(ctx context.Context, id string) error {
			if id != FRPC {
				return errors.New("wrong service")
			}
			return nil
		}
	})
	acquireFixture(t, m, opts, FRPC, fixture)
	accepted(t, m, FRPC, "serverAddr = 'private.example'", 0)
	status, err := m.Start(context.Background(), FRPC)
	if err != nil || status.State != Running || status.PID == 0 {
		t.Fatalf("frpc alive rejected: %+v %v", status, err)
	}
	public, _ := json.Marshal(status)
	if strings.Contains(string(public), "connected") {
		t.Fatal("remote connectivity invented")
	}
}

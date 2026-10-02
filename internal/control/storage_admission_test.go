package control

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"sync"
	"testing"
	"time"
)

const storageAdmissionMessage = "Persistent storage needs free space for safe configuration recovery."

// The callback and release functions can run on the deadline worker. Keep every
// count and history entry behind the same lock so this fixture also works with -race.
type storageAdmissionRequest struct {
	Path          string
	Bytes         int64
	Recovery      bool
	Granted       bool
	ReleaseCalls  int
	HeldAtRequest []string
}

type storageAdmissionFixture struct {
	mu      sync.Mutex
	history []storageAdmissionRequest
	active  map[int]string
	fail    func(storageAdmissionRequest) error
	events  chan storageAdmissionRequest
}

func newStorageAdmissionFixture() *storageAdmissionFixture {
	return &storageAdmissionFixture{active: map[int]string{}, events: make(chan storageAdmissionRequest, 64)}
}

func (a *storageAdmissionFixture) admit(ctx context.Context, path string, size int64, recovery bool) (func(), error) {
	a.mu.Lock()
	r := storageAdmissionRequest{Path: path, Bytes: size, Recovery: recovery}
	for _, held := range a.active {
		r.HeldAtRequest = append(r.HeldAtRequest, held)
	}
	var err error
	if a.fail != nil {
		err = a.fail(r)
	}
	r.Granted = err == nil
	index := len(a.history)
	a.history = append(a.history, r)
	if r.Granted {
		a.active[index] = path
	}
	select {
	case a.events <- r:
	default:
	}
	a.mu.Unlock()
	if err != nil {
		return nil, err
	}
	return func() {
		a.mu.Lock()
		a.history[index].ReleaseCalls++
		delete(a.active, index)
		a.mu.Unlock()
	}, nil
}

func (a *storageAdmissionFixture) setFailure(f func(storageAdmissionRequest) error) {
	a.mu.Lock()
	a.fail = f
	a.mu.Unlock()
}

func (a *storageAdmissionFixture) snapshot() ([]storageAdmissionRequest, []string) {
	a.mu.Lock()
	defer a.mu.Unlock()
	history := append([]storageAdmissionRequest(nil), a.history...)
	for i := range history {
		history[i].HeldAtRequest = append([]string(nil), history[i].HeldAtRequest...)
	}
	var held []string
	for _, path := range a.active {
		held = append(held, path)
	}
	return history, held
}

func (a *storageAdmissionFixture) mark() int {
	history, _ := a.snapshot()
	return len(history)
}

func (a *storageAdmissionFixture) balanced(t *testing.T) {
	t.Helper()
	history, held := a.snapshot()
	if len(held) != 0 {
		t.Fatalf("storage reservations leaked: %v", held)
	}
	for i, r := range history {
		want := 0
		if r.Granted {
			want = 1
		}
		if r.ReleaseCalls != want {
			t.Errorf("admission %d for %s released %d times, want %d", i, r.Path, r.ReleaseCalls, want)
		}
	}
}

func openStorageAdmission(t *testing.T, f *fixture, a *storageAdmissionFixture, change func(*Options)) *Manager {
	t.Helper()
	o := f.options()
	o.StorageAdmission = a.admit
	if change != nil {
		change(&o)
	}
	m, err := New(o)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { m.Close() })
	return m
}

func storageAdmissionCandidate(data, path string) bool {
	rel, err := filepath.Rel(data, path)
	if err != nil {
		return false
	}
	return strings.HasPrefix(strings.Split(rel, string(filepath.Separator))[0], "candidate-")
}

func storageAdmissionFiles(t *testing.T, dir string) map[string]string {
	t.Helper()
	files := map[string]string{}
	err := filepath.WalkDir(dir, func(path string, entry os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if entry.IsDir() {
			return nil
		}
		content, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(dir, path)
		if err == nil {
			files[rel] = string(content)
		}
		return err
	})
	if err != nil {
		t.Fatal(err)
	}
	return files
}

func storageAdmissionLive(t *testing.T, f *fixture) map[string]snapshot {
	t.Helper()
	live := map[string]snapshot{}
	for _, module := range modules {
		s, err := readDocument(filepath.Join(f.root, "etc", "config", module))
		if err != nil {
			t.Fatal(err)
		}
		live[module] = s
	}
	return live
}

func storageAdmissionReloadCount(f *fixture) int {
	f.mu.Lock()
	defer f.mu.Unlock()
	return len(f.reloads)
}

func storageAdmissionError(t *testing.T, err error, code string) {
	t.Helper()
	errorCode(t, err, code)
	if code == "storage_insufficient" {
		var ce *Error
		if !errors.As(err, &ce) || ce.Message != storageAdmissionMessage {
			t.Fatalf("admission diagnostic = %v, want fixed storage message", err)
		}
	}
}

func storageAdmissionCheckGroups(t *testing.T, a *storageAdmissionFixture, start int, f *fixture, recovery bool) []storageAdmissionRequest {
	t.Helper()
	history, _ := a.snapshot()
	var groups []storageAdmissionRequest
	liveDir := filepath.Join(f.root, "etc", "config")
	for _, r := range history[start:] {
		if storageAdmissionCandidate(f.data, r.Path) {
			if r.Recovery || len(r.HeldAtRequest) != 0 {
				t.Fatalf("candidate validation used recovery/nested reservation: %#v", r)
			}
			continue
		}
		if r.Path != f.data && r.Path != liveDir {
			t.Fatalf("transaction made a nested per-write request: %#v", r)
		}
		if r.Recovery != recovery {
			t.Fatalf("recovery flag = %v for %s, want %v", r.Recovery, r.Path, recovery)
		}
		groups = append(groups, r)
	}
	if len(groups) != 2 || groups[0].Path != f.data || groups[1].Path != liveDir {
		t.Fatalf("aggregate groups = %#v, want data then live", groups)
	}
	if len(groups[0].HeldAtRequest) != 0 || !reflect.DeepEqual(groups[1].HeldAtRequest, []string{f.data}) {
		t.Fatalf("unexpected aggregate overlap: %#v", groups)
	}
	return groups
}

func storageAdmissionHeld(a *storageAdmissionFixture, f *fixture) bool {
	_, held := a.snapshot()
	return len(held) == 2 && slicesContainStoragePath(held, f.data) && slicesContainStoragePath(held, filepath.Join(f.root, "etc", "config"))
}

func slicesContainStoragePath(paths []string, want string) bool {
	for _, path := range paths {
		if path == want {
			return true
		}
	}
	return false
}

func TestStorageAdmissionStageDenialLeavesDraftsAndLiveUntouched(t *testing.T) {
	for _, target := range []string{"candidate", "state"} {
		t.Run(target, func(t *testing.T) {
			f := newFixture(t)
			a := newStorageAdmissionFixture()
			var logs bytes.Buffer
			m := openStorageAdmission(t, f, a, func(o *Options) {
				o.Logger = slog.New(slog.NewTextHandler(&logs, nil))
			})
			privateBefore := storageAdmissionFiles(t, f.data)
			liveBefore := storageAdmissionLive(t, f)
			a.setFailure(func(r storageAdmissionRequest) error {
				if target == "candidate" && storageAdmissionCandidate(f.data, r.Path) || target == "state" && r.Path == filepath.Join(f.data, "state.json") {
					return fmt.Errorf("private admission output: %s %s", f.data, f.root)
				}
				return nil
			})
			_, err := m.Stage(context.Background(), StageRequest{"dhcp", testDHCP + " option local '/denied/'\n", m.Status().Generation})
			storageAdmissionError(t, err, "storage_insufficient")
			list, err := m.Drafts(context.Background())
			if err != nil || len(list) != 0 {
				t.Fatalf("denied staging retained drafts: %#v %v", list, err)
			}
			if !reflect.DeepEqual(privateBefore, storageAdmissionFiles(t, f.data)) || !reflect.DeepEqual(liveBefore, storageAdmissionLive(t, f)) || storageAdmissionReloadCount(f) != 0 {
				t.Fatal("denied staging changed private state or live configuration")
			}
			for _, private := range []string{f.data, f.root, "private admission output"} {
				if strings.Contains(logs.String(), private) {
					t.Fatalf("private callback diagnostic leaked into logs: %s", logs.String())
				}
			}
			a.balanced(t)
		})
	}
}

func TestStorageAdmissionCommitDenialPreservesPreviousJournal(t *testing.T) {
	for _, target := range []string{"data", "live"} {
		t.Run(target, func(t *testing.T) {
			f := newFixture(t)
			a := newStorageAdmissionFixture()
			m := openStorageAdmission(t, f, a, nil)
			first := stage(t, m, "dhcp", testDHCP+" option local '/accepted/'\n")
			old, err := commit(t, m, first, false)
			if err != nil {
				t.Fatal(err)
			}
			next := stage(t, m, "dhcp", testDHCP+" option local '/denied/'\n")
			privateBefore := storageAdmissionFiles(t, f.data)
			liveBefore := storageAdmissionLive(t, f)
			reloadsBefore := storageAdmissionReloadCount(f)
			start := a.mark()
			denyPath := f.data
			if target == "live" {
				denyPath = filepath.Join(f.root, "etc", "config")
			}
			a.setFailure(func(r storageAdmissionRequest) error {
				if r.Path == denyPath {
					return errors.New("private denial path " + denyPath)
				}
				if r.Path == f.data && (!reflect.DeepEqual(privateBefore, storageAdmissionFiles(t, f.data)) || !reflect.DeepEqual(liveBefore, storageAdmissionLive(t, f))) {
					t.Error("commit wrote journal or live files before aggregate admission")
				}
				return nil
			})
			_, err = commit(t, m, next, false)
			storageAdmissionError(t, err, "storage_insufficient")
			if !reflect.DeepEqual(privateBefore, storageAdmissionFiles(t, f.data)) || !reflect.DeepEqual(liveBefore, storageAdmissionLive(t, f)) || storageAdmissionReloadCount(f) != reloadsBefore {
				t.Fatal("denied commit changed journal/state/live files or reloaded")
			}
			m.mu.Lock()
			retainedID := m.journal.Operation.ID
			m.mu.Unlock()
			if retainedID != old.ID {
				t.Fatal("denied commit replaced the retained operation")
			}
			history, _ := a.snapshot()
			var groups []storageAdmissionRequest
			for _, r := range history[start:] {
				if !storageAdmissionCandidate(f.data, r.Path) {
					groups = append(groups, r)
				}
			}
			want := 1
			if target == "live" {
				want = 2
			}
			if len(groups) != want || groups[len(groups)-1].Granted {
				t.Fatalf("unexpected partial admission: %#v", groups)
			}
			a.balanced(t)
		})
	}
}

func TestStorageAdmissionCommitHoldsBothGroupsThroughApplyAndRollback(t *testing.T) {
	for _, failApply := range []bool{false, true} {
		t.Run(fmt.Sprint("fail_apply=", failApply), func(t *testing.T) {
			f := newFixture(t)
			a := newStorageAdmissionFixture()
			m := openStorageAdmission(t, f, a, nil)
			d := stage(t, m, "dhcp", testDHCP+" option local '/candidate/'\n")
			start := a.mark()
			var holds []bool
			f.mu.Lock()
			f.reloadFn = func(context.Context, string) error {
				holds = append(holds, storageAdmissionHeld(a, f))
				if failApply && len(holds) == 1 {
					return errors.New("synthetic apply failure")
				}
				return nil
			}
			f.mu.Unlock()
			a.setFailure(func(r storageAdmissionRequest) error {
				if r.Recovery {
					return errors.New("rollback must use already held normal reservations")
				}
				return nil
			})
			op, err := commit(t, m, d, false)
			if failApply {
				errorCode(t, err, "reload_failed")
				if op.State != "rolled_back" || readFixture(t, f, "dhcp") != testDHCP || len(holds) != 2 {
					t.Fatalf("failed apply did not reuse reservation for recovery: %#v %v", op, holds)
				}
			} else if err != nil || op.State != "committed" || len(holds) != 1 {
				t.Fatalf("successful apply failed: %#v %v holds=%v", op, err, holds)
			}
			for _, held := range holds {
				if !held {
					t.Fatal("aggregate reservation was released before reload/recovery completed")
				}
			}
			storageAdmissionCheckGroups(t, a, start, f, false)
			a.balanced(t)
		})
	}
}

func TestStorageAdmissionManualRollbackDenialRetainsJournalAndCanRetry(t *testing.T) {
	for _, target := range []string{"data", "live"} {
		t.Run(target, func(t *testing.T) {
			f := newFixture(t)
			a := newStorageAdmissionFixture()
			m := openStorageAdmission(t, f, a, nil)
			d := stage(t, m, "dhcp", testDHCP+" option local '/candidate/'\n")
			op, err := commit(t, m, d, false)
			if err != nil {
				t.Fatal(err)
			}
			privateBefore := storageAdmissionFiles(t, f.data)
			liveBefore := storageAdmissionLive(t, f)
			reloadsBefore := storageAdmissionReloadCount(f)
			denyPath := f.data
			if target == "live" {
				denyPath = filepath.Join(f.root, "etc", "config")
			}
			a.setFailure(func(r storageAdmissionRequest) error {
				if r.Recovery && r.Path == denyPath {
					return errors.New("private recovery denial " + r.Path)
				}
				return nil
			})
			_, err = m.Rollback(context.Background(), op.ID)
			errorCode(t, err, "rollback_failed")
			if s := m.Status(); s.Enabled || s.ErrorCode != "rollback_failed" {
				t.Fatalf("denied recovery not surfaced: %#v", s)
			}
			if !reflect.DeepEqual(privateBefore, storageAdmissionFiles(t, f.data)) || !reflect.DeepEqual(liveBefore, storageAdmissionLive(t, f)) || storageAdmissionReloadCount(f) != reloadsBefore {
				t.Fatal("denied recovery modified durable journal/live files or reloaded")
			}
			_, err = m.Stage(context.Background(), StageRequest{"dhcp", testDHCP, m.Status().Generation})
			errorCode(t, err, "rollback_failed")
			a.balanced(t)
			a.setFailure(nil)
			start := a.mark()
			rolled, err := m.Rollback(context.Background(), op.ID)
			if err != nil || rolled.State != "rolled_back" || !m.Status().Enabled || readFixture(t, f, "dhcp") != testDHCP {
				t.Fatalf("admitted retry did not recover: %#v %v", rolled, err)
			}
			storageAdmissionCheckGroups(t, a, start, f, true)
			a.balanced(t)
		})
	}
}

func TestStorageAdmissionDeadlineDenialIsSupervisedAndRetryable(t *testing.T) {
	f := newFixture(t)
	a := newStorageAdmissionFixture()
	m := openStorageAdmission(t, f, a, func(o *Options) { o.ConfirmationTimeout = 100 * time.Millisecond })
	d := stage(t, m, "network", strings.ReplaceAll(testNetwork, "192.168.31.1", "192.168.32.1"))
	op, err := commit(t, m, d, true)
	if err != nil {
		t.Fatal(err)
	}
	privateBefore := storageAdmissionFiles(t, f.data)
	liveBefore := storageAdmissionLive(t, f)
	a.setFailure(func(r storageAdmissionRequest) error {
		if r.Recovery {
			return errors.New("synthetic recovery budget unavailable")
		}
		return nil
	})
	timer := time.NewTimer(2 * time.Second)
	defer timer.Stop()
	for {
		select {
		case r := <-a.events:
			if !r.Recovery || r.Granted {
				continue
			}
		case <-timer.C:
			t.Fatal("deadline did not attempt recovery admission")
		}
		break
	}
	if s := m.Status(); s.Enabled || s.ErrorCode != "rollback_failed" {
		t.Fatalf("deadline admission denial not surfaced: %#v", s)
	}
	if !reflect.DeepEqual(privateBefore, storageAdmissionFiles(t, f.data)) || !reflect.DeepEqual(liveBefore, storageAdmissionLive(t, f)) {
		t.Fatal("denied deadline recovery modified persistent files")
	}
	a.balanced(t)
	a.setFailure(nil)
	// Do not wait for the worker's one-second retry. Its enabled recovery state
	// must also permit an immediate manual retry of the retained operation.
	start := a.mark()
	rolled, err := m.Rollback(context.Background(), op.ID)
	if err != nil || rolled.State != "rolled_back" || readFixture(t, f, "network") != testNetwork {
		t.Fatalf("deadline recovery retry failed: %#v %v", rolled, err)
	}
	storageAdmissionCheckGroups(t, a, start, f, true)
	a.balanced(t)
}

func TestStorageAdmissionRestartRecoveryUsesRecoveryForCompletion(t *testing.T) {
	f := newFixture(t)
	a := newStorageAdmissionFixture()
	m := openStorageAdmission(t, f, a, nil)
	d := stage(t, m, "network", strings.ReplaceAll(testNetwork, "192.168.31.1", "192.168.32.1"))
	op, err := commit(t, m, d, true)
	if err != nil {
		t.Fatal(err)
	}
	m.Close()
	privateBefore := storageAdmissionFiles(t, f.data)
	liveBefore := storageAdmissionLive(t, f)
	a.setFailure(func(r storageAdmissionRequest) error {
		if r.Recovery {
			return errors.New("synthetic restart recovery denial")
		}
		return nil
	})
	o := f.options()
	o.StorageAdmission = a.admit
	_, err = New(o)
	errorCode(t, err, "rollback_failed")
	if !reflect.DeepEqual(privateBefore, storageAdmissionFiles(t, f.data)) || !reflect.DeepEqual(liveBefore, storageAdmissionLive(t, f)) {
		t.Fatal("denied restart recovery lost journal or modified live files")
	}
	a.balanced(t)
	a.setFailure(func(r storageAdmissionRequest) error {
		if !r.Recovery {
			return errors.New("normal budget unavailable during emergency recovery completion")
		}
		return nil
	})
	start := a.mark()
	recovered, err := New(o)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { recovered.Close() })
	if !recovered.Status().Enabled || recovered.Status().PendingCommit != nil || readFixture(t, f, "network") != testNetwork {
		t.Fatal("restart did not restore pending transaction")
	}
	history, _ := a.snapshot()
	if len(history[start:]) < 2 {
		t.Fatal("restart did not request aggregate recovery admission")
	}
	for _, r := range history[start:] {
		if !r.Recovery || r.Path != f.data && r.Path != filepath.Join(f.root, "etc", "config") && r.Path != filepath.Join(f.data, "state.json") {
			t.Fatalf("restart completion escaped recovery mode: %#v", r)
		}
	}
	rolled, err := recovered.Rollback(context.Background(), op.ID)
	if err != nil || rolled.State != "rolled_back" {
		t.Fatalf("restart lost retained operation: %#v %v", rolled, err)
	}
	a.balanced(t)
}

func TestStorageAdmissionCallbackCancellationMapsCancelled(t *testing.T) {
	for _, target := range []string{"candidate", "state", "data", "live"} {
		t.Run(target, func(t *testing.T) {
			f := newFixture(t)
			a := newStorageAdmissionFixture()
			m := openStorageAdmission(t, f, a, nil)
			var d Draft
			if target == "data" || target == "live" {
				d = stage(t, m, "dhcp", testDHCP+" option local '/cancelled/'\n")
			}
			privateBefore := storageAdmissionFiles(t, f.data)
			liveBefore := storageAdmissionLive(t, f)
			a.setFailure(func(r storageAdmissionRequest) error {
				match := target == "candidate" && storageAdmissionCandidate(f.data, r.Path) || target == "state" && r.Path == filepath.Join(f.data, "state.json") || target == "data" && r.Path == f.data || target == "live" && r.Path == filepath.Join(f.root, "etc", "config")
				if match {
					return fmt.Errorf("private callback cancellation: %w", context.Canceled)
				}
				return nil
			})
			var err error
			if target == "data" || target == "live" {
				_, err = commit(t, m, d, false)
			} else {
				_, err = m.Stage(context.Background(), StageRequest{"dhcp", testDHCP + " option local '/cancelled/'\n", m.Status().Generation})
			}
			errorCode(t, err, "cancelled")
			if !reflect.DeepEqual(privateBefore, storageAdmissionFiles(t, f.data)) || !reflect.DeepEqual(liveBefore, storageAdmissionLive(t, f)) {
				t.Fatal("callback cancellation changed durable files")
			}
			a.balanced(t)
		})
	}
}

func TestStorageAdmissionFullTemporaryBytesAndValidationLifetime(t *testing.T) {
	f := newFixture(t)
	a := newStorageAdmissionFixture()
	text := testDHCP + " option note '" + strings.Repeat("é<&>", 1200) + "'\n"
	m := openStorageAdmission(t, f, a, func(o *Options) {
		o.Runner = func(ctx context.Context, path string, args ...string) ([]byte, error) {
			if len(args) == 7 {
				history, held := a.snapshot()
				if len(held) != 1 || held[0] != args[2] {
					t.Errorf("candidate admission not held through isolated runner: %v", held)
				}
				var full int64
				for _, module := range modules {
					b, err := os.ReadFile(filepath.Join(args[2], module))
					if err != nil {
						t.Error(err)
					}
					full += int64(len(b))
				}
				if r := history[len(history)-1]; r.Path != args[2] || r.Bytes < full || r.Recovery {
					t.Errorf("candidate reservation does not cover full isolated texts: %#v bytes=%d", r, full)
				}
			}
			return f.run(ctx, path, args...)
		}
	})
	start := a.mark()
	d := stage(t, m, "dhcp", text)
	if !d.Valid {
		t.Fatalf("synthetic UTF-8 candidate invalid: %#v", d)
	}
	state, err := os.ReadFile(filepath.Join(f.data, "state.json"))
	if err != nil {
		t.Fatal(err)
	}
	history, _ := a.snapshot()
	if len(history[start:]) != 2 || history[start].Recovery || history[start+1].Path != filepath.Join(f.data, "state.json") || history[start+1].Recovery || history[start+1].Bytes < int64(len(state)) {
		t.Fatalf("state admission did not cover full serialized JSON: %#v bytes=%d", history[start:], len(state))
	}
	// JSON expands '<', '>', '&' and newlines. File accounting must use the
	// marshaled bytes, not Unicode character count or changed text length.
	if len(state) <= len(text) {
		t.Fatal("fixture failed to exercise JSON expansion")
	}
	start = a.mark()
	if _, err = commit(t, m, d, false); err != nil {
		t.Fatal(err)
	}
	groups := storageAdmissionCheckGroups(t, a, start, f, false)
	state, err = os.ReadFile(filepath.Join(f.data, "state.json"))
	if err != nil {
		t.Fatal(err)
	}
	journalBytes, err := os.ReadFile(filepath.Join(f.data, "journal.json"))
	if err != nil {
		t.Fatal(err)
	}
	if groups[0].Bytes < int64(len(state)+len(journalBytes)) || groups[1].Bytes < int64(len(text)+len(testDHCP)) {
		t.Fatalf("aggregate reservations do not cover complete temp writes and rollback: %#v", groups)
	}
	for path := range storageAdmissionFiles(t, f.data) {
		if strings.HasPrefix(path, "candidate-") || strings.HasPrefix(path, ".control-") {
			t.Fatalf("released admission left temporary file %s", path)
		}
	}
	a.balanced(t)
}

func TestStorageAdmissionReleaseOnValidationAndPersistenceErrors(t *testing.T) {
	for _, kind := range []string{"validation", "state", "journal"} {
		t.Run(kind, func(t *testing.T) {
			f := newFixture(t)
			a := newStorageAdmissionFixture()
			m := openStorageAdmission(t, f, a, nil)
			if kind == "validation" {
				f.mu.Lock()
				f.runErr = errors.New("synthetic validator rejection")
				f.mu.Unlock()
				d := stage(t, m, "dhcp", testDHCP+" option local '/invalid/'\n")
				if d.Valid {
					t.Fatal("validator error accepted")
				}
			} else if kind == "state" {
				path := filepath.Join(f.data, "state.json")
				if err := os.Remove(path); err != nil {
					t.Fatal(err)
				}
				if err := os.Mkdir(path, 0700); err != nil {
					t.Fatal(err)
				}
				_, err := m.Stage(context.Background(), StageRequest{"dhcp", testDHCP + " option local '/write-error/'\n", m.Status().Generation})
				errorCode(t, err, "storage_failed")
			} else {
				d := stage(t, m, "dhcp", testDHCP+" option local '/write-error/'\n")
				if err := os.Mkdir(filepath.Join(f.data, "journal.json"), 0700); err != nil {
					t.Fatal(err)
				}
				_, err := commit(t, m, d, false)
				errorCode(t, err, "storage_failed")
				if readFixture(t, f, "dhcp") != testDHCP || storageAdmissionReloadCount(f) != 0 {
					t.Fatal("initial journal write error modified live config")
				}
			}
			a.balanced(t)
		})
	}
}

func TestStorageAdmissionNilHookReadsLegacy128KiBLiveAndJournal(t *testing.T) {
	f := newFixture(t)
	legacy := testDHCP + "#" + strings.Repeat("x", MaxDocumentBytes-len(testDHCP)-2) + "\n"
	path := filepath.Join(f.root, "etc", "config", "dhcp")
	if err := os.WriteFile(path, []byte(legacy), 0644); err != nil {
		t.Fatal(err)
	}
	m := openFixture(t, f)
	d := stage(t, m, "dhcp", testDHCP+" option local '/new/'\n")
	op, err := commit(t, m, d, false)
	if err != nil {
		t.Fatal(err)
	}
	m.Close()
	restarted := openFixture(t, f)
	rolled, err := restarted.Rollback(context.Background(), op.ID)
	if err != nil || rolled.State != "rolled_back" || readFixture(t, f, "dhcp") != legacy {
		t.Fatalf("nil-hook legacy snapshot recovery failed: %#v %v", rolled, err)
	}
	var j journal
	if err = readJSON(filepath.Join(f.data, "journal.json"), &j); err != nil {
		t.Fatal(err)
	}
	encoded, err := json.Marshal(j)
	if err != nil || len(encoded) <= MaxDocumentBytes || len(j.Before["dhcp"].Content) != MaxDocumentBytes {
		t.Fatalf("legacy journal compatibility fixture failed: bytes=%d content=%d err=%v", len(encoded), len(j.Before["dhcp"].Content), err)
	}
}

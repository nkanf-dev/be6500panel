package deviceannotations

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"sync"
	"testing"

	"be6500panel/internal/storage"
)

const testMAC = "AA:BB:CC:DD:EE:FF"

func newTestStore(t *testing.T, opts Options) *Store {
	t.Helper()
	if opts.DataDir == "" {
		opts.DataDir = t.TempDir()
	}
	s, err := New(opts)
	if err != nil {
		t.Fatal(err)
	}
	return s
}
func testUpdate(revision uint64) UpdateRequest {
	return UpdateRequest{MAC: "AA-BB-CC-DD-EE-FF", Label: "Desk", Note: "LAN admin plaintext password=example", Tags: []string{"work"}, ExpectedRevision: revision}
}
func current(t *testing.T, s *Store) Snapshot {
	t.Helper()
	snap, err := s.Snapshot(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	return snap
}
func acceptedBytes(t *testing.T, s *Store) []byte {
	t.Helper()
	data, err := os.ReadFile(filepath.Join(s.dir, FileName))
	if err != nil {
		t.Fatal(err)
	}
	return data
}
func assertPreserved(t *testing.T, s *Store, before Snapshot, raw []byte) {
	t.Helper()
	if got := current(t, s); !reflect.DeepEqual(got, before) {
		t.Fatalf("state changed: %#v", got)
	}
	if got := acceptedBytes(t, s); string(got) != string(raw) {
		t.Fatal("accepted file changed")
	}
	entries, err := os.ReadDir(s.dir)
	if err != nil {
		t.Fatal(err)
	}
	for _, entry := range entries {
		if strings.HasPrefix(entry.Name(), ".device-names-") {
			t.Fatal("temporary file leaked")
		}
	}
}

func TestCanonicalMAC(t *testing.T) {
	for _, input := range []string{testMAC, "aa:bb:cc:dd:ee:ff", "AA-BB-CC-DD-EE-FF", "aabb.ccdd.eeff", "aabbccddeeff"} {
		got, err := CanonicalMAC(input)
		if err != nil || got != testMAC {
			t.Fatalf("%q: %q, %v", input, got, err)
		}
	}
	for _, input := range []string{"", "192.168.1.2", "2001:db8::1", "aa:bb:cc:dd:ee", "AA:BB:CC:DD:EE:FF:00:11", " AA:BB:CC:DD:EE:FF", "AA:BB:CC:DD:EE:FF "} {
		if _, err := CanonicalMAC(input); !errors.Is(err, ErrInvalidInput) {
			t.Fatalf("accepted %q: %v", input, err)
		}
	}
}

func TestSaveReloadCanonicalPlaintextAndPrivatePermissions(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "annotations")
	s := newTestStore(t, Options{DataDir: dir})
	if got := current(t, s); got.Revision != 0 || got.Devices == nil || len(got.Devices) != 0 {
		t.Fatalf("initial state: %#v", got)
	}
	snap, err := s.Save(context.Background(), testUpdate(0))
	if err != nil {
		t.Fatal(err)
	}
	if snap.Revision != 1 || len(snap.Devices) != 1 || snap.Devices[testMAC].Label != "Desk" || snap.Devices[testMAC].Note != testUpdate(0).Note {
		t.Fatalf("save: %#v", snap)
	}
	for path, mode := range map[string]os.FileMode{dir: 0700, filepath.Join(dir, FileName): 0600} {
		info, err := os.Stat(path)
		if err != nil || info.Mode().Perm() != mode {
			t.Fatalf("%s: mode %v, %v", path, info, err)
		}
	}
	reopened := newTestStore(t, Options{DataDir: dir})
	if got := current(t, reopened); !reflect.DeepEqual(got, snap) {
		t.Fatalf("reload: %#v", got)
	}
	snap.Devices[testMAC] = Annotation{Label: "mutated"}
	got := current(t, s)
	annotation := got.Devices[testMAC]
	annotation.Tags[0] = "mutated"
	if got := current(t, s); got.Devices[testMAC].Label != "Desk" || got.Devices[testMAC].Tags[0] != "work" {
		t.Fatal("snapshot aliases accepted state")
	}
	var disk Snapshot
	if err := json.Unmarshal(acceptedBytes(t, s), &disk); err != nil {
		t.Fatal(err)
	}
	if disk.Devices[testMAC].Note != testUpdate(0).Note {
		t.Fatal("plaintext note altered")
	}
}

func TestAnnotationRuneLimits(t *testing.T) {
	valid := UpdateRequest{MAC: testMAC, Label: strings.Repeat("猫", MaxLabelRunes), Note: strings.Repeat("猫", MaxNoteRunes), Tags: make([]string, MaxTags)}
	for i := range valid.Tags {
		valid.Tags[i] = strings.Repeat("猫", MaxTagRunes)
	}
	s := newTestStore(t, Options{})
	if _, err := s.Save(context.Background(), valid); err != nil {
		t.Fatal(err)
	}
	before, raw := current(t, s), acceptedBytes(t, s)
	cases := map[string]func(*UpdateRequest){
		"label":        func(r *UpdateRequest) { r.Label += "猫" },
		"note":         func(r *UpdateRequest) { r.Note += "猫" },
		"tags":         func(r *UpdateRequest) { r.Tags = append(r.Tags, "extra") },
		"tag":          func(r *UpdateRequest) { r.Tags[0] += "猫" },
		"invalid UTF8": func(r *UpdateRequest) { r.Note = string([]byte{0xff}) },
		"revision":     func(r *UpdateRequest) { r.ExpectedRevision = MaxRevision + 1 },
		"IP identity":  func(r *UpdateRequest) { r.MAC = "192.168.1.3" },
	}
	for name, change := range cases {
		t.Run(name, func(t *testing.T) {
			input := valid
			input.Tags = append([]string{}, valid.Tags...)
			input.ExpectedRevision = 1
			change(&input)
			if _, err := s.Save(context.Background(), input); !errors.Is(err, ErrInvalidInput) {
				t.Fatal(err)
			}
			assertPreserved(t, s, before, raw)
		})
	}
}

func TestCASAndDeletion(t *testing.T) {
	s := newTestStore(t, Options{})
	if _, err := s.Save(context.Background(), testUpdate(0)); err != nil {
		t.Fatal(err)
	}
	before, raw := current(t, s), acceptedBytes(t, s)
	if _, err := s.Save(context.Background(), testUpdate(0)); !errors.Is(err, ErrConflict) {
		t.Fatal(err)
	}
	assertPreserved(t, s, before, raw)
	snap, err := s.Save(context.Background(), UpdateRequest{MAC: testMAC, Tags: []string{}, ExpectedRevision: 1})
	if err != nil || snap.Revision != 2 || len(snap.Devices) != 0 {
		t.Fatalf("delete: %#v %v", snap, err)
	}
	if got := current(t, newTestStore(t, Options{DataDir: s.dir})); !reflect.DeepEqual(got, snap) {
		t.Fatal("delete did not persist")
	}
}

func TestMaxDevicesAndWorstCaseBound(t *testing.T) {
	s := newTestStore(t, Options{})
	devices := make(map[string]Annotation, MaxDevices)
	for i := 0; i < MaxDevices; i++ {
		devices[fmt.Sprintf("02:00:00:00:%02X:%02X", i/256, i%256)] = Annotation{Label: strings.Repeat("\x00", MaxLabelRunes), Note: strings.Repeat("\x00", MaxNoteRunes), Tags: []string{strings.Repeat("\x00", MaxTagRunes), strings.Repeat("\x00", MaxTagRunes), strings.Repeat("\x00", MaxTagRunes), strings.Repeat("\x00", MaxTagRunes), strings.Repeat("\x00", MaxTagRunes), strings.Repeat("\x00", MaxTagRunes), strings.Repeat("\x00", MaxTagRunes), strings.Repeat("\x00", MaxTagRunes)}}
	}
	raw, err := json.Marshal(Snapshot{Revision: 4, Devices: devices})
	if err != nil || len(raw) > MaxFileBytes {
		t.Fatalf("full bounded document: %d %v", len(raw), err)
	}
	if err := os.WriteFile(filepath.Join(s.dir, FileName), raw, 0600); err != nil {
		t.Fatal(err)
	}
	s = newTestStore(t, Options{DataDir: s.dir})
	before := current(t, s)
	input := testUpdate(4)
	if _, err := s.Save(context.Background(), input); !errors.Is(err, ErrLimit) {
		t.Fatal(err)
	}
	assertPreserved(t, s, before, raw)
	input.MAC = "02:00:00:00:00:00"
	if _, err := s.Save(context.Background(), input); err != nil {
		t.Fatal(err)
	}
}

func TestConcurrentCASHasOneWinner(t *testing.T) {
	s := newTestStore(t, Options{})
	var wg sync.WaitGroup
	results := make(chan error, 12)
	for i := 0; i < 12; i++ {
		wg.Add(1)
		go func() { defer wg.Done(); _, err := s.Save(context.Background(), testUpdate(0)); results <- err }()
	}
	wg.Wait()
	close(results)
	successes := 0
	for err := range results {
		if err == nil {
			successes++
		} else if !errors.Is(err, ErrConflict) {
			t.Fatal(err)
		}
	}
	if successes != 1 || current(t, s).Revision != 1 {
		t.Fatalf("%d saves won", successes)
	}
}

func TestAdmissionDenialReleaseAndCancellationPreserveCurrent(t *testing.T) {
	s := newTestStore(t, Options{})
	if _, err := s.Save(context.Background(), testUpdate(0)); err != nil {
		t.Fatal(err)
	}
	before, raw := current(t, s), acceptedBytes(t, s)
	for _, cancelOperation := range []bool{false, true} {
		t.Run(fmt.Sprintf("cancel=%v", cancelOperation), func(t *testing.T) {
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			released := false
			s.admission = func(ctx context.Context, path string, bytes int64, recovery bool) (func(), error) {
				if path != s.dir || bytes <= 0 || recovery {
					t.Fatal("invalid normal admission")
				}
				release := func() { released = true }
				if cancelOperation {
					cancel()
					return release, nil
				}
				return release, storage.ErrInsufficientSpace
			}
			_, err := s.Save(ctx, testUpdate(1))
			expected := error(storage.ErrInsufficientSpace)
			if cancelOperation {
				expected = context.Canceled
			}
			if !errors.Is(err, expected) || !released {
				t.Fatalf("err %v released %v", err, released)
			}
			assertPreserved(t, s, before, raw)
		})
	}
}

func TestSharedBudgetAndFullTemporaryReservation(t *testing.T) {
	dir := t.TempDir()
	available := int64(13 * 4096)
	budget, err := storage.New(storage.Options{ReserveBytes: 4096, EmergencyBytes: 4096, Measure: func(string) (storage.Space, error) { return storage.Space{Volume: 9, Available: available}, nil }})
	if err != nil {
		t.Fatal(err)
	}
	s := newTestStore(t, Options{DataDir: dir, StorageAdmission: budget.Admit})
	if _, err := s.Save(context.Background(), testUpdate(0)); err != nil {
		t.Fatal(err)
	}
	before, raw := current(t, s), acceptedBytes(t, s)
	releaseOther, err := budget.Admit(context.Background(), dir, 10*4096, false)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := s.Save(context.Background(), testUpdate(1)); !errors.Is(err, storage.ErrInsufficientSpace) {
		t.Fatal(err)
	}
	assertPreserved(t, s, before, raw)
	releaseOther()
	available = 3 * 4096
	if _, err := s.Save(context.Background(), testUpdate(1)); err != nil {
		t.Fatal(err)
	}
	// File-size delta is tiny, but an entire candidate must still be reserved.
	available = 2 * 4096
	before, raw = current(t, s), acceptedBytes(t, s)
	if _, err := s.Save(context.Background(), testUpdate(2)); !errors.Is(err, storage.ErrInsufficientSpace) {
		t.Fatal(err)
	}
	assertPreserved(t, s, before, raw)
}

func TestCancellationAndWriteFailureBeforeRename(t *testing.T) {
	for _, mode := range []string{"pre-cancel", "during-write", "after-sync", "cancel-error", "short-write", "ENOSPC", "sync", "rename"} {
		t.Run(mode, func(t *testing.T) {
			s := newTestStore(t, Options{})
			if _, err := s.Save(context.Background(), testUpdate(0)); err != nil {
				t.Fatal(err)
			}
			before, raw := current(t, s), acceptedBytes(t, s)
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			released := false
			s.admission = func(context.Context, string, int64, bool) (func(), error) {
				return func() {
					released = true
					entries, _ := os.ReadDir(s.dir)
					for _, entry := range entries {
						if strings.HasPrefix(entry.Name(), ".device-names-") {
							t.Error("released before temp cleanup")
						}
					}
				}, nil
			}
			switch mode {
			case "pre-cancel":
				cancel()
			case "during-write":
				s.write = func(f *os.File, data []byte) (int, error) { n, err := f.Write(data); cancel(); return n, err }
			case "after-sync":
				s.sync = func(f *os.File) error { err := f.Sync(); cancel(); return err }
			case "cancel-error":
				s.write = func(*os.File, []byte) (int, error) { cancel(); return 0, context.Canceled }
			case "sync":
				s.sync = func(*os.File) error { return errors.New("sync denied") }
			case "short-write":
				s.write = func(f *os.File, data []byte) (int, error) { return 0, nil }
			case "ENOSPC":
				s.write = func(*os.File, []byte) (int, error) { return 0, storage.ErrInsufficientSpace }
			case "rename":
				s.rename = func(string, string) error { return errors.New("rename denied") }
			}
			_, err := s.Save(ctx, testUpdate(1))
			if err == nil {
				t.Fatal("operation unexpectedly succeeded")
			}
			if strings.Contains(mode, "cancel") || mode == "during-write" || mode == "after-sync" {
				if !errors.Is(err, context.Canceled) {
					t.Fatal(err)
				}
			}
			if mode != "pre-cancel" && !released {
				t.Fatal("reservation not released")
			}
			assertPreserved(t, s, before, raw)
		})
	}
}

func TestCancelledGateWaitAndSnapshot(t *testing.T) {
	s := newTestStore(t, Options{})
	<-s.gate
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := s.Save(ctx, testUpdate(0)); !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
	if _, err := s.Snapshot(ctx); !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
	s.gate <- struct{}{}
	if _, err := s.Save(context.Background(), testUpdate(0)); err != nil {
		t.Fatal(err)
	}
}

func TestRejectCorruptOversizedAndUnsafeStore(t *testing.T) {
	invalid := []string{
		`{}`, `{"revision":0,"devices":null}`, `{"revision":9007199254740992,"devices":{}}`,
		`{"revision":1,"devices":{"aa:bb:cc:dd:ee:ff":{"label":"x","note":"","tags":[]}}}`,
		`{"revision":1,"devices":{"192.168.1.2":{"label":"x","note":"","tags":[]}}}`,
		`{"revision":1,"devices":{},"extra":true}`, `{"revision":1,"revision":2,"devices":{}}`,
		`{"revision":1,"devices":{"AA:BB:CC:DD:EE:FF":{"label":"x","note":"","tags":null}}}`,
		`{"revision":1,"devices":{}} {}`, strings.Repeat(" ", MaxFileBytes+1),
	}
	for i, raw := range invalid {
		t.Run(fmt.Sprint(i), func(t *testing.T) {
			dir := t.TempDir()
			path := filepath.Join(dir, FileName)
			if err := os.WriteFile(path, []byte(raw), 0600); err != nil {
				t.Fatal(err)
			}
			if _, err := New(Options{DataDir: dir}); !errors.Is(err, ErrStorage) {
				t.Fatal(err)
			}
			got, _ := os.ReadFile(path)
			if string(got) != raw {
				t.Fatal("bad store overwritten")
			}
		})
	}
	t.Run("symlink file", func(t *testing.T) {
		dir := t.TempDir()
		target := filepath.Join(t.TempDir(), "target")
		if err := os.WriteFile(target, []byte(`{"revision":0,"devices":{}}`), 0600); err != nil {
			t.Fatal(err)
		}
		if err := os.Symlink(target, filepath.Join(dir, FileName)); err != nil {
			t.Fatal(err)
		}
		if _, err := New(Options{DataDir: dir}); !errors.Is(err, ErrStorage) {
			t.Fatal(err)
		}
	})
	t.Run("symlink directory", func(t *testing.T) {
		dir := filepath.Join(t.TempDir(), "link")
		if err := os.Symlink(t.TempDir(), dir); err != nil {
			t.Fatal(err)
		}
		if _, err := New(Options{DataDir: dir}); !errors.Is(err, ErrStorage) {
			t.Fatal(err)
		}
	})
}

func TestRevisionExhaustionPreservesCurrent(t *testing.T) {
	dir := t.TempDir()
	raw, err := json.Marshal(Snapshot{Revision: MaxRevision, Devices: map[string]Annotation{}})
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, FileName), raw, 0600); err != nil {
		t.Fatal(err)
	}
	s := newTestStore(t, Options{DataDir: dir})
	before := current(t, s)
	if _, err := s.Save(context.Background(), testUpdate(MaxRevision)); !errors.Is(err, ErrRevisionExhausted) {
		t.Fatal(err)
	}
	assertPreserved(t, s, before, raw)
}

func TestLateCancellationReturnsCommittedRevision(t *testing.T) {
	s := newTestStore(t, Options{})
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	s.rename = func(old, new string) error {
		err := os.Rename(old, new)
		if err == nil {
			cancel()
		}
		return err
	}
	got, err := s.Save(ctx, testUpdate(0))
	if err != nil || got.Revision != 1 {
		t.Fatalf("committed save: %#v %v", got, err)
	}
	if current(t, s).Revision != 1 {
		t.Fatal("committed revision lost")
	}
	reopened := newTestStore(t, Options{DataDir: s.dir})
	if !reflect.DeepEqual(current(t, reopened), got) {
		t.Fatal("committed file and memory differ")
	}
}

func TestCancellationWhileWaitingForStoreGate(t *testing.T) {
	s := newTestStore(t, Options{})
	<-s.gate
	ctx, cancel := context.WithCancel(context.Background())
	started := make(chan struct{})
	result := make(chan error, 1)
	go func() { close(started); _, err := s.Save(ctx, testUpdate(0)); result <- err }()
	<-started
	cancel()
	if err := <-result; !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
	s.gate <- struct{}{}
	if got := current(t, s); got.Revision != 0 {
		t.Fatal("waiting cancellation saved")
	}
}

func TestAdmissionCandidateSizeAndNoAdmissionForConflict(t *testing.T) {
	s := newTestStore(t, Options{})
	calls := 0
	var admitted int64
	s.admission = func(ctx context.Context, path string, size int64, recovery bool) (func(), error) {
		calls++
		admitted = size
		if path != s.dir || recovery {
			t.Fatal("wrong admission scope")
		}
		return nil, nil // Nil releases from optional legacy callbacks are safe.
	}
	input := testUpdate(0)
	input.Label = strings.Repeat("猫", MaxLabelRunes)
	if _, err := s.Save(context.Background(), input); err != nil {
		t.Fatal(err)
	}
	if admitted != int64(len(acceptedBytes(t, s))) {
		t.Fatal("did not reserve full candidate bytes")
	}
	if _, err := s.Save(context.Background(), testUpdate(0)); !errors.Is(err, ErrConflict) {
		t.Fatal(err)
	}
	if calls != 1 {
		t.Fatal("CAS conflict made a storage reservation")
	}
}

func TestInputSlicesCopiedAndNilContextsRejected(t *testing.T) {
	s := newTestStore(t, Options{})
	input := testUpdate(0)
	if _, err := s.Save(context.Background(), input); err != nil {
		t.Fatal(err)
	}
	input.Tags[0] = "mutated"
	if current(t, s).Devices[testMAC].Tags[0] != "work" {
		t.Fatal("input tags alias state")
	}
	if _, err := s.Save(nil, testUpdate(1)); !errors.Is(err, ErrInvalidInput) {
		t.Fatal(err)
	}
	if _, err := s.Snapshot(nil); !errors.Is(err, ErrInvalidInput) {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := New(Options{DataDir: filepath.Join(t.TempDir(), "new"), Context: ctx}); !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
}

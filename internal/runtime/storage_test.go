package runtime

import (
	"bytes"
	"context"
	"errors"
	"os"
	"path/filepath"
	"syscall"
	"testing"

	"be6500panel/internal/storage"
)

func runtimeBudget(t *testing.T, free *int64) *storage.Budget {
	t.Helper()
	b, err := storage.New(storage.Options{Measure: func(string) (storage.Space, error) { return storage.Space{Volume: 1, Available: *free}, nil }})
	if err != nil {
		t.Fatal(err)
	}
	return b
}
func storeOptions(t *testing.T) Options {
	t.Helper()
	dir := t.TempDir()
	if err := privateDir(filepath.Join(dir, SingBox)); err != nil {
		t.Fatal(err)
	}
	return Options{DataDir: dir, MaxConfigBytes: 512 << 10}
}
func commitStore(t *testing.T, opts Options, state diskState, raw []byte) diskState {
	t.Helper()
	candidate, err := stageConfig(opts, SingBox, raw)
	if err != nil {
		t.Fatal(err)
	}
	defer os.Remove(candidate)
	next, err := commitConfig(opts, SingBox, state, candidate, raw)
	if err != nil {
		t.Fatal(err)
	}
	return next
}
func TestConfigTransactionAdmissionBeforePrivateWrite(t *testing.T) {
	opts := storeOptions(t)
	free := int64(128 << 10)
	b := runtimeBudget(t, &free)
	opts.StorageAdmission = b.Admit
	_, _, err := admitConfig(opts, context.Background(), SingBox, []byte("candidate"), false)
	if !errors.Is(err, storage.ErrInsufficientSpace) {
		t.Fatal(err)
	}
	entries, err := os.ReadDir(filepath.Join(opts.DataDir, SingBox))
	if err != nil || len(entries) != 0 {
		t.Fatal(entries, err)
	}
}
func TestConfigCommitRollbackAndReadySnapshotAdmission(t *testing.T) {
	opts := storeOptions(t)
	free := int64(6 << 20)
	b := runtimeBudget(t, &free)
	opts.StorageAdmission = b.Admit
	first := commitStore(t, opts, diskState{}, []byte("known ready"))
	first.Current.Ready = true
	if err := saveState(opts, SingBox, first); err != nil {
		t.Fatal(err)
	}
	second := commitStore(t, opts, first, []byte("new unchecked"))
	if second.LastGood == nil || !second.LastGood.Ready {
		t.Fatal("lost proven rollback", second)
	}
	// A second unproven change must not replace the ready rollback snapshot.
	third := commitStore(t, opts, second, []byte("another unchecked"))
	if third.LastGood.Generation != first.Current.Generation {
		t.Fatal(third)
	}
	free = 128 << 10 // Ordinary commits denied; restore can use recovery headroom.
	_, _, err := admitConfig(opts, context.Background(), SingBox, []byte("new"), false)
	if !errors.Is(err, storage.ErrInsufficientSpace) {
		t.Fatal(err)
	}
	release, recoveryOpts, err := admitConfig(opts, context.Background(), SingBox, []byte("known ready"), true)
	if err != nil {
		t.Fatal(err)
	}
	restored := commitStore(t, recoveryOpts, third, []byte("known ready"))
	release()
	if restored.Generation != third.Generation+1 {
		t.Fatal(restored)
	}
}
func TestCommitMetadataDenialPreventsCandidateRename(t *testing.T) {
	opts := storeOptions(t)
	first := commitStore(t, opts, diskState{}, []byte("accepted"))
	first.Current.Ready = true
	if err := saveState(opts, SingBox, first); err != nil {
		t.Fatal(err)
	}
	before, err := os.ReadFile(statePath(opts, SingBox))
	if err != nil {
		t.Fatal(err)
	}
	candidate, err := stageConfig(opts, SingBox, []byte("candidate"))
	if err != nil {
		t.Fatal(err)
	}
	defer os.Remove(candidate)
	free := int64(0)
	b := runtimeBudget(t, &free)
	opts.StorageAdmission = b.Admit
	next, err := commitConfig(opts, SingBox, first, candidate, []byte("candidate"))
	if !errors.Is(err, storage.ErrInsufficientSpace) || next.Generation != first.Generation {
		t.Fatal(next, err)
	}
	if _, err = os.Stat(candidate); err != nil {
		t.Fatal("candidate renamed before metadata admission", err)
	}
	after, err := os.ReadFile(statePath(opts, SingBox))
	if err != nil || !bytes.Equal(before, after) {
		t.Fatal("manifest changed", err)
	}
}
func TestPrivateWriteENOSPCAndCancellationPreserveAcceptedState(t *testing.T) {
	opts := storeOptions(t)
	first := commitStore(t, opts, diskState{}, []byte("accepted"))
	before, err := os.ReadFile(statePath(opts, SingBox))
	if err != nil {
		t.Fatal(err)
	}
	opts.storageWrite = func(*os.File, []byte) (int, error) { return 0, syscall.ENOSPC }
	if _, err = stageConfig(opts, SingBox, []byte("new")); !errors.Is(err, storage.ErrInsufficientSpace) {
		t.Fatal(err)
	}
	if err = saveState(opts, SingBox, diskState{Generation: 2}); !errors.Is(err, storage.ErrInsufficientSpace) {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	opts.storageContext = ctx
	opts.storageWrite = func(f *os.File, p []byte) (int, error) { n, err := f.Write(p); cancel(); return n, err }
	if _, err = stageConfig(opts, SingBox, []byte("cancelled")); !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
	after, err := os.ReadFile(statePath(opts, SingBox))
	if err != nil || !bytes.Equal(before, after) {
		t.Fatal("accepted manifest changed", err)
	}
	entries, err := os.ReadDir(filepath.Join(opts.DataDir, SingBox))
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 2 {
		t.Fatalf("leaked private temps: %v", entries)
	}
	if first.Generation != 1 {
		t.Fatal(first)
	}
}
func TestLowerWriteLimitStillReadsLegacyAcceptedConfig(t *testing.T) {
	opts := storeOptions(t)
	legacy := bytes.Repeat([]byte{'x'}, 2<<20)
	first := commitStore(t, opts, diskState{}, legacy)
	first.Current.Ready = true
	if err := saveState(opts, SingBox, first); err != nil {
		t.Fatal(err)
	}
	loaded, err := loadState(opts, SingBox)
	if err != nil || loaded.Generation != 1 {
		t.Fatal(loaded, err)
	}
	if loaded.Current == nil || !loaded.Current.Ready {
		t.Fatal(loaded)
	}
}

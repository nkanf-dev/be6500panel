package traffic

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"syscall"
	"testing"

	"be6500panel/internal/storage"
)

func historyBudget(t *testing.T, free *int64) *storage.Budget {
	t.Helper()
	b, err := storage.New(storage.Options{Measure: func(string) (storage.Space, error) { return storage.Space{Volume: 1, Available: *free}, nil }})
	if err != nil {
		t.Fatal(err)
	}
	return b
}
func TestHistoryAdmissionBeforeAnyRingAllocation(t *testing.T) {
	dir := t.TempDir()
	free := int64(3 << 20)
	b := historyBudget(t, &free)
	_, err := New(Options{DataDir: dir, Source: &fakeSource{}, StorageAdmission: b.Admit})
	if !errors.Is(err, storage.ErrInsufficientSpace) {
		t.Fatal(err)
	}
	entries, err := os.ReadDir(dir)
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 0 {
		t.Fatalf("denied allocation wrote files: %v", entries)
	}
	// Six MiB leaves room for the complete fixed-year history and safe config.
	free = 6 << 20
	c, err := New(Options{DataDir: dir, Source: &fakeSource{}, StorageAdmission: b.Admit})
	if err != nil {
		t.Fatal(err)
	}
	if err = c.Close(); err != nil {
		t.Fatal(err)
	}
}
func TestExistingHistoryReopensAndFlushesBelowReserve(t *testing.T) {
	dir := t.TempDir()
	c := newTestCollector(t, dir)
	if err := c.Close(); err != nil {
		t.Fatal(err)
	}
	free := int64(0)
	b := historyBudget(t, &free)
	reopened, err := New(Options{DataDir: dir, Source: &fakeSource{}, StorageAdmission: b.Admit})
	if err != nil {
		t.Fatal(err)
	}
	start := alignedTime()
	reopened.record(snapshot(start, "wan", 1, 2))
	reopened.record(snapshot(start.Add(SampleInterval), "wan", 100, 200))
	if err = reopened.Flush(); err != nil {
		t.Fatal(err)
	}
	if err = reopened.Close(); err != nil {
		t.Fatal(err)
	}
}
func TestTruncatedHistoryDoesNotSpendRollbackReserve(t *testing.T) {
	dir := t.TempDir()
	c := newTestCollector(t, dir)
	if err := c.Close(); err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(dir, "wan-30s.ring")
	if err := os.Truncate(path, headerSize+64); err != nil {
		t.Fatal(err)
	}
	free := int64(0)
	b := historyBudget(t, &free)
	_, err := New(Options{DataDir: dir, Source: &fakeSource{}, StorageAdmission: b.Admit})
	if !errors.Is(err, storage.ErrInsufficientSpace) {
		t.Fatal(err)
	}
	info, err := os.Stat(path)
	if err != nil || info.Size() != headerSize+64 {
		t.Fatal(info, err)
	}
	// Other previously allocated tiers remain present, never deleted for space.
	for _, seconds := range []int{300, 3600} {
		if _, err = os.Stat(filepath.Join(dir, fmt.Sprintf("wan-%ds.ring", seconds))); err != nil {
			t.Fatal(err)
		}
	}
}
func TestAllocationENOSPCCleansOnlyNewRings(t *testing.T) {
	dir := t.TempDir()
	free := int64(6 << 20)
	b := historyBudget(t, &free)
	written := 0
	_, err := New(Options{DataDir: dir, Source: &fakeSource{}, StorageAdmission: b.Admit, writeAt: func(f *os.File, p []byte, offset int64) (int, error) {
		written += len(p)
		if written > 800<<10 {
			return 0, syscall.ENOSPC
		}
		return f.WriteAt(p, offset)
	}})
	if !errors.Is(err, storage.ErrInsufficientSpace) {
		t.Fatal(err)
	}
	entries, err := os.ReadDir(dir)
	if err != nil || len(entries) != 0 {
		t.Fatal(entries, err)
	}
	// A failed write releases its global reservation so config can still commit.
	release, err := b.Admit(context.Background(), dir, 512<<10, false)
	if err != nil {
		t.Fatal(err)
	}
	release()
}
func TestCanceledHistoryAllocationCleansPartialFiles(t *testing.T) {
	dir := t.TempDir()
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	_, err := New(Options{DataDir: dir, Source: &fakeSource{}, Context: ctx, writeAt: func(f *os.File, p []byte, offset int64) (int, error) { cancel(); return f.WriteAt(p, offset) }})
	if !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
	entries, err := os.ReadDir(dir)
	if err != nil || len(entries) != 0 {
		t.Fatal(entries, err)
	}
}

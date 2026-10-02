package storage

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"sync"
	"sync/atomic"
	"syscall"
	"testing"
)

type tightFS struct {
	free      int64
	size      int64
	allocated int
	reclaimed int
	fail      error
}

func (f *tightFS) Size() (int64, error) { return f.size, nil }
func (f *tightFS) Allocate(ctx context.Context, n int64) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	if f.fail != nil {
		return f.fail
	}
	if n > f.free {
		return syscall.ENOSPC
	}
	f.free -= n
	f.size = n
	f.allocated++
	return nil
}
func (f *tightFS) Reclaim() error {
	if f.fail != nil {
		return f.fail
	}
	f.free += f.size
	f.size = 0
	f.reclaimed++
	return nil
}
func (f *tightFS) measure(string) (Space, error) { return Space{Volume: 1, Available: f.free}, nil }
func newTightBudget(t *testing.T, f *tightFS) *Budget {
	t.Helper()
	b, err := New(Options{Measure: f.measure, EmergencyPath: "/data/.reserve", EmergencyStore: f})
	if err != nil {
		t.Fatal(err)
	}
	return b
}
func TestMeasuredSharedAdmission(t *testing.T) {
	fs := &tightFS{free: 6 << 20}
	b := newTightBudget(t, fs)
	if err := b.Replenish(context.Background()); err != nil {
		t.Fatal(err)
	}
	history, err := b.Admit(context.Background(), "/data/traffic", 3072192, false)
	if err != nil {
		t.Fatal(err)
	}
	// Concurrent admission cannot spend space already promised to history.
	if _, err = b.Admit(context.Background(), "/data/config", 2<<20, false); !errors.Is(err, ErrInsufficientSpace) {
		t.Fatal(err)
	}
	config, err := b.Admit(context.Background(), "/data/config", 512<<10, false)
	if err != nil {
		t.Fatal(err)
	}
	config()
	config()
	history()
	if _, err = b.Admit(context.Background(), "/data/config", 2<<20, false); err != nil {
		t.Fatal(err)
	}
	if fs.reclaimed != 0 {
		t.Fatal("normal admission reclaimed recovery reserve")
	}
}
func TestLowSpaceAndRollbackOnlyReclaimsEmergency(t *testing.T) {
	fs := &tightFS{free: 6 << 20}
	b := newTightBudget(t, fs)
	if err := b.Replenish(context.Background()); err != nil {
		t.Fatal(err)
	}
	fs.free = 0 // Another writer exhausted flash, but the emergency file exists.
	if _, err := b.Admit(context.Background(), "/data/history", 32<<10, false); !errors.Is(err, ErrInsufficientSpace) {
		t.Fatal(err)
	}
	if fs.reclaimed != 0 {
		t.Fatal("history stole rollback space")
	}
	release, err := b.Admit(context.Background(), "/data/config", 64<<10, true)
	if err != nil {
		t.Fatal(err)
	}
	if fs.reclaimed != 1 || fs.size != 0 {
		t.Fatal(fs)
	}
	release()
	if err = b.Replenish(context.Background()); !errors.Is(err, ErrInsufficientSpace) {
		t.Fatal(err)
	}
	fs.free = 6 << 20
	if err = b.Replenish(context.Background()); err != nil {
		t.Fatal(err)
	}
	if fs.allocated != 2 {
		t.Fatal(fs)
	}
}
func TestConcurrentReservations(t *testing.T) {
	fs := &tightFS{free: 2 << 20}
	b := newTightBudget(t, fs)
	var wg sync.WaitGroup
	var admitted atomic.Int64
	releases := make(chan func(), 32)
	for i := 0; i < 32; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			release, err := b.Admit(context.Background(), "/data", 128<<10, false)
			if err == nil {
				admitted.Add(1)
				releases <- release
			} else if !errors.Is(err, ErrInsufficientSpace) {
				t.Error(err)
			}
		}()
	}
	wg.Wait()
	close(releases)
	if admitted.Load() != 7 {
		t.Fatalf("admitted=%d", admitted.Load())
	}
	for release := range releases {
		release()
	}
	if len(b.pending) != 0 {
		t.Fatal(b.pending)
	}
}
func TestCancellationMeasurementAndInjectedENOSPC(t *testing.T) {
	fs := &tightFS{free: 6 << 20}
	b := newTightBudget(t, fs)
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := b.Admit(ctx, "/data", 1, false); !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
	if err := b.Replenish(ctx); !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
	fs.fail = syscall.ENOSPC
	if err := b.Replenish(context.Background()); !errors.Is(err, ErrInsufficientSpace) {
		t.Fatal(err)
	}
	if len(b.pending) != 0 || fs.size != 0 {
		t.Fatal("failed write leaked reservation")
	}
	b.measure = func(string) (Space, error) { return Space{}, syscall.EIO }
	if _, err := b.Admit(context.Background(), "/data", 1, true); !errors.Is(err, ErrMeasurement) {
		t.Fatal(err)
	}
}
func TestRecoveryDoesNotReclaimOtherVolume(t *testing.T) {
	fs := &tightFS{size: 256 << 10}
	b := newTightBudget(t, fs)
	b.measure = func(path string) (Space, error) {
		if path == "/data/.reserve" {
			return Space{Volume: 1}, nil
		}
		return Space{Volume: 2}, nil
	}
	if _, err := b.Admit(context.Background(), "/etc", 1, true); !errors.Is(err, ErrInsufficientSpace) {
		t.Fatal(err)
	}
	if fs.reclaimed != 0 {
		t.Fatal("reclaimed another filesystem")
	}
}
func TestPhysicalReserveAndNearestExistingParent(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, ".reserve")
	b, err := New(Options{EmergencyPath: path})
	if err != nil {
		t.Fatal(err)
	}
	if err = b.Replenish(context.Background()); err != nil {
		t.Fatal(err)
	}
	info, err := os.Stat(path)
	if err != nil {
		t.Fatal(err)
	}
	if info.Size() != DefaultEmergencyBytes || info.Mode().Perm() != 0600 {
		t.Fatal(info)
	}
	space, err := measureSpace(filepath.Join(dir, "missing", "candidate"))
	if err != nil || space.Available <= 0 {
		t.Fatal(space, err)
	}
	r := &fileReserve{path: path}
	if err = r.Reclaim(); err != nil {
		t.Fatal(err)
	}
	if _, err = os.Stat(path); !os.IsNotExist(err) {
		t.Fatal(err)
	}
}

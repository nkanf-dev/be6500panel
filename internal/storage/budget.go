// Package storage admits temporary growth against measured shared filesystem
// space. It protects recovery headroom, not per-feature disk quotas.
package storage

import (
	"context"
	"errors"
	"math"
	"sync"
)

const (
	DefaultReserveBytes   int64 = 1 << 20
	DefaultEmergencyBytes int64 = 256 << 10
	allocationUnit        int64 = 4096
)

var (
	ErrInsufficientSpace = errors.New("persistent storage needs free space for safe configuration recovery")
	ErrMeasurement       = errors.New("persistent storage free space is unavailable")
)

// Admission reserves full new/temporary bytes, not final-size growth. Hold the
// release until all writes and cleanup finish. Recovery is only for restoring
// existing accepted state, never a new draft, history, or artifact.
type Admission func(context.Context, string, int64, bool) (func(), error)

// Space identifies a filesystem and bytes available to this process. Inject
// Measure for portable tight-filesystem tests. Production uses statfs.
type Space struct {
	Volume    uint64
	Available int64
}

// EmergencyStore owns a physically allocated file on the protected filesystem.
// Only recovery admission can Reclaim it. Allocate must clean failed writes.
// Injection permits ENOSPC tests without filling the host filesystem.
type EmergencyStore interface {
	Size() (int64, error)
	Allocate(context.Context, int64) error
	Reclaim() error
}

type Options struct {
	ReserveBytes   int64
	EmergencyBytes int64
	EmergencyPath  string
	Measure        func(string) (Space, error)
	EmergencyStore EmergencyStore
}

type Budget struct {
	mu                      sync.Mutex
	reserve, emergencyBytes int64
	emergencyPath           string
	measure                 func(string) (Space, error)
	emergency               EmergencyStore
	pending                 map[uint64]int64
}

// New creates an admission gate, without allocating an emergency file. Share
// this instance among all owners of a persistent volume. Replenish is explicit.
func New(opts Options) (*Budget, error) {
	if opts.ReserveBytes == 0 {
		opts.ReserveBytes = DefaultReserveBytes
	}
	if opts.EmergencyBytes == 0 {
		opts.EmergencyBytes = DefaultEmergencyBytes
	}
	if opts.ReserveBytes < 0 || opts.EmergencyBytes < 0 {
		return nil, errors.New("invalid storage reserve")
	}
	if opts.Measure == nil {
		opts.Measure = measureSpace
	}
	b := &Budget{reserve: opts.ReserveBytes, emergencyBytes: opts.EmergencyBytes, emergencyPath: opts.EmergencyPath, measure: opts.Measure, pending: make(map[uint64]int64), emergency: opts.EmergencyStore}
	if b.emergency == nil && opts.EmergencyPath != "" {
		b.emergency = &fileReserve{path: opts.EmergencyPath}
	}
	if b.emergency != nil && b.emergencyPath == "" {
		return nil, errors.New("emergency reserve requires its filesystem path")
	}
	return b, nil
}

func charge(bytes int64) (int64, error) {
	if bytes < 0 || bytes > math.MaxInt64-2*allocationUnit {
		return 0, errors.New("invalid storage allocation")
	}
	if bytes == 0 {
		return 0, nil
	}
	return ((bytes+allocationUnit-1)/allocationUnit + 1) * allocationUnit, nil
}

// Admit checks current free bytes and outstanding reservations under one lock.
// Reservations intentionally stay charged after writes become visible to
// statfs; this conservative accounting prevents concurrent overcommit. Existing
// fixed-size overwrites need no admission. External writers may still cause
// ENOSPC, which callers must return without replacing accepted state.
func (b *Budget) Admit(ctx context.Context, path string, bytes int64, recovery bool) (func(), error) {
	if ctx == nil {
		return nil, errors.New("storage context is required")
	}
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	n, err := charge(bytes)
	if err != nil {
		return nil, err
	}
	if n == 0 {
		return func() {}, nil
	}
	b.mu.Lock()
	defer b.mu.Unlock()
	if err = ctx.Err(); err != nil {
		return nil, err
	}
	space, err := b.measure(path)
	if err != nil || space.Available < 0 {
		return nil, ErrMeasurement
	}
	held := b.pending[space.Volume]
	reserve := b.reserve
	if recovery {
		reserve = 0
	}
	fits := func() bool {
		return held <= space.Available && n <= space.Available-held && reserve <= space.Available-held-n
	}
	if !fits() && recovery && b.emergency != nil {
		// Reclaim only when it is on the requested filesystem. A volatile or split
		// filesystem cannot borrow persistent emergency space.
		emergencySpace, measureErr := b.measure(b.emergencyPath)
		if measureErr != nil {
			return nil, ErrMeasurement
		}
		if emergencySpace.Volume == space.Volume {
			size, sizeErr := b.emergency.Size()
			if sizeErr != nil {
				return nil, ErrMeasurement
			}
			if size > 0 {
				if err = ctx.Err(); err != nil {
					return nil, err
				}
				if err = b.emergency.Reclaim(); err != nil {
					return nil, ErrInsufficientSpace
				}
				space, err = b.measure(path)
				if err != nil || space.Available < 0 {
					return nil, ErrMeasurement
				}
			}
		}
	}
	if !fits() {
		return nil, ErrInsufficientSpace
	}
	if err = ctx.Err(); err != nil {
		return nil, err
	}
	b.pending[space.Volume] = held + n
	var once sync.Once
	return func() {
		once.Do(func() {
			b.mu.Lock()
			defer b.mu.Unlock()
			b.pending[space.Volume] -= n
			if b.pending[space.Volume] == 0 {
				delete(b.pending, space.Volume)
			}
		})
	}, nil
}

// Replenish restores the emergency file only when ordinary free headroom is
// available. Call at startup or after explicit space cleanup, not during a
// rollback. Failure leaves existing recovery space intact and is reportable.
func (b *Budget) Replenish(ctx context.Context) error {
	if ctx == nil {
		return errors.New("storage context is required")
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	if b.emergency == nil {
		return nil
	}
	b.mu.Lock()
	defer b.mu.Unlock()
	size, err := b.emergency.Size()
	if err != nil {
		return ErrMeasurement
	}
	if size == b.emergencyBytes {
		return nil
	}
	if size != 0 {
		return errors.New("emergency reserve has unexpected size")
	}
	space, err := b.measure(b.emergencyPath)
	if err != nil || space.Available < 0 {
		return ErrMeasurement
	}
	n, err := charge(b.emergencyBytes)
	if err != nil {
		return err
	}
	held := b.pending[space.Volume]
	if held > space.Available || n > space.Available-held || b.reserve > space.Available-held-n {
		return ErrInsufficientSpace
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	if err := b.emergency.Allocate(ctx, b.emergencyBytes); err != nil {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		return ErrInsufficientSpace
	}
	return nil
}

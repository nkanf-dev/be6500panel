package control

import (
	"context"
	"errors"
	"path/filepath"
	"time"
)

// Temporary allocations include full file contents, block rounding, and one
// filesystem metadata block per atomic write. The shared owner may add overhead.
func temporaryBytes(size int) int64 {
	const block = int64(4096)
	return (int64(size)+block-1)/block*block + block
}

func isStorageAdmissionError(err error) bool {
	var typed *Error
	return errors.As(err, &typed) && (typed.Code == "storage_insufficient" || typed.Code == "cancelled")
}

func (m *Manager) admitStorage(ctx context.Context, path string, size int64, recovery bool) (func(), error) {
	if ctx.Err() != nil {
		return nil, failure("cancelled", "Configuration operation was cancelled.")
	}
	if m.storageAdmission == nil || size == 0 {
		return func() {}, nil
	}
	release, err := m.storageAdmission(ctx, path, size, recovery)
	if err != nil {
		if release != nil {
			release()
		}
		if ctx.Err() != nil || errors.Is(err, context.Canceled) || errors.Is(err, context.DeadlineExceeded) {
			return nil, failure("cancelled", "Configuration operation was cancelled.")
		}
		return nil, failure("storage_insufficient", "Persistent storage needs free space for safe configuration recovery.")
	}
	if release == nil {
		release = func() {}
	}
	return release, nil
}

// reserveStorage holds both paths together. They can share a filesystem or
// reside on different filesystems; the shared callback owns that distinction.
// Only Manager's mutation gate uses this flag, so nested writes do not reserve
// the same bytes again. Releases run after the operation and all temp cleanup.
func (m *Manager) reserveStorage(ctx context.Context, dataBytes, liveBytes int64, recovery bool) (func(), error) {
	if m.storageReserved || m.storageAdmission == nil {
		return func() {}, nil
	}
	releaseData, err := m.admitStorage(ctx, m.dataDir, dataBytes, recovery)
	if err != nil {
		return nil, err
	}
	releaseLive, err := m.admitStorage(ctx, filepath.Join(m.root, "etc", "config"), liveBytes, recovery)
	if err != nil {
		releaseData()
		return nil, err
	}
	m.storageReserved = true
	return func() {
		m.storageReserved = false
		releaseLive()
		releaseData()
	}, nil
}

func (m *Manager) finishStorageReservation(release func()) {
	if release != nil {
		release()
	}
}

// State generation counters can grow during recovery. Reserve the largest
// decimal counter representation rather than depending on the current value.
func storageStateBytes(state diskState) (int64, error) {
	state.Generation = ^uint64(0)
	data, err := marshalStore(state)
	return temporaryBytes(len(data)), err
}

func storageJournalBytes(j journal) (int64, error) {
	j.BaseGeneration = ^uint64(0)
	j.Operation.Generation = ^uint64(0)
	j.Operation.State = "pending_confirmation"
	j.Phase = "rolling_back"
	// The timestamp with fractional seconds is longer than other normal phases.
	deadline := time.Date(9999, 12, 31, 23, 59, 59, 999999999, time.UTC)
	j.Operation.Deadline = &deadline
	data, err := marshalStore(j)
	return temporaryBytes(len(data)), err
}

func (m *Manager) reserveCommitStorage(ctx context.Context, j journal, candidates map[string]string) (func(), error) {
	if m.storageAdmission == nil {
		return func() {}, nil
	}
	stateBytes, err := storageStateBytes(m.disk)
	if err != nil {
		return nil, failure("storage_failed", "Cannot size private configuration state.")
	}
	journalBytes, err := storageJournalBytes(j)
	if err != nil {
		return nil, failure("storage_failed", "Cannot size configuration rollback journal.")
	}
	// Keep the full successful apply plus a full recovery sequence reserved.
	// Sum allocations, not final growth: no later callback or file-size delta can
	// spend recovery scratch while a partially applied transaction is active.
	dataBytes := 4*journalBytes + 3*stateBytes
	var liveBytes int64
	for _, module := range j.Operation.ChangedModules {
		liveBytes += temporaryBytes(len(candidates[module]))
		if prior := j.Before[module]; prior.Exists {
			liveBytes += temporaryBytes(len(prior.Content))
		}
	}
	return m.reserveStorage(ctx, dataBytes, liveBytes, false)
}

func (m *Manager) reserveRollbackStorage(ctx context.Context, j journal) (func(), error) {
	if m.storageReserved || m.storageAdmission == nil {
		return func() {}, nil
	}
	stateBytes, err := storageStateBytes(m.disk)
	if err != nil {
		return nil, failure("storage_failed", "Cannot size private configuration state.")
	}
	journalBytes, err := storageJournalBytes(j)
	if err != nil {
		return nil, failure("storage_failed", "Cannot size configuration rollback journal.")
	}
	var liveBytes int64
	for _, prior := range j.Before {
		if prior.Exists {
			liveBytes += temporaryBytes(len(prior.Content))
		}
	}
	return m.reserveStorage(ctx, 2*journalBytes+stateBytes, liveBytes, true)
}

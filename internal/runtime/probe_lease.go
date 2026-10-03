package runtime

import (
	"context"
	"crypto/sha256"
	"errors"
	"io"
	"os"
	"path/filepath"
	"sync"
	"syscall"
)

// ErrProbeLease is deliberately fixed: filesystem errors must not expose a
// private executable path, configuration, or process arguments to callers.
var ErrProbeLease = errors.New("verified probe executable is unavailable")

// RuntimeProbeLease owns a frozen, checksum-verified sing-box executable copy
// in the manager's private RAM RunDir. Path is private process input, not a
// public API field: callers must never serialize or log it. A copied lease
// shares the same release ownership. Its zero value is safe to release.
//
// The probe owner must stop and reap every process using Path before Release.
// Root must close that owner before closing the runtime manager. The manager
// does not release leases on Close, because doing so could race a probe process.
// Only one lease may be owned at a time; callers must release even failed jobs.
// RunDir must be RAM/tmpfs in production, as for managed runtime artifacts.
// This lease never starts or changes the live runtime or its owned resources.
type RuntimeProbeLease struct {
	state *probeLeaseState
}

type probeLeaseState struct {
	once    sync.Once
	manager *Manager
	path    string
	dir     string
	file    os.FileInfo
	dirInfo os.FileInfo
}

// Path returns an absolute private executable path valid until Release. It is
// intentionally a method, so JSON encoding cannot expose the path by default.
func (l RuntimeProbeLease) Path() string {
	if l.state == nil {
		return ""
	}
	return l.state.path
}

func (RuntimeProbeLease) String() string   { return "RuntimeProbeLease{private}" }
func (RuntimeProbeLease) GoString() string { return "RuntimeProbeLease{private}" }

// Release removes only this lease's copy, not the active artifact or another
// job's files. It is safe for concurrent calls and copied lease values. If the
// owned file cannot be removed, keep the cap occupied rather than permit RAM
// growth. Release does not acquire the runtime mutation lane or change status.
func (l RuntimeProbeLease) Release() {
	if l.state == nil {
		return
	}
	s := l.state
	s.once.Do(func() {
		if !removeProbeCopy(s) {
			return
		}
		s.manager.mu.Lock()
		if s.manager.probeLease == s {
			s.manager.probeLease = nil
		}
		s.manager.mu.Unlock()
	})
}

// AcquireProbeLease is fixed to sing-box. Admission and the caller's read-only
// guard run before filesystem work. The shared lane covers only this bounded
// checksum/copy, never the caller's probe job. Guard errors must be safe public
// errors, just as for the other Guarded manager methods.
func (m *Manager) AcquireProbeLease(ctx context.Context, guard func(context.Context) error) (RuntimeProbeLease, error) {
	ctx, done, err := m.begin(ctx, SingBox)
	if err != nil {
		return RuntimeProbeLease{}, err
	}
	defer done()
	if err := checkMutationGuard(ctx, guard); err != nil {
		return RuntimeProbeLease{}, err
	}
	// File work is finite even if the caller supplies an unbounded context.
	ctx, cancel := context.WithTimeout(ctx, m.opts.CheckTimeout)
	defer cancel()
	m.mu.Lock()
	service := m.services[SingBox]
	binary, want := service.binary, service.binarySHA256
	busy := m.probeLease != nil
	m.mu.Unlock()
	if busy {
		return RuntimeProbeLease{}, ErrBusy
	}
	if binary == "" || want == [sha256.Size]byte{} {
		return RuntimeProbeLease{}, ErrNoArtifact
	}
	source, original, err := openProbeOriginal(m.opts, binary)
	if err != nil {
		return RuntimeProbeLease{}, err
	}
	defer source.Close()
	if err := ctx.Err(); err != nil {
		return RuntimeProbeLease{}, err
	}
	if err := checkRunSpace(m.opts); err != nil {
		return RuntimeProbeLease{}, ErrProbeLease
	}
	dir, err := os.MkdirTemp(m.opts.RunDir, ".probe-lease-*")
	if err != nil {
		return RuntimeProbeLease{}, ErrProbeLease
	}
	state := &probeLeaseState{manager: m, dir: dir, path: filepath.Join(dir, SingBox)}
	published := false
	defer func() {
		if !published {
			// No other files are owned here; never recursively remove this tree.
			_ = os.Remove(state.path)
			_ = os.Remove(dir)
		}
	}()
	state.dirInfo, err = os.Lstat(dir)
	if err != nil || !state.dirInfo.IsDir() || state.dirInfo.Mode().Perm() != 0700 || !privateProbeRoot(m.opts.RunDir) {
		return RuntimeProbeLease{}, ErrProbeLease
	}
	output, err := os.OpenFile(state.path, os.O_WRONLY|os.O_CREATE|os.O_EXCL|syscall.O_NOFOLLOW, 0700)
	if err != nil {
		return RuntimeProbeLease{}, ErrProbeLease
	}
	defer output.Close()
	if err := output.Chmod(0700); err != nil {
		return RuntimeProbeLease{}, ErrProbeLease
	}
	digest := sha256.New()
	n, err := copyProbeBytes(ctx, io.MultiWriter(output, digest), source, m.opts.MaxUncompressedBytes)
	if err != nil {
		return RuntimeProbeLease{}, probeCopyError(ctx, err)
	}
	var got [sha256.Size]byte
	copy(got[:], digest.Sum(nil))
	if n != original.Size() || got != want || !probeOriginalUnchanged(source, original, binary, m.opts) {
		return RuntimeProbeLease{}, ErrProbeLease
	}
	state.file, err = output.Stat()
	if err != nil || !state.file.Mode().IsRegular() || state.file.Mode().Perm() != 0700 || state.file.Size() != n {
		return RuntimeProbeLease{}, ErrProbeLease
	}
	if err := output.Close(); err != nil {
		return RuntimeProbeLease{}, ErrProbeLease
	}
	if err := ctx.Err(); err != nil {
		return RuntimeProbeLease{}, err
	}
	m.mu.Lock()
	if m.closed {
		m.mu.Unlock()
		return RuntimeProbeLease{}, ErrClosed
	}
	m.probeLease = state
	m.mu.Unlock()
	published = true
	return RuntimeProbeLease{state: state}, nil
}

// verifiedArtifactDigest is used only immediately after acquireArtifact has
// passed the supplied compressed checksum. It is not a repair path for missing
// provenance, current binaries, or files left in RunDir after manager startup.
func verifiedArtifactDigest(ctx context.Context, opts Options, staged string) ([sha256.Size]byte, error) {
	var digest [sha256.Size]byte
	file, original, err := openProbeOriginal(opts, staged)
	if err != nil {
		return digest, err
	}
	defer file.Close()
	hash := sha256.New()
	n, err := copyProbeBytes(ctx, hash, file, opts.MaxUncompressedBytes)
	if err != nil {
		return digest, err
	}
	if n != original.Size() || !probeOriginalUnchanged(file, original, staged, opts) {
		return digest, ErrProbeLease
	}
	copy(digest[:], hash.Sum(nil))
	return digest, nil
}

// Artifacts created by this manager are direct children of its canonical
// RunDir. Reject all nested paths, ancestor aliases and final symlinks. Open
// without following a last-component symlink; nonblocking open also avoids a
// raced FIFO hanging the lane before fstat rejects it.
func openProbeOriginal(opts Options, path string) (*os.File, os.FileInfo, error) {
	if !filepath.IsAbs(path) || filepath.Clean(path) != path || filepath.Dir(path) != opts.RunDir || !privateProbeRoot(opts.RunDir) {
		return nil, nil, ErrProbeLease
	}
	original, err := os.Lstat(path)
	if err != nil || !original.Mode().IsRegular() || original.Mode().Perm()&0100 == 0 {
		return nil, nil, ErrProbeLease
	}
	if original.Size() > opts.MaxUncompressedBytes {
		return nil, nil, ErrArtifactUncompressedLimit
	}
	if original.Size() <= 0 {
		return nil, nil, ErrProbeLease
	}
	file, err := os.OpenFile(path, os.O_RDONLY|syscall.O_NOFOLLOW|syscall.O_NONBLOCK, 0)
	if err != nil {
		return nil, nil, ErrProbeLease
	}
	opened, err := file.Stat()
	if err != nil || !sameProbeFile(original, opened) {
		file.Close()
		return nil, nil, ErrProbeLease
	}
	return file, original, nil
}

func privateProbeRoot(path string) bool {
	info, err := os.Lstat(path)
	if err != nil || !info.IsDir() || info.Mode().Perm() != 0700 {
		return false
	}
	real, err := filepath.EvalSymlinks(path)
	return err == nil && real == path
}

func sameProbeFile(before, after os.FileInfo) bool {
	return after != nil && after.Mode().IsRegular() && os.SameFile(before, after) && before.Size() == after.Size() && before.Mode() == after.Mode() && before.ModTime().Equal(after.ModTime())
}

func probeOriginalUnchanged(source *os.File, original os.FileInfo, path string, opts Options) bool {
	opened, err := source.Stat()
	if err != nil || !sameProbeFile(original, opened) {
		return false
	}
	current, err := os.Lstat(path)
	return err == nil && sameProbeFile(original, current) && privateProbeRoot(opts.RunDir)
}

// copyProbeBytes cannot use io.Copy's file fast path: read/write and context
// checks must remain bounded. It probes one byte beyond the limit but never
// writes that extra byte. Cancellation between a read and write writes nothing.
func copyProbeBytes(ctx context.Context, output io.Writer, source io.Reader, limit int64) (int64, error) {
	var buffer [32 << 10]byte
	var total int64
	for {
		if err := ctx.Err(); err != nil {
			return total, err
		}
		chunk := buffer[:]
		remaining := limit - total
		if remaining < int64(len(chunk)) {
			chunk = chunk[:remaining+1]
		}
		n, readErr := source.Read(chunk)
		if err := ctx.Err(); err != nil {
			return total, err
		}
		if int64(n) > remaining {
			return total, ErrArtifactUncompressedLimit
		}
		if n > 0 {
			written, err := output.Write(chunk[:n])
			total += int64(written)
			if err != nil {
				return total, err
			}
			if written != n {
				return total, io.ErrShortWrite
			}
		}
		if readErr == io.EOF {
			return total, ctx.Err()
		}
		if readErr != nil {
			return total, readErr
		}
		if n == 0 {
			return total, io.ErrNoProgress
		}
	}
}

func probeCopyError(ctx context.Context, err error) error {
	if ctx.Err() != nil {
		return ctx.Err()
	}
	if errors.Is(err, ErrArtifactUncompressedLimit) {
		return ErrArtifactUncompressedLimit
	}
	return ErrProbeLease
}

func removeProbeCopy(s *probeLeaseState) bool {
	// Refuse replaced directories and files rather than delete another owner's
	// contents. os.Remove, unlike RemoveAll, cannot recurse into unknown files.
	dir, err := os.Lstat(s.dir)
	if os.IsNotExist(err) {
		return true
	}
	if err != nil || !dir.IsDir() || !os.SameFile(s.dirInfo, dir) {
		return false
	}
	file, err := os.Lstat(s.path)
	if err == nil {
		if !file.Mode().IsRegular() || !os.SameFile(s.file, file) || os.Remove(s.path) != nil {
			return false
		}
	} else if !os.IsNotExist(err) {
		return false
	}
	return os.Remove(s.dir) == nil
}

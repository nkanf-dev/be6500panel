package runtime

import (
	"bytes"
	"context"
	"crypto/sha256"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"math/rand"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"syscall"
	"time"
)

type serviceRuntime struct {
	disk   diskState
	state  State
	binary string
	// binarySHA256 is volatile provenance from a verified download, never from boot leftovers.
	binarySHA256   [sha256.Size]byte
	proc           *managedProcess
	desired        bool
	restarts       int
	retryAt        time.Time
	errorCode      string
	restored       bool
	needsRecovery  bool
	restorePending bool
	cleanupPending bool
	epoch          uint64
	cancelWatch    context.CancelFunc
}

// Manager has a single non-blocking mutation lane. Status remains responsive
// while checks and downloads run. Exit supervision uses that same lane.
type Manager struct {
	opts         Options
	mu           sync.Mutex
	services     map[string]*serviceRuntime
	gate         chan struct{}
	ctx          context.Context
	cancel       context.CancelFunc
	closed       bool
	closeOnce    sync.Once
	closeDone    chan struct{}
	closeErr     error
	activeCancel context.CancelFunc
	probeLease   *probeLeaseState // caller owns release after its probe process is reaped
	waitingExits int
	wg           sync.WaitGroup
	lock         *os.File
}

func New(opts Options) (*Manager, error) {
	if opts.DataDir == "" || opts.RunDir == "" {
		return nil, errors.New("DataDir and RunDir are required")
	}
	var err error
	if opts.DataDir, err = filepath.Abs(opts.DataDir); err != nil {
		return nil, err
	}
	if opts.RunDir, err = filepath.Abs(opts.RunDir); err != nil {
		return nil, err
	}
	if pathsOverlap(opts.DataDir, opts.RunDir) {
		return nil, errors.New("persistent and runtime directories must be separate")
	}
	if err = normalizeOptions(&opts); err != nil {
		return nil, err
	}
	if err = privateDir(opts.DataDir); err != nil {
		return nil, err
	}
	if err = privateDir(opts.RunDir); err != nil {
		return nil, err
	}
	// Resolve ancestor symlinks too: lexical separation alone is not sufficient.
	dataReal, dataErr := filepath.EvalSymlinks(opts.DataDir)
	runReal, runErr := filepath.EvalSymlinks(opts.RunDir)
	if dataErr != nil || runErr != nil || pathsOverlap(dataReal, runReal) {
		return nil, errors.New("persistent and runtime directories must be physically separate")
	}
	opts.DataDir, opts.RunDir = dataReal, runReal
	// One manager owns a data store; avoid two independent PID/config controllers.
	lockPath := filepath.Join(opts.DataDir, ".manager.lock")
	if info, statErr := os.Lstat(lockPath); statErr == nil && !info.Mode().IsRegular() {
		return nil, errors.New("invalid manager lock file")
	}
	lock, err := os.OpenFile(lockPath, os.O_CREATE|os.O_RDWR, 0600)
	if err != nil {
		return nil, err
	}
	if err = lock.Chmod(0600); err != nil {
		lock.Close()
		return nil, err
	}
	if err = syscall.Flock(int(lock.Fd()), syscall.LOCK_EX|syscall.LOCK_NB); err != nil {
		lock.Close()
		return nil, errors.New("runtime data directory is already in use")
	}
	ctx, cancel := context.WithCancel(context.Background())
	m := &Manager{opts: opts, services: make(map[string]*serviceRuntime), gate: make(chan struct{}, 1), ctx: ctx, cancel: cancel, closeDone: make(chan struct{}), lock: lock}
	for _, id := range []string{SingBox, FRPC} {
		if err = privateDir(filepath.Join(opts.DataDir, id)); err != nil {
			m.releaseLock()
			cancel()
			return nil, err
		}
		disk, loadErr := loadState(opts, id)
		if loadErr != nil {
			m.releaseLock()
			cancel()
			return nil, fmt.Errorf("%s private state: %w", id, loadErr)
		}
		s := &serviceRuntime{disk: disk, state: NotConfigured}
		if disk.Current != nil {
			s.state = Rebuilding
			s.errorCode = "artifact_missing"
		}
		// RunDir contents are never blindly trusted/executed after a new manager starts.
		// Reacquire a checksum-verified artifact; persistent metadata is only a plan.
		s.cleanupPending = opts.CleanupHook != nil // first Start clears caller-owned boot leftovers
		m.services[id] = s
		if syncErr := stateSync(opts)(filepath.Join(opts.DataDir, id)); syncErr == nil {
			pruneConfigs(opts, id, disk)
		} else {
			s.errorCode = "state_not_durable"
		}
	}
	return m, nil
}
func normalizeOptions(o *Options) error {
	if o.Logger == nil {
		o.Logger = slog.New(slog.NewTextHandler(io.Discard, nil))
	}
	if o.MaxCompressedBytes == 0 {
		o.MaxCompressedBytes = 16 << 20
	}
	if o.MaxUncompressedBytes == 0 {
		o.MaxUncompressedBytes = 40 << 20
	}
	if o.MaxConfigBytes == 0 {
		o.MaxConfigBytes = 1 << 20
	}
	if o.TailBytes == 0 {
		o.TailBytes = 4096
	}
	if o.DownloadTimeout == 0 {
		o.DownloadTimeout = 2 * time.Minute
	}
	if o.CheckTimeout == 0 {
		o.CheckTimeout = 10 * time.Second
	}
	if o.ReadyTimeout == 0 {
		o.ReadyTimeout = 10 * time.Second
	}
	if o.ResourceTimeout == 0 {
		o.ResourceTimeout = 30 * time.Second
	}
	if o.TermGrace == 0 {
		o.TermGrace = 2 * time.Second
	}
	if o.BackoffInitial == 0 {
		o.BackoffInitial = time.Second
	}
	if o.BackoffMax == 0 {
		o.BackoffMax = 30 * time.Second
	}
	if o.StableAfter == 0 {
		o.StableAfter = time.Minute
	}
	if o.MaxRestarts == 0 {
		o.MaxRestarts = 5
	}
	if o.MaxCompressedBytes < 1 || o.MaxCompressedBytes > 64<<20 || o.MaxUncompressedBytes < 1 || o.MaxUncompressedBytes > 128<<20 || o.MaxConfigBytes < 1 || o.MaxConfigBytes > 4<<20 || o.TailBytes < 1 || o.TailBytes > 16<<10 || o.MinFreeRunBytes < 0 || o.MaxRestarts < 1 || o.MaxRestarts > 20 {
		return errors.New("invalid runtime resource limits")
	}
	if o.DownloadTimeout < time.Millisecond || o.DownloadTimeout > 10*time.Minute || o.CheckTimeout < time.Millisecond || o.CheckTimeout > time.Minute || o.ReadyTimeout < time.Millisecond || o.ReadyTimeout > time.Minute || o.ResourceTimeout < time.Millisecond || o.ResourceTimeout > 2*time.Minute || o.TermGrace < time.Millisecond || o.TermGrace > 10*time.Second || o.BackoffInitial < time.Millisecond || o.BackoffMax < o.BackoffInitial || o.BackoffMax > 5*time.Minute || o.StableAfter < time.Millisecond {
		return errors.New("invalid runtime time limits")
	}
	return nil
}
func (m *Manager) releaseLock() {
	if m.lock != nil {
		_ = syscall.Flock(int(m.lock.Fd()), syscall.LOCK_UN)
		_ = m.lock.Close()
		m.lock = nil
	}
}

func pathsOverlap(a, b string) bool {
	inside := func(root, child string) bool {
		rel, err := filepath.Rel(root, child)
		return err != nil || rel == "." || rel != ".." && !strings.HasPrefix(rel, ".."+string(filepath.Separator))
	}
	return inside(a, b) || inside(b, a)
}

func validService(id string) bool { return id == SingBox || id == FRPC }
func (m *Manager) begin(ctx context.Context, id string) (context.Context, func(), error) {
	if !validService(id) {
		return nil, nil, ErrService
	}
	if ctx == nil {
		return nil, nil, errors.New("context is required")
	}
	if err := ctx.Err(); err != nil {
		return nil, nil, err
	}
	select {
	case m.gate <- struct{}{}:
	default:
		return nil, nil, ErrBusy
	}
	opctx, cancel := context.WithCancel(ctx)
	m.mu.Lock()
	closed, exitPending := m.closed, m.waitingExits > 0
	if !closed && !exitPending {
		m.activeCancel = cancel
	}
	m.mu.Unlock()
	if closed || exitPending {
		cancel()
		<-m.gate
		if closed {
			return nil, nil, ErrClosed
		}
		return nil, nil, ErrBusy
	}
	detach := context.AfterFunc(m.ctx, cancel)
	return opctx, func() {
		detach()
		cancel()
		m.mu.Lock()
		m.activeCancel = nil
		m.mu.Unlock()
		<-m.gate
	}, nil
}
func (m *Manager) Status(id string) (Status, error) {
	if !validService(id) {
		return Status{}, ErrService
	}
	m.mu.Lock()
	defer m.mu.Unlock()
	return m.statusLocked(id), nil
}
func (m *Manager) statusLocked(id string) Status {
	s := m.services[id]
	out := Status{Service: id, State: s.state, Generation: s.disk.Generation, Configured: s.disk.Current != nil, ArtifactAvailable: s.binary != "", Desired: s.desired, Restarts: s.restarts, RetryAt: s.retryAt, ErrorCode: s.errorCode, Restored: s.restored, NeedsRecovery: s.needsRecovery || s.cleanupPending || s.restorePending}
	if s.disk.Artifact != nil {
		out.Version = s.disk.Artifact.Version
	}
	if s.proc != nil {
		s.proc.signalMu.Lock()
		if !s.proc.exited {
			out.PID = s.proc.cmd.Process.Pid
			out.RSSBytes, out.RSSAvailable = processRSS(out.PID)
		}
		s.proc.signalMu.Unlock()
	}
	if s.binary == "" {
		out.RecoveryPlan = append(out.RecoveryPlan, "acquire_verified_artifact")
	}
	if s.disk.Current == nil {
		out.RecoveryPlan = append(out.RecoveryPlan, "configure_and_verify")
	}
	if s.errorCode == "restart_limit" {
		out.RecoveryPlan = append(out.RecoveryPlan, "inspect_core_and_start_explicitly")
	}
	if s.errorCode == "readiness_failed" {
		out.RecoveryPlan = append(out.RecoveryPlan, "inspect_local_listeners_and_start_explicitly")
	}
	if s.cleanupPending {
		out.RecoveryPlan = append(out.RecoveryPlan, "retry_owned_resource_cleanup")
	}
	if s.errorCode == "state_not_durable" {
		out.RecoveryPlan = append(out.RecoveryPlan, "check_persistent_storage", "stop_then_start_to_apply_accepted_config")
	}
	if s.restorePending {
		out.RecoveryPlan = append(out.RecoveryPlan, "retry_owned_resource_restore")
	}
	if s.needsRecovery {
		out.RecoveryPlan = append(out.RecoveryPlan, "inspect_runtime_and_start_explicitly")
	}
	if s.disk.LastGood != nil && !s.restored && (s.state == Error || s.errorCode != "") {
		out.RecoveryPlan = append(out.RecoveryPlan, "restore_last_good_config")
	}
	return out
}

// checkMutationGuard runs only after admission and before any mutation work.
// A successful guard may cancel the operation, so check context again afterward.
func checkMutationGuard(ctx context.Context, guard func(context.Context) error) error {
	if guard != nil {
		if err := guard(ctx); err != nil {
			return err
		}
	}
	return ctx.Err()
}

func (m *Manager) result(id string, err error) (Status, error) { s, _ := m.Status(id); return s, err }
func (m *Manager) setState(id string, state State, code string) {
	m.mu.Lock()
	s := m.services[id]
	s.state = state
	s.errorCode = code
	if state == Checking || state == Downloading || state == Starting {
		s.restored = false
	}
	m.mu.Unlock()
	m.opts.Logger.Debug("managed runtime transition", "service", id, "state", state, "code", code)
}

// liveSnapshot contains immutable checked records. Ready is set only after a
// successful start, so a check-only candidate is never an automatic fallback.
type liveSnapshot struct {
	disk         diskState
	binary       string
	binarySHA256 [sha256.Size]byte
	desired      bool
	ready        *configRecord
	admitted     *Options // holds candidate + prior ready scratch for the entire change
}

// admitChange reserves recovery scratch before any candidate write. A smaller
// new config cannot consume the space needed to copy a larger ready snapshot.
func (m *Manager) admitChange(ctx context.Context, id string, raw []byte, rollback bool, live *liveSnapshot) (func(), Options, error) {
	opts := m.opts
	if rollback {
		opts.MaxConfigBytes = storedConfigLimit(opts)
	}
	release, opts, err := admitConfig(opts, ctx, id, raw, rollback)
	if err != nil {
		return nil, opts, err
	}
	if opts.StorageAdmission != nil && live.desired && live.ready != nil {
		info, err := os.Lstat(configPath(opts, id, live.ready))
		if err != nil || !info.Mode().IsRegular() {
			release()
			return nil, opts, errors.New("previous ready config is unavailable")
		}
		// Normal reservation holds two candidate bodies; rollback holds one.
		heldConfig := int64(len(raw))
		if !rollback {
			heldConfig *= 2
		}
		needed := int64(len(raw)) + info.Size()
		if needed > heldConfig {
			extraRelease, err := opts.StorageAdmission(ctx, filepath.Join(opts.DataDir, id), needed-heldConfig, rollback)
			if err != nil {
				release()
				return nil, opts, err
			}
			firstRelease := release
			release = func() { extraRelease(); firstRelease() }
		}
		live.admitted = &opts
	}
	return release, opts, nil
}

func snapshotRuntime(s *serviceRuntime) liveSnapshot {
	previous := liveSnapshot{disk: s.disk, binary: s.binary, binarySHA256: s.binarySHA256, desired: s.desired}
	if s.disk.Current != nil && s.disk.Current.Ready {
		previous.ready = s.disk.Current
	} else if s.disk.LastGood != nil && s.disk.LastGood.Ready {
		previous.ready = s.disk.LastGood
	}
	return previous
}

// restartChange applies a live change, then restores a proven-ready snapshot if
// activation fails. Recovery stays on the same lane and uses manager lifetime:
// a canceled HTTP request must not strand an already accepted working service.
func (m *Manager) restartChange(ctx context.Context, id string, previous liveSnapshot) error {
	err := m.stopProcess(id, false)
	if err == nil {
		err = m.startProcess(ctx, id)
	}
	if err == nil {
		return nil
	}
	m.mu.Lock()
	s := m.services[id]
	originalCode := s.errorCode
	if originalCode == "" {
		originalCode = "operation_cancelled"
	}
	s.needsRecovery = true
	m.mu.Unlock()
	if !previous.desired || previous.ready == nil || previous.binary == "" {
		m.mu.Lock()
		s.desired = false
		s.errorCode = originalCode
		m.mu.Unlock()
		return err
	}
	recoveryCtx, cancel := context.WithTimeout(m.ctx, m.opts.CheckTimeout+m.opts.ReadyTimeout+2*m.opts.ResourceTimeout+2*m.opts.TermGrace)
	defer cancel()
	m.mu.Lock()
	m.activeCancel = cancel // another core exit can still preempt owned recovery
	m.mu.Unlock()
	recoveryErr := m.recoverRuntime(recoveryCtx, id, previous)
	m.mu.Lock()
	s.errorCode = originalCode // never hide the failed candidate's safe diagnostic
	s.needsRecovery = recoveryErr != nil
	m.mu.Unlock()
	if recoveryErr != nil {
		return errors.Join(err, ErrRecovery, recoveryErr)
	}
	return err // automatic recovery is not success of the requested change
}

func (m *Manager) recoverRuntime(ctx context.Context, id string, previous liveSnapshot) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	// Read the immutable proven-ready bytes, not a newly checked LastGood alias.
	opts := m.opts
	opts.MaxConfigBytes = storedConfigLimit(opts)
	raw, err := readBounded(configPath(opts, id, previous.ready), opts.MaxConfigBytes)
	if err != nil {
		return errors.New("previous ready config is unavailable")
	}
	if previous.admitted != nil {
		opts = *previous.admitted
		opts.MaxConfigBytes = storedConfigLimit(opts)
		opts.storageContext = ctx
		opts.storageRollback = true
	} else {
		release, admittedOpts, err := admitConfig(opts, ctx, id, raw, true)
		if err != nil {
			return err
		}
		defer release()
		opts = admittedOpts
	}
	candidate, err := stageConfig(opts, id, raw)
	if err != nil {
		return errors.New("cannot stage previous ready config")
	}
	defer os.Remove(candidate)
	if err := verify(ctx, previous.binary, id, candidate, opts); err != nil {
		return err
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	m.mu.Lock()
	s := m.services[id]
	disk := s.disk // rollback always advances the latest committed generation
	cleanupPending := s.cleanupPending
	processLive := s.proc != nil
	m.mu.Unlock()
	// Restore artifact metadata and config in the same manifest transaction.
	// The candidate's artifact must not survive a successful executable rollback.
	disk.Artifact = previous.disk.Artifact
	next, persistErr := commitConfig(opts, id, disk, candidate, raw)
	if persistErr != nil && !errors.Is(persistErr, ErrDurability) {
		return errors.New("previous ready config persistence failed")
	}
	// A running candidate must stop before the executable/config identity changes.
	// Failed cleanup is not retried or disguised as successful resource recovery.
	if !cleanupPending && processLive {
		if err := m.stopProcess(id, false); err != nil {
			cleanupPending = true
		}
	}
	m.mu.Lock()
	s.disk = next
	s.binary = previous.binary
	s.binarySHA256 = previous.binarySHA256
	if persistErr == nil {
		pruneConfigs(opts, id, next)
	}
	m.mu.Unlock()
	if persistErr != nil {
		return persistErr
	}
	if cleanupPending {
		return errors.New("owned resource cleanup failed")
	}
	m.mu.Lock()
	s.desired = true
	s.restarts = 0
	m.mu.Unlock()
	if err := m.startProcess(ctx, id); err != nil {
		return err
	}
	m.mu.Lock()
	s.restored = true
	resourcesFailed := s.restorePending
	m.mu.Unlock()
	if resourcesFailed {
		return errors.New("owned resource restore failed")
	}
	return nil
}

// Acquire stages and verifies an artifact before committing metadata and
// activating it. An existing accepted config is checked with the new artifact.
// A failed download/check never stops the previous running core.
func (m *Manager) Acquire(ctx context.Context, id string, artifact Artifact) (Status, error) {
	return m.AcquireGuarded(ctx, id, artifact, nil)
}

// AcquireGuarded checks caller-owned state inside the shared mutation lane before
// artifact acquisition. The guard may read status but must not mutate the manager.
func (m *Manager) AcquireGuarded(ctx context.Context, id string, artifact Artifact, guard func(context.Context) error) (Status, error) {
	ctx, done, err := m.begin(ctx, id)
	if err != nil {
		return m.resultSafe(id, err)
	}
	defer done()
	if err = checkMutationGuard(ctx, guard); err != nil {
		return m.result(id, err)
	}
	m.mu.Lock()
	s := m.services[id]
	previous := s.state
	disk := s.disk
	live := snapshotRuntime(s)
	m.mu.Unlock()
	if len(artifact.Version) > 128 || strings.ContainsAny(artifact.Version, "\r\n\x00") {
		return m.result(id, errors.New("invalid artifact version"))
	}
	if len(artifact.URL) == 0 || len(artifact.URL) > 4096 {
		return m.result(id, errors.New("invalid artifact URL size"))
	}
	if err = checkRunSpace(m.opts); err != nil {
		return m.result(id, err)
	}
	m.setState(id, Downloading, "")
	downloadCtx, cancel := context.WithTimeout(ctx, m.opts.DownloadTimeout)
	staged, err := acquireArtifact(downloadCtx, m.opts, id, artifact)
	cancel()
	if err != nil {
		code := "artifact_acquire_failed"
		var publicError error = errors.New("artifact acquisition failed")
		switch {
		case errors.Is(err, ErrArtifactCompressedLimit):
			code, publicError = "artifact_compressed_limit", ErrArtifactCompressedLimit
		case errors.Is(err, ErrArtifactUncompressedLimit):
			code, publicError = "artifact_uncompressed_limit", ErrArtifactUncompressedLimit
		}
		m.setState(id, previous, code)
		if ctx.Err() != nil {
			return m.result(id, ctx.Err())
		}
		return m.result(id, publicError)
	}
	activated := false
	defer func() {
		if !activated {
			_ = os.Remove(staged)
		}
	}()
	// acquireArtifact has verified the fetched (possibly compressed) checksum.
	// Record extracted bytes now, before any verifier or activation can use them.
	// Never derive original trust by hashing an arbitrary current executable.
	binarySHA256, err := verifiedArtifactDigest(ctx, m.opts, staged)
	if err != nil {
		m.setState(id, previous, "artifact_acquire_failed")
		if ctx.Err() != nil {
			return m.result(id, ctx.Err())
		}
		return m.result(id, errors.New("artifact acquisition failed"))
	}
	if disk.Current != nil {
		m.setState(id, Checking, "")
		if err = m.verifyAccepted(ctx, id, staged, disk.Current); err != nil {
			m.setState(id, previous, "artifact_check_failed")
			return m.result(id, err)
		}
	}
	if err = ctx.Err(); err != nil {
		m.setState(id, previous, "operation_cancelled")
		return m.result(id, err)
	}
	// Reserve rollback scratch before artifact activation can stop a working core.
	if live.desired && live.ready != nil && m.opts.StorageAdmission != nil {
		raw, err := readBounded(configPath(m.opts, id, live.ready), storedConfigLimit(m.opts))
		if err != nil {
			return m.result(id, errors.New("previous ready config is unavailable"))
		}
		release, _, err := m.admitChange(ctx, id, raw, false, &live)
		if err != nil {
			m.setState(id, previous, "storage_insufficient")
			return m.result(id, err)
		}
		defer release()
	}
	// The executable has a unique private RunDir path. Persist metadata first;
	// an I/O failure cannot overwrite the previous process's executable.
	next := disk
	next.Artifact = &artifact
	persistErr := saveState(m.opts, id, next)
	if persistErr != nil && !errors.Is(persistErr, ErrDurability) {
		m.setState(id, previous, "artifact_state_failed")
		return m.result(id, errors.New("artifact metadata persistence failed"))
	}
	m.mu.Lock()
	oldBinary := s.binary
	s.disk = next
	s.binary = staged
	s.binarySHA256 = binarySHA256
	shouldRun := s.desired
	m.mu.Unlock()
	activated = true
	defer func() {
		m.mu.Lock()
		activeBinary := s.binary
		m.mu.Unlock()
		for _, binary := range []string{oldBinary, staged} {
			if binary != "" && binary != activeBinary {
				_ = os.Remove(binary)
			}
		}
	}()
	if persistErr != nil {
		m.setState(id, previous, "state_not_durable")
		return m.result(id, persistErr)
	}
	if shouldRun {
		return m.result(id, m.restartChange(ctx, id, live))
	}
	state := NotConfigured
	if disk.Current != nil {
		state = Stopped
	}
	m.setState(id, state, "")
	return m.result(id, nil)
}
func (m *Manager) resultSafe(id string, err error) (Status, error) {
	if !validService(id) {
		return Status{}, err
	}
	return m.result(id, err)
}

// Configure uses compare-and-swap generation. Failed checks preserve the old
// manifest, generation, process and config. Accepted changes increment once.
func (m *Manager) Configure(ctx context.Context, id string, raw []byte, expectedGeneration uint64) (Status, error) {
	return m.ConfigureGuarded(ctx, id, raw, expectedGeneration, nil)
}

// ConfigureGuarded checks caller-owned state inside the shared mutation lane before
// configuration staging. The guard may read status but must not mutate the manager.
func (m *Manager) ConfigureGuarded(ctx context.Context, id string, raw []byte, expectedGeneration uint64, guard func(context.Context) error) (Status, error) {
	ctx, done, err := m.begin(ctx, id)
	if err != nil {
		return m.resultSafe(id, err)
	}
	defer done()
	if err = checkMutationGuard(ctx, guard); err != nil {
		return m.result(id, err)
	}
	if len(raw) == 0 || int64(len(raw)) > m.opts.MaxConfigBytes {
		return m.result(id, errors.New("invalid configuration size"))
	}
	// Own a copy so caller reuse cannot change bytes after verification.
	raw = append([]byte(nil), raw...)
	m.mu.Lock()
	s := m.services[id]
	disk := s.disk
	binary := s.binary
	previous := s.state
	live := snapshotRuntime(s)
	m.mu.Unlock()
	if disk.Generation != expectedGeneration {
		return m.result(id, ErrGeneration)
	}
	if binary == "" {
		return m.result(id, ErrNoArtifact)
	}
	release, operationOpts, err := m.admitChange(ctx, id, raw, false, &live)
	if err != nil {
		m.setState(id, previous, "storage_insufficient")
		return m.result(id, err)
	}
	defer release()
	candidate, err := stageConfig(operationOpts, id, raw)
	if err != nil {
		return m.result(id, errors.New("cannot stage private config"))
	}
	defer os.Remove(candidate)
	m.setState(id, Checking, "")
	if err = verify(ctx, binary, id, candidate, m.opts); err != nil {
		m.setState(id, previous, "config_check_failed")
		return m.result(id, err)
	}
	if err = ctx.Err(); err != nil {
		m.setState(id, previous, "operation_cancelled")
		return m.result(id, err)
	}
	next, err := commitConfig(operationOpts, id, disk, candidate, raw)
	if err != nil && !errors.Is(err, ErrDurability) {
		m.setState(id, previous, "config_commit_failed")
		return m.result(id, errors.New("private config commit failed"))
	}
	m.mu.Lock()
	s.disk = next
	if err == nil {
		pruneConfigs(m.opts, id, next)
	}
	shouldRun := s.desired
	m.mu.Unlock()
	if err != nil {
		m.setState(id, previous, "state_not_durable")
		return m.result(id, err)
	}
	if shouldRun {
		return m.result(id, m.restartChange(ctx, id, live))
	}
	m.setState(id, Stopped, "")
	return m.result(id, nil)
}

// Restore verifies and accepts the last-good private config as a new generation.
// It never moves generation backwards, so stale clients cannot overwrite it.
func (m *Manager) Restore(ctx context.Context, id string, expectedGeneration uint64) (Status, error) {
	return m.RestoreGuarded(ctx, id, expectedGeneration, nil)
}

// RestoreGuarded checks caller-owned state inside the shared mutation lane before
// last-good configuration reads. The guard may read status but must not mutate
// the manager.
func (m *Manager) RestoreGuarded(ctx context.Context, id string, expectedGeneration uint64, guard func(context.Context) error) (Status, error) {
	ctx, done, err := m.begin(ctx, id)
	if err != nil {
		return m.resultSafe(id, err)
	}
	defer done()
	if err = checkMutationGuard(ctx, guard); err != nil {
		return m.result(id, err)
	}
	m.mu.Lock()
	s := m.services[id]
	disk := s.disk
	binary := s.binary
	previous := s.state
	live := snapshotRuntime(s)
	m.mu.Unlock()
	if disk.Generation != expectedGeneration {
		return m.result(id, ErrGeneration)
	}
	if disk.LastGood == nil {
		return m.result(id, errors.New("last-good config is unavailable"))
	}
	if binary == "" {
		return m.result(id, ErrNoArtifact)
	}
	raw, err := readBounded(configPath(m.opts, id, disk.LastGood), storedConfigLimit(m.opts))
	if err != nil {
		return m.result(id, errors.New("last-good config is unavailable"))
	}
	release, operationOpts, err := m.admitChange(ctx, id, raw, true, &live)
	if err != nil {
		m.setState(id, previous, "storage_insufficient")
		return m.result(id, err)
	}
	defer release()
	candidate, err := stageConfig(operationOpts, id, raw)
	if err != nil {
		return m.result(id, err)
	}
	defer os.Remove(candidate)
	m.setState(id, Checking, "")
	if err = verify(ctx, binary, id, candidate, m.opts); err != nil {
		m.setState(id, previous, "config_check_failed")
		return m.result(id, err)
	}
	if err = ctx.Err(); err != nil {
		m.setState(id, previous, "operation_cancelled")
		return m.result(id, err)
	}
	next, err := commitConfig(operationOpts, id, disk, candidate, raw)
	if err != nil && !errors.Is(err, ErrDurability) {
		m.setState(id, previous, "config_commit_failed")
		return m.result(id, errors.New("private config restore failed"))
	}
	m.mu.Lock()
	s.disk = next
	if err == nil {
		pruneConfigs(m.opts, id, next)
	}
	shouldRun := s.desired
	m.mu.Unlock()
	if err != nil {
		m.setState(id, previous, "state_not_durable")
		return m.result(id, err)
	}
	if shouldRun {
		return m.result(id, m.restartChange(ctx, id, live))
	}
	m.setState(id, Stopped, "")
	return m.result(id, nil)
}

func (m *Manager) Start(ctx context.Context, id string) (Status, error) {
	return m.StartGuarded(ctx, id, nil)
}

// StartGuarded checks caller-owned state inside the shared mutation lane before
// verification or process and resource changes. The guard may read status but
// must not mutate the manager.
func (m *Manager) StartGuarded(ctx context.Context, id string, guard func(context.Context) error) (Status, error) {
	ctx, done, err := m.begin(ctx, id)
	if err != nil {
		return m.resultSafe(id, err)
	}
	defer done()
	if err = checkMutationGuard(ctx, guard); err != nil {
		return m.result(id, err)
	}
	m.mu.Lock()
	s := m.services[id]
	exitedPrevious := s.cleanupPending
	if s.proc != nil {
		select {
		case <-s.proc.done:
			exitedPrevious = true
		default:
			m.mu.Unlock()
			if err := m.checkReady(ctx, id); err != nil {
				return m.result(id, err)
			}
			m.restoreResources(ctx, id)
			m.mu.Lock()
			s.needsRecovery = s.restorePending
			m.mu.Unlock()
			return m.result(id, nil)
		}
	}
	m.mu.Unlock()
	if exitedPrevious {
		// The old watcher may still be waiting for this lane. Perform its cleanup
		// before replacing its identity; otherwise stale resources could survive.
		if err = m.stopProcess(id, false); err != nil {
			return m.result(id, err)
		}
	}
	m.mu.Lock()
	if s.disk.Current == nil {
		m.mu.Unlock()
		return m.result(id, ErrNotConfigured)
	}
	if s.binary == "" {
		m.mu.Unlock()
		return m.result(id, ErrNoArtifact)
	}
	binary := s.binary
	record := s.disk.Current
	previous := s.state
	m.mu.Unlock()
	m.setState(id, Checking, "")
	if err = m.verifyAccepted(ctx, id, binary, record); err != nil {
		m.setState(id, previous, "config_check_failed")
		return m.result(id, err)
	}
	if err = ctx.Err(); err != nil {
		m.setState(id, previous, "operation_cancelled")
		return m.result(id, err)
	}
	m.mu.Lock()
	s.desired = true
	s.restarts = 0
	s.epoch++
	m.mu.Unlock()
	err = m.startProcess(ctx, id)
	return m.result(id, err)
}

// Restart verifies the current accepted config before replacing a process.
// Unlike Start it always stops the old process and owned resources. The entire
// preflight/stop/start sequence owns one mutation lane, so no configuration or
// retry operation can interleave between the checked record and its activation.
func (m *Manager) Restart(ctx context.Context, id string) (Status, error) {
	return m.RestartGuarded(ctx, id, nil)
}

// RestartGuarded checks caller-owned configuration state after acquiring the
// shared lane and before changing a process or even starting verification. The
// guard must not call manager mutations; read-only status checks are safe.
func (m *Manager) RestartGuarded(ctx context.Context, id string, guard func(context.Context) error) (Status, error) {
	ctx, done, err := m.begin(ctx, id)
	if err != nil {
		return m.resultSafe(id, err)
	}
	defer done()
	if err = checkMutationGuard(ctx, guard); err != nil {
		return m.result(id, err)
	}
	m.mu.Lock()
	s := m.services[id]
	if s.disk.Current == nil {
		m.mu.Unlock()
		return m.result(id, ErrNotConfigured)
	}
	if s.binary == "" {
		m.mu.Unlock()
		return m.result(id, ErrNoArtifact)
	}
	binary, record, previous := s.binary, s.disk.Current, s.state
	m.mu.Unlock()
	m.setState(id, Checking, "")
	if err = m.verifyAccepted(ctx, id, binary, record); err != nil {
		code := "config_check_failed"
		if ctx.Err() != nil {
			code = "operation_cancelled"
		}
		m.setState(id, previous, code)
		return m.result(id, err)
	}
	if err = ctx.Err(); err != nil {
		m.setState(id, previous, "operation_cancelled")
		return m.result(id, err)
	}
	// Disable desire and cancel the old epoch first. A canceled request after
	// cleanup must not leave a backoff watcher able to launch a replacement.
	if err = m.stopProcess(id, true); err != nil {
		return m.result(id, err)
	}
	if err = ctx.Err(); err != nil {
		m.setState(id, Stopped, "operation_cancelled")
		return m.result(id, err)
	}
	m.mu.Lock()
	s.desired = true
	s.restarts = 0
	s.retryAt = time.Time{}
	s.epoch++
	m.mu.Unlock()
	err = m.startProcess(ctx, id)
	if err != nil {
		// Cancellation can arrive between the preceding check and startProcess's
		// first check. Preserve actual stopped/error state, with no desired retry.
		m.mu.Lock()
		if s.proc == nil {
			s.desired = false
			s.retryAt = time.Time{}
			if s.errorCode == "" && ctx.Err() != nil {
				s.errorCode = "operation_cancelled"
			}
		}
		m.mu.Unlock()
	}
	return m.result(id, err)
}

func (m *Manager) Stop(ctx context.Context, id string) (Status, error) {
	_, done, err := m.begin(ctx, id)
	if err != nil {
		return m.resultSafe(id, err)
	}
	defer done()
	err = m.stopProcess(id, true)
	return m.result(id, err)
}

// markReady persists copy-on-write proof. Checking a candidate is not proof of
// local readiness, and record aliases retained in an old snapshot must not change.
func (m *Manager) markReady(id string) error {
	m.mu.Lock()
	s := m.services[id]
	next := s.disk
	if next.Current.Ready {
		m.mu.Unlock()
		return nil
	}
	record := *next.Current
	record.Ready = true
	next.Current = &record
	m.mu.Unlock()
	err := saveState(m.opts, id, next)
	if err == nil || errors.Is(err, ErrDurability) {
		m.mu.Lock()
		s.disk = next
		m.mu.Unlock()
	}
	return err
}

func (m *Manager) startProcess(ctx context.Context, id string) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	m.mu.Lock()
	s := m.services[id]
	if m.closed {
		m.mu.Unlock()
		return ErrClosed
	}
	binary := s.binary
	path := configPath(m.opts, id, s.disk.Current)
	epoch := s.epoch
	s.state = Starting
	s.errorCode = ""
	s.retryAt = time.Time{}
	m.mu.Unlock()
	args := []string{"run", "-c", path}
	if id == FRPC {
		args = []string{"-c", path}
	}
	p, err := launch(binary, args, m.opts)
	if err != nil {
		m.mu.Lock()
		s.state = Error
		s.errorCode = "start_failed"
		s.desired = false
		m.mu.Unlock()
		return errors.New("managed process start failed")
	}
	m.mu.Lock()
	s.proc = p // Status exposes starting/PID while the local readiness hook waits.
	m.mu.Unlock()
	if m.opts.ReadyHook != nil {
		readyCtx, cancelReady := context.WithTimeout(ctx, m.opts.ReadyTimeout)
		// A leader exit wakes a cooperative hook immediately rather than waiting
		// for ReadyTimeout. The monitor itself ends when the hook returns.
		monitorDone := make(chan struct{})
		go func() {
			defer close(monitorDone)
			select {
			case <-p.done:
				cancelReady()
			case <-readyCtx.Done():
			}
		}()
		hookErr := m.opts.ReadyHook(readyCtx, id)
		readyErr := readyCtx.Err()
		cancelReady()
		<-monitorDone
		p.signalMu.Lock()
		exited := p.exited
		p.signalMu.Unlock()
		if hookErr != nil || readyErr != nil || exited {
			// Keep the readiness/cancellation error even when cleanup also fails.
			// Never forward hook messages or configuration details.
			var failed error = ErrReadiness
			if ctx.Err() != nil {
				failed = ctx.Err()
			}
			cleanupErr := m.stopProcess(id, true)
			if cleanupErr == nil {
				m.setState(id, Error, "readiness_failed")
			}
			return errors.Join(failed, cleanupErr)
		}
	}
	// Cancellation may arrive during exec even when no local readiness hook is
	// configured (for example FRPC with no listener). Never leave that start live.
	if err := ctx.Err(); err != nil {
		cleanupErr := m.stopProcess(id, true)
		if cleanupErr == nil {
			m.setState(id, Error, "readiness_failed")
		}
		return errors.Join(err, cleanupErr)
	}
	if err := m.markReady(id); err != nil {
		cleanupErr := m.stopProcess(id, true)
		if cleanupErr == nil {
			m.setState(id, Error, "state_not_durable")
		}
		if errors.Is(err, ErrDurability) {
			return errors.Join(ErrDurability, cleanupErr)
		}
		return errors.Join(errors.New("ready config proof persistence failed"), cleanupErr)
	}
	m.restoreResources(ctx, id)
	if err := ctx.Err(); err != nil {
		return errors.Join(err, m.stopProcess(id, true))
	}
	watchCtx, cancelWatch := context.WithCancel(m.ctx)
	m.mu.Lock()
	if s.cancelWatch != nil {
		s.cancelWatch()
	}
	s.cancelWatch = cancelWatch
	s.state = Running
	s.needsRecovery = false
	m.mu.Unlock()
	m.wg.Add(1)
	go m.watch(watchCtx, id, p, epoch)
	return nil
}
func (m *Manager) stopProcess(id string, disable bool) error {
	m.mu.Lock()
	s := m.services[id]
	p := s.proc
	if s.cancelWatch != nil {
		s.cancelWatch()
		s.cancelWatch = nil
	}
	s.proc = nil
	s.epoch++
	s.retryAt = time.Time{}
	if disable {
		s.desired = false
		s.restarts = 0
		s.restored = false
	}
	m.mu.Unlock()
	if p != nil {
		p.terminate(m.opts.TermGrace)
	}
	if err := m.cleanup(id); err != nil {
		m.mu.Lock()
		s.state = Error
		s.errorCode = "cleanup_failed"
		s.desired = false
		m.mu.Unlock()
		return err
	}
	state := Stopped
	m.mu.Lock()
	if s.disk.Current == nil {
		state = NotConfigured
	}
	if s.disk.Current != nil && s.binary == "" {
		state = Rebuilding
	}
	s.state = state
	s.errorCode = ""
	if disable {
		s.needsRecovery = false
	}
	m.mu.Unlock()
	return nil
}
func (m *Manager) cleanup(id string) error {
	var err error
	if m.opts.CleanupHook != nil {
		ctx, cancel := context.WithTimeout(context.Background(), m.opts.ResourceTimeout)
		err = m.opts.CleanupHook(ctx, id)
		cancel()
	}
	m.mu.Lock()
	m.services[id].cleanupPending = err != nil
	m.mu.Unlock()
	if err != nil {
		return errors.New("owned resource cleanup failed")
	}
	return nil
}

func (m *Manager) watch(ctx context.Context, id string, p *managedProcess, epoch uint64) {
	defer m.wg.Done()
	select {
	case <-p.done:
	case <-ctx.Done():
		return
	}
	// A core exit must not wait behind another service's slow download/check:
	// cancel that bounded operation, then reserve priority on the same lane.
	m.mu.Lock()
	s := m.services[id]
	if m.closed || s.proc != p || s.epoch != epoch || !s.desired {
		m.mu.Unlock()
		return
	}
	m.waitingExits++
	cancelOperation := m.activeCancel
	m.mu.Unlock()
	if cancelOperation != nil {
		cancelOperation()
	}
	select {
	case m.gate <- struct{}{}:
	case <-ctx.Done():
		m.mu.Lock()
		m.waitingExits--
		m.mu.Unlock()
		return
	}
	m.mu.Lock()
	m.waitingExits--
	if m.closed || s.proc != p || s.epoch != epoch || !s.desired {
		m.mu.Unlock()
		<-m.gate
		return
	}
	s.proc = nil
	if p.exitedAt.Sub(p.started) >= m.opts.StableAfter {
		s.restarts = 0
	}
	s.restarts++
	m.mu.Unlock()
	if err := m.cleanup(id); err != nil {
		m.mu.Lock()
		s.state = Error
		s.errorCode = "cleanup_failed"
		s.desired = false
		m.mu.Unlock()
		<-m.gate
		return
	}
	m.mu.Lock()
	if s.restarts > m.opts.MaxRestarts {
		s.state = Error
		s.errorCode = "restart_limit"
		s.desired = false
		m.mu.Unlock()
		<-m.gate
		return
	}
	delay := m.opts.BackoffInitial
	for n := 1; n < s.restarts && delay < m.opts.BackoffMax; n++ {
		if delay > m.opts.BackoffMax/2 {
			delay = m.opts.BackoffMax
			break
		}
		delay *= 2
	}
	// Downward jitter stays bounded by BackoffMax even at the cap.
	delay = delay - delay/5 + time.Duration(rand.Int63n(int64(delay/5)+1))
	s.state = Backoff
	s.errorCode = "process_exited"
	s.retryAt = time.Now().Add(delay)
	m.mu.Unlock()
	<-m.gate
	timer := time.NewTimer(delay)
	defer timer.Stop()
	select {
	case <-timer.C:
	case <-ctx.Done():
		return
	}
	select {
	case m.gate <- struct{}{}:
	case <-ctx.Done():
		return
	}
	defer func() { <-m.gate }()
	m.mu.Lock()
	allowed := !m.closed && s.desired && s.epoch == epoch && s.proc == nil
	m.mu.Unlock()
	if allowed {
		// Restarts use manager lifetime, not a completed HTTP request context.
		// They remain preemptible by another core's unexpected exit.
		restartCtx, cancelRestart := context.WithCancel(m.ctx)
		m.mu.Lock()
		m.activeCancel = cancelRestart
		m.mu.Unlock()
		_ = m.startProcess(restartCtx, id)
		cancelRestart()
		m.mu.Lock()
		m.activeCancel = nil
		m.mu.Unlock()
	}
}

// Close cancels downloads/checks/retries, waits for the active operation, stops
// and reaps both process groups, then releases the private store lock. Probe
// leases remain caller-owned: close/reap the probe owner before runtime Close,
// and call each lease's Release after its last process is reaped.
func (m *Manager) Close() error {
	m.closeOnce.Do(func() {
		m.mu.Lock()
		m.closed = true
		m.mu.Unlock()
		m.cancel()
		m.gate <- struct{}{}
		for _, id := range []string{SingBox, FRPC} {
			if err := m.stopProcess(id, true); err != nil {
				m.closeErr = errors.Join(m.closeErr, err)
			}
		}
		<-m.gate
		m.wg.Wait()
		// Artifacts are volatile; release their RAM only after every process reaps.
		m.mu.Lock()
		for _, s := range m.services {
			if s.binary != "" {
				_ = os.Remove(s.binary)
				s.binary = ""
			}
			s.binarySHA256 = [sha256.Size]byte{}
		}
		m.mu.Unlock()
		m.releaseLock()
		close(m.closeDone)
	})
	<-m.closeDone
	return m.closeErr
}

// Config returns a copy of the private accepted configuration and its current
// CAS generation. Callers must authenticate access and must not log this body.
func (m *Manager) Config(id string) ([]byte, uint64, error) {
	if !validService(id) {
		return nil, 0, ErrService
	}
	m.mu.Lock()
	defer m.mu.Unlock()
	s := m.services[id]
	if m.closed {
		return nil, s.disk.Generation, ErrClosed
	}
	if s.disk.Current == nil {
		return nil, s.disk.Generation, ErrNotConfigured
	}
	raw, err := readBounded(configPath(m.opts, id, s.disk.Current), storedConfigLimit(m.opts))
	if err != nil {
		return nil, s.disk.Generation, errors.New("accepted private config is unavailable")
	}
	return raw, s.disk.Generation, nil
}

// Check a copy so a checker cannot accidentally rewrite the accepted file.
func (m *Manager) verifyAccepted(ctx context.Context, id, binary string, record *configRecord) error {
	raw, err := readBounded(configPath(m.opts, id, record), storedConfigLimit(m.opts))
	if err != nil {
		return errors.New("accepted private config is unavailable")
	}
	checkOpts := m.opts
	checkOpts.MaxConfigBytes = storedConfigLimit(checkOpts)
	checkOpts.storageRollback = true
	candidate, err := stageConfig(checkOpts, id, raw)
	if err != nil {
		return errors.New("cannot stage private verifier copy")
	}
	defer os.Remove(candidate)
	if err = verify(ctx, binary, id, candidate, m.opts); err != nil {
		return err
	}
	checked, err := readBounded(candidate, storedConfigLimit(m.opts))
	if err != nil || !bytes.Equal(checked, raw) {
		return errors.New("verifier modified private candidate")
	}
	return nil
}

// ReadyOperation serializes a device-scope mutation with starts, stops and
// configuration commits. It cannot activate resources against a starting core.
func (m *Manager) ReadyOperation(ctx context.Context, id string, operation func(context.Context) error) error {
	ctx, done, err := m.begin(ctx, id)
	if err != nil {
		return err
	}
	defer done()
	ctx, cancel := context.WithTimeout(ctx, m.opts.ResourceTimeout)
	defer cancel()
	m.mu.Lock()
	s := m.services[id]
	ready := s.state == Running && s.proc != nil
	if ready {
		select {
		case <-s.proc.done:
			ready = false
		default:
		}
	}
	m.mu.Unlock()
	if !ready {
		return ErrReadiness
	}
	if err := m.checkReady(ctx, id); err != nil {
		return err
	}
	err = operation(ctx)
	m.mu.Lock()
	process := m.services[id].proc
	m.mu.Unlock()
	if process != nil {
		select {
		case <-process.done:
			err = errors.Join(err, ErrReadiness, m.cleanup(id))
		default:
		}
	}
	if err == nil && ctx.Err() != nil {
		return ctx.Err()
	}
	return err
}
func (m *Manager) restoreResources(ctx context.Context, id string) {
	if m.opts.RestoreHook == nil {
		return
	}
	restoreCtx, cancel := context.WithTimeout(ctx, m.opts.ResourceTimeout)
	defer cancel()
	m.mu.Lock()
	process := m.services[id].proc
	m.mu.Unlock()
	monitorDone := make(chan struct{})
	go func() {
		defer close(monitorDone)
		if process == nil {
			cancel()
			return
		}
		select {
		case <-process.done:
			cancel()
		case <-restoreCtx.Done():
		}
	}()
	defer func() { cancel(); <-monitorDone }()
	err := m.opts.RestoreHook(restoreCtx, id)
	if err == nil {
		err = restoreCtx.Err()
	}
	m.mu.Lock()
	s := m.services[id]
	s.restorePending = err != nil
	if err != nil {
		s.errorCode = "resource_restore_failed"
	} else if s.errorCode == "resource_restore_failed" {
		s.errorCode = ""
	}
	m.mu.Unlock()
	if err != nil {
		m.opts.Logger.Warn("Owned resources suspended", "service", id, "code", "resource_restore_failed")
	}
}

func (m *Manager) checkReady(ctx context.Context, id string) error {
	if m.opts.ReadyHook == nil {
		return nil
	}
	readyCtx, cancel := context.WithTimeout(ctx, m.opts.ReadyTimeout)
	defer cancel()
	if err := m.opts.ReadyHook(readyCtx, id); err != nil {
		if cleanupErr := m.cleanup(id); cleanupErr != nil {
			return cleanupErr
		}
		if ctx.Err() != nil {
			return ctx.Err()
		}
		return ErrReadiness
	}
	if readyCtx.Err() != nil {
		if cleanupErr := m.cleanup(id); cleanupErr != nil {
			return cleanupErr
		}
		return ErrReadiness
	}
	return nil
}

// ResourceOperation reserves the same lane for native network transactions.
// Unlike ReadyOperation it also permits withdrawal while the core is stopped.
func (m *Manager) ResourceOperation(ctx context.Context, id string, operation func(context.Context) error) error {
	ctx, done, err := m.begin(ctx, id)
	if err != nil {
		return err
	}
	defer done()
	return operation(ctx)
}

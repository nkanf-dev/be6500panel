package runtime

import (
	"bytes"
	"context"
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
	disk           diskState
	state          State
	binary         string
	proc           *managedProcess
	desired        bool
	restarts       int
	retryAt        time.Time
	errorCode      string
	cleanupPending bool
	epoch          uint64
	cancelWatch    context.CancelFunc
}

// Manager has a single non-blocking mutation lane. Status remains responsive
// while checks and downloads run. Exit supervision uses that same lane.
type Manager struct {
	opts      Options
	mu        sync.Mutex
	services  map[string]*serviceRuntime
	gate      chan struct{}
	ctx       context.Context
	cancel    context.CancelFunc
	closed    bool
	closeOnce sync.Once
	closeDone chan struct{}
	closeErr  error
	wg        sync.WaitGroup
	lock      *os.File
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
	if o.DownloadTimeout < time.Millisecond || o.DownloadTimeout > 10*time.Minute || o.CheckTimeout < time.Millisecond || o.CheckTimeout > time.Minute || o.TermGrace < time.Millisecond || o.TermGrace > 10*time.Second || o.BackoffInitial < time.Millisecond || o.BackoffMax < o.BackoffInitial || o.BackoffMax > 5*time.Minute || o.StableAfter < time.Millisecond {
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
	m.mu.Lock()
	closed := m.closed
	m.mu.Unlock()
	if closed {
		<-m.gate
		return nil, nil, ErrClosed
	}
	opctx, cancel := context.WithCancel(ctx)
	detach := context.AfterFunc(m.ctx, cancel)
	return opctx, func() { detach(); cancel(); <-m.gate }, nil
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
	out := Status{Service: id, State: s.state, Generation: s.disk.Generation, Configured: s.disk.Current != nil, ArtifactAvailable: s.binary != "", Desired: s.desired, Restarts: s.restarts, RetryAt: s.retryAt, ErrorCode: s.errorCode}
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
	if s.cleanupPending {
		out.RecoveryPlan = append(out.RecoveryPlan, "retry_owned_resource_cleanup")
	}
	if s.errorCode == "state_not_durable" {
		out.RecoveryPlan = append(out.RecoveryPlan, "check_persistent_storage", "stop_then_start_to_apply_accepted_config")
	}
	if s.disk.LastGood != nil && (s.state == Error || s.errorCode != "") {
		out.RecoveryPlan = append(out.RecoveryPlan, "restore_last_good_config")
	}
	return out
}
func (m *Manager) result(id string, err error) (Status, error) { s, _ := m.Status(id); return s, err }
func (m *Manager) setState(id string, state State, code string) {
	m.mu.Lock()
	s := m.services[id]
	s.state = state
	s.errorCode = code
	m.mu.Unlock()
	m.opts.Logger.Debug("managed runtime transition", "service", id, "state", state, "code", code)
}

// Acquire stages and verifies an artifact before committing metadata and
// activating it. An existing accepted config is checked with the new artifact.
// A failed download/check never stops the previous running core.
func (m *Manager) Acquire(ctx context.Context, id string, artifact Artifact) (Status, error) {
	ctx, done, err := m.begin(ctx, id)
	if err != nil {
		return m.resultSafe(id, err)
	}
	defer done()
	m.mu.Lock()
	s := m.services[id]
	previous := s.state
	disk := s.disk
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
		m.setState(id, previous, "artifact_acquire_failed")
		return m.result(id, errors.New("artifact acquisition failed"))
	}
	activated := false
	defer func() {
		if !activated {
			_ = os.Remove(staged)
		}
	}()
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
	shouldRun := s.desired
	m.mu.Unlock()
	activated = true
	if oldBinary != "" && oldBinary != staged {
		defer os.Remove(oldBinary)
	}
	if persistErr != nil {
		m.setState(id, previous, "state_not_durable")
		return m.result(id, persistErr)
	}
	if shouldRun {
		if err = m.stopProcess(id, false); err == nil {
			err = m.startProcess(id)
		}
		return m.result(id, err)
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
	ctx, done, err := m.begin(ctx, id)
	if err != nil {
		return m.resultSafe(id, err)
	}
	defer done()
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
	m.mu.Unlock()
	if disk.Generation != expectedGeneration {
		return m.result(id, ErrGeneration)
	}
	if binary == "" {
		return m.result(id, ErrNoArtifact)
	}
	candidate, err := stageConfig(m.opts, id, raw)
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
	next, err := commitConfig(m.opts, id, disk, candidate, raw)
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
		if err = m.stopProcess(id, false); err == nil {
			err = m.startProcess(id)
		}
		return m.result(id, err)
	}
	m.setState(id, Stopped, "")
	return m.result(id, nil)
}

// Restore verifies and accepts the last-good private config as a new generation.
// It never moves generation backwards, so stale clients cannot overwrite it.
func (m *Manager) Restore(ctx context.Context, id string, expectedGeneration uint64) (Status, error) {
	ctx, done, err := m.begin(ctx, id)
	if err != nil {
		return m.resultSafe(id, err)
	}
	defer done()
	m.mu.Lock()
	s := m.services[id]
	disk := s.disk
	binary := s.binary
	previous := s.state
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
	raw, err := readBounded(configPath(m.opts, id, disk.LastGood), m.opts.MaxConfigBytes)
	if err != nil {
		return m.result(id, errors.New("last-good config is unavailable"))
	}
	candidate, err := stageConfig(m.opts, id, raw)
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
	next, err := commitConfig(m.opts, id, disk, candidate, raw)
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
		if err = m.stopProcess(id, false); err == nil {
			err = m.startProcess(id)
		}
		return m.result(id, err)
	}
	m.setState(id, Stopped, "")
	return m.result(id, nil)
}

func (m *Manager) Start(ctx context.Context, id string) (Status, error) {
	ctx, done, err := m.begin(ctx, id)
	if err != nil {
		return m.resultSafe(id, err)
	}
	defer done()
	m.mu.Lock()
	s := m.services[id]
	exitedPrevious := s.cleanupPending
	if s.proc != nil {
		select {
		case <-s.proc.done:
			exitedPrevious = true
		default:
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
	err = m.startProcess(id)
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
func (m *Manager) startProcess(id string) error {
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
	watchCtx, cancelWatch := context.WithCancel(m.ctx)
	m.mu.Lock()
	if s.cancelWatch != nil {
		s.cancelWatch()
	}
	s.cancelWatch = cancelWatch
	s.proc = p
	s.state = Running
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
	m.mu.Unlock()
	return nil
}
func (m *Manager) cleanup(id string) error {
	var err error
	if m.opts.CleanupHook != nil {
		ctx, cancel := context.WithTimeout(context.Background(), m.opts.TermGrace)
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
	select {
	case m.gate <- struct{}{}:
	case <-ctx.Done():
		return
	}
	m.mu.Lock()
	s := m.services[id]
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
		_ = m.startProcess(id)
	}
}

// Close cancels downloads/checks/retries, waits for the active operation, stops
// and reaps both process groups, then releases the private store lock.
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
	raw, err := readBounded(configPath(m.opts, id, s.disk.Current), m.opts.MaxConfigBytes)
	if err != nil {
		return nil, s.disk.Generation, errors.New("accepted private config is unavailable")
	}
	return raw, s.disk.Generation, nil
}

// Check a copy so a checker cannot accidentally rewrite the accepted file.
func (m *Manager) verifyAccepted(ctx context.Context, id, binary string, record *configRecord) error {
	raw, err := readBounded(configPath(m.opts, id, record), m.opts.MaxConfigBytes)
	if err != nil {
		return errors.New("accepted private config is unavailable")
	}
	candidate, err := stageConfig(m.opts, id, raw)
	if err != nil {
		return errors.New("cannot stage private verifier copy")
	}
	defer os.Remove(candidate)
	if err = verify(ctx, binary, id, candidate, m.opts); err != nil {
		return err
	}
	checked, err := readBounded(candidate, m.opts.MaxConfigBytes)
	if err != nil || !bytes.Equal(checked, raw) {
		return errors.New("verifier modified private candidate")
	}
	return nil
}

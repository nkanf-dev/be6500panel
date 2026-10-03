package nodeprobe

import (
	"be6500panel/internal/proxy"
	"context"
	"crypto/rand"
	"encoding/hex"
	"errors"
	"sync"
	"time"
)

type probeSession interface {
	Probe(context.Context, int) (int64, string)
	Close()
}
type sessionFactory func(context.Context, CoreLease, []proxy.Node, string) (probeSession, error)
type Manager struct {
	mu       sync.Mutex
	cfg      Config
	ctx      context.Context
	cancel   context.CancelFunc
	closed   bool
	running  bool
	revision string
	job      *Job
	results  []Result
	// Same-revision observations outside the latest job survive single/page
	// probes. The public view remains bounded to MaxNodes (newest job first).
	cached       []Result
	jobCancel    context.CancelFunc
	done         chan struct{}
	startSession sessionFactory
}

func New(cfg Config) (*Manager, error) {
	if cfg.Nodes == nil {
		return nil, errors.New("node provider is required")
	}
	ctx, cancel := context.WithCancel(context.Background())
	return &Manager{cfg: cfg, ctx: ctx, cancel: cancel, results: []Result{}, startSession: newCoreSession}, nil
}
func (m *Manager) Snapshot() Snapshot {
	set, err := m.cfg.Nodes()
	code := ""
	if err != nil {
		code = "empty_subscription"
	} else if len(set.Nodes) == 0 {
		code = "empty_subscription"
	}
	if code == "" && m.cfg.AcquireLease == nil {
		code = "probes_unavailable"
	}
	if code == "" && m.cfg.Availability != nil {
		code = m.cfg.Availability()
		if code != "" {
			code = "artifact_unavailable"
		}
	}
	m.mu.Lock()
	defer m.mu.Unlock()
	view := m.snapshotLocked()
	if err == nil {
		view.Revision = set.Revision
	}
	if view.Revision != m.revision {
		view.Results = []Result{}
		view.Job = nil
	}
	view.Available = code == "" && !m.closed
	if m.closed {
		code = "probes_unavailable"
	}
	view.UnavailableCode = code
	return view
}
func (m *Manager) snapshotLocked() Snapshot {
	results := append([]Result{}, m.results...)
	remaining := MaxNodes - len(results)
	if remaining > len(m.cached) {
		remaining = len(m.cached)
	}
	if remaining > 0 {
		results = append(results, m.cached[:remaining]...)
	}
	view := Snapshot{Revision: m.revision, Target: Target, Running: m.running, Results: results, Limits: Limits{MaxNodes, Concurrency, Timeout.Milliseconds()}}
	if m.job != nil {
		job := *m.job
		view.Job = &job
		if job.FinishedAt != nil {
			value := *job.FinishedAt
			view.Job.FinishedAt = &value
		}
	}
	for i := range view.Results {
		if view.Results[i].DelayMS != nil {
			value := *view.Results[i].DelayMS
			view.Results[i].DelayMS = &value
		}
		if view.Results[i].MeasuredAt != nil {
			value := *view.Results[i].MeasuredAt
			view.Results[i].MeasuredAt = &value
		}
	}
	return view
}
func chooseNodes(set NodeSet, in StartInput) ([]proxy.Node, error) {
	if in.Revision == "" || len(in.Revision) > 128 || in.Revision != set.Revision {
		return nil, ErrRevision
	}
	if len(set.Nodes) == 0 || len(in.NodeIDs) > MaxNodes || (in.All && len(in.NodeIDs) != 0) || (!in.All && len(in.NodeIDs) == 0) {
		return nil, ErrNodes
	}
	if in.All {
		if len(set.Nodes) > MaxNodes {
			return nil, ErrNodes
		}
		return append([]proxy.Node{}, set.Nodes...), nil
	}
	available := make(map[string]proxy.Node, len(set.Nodes))
	for _, n := range set.Nodes {
		available[n.ID] = n
	}
	seen := map[string]bool{}
	selected := make([]proxy.Node, 0, len(in.NodeIDs))
	for _, id := range in.NodeIDs {
		n, ok := available[id]
		if !ok || id == "" || len(id) > 128 || seen[id] {
			return nil, ErrNodes
		}
		seen[id] = true
		selected = append(selected, n)
	}
	return selected, nil
}

// Start returns after acquiring a trusted frozen lease. The accepted job uses
// manager ownership, not the request/page lifetime. No process is started by GET.
func (m *Manager) Start(requestCtx context.Context, in StartInput) (Snapshot, error) {
	set, err := m.cfg.Nodes()
	if err != nil {
		return Snapshot{}, ErrNodes
	}
	nodes, err := chooseNodes(set, in)
	if err != nil {
		return Snapshot{}, err
	}
	if m.cfg.AcquireLease == nil {
		return Snapshot{}, ErrUnavailable
	}
	if m.cfg.Availability != nil && m.cfg.Availability() != "" {
		return Snapshot{}, ErrUnavailable
	}
	if requestCtx.Err() != nil {
		return Snapshot{}, requestCtx.Err()
	}
	token := make([]byte, 16)
	if _, err = rand.Read(token); err != nil {
		return Snapshot{}, ErrUnavailable
	}
	m.mu.Lock()
	if m.closed {
		m.mu.Unlock()
		return Snapshot{}, ErrClosed
	}
	if m.running {
		m.mu.Unlock()
		return Snapshot{}, ErrBusy
	}
	ctx, cancel := context.WithTimeout(m.ctx, JobTimeout)
	m.jobCancel = cancel
	previous := m.snapshotLocked().Results
	m.cached = nil
	selected := make(map[string]bool, len(nodes))
	for _, n := range nodes {
		selected[n.ID] = true
	}
	if m.revision == set.Revision && len(nodes) < MaxNodes {
		for _, result := range previous {
			if !selected[result.NodeID] && result.MeasuredAt != nil {
				m.cached = append(m.cached, result)
				if len(m.cached) == MaxNodes-len(nodes) {
					break
				}
			}
		}
	}
	m.running = true
	m.revision = set.Revision
	m.done = make(chan struct{})
	m.job = &Job{ID: hex.EncodeToString(token), Status: "preparing", Total: len(nodes), StartedAt: time.Now().UTC()}
	m.results = make([]Result, len(nodes))
	for i, n := range nodes {
		m.results[i] = Result{NodeID: n.ID, Status: "queued", Target: Target}
	}
	done := m.done
	m.mu.Unlock()
	// Abort failed admission when its request goes away. Stop forwarding request
	// cancellation before returning acceptance; the background owner survives it.
	stopRequestCancel := context.AfterFunc(requestCtx, cancel)
	acquireCtx, acquireCancel := context.WithTimeout(ctx, 10*time.Second)
	lease, err := m.cfg.AcquireLease(acquireCtx)
	acquireCancel()
	stopRequestCancel()
	if err != nil || lease.Path == "" || lease.Release == nil {
		if lease.Release != nil {
			lease.Release()
		}
		m.finish("failed", "artifact_unavailable", done)
		cancel()
		return Snapshot{}, ErrUnavailable
	}
	// A subscription mutation while the lease was copied cannot admit stale nodes.
	latest, latestErr := m.cfg.Nodes()
	if ctx.Err() != nil || requestCtx.Err() != nil || latestErr != nil || latest.Revision != set.Revision {
		lease.Release()
		m.finish("invalidated", "revision_mismatch", done)
		cancel()
		if latestErr == nil && latest.Revision != set.Revision {
			return Snapshot{}, ErrRevision
		}
		return Snapshot{}, ErrClosed
	}
	m.mu.Lock()
	view := m.snapshotLocked()
	view.Available = true
	m.mu.Unlock()
	go m.run(ctx, cancel, lease, nodes, done)
	return view, nil
}
func (m *Manager) run(ctx context.Context, cancel context.CancelFunc, lease CoreLease, nodes []proxy.Node, done chan struct{}) {
	defer cancel()
	// Release only after process/file cleanup. Session startup also cleans its
	// exact directory if it fails, and never includes private errors in results.
	session, err := m.startSession(ctx, lease, nodes, m.cfg.TempDir)
	if err != nil {
		lease.Release()
		if ctx.Err() != nil {
			m.finish("cancelled", "node_cancelled", done)
		} else {
			m.finish("failed", "core_unavailable", done)
		}
		return
	}
	m.mu.Lock()
	if ctx.Err() == nil {
		m.job.Status = "running"
	}
	m.mu.Unlock()
	for i := range nodes {
		if ctx.Err() != nil {
			break
		}
		m.mu.Lock()
		if ctx.Err() == nil {
			m.results[i].Status = "probing"
		}
		m.mu.Unlock()
		nodeCtx, nodeCancel := context.WithTimeout(ctx, Timeout)
		delay, code := session.Probe(nodeCtx, i)
		nodeCancel()
		m.mu.Lock()
		if ctx.Err() == nil {
			measured := time.Now().UTC()
			result := &m.results[i]
			result.MeasuredAt = &measured
			switch code {
			case "":
				result.Status = "success"
				result.DelayMS = &delay
			case "node_timeout":
				result.Status = "timeout"
				result.ErrorCode = code
			default:
				result.Status = "unreachable"
				result.ErrorCode = "node_unreachable"
			}
			m.job.Completed++
		}
		m.mu.Unlock()
	}
	session.Close()
	lease.Release()
	if ctx.Err() != nil {
		m.finish("cancelled", "node_cancelled", done)
	} else {
		m.finish("completed", "", done)
	}
}
func (m *Manager) finish(status, code string, done chan struct{}) {
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.done != done {
		return
	}
	// Preserve explicit invalidation/cancellation and already measured outcomes.
	if m.job.Status == "invalidated" || m.job.Status == "cancelled" {
		status = m.job.Status
		code = m.job.ErrorCode
	}
	measured := time.Now().UTC()
	for i := range m.results {
		result := &m.results[i]
		if result.Status == "queued" || result.Status == "probing" {
			result.Status = "cancelled"
			result.ErrorCode = "node_cancelled"
			if status == "failed" {
				result.Status = "unreachable"
				result.ErrorCode = code
				result.MeasuredAt = &measured
			}
			m.job.Completed++
		}
	}
	m.job.Status = status
	m.job.ErrorCode = code
	m.job.FinishedAt = &measured
	m.running = false
	m.jobCancel = nil
	close(done)
}
func (m *Manager) Cancel() Snapshot {
	m.mu.Lock()
	if m.running {
		m.job.Status = "cancelled"
		m.job.ErrorCode = "node_cancelled"
		m.jobCancel()
	}
	m.mu.Unlock()
	return m.Snapshot()
}

// Invalidate must be called after an authoritative subscription mutation. Old
// revision results remain private and cannot be rendered as new-node evidence.
func (m *Manager) Invalidate(revision string) {
	m.mu.Lock()
	defer m.mu.Unlock()
	if revision == m.revision {
		return
	}
	if m.running {
		m.job.Status = "invalidated"
		m.job.ErrorCode = "revision_mismatch"
		m.jobCancel()
	} else {
		m.job = nil
		m.results = []Result{}
		m.cached = nil
	}
	// Keep the old job revision until cleanup so Snapshot hides the stale results.
	if !m.running {
		m.revision = revision
	}
}
func (m *Manager) Close() {
	m.mu.Lock()
	m.closed = true
	m.cancel()
	done := m.done
	running := m.running
	m.mu.Unlock()
	if running {
		<-done
	}
}

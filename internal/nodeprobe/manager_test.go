package nodeprobe

import (
	"be6500panel/internal/proxy"
	"context"
	"errors"
	"fmt"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

type fakeSession struct {
	calls  atomic.Int32
	active atomic.Int32
	peak   atomic.Int32
	gate   chan struct{}
	closed atomic.Int32
	first  chan struct{}
}

func (f *fakeSession) Probe(ctx context.Context, index int) (int64, string) {
	active := f.active.Add(1)
	defer f.active.Add(-1)
	if active > f.peak.Load() {
		f.peak.Store(active)
	}
	f.calls.Add(1)
	if f.first != nil && index == 0 {
		return 68, ""
	}
	if f.first != nil && index == 1 {
		close(f.first)
	}
	if f.gate != nil {
		select {
		case <-ctx.Done():
			return 0, "node_timeout"
		case <-f.gate:
		}
	}
	return int64(index + 1), ""
}
func (f *fakeSession) Close() { f.closed.Add(1) }
func managerFixture(t *testing.T, count int) (*Manager, *fakeSession, *atomic.Int32, *sync.Mutex, *NodeSet) {
	t.Helper()
	set := &NodeSet{Revision: "rev-one", Nodes: make([]proxy.Node, count)}
	for i := range set.Nodes {
		set.Nodes[i].ID = fmt.Sprintf("node-%d", i)
	}
	var mu sync.Mutex
	released := &atomic.Int32{}
	session := &fakeSession{}
	manager, err := New(Config{Nodes: func() (NodeSet, error) {
		mu.Lock()
		defer mu.Unlock()
		return NodeSet{Revision: set.Revision, Nodes: append([]proxy.Node{}, set.Nodes...)}, nil
	}, AcquireLease: func(context.Context) (CoreLease, error) {
		return CoreLease{Path: "/private/frozen/core", Release: func() { released.Add(1) }}, nil
	}})
	if err != nil {
		t.Fatal(err)
	}
	manager.startSession = func(context.Context, CoreLease, []proxy.Node, string) (probeSession, error) { return session, nil }
	t.Cleanup(manager.Close)
	return manager, session, released, &mu, set
}
func waitJob(t *testing.T, m *Manager) {
	t.Helper()
	m.mu.Lock()
	done := m.done
	m.mu.Unlock()
	select {
	case <-done:
	case <-time.After(2 * time.Second):
		t.Fatal("job not cleaned")
	}
}
func TestBatch220SingleSessionSerialAndOwnerContext(t *testing.T) {
	m, session, released, _, _ := managerFixture(t, 220)
	session.gate = make(chan struct{})
	ctx, cancel := context.WithCancel(context.Background())
	view, err := m.Start(ctx, StartInput{All: true, NodeIDs: []string{}, Revision: "rev-one"})
	cancel()
	if err != nil || !view.Running || !view.Available || len(view.Results) != 220 {
		t.Fatal("explicit batch admission failed")
	}
	if _, err = m.Start(context.Background(), StartInput{All: true, Revision: "rev-one"}); !errors.Is(err, ErrBusy) {
		t.Fatal("concurrent job allowed")
	}
	close(session.gate)
	waitJob(t, m)
	view = m.Snapshot()
	if session.calls.Load() != 220 || session.peak.Load() != 1 || released.Load() != 1 || session.closed.Load() != 1 || view.Job.Status != "completed" || view.Job.Completed != 220 {
		t.Fatal("unbounded or incomplete job")
	}
	if view.Results[0].DelayMS == nil || *view.Results[0].DelayMS != 1 || view.Results[0].MeasuredAt == nil || view.Target != Target {
		t.Fatal("missing real measurement metadata")
	}
	*view.Results[0].DelayMS = 999
	if *m.Snapshot().Results[0].DelayMS == 999 {
		t.Fatal("snapshot aliased private store")
	}
}
func TestCancelRetainsMeasuredOutcomesAndCloseOwnsCleanup(t *testing.T) {
	m, session, released, _, _ := managerFixture(t, 20)
	session.first = make(chan struct{})
	session.gate = make(chan struct{})
	if _, err := m.Start(context.Background(), StartInput{All: true, Revision: "rev-one"}); err != nil {
		t.Fatal(err)
	}
	<-session.first
	m.Cancel()
	m.Close()
	view := m.Snapshot()
	if view.Running || released.Load() != 1 || session.closed.Load() != 1 || view.Job.Status != "cancelled" || view.Job.Completed != 20 {
		t.Fatal("cancel cleanup incomplete")
	}
	if view.Results[0].Status != "success" || view.Results[0].DelayMS == nil {
		t.Fatal("partial measurement lost")
	}
	for _, result := range view.Results[1:] {
		if result.Status != "cancelled" || result.DelayMS != nil {
			t.Fatal("unmeasured outcome fabricated")
		}
	}
}
func TestRevisionMutationCancelsAndHidesOldOutcomes(t *testing.T) {
	m, session, released, mu, set := managerFixture(t, 2)
	session.gate = make(chan struct{})
	if _, err := m.Start(context.Background(), StartInput{All: true, Revision: "rev-one"}); err != nil {
		t.Fatal(err)
	}
	mu.Lock()
	set.Revision = "rev-two"
	mu.Unlock()
	m.Invalidate("rev-two")
	waitJob(t, m)
	view := m.Snapshot()
	if view.Revision != "rev-two" || view.Running || len(view.Results) != 0 || view.Job != nil || released.Load() != 1 {
		t.Fatal("stale revision visible or live")
	}
	if _, err := m.Start(context.Background(), StartInput{All: true, Revision: "rev-one"}); !errors.Is(err, ErrRevision) {
		t.Fatal("old revision admitted")
	}
}
func TestGETPureAndAdmissionStrictLimits(t *testing.T) {
	m, session, released, _, _ := managerFixture(t, 257)
	for i := 0; i < 10; i++ {
		m.Snapshot()
	}
	if session.calls.Load() != 0 || released.Load() != 0 {
		t.Fatal("GET leased/probed")
	}
	cases := []StartInput{{All: true, Revision: "rev-one"}, {All: false, Revision: "rev-one"}, {All: true, NodeIDs: []string{"node-1"}, Revision: "rev-one"}, {NodeIDs: []string{"node-1", "node-1"}, Revision: "rev-one"}, {NodeIDs: []string{"missing"}, Revision: "rev-one"}}
	for _, in := range cases {
		if _, err := m.Start(context.Background(), in); !errors.Is(err, ErrNodes) {
			t.Fatal("invalid nodes accepted")
		}
	}
	if _, err := m.Start(context.Background(), StartInput{NodeIDs: []string{"node-1"}, Revision: "rev-one"}); err != nil {
		t.Fatal("page from larger subscription rejected")
	}
	waitJob(t, m)
}
func TestLeaseFailureAndRevisionDuringLease(t *testing.T) {
	m, _, _, mu, set := managerFixture(t, 1)
	var released atomic.Int32
	m.cfg.AcquireLease = func(context.Context) (CoreLease, error) {
		mu.Lock()
		set.Revision = "next"
		mu.Unlock()
		return CoreLease{Path: "/frozen", Release: func() { released.Add(1) }}, nil
	}
	if _, err := m.Start(context.Background(), StartInput{All: true, Revision: "rev-one"}); !errors.Is(err, ErrRevision) {
		t.Fatal("stale admission survived lease")
	}
	if released.Load() != 1 {
		t.Fatal("stale lease leaked")
	}
	m.cfg.AcquireLease = func(context.Context) (CoreLease, error) { return CoreLease{}, errors.New("PRIVATEARGVUUID") }
	if _, err := m.Start(context.Background(), StartInput{All: true, Revision: "next"}); !errors.Is(err, ErrUnavailable) {
		t.Fatal("lease error not fixed")
	}
	if m.Snapshot().Job.ErrorCode != "artifact_unavailable" {
		t.Fatal("private error exposed")
	}
}
func TestCloseDuringLeaseDoesNotDeadlock(t *testing.T) {
	m, _, _, _, _ := managerFixture(t, 1)
	entered := make(chan struct{})
	m.cfg.AcquireLease = func(ctx context.Context) (CoreLease, error) {
		close(entered)
		<-ctx.Done()
		return CoreLease{}, ctx.Err()
	}
	started := make(chan struct{})
	go func() { m.Start(context.Background(), StartInput{All: true, Revision: "rev-one"}); close(started) }()
	<-entered
	m.Close()
	<-started
	if m.Snapshot().Running {
		t.Fatal("close left admission live")
	}
}

package control

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

func TestStatusRetainsAuthoritativeOperationAfterLostResponseAndTerminalActions(t *testing.T) {
	f := newFixture(t)
	m := openFixture(t, f)
	if m.Status().Operation != nil {
		t.Fatal("invented operation without a journal")
	}
	d := stage(t, m, "network", strings.ReplaceAll(testNetwork, "192.168.31.1", "192.168.32.1"))
	op, err := commit(t, m, d, true)
	if err != nil {
		t.Fatal(err)
	}
	s := m.Status()
	if s.Operation == nil || s.Operation.ID != op.ID || s.Operation.Phase != "pending" || !s.Operation.CanConfirm || !s.Operation.CanRollback {
		t.Fatalf("lost response cannot be reconciled: %#v", s.Operation)
	}
	s.Operation.ChangedModules[0] = "system"
	*s.Operation.Deadline = time.Time{}
	if m.Status().Operation.ChangedModules[0] != "network" || m.Status().Operation.Deadline.IsZero() {
		t.Fatal("status leaked journal references")
	}
	confirmed, err := m.Confirm(context.Background(), op.ID)
	if err != nil {
		t.Fatal(err)
	}
	s = m.Status()
	if s.PendingCommit != nil || s.Operation == nil || s.Operation.ID != confirmed.ID || s.Operation.Phase != "committed" || s.Operation.State != "committed" || s.Operation.CanConfirm || !s.Operation.CanRollback {
		t.Fatalf("confirmed journal was lost: %#v", s)
	}
	rolled, err := m.Rollback(context.Background(), op.ID)
	if err != nil {
		t.Fatal(err)
	}
	s = m.Status()
	if s.Operation == nil || s.Operation.ID != rolled.ID || s.Operation.Phase != "rolled_back" || s.Operation.State != "rolled_back" || s.Operation.CanConfirm || s.Operation.CanRollback {
		t.Fatalf("terminal rollback was lost: %#v", s)
	}
	_, err = m.Confirm(context.Background(), op.ID)
	errorCode(t, err, "not_pending")
	if m.Status().Operation.State != "rolled_back" || readFixture(t, f, "network") != testNetwork {
		t.Fatal("late confirmation revived the candidate")
	}
	m.Close()
	s = m.Status()
	if s.Enabled || s.ErrorCode != "closed" || s.Operation.CanConfirm || s.Operation.CanRollback {
		t.Fatalf("closed manager advertised actions: %#v", s)
	}
}

func TestExpiryRecoveryFailureKeepsJournalAndExplicitRetryRestoresExactPrevious(t *testing.T) {
	f := newFixture(t)
	o := f.options()
	o.ConfirmationTimeout = 15 * time.Millisecond
	var calls atomic.Int32
	var failRestore atomic.Bool
	failRestore.Store(true)
	rollbackAttempted := make(chan struct{}, 1)
	o.Reload = func(context.Context, string) error {
		if calls.Add(1) > 1 && failRestore.Load() {
			select {
			case rollbackAttempted <- struct{}{}:
			default:
			}
			return errors.New("synthetic restore reload failure")
		}
		return nil
	}
	m, err := New(o)
	if err != nil {
		t.Fatal(err)
	}
	defer m.Close()
	d := stage(t, m, "network", strings.ReplaceAll(testNetwork, "192.168.31.1", "192.168.32.1"))
	op, err := commit(t, m, d, true)
	if err != nil {
		t.Fatal(err)
	}
	select {
	case <-rollbackAttempted:
	case <-time.After(time.Second):
		t.Fatal("expiry did not attempt recovery")
	}
	s := m.Status()
	if s.Enabled || s.PendingCommit != nil || s.ErrorCode != "rollback_failed" || s.Operation == nil || s.Operation.ID != op.ID || s.Operation.Phase != "rolling_back" || s.Operation.State == "rolled_back" || s.Operation.ErrorCode != "rollback_failed" || s.Operation.CanConfirm || !s.Operation.CanRollback {
		t.Fatalf("missing pending was treated as restored: %#v / %#v", s, s.Operation)
	}
	_, err = m.Documents(context.Background())
	errorCode(t, err, "rollback_failed")
	failRestore.Store(false)
	restored, err := m.Rollback(context.Background(), op.ID)
	if err != nil || restored.State != "rolled_back" {
		t.Fatalf("explicit recovery failed: %#v %v", restored, err)
	}
	info, err := os.Stat(filepath.Join(f.root, "etc", "config", "network"))
	if err != nil || info.Mode().Perm() != 0644 || readFixture(t, f, "network") != testNetwork {
		t.Fatal("previous bytes or permissions were not restored exactly")
	}
	s = m.Status()
	if !s.Enabled || s.ErrorCode != "" || s.Operation.Phase != "rolled_back" || s.Operation.CanRollback {
		t.Fatalf("recovery did not become terminal: %#v", s)
	}
}

func TestRestartRecoveryFailureRemainsAvailableAndRetryable(t *testing.T) {
	f := newFixture(t)
	m := openFixture(t, f)
	d := stage(t, m, "network", strings.ReplaceAll(testNetwork, "192.168.31.1", "192.168.32.1"))
	op, err := commit(t, m, d, true)
	if err != nil {
		t.Fatal(err)
	}
	m.Close()
	var failReload atomic.Bool
	failReload.Store(true)
	o := f.options()
	o.Reload = func(context.Context, string) error {
		if failReload.Load() {
			return errors.New("synthetic recovery failure")
		}
		return nil
	}
	recovering, err := New(o)
	if err != nil {
		t.Fatalf("valid rollback journal became unreachable: %v", err)
	}
	defer recovering.Close()
	s := recovering.Status()
	if s.Enabled || s.Operation == nil || s.Operation.ID != op.ID || s.Operation.Phase != "rolling_back" || !s.Operation.CanRollback || s.Operation.CanConfirm {
		t.Fatalf("recovery unavailable after restart: %#v", s)
	}
	failReload.Store(false)
	_, err = recovering.Rollback(context.Background(), op.ID)
	if err != nil {
		t.Fatal(err)
	}
	recovering.Close()
	reopened, err := New(f.options())
	if err != nil {
		t.Fatal(err)
	}
	defer reopened.Close()
	if s := reopened.Status(); s.Operation == nil || s.Operation.ID != op.ID || s.Operation.Phase != "rolled_back" || s.Operation.State != "rolled_back" || !s.Enabled {
		t.Fatalf("terminal journal missing after reconnect: %#v", s)
	}
}

func TestStatusRollbackActionRejectsKnownNewerGeneration(t *testing.T) {
	f := newFixture(t)
	m := openFixture(t, f)
	d := stage(t, m, "dhcp", testDHCP+" option local '/one/'\n")
	op, err := commit(t, m, d, false)
	if err != nil {
		t.Fatal(err)
	}
	if !m.Status().Operation.CanRollback {
		t.Fatal("last accepted generation was not recoverable")
	}
	if err = os.WriteFile(filepath.Join(f.root, "etc", "config", "dhcp"), []byte(testDHCP+" option local '/external/'\n"), 0644); err != nil {
		t.Fatal(err)
	}
	if _, err = m.Documents(context.Background()); err != nil {
		t.Fatal(err)
	}
	if m.Status().Operation.CanRollback {
		t.Fatal("old rollback could overwrite a known newer generation")
	}
	_, err = m.Rollback(context.Background(), op.ID)
	errorCode(t, err, "generation_conflict")
}

func TestLateConfirmAtExpiredDeadlineReportsAuthoritativeRestoration(t *testing.T) {
	f := newFixture(t)
	m := openFixture(t, f)
	d := stage(t, m, "network", strings.ReplaceAll(testNetwork, "192.168.31.1", "192.168.32.1"))
	op, err := commit(t, m, d, true)
	if err != nil {
		t.Fatal(err)
	}
	m.mu.Lock()
	deadline := time.Now().Add(-time.Second)
	m.journal.Operation.Deadline = &deadline
	if err = m.saveJournal(); err != nil {
		t.Fatal(err)
	}
	m.mu.Unlock()
	if m.Status().Operation.CanConfirm {
		t.Fatal("expired deadline advertised confirmation")
	}
	result, err := m.Confirm(context.Background(), op.ID)
	if err != nil || result.State != "rolled_back" {
		t.Fatalf("late confirmation accepted candidate: %#v %v", result, err)
	}
	s := m.Status()
	if s.Operation.ID != op.ID || s.Operation.Phase != "rolled_back" || readFixture(t, f, "network") != testNetwork {
		t.Fatal("late confirmation lost recovery truth")
	}
}

func TestFailedTerminalJournalWriteNeverClaimsRestored(t *testing.T) {
	f := newFixture(t)
	m := openFixture(t, f)
	d := stage(t, m, "network", strings.ReplaceAll(testNetwork, "192.168.31.1", "192.168.32.1"))
	op, err := commit(t, m, d, true)
	if err != nil {
		t.Fatal(err)
	}
	journalPath := filepath.Join(f.data, "journal.json")
	f.mu.Lock()
	f.reloadFn = func(context.Context, string) error {
		if err := os.Remove(journalPath); err != nil {
			return err
		}
		return os.Mkdir(journalPath, 0700)
	}
	f.mu.Unlock()
	_, err = m.Rollback(context.Background(), op.ID)
	if err == nil {
		t.Fatal("terminal journal failure not reported")
	}
	s := m.Status()
	if s.Enabled || s.Operation.Phase != "rolling_back" || s.Operation.State == "rolled_back" || s.Operation.ID != op.ID || !s.Operation.CanRollback || s.Operation.CanConfirm {
		t.Fatalf("undurable terminal result exposed: %#v", s)
	}
	_, err = m.Rollback(context.Background(), "unknown-operation")
	errorCode(t, err, "operation_not_found")
	if m.Status().Operation.ID != op.ID {
		t.Fatal("unknown operation displaced journal")
	}
	f.mu.Lock()
	f.reloadFn = nil
	f.mu.Unlock()
	if err = os.Remove(journalPath); err != nil {
		t.Fatal(err)
	}
	result, err := m.Rollback(context.Background(), op.ID)
	if err != nil || result.State != "rolled_back" || m.Status().Operation.Phase != "rolled_back" {
		t.Fatalf("durable retry did not close recovery: %#v %v", result, err)
	}
}

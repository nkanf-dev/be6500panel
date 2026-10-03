package control

import (
	"context"
	"sync"
	"testing"
	"time"
)

func TestDeleteDraftCancelledBeforeAdmissionDoesNotWaitOrDeleteLater(t *testing.T) {
	f := newFixture(t)
	m := openFixture(t, f)
	d := stage(t, m, "dhcp", testDHCP+" option local '/retained/'\n")
	m.mu.Lock()
	var unlock sync.Once
	release := func() { unlock.Do(m.mu.Unlock) }
	defer release()
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	finished := make(chan error, 1)
	go func() { finished <- m.DeleteDraft(ctx, d.ID) }()
	cancel()
	select {
	case err := <-finished:
		errorCode(t, err, "cancelled")
	case <-time.After(time.Second):
		t.Fatal("cancelled draft cleanup still waited on manager mutex")
	}
	release()
	drafts, err := m.Drafts(context.Background())
	if err != nil || len(drafts) != 1 || drafts[0].ID != d.ID {
		t.Fatal("canceled cleanup deleted a retained draft after returning")
	}
	m.Close()
	reopened, err := New(f.options())
	if err != nil {
		t.Fatal(err)
	}
	defer reopened.Close()
	drafts, err = reopened.Drafts(context.Background())
	if err != nil || len(drafts) != 1 || drafts[0].ID != d.ID {
		t.Fatal("canceled cleanup did not preserve the exact draft on disk")
	}
}

func TestDeleteDraftDeadlineBeforeAdmissionPreservesDraft(t *testing.T) {
	f := newFixture(t)
	m := openFixture(t, f)
	d := stage(t, m, "dhcp", testDHCP+" option local '/retained/'\n")
	m.mu.Lock()
	var unlock sync.Once
	release := func() { unlock.Do(m.mu.Unlock) }
	defer release()
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Millisecond)
	defer cancel()
	finished := make(chan error, 1)
	go func() { finished <- m.DeleteDraft(ctx, d.ID) }()
	select {
	case err := <-finished:
		errorCode(t, err, "cancelled")
	case <-time.After(time.Second):
		t.Fatal("timed-out cleanup did not return before admission")
	}
	release()
	drafts, err := m.Drafts(context.Background())
	if err != nil || len(drafts) != 1 || drafts[0].ID != d.ID {
		t.Fatal("deadline cleanup deleted retained draft")
	}
	if err = m.DeleteDraft(context.Background(), d.ID); err != nil {
		t.Fatal(err)
	}
	drafts, err = m.Drafts(context.Background())
	if err != nil || len(drafts) != 0 {
		t.Fatal("explicit cleanup did not work after mutex became available")
	}
}

func TestDeleteDraftManagerCloseCancelsAdmissionAndPreservesDisk(t *testing.T) {
	f := newFixture(t)
	m := openFixture(t, f)
	d := stage(t, m, "dhcp", testDHCP+" option local '/retained/'\n")
	m.mu.Lock()
	var unlock sync.Once
	release := func() { unlock.Do(m.mu.Unlock) }
	defer release()
	finished := make(chan error, 1)
	go func() { finished <- m.DeleteDraft(context.Background(), d.ID) }()
	closed := make(chan struct{})
	go func() { m.Close(); close(closed) }()
	select {
	case err := <-finished:
		errorCode(t, err, "closed")
	case <-time.After(time.Second):
		t.Fatal("manager close did not cancel cleanup admission")
	}
	release()
	select {
	case <-closed:
	case <-time.After(time.Second):
		t.Fatal("manager close did not finish")
	}
	reopened, err := New(f.options())
	if err != nil {
		t.Fatal(err)
	}
	defer reopened.Close()
	drafts, err := reopened.Drafts(context.Background())
	if err != nil || len(drafts) != 1 || drafts[0].ID != d.ID {
		t.Fatal("closed cleanup deleted persisted draft")
	}
}

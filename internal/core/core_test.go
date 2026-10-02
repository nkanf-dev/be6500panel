package core

import (
	"context"
	"errors"
	"io"
	"log/slog"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

type provider struct{ module Module }

func (p provider) Descriptor() Module { return p.module }
func TestRegistry(t *testing.T) {
	r := &Registry{}
	m := Module{ID: "test", Title: "Test", State: "ready", Capabilities: []Capability{{ID: "read", Title: "Read", Supported: true}}}
	if err := r.Register(provider{m}); err != nil {
		t.Fatal(err)
	}
	if err := r.Register(provider{m}); err == nil {
		t.Fatal("duplicate accepted")
	}
	copies := r.Modules()
	copies[0].Capabilities[0].Title = "changed"
	copies[0].ID = "other"
	if r.Modules()[0].ID != "test" || r.Modules()[0].Capabilities[0].Title != "Read" {
		t.Fatal("registry aliases")
	}
	for _, bad := range []Module{{ID: "", Title: "Test", State: "ready"}, {ID: "bad", Title: "Test", State: "unknown"}, {ID: "bad", Title: "Test", State: "unavailable", Capabilities: []Capability{{ID: "read", Title: "Read"}}}} {
		if err := r.Register(provider{bad}); err == nil {
			t.Fatal("bad descriptor accepted")
		}
	}
}
func TestSamplerBoundedAndFailureRecovery(t *testing.T) {
	var calls atomic.Int64
	var failed atomic.Bool
	observe := func(context.Context) (SystemStatus, error) {
		calls.Add(1)
		if failed.Load() {
			return SystemStatus{}, errors.New("test unavailable")
		}
		return SystemStatus{Mode: "demo", SampledAt: time.Now()}, nil
	}
	s := NewSampler(observe, time.Hour)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	s.Start(ctx)
	defer s.Close()
	var channels []<-chan Event
	var cancels []func()
	for i := 0; i < MaxSubscribers; i++ {
		ch, cancel, err := s.Subscribe()
		if err != nil {
			t.Fatal(err)
		}
		channels = append(channels, ch)
		cancels = append(cancels, cancel)
	}
	if _, _, err := s.Subscribe(); !errors.Is(err, ErrTooManySubscribers) {
		t.Fatal(err)
	}
	if calls.Load() != 1 {
		t.Fatal("per-client sampling")
	}
	for i := 0; i < 10; i++ {
		s.sample(ctx)
	}
	for _, ch := range channels {
		if len(ch) != 1 {
			t.Fatal("unbounded queue")
		}
		event := <-ch
		if event.ID != 11 {
			t.Fatal("stale event", event.ID)
		}
	}
	// Populate one stale sample, then fail. All queues must drain and close.
	s.sample(ctx)
	failed.Store(true)
	s.sample(ctx)
	if s.SubscriberCount() != 0 {
		t.Fatal("failed stream retained")
	}
	for _, ch := range channels {
		if _, ok := <-ch; ok {
			t.Fatal("stale sample after failure")
		}
	}
	if _, err := s.Latest(); err == nil {
		t.Fatal("stale latest")
	}
	if _, _, err := s.Subscribe(); err == nil {
		t.Fatal("failed stream accepted")
	}
	for _, cancel := range cancels {
		cancel()
		cancel()
	}
	failed.Store(false)
	s.sample(ctx)
	ch, cancelSub, err := s.Subscribe()
	if err != nil {
		t.Fatal(err)
	}
	defer cancelSub()
	if (<-ch).ID != 13 {
		t.Fatal("recovery")
	}
	s.Close()
	if s.SubscriberCount() != 0 {
		t.Fatal("close leak")
	}
	if _, _, err := s.Subscribe(); !errors.Is(err, ErrSamplerClosed) {
		t.Fatal(err)
	}
}
func TestSamplerConcurrentDisconnect(t *testing.T) {
	s := NewSampler(func(context.Context) (SystemStatus, error) { return SystemStatus{SampledAt: time.Now()}, nil }, time.Hour)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	s.Start(ctx)
	defer s.Close()
	var wg sync.WaitGroup
	for i := 0; i < 32; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for j := 0; j < 20; j++ {
				_, unsubscribe, err := s.Subscribe()
				if err == nil {
					unsubscribe()
				}
			}
		}()
	}
	for i := 0; i < 100; i++ {
		s.sample(ctx)
	}
	wg.Wait()
	if s.SubscriberCount() != 0 {
		t.Fatal("leak")
	}
}
func TestSamplerCloseStopsWork(t *testing.T) {
	var calls atomic.Int64
	s := NewSampler(func(context.Context) (SystemStatus, error) {
		calls.Add(1)
		return SystemStatus{SampledAt: time.Now()}, nil
	}, time.Millisecond)
	s.Start(context.Background())
	s.Close()
	// done must be signalled even when the caller's context remains active.
	select {
	case <-s.done:
	case <-time.After(time.Second):
		t.Fatal("sampler never stopped")
	}
}
func TestLogRing(t *testing.T) {
	buffer := &LogBuffer{}
	logger := slog.New(NewRingHandler(slog.NewJSONHandler(io.Discard, nil), buffer)).With("module", "proxy")
	for i := 0; i < 600; i++ {
		logger.Info("Plan validated", "code", "plan_validated", "password", "must-not-enter-api")
	}
	entries := buffer.Entries(500)
	if len(entries) != 500 || entries[0].Sequence != 101 || entries[499].Sequence != 600 {
		t.Fatal("ring bound/order", entries)
	}
	if entries[0].Code != "plan_validated" || entries[0].Module != "proxy" || entries[0].Message != "Plan validated" {
		t.Fatal(entries[0])
	}
	if len(buffer.Entries(3)) != 3 || len(buffer.Entries(1000)) != 500 || len(buffer.Entries(-1)) != 0 {
		t.Fatal("limit")
	}
}
func TestCoordinatorReadOnly(t *testing.T) {
	c := NewCoordinator()
	first, err := c.Plan("test", []PlanStep{}, []string{})
	if err != nil {
		t.Fatal(err)
	}
	second, _ := c.Plan("test", []PlanStep{}, []string{})
	if first.ID == second.ID || first.Generation != second.Generation || first.CanApply || !first.ReadOnly {
		t.Fatal(first, second)
	}
}

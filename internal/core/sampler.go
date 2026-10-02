package core

import (
	"context"
	"errors"
	"sync"
	"time"
)

var ErrSamplerClosed = errors.New("sampler closed")
var ErrTooManySubscribers = errors.New("too many event subscribers")

const MaxSubscribers = 64

type Observation func(context.Context) (SystemStatus, error)
type Event struct {
	ID       uint64
	Snapshot Snapshot
}

// Sampler observes once per interval for all clients. Each client has one queued
// event; a newer event replaces a stale one. There are no per-client pollers.
type Sampler struct {
	observe     Observation
	interval    time.Duration
	mu          sync.Mutex
	subscribers map[chan Event]struct{}
	latest      Event
	lastErr     error
	closed      bool
	done        chan struct{}
	once        sync.Once
}

func NewSampler(observe Observation, interval time.Duration) *Sampler {
	if interval <= 0 {
		interval = 2 * time.Second
	}
	return &Sampler{observe: observe, interval: interval, subscribers: make(map[chan Event]struct{}), done: make(chan struct{})}
}
func (s *Sampler) Start(ctx context.Context) {
	s.once.Do(func() {
		s.sample(ctx)
		go func() {
			ticker := time.NewTicker(s.interval)
			defer ticker.Stop()
			defer s.Close()
			for {
				select {
				case <-ctx.Done():
					return
				case <-s.done:
					return
				case <-ticker.C:
					s.sample(ctx)
				}
			}
		}()
	})
}
func (s *Sampler) sample(ctx context.Context) {
	system, err := s.observe(ctx)
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed {
		return
	}
	s.lastErr = err
	if err != nil {
		// Invalidate every stream, including queued samples. Reconnect fails
		// until observation recovers, so stale data cannot look connected.
		for ch := range s.subscribers {
			select {
			case <-ch:
			default:
			}
			delete(s.subscribers, ch)
			close(ch)
		}
		return
	} // Never turn an observation failure into zero/fabricated data.
	s.latest = Event{ID: s.latest.ID + 1, Snapshot: Snapshot{System: system, SampledAt: system.SampledAt}}
	for ch := range s.subscribers {
		select {
		case ch <- s.latest:
		default:
			select {
			case <-ch:
			default:
			}
			select {
			case ch <- s.latest:
			default:
			}
		}
	}
}
func (s *Sampler) Latest() (SystemStatus, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.lastErr != nil {
		return SystemStatus{}, s.lastErr
	}
	if s.latest.ID == 0 {
		return SystemStatus{}, errors.New("no system observation available")
	}
	return s.latest.Snapshot.System, nil
}
func (s *Sampler) Subscribe() (<-chan Event, func(), error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed {
		return nil, nil, ErrSamplerClosed
	}
	if len(s.subscribers) >= MaxSubscribers {
		return nil, nil, ErrTooManySubscribers
	}
	if s.lastErr != nil {
		return nil, nil, s.lastErr
	}
	if s.latest.ID == 0 {
		return nil, nil, errors.New("no system observation available")
	}
	ch := make(chan Event, 1)
	s.subscribers[ch] = struct{}{}
	ch <- s.latest
	var once sync.Once
	unsubscribe := func() {
		once.Do(func() {
			s.mu.Lock()
			defer s.mu.Unlock()
			if _, ok := s.subscribers[ch]; ok {
				delete(s.subscribers, ch)
				close(ch)
			}
		})
	}
	return ch, unsubscribe, nil
}
func (s *Sampler) SubscriberCount() int { s.mu.Lock(); defer s.mu.Unlock(); return len(s.subscribers) }
func (s *Sampler) Close() {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed {
		return
	}
	s.closed = true
	close(s.done)
	for ch := range s.subscribers {
		delete(s.subscribers, ch)
		close(ch)
	}
}

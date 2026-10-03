package devicetelemetry

import (
	"context"
	"errors"
	"math/bits"
	"sort"
	"sync"
	"time"
)

type bucket struct {
	start            int64
	rx, tx, coverage uint64
}
type baseline struct {
	at       time.Time
	online   *uint64
	counters []Counter
}
type tracked struct {
	observation    Observation
	seen           time.Time
	previous       *baseline
	rxRate, txRate *float64
	fine           [fineCapacity]bucket
	coarse         [coarseCapacity]bucket
}
type Collector struct {
	mu                sync.Mutex
	source            Source
	devices           map[string]*tracked
	sampledAt         time.Time
	observationError  string
	sourceUnavailable bool
	truncated         bool
	now               func() time.Time
	started, closed   bool
	cancel            context.CancelFunc
	done              chan struct{}
}

// New creates a memory-only central collector. It starts no goroutine and
// writes no files. Start must be called by the application, not an HTTP route.
func New(source Source) (*Collector, error) {
	if source == nil {
		return nil, errors.New("device telemetry requires a source")
	}
	return &Collector{source: source, devices: make(map[string]*tracked), now: time.Now}, nil
}
func (c *Collector) Start(ctx context.Context) {
	c.mu.Lock()
	if c.started || c.closed {
		c.mu.Unlock()
		return
	}
	worker, cancel := context.WithCancel(ctx)
	c.cancel = cancel
	c.started = true
	c.done = make(chan struct{})
	done := c.done
	c.mu.Unlock()
	go func() {
		defer close(done)
		ticker := time.NewTicker(SampleInterval)
		defer ticker.Stop()
		c.collect(worker)
		for {
			select {
			case <-worker.Done():
				return
			case <-ticker.C:
				c.collect(worker)
			}
		}
	}()
}
func (c *Collector) collect(ctx context.Context) {
	child, cancel := context.WithTimeout(ctx, 1500*time.Millisecond)
	s, err := c.source.Snapshot(child)
	cancel()
	if err != nil {
		c.mu.Lock()
		defer c.mu.Unlock()
		c.observationError = "System device traffic counters could not be read."
		c.sourceUnavailable = true
		for _, d := range c.devices {
			d.previous = nil
			d.rxRate = nil
			d.txRate = nil
		}
		return
	}
	c.record(s)
}
func (c *Collector) Close() error {
	c.mu.Lock()
	if c.closed {
		c.mu.Unlock()
		return nil
	}
	c.closed = true
	if c.cancel != nil {
		c.cancel()
	}
	done := c.done
	c.mu.Unlock()
	if done != nil {
		<-done
	}
	return nil
}
func (c *Collector) record(s Snapshot) {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.closed {
		return
	}
	if s.SampledAt.IsZero() || s.SampledAt.Unix() < 0 || s.SampledAt.Year() > 9999 || len(s.Devices) > MaxSourceRows {
		c.observationError = "System device observation is invalid."
		c.sourceUnavailable = true
		for _, d := range c.devices {
			d.previous = nil
			d.rxRate = nil
			d.txRate = nil
		}
		return
	}
	// A repeated source cache must not establish a new baseline or fresh rates.
	if !c.sampledAt.IsZero() && s.SampledAt.Before(c.sampledAt) {
		// Wall clock regression invalidates timing. Do not create a short elapsed
		// interval spanning the ignored backwards-clock traffic on recovery.
		for _, d := range c.devices {
			d.previous = nil
			d.rxRate = nil
			d.txRate = nil
		}
		return
	}
	if s.SampledAt.Equal(c.sampledAt) {
		return
	}
	c.sampledAt = s.SampledAt.UTC()
	c.observationError = s.Error
	c.sourceUnavailable = s.Error != "" && len(s.Devices) == 0
	c.truncated = s.Truncated
	seen := map[string]bool{}
	// Limit the actor cardinality even when an injected source exceeds the cap.
	rows := s.Devices
	if len(rows) > MaxDevices {
		rows = rows[:MaxDevices]
		c.truncated = true
	}
	// Protect all incoming identities from LRU eviction during this sample.
	incoming := map[string]bool{}
	for _, o := range rows {
		incoming[o.ID] = true
	}
	for _, o := range rows {
		if mac(o.ID) != o.ID || !validText(o.Name, 128) || !validText(o.Interface, 256) || len(o.Counters) > MaxAddresses || seen[o.ID] {
			c.observationError = "Some system device rows were invalid."
			continue
		}
		seen[o.ID] = true
		d := c.devices[o.ID]
		if d == nil {
			if len(c.devices) >= MaxDevices {
				var oldestID string
				for id, entry := range c.devices {
					if incoming[id] {
						continue
					}
					if oldestID == "" || entry.seen.Before(c.devices[oldestID].seen) || (entry.seen.Equal(c.devices[oldestID].seen) && id < oldestID) {
						oldestID = id
					}
				}
				if oldestID == "" {
					c.truncated = true
					continue
				}
				delete(c.devices, oldestID)
				c.truncated = true
			}
			d = &tracked{}
			c.devices[o.ID] = d
		}
		// Copy the source: later source mutation must not corrupt a baseline.
		o = copyObservation(o)
		d.observation = o
		d.seen = s.SampledAt.UTC()
		d.rxRate = nil
		d.txRate = nil
		prev := d.previous
		d.previous = nil
		if !o.Associated || len(o.Counters) == 0 || (o.AgeingSeconds != nil && *o.AgeingSeconds > uint64(StaleAfter/time.Second)) {
			continue
		}
		next := &baseline{at: s.SampledAt.UTC(), online: o.OnlineSeconds, counters: o.Counters}
		d.previous = next
		if prev == nil {
			continue
		}
		elapsed := next.at.Sub(prev.at)
		if elapsed <= 0 || elapsed > 3*SampleInterval || (next.online != nil && prev.online != nil && *next.online < *prev.online) {
			continue
		}
		rx, tx, valid := counterDelta(prev.counters, next.counters)
		if !valid || float64(rx)/elapsed.Seconds() > float64(uint64(1)<<40) || float64(tx)/elapsed.Seconds() > float64(uint64(1)<<40) {
			continue
		}
		rr, tr := float64(rx)/elapsed.Seconds(), float64(tx)/elapsed.Seconds()
		d.rxRate = &rr
		d.txRate = &tr
		add(d.fine[:], fineSeconds, prev.at, next.at, rx, tx)
		add(d.coarse[:], coarseSeconds, prev.at, next.at, rx, tx)
	}
	for id, d := range c.devices {
		if !seen[id] {
			d.previous = nil
			d.rxRate = nil
			d.txRate = nil
		}
		if s.SampledAt.Sub(d.seen) > RetentionDays*24*time.Hour {
			delete(c.devices, id)
		}
	}
}
func counterDelta(previous, next []Counter) (uint64, uint64, bool) {
	if len(previous) != len(next) || len(next) == 0 {
		return 0, 0, false
	}
	old := make(map[string]Counter, len(previous))
	for _, p := range previous {
		if _, ok := old[p.Address]; ok {
			return 0, 0, false
		}
		old[p.Address] = p
	}
	var rx, tx uint64
	seen := map[string]bool{}
	for _, n := range next {
		p, ok := old[n.Address]
		if !ok || seen[n.Address] || n.RX < p.RX || n.TX < p.TX {
			return 0, 0, false
		}
		seen[n.Address] = true
		dr, dt := n.RX-p.RX, n.TX-p.TX
		if ^uint64(0)-rx < dr || ^uint64(0)-tx < dt {
			return 0, 0, false
		}
		rx += dr
		tx += dt
	}
	return rx, tx, true
}
func fraction(total, elapsed, duration uint64) uint64 {
	hi, lo := bits.Mul64(total, elapsed)
	part, _ := bits.Div64(hi, lo, duration)
	return part
}

// Actual deltas cross time buckets using cumulative integer fractions. The
// pieces sum to the original counters; unobserved intervals stay absent.
func add(ring []bucket, seconds int64, from, to time.Time, rx, tx uint64) {
	duration := uint64(to.Sub(from))
	cursor := from
	var assignedRX, assignedTX uint64
	for cursor.Before(to) {
		start := cursor.Unix() / seconds * seconds
		end := time.Unix(start+seconds, 0).UTC()
		if end.After(to) {
			end = to
		}
		cumulative := uint64(end.Sub(from))
		nr, nt := fraction(rx, cumulative, duration), fraction(tx, cumulative, duration)
		b := &ring[(start/seconds)%int64(len(ring))]
		if b.start != start {
			*b = bucket{start: start}
		}
		coverage := uint64(end.Sub(cursor))
		if b.coverage+coverage <= uint64(seconds)*uint64(time.Second) {
			b.rx += nr - assignedRX
			b.tx += nt - assignedTX
			b.coverage += coverage
		}
		assignedRX = nr
		assignedTX = nt
		cursor = end
	}
}
func deviceOrder(devices []Device) {
	sort.Slice(devices, func(i, j int) bool {
		a, b := devices[i], devices[j]
		if a.RXBytes+a.TXBytes != b.RXBytes+b.TXBytes {
			return a.RXBytes+a.TXBytes > b.RXBytes+b.TXBytes
		}
		if a.Stale != b.Stale {
			return !a.Stale
		}
		return a.ID < b.ID
	})
}

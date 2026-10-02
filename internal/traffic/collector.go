package traffic

import (
	"context"
	"errors"
	"math"
	"math/bits"
	"os"
	"sync"
	"time"

	"be6500panel/internal/router"
)

type observation struct {
	at     time.Time
	source string
	rx, tx uint64
}

type Collector struct {
	mu               sync.Mutex
	source           Source
	rings            []*ring
	previous         *observation
	latest           time.Time
	lastFlushAt      time.Time
	sourceName       string
	observationError string
	storageError     string
	recoveryWarning  bool
	closed           bool
	started          bool
	cancel           context.CancelFunc
	done             chan struct{}
	now              func() time.Time
}

// New opens fixed allocated rings. It returns an error on an unusable data
// directory/header, insufficient space, or sync failure. It starts no goroutine.
func New(opts Options) (*Collector, error) {
	if opts.DataDir == "" {
		return nil, errors.New("traffic history requires a persistent data directory")
	}
	if opts.Source == nil {
		return nil, errors.New("traffic history requires a router observation source")
	}
	if err := os.MkdirAll(opts.DataDir, 0700); err != nil {
		return nil, err
	}
	c := &Collector{source: opts.Source, now: time.Now}
	for _, layout := range layouts {
		r, recovered, err := openRing(opts.DataDir, layout.seconds, layout.capacity)
		if err != nil {
			for _, opened := range c.rings {
				_ = opened.file.Close()
			}
			return nil, err
		}
		c.rings = append(c.rings, r)
		c.recoveryWarning = c.recoveryWarning || recovered
		for _, b := range r.buckets {
			if b.valid {
				at := time.Unix(b.start, int64(b.endMillis)*int64(time.Millisecond))
				if at.After(c.latest) {
					c.latest = at
				}
			}
		}
	}
	return c, nil
}

// Start starts one central collector even with no HTTP/SSE subscribers. Source
// reads run under a short deadline; no source goroutine is leaked on shutdown.
func (c *Collector) Start(ctx context.Context) {
	c.mu.Lock()
	if c.closed || c.started {
		c.mu.Unlock()
		return
	}
	workerCtx, cancel := context.WithCancel(ctx)
	c.cancel = cancel
	c.started = true
	c.done = make(chan struct{})
	c.mu.Unlock()
	go func() {
		defer close(c.done)
		sample := time.NewTicker(SampleInterval)
		defer sample.Stop()
		flush := time.NewTicker(FlushInterval)
		defer flush.Stop()
		c.collect(workerCtx)
		for {
			select {
			case <-workerCtx.Done():
				return
			case <-sample.C:
				c.collect(workerCtx)
			case <-flush.C:
				_ = c.Flush()
			}
		}
	}()
}

func (c *Collector) collect(ctx context.Context) {
	child, cancel := context.WithTimeout(ctx, 1500*time.Millisecond)
	snapshot, err := c.source.Snapshot(child)
	cancel()
	if err != nil {
		c.mu.Lock()
		c.previous = nil
		c.observationError = "WAN observation is unavailable."
		c.mu.Unlock()
		return
	}
	c.record(snapshot)
}

// record trusts only the exact selected default-route interface, not adapter
// rate fields or a sum of WAN bridge and physical interface counters.
func (c *Collector) record(s router.Snapshot) {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.closed {
		return
	}
	next, problem := wanObservation(s)
	if problem != "" {
		c.previous = nil
		c.sourceName = ""
		c.observationError = problem
		return
	}
	c.sourceName = next.source
	c.observationError = ""
	previous := c.previous
	c.previous = &next
	if previous == nil {
		return
	}
	elapsed := next.at.Sub(previous.at)
	// Stale adapter caches do not erase the last valid baseline. Wall-clock
	// regressions, long outages and raw counter resets do not invent a delta.
	if elapsed == 0 {
		c.previous = previous
		return
	}
	if elapsed < 0 || elapsed > 5*SampleInterval || next.source != previous.source || next.rx < previous.rx || next.tx < previous.tx {
		return
	}
	// Never overwrite measured intervals if the wall clock moved backwards.
	if previous.at.Before(c.latest) {
		return
	}
	rx, tx := next.rx-previous.rx, next.tx-previous.tx
	// A BE6500 cannot move 1 TiB/s. Reject corrupt/implausible deltas rather
	// than overflow byte sums or turn a replaced counter into a huge spike.
	if float64(rx)/elapsed.Seconds() > float64(uint64(1)<<40) || float64(tx)/elapsed.Seconds() > float64(uint64(1)<<40) {
		return
	}
	for _, r := range c.rings {
		r.add(previous.at, next.at, rx, tx)
	}
	c.latest = next.at
}

func wanObservation(s router.Snapshot) (observation, string) {
	if s.SampledAt.IsZero() || s.SampledAt.Unix() < 0 || s.SampledAt.Year() > 9999 {
		return observation{}, "WAN observation timestamp is invalid."
	}
	if len(s.Routes) > 4096 || len(s.Traffic) > 4096 {
		return observation{}, "WAN observation exceeded its interface limit."
	}
	// Invalid traffic/route data cannot establish trustworthy coverage.
	for _, e := range s.Errors {
		if e.Module == "traffic" {
			return observation{}, "WAN counters are unavailable or invalid."
		}
	}
	var route *router.Route
	for i := range s.Routes {
		r := &s.Routes[i]
		is4 := r.Family == "ipv4" && r.Destination == "0.0.0.0/0"
		is6 := r.Family == "ipv6" && r.Destination == "::/0"
		if !is4 && !is6 {
			continue
		}
		if route == nil || (r.Family == "ipv4" && route.Family != "ipv4") || (r.Family == route.Family && (r.Metric < route.Metric || (r.Metric == route.Metric && r.Interface < route.Interface))) {
			route = r
		}
	}
	if route == nil {
		return observation{}, "No default-route WAN interface is available."
	}
	for _, e := range s.Errors {
		if e.Module == "routes."+route.Family {
			return observation{}, "Default-route WAN observation is unavailable or invalid."
		}
	}
	if len(route.Interface) == 0 || len(route.Interface) > 64 {
		return observation{}, "Default-route WAN interface is invalid."
	}
	var found *router.Traffic
	for i := range s.Traffic {
		if s.Traffic[i].Interface != route.Interface {
			continue
		}
		if found != nil {
			return observation{}, "Default-route WAN counters are ambiguous."
		}
		found = &s.Traffic[i]
	}
	if found == nil {
		return observation{}, "Default-route WAN counters are unavailable."
	}
	return observation{at: s.SampledAt, source: route.Interface, rx: found.RXBytes, tx: found.TXBytes}, ""
}

// partBytes allocates each actual counter delta across crossed time buckets.
// Integer cumulative fractions make all pieces sum to the exact raw delta.
func partBytes(total, elapsed, duration uint64) uint64 {
	hi, lo := bits.Mul64(total, elapsed)
	result, _ := bits.Div64(hi, lo, duration)
	return result
}
func (r *ring) add(from, to time.Time, rx, tx uint64) {
	duration := uint64(to.Sub(from))
	rxPeak, txPeak := float64(rx)/to.Sub(from).Seconds(), float64(tx)/to.Sub(from).Seconds()
	cursor := from
	var assignedRX, assignedTX uint64
	for cursor.Before(to) {
		start := cursor.Unix() / r.seconds * r.seconds
		end := time.Unix(start+r.seconds, 0)
		if end.After(to) {
			end = to
		}
		cumulative := uint64(end.Sub(from))
		nextRX, nextTX := partBytes(rx, cumulative, duration), partBytes(tx, cumulative, duration)
		index := int((start / r.seconds) % int64(r.capacity))
		b := &r.buckets[index]
		if !b.valid || b.start != start {
			*b = bucket{start: start, generation: b.generation, valid: true}
		}
		coverage := uint64(end.Sub(cursor))
		if b.coverage+coverage <= uint64(r.seconds)*1e9 {
			b.rx += nextRX - assignedRX
			b.tx += nextTX - assignedTX
			b.coverage += coverage
			b.endMillis = uint32((end.Sub(time.Unix(start, 0)) + time.Millisecond - 1) / time.Millisecond)
			b.rxPeak = math.Max(b.rxPeak, rxPeak)
			b.txPeak = math.Max(b.txPeak, txPeak)
			r.dirty[index] = true
		}
		assignedRX = nextRX
		assignedTX = nextTX
		cursor = end
	}
}

// Flush writes only dirty slots and syncs once per changed ring. Automatic
// collection calls it at most once per minute; Close performs a final flush.
func (c *Collector) Flush() error {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.closed {
		return ErrClosed
	}
	return c.flushLocked()
}
func (c *Collector) flushLocked() error {
	changed := false
	for _, r := range c.rings {
		for _, dirty := range r.dirty {
			if dirty {
				changed = true
				break
			}
		}
	}
	for _, r := range c.rings {
		if err := r.flush(); err != nil {
			c.storageError = "Traffic history could not be synced to persistent storage."
			return err
		}
	}
	c.storageError = ""
	if changed {
		c.lastFlushAt = c.now().UTC()
	}
	return nil
}

// Close stops collection, waits for its bounded source read and flushes. It is
// idempotent. Callers must report a returned final-write error.
func (c *Collector) Close() error {
	c.mu.Lock()
	if c.closed {
		c.mu.Unlock()
		return nil
	}
	if c.cancel != nil {
		c.cancel()
	}
	done := c.done
	c.mu.Unlock()
	if done != nil {
		<-done
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.closed {
		return nil
	}
	err := c.flushLocked()
	for _, r := range c.rings {
		if closeErr := r.file.Close(); err == nil {
			err = closeErr
		}
	}
	c.closed = true
	return err
}

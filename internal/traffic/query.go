package traffic

import (
	"context"
	"time"
)

// Query uses one complete tier for the requested span, never overlapping tiers.
// Each tier received the same exact counter deltas during collection. Output
// downsampling sums bytes/coverage and takes maxima; it never sums rates.
// Range edges are rounded out to stored bucket boundaries (at most one native
// bucket) because arbitrary partial historical counter deltas are unknowable.
func (c *Collector) Query(ctx context.Context, name string, maxPoints int) (History, error) {
	if err := ValidateQuery(name, maxPoints); err != nil {
		return History{}, err
	}
	if err := ctx.Err(); err != nil {
		return History{}, err
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if err := ctx.Err(); err != nil {
		return History{}, err
	}
	if c.closed {
		return History{}, ErrClosed
	}
	h := History{Enabled: true, Persistent: c.storageError == "", RetentionDays: RetentionDays, Source: c.sourceName, Range: name, Samples: []Sample{}, MaxUnsyncedSeconds: int64(FlushInterval / time.Second)}
	if !c.lastFlushAt.IsZero() {
		lastFlushAt := c.lastFlushAt
		h.LastFlushAt = &lastFlushAt
	}
	if c.storageError != "" {
		h.Error = c.storageError
	} else if c.observationError != "" {
		h.Error = c.observationError
	} else if c.recoveryWarning {
		h.Error = "Damaged history records were skipped; previous valid records were recovered."
	}
	now := c.now().UTC()
	duration := ranges[name]
	r := c.rings[0]
	if duration > 48*time.Hour {
		r = c.rings[1]
	}
	if duration > 30*24*time.Hour {
		r = c.rings[2]
	}
	first := now.Add(-duration).Unix() / r.seconds * r.seconds
	last := now.Unix() / r.seconds * r.seconds
	nativeCount := (last-first)/r.seconds + 1
	factor := (nativeCount + int64(maxPoints) - 1) / int64(maxPoints)
	resolution := factor * r.seconds
	h.ResolutionSeconds = resolution
	outputCount := (nativeCount + factor - 1) / factor
	h.Samples = make([]Sample, int(outputCount))
	for i := range h.Samples {
		h.Samples[i].Time = time.Unix(first+int64(i)*resolution, 0).UTC()
	}
	// oldestAt is the earliest retained real coverage, not a fabricated start.
	for _, tier := range c.rings {
		cutoff := now.Unix() - tier.seconds*int64(tier.capacity)
		for i, b := range tier.buckets {
			if i%256 == 0 {
				if err := ctx.Err(); err != nil {
					return History{}, err
				}
			}
			if b.valid && b.coverage > 0 && b.start >= cutoff && b.start <= now.Unix() {
				at := time.Unix(b.start, 0).UTC()
				if h.OldestAt == nil || at.Before(*h.OldestAt) {
					h.OldestAt = &at
				}
			}
		}
	}
	for i, b := range r.buckets {
		if i%256 == 0 {
			if err := ctx.Err(); err != nil {
				return History{}, err
			}
		}
		if !b.valid || b.start < first || b.start > last {
			continue
		}
		s := &h.Samples[(b.start-first)/resolution]
		s.RXBytes += b.rx
		s.TXBytes += b.tx
		s.CoverageSeconds += float64(b.coverage) / 1e9
		if b.rxPeak > s.RXPeak {
			s.RXPeak = b.rxPeak
		}
		if b.txPeak > s.TXPeak {
			s.TXPeak = b.txPeak
		}
		h.Summary.RXBytes += b.rx
		h.Summary.TXBytes += b.tx
		h.Summary.CoverageSeconds += float64(b.coverage) / 1e9
	}
	for i := range h.Samples {
		s := &h.Samples[i]
		if s.CoverageSeconds > 0 {
			s.RX = float64(s.RXBytes) / s.CoverageSeconds
			s.TX = float64(s.TXBytes) / s.CoverageSeconds
		}
	}
	return h, nil
}

package devicetelemetry

import (
	"context"
	"sort"
	"strings"
	"time"
	"unicode/utf8"
)

var ranges = map[string]time.Duration{"30m": 30 * time.Minute, "24h": 24 * time.Hour, "7d": 7 * 24 * time.Hour}

func ValidateQuery(name string, maxPoints, limit int, search string) error {
	if _, ok := ranges[name]; !ok {
		return ErrQuery
	}
	if maxPoints < 1 || maxPoints > MaxPoints || limit < 1 || limit > MaxQueryDevices || utf8.RuneCountInString(search) > 64 || !validText(search, 256) {
		return ErrQuery
	}
	return nil
}

// Query reads collected memory only. It never calls the source or any RPC.
// Groups summarize every matched device before the returned top-row limit.
// Gaps serialize as null bytes, measured zero bytes retain real coverage.
func (c *Collector) Query(ctx context.Context, name string, maxPoints, limit int, search string) (History, error) {
	if err := ValidateQuery(name, maxPoints, limit, search); err != nil {
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
	h := History{Enabled: true, RetentionDays: RetentionDays, Source: "trafficd", Direction: "vendor-rx-tx", Range: name, State: "waiting", Devices: []Device{}, Groups: []Group{}, Error: c.observationError, Truncated: c.truncated}
	now := c.now().UTC()
	if !c.sampledAt.IsZero() {
		at := c.sampledAt
		h.SampledAt = &at
		h.State = "ok"
		if now.Sub(at) > StaleAfter {
			h.State = "stale"
		}
	}
	if c.sourceUnavailable {
		h.State = "unavailable"
	}
	seconds := fineSeconds
	if name != "30m" {
		seconds = coarseSeconds
	}
	first := now.Add(-ranges[name]).Unix() / seconds * seconds
	last := now.Unix() / seconds * seconds
	nativeCount := (last-first)/seconds + 1
	factor := (nativeCount + int64(maxPoints) - 1) / int64(maxPoints)
	resolution := seconds * factor
	h.ResolutionSeconds = resolution
	points := (nativeCount + factor - 1) / factor
	conflicts := map[string]int{}
	for _, d := range c.devices {
		if now.Sub(d.seen) > StaleAfter || !d.observation.Associated {
			continue
		}
		for _, counter := range d.observation.Counters {
			conflicts[counter.Address]++
		}
	}
	groups := map[string]*Group{}
	search = strings.ToLower(strings.TrimSpace(search))
	for id, d := range c.devices {
		if err := ctx.Err(); err != nil {
			return History{}, err
		}
		if now.Sub(d.seen) > RetentionDays*24*time.Hour {
			continue
		}
		h.DeviceCount++
		o := d.observation
		addresses := make([]string, 0, len(o.Counters))
		for _, counter := range o.Counters {
			addresses = append(addresses, counter.Address)
		}
		sort.Strings(addresses)
		text := strings.ToLower(id + " " + o.Name + " " + o.Interface + " " + strings.Join(addresses, " "))
		if search != "" && !strings.Contains(text, search) {
			continue
		}
		h.MatchedCount++
		stale := now.Sub(d.seen) > StaleAfter || h.State == "stale" || h.State == "unavailable" || (o.AgeingSeconds != nil && *o.AgeingSeconds > uint64(StaleAfter/time.Second))
		row := Device{ID: id, Name: o.Name, Addresses: addresses, Interface: o.Interface, Associated: o.Associated, LastSeen: d.seen, Stale: stale, OnlineSeconds: copyValue(o.OnlineSeconds), AgeingSeconds: copyValue(o.AgeingSeconds), Counters: []CurrentCounter{}, Links: copyLinks(o.Links), AddressConflicts: []string{}, Samples: make([]Point, int(points))}
		if !stale {
			row.RXBytesPerSecond = copyValue(d.rxRate)
			row.TXBytesPerSecond = copyValue(d.txRate)
		}
		if len(o.Counters) > 0 {
			var rx, tx uint64
			for _, counter := range o.Counters {
				rx += counter.RX
				tx += counter.TX
				row.Counters = append(row.Counters, CurrentCounter{Address: counter.Address, RXBytes: counter.RX, TXBytes: counter.TX})
				if conflicts[counter.Address] > 1 {
					row.AddressConflicts = append(row.AddressConflicts, counter.Address)
				}
			}
			row.RawRXBytes = &rx
			row.RawTXBytes = &tx
		}
		for i := range row.Samples {
			row.Samples[i].Time = time.Unix(first+int64(i)*resolution, 0).UTC()
		}
		ring := d.fine[:]
		if name != "30m" {
			ring = d.coarse[:]
		}
		for _, b := range ring {
			if b.coverage == 0 {
				continue
			}
			if b.start >= now.Add(-RetentionDays*24*time.Hour).Unix() && b.start <= last && (h.OldestAt == nil || b.start < h.OldestAt.Unix()) {
				at := time.Unix(b.start, 0).UTC()
				h.OldestAt = &at
			}
			if b.start < first || b.start > last {
				continue
			}
			p := &row.Samples[(b.start-first)/resolution]
			if p.RXBytes == nil {
				rx, tx := uint64(0), uint64(0)
				p.RXBytes = &rx
				p.TXBytes = &tx
			}
			*p.RXBytes += b.rx
			*p.TXBytes += b.tx
			p.CoverageSeconds += float64(b.coverage) / float64(time.Second)
			row.RXBytes += b.rx
			row.TXBytes += b.tx
			row.CoverageSeconds += float64(b.coverage) / float64(time.Second)
		}
		groupName := o.Interface
		if groupName == "" {
			groupName = "unreported"
		}
		group := groups[groupName]
		if group == nil {
			group = &Group{Name: groupName}
			groups[groupName] = group
		}
		group.CoverageSeconds += row.CoverageSeconds
		group.DeviceCount++
		group.RXBytes += row.RXBytes
		group.TXBytes += row.TXBytes
		if row.RXBytesPerSecond != nil && row.TXBytesPerSecond != nil {
			if group.RXBytesPerSecond == nil {
				rx, tx := float64(0), float64(0)
				group.RXBytesPerSecond = &rx
				group.TXBytesPerSecond = &tx
			}
			*group.RXBytesPerSecond += *row.RXBytesPerSecond
			*group.TXBytesPerSecond += *row.TXBytesPerSecond
		}
		h.Devices = append(h.Devices, row)
	}
	deviceOrder(h.Devices)
	if len(h.Devices) > limit {
		h.Truncated = true
		h.Devices = h.Devices[:limit]
	}
	for _, group := range groups {
		h.Groups = append(h.Groups, *group)
	}
	sort.Slice(h.Groups, func(i, j int) bool { return h.Groups[i].Name < h.Groups[j].Name })
	return h, nil
}

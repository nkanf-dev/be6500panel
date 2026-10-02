package router

import (
	"context"
	"errors"
	"sync"
)

// WANSource is a low-cost raw-counter source for central traffic collection.
// It reads only proc network counters and default routes. It never invokes
// commands or observes leases, WiFi, firewall, platform, or device identity.
// Its cache and mutex are independent of Adapter's full-observation cache.
type WANSource struct {
	adapter *Adapter
	mu      sync.Mutex
	cached  Snapshot
}

// NewWANSource shares the adapter's root and bounded file reader, not its full
// Snapshot work. Use this source for a traffic collector, not Adapter itself.
func NewWANSource(adapter *Adapter) *WANSource { return &WANSource{adapter: adapter} }

// Snapshot returns one exact default-route interface's raw byte counters.
// Rate fields remain zero: the consumer owns its counter baseline and coverage.
// Only cancellation is a top-level error; source failures are module errors.
func (s *WANSource) Snapshot(ctx context.Context) (Snapshot, error) {
	if err := ctx.Err(); err != nil {
		return Snapshot{}, err
	}
	if s.adapter == nil {
		return Snapshot{}, errors.New("WAN source requires a router adapter")
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if err := ctx.Err(); err != nil {
		return Snapshot{}, err
	}
	now := s.adapter.now()
	if !s.cached.SampledAt.IsZero() && now.Sub(s.cached.SampledAt) >= 0 && now.Sub(s.cached.SampledAt) < sampleInterval {
		return cloneSnapshot(s.cached), nil
	}
	snapshot := emptySnapshot(now)
	data, err := s.adapter.readFile("/proc/net/route", procLimit)
	if err != nil {
		snapshot.Errors = append(snapshot.Errors, moduleError("routes.ipv4", sourceCode(err)))
	} else {
		rows, bad := parseIPv4Routes(data)
		if bad {
			snapshot.Errors = append(snapshot.Errors, moduleError("routes.ipv4", "invalid"))
		}
		if len(rows) > 4096 {
			snapshot.Errors = append(snapshot.Errors, moduleError("routes.ipv4", "too_large"))
		} else if route, ok := chooseDefaultRoute(rows, "ipv4", "0.0.0.0/0"); ok {
			snapshot.Routes = append(snapshot.Routes, route)
		}
	}
	if err := ctx.Err(); err != nil {
		return snapshot, err
	}
	// IPv6 is a fallback, not another series added to an IPv4 physical/bridge.
	if len(snapshot.Routes) == 0 {
		data, err = s.adapter.readFile("/proc/net/ipv6_route", procLimit)
		if err != nil {
			snapshot.Errors = append(snapshot.Errors, moduleError("routes.ipv6", sourceCode(err)))
		} else {
			rows, bad := parseIPv6Routes(data)
			if bad {
				snapshot.Errors = append(snapshot.Errors, moduleError("routes.ipv6", "invalid"))
			}
			if len(rows) > 4096 {
				snapshot.Errors = append(snapshot.Errors, moduleError("routes.ipv6", "too_large"))
			} else if route, ok := chooseDefaultRoute(rows, "ipv6", "::/0"); ok {
				snapshot.Routes = append(snapshot.Routes, route)
			}
		}
	}
	if err := ctx.Err(); err != nil {
		return snapshot, err
	}
	data, err = s.adapter.readFile("/proc/net/dev", procLimit)
	if err != nil {
		snapshot.Errors = append(snapshot.Errors, moduleError("traffic", sourceCode(err)))
	} else {
		rows, bad := parseNetDev(data)
		if bad {
			snapshot.Errors = append(snapshot.Errors, moduleError("traffic", "invalid"))
		}
		if len(rows) > 4096 {
			snapshot.Errors = append(snapshot.Errors, moduleError("traffic", "too_large"))
		} else if len(snapshot.Routes) > 0 {
			for _, row := range rows {
				if row.Interface == snapshot.Routes[0].Interface {
					snapshot.Traffic = append(snapshot.Traffic, row)
				}
			}
		}
	}
	snapshot.SampledAt = s.adapter.now().UTC()
	if err := ctx.Err(); err != nil {
		return snapshot, err
	}
	s.cached = snapshot
	return cloneSnapshot(snapshot), nil
}

func chooseDefaultRoute(rows []Route, family, destination string) (Route, bool) {
	var selected Route
	found := false
	for _, row := range rows {
		if row.Family != family || row.Destination != destination {
			continue
		}
		if !found || row.Metric < selected.Metric || (row.Metric == selected.Metric && row.Interface < selected.Interface) {
			selected = row
			found = true
		}
	}
	return selected, found
}

package router

import (
	"context"
	"errors"
	"sync"

	"be6500panel/internal/devicetelemetry"
)

// DeviceSource reads one fixed, bounded trafficd hw detail source. It does not
// read credentials, change offload/capture, decrypt traffic, or accept browser
// command arguments. Its cache is independent of the full router snapshot.
type DeviceSource struct {
	adapter *Adapter
	mu      sync.Mutex
	cached  devicetelemetry.Snapshot
}

func NewDeviceSource(adapter *Adapter) *DeviceSource { return &DeviceSource{adapter: adapter} }
func (s *DeviceSource) Snapshot(ctx context.Context) (devicetelemetry.Snapshot, error) {
	if err := ctx.Err(); err != nil {
		return devicetelemetry.Snapshot{}, err
	}
	if s.adapter == nil {
		return devicetelemetry.Snapshot{}, errors.New("device source requires a router adapter")
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if err := ctx.Err(); err != nil {
		return devicetelemetry.Snapshot{}, err
	}
	now := s.adapter.now().UTC()
	if !s.cached.SampledAt.IsZero() && now.Sub(s.cached.SampledAt) >= 0 && now.Sub(s.cached.SampledAt) < sampleInterval {
		return cloneDeviceSnapshot(s.cached), nil
	}
	var data []byte
	var err error
	if s.adapter.live {
		data, err = readCommand(ctx, "ubus", "call", "trafficd", "hw", `{"detail":true,"wlan":true,"mlo":true}`)
	} else {
		data, err = s.adapter.readFile("/var/run/be6500panel/trafficd-hw.json", devicetelemetry.MaxSourceBytes)
	}
	if ctx.Err() != nil {
		return devicetelemetry.Snapshot{}, ctx.Err()
	}
	next := devicetelemetry.Snapshot{SampledAt: s.adapter.now().UTC(), Devices: []devicetelemetry.Observation{}}
	if err != nil {
		next.Error = "System traffic counters are unavailable (" + sourceCode(err) + ")."
	} else {
		next.Devices, next.Truncated, err = devicetelemetry.ParseTrafficd(data)
		if err != nil {
			next.Error = "Some system traffic counters are invalid; only valid device rows are shown."
		}
	}
	s.cached = next
	return cloneDeviceSnapshot(next), nil
}
func cloneDeviceSnapshot(s devicetelemetry.Snapshot) devicetelemetry.Snapshot {
	return devicetelemetry.CloneSnapshot(s)
}

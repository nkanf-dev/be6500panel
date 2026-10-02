package router

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"runtime"
	"sort"
	"strings"
	"sync"
	"time"
)

const sampleInterval = 2 * time.Second
const sourceInterval = 5 * time.Second

type counterSample struct {
	rx, tx uint64
	at     time.Time
}

// Adapter caches observations for concurrent callers. It starts no goroutine.
// A central application sampler should call Snapshot at a two-second cadence.
type Adapter struct {
	root        string
	live        bool
	mu          sync.Mutex
	now         func() time.Time
	cached      Snapshot
	slow        Snapshot
	lastSample  time.Time
	lastSources time.Time
	counters    map[string]counterSample
}

// New creates a read-only adapter. Empty root or "/" means the actual system.
// Any other root is a fixture tree and never executes host commands.
func New(root string) *Adapter {
	live := root == "" || root == "/"
	if root == "" {
		root = "/"
	}
	absolute, err := filepath.Abs(root)
	if err == nil {
		root = absolute
	}
	root = filepath.Clean(root)
	if resolved, err := filepath.EvalSymlinks(root); err == nil {
		root = resolved
	}
	return &Adapter{root: root, live: live, now: time.Now, counters: map[string]counterSample{}}
}

func emptySnapshot(now time.Time) Snapshot {
	return Snapshot{Devices: []Device{}, WiFi: []WiFi{}, DNS: DNS{Resolvers: []string{}}, Traffic: []Traffic{}, Routes: []Route{}, Errors: []ModuleError{}, SampledAt: now.UTC()}
}

// Snapshot preserves successful modules if another source fails. Only caller
// context cancellation/deadline is a top-level error; source errors are in Errors.
// Returned slices and lease timestamps are copies and may be changed by callers.
func (a *Adapter) Snapshot(ctx context.Context) (Snapshot, error) {
	if err := ctx.Err(); err != nil {
		return emptySnapshot(a.now()), err
	}
	a.mu.Lock()
	defer a.mu.Unlock()
	now := a.now()
	if err := ctx.Err(); err != nil {
		return emptySnapshot(now), err
	}
	if !a.lastSample.IsZero() && now.Sub(a.lastSample) >= 0 && now.Sub(a.lastSample) < sampleInterval {
		return cloneSnapshot(a.cached), nil
	}
	slow := a.slow
	if a.lastSources.IsZero() || now.Sub(a.lastSources) < 0 || now.Sub(a.lastSources) >= sourceInterval {
		slow = a.observeSources(ctx, now)
		if err := ctx.Err(); err != nil {
			return slow, err
		}
		a.slow = slow
		a.lastSources = a.now()
	}
	snapshot := cloneSnapshot(slow)
	data, err := a.readFile("/proc/net/dev", procLimit)
	now = a.now()
	snapshot.SampledAt = now.UTC()
	if err != nil {
		snapshot.Errors = append(snapshot.Errors, moduleError("traffic", sourceCode(err)))
	} else {
		rows, bad := parseNetDev(data)
		if bad {
			snapshot.Errors = append(snapshot.Errors, moduleError("traffic", "invalid"))
		}
		next := make(map[string]counterSample, len(rows))
		for i := range rows {
			previous, ok := a.counters[rows[i].Interface]
			elapsed := now.Sub(previous.at).Seconds()
			if ok && elapsed > 0 {
				if rows[i].RXBytes >= previous.rx {
					rows[i].RXBytesPerSecond = float64(rows[i].RXBytes-previous.rx) / elapsed
				}
				if rows[i].TXBytes >= previous.tx {
					rows[i].TXBytesPerSecond = float64(rows[i].TXBytes-previous.tx) / elapsed
				}
			}
			next[rows[i].Interface] = counterSample{rx: rows[i].RXBytes, tx: rows[i].TXBytes, at: now}
		}
		a.counters = next
		snapshot.Traffic = rows
	}
	if err := ctx.Err(); err != nil {
		return snapshot, err
	}
	a.cached = snapshot
	a.lastSample = now
	return cloneSnapshot(snapshot), nil
}

func cloneSnapshot(s Snapshot) Snapshot {
	s.Devices = append([]Device{}, s.Devices...)
	for i := range s.Devices {
		if s.Devices[i].ExpiresAt != nil {
			t := *s.Devices[i].ExpiresAt
			s.Devices[i].ExpiresAt = &t
		}
	}
	s.WiFi = append([]WiFi{}, s.WiFi...)
	s.DNS.Resolvers = append([]string{}, s.DNS.Resolvers...)
	s.Traffic = append([]Traffic{}, s.Traffic...)
	s.Routes = append([]Route{}, s.Routes...)
	s.Errors = append([]ModuleError{}, s.Errors...)
	return s
}

func (a *Adapter) observeSources(ctx context.Context, now time.Time) Snapshot {
	s := emptySnapshot(now)
	a.observePlatform(&s)
	observation, leaseCount, ipv4Routes, identityErrors := a.observeCaptureSources(ctx, now)
	s.Devices = observation.Devices
	s.DNS.LeaseCount = leaseCount
	s.Routes = append(s.Routes, ipv4Routes...)
	s.Errors = append(s.Errors, identityErrors...)
	data, err := a.readFile("/etc/config/wireless", fileLimit)
	if err != nil {
		s.Errors = append(s.Errors, moduleError("wifi", sourceCode(err)))
	} else {
		sections, bad := parseUCI(data, wirelessFields)
		rows, invalid := wirelessFromUCI(sections)
		s.WiFi = rows
		if bad || invalid {
			s.Errors = append(s.Errors, moduleError("wifi", "invalid"))
		}
	}
	// Prefer generated upstream DNS over the router's local resolver stub.
	data, err = a.firstFile([]string{"/tmp/resolv.conf.d/resolv.conf.auto", "/tmp/resolv.conf.auto", "/etc/resolv.conf"}, fileLimit)
	if err != nil {
		s.Errors = append(s.Errors, moduleError("dns", sourceCode(err)))
	} else {
		var bad bool
		s.DNS.Resolvers, bad = parseResolvers(data)
		if bad {
			s.Errors = append(s.Errors, moduleError("dns", "invalid"))
		}
	}
	for _, ipv6 := range []bool{false, true} {
		module := "firewall.ipv4"
		if ipv6 {
			module = "firewall.ipv6"
		}
		data, err = a.firewallSource(ctx, ipv6)
		if err != nil {
			s.Errors = append(s.Errors, moduleError(module, sourceCode(err)))
		} else {
			fw, bad := parseFirewall(data)
			if ipv6 {
				s.Firewall.IPv6 = fw
			} else {
				s.Firewall.IPv4 = fw
			}
			if bad {
				s.Errors = append(s.Errors, moduleError(module, "invalid"))
			}
		}
	}
	data, err = a.readFile("/proc/net/ipv6_route", procLimit)
	if err != nil {
		s.Errors = append(s.Errors, moduleError("routes.ipv6", sourceCode(err)))
	} else {
		rows, bad := parseIPv6Routes(data)
		s.Routes = append(s.Routes, rows...)
		if bad {
			s.Errors = append(s.Errors, moduleError("routes.ipv6", "invalid"))
		}
	}
	sort.Slice(s.Routes, func(i, j int) bool {
		x, y := s.Routes[i], s.Routes[j]
		if x.Family != y.Family {
			return x.Family < y.Family
		}
		if x.Destination != y.Destination {
			return x.Destination < y.Destination
		}
		if x.Interface != y.Interface {
			return x.Interface < y.Interface
		}
		return x.Metric < y.Metric
	})
	return s
}

func (a *Adapter) observePlatform(s *Snapshot) {
	data, err := a.firstFile([]string{"/usr/share/xiaoqiang/xiaoqiang_version", "/etc/config/version"}, fileLimit)
	var versionErr error
	if err == nil {
		sections, bad := parseUCI(data, versionFields)
		for _, section := range sections {
			if section.name == "version" || section.kind == "core" {
				s.Platform.Model = section.options["HARDWARE"]
				s.Platform.Firmware = section.options["ROM"]
				break
			}
		}
		if bad {
			s.Errors = append(s.Errors, moduleError("platform.version", "invalid"))
		}
	} else {
		versionErr = err
	}
	if s.Platform.Model == "" || s.Platform.Firmware == "" {
		data, err = a.firstFile([]string{"/etc/miwifi_version", "/etc/xiaoqiang_version"}, fileLimit)
		if err == nil {
			values, bad := parseAssignments(data, versionFields)
			if s.Platform.Model == "" {
				s.Platform.Model = values["HARDWARE"]
			}
			if s.Platform.Firmware == "" {
				s.Platform.Firmware = values["ROM"]
			}
			if bad {
				s.Errors = append(s.Errors, moduleError("platform.version", "invalid"))
			}
		} else if versionErr == nil {
			versionErr = err
		}
	}
	if s.Platform.Model == "" {
		if data, err = a.readFile("/tmp/sysinfo/model", fileLimit); err == nil {
			value := strings.TrimSpace(string(data))
			if safeString(value, 256) {
				s.Platform.Model = value
			}
		}
	}
	// OpenWrt release describes the userspace target, which is useful on RN02's
	// ARMv7 userspace even when the SoC itself supports ARM64.
	data, err = a.readFile("/etc/openwrt_release", fileLimit)
	if err == nil {
		values, bad := parseAssignments(data, map[string]bool{"DISTRIB_ARCH": true, "DISTRIB_RELEASE": true})
		s.Platform.Architecture = values["DISTRIB_ARCH"]
		if s.Platform.Firmware == "" {
			s.Platform.Firmware = values["DISTRIB_RELEASE"]
		}
		if bad {
			s.Errors = append(s.Errors, moduleError("platform.release", "invalid"))
		}
	} else if !errors.Is(err, os.ErrNotExist) {
		s.Errors = append(s.Errors, moduleError("platform.release", sourceCode(err)))
	}
	if s.Platform.Model == "" || s.Platform.Firmware == "" {
		code := "invalid"
		if versionErr != nil {
			code = sourceCode(versionErr)
		}
		s.Errors = append(s.Errors, moduleError("platform.version", code))
	}
	data, err = a.readFile("/proc/sys/kernel/osrelease", fileLimit)
	if err != nil {
		s.Errors = append(s.Errors, moduleError("platform.kernel", sourceCode(err)))
	} else {
		value := strings.TrimSpace(string(data))
		if value != "" && safeString(value, 256) {
			s.Platform.Kernel = value
		} else {
			s.Errors = append(s.Errors, moduleError("platform.kernel", "invalid"))
		}
	}
	if s.Platform.Architecture == "" {
		if a.live {
			s.Platform.Architecture = runtime.GOARCH
		} else {
			s.Errors = append(s.Errors, moduleError("platform.architecture", "unavailable"))
		}
	}
}

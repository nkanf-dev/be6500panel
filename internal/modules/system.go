package modules

import (
	"context"
	"errors"
	"fmt"
	"io"
	"math"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"time"

	"be6500panel/internal/core"
)

// System keeps the Linux /proc adapter outside the core contracts.
type System struct {
	demo     bool
	procRoot string
}

func NewSystem(demo bool) *System { return &System{demo: demo, procRoot: "/proc"} }
func (s *System) Mode() string {
	if s.demo {
		return "demo"
	}
	return "host"
}
func (s *System) Descriptor() core.Module {
	supported := s.demo || runtime.GOOS == "linux"
	reason := ""
	if !supported {
		reason = "Host system metrics require readable Linux /proc; use explicit --demo on other platforms."
	}
	return core.Module{ID: "system", Title: "System", Description: "Read-only current-host metrics, or explicitly selected demo samples.", State: state(supported), Capabilities: []core.Capability{{ID: "observe", Title: "System observation", Supported: supported, Reason: reason}}}
}
func (s *System) Observe(ctx context.Context) (core.SystemStatus, error) {
	if err := ctx.Err(); err != nil {
		return core.SystemStatus{}, err
	}
	now := time.Now().UTC()
	if s.demo {
		return core.SystemStatus{Mode: "demo", Hostname: "be6500panel-demo", OS: "linux", Arch: "arm", Kernel: "demo (not a device observation)", UptimeSeconds: 86400, CPUCount: 4, Memory: core.Memory{TotalBytes: 512 * 1024 * 1024, AvailableBytes: 320 * 1024 * 1024}, Load: [3]float64{.12, .18, .15}, SampledAt: now}, nil
	}
	if runtime.GOOS != "linux" {
		return core.SystemStatus{}, errors.New("host system observation requires Linux /proc; select --demo explicitly for simulation")
	}
	return s.observeProc(ctx, now)
}

// observeProc is the Linux adapter seam. Tests use synthetic proc fixtures; the
// running server always reads the real /proc and never a client-supplied path.
func (s *System) observeProc(ctx context.Context, now time.Time) (core.SystemStatus, error) {
	if err := ctx.Err(); err != nil {
		return core.SystemStatus{}, err
	}
	hostname, err := os.Hostname()
	if err != nil {
		return core.SystemStatus{}, errors.New("cannot observe host name")
	}
	kernel, err := readProc(s.procRoot, "sys/kernel/osrelease")
	if err != nil {
		return core.SystemStatus{}, err
	}
	uptimeText, err := readProc(s.procRoot, "uptime")
	if err != nil {
		return core.SystemStatus{}, err
	}
	uptimeFields := strings.Fields(uptimeText)
	if len(uptimeFields) < 1 {
		return core.SystemStatus{}, errors.New("invalid /proc/uptime")
	}
	uptime, err := parseNonnegative(uptimeFields[0])
	if err != nil {
		return core.SystemStatus{}, errors.New("invalid /proc/uptime")
	}
	loadText, err := readProc(s.procRoot, "loadavg")
	if err != nil {
		return core.SystemStatus{}, err
	}
	loads, err := parseLoad(loadText)
	if err != nil {
		return core.SystemStatus{}, err
	}
	memoryText, err := readProc(s.procRoot, "meminfo")
	if err != nil {
		return core.SystemStatus{}, err
	}
	memory, err := parseMemory(memoryText)
	if err != nil {
		return core.SystemStatus{}, err
	}
	return core.SystemStatus{Mode: "host", Hostname: hostname, OS: runtime.GOOS, Arch: runtime.GOARCH, Kernel: strings.TrimSpace(kernel), UptimeSeconds: uptime, CPUCount: runtime.NumCPU(), Memory: memory, Load: loads, SampledAt: now}, nil
}
func readProc(root, name string) (string, error) {
	f, err := os.Open(filepath.Join(root, name))
	if err != nil {
		return "", fmt.Errorf("host observation unavailable: cannot read /proc/%s", name)
	}
	defer f.Close()
	b, err := io.ReadAll(io.LimitReader(f, 65537))
	if err != nil || len(b) > 65536 {
		return "", fmt.Errorf("host observation unavailable: invalid /proc/%s", name)
	}
	return string(b), nil
}
func parseNonnegative(text string) (float64, error) {
	v, e := strconv.ParseFloat(text, 64)
	if e != nil || math.IsNaN(v) || math.IsInf(v, 0) || v < 0 {
		return 0, errors.New("invalid metric")
	}
	return v, nil
}
func parseLoad(text string) ([3]float64, error) {
	var result [3]float64
	fields := strings.Fields(text)
	if len(fields) < 3 {
		return result, errors.New("invalid /proc/loadavg")
	}
	for i := range result {
		v, err := parseNonnegative(fields[i])
		if err != nil {
			return result, errors.New("invalid /proc/loadavg")
		}
		result[i] = v
	}
	return result, nil
}
func parseMemory(text string) (core.Memory, error) {
	values := map[string]uint64{}
	for _, line := range strings.Split(text, "\n") {
		fields := strings.Fields(line)
		if len(fields) == 0 {
			continue
		}
		key := strings.TrimSuffix(fields[0], ":")
		if key != "MemTotal" && key != "MemAvailable" {
			continue
		}
		if len(fields) != 3 || fields[2] != "kB" {
			return core.Memory{}, errors.New("invalid /proc/meminfo")
		}
		v, err := strconv.ParseUint(fields[1], 10, 64)
		if err != nil || v > math.MaxUint64/1024 {
			return core.Memory{}, errors.New("invalid /proc/meminfo")
		}
		values[key] = v * 1024
	}
	total, okT := values["MemTotal"]
	available, okA := values["MemAvailable"]
	if !okT || !okA || total == 0 || available > total {
		return core.Memory{}, errors.New("host memory metrics unavailable: MemTotal/MemAvailable required")
	}
	return core.Memory{TotalBytes: total, AvailableBytes: available}, nil
}
func state(supported bool) string {
	if supported {
		return "ready"
	}
	return "unavailable"
}

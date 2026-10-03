package router

import (
	"context"
	"encoding/binary"
	"encoding/json"
	"errors"
	"io"
	"math"
	"os"
	"path/filepath"
	"runtime"
	"sort"
	"strconv"
	"strings"
	"time"
)

const (
	serviceRows     = 128
	serviceInterval = 5 * time.Second
	serviceTimeout  = 3 * time.Second
	serviceFixture  = "/var/run/be6500panel/ubus-service-list.json"
)

// ServiceState keeps installation, procd registration and verified process
// observations separate. Neither installation nor registration implies health.
type ServiceState struct {
	Name          string   `json:"name"`
	Instance      string   `json:"instance"`
	Configured    string   `json:"configured"`
	Registered    string   `json:"registered"`
	ProcessState  string   `json:"processState"`
	ProcdRunning  *bool    `json:"procdRunning,omitempty"`
	ReportedPID   int      `json:"reportedPID,omitempty"`
	PID           int      `json:"pid,omitempty"`
	Executable    string   `json:"executable,omitempty"`
	StartTicks    uint64   `json:"startTicks,omitempty"`
	UptimeSeconds *float64 `json:"uptimeSeconds,omitempty"`
	RSSBytes      *uint64  `json:"rssBytes,omitempty"`
	ErrorCode     string   `json:"errorCode,omitempty"`
	Protected     bool     `json:"protected"`
	Actions       []string `json:"actions,omitempty"`
	ActionImpact  string   `json:"actionImpact,omitempty"`
}

type ServiceSnapshot struct {
	Source    string         `json:"source"`
	SampledAt *time.Time     `json:"sampledAt"`
	CheckedAt time.Time      `json:"checkedAt"`
	Stale     bool           `json:"stale"`
	ErrorCode string         `json:"errorCode,omitempty"`
	Services  []ServiceState `json:"services"`
	Errors    []ModuleError  `json:"errors"`
}

// ServiceObserver shares one bounded service snapshot across browser callers.
// It starts no goroutine and only visits PIDs reported by procd, never /proc as
// a whole. Any non-system root uses fixture files and never host commands.
type ServiceObserver struct {
	adapter     *Adapter
	gate        chan struct{}
	now         func() time.Time
	cached      ServiceSnapshot
	lastAttempt time.Time
	source      func(context.Context) ([]byte, error)
	read        func(string, int64) ([]byte, error)
	runAction   func(context.Context, string, string) error
	identities  map[string]processIdentity
}

func NewServiceObserver(root string) *ServiceObserver {
	a := New(root)
	o := &ServiceObserver{adapter: a, gate: make(chan struct{}, 1), now: time.Now, read: a.readFile}
	o.runAction = runServiceCommand
	o.source = func(ctx context.Context) ([]byte, error) {
		if !a.live {
			return a.readFile(serviceFixture, commandLimit)
		}
		// Fixed read-only command. No browser-selected argv or service names.
		return readCommand(ctx, "ubus", "call", "service", "list", "{}")
	}
	return o
}

func (o *ServiceObserver) Snapshot(ctx context.Context) (ServiceSnapshot, error) {
	select {
	case o.gate <- struct{}{}:
	case <-ctx.Done():
		return emptyServices(o.now()), ctx.Err()
	}
	defer func() { <-o.gate }()
	if err := ctx.Err(); err != nil {
		return emptyServices(o.now()), err
	}
	now := o.now()
	if !o.lastAttempt.IsZero() && now.Sub(o.lastAttempt) >= 0 && now.Sub(o.lastAttempt) < serviceInterval {
		return cloneServices(o.cached), nil
	}
	return o.refresh(ctx)
}

func emptyServices(now time.Time) ServiceSnapshot {
	return ServiceSnapshot{Source: "procd/ubus + proc", CheckedAt: now.UTC(), Stale: true, Services: []ServiceState{}, Errors: []ModuleError{}}
}
func cloneServices(s ServiceSnapshot) ServiceSnapshot {
	s.Services = append([]ServiceState{}, s.Services...)
	for i := range s.Services {
		r := &s.Services[i]
		r.Actions = append([]string(nil), r.Actions...)
		if r.ProcdRunning != nil {
			v := *r.ProcdRunning
			r.ProcdRunning = &v
		}
		if r.UptimeSeconds != nil {
			v := *r.UptimeSeconds
			r.UptimeSeconds = &v
		}
		if r.RSSBytes != nil {
			v := *r.RSSBytes
			r.RSSBytes = &v
		}
	}
	s.Errors = append([]ModuleError{}, s.Errors...)
	if s.SampledAt != nil {
		v := *s.SampledAt
		s.SampledAt = &v
	}
	return s
}
func (o *ServiceObserver) refresh(ctx context.Context) (ServiceSnapshot, error) {
	child, cancel := context.WithTimeout(ctx, serviceTimeout)
	defer cancel()
	now := o.now()
	s := emptyServices(now)
	data, err := o.source(child)
	var rows []serviceInstance
	if err == nil {
		rows, err = parseServiceList(data)
	}
	if err != nil {
		// Keep last successful evidence, but never give it a fresh timestamp or
		// pretend that a lost source means all services stopped.
		if o.cached.SampledAt != nil {
			s = cloneServices(o.cached)
		}
		s.CheckedAt = now.UTC()
		s.Stale = true
		s.ErrorCode = sourceCode(err)
		if errors.Is(err, errServiceInvalid) {
			s.ErrorCode = "invalid"
		}
		s.Errors = []ModuleError{moduleError("services.procd", s.ErrorCode)}
	} else {
		installed, installErr := o.installedServices()
		if installErr != nil {
			s.Errors = append(s.Errors, moduleError("services.installed", sourceCode(installErr)))
		}
		uptime, ticks, timingErr := o.processClock()
		if timingErr != nil {
			s.Errors = append(s.Errors, moduleError("services.proc_timing", sourceCode(timingErr)))
		}
		registered := map[string]bool{}
		nextIdentities := map[string]processIdentity{}
		for _, instance := range rows {
			if err = child.Err(); err != nil {
				break
			}
			registered[instance.name] = true
			r := ServiceState{Name: instance.name, Instance: instance.instance, Configured: installedState(instance.name, installed, installErr), Registered: "registered", ProcessState: "unknown", ProcdRunning: instance.running, ReportedPID: instance.pid, Protected: protectedService(instance.name)}
			o.verifyProcess(child, &r, instance, uptime, ticks)
			o.checkIdentity(&r, nextIdentities)
			s.Services = append(s.Services, r)
		}
		// Show known native and rescue entries even when they have no procd row.
		names := map[string]bool{}
		for name := range installed {
			names[name] = true
		}
		for _, name := range []string{"dnsmasq", "dropbear", "ddns", "be6500panel", "be6500-rescue", "rescue"} {
			names[name] = true
		}
		for _, name := range sortedKeys(names) {
			if registered[name] {
				continue
			}
			if len(s.Services) >= serviceRows {
				err = errTooLarge
				break
			}
			s.Services = append(s.Services, ServiceState{Name: name, Instance: "", Configured: installedState(name, installed, installErr), Registered: "unregistered", ProcessState: "unknown", Protected: protectedService(name)})
		}
		sort.Slice(s.Services, func(i, j int) bool {
			if s.Services[i].Name != s.Services[j].Name {
				return s.Services[i].Name < s.Services[j].Name
			}
			return s.Services[i].Instance < s.Services[j].Instance
		})
		if err == nil {
			o.identities = nextIdentities
			sampled := o.now().UTC()
			s.SampledAt = &sampled
			s.Stale = false
		}
	}
	if err == nil {
		err = child.Err()
	}
	if err != nil && s.ErrorCode == "" {
		s.Stale = true
		s.ErrorCode = sourceCode(err)
		s.Errors = append(s.Errors, moduleError("services", s.ErrorCode))
	}
	if ctx.Err() != nil {
		return s, ctx.Err()
	}
	o.attachActions(&s)
	o.cached = cloneServices(s)
	o.lastAttempt = o.now()
	return cloneServices(s), nil
}
func installedState(name string, names map[string]bool, err error) string {
	if err != nil {
		return "unknown"
	}
	if names[name] {
		return "present"
	}
	return "absent"
}
func protectedService(name string) bool {
	return name == "rescue" || name == "be6500-rescue" || name == "be6500panel" || name == "dropbear"
}
func (o *ServiceObserver) rootPath(path string, resolveLeaf bool) (string, error) {
	full := filepath.Join(o.adapter.root, filepath.FromSlash(strings.TrimPrefix(path, "/")))
	if o.adapter.live {
		return full, nil
	}
	check := full
	if !resolveLeaf {
		check = filepath.Dir(full)
	}
	resolved, err := filepath.EvalSymlinks(check)
	if err != nil {
		return "", err
	}
	relative, err := filepath.Rel(o.adapter.root, resolved)
	if err != nil || relative == ".." || strings.HasPrefix(relative, ".."+string(filepath.Separator)) {
		return "", errOutsideRoot
	}
	if !resolveLeaf {
		resolved = filepath.Join(resolved, filepath.Base(full))
	}
	return resolved, nil
}
func (o *ServiceObserver) installedServices() (map[string]bool, error) {
	out := map[string]bool{}
	path, err := o.rootPath("/etc/init.d", true)
	if err != nil {
		return out, err
	}
	dir, err := os.Open(path)
	if err != nil {
		return out, err
	}
	defer dir.Close()
	entries, err := dir.ReadDir(serviceRows + 1)
	if err != nil && !errors.Is(err, io.EOF) {
		return out, err
	}
	if len(entries) > serviceRows {
		return out, errTooLarge
	}
	for _, entry := range entries {
		name := entry.Name()
		if !validServiceName(name) {
			continue
		}
		p, e := o.rootPath("/etc/init.d/"+name, true)
		if e != nil {
			return out, e
		}
		info, e := os.Stat(p)
		if e != nil {
			return out, e
		}
		if info.Mode().IsRegular() {
			out[name] = true
		}
	}
	return out, nil
}

var errServiceInvalid = errors.New("invalid service observation")

type serviceInstance struct {
	name, instance, command string
	running                 *bool
	pid                     int
	exitCode                *int
}

func validServiceName(name string) bool {
	if name == "" || len(name) > 64 {
		return false
	}
	for _, r := range name {
		if !(r >= 'a' && r <= 'z' || r >= 'A' && r <= 'Z' || r >= '0' && r <= '9' || r == '_' || r == '-' || r == '.') {
			return false
		}
	}
	return name != "." && name != ".."
}

// Detect duplicate fields rather than silently trusting the last PID/command.
func serviceObject(data []byte) (map[string]json.RawMessage, error) {
	d := json.NewDecoder(strings.NewReader(string(data)))
	token, err := d.Token()
	if err != nil || token != json.Delim('{') {
		return nil, errServiceInvalid
	}
	out := map[string]json.RawMessage{}
	for d.More() {
		key, err := d.Token()
		if err != nil {
			return nil, errServiceInvalid
		}
		name, ok := key.(string)
		if !ok {
			return nil, errServiceInvalid
		}
		if _, ok = out[name]; ok {
			return nil, errServiceInvalid
		}
		var raw json.RawMessage
		if d.Decode(&raw) != nil {
			return nil, errServiceInvalid
		}
		out[name] = raw
	}
	if token, err = d.Token(); err != nil || token != json.Delim('}') {
		return nil, errServiceInvalid
	}
	if _, err = d.Token(); err != io.EOF {
		return nil, errServiceInvalid
	}
	return out, nil
}
func parseServiceList(data []byte) ([]serviceInstance, error) {
	if len(data) > commandLimit {
		return nil, errTooLarge
	}
	services, err := serviceObject(data)
	if err != nil {
		return nil, err
	}
	if len(services) > serviceRows {
		return nil, errTooLarge
	}
	rows := []serviceInstance{}
	for _, name := range sortedRawKeys(services) {
		if !validServiceName(name) {
			return nil, errServiceInvalid
		}
		service, err := serviceObject(services[name])
		if err != nil {
			return nil, err
		}
		instances := map[string]json.RawMessage{}
		if raw, ok := service["instances"]; ok {
			instances, err = serviceObject(raw)
			if err != nil {
				return nil, err
			}
		}
		if len(instances) == 0 {
			if len(rows) >= serviceRows {
				return nil, errTooLarge
			}
			rows = append(rows, serviceInstance{name: name})
			continue
		}
		for _, instanceName := range sortedRawKeys(instances) {
			if !validServiceName(instanceName) {
				return nil, errServiceInvalid
			}
			if len(rows) >= serviceRows {
				return nil, errTooLarge
			}
			fields, err := serviceObject(instances[instanceName])
			if err != nil {
				return nil, err
			}
			instance := serviceInstance{name: name, instance: instanceName}
			if raw, ok := fields["running"]; ok {
				var running bool
				if json.Unmarshal(raw, &running) != nil || string(raw) == "null" {
					return nil, errServiceInvalid
				}
				instance.running = &running
			}
			if raw, ok := fields["pid"]; ok {
				if json.Unmarshal(raw, &instance.pid) != nil || instance.pid < 0 || instance.pid > 1<<22 || string(raw) == "null" {
					return nil, errServiceInvalid
				}
			}
			if raw, ok := fields["exit_code"]; ok {
				var code int
				if json.Unmarshal(raw, &code) != nil || code < 0 || code > 255 || string(raw) == "null" {
					return nil, errServiceInvalid
				}
				instance.exitCode = &code
			}
			if raw, ok := fields["command"]; ok {
				var command []string
				if json.Unmarshal(raw, &command) != nil || len(command) > 64 {
					return nil, errServiceInvalid
				}
				if len(command) > 0 {
					instance.command = command[0]
					if !safeString(instance.command, 512) || filepath.Clean(instance.command) != instance.command || (!filepath.IsAbs(instance.command) && !validServiceName(instance.command)) {
						return nil, errServiceInvalid
					}
				}
			}
			rows = append(rows, instance)
		}
	}
	return rows, nil
}
func sortedRawKeys(values map[string]json.RawMessage) []string {
	out := make([]string, 0, len(values))
	for key := range values {
		out = append(out, key)
	}
	sort.Strings(out)
	return out
}

// A repeated procd PID alone does not prove service identity after PID reuse.
// Keep the previous start token until procd publishes no PID or a different PID.
// This bounded map is replaced only after a complete list observation.
type processIdentity struct {
	pid   int
	start uint64
}

func (o *ServiceObserver) checkIdentity(row *ServiceState, next map[string]processIdentity) {
	key := row.Name + "/" + row.Instance
	previous, ok := o.identities[key]
	if ok && row.ReportedPID == previous.pid {
		next[key] = previous
		if row.ProcessState == "running" && row.StartTicks != previous.start {
			row.ProcessState = "unknown"
			row.ErrorCode = "pid_reused"
			row.PID = 0
			row.Executable = ""
			row.StartTicks = 0
			row.UptimeSeconds = nil
			row.RSSBytes = nil
		}
		return
	}
	if row.ProcessState == "running" {
		next[key] = processIdentity{pid: row.PID, start: row.StartTicks}
	}
}

type processStat struct {
	pid   int
	state string
	start uint64
}

func parseServiceStat(data []byte) (processStat, error) {
	text := strings.TrimSpace(string(data))
	start := strings.IndexByte(text, '(')
	end := strings.LastIndexByte(text, ')')
	if start < 1 || end <= start {
		return processStat{}, errServiceInvalid
	}
	pid, err := strconv.Atoi(strings.TrimSpace(text[:start]))
	if err != nil || pid <= 0 {
		return processStat{}, errServiceInvalid
	}
	fields := strings.Fields(text[end+1:])
	if len(fields) < 22 || len(fields[0]) != 1 || !strings.Contains("RSDZTWtXxIKP", fields[0]) {
		return processStat{}, errServiceInvalid
	}
	ticks, err := strconv.ParseUint(fields[19], 10, 64)
	if err != nil || ticks == 0 {
		return processStat{}, errServiceInvalid
	}
	return processStat{pid: pid, state: fields[0], start: ticks}, nil
}
func parseServiceRSS(data []byte, pid int) (uint64, error) {
	pidFound := false
	found := false
	var value uint64
	for _, line := range strings.Split(string(data), "\n") {
		if strings.HasPrefix(line, "Pid:") {
			fields := strings.Fields(line)
			if pidFound || len(fields) != 2 || fields[1] != strconv.Itoa(pid) {
				return 0, errServiceInvalid
			}
			pidFound = true
		}
		if strings.HasPrefix(line, "VmRSS:") {
			if found {
				return 0, errServiceInvalid
			}
			found = true
			fields := strings.Fields(line)
			if len(fields) != 3 || fields[2] != "kB" {
				return 0, errServiceInvalid
			}
			n, err := strconv.ParseUint(fields[1], 10, 64)
			if err != nil || n > math.MaxUint64/1024 {
				return 0, errServiceInvalid
			}
			value = n * 1024
		}
	}
	if !found || !pidFound {
		return 0, errServiceInvalid
	}
	return value, nil
}
func (o *ServiceObserver) processClock() (float64, uint64, error) {
	data, err := o.read("/proc/uptime", 4096)
	if err != nil {
		return 0, 0, err
	}
	fields := strings.Fields(string(data))
	if len(fields) != 2 {
		return 0, 0, errServiceInvalid
	}
	uptime, err := strconv.ParseFloat(fields[0], 64)
	if err != nil || math.IsNaN(uptime) || math.IsInf(uptime, 0) || uptime < 0 {
		return 0, 0, errServiceInvalid
	}
	data, err = o.read("/proc/self/auxv", 4096)
	if err != nil {
		return 0, 0, err
	}
	word := 4
	if runtime.GOARCH == "amd64" || runtime.GOARCH == "arm64" || runtime.GOARCH == "riscv64" {
		word = 8
	}
	ticks, err := parseClockTicks(data, word)
	return uptime, ticks, err
}
func parseClockTicks(data []byte, word int) (uint64, error) {
	if (word != 4 && word != 8) || len(data)%(word*2) != 0 {
		return 0, errServiceInvalid
	}
	value := func(b []byte) uint64 {
		if word == 4 {
			return uint64(binary.LittleEndian.Uint32(b))
		}
		return binary.LittleEndian.Uint64(b)
	}
	for i := 0; i < len(data); i += word * 2 {
		tag := value(data[i : i+word])
		n := value(data[i+word : i+word*2])
		if tag == 0 {
			break
		}
		if tag == 17 {
			if n == 0 || n > 1<<20 {
				return 0, errServiceInvalid
			}
			return n, nil
		}
	}
	return 0, errServiceInvalid
}
func (o *ServiceObserver) executable(path string) (string, error) {
	full, err := o.rootPath(path, true)
	if err != nil {
		return "", err
	}
	resolved, err := filepath.EvalSymlinks(full)
	if err != nil {
		return "", err
	}
	if !o.adapter.live {
		relative, err := filepath.Rel(o.adapter.root, resolved)
		if err != nil || relative == ".." || strings.HasPrefix(relative, ".."+string(filepath.Separator)) {
			return "", errOutsideRoot
		}
		resolved = "/" + filepath.ToSlash(relative)
	}
	if !safeString(resolved, 512) {
		return "", errServiceInvalid
	}
	return resolved, nil
}

// procd on stock RN02 also reports bare executable names. Resolve those only
// through fixed system directories, retaining the same resolved /proc identity
// proof. Missing/ambiguous names leave this row unknown, not the entire list.
func (o *ServiceObserver) reportedExecutable(command string) (string, error) {
	if filepath.IsAbs(command) {
		return o.executable(command)
	}
	if !validServiceName(command) {
		return "", errServiceInvalid
	}
	resolved := ""
	for _, directory := range []string{"/usr/sbin", "/usr/bin", "/sbin", "/bin"} {
		candidate, err := o.executable(directory + "/" + command)
		if err != nil {
			continue
		}
		if resolved != "" && resolved != candidate {
			return "", errServiceInvalid
		}
		resolved = candidate
	}
	if resolved == "" {
		return "", os.ErrNotExist
	}
	return resolved, nil
}

func (o *ServiceObserver) verifyProcess(ctx context.Context, row *ServiceState, instance serviceInstance, uptime float64, ticks uint64) {
	if instance.pid <= 0 {
		if instance.exitCode != nil && *instance.exitCode != 0 {
			row.ProcessState = "failed"
			row.ErrorCode = "procd_exit"
		} else if instance.running != nil && !*instance.running {
			row.ProcessState = "not_running"
		} else {
			row.ErrorCode = "pid_unavailable"
		}
		return
	}
	if ctx.Err() != nil {
		row.ErrorCode = "timeout"
		return
	}
	path := "/proc/" + strconv.Itoa(instance.pid)
	first, err := o.read(path+"/stat", 4096)
	if err != nil {
		row.ErrorCode = sourceCode(err)
		if errors.Is(err, os.ErrNotExist) {
			row.ProcessState = "failed"
			row.ErrorCode = "process_exited"
		}
		return
	}
	before, err := parseServiceStat(first)
	if err != nil || before.pid != instance.pid {
		row.ErrorCode = "invalid_process"
		return
	}
	if before.state == "Z" || before.state == "X" || before.state == "x" {
		row.ProcessState = "failed"
		row.ErrorCode = "process_exited"
		return
	}
	if instance.command == "" {
		row.ErrorCode = "command_unavailable"
		return
	}
	expected, err := o.reportedExecutable(instance.command)
	if err != nil {
		row.ErrorCode = "executable_unavailable"
		return
	}
	actual, err := o.executable(path + "/exe")
	if err != nil {
		row.ErrorCode = sourceCode(err)
		return
	}
	if expected != actual {
		row.ErrorCode = "identity_mismatch"
		return
	}
	rssData, rssErr := o.read(path+"/status", 16<<10)
	var rss uint64
	if rssErr == nil {
		rss, rssErr = parseServiceRSS(rssData, instance.pid)
	}
	second, err := o.read(path+"/stat", 4096)
	if err != nil {
		row.ErrorCode = "process_changed"
		return
	}
	after, err := parseServiceStat(second)
	if err != nil || after.pid != before.pid || after.start != before.start || after.state == "Z" || after.state == "X" || after.state == "x" {
		row.ErrorCode = "process_changed"
		return
	}
	// Read executable again so an exec() or PID reuse during RSS reading cannot
	// attach evidence from another process to the procd service.
	executableAgain, err := o.executable(path + "/exe")
	if err != nil || executableAgain != actual {
		row.ErrorCode = "process_changed"
		return
	}
	if ctx.Err() != nil {
		row.ErrorCode = "timeout"
		return
	}
	row.ProcessState = "running"
	row.PID = before.pid
	row.Executable = actual
	row.StartTicks = before.start
	if rssErr == nil {
		row.RSSBytes = &rss
	} else {
		row.ErrorCode = "rss_unavailable"
	}
	if ticks > 0 {
		seconds := uptime - float64(before.start)/float64(ticks)
		if seconds >= 0 && !math.IsInf(seconds, 0) {
			row.UptimeSeconds = &seconds
		} else {
			row.ErrorCode = "start_time_invalid"
		}
	}
	if instance.running != nil && !*instance.running {
		row.ErrorCode = "procd_state_mismatch"
	}
}

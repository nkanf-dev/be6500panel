package router

import (
	"context"
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"
)

func statFixture(pid int, state string, ticks uint64) string {
	fields := []string{state}
	for field := 4; field <= 24; field++ {
		value := "0"
		if field == 22 {
			value = strconv.FormatUint(ticks, 10)
		}
		fields = append(fields, value)
	}
	return fmt.Sprintf("%d (name with ) parens) %s\n", pid, strings.Join(fields, " "))
}
func serviceRoot(t *testing.T) string {
	t.Helper()
	root := t.TempDir()
	fixtureFile(t, root, serviceFixture, `{"dnsmasq":{"instances":{"main":{"running":true,"pid":42,"command":["/usr/sbin/dnsmasq","--secret=PRIVATE-ARG"]}}},"be6500-rescue":{"instances":{"main":{"running":false,"exit_code":7}}}}`)
	fixtureFile(t, root, "/etc/init.d/dnsmasq", "#!/bin/sh\n")
	fixtureFile(t, root, "/etc/init.d/be6500-rescue", "#!/bin/sh\n")
	fixtureFile(t, root, "/usr/sbin/dnsmasq", "synthetic executable")
	fixtureFile(t, root, "/usr/sbin/other", "other executable")
	fixtureFile(t, root, "/proc/42/stat", statFixture(42, "S", 1000))
	fixtureFile(t, root, "/proc/42/status", "Name:\tdnsmasq\nPid:\t42\nVmRSS:\t2048 kB\n")
	fixtureFile(t, root, "/proc/uptime", "500.00 99.00\n")
	fixtureFile(t, root, "/proc/self/auxv", "")
	word := 4
	if runtime.GOARCH == "arm64" || runtime.GOARCH == "amd64" || runtime.GOARCH == "riscv64" {
		word = 8
	}
	aux := make([]byte, word*4)
	if word == 4 {
		binary.LittleEndian.PutUint32(aux, 17)
		binary.LittleEndian.PutUint32(aux[word:], 100)
	} else {
		binary.LittleEndian.PutUint64(aux, 17)
		binary.LittleEndian.PutUint64(aux[word:], 100)
	}
	if err := os.WriteFile(filepath.Join(root, "proc/self/auxv"), aux, 0600); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink("../../usr/sbin/dnsmasq", filepath.Join(root, "proc/42/exe")); err != nil {
		t.Fatal(err)
	}
	return root
}
func serviceRow(t *testing.T, s ServiceSnapshot, name string) ServiceState {
	t.Helper()
	for _, r := range s.Services {
		if r.Name == name {
			return r
		}
	}
	t.Fatalf("no %s in %+v", name, s)
	return ServiceState{}
}
func TestServiceObservationVerifiesProcessAndProtectedRescue(t *testing.T) {
	o := NewServiceObserver(serviceRoot(t))
	s, err := o.Snapshot(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if s.Stale || s.SampledAt == nil || s.Source != "procd/ubus + proc" {
		t.Fatalf("source %+v", s)
	}
	r := serviceRow(t, s, "dnsmasq")
	if r.Configured != "present" || r.Registered != "registered" || r.ProcessState != "running" || r.PID != 42 || r.ReportedPID != 42 || r.Executable != "/usr/sbin/dnsmasq" || r.StartTicks != 1000 || r.UptimeSeconds == nil || *r.UptimeSeconds != 490 || r.RSSBytes == nil || *r.RSSBytes != 2<<20 {
		t.Fatalf("bad process %+v", r)
	}
	rescue := serviceRow(t, s, "be6500-rescue")
	if !rescue.Protected || rescue.ProcessState != "failed" || rescue.ErrorCode != "procd_exit" || len(rescue.Actions) > 0 {
		t.Fatalf("rescue %+v", rescue)
	}
	unknown := serviceRow(t, s, "dropbear")
	if unknown.ProcessState != "unknown" || unknown.Registered != "unregistered" || unknown.Configured != "absent" {
		t.Fatalf("no fake stopped %+v", unknown)
	}
	raw, _ := json.Marshal(s)
	if strings.Contains(string(raw), "PRIVATE-ARG") {
		t.Fatal("leaked command arguments")
	}
}
func TestServiceStatesAreIndependentAndNeverGreenFromInstallation(t *testing.T) {
	root := serviceRoot(t)
	fixtureFile(t, root, serviceFixture, `{"dnsmasq":{"instances":{"main":{"running":false}}},"ddns":{"instances":{"main":{"running":true}}},"missing":{"instances":{"main":{"running":true,"pid":91,"command":["/bin/missing"]}}}}`)
	s, _ := NewServiceObserver(root).Snapshot(context.Background())
	if r := serviceRow(t, s, "dnsmasq"); r.Configured != "present" || r.ProcessState != "not_running" || r.PID != 0 {
		t.Fatalf("bad stopped %+v", r)
	}
	if r := serviceRow(t, s, "ddns"); r.ProcessState != "unknown" || r.ErrorCode != "pid_unavailable" {
		t.Fatalf("pid unknown %+v", r)
	}
	if r := serviceRow(t, s, "missing"); r.ProcessState != "failed" || r.ErrorCode != "process_exited" {
		t.Fatalf("exited %+v", r)
	}
}
func TestServiceProcessIdentityExitAndMissingDetails(t *testing.T) {
	for _, tc := range []struct {
		name, stat, status, exe string
		remove                  bool
		state, code             string
	}{
		{"wrong_executable", statFixture(42, "S", 1000), "Pid: 42\nVmRSS: 2 kB", "../../usr/sbin/other", false, "unknown", "identity_mismatch"},
		{"wrong_stat_pid", statFixture(50, "S", 1000), "Pid: 42\nVmRSS: 2 kB", "", false, "unknown", "invalid_process"},
		{"zombie", statFixture(42, "Z", 1000), "", "", false, "failed", "process_exited"},
		{"exited", "", "", "", true, "failed", "process_exited"},
		{"wrong_rss_identity", statFixture(42, "S", 1000), "Pid: 99\nVmRSS: 2 kB", "", false, "running", "rss_unavailable"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			root := serviceRoot(t)
			if tc.remove {
				os.Remove(filepath.Join(root, "proc/42/stat"))
			} else {
				fixtureFile(t, root, "/proc/42/stat", tc.stat)
			}
			fixtureFile(t, root, "/proc/42/status", tc.status)
			if tc.exe != "" {
				os.Remove(filepath.Join(root, "proc/42/exe"))
				os.Symlink(tc.exe, filepath.Join(root, "proc/42/exe"))
			}
			s, _ := NewServiceObserver(root).Snapshot(context.Background())
			r := serviceRow(t, s, "dnsmasq")
			if r.ProcessState != tc.state || r.ErrorCode != tc.code {
				t.Fatalf("%+v", r)
			}
			if tc.code == "rss_unavailable" && r.RSSBytes != nil {
				t.Fatal("invented RSS")
			}
		})
	}
}
func TestServicePIDReuseDuringObservation(t *testing.T) {
	o := NewServiceObserver(serviceRoot(t))
	original := o.read
	reads := 0
	o.read = func(path string, limit int64) ([]byte, error) {
		if path == "/proc/42/stat" {
			reads++
			if reads == 2 {
				return []byte(statFixture(42, "S", 2000)), nil
			}
		}
		return original(path, limit)
	}
	s, _ := o.Snapshot(context.Background())
	r := serviceRow(t, s, "dnsmasq")
	if r.ProcessState != "unknown" || r.ErrorCode != "process_changed" || r.PID != 0 || r.RSSBytes != nil {
		t.Fatalf("PID reuse leaked verified state %+v", r)
	}
}
func TestServiceParserBoundsAndRejectsMaliciousOutput(t *testing.T) {
	for _, data := range []string{`null`, `[]`, `{} {}`, `{"../bad":{}}`, `{"a":{"instances":{"bad/name":{}}}}`, `{"a":{"instances":{"main":{"pid":1,"pid":2}}}}`, `{"a":{"instances":{"main":{"pid":-1}}}}`, `{"a":{"instances":{"main":{"running":null}}}}`, `{"a":{"instances":{"main":{"command":["/a/../b"]}}}}`, `{"a":{"instances":{"main":{"command":["unsafe/path"]}}}}`, `{"a":{},"a":{}}`} {
		if _, err := parseServiceList([]byte(data)); err == nil {
			t.Fatalf("accepted %q", data)
		}
	}
	if _, err := parseServiceList([]byte(strings.Repeat(" ", commandLimit+1))); !errors.Is(err, errTooLarge) {
		t.Fatal(err)
	}
	rows := map[string]any{}
	for i := 0; i <= serviceRows; i++ {
		rows[fmt.Sprintf("service%d", i)] = map[string]any{}
	}
	data, _ := json.Marshal(rows)
	if _, err := parseServiceList(data); !errors.Is(err, errTooLarge) {
		t.Fatal("missing row bound", err)
	}
	instances := map[string]any{}
	for i := 0; i <= serviceRows; i++ {
		instances[fmt.Sprintf("p%d", i)] = map[string]any{}
	}
	data, _ = json.Marshal(map[string]any{"a": map[string]any{"instances": instances}})
	if _, err := parseServiceList(data); !errors.Is(err, errTooLarge) {
		t.Fatal("missing instance bound", err)
	}
}
func TestServiceFixtureCannotEscapeAndNeverCommands(t *testing.T) {
	for _, kind := range []string{"source", "proc", "exe", "init"} {
		t.Run(kind, func(t *testing.T) {
			root := serviceRoot(t)
			outside := t.TempDir()
			outsideFile := filepath.Join(outside, "external")
			os.WriteFile(outsideFile, []byte("PRIVATE-OUTSIDE"), 0600)
			var path string
			switch kind {
			case "source":
				path = serviceFixture
			case "proc":
				path = "/proc/42/stat"
			case "exe":
				path = "/proc/42/exe"
			case "init":
				path = "/etc/init.d/dnsmasq"
			}
			full := filepath.Join(root, strings.TrimPrefix(path, "/"))
			os.Remove(full)
			if err := os.Symlink(outsideFile, full); err != nil {
				t.Fatal(err)
			}
			o := NewServiceObserver(root)
			o.runAction = func(context.Context, string, string) error { t.Fatal("fixture executed command"); return nil }
			s, _ := o.Snapshot(context.Background())
			raw, _ := json.Marshal(s)
			if strings.Contains(string(raw), "PRIVATE-OUTSIDE") {
				t.Fatal("fixture read host")
			}
			if kind == "source" && (!s.Stale || s.ErrorCode != "permission_denied") {
				t.Fatalf("%+v", s)
			}
			if kind == "proc" || kind == "exe" {
				r := serviceRow(t, s, "dnsmasq")
				if r.ProcessState == "running" {
					t.Fatalf("escaped identity %+v", r)
				}
			}
			result, err := o.Action(context.Background(), ServiceActionRequest{Service: "ddns", Action: "start"})
			if err == nil || result.ErrorCode != "fixture_read_only" {
				t.Fatalf("fixture action %+v %v", result, err)
			}
		})
	}
}
func TestServiceSharedCacheTimeoutAndStaleTimestamp(t *testing.T) {
	o := NewServiceObserver(serviceRoot(t))
	now := time.Date(2026, 10, 3, 1, 2, 3, 0, time.UTC)
	o.now = func() time.Time { return now }
	source := o.source
	calls := 0
	o.source = func(ctx context.Context) ([]byte, error) { calls++; return source(ctx) }
	var wg sync.WaitGroup
	for i := 0; i < 20; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			if _, err := o.Snapshot(context.Background()); err != nil {
				t.Error(err)
			}
		}()
	}
	wg.Wait()
	if calls != 1 {
		t.Fatal("cache duplicated probes", calls)
	}
	before := cloneServices(o.cached)
	now = now.Add(serviceInterval)
	o.source = func(context.Context) ([]byte, error) { calls++; return nil, context.DeadlineExceeded }
	s, err := o.Snapshot(context.Background())
	if err != nil || !s.Stale || s.ErrorCode != "timeout" || s.SampledAt == nil || !s.SampledAt.Equal(*before.SampledAt) || !s.CheckedAt.Equal(now) {
		t.Fatalf("bad stale %+v %v", s, err)
	}
	if r := serviceRow(t, s, "dnsmasq"); r.PID != 42 {
		t.Fatal("lost evidence")
	}
	_, _ = o.Snapshot(context.Background())
	if calls != 2 {
		t.Fatal("failed probes not cached", calls)
	}
	// Returned pointers and slices are independent of the collector cache.
	*s.SampledAt = time.Time{}
	*s.Services[0].ProcdRunning = true
	s.Services = nil
	next, _ := o.Snapshot(context.Background())
	if next.SampledAt.IsZero() {
		t.Fatal("cache alias")
	}
}
func TestServiceCacheWaitRespectsCancellation(t *testing.T) {
	o := NewServiceObserver(serviceRoot(t))
	o.gate <- struct{}{}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	_, err := o.Snapshot(ctx)
	if !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
	<-o.gate
}
func TestServiceClockAndRSSParsers(t *testing.T) {
	if _, err := parseServiceStat([]byte("broken")); err == nil {
		t.Fatal("malformed stat")
	}
	for _, data := range []string{"Pid: 42\nVmRSS: 1 MB", "Pid: 42\nVmRSS: -1 kB", "Pid: 42\nVmRSS: 18446744073709551615 kB", "VmRSS: 1 kB", "Pid: 42\nVmRSS: 1 kB\nVmRSS: 2 kB"} {
		if _, err := parseServiceRSS([]byte(data), 42); err == nil {
			t.Fatal("bad RSS", data)
		}
	}
	for _, word := range []int{4, 8} {
		data := make([]byte, word*4)
		if word == 4 {
			binary.LittleEndian.PutUint32(data, 17)
			binary.LittleEndian.PutUint32(data[word:], 100)
		} else {
			binary.LittleEndian.PutUint64(data, 17)
			binary.LittleEndian.PutUint64(data[word:], 100)
		}
		ticks, err := parseClockTicks(data, word)
		if err != nil || ticks != 100 {
			t.Fatalf("clock %d %d %v", word, ticks, err)
		}
	}
}

func TestServicePIDReuseBetweenSnapshotsKeepsIdentityUnknown(t *testing.T) {
	root := serviceRoot(t)
	o := NewServiceObserver(root)
	now := time.Date(2026, 10, 3, 1, 2, 3, 0, time.UTC)
	o.now = func() time.Time { return now }
	first, _ := o.Snapshot(context.Background())
	if serviceRow(t, first, "dnsmasq").ProcessState != "running" {
		t.Fatal("initial process unverified")
	}
	fixtureFile(t, root, "/proc/42/stat", statFixture(42, "S", 2000))
	now = now.Add(serviceInterval)
	second, _ := o.Snapshot(context.Background())
	r := serviceRow(t, second, "dnsmasq")
	if r.ProcessState != "unknown" || r.ErrorCode != "pid_reused" || r.PID != 0 || r.RSSBytes != nil || r.UptimeSeconds != nil {
		t.Fatalf("PID reused same executable %+v", r)
	}
	now = now.Add(serviceInterval)
	third, _ := o.Snapshot(context.Background())
	if serviceRow(t, third, "dnsmasq").ProcessState != "unknown" {
		t.Fatal("stale procd PID regained green")
	}
	fixtureFile(t, root, serviceFixture, `{"dnsmasq":{"instances":{"main":{"running":false}}}}`)
	now = now.Add(serviceInterval)
	_, _ = o.Snapshot(context.Background())
	fixtureFile(t, root, serviceFixture, `{"dnsmasq":{"instances":{"main":{"running":true,"pid":42,"command":["/usr/sbin/dnsmasq"]}}}}`)
	now = now.Add(serviceInterval)
	final, _ := o.Snapshot(context.Background())
	if serviceRow(t, final, "dnsmasq").ProcessState != "running" {
		t.Fatal("fresh procd PID lifecycle remained blocked")
	}
	if len(o.identities) > serviceRows {
		t.Fatal("unbounded identities")
	}
}

func TestServiceVendorBareCommandsDoNotRejectEntireObservation(t *testing.T) {
	root := serviceRoot(t)
	fixtureFile(t, root, serviceFixture, `{"dnsmasq":{"instances":{"main":{"running":true,"pid":42,"command":["dnsmasq","--secret=PRIVATE-ARG"]}}},"miio_client":{"instances":{"main":{"running":true,"pid":91,"command":["miio_client","PRIVATE-TOKEN"]}}},"be6500-rescue":{"instances":{"main":{"running":false,"exit_code":7}}}}`)
	fixtureFile(t, root, "/proc/91/stat", statFixture(91, "S", 2000))
	fixtureFile(t, root, "/proc/91/status", "Pid: 91\nVmRSS: 2 kB\n")
	snapshot, err := NewServiceObserver(root).Snapshot(context.Background())
	if err != nil || snapshot.Stale || snapshot.SampledAt == nil {
		t.Fatal(snapshot, err)
	}
	row := serviceRow(t, snapshot, "dnsmasq")
	if row.ProcessState != "running" || row.PID != 42 || row.Executable != "/usr/sbin/dnsmasq" {
		t.Fatal(row)
	}
	missing := serviceRow(t, snapshot, "miio_client")
	if missing.ProcessState != "unknown" || missing.ErrorCode != "executable_unavailable" || missing.PID != 0 {
		t.Fatal(missing)
	}
	raw, _ := json.Marshal(snapshot)
	if strings.Contains(string(raw), "PRIVATE-") {
		t.Fatal("vendor argv leaked")
	}
	if !serviceRow(t, snapshot, "be6500-rescue").Protected {
		t.Fatal("rescue protection lost")
	}
}
func TestServiceBareCommandRequiresUniqueResolvedIdentity(t *testing.T) {
	for _, kind := range []string{"ambiguous", "wrong_executable", "outside_root"} {
		t.Run(kind, func(t *testing.T) {
			root := serviceRoot(t)
			fixtureFile(t, root, serviceFixture, `{"dnsmasq":{"instances":{"main":{"running":true,"pid":42,"command":["dnsmasq"]}}}}`)
			switch kind {
			case "ambiguous":
				fixtureFile(t, root, "/usr/bin/dnsmasq", "different synthetic executable")
			case "wrong_executable":
				os.Remove(filepath.Join(root, "proc/42/exe"))
				if err := os.Symlink("../../usr/sbin/other", filepath.Join(root, "proc/42/exe")); err != nil {
					t.Fatal(err)
				}
			case "outside_root":
				outside := filepath.Join(t.TempDir(), "executable")
				if err := os.WriteFile(outside, []byte("host"), 0700); err != nil {
					t.Fatal(err)
				}
				os.Remove(filepath.Join(root, "usr/sbin/dnsmasq"))
				if err := os.Symlink(outside, filepath.Join(root, "usr/sbin/dnsmasq")); err != nil {
					t.Fatal(err)
				}
			}
			snapshot, err := NewServiceObserver(root).Snapshot(context.Background())
			if err != nil || snapshot.Stale {
				t.Fatal(snapshot, err)
			}
			row := serviceRow(t, snapshot, "dnsmasq")
			if row.ProcessState != "unknown" || row.PID != 0 || row.Executable != "" {
				t.Fatal(kind, row)
			}
		})
	}
}

func TestServiceManagementDependenciesAreProtected(t *testing.T) {
	for _, name := range []string{"be6500-rescue", "dropbear", "be6500panel", "network", "wifi", "firewall"} {
		if !protectedService(name) {
			t.Fatalf("%s lacks protected tag", name)
		}
	}
	for _, name := range []string{"ddns", "dnsmasq", "trafficd"} {
		if protectedService(name) {
			t.Fatalf("%s incorrectly tagged protected", name)
		}
	}
}

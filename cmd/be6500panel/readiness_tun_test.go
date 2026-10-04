package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/netip"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"time"

	managedruntime "be6500panel/internal/runtime"
)

const tunReadinessFixtureJSON = `{"inbounds":[{"type":"mixed","tag":"mixed-in","listen":"127.0.0.1","listen_port":2080},{"type":"tun","tag":"tun-in","interface_name":"b6p-tun","address":["172.31.255.253/30"],"mtu":1500,"stack":"system","dns_mode":"disabled","auto_route":false,"auto_redirect":false,"udp_timeout":"2m","udp_nat_max":1024}],"route":{"rules":[{"ip_version":6,"outbound":"direct"}]}}`
const tunProcFixtureBase = "/proc/4321"
const tunExecutableFixture = "/run/be6500panel/runtime/.artifact-qualified"
const tunTCPFixtureHeader = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n"

func tunTCPFixtureLine(address string, port int, state, inode string) string {
	ip := netip.MustParseAddr(address).As4()
	return fmt.Sprintf("  0: %02X%02X%02X%02X:%04X 00000000:0000 %s 00000000:00000000 00:00000000 00000000 0 0 %s 1\n", ip[3], ip[2], ip[1], ip[0], port, state, inode)
}

func tunStatFixture(pid int, start uint64, state string) []byte {
	// Fields 3..22. The unusual comm catches parsers using Fields on all stat.
	fields := append([]string{state}, make([]string, 18)...)
	for i := 1; i < len(fields); i++ {
		fields[i] = "0"
	}
	fields = append(fields, fmt.Sprint(start))
	return []byte(fmt.Sprintf("%d (core name ) worker) %s\n", pid, strings.Join(fields, " ")))
}

type tunReadinessFixture struct {
	io          tunReadinessIO
	files       map[string][]byte
	links       map[string]string
	infos       map[string]os.FileInfo
	directories map[string][]os.DirEntry
	ifaces      []tunReadinessInterface
	status      managedruntime.Status
	statusErr   error
	readErrors  map[string]error
	linkErrors  map[string]error
	reads       []string
	probes      int
	onProbe     func()
	onRead      func(string)
}

func newTUNReadinessFixture(t *testing.T) *tunReadinessFixture {
	t.Helper()
	// Only ordinary temporary-file metadata is used for os.SameFile. All proc,
	// interface and socket observations are in memory; no core or TUN is run.
	dir := t.TempDir()
	if err := os.Chmod(dir, 0700); err != nil {
		t.Fatal(err)
	}
	exe := filepath.Join(dir, ".artifact-qualified")
	if err := os.WriteFile(exe, []byte("qualified inode metadata"), 0700); err != nil {
		t.Fatal(err)
	}
	info, err := os.Stat(exe)
	if err != nil {
		t.Fatal(err)
	}
	dirInfo, err := os.Stat(dir)
	if err != nil {
		t.Fatal(err)
	}
	f := &tunReadinessFixture{
		files: map[string][]byte{
			tunProcFixtureBase + "/stat":                tunStatFixture(4321, 123456, "S"),
			tunProcFixtureBase + "/fdinfo/7":            []byte("pos:\t0\nflags:\t0104002\nmnt_id:\t17\niff:\tb6p-tun\n"),
			tunProcFixtureBase + "/fdinfo/8":            []byte("pos:\t0\nflags:\t02004002\n"),
			"/proc/sys/net/ipv4/conf/b6p-tun/rp_filter": []byte("2\n"),
			"/proc/net/tcp":                             []byte(tunTCPFixtureHeader + tunTCPFixtureLine("172.31.255.253", 53, "0A", "222") + tunTCPFixtureLine("172.31.255.253", 35001, "0A", "111")),
		},
		links:       map[string]string{tunProcFixtureBase + "/exe": tunExecutableFixture, tunProcFixtureBase + "/fd/7": "/dev/net/tun", tunProcFixtureBase + "/fd/8": "socket:[111]"},
		infos:       map[string]os.FileInfo{tunExecutableFixture: info, tunProcFixtureBase + "/exe": info, filepath.Dir(tunExecutableFixture): dirInfo},
		directories: map[string][]os.DirEntry{tunProcFixtureBase + "/fd": {tunFixtureDirEntry("7"), tunFixtureDirEntry("8")}},
		ifaces:      []tunReadinessInterface{{name: "lo", up: true, mtu: 65536, addresses: []netip.Prefix{netip.MustParsePrefix("127.0.0.1/8")}}, {name: "b6p-tun", up: true, mtu: 1500, addresses: []netip.Prefix{netip.MustParsePrefix("172.31.255.253/30")}}},
		status:      managedruntime.Status{Service: managedruntime.SingBox, PID: 4321, Generation: 8, State: managedruntime.Starting, Configured: true, ArtifactAvailable: true, NeedsRecovery: true},
		readErrors:  map[string]error{}, linkErrors: map[string]error{},
	}
	f.io = tunReadinessIO{
		readFile: func(path string) ([]byte, error) {
			f.reads = append(f.reads, path)
			if f.onRead != nil {
				f.onRead(path)
			}
			if err := f.readErrors[path]; err != nil {
				return nil, err
			}
			value, ok := f.files[path]
			if !ok {
				return nil, os.ErrNotExist
			}
			return append([]byte(nil), value...), nil
		},
		readlink: func(path string) (string, error) {
			if err := f.linkErrors[path]; err != nil {
				return "", err
			}
			value, ok := f.links[path]
			if !ok {
				return "", os.ErrNotExist
			}
			return value, nil
		},
		stat: func(path string) (os.FileInfo, error) {
			value, ok := f.infos[path]
			if !ok {
				return nil, os.ErrNotExist
			}
			return value, nil
		},
		lstat: func(path string) (os.FileInfo, error) {
			value, ok := f.infos[path]
			if !ok {
				return nil, os.ErrNotExist
			}
			return value, nil
		},
		readDir: func(path string) ([]os.DirEntry, error) {
			value, ok := f.directories[path]
			if !ok {
				return nil, os.ErrNotExist
			}
			return value, nil
		},
		interfaces: func() ([]tunReadinessInterface, error) { return f.ifaces, nil },
		probeListeners: func(ctx context.Context, raw []byte) error {
			f.probes++
			if f.onProbe != nil {
				f.onProbe()
			}
			return ctx.Err()
		},
	}
	return f
}

func (f *tunReadinessFixture) getStatus() (managedruntime.Status, error) {
	return f.status, f.statusErr
}

type tunFixtureDirEntry string

func (d tunFixtureDirEntry) Name() string               { return string(d) }
func (d tunFixtureDirEntry) IsDir() bool                { return false }
func (d tunFixtureDirEntry) Type() os.FileMode          { return os.ModeSymlink }
func (d tunFixtureDirEntry) Info() (os.FileInfo, error) { return nil, os.ErrNotExist }

func TestNativeTUNReadinessTarget(t *testing.T) {
	target, err := nativeTUNReadinessTarget([]byte(tunReadinessFixtureJSON))
	if err != nil || target == nil || target.name != "b6p-tun" || target.address.String() != "172.31.255.253/30" {
		t.Fatalf("target = %#v, err = %v", target, err)
	}
	legacy := []byte(`{"inbounds":[{"type":"tproxy","tag":"tproxy-in","listen_port":7893},{"type":"mixed","listen_port":2080}]}`)
	if target, err := nativeTUNReadinessTarget(legacy); err != nil || target != nil {
		t.Fatalf("legacy target = %#v, err = %v", target, err)
	}
	stringAddress := strings.Replace(tunReadinessFixtureJSON, `["172.31.255.253/30"]`, `"172.31.255.253/30"`, 1)
	if _, err := nativeTUNReadinessTarget([]byte(stringAddress)); err == nil {
		t.Fatal("accepted non-compiler address shape")
	}
}

func changeTUNReadinessConfig(t *testing.T, change func(map[string]any, map[string]any)) []byte {
	t.Helper()
	var config map[string]any
	if err := json.Unmarshal([]byte(tunReadinessFixtureJSON), &config); err != nil {
		t.Fatal(err)
	}
	inbound := config["inbounds"].([]any)[1].(map[string]any)
	change(inbound, config)
	raw, err := json.Marshal(config)
	if err != nil {
		t.Fatal(err)
	}
	return raw
}

func TestNativeTUNReadinessRejectsWrongAcceptedLane(t *testing.T) {
	tests := []struct {
		name   string
		change func(map[string]any, map[string]any)
	}{
		{"wrong-type", func(i, c map[string]any) { i["type"] = "direct" }},
		{"wrong-tag", func(i, c map[string]any) { i["tag"] = "other" }},
		{"wrong-stack", func(i, c map[string]any) { i["stack"] = "gvisor" }},
		{"dns-enabled", func(i, c map[string]any) { i["dns_mode"] = "hijack" }},
		{"wrong-mtu", func(i, c map[string]any) { i["mtu"] = 1400 }},
		{"auto-route", func(i, c map[string]any) { i["auto_route"] = true }},
		{"auto-redirect", func(i, c map[string]any) { i["auto_redirect"] = true }},
		{"missing-auto-route", func(i, c map[string]any) { delete(i, "auto_route") }},
		{"missing-auto-redirect", func(i, c map[string]any) { delete(i, "auto_redirect") }},
		{"legacy-address", func(i, c map[string]any) { i["inet4_address"] = i["address"]; delete(i, "address") }},
		{"legacy-dns", func(i, c map[string]any) { i["dns_hijack"] = []any{} }},
		{"mapping-number", func(i, c map[string]any) { i["udp_mapping"] = 4 }},
		{"listener-fields", func(i, c map[string]any) { i["listen_port"] = 7893 }},
		{"netns", func(i, c map[string]any) { i["netns"] = "foreign" }},
		{"route-address", func(i, c map[string]any) { i["route_address"] = []any{} }},
		{"include-uid", func(i, c map[string]any) { i["include_uid"] = []any{} }},
		{"uppercase-alias", func(i, c map[string]any) { i["MTU"] = i["mtu"]; delete(i, "mtu") }},
		{"wildcard-name", func(i, c map[string]any) { i["interface_name"] = "b6p-+" }},
		{"management-name", func(i, c map[string]any) { i["interface_name"] = "br-lan" }},
		{"dot-name", func(i, c map[string]any) { i["interface_name"] = "b6p-tun.1" }},
		{"colon-name", func(i, c map[string]any) { i["interface_name"] = "b6p-tun:1" }},
		{"long-name", func(i, c map[string]any) { i["interface_name"] = "b6p-123456789012" }},
		{"public-address", func(i, c map[string]any) { i["address"] = []any{"203.0.113.1/30"} }},
		{"cgnat-address", func(i, c map[string]any) { i["address"] = []any{"100.64.0.1/30"} }},
		{"mapped-address", func(i, c map[string]any) { i["address"] = []any{"::ffff:172.31.255.253/126"} }},
		{"ipv6-address", func(i, c map[string]any) { i["address"] = []any{"fd00::1/126"} }},
		{"network-address", func(i, c map[string]any) { i["address"] = []any{"172.31.255.252/30"} }},
		{"second-host", func(i, c map[string]any) { i["address"] = []any{"172.31.255.254/30"} }},
		{"broadcast", func(i, c map[string]any) { i["address"] = []any{"172.31.255.255/30"} }},
		{"wrong-prefix", func(i, c map[string]any) { i["address"] = []any{"172.31.255.253/32"} }},
		{"multiple-addresses", func(i, c map[string]any) { i["address"] = []any{"172.31.255.253/30", "10.0.0.1/30"} }},
		{"wrong-timeout", func(i, c map[string]any) { i["udp_timeout"] = "60s" }},
		{"wrong-nat-max", func(i, c map[string]any) { i["udp_nat_max"] = 8192 }},
		{"missing-ipv6-policy", func(i, c map[string]any) { delete(c, "route") }},
		{"ipv6-follow", func(i, c map[string]any) {
			c["route"] = map[string]any{"rules": []any{map[string]any{"ip_version": 6, "outbound": "proxy"}}}
		}},
		{"ipv6-block", func(i, c map[string]any) {
			c["route"] = map[string]any{"rules": []any{map[string]any{"ip_version": 6, "action": "reject"}}}
		}},
		{"conditional-ipv6", func(i, c map[string]any) {
			c["route"] = map[string]any{"rules": []any{map[string]any{"ip_version": 6, "outbound": "direct", "inbound": "tun-in"}}}
		}},
		{"duplicate-tun", func(i, c map[string]any) { c["inbounds"] = append(c["inbounds"].([]any), i) }},
		{"tproxy-conflict", func(i, c map[string]any) {
			c["inbounds"] = append(c["inbounds"].([]any), map[string]any{"type": "tproxy", "tag": "tproxy-in", "listen_port": 7893})
		}},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			if _, err := nativeTUNReadinessTarget(changeTUNReadinessConfig(t, test.change)); err == nil {
				t.Fatal("wrong accepted lane passed")
			}
		})
	}
}

func TestOwnedNativeTUNReadinessStartingAndRunning(t *testing.T) {
	for _, state := range []managedruntime.State{managedruntime.Starting, managedruntime.Running} {
		t.Run(string(state), func(t *testing.T) {
			f := newTUNReadinessFixture(t)
			f.status.State = state
			ctx, cancel := context.WithTimeout(context.Background(), time.Second)
			defer cancel()
			if err := checkOwnedNativeReadinessWithIO(ctx, []byte(tunReadinessFixtureJSON), f.getStatus, f.io); err != nil {
				t.Fatal(err)
			}
			if f.probes != 1 {
				t.Fatalf("listener probes = %d", f.probes)
			}
			for _, path := range f.reads {
				if path == tunExecutableFixture || strings.Contains(path, "cmdline") || strings.Contains(path, "inet_diag") {
					t.Fatalf("readiness unexpectedly read private binary or arguments: %s", path)
				}
			}
		})
	}
}

func TestOwnedNativeTUNReadinessLegacyDoesNotObserveHost(t *testing.T) {
	called := false
	observe := tunReadinessIO{probeListeners: func(ctx context.Context, raw []byte) error { called = true; return nil }}
	if err := checkOwnedNativeReadinessWithIO(context.Background(), []byte(`{"inbounds":[{"type":"tproxy","listen_port":7893}]}`), nil, observe); err != nil || !called {
		t.Fatalf("legacy readiness err = %v, probe called = %v", err, called)
	}
}

func TestOwnedNativeTUNReadinessRejectsOwnerStatus(t *testing.T) {
	for _, state := range []managedruntime.State{managedruntime.NotConfigured, managedruntime.Checking, managedruntime.Stopped, managedruntime.Backoff, managedruntime.Error} {
		f := newTUNReadinessFixture(t)
		f.status.State = state
		if err := checkOwnedNativeReadinessWithIO(context.Background(), []byte(tunReadinessFixtureJSON), f.getStatus, f.io); err == nil {
			t.Fatalf("state %q passed", state)
		}
	}
	for _, change := range []func(*tunReadinessFixture){
		func(f *tunReadinessFixture) { f.status.PID = 0 },
		func(f *tunReadinessFixture) { f.status.Service = managedruntime.FRPC },
		func(f *tunReadinessFixture) { f.status.Configured = false },
		func(f *tunReadinessFixture) { f.status.ArtifactAvailable = false },
		func(f *tunReadinessFixture) { f.statusErr = errors.New("unavailable") },
	} {
		f := newTUNReadinessFixture(t)
		change(f)
		if err := checkOwnedNativeReadinessWithIO(context.Background(), []byte(tunReadinessFixtureJSON), f.getStatus, f.io); err == nil {
			t.Fatal("unowned status passed")
		}
	}
	f := newTUNReadinessFixture(t)
	if err := checkOwnedNativeReadinessWithIO(context.Background(), []byte(tunReadinessFixtureJSON), nil, f.io); err == nil {
		t.Fatal("nil status passed")
	}
}

func TestOwnedNativeTUNReadinessFailsClosed(t *testing.T) {
	tests := []struct {
		name   string
		change func(*tunReadinessFixture)
	}{
		{"interface-missing", func(f *tunReadinessFixture) { f.ifaces = f.ifaces[:1] }},
		{"interface-down", func(f *tunReadinessFixture) { f.ifaces[1].up = false }},
		{"wrong-mtu", func(f *tunReadinessFixture) { f.ifaces[1].mtu = 1400 }},
		{"wrong-address", func(f *tunReadinessFixture) { f.ifaces[1].addresses[0] = netip.MustParsePrefix("172.31.255.254/30") }},
		{"wrong-address-prefix", func(f *tunReadinessFixture) { f.ifaces[1].addresses[0] = netip.MustParsePrefix("172.31.255.253/32") }},
		{"missing-address", func(f *tunReadinessFixture) { f.ifaces[1].addresses = nil }},
		{"extra-address", func(f *tunReadinessFixture) {
			f.ifaces[1].addresses = append(f.ifaces[1].addresses, netip.MustParsePrefix("10.0.0.1/30"))
		}},
		{"strict-rpf", func(f *tunReadinessFixture) { f.files["/proc/sys/net/ipv4/conf/b6p-tun/rp_filter"] = []byte("1\n") }},
		{"disabled-rpf", func(f *tunReadinessFixture) { f.files["/proc/sys/net/ipv4/conf/b6p-tun/rp_filter"] = []byte("0\n") }},
		{"missing-rpf", func(f *tunReadinessFixture) { delete(f.files, "/proc/sys/net/ipv4/conf/b6p-tun/rp_filter") }},
		{"missing-tun-fdinfo", func(f *tunReadinessFixture) { delete(f.files, tunProcFixtureBase+"/fdinfo/7") }},
		{"unreadable-tun-fdinfo", func(f *tunReadinessFixture) { f.readErrors[tunProcFixtureBase+"/fdinfo/7"] = os.ErrPermission }},
		{"no-iff-abi", func(f *tunReadinessFixture) {
			f.files[tunProcFixtureBase+"/fdinfo/7"] = []byte("pos:\t0\nflags:\t0104002\n")
		}},
		{"foreign-tun-iff", func(f *tunReadinessFixture) { f.files[tunProcFixtureBase+"/fdinfo/7"] = []byte("iff:\tb6p-other\n") }},
		{"substring-iff", func(f *tunReadinessFixture) { f.files[tunProcFixtureBase+"/fdinfo/7"] = []byte("iff:\tb6p-tun2\n") }},
		{"missing-fd-dir", func(f *tunReadinessFixture) { delete(f.directories, tunProcFixtureBase+"/fd") }},
		{"foreign-listener", func(f *tunReadinessFixture) { f.links[tunProcFixtureBase+"/fd/8"] = "socket:[999]" }},
		{"missing-listener", func(f *tunReadinessFixture) { f.files["/proc/net/tcp"] = []byte(tunTCPFixtureHeader) }},
		{"dns-only", func(f *tunReadinessFixture) {
			f.files["/proc/net/tcp"] = []byte(tunTCPFixtureHeader + tunTCPFixtureLine("172.31.255.253", 53, "0A", "111"))
		}},
		{"foreign-private-address", func(f *tunReadinessFixture) {
			f.files["/proc/net/tcp"] = []byte(tunTCPFixtureHeader + tunTCPFixtureLine("172.31.255.254", 35001, "0A", "111"))
		}},
		{"wildcard-listener", func(f *tunReadinessFixture) {
			f.files["/proc/net/tcp"] = []byte(tunTCPFixtureHeader + tunTCPFixtureLine("0.0.0.0", 35001, "0A", "111"))
		}},
		{"duplicate-private-listener", func(f *tunReadinessFixture) {
			f.files["/proc/net/tcp"] = append(f.files["/proc/net/tcp"], []byte(tunTCPFixtureLine("172.31.255.253", 35002, "0A", "999"))...)
		}},
		{"duplicate-same-inode", func(f *tunReadinessFixture) {
			f.files["/proc/net/tcp"] = append(f.files["/proc/net/tcp"], []byte(tunTCPFixtureLine("172.31.255.253", 35001, "0A", "111"))...)
		}},
		{"established-not-listener", func(f *tunReadinessFixture) {
			f.files["/proc/net/tcp"] = []byte(tunTCPFixtureHeader + tunTCPFixtureLine("172.31.255.253", 35001, "01", "111"))
		}},
		{"missing-proc-tcp", func(f *tunReadinessFixture) { delete(f.files, "/proc/net/tcp") }},
		{"malformed-proc-tcp", func(f *tunReadinessFixture) { f.files["/proc/net/tcp"] = []byte(tunTCPFixtureHeader + "truncated\n") }},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			f := newTUNReadinessFixture(t)
			test.change(f)
			ctx, cancel := context.WithTimeout(context.Background(), time.Millisecond)
			defer cancel()
			if err := checkOwnedNativeReadinessWithIO(ctx, []byte(tunReadinessFixtureJSON), f.getStatus, f.io); !errors.Is(err, context.DeadlineExceeded) {
				t.Fatalf("unavailable observation should expire safely: %v", err)
			}
			if f.probes != 0 {
				t.Fatalf("invalid TUN performed listener probes: %d", f.probes)
			}
		})
	}
}

func TestOwnedNativeTUNReadinessRejectsInitialProcessIdentity(t *testing.T) {
	tests := []struct {
		name   string
		change func(*tunReadinessFixture)
	}{
		{"wrong-exe", func(f *tunReadinessFixture) { f.links[tunProcFixtureBase+"/exe"] = "/usr/bin/foreign-core" }},
		{"relative-exe", func(f *tunReadinessFixture) { f.links[tunProcFixtureBase+"/exe"] = ".artifact-qualified" }},
		{"deleted-exe", func(f *tunReadinessFixture) { f.links[tunProcFixtureBase+"/exe"] += " (deleted)" }},
		{"wrong-pid-stat", func(f *tunReadinessFixture) { f.files[tunProcFixtureBase+"/stat"] = tunStatFixture(9876, 123456, "S") }},
		{"zombie", func(f *tunReadinessFixture) { f.files[tunProcFixtureBase+"/stat"] = tunStatFixture(4321, 123456, "Z") }},
		{"dead", func(f *tunReadinessFixture) { f.files[tunProcFixtureBase+"/stat"] = tunStatFixture(4321, 123456, "X") }},
		{"missing-stat", func(f *tunReadinessFixture) { delete(f.files, tunProcFixtureBase+"/stat") }},
		{"missing-exe-stat", func(f *tunReadinessFixture) { delete(f.infos, tunProcFixtureBase+"/exe") }},
		{"no-starttime", func(f *tunReadinessFixture) { f.files[tunProcFixtureBase+"/stat"] = tunStatFixture(4321, 0, "S") }},
		{"truncated-stat", func(f *tunReadinessFixture) { f.files[tunProcFixtureBase+"/stat"] = []byte("4321 (core) S 0 0") }},
		{"different-exe-inode", func(f *tunReadinessFixture) {
			f.infos[tunProcFixtureBase+"/exe"] = f.infos[filepath.Dir(tunExecutableFixture)]
		}},
		{"missing-private-dir", func(f *tunReadinessFixture) { delete(f.infos, filepath.Dir(tunExecutableFixture)) }},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			f := newTUNReadinessFixture(t)
			test.change(f)
			if err := checkOwnedNativeReadinessWithIO(context.Background(), []byte(tunReadinessFixtureJSON), f.getStatus, f.io); err == nil {
				t.Fatal("wrong process identity passed")
			}
		})
	}
}

func TestOwnedNativeTUNReadinessRechecksProcessAfterListeners(t *testing.T) {
	for _, test := range []struct {
		name   string
		change func(*tunReadinessFixture)
	}{
		{"pid-reuse", func(f *tunReadinessFixture) { f.files[tunProcFixtureBase+"/stat"] = tunStatFixture(4321, 999999, "S") }},
		{"manager-pid-changed", func(f *tunReadinessFixture) { f.status.PID++ }},
		{"manager-generation-changed", func(f *tunReadinessFixture) { f.status.Generation++ }},
		{"manager-stopped", func(f *tunReadinessFixture) { f.status.State = managedruntime.Stopped }},
		{"exe-path-changed", func(f *tunReadinessFixture) {
			f.links[tunProcFixtureBase+"/exe"] = "/run/be6500panel/runtime/.artifact-replaced"
			f.infos[f.links[tunProcFixtureBase+"/exe"]] = f.infos[tunExecutableFixture]
		}},
		{"exe-inode-changed", func(f *tunReadinessFixture) {
			other := newTUNReadinessFixture(t)
			f.infos[tunExecutableFixture] = other.infos[tunExecutableFixture]
			f.infos[tunProcFixtureBase+"/exe"] = other.infos[tunExecutableFixture]
		}},
	} {
		t.Run(test.name, func(t *testing.T) {
			f := newTUNReadinessFixture(t)
			f.onProbe = func() { test.change(f) }
			ctx, cancel := context.WithTimeout(context.Background(), time.Second)
			defer cancel()
			if err := checkOwnedNativeReadinessWithIO(ctx, []byte(tunReadinessFixtureJSON), f.getStatus, f.io); err == nil || !strings.Contains(err.Error(), "identity changed") {
				t.Fatalf("changed owner passed: %v", err)
			}
		})
	}
}

func TestOwnedNativeTUNReadinessRechecksTUNAfterListeners(t *testing.T) {
	f := newTUNReadinessFixture(t)
	f.onProbe = func() { f.ifaces[1].up = false }
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Millisecond)
	defer cancel()
	if err := checkOwnedNativeReadinessWithIO(ctx, []byte(tunReadinessFixtureJSON), f.getStatus, f.io); !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("disappeared TUN passed: %v", err)
	}
}

func TestOwnedNativeTUNReadinessWaitsForCoreInterface(t *testing.T) {
	f := newTUNReadinessFixture(t)
	calls := 0
	f.io.interfaces = func() ([]tunReadinessInterface, error) {
		calls++
		if calls == 1 {
			return f.ifaces[:1], nil
		}
		return f.ifaces, nil
	}
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	if err := checkOwnedNativeReadinessWithIO(ctx, []byte(tunReadinessFixtureJSON), f.getStatus, f.io); err != nil {
		t.Fatal(err)
	}
	if calls != 3 || f.probes != 1 {
		t.Fatalf("interface reads = %d, probes = %d", calls, f.probes)
	}
}

func TestOwnedNativeTUNReadinessDoesNotRebindReusedPID(t *testing.T) {
	f := newTUNReadinessFixture(t)
	calls := 0
	f.onRead = func(path string) {
		if path == tunProcFixtureBase+"/stat" {
			calls++
			if calls == 2 {
				f.files[path] = tunStatFixture(4321, 654321, "S")
			}
		}
	}
	if err := checkOwnedNativeReadinessWithIO(context.Background(), []byte(tunReadinessFixtureJSON), f.getStatus, f.io); err == nil {
		t.Fatal("reused PID during initial identity reads passed")
	}
}

func TestOwnedNativeTUNReadinessCancellation(t *testing.T) {
	f := newTUNReadinessFixture(t)
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if err := checkOwnedNativeReadinessWithIO(ctx, []byte(tunReadinessFixtureJSON), f.getStatus, f.io); !errors.Is(err, context.Canceled) {
		t.Fatalf("cancelled readiness = %v", err)
	}
	if len(f.reads) != 0 || f.probes != 0 {
		t.Fatal("cancelled readiness performed I/O")
	}
}

func TestTUNPrivateTCPListenerIgnoresDNSAndOtherAddresses(t *testing.T) {
	raw := []byte(tunTCPFixtureHeader +
		tunTCPFixtureLine("172.31.255.253", 53, "0A", "222") +
		tunTCPFixtureLine("127.0.0.1", 2080, "0A", "333") +
		tunTCPFixtureLine("172.31.255.253", 35001, "0A", "111") +
		tunTCPFixtureLine("172.31.255.253", 38000, "01", "444"))
	inode, err := tunPrivateTCPListener(raw, netip.MustParseAddr("172.31.255.253"))
	if err != nil || inode != 111 {
		t.Fatalf("private listener inode = %d, err = %v", inode, err)
	}
}

func TestNativeReadinessTargetsDoesNotInventTUNTProxySocket(t *testing.T) {
	targets, err := nativeReadinessTargets([]byte(tunReadinessFixtureJSON))
	want := []readinessTarget{{network: "tcp", address: "127.0.0.1:2080"}}
	if err != nil || !reflect.DeepEqual(targets, want) {
		t.Fatalf("targets = %#v, err = %v", targets, err)
	}
}

func TestRuntimeReadinessNilManagerSafe(t *testing.T) {
	if err := runtimeReadiness(func() *managedruntime.Manager { return nil })(context.Background(), managedruntime.SingBox); err == nil {
		t.Fatal("nil manager passed sing-box readiness")
	}
}

func TestOwnedNativeTUNReadinessAllowsOnlyIPv6LinkMetadata(t *testing.T) {
	for _, test := range []struct {
		name, address string
		wantReady     bool
	}{
		{"link-local", "fe80::1234/64", true},
		{"global-ipv6", "2001:db8::1/64", false},
		{"private-ipv6", "fd00::1/64", false},
		{"mapped-ipv4", "::ffff:172.31.255.253/126", false},
		{"duplicate-owned-ipv4", "172.31.255.253/30", false},
	} {
		t.Run(test.name, func(t *testing.T) {
			f := newTUNReadinessFixture(t)
			f.ifaces[1].addresses = append(f.ifaces[1].addresses, netip.MustParsePrefix(test.address))
			ctx, cancel := context.WithTimeout(context.Background(), time.Millisecond)
			defer cancel()
			err := checkOwnedNativeReadinessWithIO(ctx, []byte(tunReadinessFixtureJSON), f.getStatus, f.io)
			if (err == nil) != test.wantReady {
				t.Fatalf("ready error = %v, want ready = %v", err, test.wantReady)
			}
		})
	}
}

func TestNativeTUNPreStartRejectsInterfaceAndPrefixCollisions(t *testing.T) {
	tests := []struct {
		name     string
		ifaces   []tunReadinessInterface
		routes   string
		routeErr error
	}{
		{"occupied-down", []tunReadinessInterface{{name: "b6p-tun"}}, "", nil},
		{"occupied-ipv6-only", []tunReadinessInterface{{name: "b6p-tun", addresses: []netip.Prefix{netip.MustParsePrefix("fe80::1/64")}}}, "", nil},
		{"local-broad-prefix", []tunReadinessInterface{{name: "br-lan", addresses: []netip.Prefix{netip.MustParsePrefix("172.31.1.1/16")}}}, "", nil},
		{"local-host-same", []tunReadinessInterface{{name: "br-lan", addresses: []netip.Prefix{netip.MustParsePrefix("172.31.255.253/32")}}}, "", nil},
		{"local-peer", []tunReadinessInterface{{name: "br-lan", addresses: []netip.Prefix{netip.MustParsePrefix("172.31.255.254/32")}}}, "", nil},
		{"local-broadcast", []tunReadinessInterface{{name: "br-lan", addresses: []netip.Prefix{netip.MustParsePrefix("172.31.255.255/32")}}}, "", nil},
		{"local-network", []tunReadinessInterface{{name: "br-lan", addresses: []netip.Prefix{netip.MustParsePrefix("172.31.255.252/32")}}}, "", nil},
		{"route-broad-prefix", nil, "172.31.0.0/16 dev br-lan proto kernel scope link\n", nil},
		{"route-owned-prefix", nil, "172.31.255.252/30 dev foreign table custom proto static\n", nil},
		{"route-node-host", nil, "172.31.255.254 via 192.168.31.1 dev br-lan table 200\n", nil},
		{"route-local-host", nil, "local 172.31.255.253 dev lo table local proto kernel scope host\n", nil},
		{"route-broadcast", nil, "broadcast 172.31.255.255 dev br-lan table local proto kernel\n", nil},
		{"route-blackhole", nil, "blackhole 172.31.255.252/30 table 200\n", nil},
		{"route-throw", nil, "throw 172.31.255.252/30 table 200\n", nil},
		{"route-no-data", nil, "", errors.New("unavailable")},
		{"route-unknown-output", nil, "Warning: route observation unavailable\n", nil},
		{"route-truncated", nil, "local\n", nil},
		{"route-nonmasked-prefix", nil, "172.31.255.253/30 dev br-lan\n", nil},
		{"route-invalid-prefix", nil, "172.31.255.252/99 dev br-lan\n", nil},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			f := newTUNReadinessFixture(t)
			f.ifaces = test.ifaces
			f.io.ipv4Routes = func(context.Context) ([]byte, error) { return []byte(test.routes), test.routeErr }
			if err := checkNativeTUNPreStartWithIO(context.Background(), []byte(tunReadinessFixtureJSON), f.io); err == nil {
				t.Fatal("pre-start collision passed")
			}
			if f.probes != 0 {
				t.Fatal("pre-start dialed local listener")
			}
		})
	}
}

func TestNativeTUNPreStartAllowsSeparateLANAndDefaultRoutes(t *testing.T) {
	f := newTUNReadinessFixture(t)
	f.ifaces = []tunReadinessInterface{
		{name: "lo", up: true, addresses: []netip.Prefix{netip.MustParsePrefix("127.0.0.1/8"), netip.MustParsePrefix("::1/128")}},
		{name: "br-lan", up: true, addresses: []netip.Prefix{netip.MustParsePrefix("192.168.31.1/24"), netip.MustParsePrefix("fe80::1/64")}},
	}
	f.io.ipv4Routes = func(context.Context) ([]byte, error) {
		return []byte("default via 192.0.2.1 dev eth0\n" +
			"unicast 0.0.0.0/0 via 192.0.2.1 dev eth0 table 200\n" +
			"192.168.31.0/24 dev br-lan proto kernel scope link src 192.168.31.1\n" +
			"local 127.0.0.1 dev lo table local proto kernel scope host\n" +
			"broadcast 192.168.31.255 dev br-lan table local proto kernel\n" +
			"172.31.255.248/30 dev separate table 300\n"), nil
	}
	if err := checkNativeTUNPreStartWithIO(context.Background(), []byte(tunReadinessFixtureJSON), f.io); err != nil {
		t.Fatal(err)
	}
	if len(f.reads) != 0 || f.probes != 0 {
		t.Fatal("pre-start touched process or socket state")
	}
}

func TestNativeTUNPreStartLegacyDoesNotObserveHost(t *testing.T) {
	legacy := []byte(`{"inbounds":[{"type":"tproxy","listen_port":7893}]}`)
	if err := checkNativeTUNPreStartWithIO(context.Background(), legacy, tunReadinessIO{}); err != nil {
		t.Fatal(err)
	}
}

func TestNativeTUNPreStartObservationAndContextFailure(t *testing.T) {
	f := newTUNReadinessFixture(t)
	f.io.interfaces = func() ([]tunReadinessInterface, error) { return nil, errors.New("unavailable") }
	if err := checkNativeTUNPreStartWithIO(context.Background(), []byte(tunReadinessFixtureJSON), f.io); err == nil {
		t.Fatal("missing interface observation passed")
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if err := checkNativeTUNPreStartWithIO(ctx, []byte(tunReadinessFixtureJSON), tunReadinessIO{}); !errors.Is(err, context.Canceled) {
		t.Fatalf("cancelled pre-start = %v", err)
	}
}

func TestTUNPreStartRouteOutputBound(t *testing.T) {
	output := &tunReadinessBoundedOutput{}
	if n, err := output.Write(make([]byte, 4<<20)); err != nil || n != 4<<20 {
		t.Fatalf("bounded write = %d, %v", n, err)
	}
	if _, err := output.Write([]byte{1}); err == nil {
		t.Fatal("oversized route output accepted")
	}
}

func TestCaptureBackendObservationIsOneShot(t *testing.T) {
	for _, which := range []string{"ready", "missing", "down", "rpf", "pid-reused"} {
		f := newTUNReadinessFixture(t)
		interfaceReads := 0
		switch which {
		case "missing":
			f.ifaces = f.ifaces[:1]
		case "down":
			f.ifaces[1].up = false
		case "rpf":
			f.files["/proc/sys/net/ipv4/conf/b6p-tun/rp_filter"] = []byte("0\n")
		}
		f.io.interfaces = func() ([]tunReadinessInterface, error) { interfaceReads++; return f.ifaces, nil }
		if which == "pid-reused" {
			f.onRead = func(path string) {
				if path == tunProcFixtureBase+"/fdinfo/7" {
					f.files[tunProcFixtureBase+"/stat"] = tunStatFixture(4321, 123457, "S")
				}
			}
		}
		started := time.Now()
		err := observeOwnedNativeBackendWithIO(context.Background(), []byte(tunReadinessFixtureJSON), f.getStatus, f.io)
		if (err == nil) != (which == "ready") {
			t.Fatal(which, err)
		}
		if f.probes != 0 || interfaceReads > 1 || time.Since(started) > time.Second {
			t.Fatal("GET retried/probed instead of observing", which, f.probes, interfaceReads)
		}
	}
}
func TestCaptureBackendObservationLegacyDoesNotProbe(t *testing.T) {
	observed := tunReadinessIO{probeListeners: func(context.Context, []byte) error { t.Fatal("GET performed listener probe"); return nil }}
	if err := observeOwnedNativeBackendWithIO(context.Background(), []byte(`{"inbounds":[{"type":"tproxy"}]}`), nil, observed); err != nil {
		t.Fatal(err)
	}
}

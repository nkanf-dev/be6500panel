package router

import (
	"context"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"
)

// All addresses, names, counters, versions, and wireless secrets below are
// synthetic documentation-range data, never captures from a real router.
func fixtureFile(t *testing.T, root, path, contents string) {
	t.Helper()
	file := filepath.Join(root, filepath.FromSlash(strings.TrimPrefix(path, "/")))
	if err := os.MkdirAll(filepath.Dir(file), 0700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(file, []byte(contents), 0600); err != nil {
		t.Fatal(err)
	}
}
func syntheticFixture(t *testing.T) string {
	t.Helper()
	root := t.TempDir()
	fixtureFile(t, root, "/etc/config/version", "config core 'version'\n option HARDWARE 'RN02'\n option ROM '9.8.7-test'\n option LINUX '5.4'\n")
	fixtureFile(t, root, "/etc/openwrt_release", "DISTRIB_ARCH='arm-test-target'\nDISTRIB_RELEASE='test-release'\n")
	fixtureFile(t, root, "/proc/sys/kernel/osrelease", "5.4.123-test\n")
	fixtureFile(t, root, "/tmp/dhcp.leases", "2000000600 02:00:00:00:00:01 192.0.2.10 test-pc *\n0 02:00:00:00:00:02 192.0.2.11 * *\n")
	fixtureFile(t, root, "/proc/net/arp", "IP address HW type Flags HW address Mask Device\n192.0.2.10 0x1 0x2 02:00:00:00:00:01 * br-lan\n")
	fixtureFile(t, root, "/etc/config/wireless", "config wifi-device 'wifi0'\n option hwmode '11beg'\n option channel '6'\n option bw '40'\nconfig wifi-iface\n option device 'wifi0'\n option ifname 'wl1'\n option ssid 'Synthetic Lab'\n option encryption 'psk2'\n option key 'SECRET-SYNTHETIC-KEY'\n option password 'SECRET-SYNTHETIC-PASSWORD'\n")
	fixtureFile(t, root, "/tmp/resolv.conf.auto", "nameserver 192.0.2.53\nnameserver 2001:db8::53\n")
	fixtureFile(t, root, "/etc/resolv.conf", "nameserver 127.0.0.1\n")
	firewall := "*filter\n:INPUT DROP [0:0]\n:FORWARD DROP [0:0]\n:OUTPUT ACCEPT [0:0]\n-A INPUT -i lo -j ACCEPT\nCOMMIT\n"
	fixtureFile(t, root, "/var/run/be6500panel/iptables-save", firewall)
	fixtureFile(t, root, "/var/run/be6500panel/ip6tables-save", firewall)
	fixtureFile(t, root, "/proc/net/route", "Iface Destination Gateway Flags RefCnt Use Metric Mask MTU Window IRTT\neth0.2 00000000 010200C0 0003 0 0 3 00000000 0 0 0\n")
	fixtureFile(t, root, "/proc/net/ipv6_route", "20010db8000000000000000000000000 20 00000000000000000000000000000000 00 fe800000000000000000000000000001 00000008 00000000 00000000 00000003 eth0.2\n")
	fixtureFile(t, root, "/proc/net/dev", netDevFixture("100", "200"))
	return root
}
func netDevFixture(rx, tx string) string {
	return "Inter-| Receive | Transmit\n face |bytes packets errs drop fifo frame compressed multicast|bytes packets errs drop fifo colls carrier compressed\n eth0.2: " + rx + " 0 0 0 0 0 0 0 " + tx + " 0 0 0 0 0 0 0\n"
}

func TestSnapshotRealSourcesContractAndRedaction(t *testing.T) {
	root := syntheticFixture(t)
	now := time.Unix(2000000000, 0)
	a := New(root)
	a.now = func() time.Time { return now }
	s, err := a.Snapshot(context.Background())
	if err != nil || len(s.Errors) != 0 {
		t.Fatalf("err=%v modules=%+v", err, s.Errors)
	}
	if s.Platform.Model != "RN02" || s.Platform.Firmware != "9.8.7-test" || s.Platform.Kernel != "5.4.123-test" || s.Platform.Architecture != "arm-test-target" {
		t.Fatal(s.Platform)
	}
	if len(s.Devices) != 2 || !s.Devices[0].Online || s.Devices[1].Online || s.DNS.LeaseCount != 2 || len(s.WiFi) != 1 || len(s.Routes) != 2 || len(s.Traffic) != 1 {
		t.Fatalf("snapshot=%+v", s)
	}
	if s.DNS.Resolvers[0] != "192.0.2.53" || s.Firewall.IPv4.Input != "DROP" || s.Firewall.IPv6.Rules != 1 {
		t.Fatal(s)
	}
	if s.Traffic[0].RXBytesPerSecond != 0 || s.Traffic[0].TXBytesPerSecond != 0 {
		t.Fatal("first observation must not invent a rate")
	}
	encoded, err := json.Marshal(s)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(encoded), "SECRET-SYNTHETIC") || strings.Contains(string(encoded), `"password"`) || strings.Contains(string(encoded), `"key"`) {
		t.Fatalf("secret disclosed: %s", encoded)
	}
	var payload map[string]json.RawMessage
	if err = json.Unmarshal(encoded, &payload); err != nil {
		t.Fatal(err)
	}
	for _, field := range []string{"platform", "devices", "wifi", "dns", "firewall", "traffic", "routes", "sampledAt", "errors"} {
		if len(payload[field]) == 0 {
			t.Fatal("missing JSON field", field)
		}
	}
}

func TestSnapshotCachingDeltaResetAndIndependentCopies(t *testing.T) {
	root := syntheticFixture(t)
	now := time.Unix(2000000000, 0)
	a := New(root)
	a.now = func() time.Time { return now }
	s, _ := a.Snapshot(context.Background())
	s.Devices[0].Hostname = "tampered"
	*s.Devices[0].ExpiresAt = time.Time{}
	s.DNS.Resolvers[0] = "tampered"
	s.WiFi[0].SSID = "tampered"
	s.Traffic[0].RXBytes = 1234
	s.Errors = append(s.Errors, moduleError("test", "invalid"))
	fixtureFile(t, root, "/proc/net/dev", netDevFixture("300", "600"))
	fixtureFile(t, root, "/etc/config/wireless", "malformed-now\n")
	now = now.Add(time.Second)
	s, _ = a.Snapshot(context.Background())
	if s.Traffic[0].RXBytes != 100 || s.Devices[0].Hostname == "tampered" || s.Devices[0].ExpiresAt.IsZero() || s.DNS.Resolvers[0] == "tampered" || s.WiFi[0].SSID == "tampered" || len(s.Errors) != 0 {
		t.Fatalf("cache leaked/was resampled: %+v", s)
	}
	now = now.Add(time.Second)
	s, _ = a.Snapshot(context.Background())
	if s.Traffic[0].RXBytesPerSecond != 100 || s.Traffic[0].TXBytesPerSecond != 200 || len(s.WiFi) != 1 || len(s.Errors) != 0 {
		t.Fatalf("delta or slow cache wrong: %+v", s)
	}
	fixtureFile(t, root, "/proc/net/dev", netDevFixture("1", "2"))
	now = now.Add(2 * time.Second)
	s, _ = a.Snapshot(context.Background())
	if s.Traffic[0].RXBytesPerSecond != 0 || s.Traffic[0].TXBytesPerSecond != 0 {
		t.Fatal("counter reset fabricated a rate", s.Traffic)
	}
	fixtureFile(t, root, "/proc/net/dev", netDevFixture("21", "42"))
	now = now.Add(2 * time.Second)
	s, _ = a.Snapshot(context.Background())
	if s.Traffic[0].RXBytesPerSecond != 10 || s.Traffic[0].TXBytesPerSecond != 20 || len(s.WiFi) != 0 || !hasModuleError(s, "wifi", "invalid") {
		t.Fatalf("sample refresh wrong: %+v", s)
	}
}

func hasModuleError(s Snapshot, module, code string) bool {
	for _, e := range s.Errors {
		if e.Module == module && e.Code == code {
			return true
		}
	}
	return false
}

func TestSnapshotPartialErrorsAndBoundedFixture(t *testing.T) {
	root := syntheticFixture(t)
	now := time.Unix(2000000000, 0)
	if err := os.Remove(filepath.Join(root, "var/run/be6500panel/ip6tables-save")); err != nil {
		t.Fatal(err)
	}
	fixtureFile(t, root, "/etc/config/wireless", "config wifi-device a\n option key 'PRIVATE-INVALID-DATA'\nconfig wifi-iface b\n option device a\n option disabled maybe\n")
	fixtureFile(t, root, "/tmp/dhcp.leases", strings.Repeat("x", fileLimit+1))
	a := New(root)
	a.now = func() time.Time { return now }
	s, err := a.Snapshot(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if !hasModuleError(s, "firewall.ipv6", "unavailable") || !hasModuleError(s, "devices.leases", "too_large") || !hasModuleError(s, "wifi", "invalid") {
		t.Fatal(s.Errors)
	}
	if s.Platform.Model != "RN02" || len(s.Routes) != 2 || s.Firewall.IPv4.Input != "DROP" || len(s.Traffic) != 1 || len(s.Devices) != 1 || !s.Devices[0].Online {
		t.Fatal("module failure discarded working sources", s)
	}
	encoded, _ := json.Marshal(s)
	if strings.Contains(string(encoded), "PRIVATE-INVALID-DATA") {
		t.Fatal("raw invalid data leaked")
	}
}

func TestSnapshotMissingSourcesAndCancellation(t *testing.T) {
	a := New(t.TempDir())
	s, err := a.Snapshot(context.Background())
	if err != nil || len(s.Errors) < 8 || s.Devices == nil || s.WiFi == nil || s.Routes == nil || s.Traffic == nil || s.DNS.Resolvers == nil || s.Errors == nil {
		t.Fatalf("%+v %v", s, err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	s, err = a.Snapshot(ctx)
	if !errors.Is(err, context.Canceled) || s.Devices == nil {
		t.Fatalf("cancel: %v %+v", err, s)
	}
}

func TestSnapshotConcurrentCallers(t *testing.T) {
	root := syntheticFixture(t)
	a := New(root)
	a.now = func() time.Time { return time.Unix(2000000000, 0) }
	var wg sync.WaitGroup
	for i := 0; i < 30; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for j := 0; j < 10; j++ {
				s, err := a.Snapshot(context.Background())
				if err != nil || len(s.Errors) != 0 || len(s.Traffic) != 1 {
					t.Errorf("err=%v snapshot=%+v", err, s)
					return
				}
				s.Devices[0].Hostname = "local"
				s.DNS.Resolvers[0] = "local"
				s.Traffic[0].RXBytes = 9999
			}
		}()
	}
	wg.Wait()
	s, _ := a.Snapshot(context.Background())
	if s.Devices[0].Hostname != "test-pc" || s.Traffic[0].RXBytes != 100 {
		t.Fatal("concurrent copy mutated cache")
	}
}

func TestTrafficInterfaceDisappearanceAndClockReset(t *testing.T) {
	root := syntheticFixture(t)
	now := time.Unix(2000000000, 0)
	a := New(root)
	a.now = func() time.Time { return now }
	_, _ = a.Snapshot(context.Background())
	now = now.Add(2 * time.Second)
	fixtureFile(t, root, "/proc/net/dev", strings.Join(strings.Split(netDevFixture("0", "0"), "\n")[:2], "\n")+"\n")
	s, _ := a.Snapshot(context.Background())
	if len(s.Traffic) != 0 {
		t.Fatal(s.Traffic)
	}
	now = now.Add(2 * time.Second)
	fixtureFile(t, root, "/proc/net/dev", netDevFixture("200", "500"))
	s, _ = a.Snapshot(context.Background())
	if s.Traffic[0].RXBytesPerSecond != 0 {
		t.Fatal("reappearing interface inherited old rate")
	}
	now = now.Add(-10 * time.Second)
	fixtureFile(t, root, "/proc/net/dev", netDevFixture("300", "600"))
	s, _ = a.Snapshot(context.Background())
	if s.Traffic[0].RXBytesPerSecond != 0 || s.Traffic[0].TXBytesPerSecond != 0 {
		t.Fatal("clock reversal produced a rate")
	}
}

func TestVersionFileFallback(t *testing.T) {
	root := syntheticFixture(t)
	if err := os.Remove(filepath.Join(root, "etc/config/version")); err != nil {
		t.Fatal(err)
	}
	fixtureFile(t, root, "/etc/miwifi_version", "HARDWARE='RN02'\nROM='9.8.8-test'\nSECRET='UNEXPORTED'\n")
	a := New(root)
	a.now = func() time.Time { return time.Unix(2000000000, 0) }
	s, err := a.Snapshot(context.Background())
	if err != nil || len(s.Errors) != 0 || s.Platform.Model != "RN02" || s.Platform.Firmware != "9.8.8-test" {
		t.Fatalf("%+v %v", s, err)
	}
}

func TestCanonicalRN02VersionPathTakesPriority(t *testing.T) {
	root := syntheticFixture(t)
	fixtureFile(t, root, "/usr/share/xiaoqiang/xiaoqiang_version", "config core 'version'\n option HARDWARE 'RN02'\n option ROM '9.9.9-canonical-test'\n")
	fixtureFile(t, root, "/tmp/sysinfo/model", "Synthetic Generic Board\n")
	a := New(root)
	a.now = func() time.Time { return time.Unix(2000000000, 0) }
	s, err := a.Snapshot(context.Background())
	if err != nil || len(s.Errors) != 0 || s.Platform.Model != "RN02" || s.Platform.Firmware != "9.9.9-canonical-test" {
		t.Fatalf("canonical version did not override fallback: platform=%+v errors=%+v err=%v", s.Platform, s.Errors, err)
	}
	// The canonical RN02 file also works when /etc/config/version is absent.
	if err := os.Remove(filepath.Join(root, "etc/config/version")); err != nil {
		t.Fatal(err)
	}
	a = New(root)
	a.now = func() time.Time { return time.Unix(2000000000, 0) }
	s, err = a.Snapshot(context.Background())
	if err != nil || len(s.Errors) != 0 || s.Platform.Model != "RN02" || s.Platform.Firmware != "9.9.9-canonical-test" {
		t.Fatalf("canonical-only version failed: platform=%+v errors=%+v err=%v", s.Platform, s.Errors, err)
	}
}

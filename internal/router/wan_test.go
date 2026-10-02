package router

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"
)

func wanFixture(t testing.TB) string {
	t.Helper()
	root := t.TempDir()
	files := map[string]string{
		"proc/net/route": "Iface Destination Gateway Flags RefCnt Use Metric Mask MTU Window IRTT\neth0.2 00000000 010200C0 0003 0 0 3 00000000 0 0 0\n",
		"proc/net/dev":   netDevFixture("100", "200"),
	}
	for path, data := range files {
		full := filepath.Join(root, path)
		if err := os.MkdirAll(filepath.Dir(full), 0700); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(full, []byte(data), 0600); err != nil {
			t.Fatal(err)
		}
	}
	return root
}

func TestWANSourceOnlyReadsNetworkProcAndRawCounters(t *testing.T) {
	root := wanFixture(t)
	// No platform/leases/WiFi/firewall fixtures exist. A full Snapshot would
	// report many errors and try expensive module work; this must not do so.
	adapter := New(root)
	now := time.Unix(2000000000, 0)
	adapter.now = func() time.Time { return now }
	source := NewWANSource(adapter)
	snapshot, err := source.Snapshot(context.Background())
	if err != nil || len(snapshot.Errors) != 0 || len(snapshot.Routes) != 1 || len(snapshot.Traffic) != 1 {
		t.Fatal(snapshot, err)
	}
	if snapshot.Traffic[0].Interface != "eth0.2" || snapshot.Traffic[0].RXBytes != 100 || snapshot.Traffic[0].TXBytes != 200 {
		t.Fatal(snapshot.Traffic)
	}
	if len(snapshot.Devices) != 0 || len(snapshot.WiFi) != 0 || snapshot.Platform.Model != "" || adapter.lastSample.IsZero() == false || adapter.lastSources.IsZero() == false {
		t.Fatal("full adapter work performed")
	}
	fixtureFile(t, root, "/proc/net/dev", netDevFixture("300", "600"))
	now = now.Add(2 * time.Second)
	snapshot, err = source.Snapshot(context.Background())
	if err != nil || snapshot.Traffic[0].RXBytes != 300 || snapshot.Traffic[0].RXBytesPerSecond != 0 || snapshot.Traffic[0].TXBytesPerSecond != 0 {
		t.Fatal(snapshot, err)
	}
	// IPv6 source is absent but not needed when the IPv4 default is available.
	if hasModuleError(snapshot, "routes.ipv6", "unavailable") {
		t.Fatal("unnecessary IPv6 read")
	}
}

func TestWANSourceIndependentLockCacheAndCopies(t *testing.T) {
	root := wanFixture(t)
	adapter := New(root)
	now := time.Unix(2000000000, 0)
	adapter.now = func() time.Time { return now }
	source := NewWANSource(adapter)
	// Holding the full observation lock must never delay traffic's proc read.
	adapter.mu.Lock()
	done := make(chan Snapshot, 1)
	go func() {
		snapshot, err := source.Snapshot(context.Background())
		if err == nil {
			done <- snapshot
		}
	}()
	select {
	case snapshot := <-done:
		if len(snapshot.Traffic) != 1 {
			t.Fatal(snapshot)
		}
	case <-time.After(time.Second):
		adapter.mu.Unlock()
		t.Fatal("WAN waited for full adapter work")
	}
	adapter.mu.Unlock()
	snapshot, _ := source.Snapshot(context.Background())
	snapshot.Traffic[0].RXBytes = 999
	snapshot.Routes[0].Interface = "tamper"
	snapshot.Errors = append(snapshot.Errors, ModuleError{Module: "tamper"})
	fixtureFile(t, root, "/proc/net/dev", netDevFixture("500", "900"))
	now = now.Add(time.Second)
	snapshot, _ = source.Snapshot(context.Background())
	if snapshot.Traffic[0].RXBytes != 100 || snapshot.Routes[0].Interface != "eth0.2" || len(snapshot.Errors) != 0 {
		t.Fatal("cache/copy failed", snapshot)
	}
	now = now.Add(time.Second)
	snapshot, _ = source.Snapshot(context.Background())
	if snapshot.Traffic[0].RXBytes != 500 {
		t.Fatal(snapshot)
	}
	var workers sync.WaitGroup
	for i := 0; i < 16; i++ {
		workers.Add(1)
		go func() {
			defer workers.Done()
			s, err := source.Snapshot(context.Background())
			if err != nil || s.Traffic[0].RXBytes != 500 {
				t.Error(s, err)
			}
		}()
	}
	workers.Wait()
}

func TestWANSourceMetricAndIPv6Fallback(t *testing.T) {
	root := wanFixture(t)
	fixtureFile(t, root, "/proc/net/route", "Iface Destination Gateway Flags RefCnt Use Metric Mask MTU Window IRTT\nwan-z 00000000 010200C0 0003 0 0 10 00000000 0 0 0\nwan-b 00000000 010200C0 0003 0 0 3 00000000 0 0 0\nwan-a 00000000 010200C0 0003 0 0 3 00000000 0 0 0\n")
	fixtureFile(t, root, "/proc/net/dev", strings.ReplaceAll(netDevFixture("900", "800"), "eth0.2", "wan-a"))
	adapter := New(root)
	now := time.Unix(2000000000, 0)
	adapter.now = func() time.Time { return now }
	source := NewWANSource(adapter)
	snapshot, _ := source.Snapshot(context.Background())
	if len(snapshot.Routes) != 1 || snapshot.Routes[0].Interface != "wan-a" || len(snapshot.Traffic) != 1 {
		t.Fatal(snapshot)
	}
	fixtureFile(t, root, "/proc/net/route", "Iface Destination Gateway Flags RefCnt Use Metric Mask MTU Window IRTT\neth0.2 000200C0 00000000 0001 0 0 0 00FFFFFF 0 0 0\n")
	fixtureFile(t, root, "/proc/net/ipv6_route", "00000000000000000000000000000000 00 00000000000000000000000000000000 00 fe800000000000000000000000000001 00000008 00000000 00000000 00000003 wan-a\n")
	now = now.Add(2 * time.Second)
	snapshot, _ = source.Snapshot(context.Background())
	if len(snapshot.Errors) != 0 || len(snapshot.Routes) != 1 || snapshot.Routes[0].Family != "ipv6" || len(snapshot.Traffic) != 1 {
		t.Fatal(snapshot)
	}
}

func TestWANSourceErrorsBoundsAndCancellation(t *testing.T) {
	root := wanFixture(t)
	adapter := New(root)
	source := NewWANSource(adapter)
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := source.Snapshot(ctx); err != context.Canceled {
		t.Fatal(err)
	}
	if _, err := NewWANSource(nil).Snapshot(context.Background()); err == nil {
		t.Fatal("nil adapter accepted")
	}
	fixtureFile(t, root, "/proc/net/dev", strings.Repeat("x", procLimit+1))
	snapshot, err := source.Snapshot(context.Background())
	if err != nil || !hasModuleError(snapshot, "traffic", "too_large") || len(snapshot.Traffic) != 0 {
		t.Fatal(snapshot, err)
	}
	// Malformed routes are disclosed rather than establishing valid coverage.
	fixtureFile(t, root, "/proc/net/route", "not a valid route table\n")
	snapshot, err = NewWANSource(adapter).Snapshot(context.Background())
	if err != nil || !hasModuleError(snapshot, "routes.ipv4", "invalid") || !hasModuleError(snapshot, "routes.ipv6", "unavailable") {
		t.Fatal(snapshot, err)
	}
	// A fixture cannot follow a symlink outside its root.
	outside := filepath.Join(t.TempDir(), "dev")
	if err = os.WriteFile(outside, []byte(netDevFixture("1", "2")), 0600); err != nil {
		t.Fatal(err)
	}
	if err = os.Remove(filepath.Join(root, "proc/net/dev")); err != nil {
		t.Fatal(err)
	}
	if err = os.Symlink(outside, filepath.Join(root, "proc/net/dev")); err != nil {
		t.Fatal(err)
	}
	snapshot, err = NewWANSource(adapter).Snapshot(context.Background())
	if err != nil || !hasModuleError(snapshot, "traffic", "permission_denied") {
		t.Fatal(snapshot, err)
	}
}

func BenchmarkWANSourceProcOnly(b *testing.B) {
	root := wanFixture(b)
	adapter := New(root)
	now := time.Unix(2000000000, 0)
	adapter.now = func() time.Time { return now }
	source := NewWANSource(adapter)
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		now = now.Add(2 * time.Second)
		if _, err := source.Snapshot(context.Background()); err != nil {
			b.Fatal(err)
		}
	}
}

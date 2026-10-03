package router

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestDeviceSourceFixtureIsolationCacheBoundsAndClone(t *testing.T) {
	root := t.TempDir()
	adapter := New(root)
	now := time.Date(2026, 1, 1, 12, 0, 0, 0, time.UTC)
	adapter.now = func() time.Time { return now }
	source := NewDeviceSource(adapter)
	data, err := os.ReadFile("../devicetelemetry/testdata/trafficd-wireless-synthetic.json")
	if err != nil {
		t.Fatal(err)
	}
	fixtureFile(t, root, "/var/run/be6500panel/trafficd-hw.json", string(data))
	got, err := source.Snapshot(context.Background())
	if err != nil || got.Error != "" || len(got.Devices) != 4 || len(got.Devices[0].Links) != 2 {
		t.Fatal(got, err)
	}
	got.Devices[0].Counters[0].RX = 1
	got.Devices[0].Links[0].Protocol = "mutated"
	fixtureFile(t, root, "/var/run/be6500panel/trafficd-hw.json", "{}")
	cache, err := source.Snapshot(context.Background())
	if err != nil || cache.Devices[0].Counters[0].RX == 1 || cache.Devices[0].Links[0].Protocol == "mutated" || !cache.SampledAt.Equal(now) {
		t.Fatal(cache, err)
	}
	now = now.Add(15 * time.Second)
	empty, err := source.Snapshot(context.Background())
	if err != nil || empty.Error != "" || len(empty.Devices) != 0 {
		t.Fatal(empty, err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err = source.Snapshot(ctx); !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
	now = now.Add(15 * time.Second)
	if err = os.Remove(filepath.Join(root, "var/run/be6500panel/trafficd-hw.json")); err != nil {
		t.Fatal(err)
	}
	unavailable, err := source.Snapshot(context.Background())
	if err != nil || unavailable.Error == "" || len(unavailable.Devices) != 0 {
		t.Fatal("fixture executed live command", unavailable, err)
	}
	outside := filepath.Join(t.TempDir(), "source")
	if err = os.WriteFile(outside, data, 0600); err != nil {
		t.Fatal(err)
	}
	if err = os.Symlink(outside, filepath.Join(root, "var/run/be6500panel/trafficd-hw.json")); err != nil {
		t.Fatal(err)
	}
	now = now.Add(15 * time.Second)
	denied, err := source.Snapshot(context.Background())
	if err != nil || denied.Error == "" || len(denied.Devices) != 0 {
		t.Fatal(denied, err)
	}
}

func TestDeviceSourceReadLatencyDoesNotSkipNextCollectorTick(t *testing.T) {
	root := t.TempDir()
	adapter := New(root)
	start := time.Date(2026, 1, 1, 12, 0, 0, 0, time.UTC)
	now := start
	calls := 0
	adapter.now = func() time.Time {
		calls++
		if calls == 2 {
			return now.Add(100 * time.Millisecond)
		}
		return now
	}
	fixtureFile(t, root, "/var/run/be6500panel/trafficd-hw.json", "{}")
	source := NewDeviceSource(adapter)
	first, err := source.Snapshot(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if !first.SampledAt.Equal(start.Add(100 * time.Millisecond)) {
		t.Fatal(first)
	}
	now = start.Add(15 * time.Second)
	second, err := source.Snapshot(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if !second.SampledAt.Equal(now) {
		t.Fatal("read completion cache skipped nominal sampling tick", second)
	}
}

package devicetelemetry

import (
	"context"
	"errors"
	"fmt"
	"sync/atomic"
	"testing"
	"time"
	"unsafe"
)

type noSource struct{}

func (noSource) Snapshot(context.Context) (Snapshot, error) {
	return Snapshot{}, errors.New("no source")
}
func makeCollector(t *testing.T, now *time.Time) *Collector {
	t.Helper()
	c, err := New(noSource{})
	if err != nil {
		t.Fatal(err)
	}
	c.now = func() time.Time { return *now }
	t.Cleanup(func() { _ = c.Close() })
	return c
}
func observed(id string, rx, tx uint64) Observation {
	return Observation{ID: id, Name: "synthetic " + id, Interface: "wl0", Associated: true, Counters: []Counter{{Address: "192.0.2.1", RX: rx, TX: tx}}}
}
func recordAt(c *Collector, at time.Time, rows ...Observation) {
	c.record(Snapshot{SampledAt: at, Devices: rows})
}
func query(t *testing.T, c *Collector, name string, points, limit int, search string) History {
	t.Helper()
	h, err := c.Query(context.Background(), name, points, limit, search)
	if err != nil {
		t.Fatal(err)
	}
	return h
}
func TestMeasuredZeroMissingResetCacheAndMultiIP(t *testing.T) {
	now := time.Date(2026, 1, 1, 12, 0, 0, 0, time.UTC)
	c := makeCollector(t, &now)
	row := observed("02:00:00:00:00:01", 100, 200)
	row.Counters = append(row.Counters, Counter{Address: "2001:db8::1", RX: 300, TX: 400})
	recordAt(c, now, row)
	h := query(t, c, "30m", 288, 32, "")
	if h.Devices[0].RXBytesPerSecond != nil || h.Devices[0].CoverageSeconds != 0 || h.Devices[0].Samples[len(h.Devices[0].Samples)-1].RXBytes != nil {
		t.Fatal("baseline invented measurements", h)
	}
	now = now.Add(SampleInterval)
	recordAt(c, now, row)
	h = query(t, c, "30m", 288, 32, "")
	if h.Devices[0].CoverageSeconds != 15 || h.Devices[0].RXBytesPerSecond == nil || *h.Devices[0].RXBytesPerSecond != 0 {
		t.Fatal("measured zero lost", h)
	}
	point := h.Devices[0].Samples[len(h.Devices[0].Samples)-1]
	if point.RXBytes == nil || *point.RXBytes != 0 || point.CoverageSeconds != 15 {
		t.Fatal(point)
	}
	// Repeated timestamp with mutated cached rows must not disturb baseline.
	cached := observed(row.ID, 999999, 999999)
	recordAt(c, now, cached)
	row.Counters[0].RX += 15
	row.Counters[1].RX += 30
	row.Counters[0].TX += 10
	row.Counters[1].TX += 20
	now = now.Add(SampleInterval)
	recordAt(c, now, row)
	h = query(t, c, "30m", 288, 32, "")
	if h.Devices[0].RXBytes != 45 || h.Devices[0].TXBytes != 30 || *h.Devices[0].RXBytesPerSecond != 3 {
		t.Fatal("multi IP/cached delta", h)
	}
	// Per-IP reset cannot be hidden by another IP's increasing sum.
	row.Counters[0].RX = 1
	row.Counters[1].RX += 100000
	now = now.Add(SampleInterval)
	recordAt(c, now, row)
	h = query(t, c, "30m", 288, 32, "")
	if h.Devices[0].RXBytes != 45 || h.Devices[0].RXBytesPerSecond != nil {
		t.Fatal("reset spike", h)
	}
	// MAC remains stable after IP replacement but first interval is a gap.
	row.Counters = []Counter{{Address: "192.0.2.9", RX: 99999, TX: 99999}}
	now = now.Add(SampleInterval)
	recordAt(c, now, row)
	h = query(t, c, "30m", 288, 32, "")
	if h.Devices[0].ID != row.ID || h.Devices[0].Addresses[0] != "192.0.2.9" || h.Devices[0].RXBytes != 45 {
		t.Fatal(h)
	}
	// A long sampling gap is not converted to bytes across unobserved time.
	now = now.Add(2 * time.Minute)
	row.Counters[0].RX += 100
	recordAt(c, now, row)
	if got := query(t, c, "30m", 288, 32, "").Devices[0]; got.RXBytes != 45 || got.RXBytesPerSecond != nil {
		t.Fatal(got)
	}
	now = now.Add(StaleAfter + time.Second)
	if got := query(t, c, "30m", 288, 32, ""); got.State != "stale" || !got.Devices[0].Stale || got.Devices[0].RXBytesPerSecond != nil {
		t.Fatal(got)
	}
}
func TestBoundarySplitSubsampleAndTimeRegression(t *testing.T) {
	now := time.Date(2026, 1, 1, 12, 4, 55, 0, time.UTC)
	c := makeCollector(t, &now)
	row := observed("02:00:00:00:00:01", 0, 0)
	recordAt(c, now, row)
	now = now.Add(15 * time.Second)
	row.Counters[0].RX = 90
	row.Counters[0].TX = 45
	recordAt(c, now, row)
	h := query(t, c, "30m", 288, 32, "")
	d := h.Devices[0]
	var rx, tx uint64
	var covered float64
	measured := 0
	for _, p := range d.Samples {
		if p.RXBytes != nil {
			rx += *p.RXBytes
			tx += *p.TXBytes
			covered += p.CoverageSeconds
			measured++
		}
	}
	if rx != 90 || tx != 45 || covered != 15 || measured != 2 {
		t.Fatal(rx, tx, covered, measured)
	}
	for _, name := range []string{"30m", "24h", "7d"} {
		small := query(t, c, name, 1, 1, "")
		if len(small.Devices[0].Samples) != 1 || small.Devices[0].RXBytes != 90 || small.Devices[0].CoverageSeconds != 15 {
			t.Fatal(name, small)
		}
	}
	recordAt(c, now.Add(-time.Second), observed(row.ID, 100000, 100000))
	if got := query(t, c, "30m", 288, 32, "").Devices[0]; got.RXBytes != 90 {
		t.Fatal("clock regression overwrote coverage", got)
	}
	now = now.Add(15 * time.Second)
	row.Counters[0].RX = 100000
	recordAt(c, now, row)
	if got := query(t, c, "30m", 288, 32, "").Devices[0]; got.RXBytes != 90 || got.RXBytesPerSecond != nil {
		t.Fatal("clock recovery invented a shortened interval", got)
	}
}
func TestDisappearanceHotplugBoundAndSearch(t *testing.T) {
	now := time.Date(2026, 1, 1, 12, 0, 0, 0, time.UTC)
	c := makeCollector(t, &now)
	rows := make([]Observation, MaxDevices)
	for i := range rows {
		rows[i] = observed(fmt.Sprintf("02:00:00:00:00:%02X", i), 100, 200)
		rows[i].Counters[0].Address = fmt.Sprintf("192.0.2.%d", i+1)
	}
	recordAt(c, now, rows...)
	now = now.Add(SampleInterval)
	for i := range rows {
		rows[i].Counters[0].RX += uint64(i + 1)
		rows[i].Counters[0].TX += 1
	}
	recordAt(c, now, rows...)
	h := query(t, c, "24h", 168, 1, "")
	if h.DeviceCount != 128 || h.MatchedCount != 128 || len(h.Devices) != 1 || !h.Truncated || h.Groups[0].DeviceCount != 128 || h.Groups[0].RXBytes != 8256 {
		t.Fatal(h)
	}
	if got := query(t, c, "24h", 1, 1, rows[0].ID); got.MatchedCount != 1 || got.Devices[0].ID != rows[0].ID {
		t.Fatal(got)
	}
	// Replace one MAC. The bounded LRU evicts absent identity, not an incoming one.
	rows[0] = observed("02:00:00:00:01:01", 1000, 2000)
	now = now.Add(SampleInterval)
	recordAt(c, now, rows...)
	if len(c.devices) != 128 || c.devices["02:00:00:00:00:00"] != nil || c.devices[rows[0].ID] == nil {
		t.Fatal("cardinality bound or hotplug failed", len(c.devices))
	}
	// An empty successful observation invalidates all baselines, not history.
	now = now.Add(SampleInterval)
	recordAt(c, now)
	now = now.Add(SampleInterval)
	recordAt(c, now, rows...)
	if d := query(t, c, "24h", 1, 1, rows[0].ID).Devices[0]; d.CoverageSeconds != 0 || d.RXBytesPerSecond != nil {
		t.Fatal("reappearing MAC got guessed activity", d)
	}
	if unsafe.Sizeof(bucket{}) != 32 || unsafe.Sizeof(tracked{})*MaxDevices > 2<<20 || BucketMemoryBytes > 2<<20 {
		t.Fatal("memory bound exceeded", unsafe.Sizeof(tracked{})*MaxDevices)
	}
}
func TestDuplicateCurrentAddressReportsConflictWithoutMergingMAC(t *testing.T) {
	now := time.Date(2026, 1, 1, 12, 0, 0, 0, time.UTC)
	c := makeCollector(t, &now)
	a := observed("02:00:00:00:00:01", 100, 200)
	b := observed("02:00:00:00:00:02", 300, 400)
	recordAt(c, now, a, b)
	h := query(t, c, "24h", 1, 32, "")
	if len(h.Devices) != 2 || len(h.Devices[0].AddressConflicts) != 1 || h.Devices[0].ID == h.Devices[1].ID {
		t.Fatal(h)
	}
}

type lifecycleSource struct {
	calls   atomic.Int32
	started chan struct{}
}

func (s *lifecycleSource) Snapshot(ctx context.Context) (Snapshot, error) {
	s.calls.Add(1)
	select {
	case s.started <- struct{}{}:
	default:
	}
	<-ctx.Done()
	return Snapshot{}, ctx.Err()
}
func TestCentralStartCancellationAndQueriesDoNotReadSource(t *testing.T) {
	src := &lifecycleSource{started: make(chan struct{}, 1)}
	c, err := New(src)
	if err != nil {
		t.Fatal(err)
	}
	for i := 0; i < 3; i++ {
		if _, err = c.Query(context.Background(), "24h", 1, 1, ""); err != nil {
			t.Fatal(err)
		}
	}
	if src.calls.Load() != 0 {
		t.Fatal("HTTP query polled source")
	}
	ctx, cancel := context.WithCancel(context.Background())
	c.Start(ctx)
	c.Start(ctx)
	select {
	case <-src.started:
	case <-time.After(time.Second):
		t.Fatal("no collection without subscribers")
	}
	cancel()
	if err = c.Close(); err != nil {
		t.Fatal(err)
	}
	if src.calls.Load() != 1 {
		t.Fatal("duplicate collector", src.calls.Load())
	}
	if _, err = c.Query(context.Background(), "24h", 1, 1, ""); !errors.Is(err, ErrClosed) {
		t.Fatal(err)
	}
	_ = c.Close()
}
func TestQueryValidationCancellationAndSourceFailure(t *testing.T) {
	for _, q := range []struct {
		name string
		p, l int
		s    string
	}{{"1y", 1, 1, ""}, {"24h", 0, 1, ""}, {"24h", 289, 1, ""}, {"24h", 1, 65, ""}, {"24h", 1, 1, string(make([]byte, 65))}} {
		if ValidateQuery(q.name, q.p, q.l, q.s) == nil {
			t.Fatal(q)
		}
	}
	now := time.Date(2026, 1, 1, 12, 0, 0, 0, time.UTC)
	c := makeCollector(t, &now)
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := c.Query(ctx, "24h", 1, 1, ""); !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
	row := observed("02:00:00:00:00:01", 100, 200)
	recordAt(c, now, row)
	c.collect(context.Background())
	h := query(t, c, "24h", 1, 1, "")
	if h.State != "unavailable" || !h.Devices[0].Stale || h.Error == "" || h.Devices[0].RXBytesPerSecond != nil {
		t.Fatal(h)
	}
}

func TestReturnedHistoryDoesNotMutatePrivateMetadata(t *testing.T) {
	now := time.Date(2026, 1, 1, 12, 0, 0, 0, time.UTC)
	c := makeCollector(t, &now)
	online, age, signal, mld := uint64(100), uint64(1), -62, true
	row := observed("02:00:00:00:00:01", 100, 200)
	row.OnlineSeconds = &online
	row.AgeingSeconds = &age
	row.Links = []WirelessLink{{Interface: "wl0", SignalDBM: &signal, MLD: &mld}}
	recordAt(c, now, row)
	online = 0
	signal = 0
	mld = false
	h := query(t, c, "24h", 1, 1, "")
	if *h.Devices[0].OnlineSeconds != 100 || *h.Devices[0].Links[0].SignalDBM != -62 || !*h.Devices[0].Links[0].MLD {
		t.Fatal("source changed private metadata", h)
	}
	*h.Devices[0].OnlineSeconds = 0
	*h.Devices[0].Links[0].SignalDBM = 0
	again := query(t, c, "24h", 1, 1, "")
	if *again.Devices[0].OnlineSeconds != 100 || *again.Devices[0].Links[0].SignalDBM != -62 {
		t.Fatal("query changed private metadata", again)
	}
}

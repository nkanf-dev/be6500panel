package traffic

import (
	"context"
	"errors"
	"math"
	"os"
	"path/filepath"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"be6500panel/internal/router"
)

type fakeSource struct {
	calls   atomic.Int64
	entered chan struct{}
}

func (s *fakeSource) Snapshot(ctx context.Context) (router.Snapshot, error) {
	call := s.calls.Add(1)
	if s.entered != nil {
		select {
		case s.entered <- struct{}{}:
		default:
		}
	}
	return snapshot(time.Unix(2000000000+call*2, 0), "eth0.2", uint64(call)*200, uint64(call)*20), ctx.Err()
}
func snapshot(at time.Time, source string, rx, tx uint64) router.Snapshot {
	return router.Snapshot{SampledAt: at, Routes: []router.Route{{Family: "ipv4", Destination: "0.0.0.0/0", Interface: source}}, Traffic: []router.Traffic{{Interface: source, RXBytes: rx, TXBytes: tx, RXBytesPerSecond: 999999, TXBytesPerSecond: 999999}}}
}
func newTestCollector(t *testing.T, dir string) *Collector {
	t.Helper()
	c, err := New(Options{DataDir: dir, Source: &fakeSource{}})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		if err := c.Close(); err != nil && !c.closed {
			t.Error(err)
		}
	})
	return c
}
func alignedTime() time.Time { return time.Unix(2000000000/3600*3600, 0).UTC() }
func queryTest(t *testing.T, c *Collector, name string, max int) History {
	t.Helper()
	h, err := c.Query(context.Background(), name, max)
	if err != nil {
		t.Fatal(err)
	}
	return h
}
func assertTotals(t *testing.T, h History, rx, tx uint64, coverage float64) {
	t.Helper()
	if h.Summary.RXBytes != rx || h.Summary.TXBytes != tx || math.Abs(h.Summary.CoverageSeconds-coverage) > 1e-6 {
		t.Fatalf("summary=%+v want=%d,%d,%f", h.Summary, rx, tx, coverage)
	}
	var totalRX, totalTX uint64
	var totalCoverage float64
	for _, s := range h.Samples {
		totalRX += s.RXBytes
		totalTX += s.TXBytes
		totalCoverage += s.CoverageSeconds
	}
	if totalRX != rx || totalTX != tx || math.Abs(totalCoverage-coverage) > 1e-6 {
		t.Fatalf("sample sums=%d,%d,%f", totalRX, totalTX, totalCoverage)
	}
}

func TestCounterDeltasRatesCoverageAndPeaks(t *testing.T) {
	c := newTestCollector(t, t.TempDir())
	start := alignedTime()
	// First observation is only a baseline, not traffic since boot.
	c.record(snapshot(start, "eth0.2", 9000000, 12000000))
	c.record(snapshot(start.Add(2*time.Second), "eth0.2", 9000200, 12000020))
	c.record(snapshot(start.Add(4*time.Second), "eth0.2", 9000800, 12000100))
	c.now = func() time.Time { return start.Add(4 * time.Second) }
	h := queryTest(t, c, "30m", 1500)
	assertTotals(t, h, 800, 100, 4)
	s := h.Samples[len(h.Samples)-1]
	if s.RX != 200 || s.TX != 25 || s.RXPeak != 300 || s.TXPeak != 40 {
		t.Fatal(s)
	}
	if h.OldestAt == nil || !h.OldestAt.Equal(start) || h.Source != "eth0.2" || !h.Persistent || !h.Enabled {
		t.Fatal(h)
	}
	if s.CoverageSeconds >= float64(h.ResolutionSeconds) {
		t.Fatal("partial bucket coverage was fabricated")
	}
}

func TestExactSplitsAcrossAllBucketBoundaries(t *testing.T) {
	c := newTestCollector(t, t.TempDir())
	start := alignedTime().Add(-time.Second)
	c.record(snapshot(start, "eth0.2", 100, 100))
	c.record(snapshot(start.Add(2*time.Second), "eth0.2", 107, 111))
	for _, r := range c.rings {
		var rx, tx, coverage uint64
		for _, b := range r.buckets {
			rx += b.rx
			tx += b.tx
			coverage += b.coverage
		}
		if rx != 7 || tx != 11 || coverage != uint64(2*time.Second) {
			t.Fatalf("tier=%d rx=%d tx=%d coverage=%d", r.seconds, rx, tx, coverage)
		}
		first := r.buckets[int((start.Unix()/r.seconds)%int64(r.capacity))]
		if first.rx != 3 || first.tx != 5 || first.coverage != uint64(time.Second) {
			t.Fatal(first)
		}
	}
	// Multiplication uses 128-bit arithmetic: large counters cannot wrap.
	if partBytes(math.MaxUint64, 9, 10) != 16602069666338596453 {
		t.Fatal("delta fraction overflow")
	}
}

func TestGapsResetsSourceChangeAndClockRegression(t *testing.T) {
	c := newTestCollector(t, t.TempDir())
	start := alignedTime()
	steps := []struct {
		offset int
		source string
		rx, tx uint64
	}{
		{0, "eth0.2", 100, 200}, {2, "eth0.2", 300, 600},
		{4, "eth0.2", 1, 2}, // both counters reset: do not count old boot bytes
		{6, "eth0.2", 21, 42},
		{8, "wan", 999000, 999000}, // source change establishes a new baseline
		{10, "wan", 999100, 999200},
		{40, "wan", 1000000, 1000000}, // long outage: delta is unknowable
		{42, "wan", 1000030, 1000040},
	}
	for _, step := range steps {
		c.record(snapshot(start.Add(time.Duration(step.offset)*time.Second), step.source, step.rx, step.tx))
	}
	// Missing interface/source error clears baseline. Recovery cannot bridge gap.
	c.record(router.Snapshot{SampledAt: start.Add(44 * time.Second)})
	c.record(snapshot(start.Add(46*time.Second), "wan", 1000040, 1000060))
	c.record(snapshot(start.Add(48*time.Second), "wan", 1000060, 1000100))
	// Backward clock sample and next sample behind latest are not re-counted.
	c.record(snapshot(start.Add(46*time.Second), "wan", 1000070, 1000110))
	c.record(snapshot(start.Add(47*time.Second), "wan", 1000080, 1000120))
	c.record(snapshot(start.Add(49*time.Second), "wan", 1000100, 1000140))
	c.now = func() time.Time { return start.Add(50 * time.Second) }
	h := queryTest(t, c, "30m", 1500)
	assertTotals(t, h, 370, 720, 10)
	for _, s := range h.Samples {
		if s.CoverageSeconds > float64(h.ResolutionSeconds) {
			t.Fatal("coverage overrun", s)
		}
	}
	// Traffic module errors preserve explicit uncovered buckets, not a zero rate
	// with full coverage. First row before recording has no observed coverage.
	broken := snapshot(start.Add(50*time.Second), "wan", 999999999, 999999999)
	broken.Errors = []router.ModuleError{{Module: "traffic", Code: "invalid"}}
	c.record(broken)
	h = queryTest(t, c, "30m", 1500)
	if h.Error == "" || h.Source != "" || h.Samples[0].CoverageSeconds != 0 {
		t.Fatal(h)
	}
}

func TestWANExactDefaultRouteNotBridgePhysicalOrNameGuess(t *testing.T) {
	at := alignedTime()
	s := snapshot(at, "pppoe-wan", 500, 600)
	s.Routes = append(s.Routes, router.Route{Family: "ipv4", Destination: "192.0.2.0/24", Interface: "wan", Metric: 0}, router.Route{Family: "ipv4", Destination: "0.0.0.0/0", Interface: "eth0.2", Metric: 10}, router.Route{Family: "ipv6", Destination: "::/0", Interface: "br-wan", Metric: 0})
	s.Traffic = append(s.Traffic, router.Traffic{Interface: "br-wan", RXBytes: 999999}, router.Traffic{Interface: "eth0.2", RXBytes: 999999}, router.Traffic{Interface: "wan", RXBytes: 999999})
	o, problem := wanObservation(s)
	if problem != "" || o.source != "pppoe-wan" || o.rx != 500 {
		t.Fatal(o, problem)
	}
	s.Errors = []router.ModuleError{{Module: "routes.ipv6", Code: "invalid"}}
	if _, problem = wanObservation(s); problem != "" {
		t.Fatal("unrelated IPv6 error discarded valid IPv4 WAN", problem)
	}
	s.Errors = []router.ModuleError{{Module: "routes.ipv4", Code: "invalid"}}
	if _, problem = wanObservation(s); problem == "" {
		t.Fatal("invalid chosen IPv4 default route accepted")
	}
	s.Routes = s.Routes[3:]
	o, problem = wanObservation(s)
	if problem != "" || o.source != "br-wan" {
		t.Fatal(o, problem)
	}
	s.Errors = []router.ModuleError{{Module: "routes.ipv6", Code: "unavailable"}}
	if _, problem = wanObservation(s); problem == "" {
		t.Fatal("invalid chosen IPv6 default route accepted")
	}
	s.Errors = nil
	s.Traffic = append(s.Traffic, router.Traffic{Interface: "br-wan"})
	if _, problem = wanObservation(s); problem == "" {
		t.Fatal("duplicate interface accepted")
	}
	s.Routes = make([]router.Route, 4097)
	if _, problem = wanObservation(s); problem == "" {
		t.Fatal("unbounded routes accepted")
	}
}

func TestOneYearRetentionBoundedRingsAndExactDownsampling(t *testing.T) {
	c := newTestCollector(t, t.TempDir())
	start := alignedTime()
	end := start.Add(405 * 24 * time.Hour)
	// Every measured 30s delta is independently retained in all three tiers.
	// Filling beyond every ring capacity exercises wrap-around without needing
	// 405 days of wall-clock execution or a browser subscriber.
	for at := start; at.Before(end); at = at.Add(30 * time.Second) {
		for _, r := range c.rings {
			r.add(at, at.Add(30*time.Second), 301, 97)
		}
	}
	c.now = func() time.Time { return end }
	for _, r := range c.rings {
		if len(r.buckets) != r.capacity || len(r.dirty) != r.capacity {
			t.Fatal("unbounded ring")
		}
		valid := 0
		for _, b := range r.buckets {
			if b.valid {
				valid++
			}
		}
		if valid != r.capacity {
			t.Fatal("ring missing wrap-around records")
		}
	}
	for _, name := range []string{"30m", "3h", "6h", "1d", "7d", "30d", "180d", "1y"} {
		h := queryTest(t, c, name, 137)
		seconds := ranges[name].Seconds()
		// 30d exactly fills its ring. The current empty bucket does not overwrite
		// the first measured bucket until an actual next measurement arrives.
		assertTotals(t, h, uint64(seconds/30)*301, uint64(seconds/30)*97, seconds)
		if len(h.Samples) > 137 {
			t.Fatal("query output exceeds bound", name, len(h.Samples))
		}
		if h.ResolutionSeconds <= 0 {
			t.Fatal(h)
		}
	}
	year := queryTest(t, c, "1y", 1500)
	if year.OldestAt == nil || !year.OldestAt.Equal(end.Add(-400*24*time.Hour)) {
		t.Fatal("400-day retention lost", year.OldestAt)
	}
	if err := c.Flush(); err != nil {
		t.Fatal(err)
	}
	files, err := os.ReadDir(filepath.Dir(c.rings[0].file.Name()))
	if err != nil {
		t.Fatal(err)
	}
	var size int64
	for _, f := range files {
		info, err := f.Info()
		if err != nil {
			t.Fatal(err)
		}
		size += info.Size()
	}
	if len(files) != 3 || size != DiskBytes {
		t.Fatalf("disk files=%d size=%d want=%d", len(files), size, DiskBytes)
	}
	if err := c.Close(); err != nil {
		t.Fatal(err)
	}
	reopened := newTestCollector(t, filepath.Dir(c.rings[0].file.Name()))
	reopened.now = c.now
	restored := queryTest(t, reopened, "1y", 1500)
	assertTotals(t, restored, year.Summary.RXBytes, year.Summary.TXBytes, year.Summary.CoverageSeconds)
}

func TestAggregationSumsBytesNotRatesAndKeepsGaps(t *testing.T) {
	c := newTestCollector(t, t.TempDir())
	start := alignedTime()
	r := c.rings[0]
	r.add(start, start.Add(2*time.Second), 200, 20)
	r.add(start.Add(60*time.Second), start.Add(66*time.Second), 1800, 180)
	c.now = func() time.Time { return start.Add(90 * time.Second) }
	h := queryTest(t, c, "30m", 1)
	assertTotals(t, h, 2000, 200, 8)
	if len(h.Samples) != 1 || h.Samples[0].RX != 250 || h.Samples[0].RXPeak != 300 || h.Samples[0].TX != 25 {
		t.Fatal(h.Samples)
	}
	h = queryTest(t, c, "30m", 1500)
	var covered, uncovered int
	for _, s := range h.Samples {
		if s.CoverageSeconds > 0 {
			covered++
		} else {
			uncovered++
		}
	}
	if covered != 2 || uncovered == 0 {
		t.Fatal("gaps erased", h.Samples)
	}
}

func TestRestartPersistenceNoCrossBootDeltaAndMinuteLossWindow(t *testing.T) {
	dir := t.TempDir()
	c := newTestCollector(t, dir)
	start := alignedTime()
	c.record(snapshot(start, "eth0.2", 1000, 1000))
	c.record(snapshot(start.Add(2*time.Second), "eth0.2", 1200, 1040))
	if err := c.Flush(); err != nil {
		t.Fatal(err)
	}
	c.record(snapshot(start.Add(4*time.Second), "eth0.2", 1600, 1120))
	// Simulate a process crash (not Close): dirty memory is deliberately lost.
	for _, r := range c.rings {
		if err := r.file.Close(); err != nil {
			t.Fatal(err)
		}
	}
	c.closed = true
	restored := newTestCollector(t, dir)
	restored.now = func() time.Time { return start.Add(12 * time.Second) }
	assertTotals(t, queryTest(t, restored, "30m", 1500), 200, 40, 2)
	// Counters could reset or already exceed pre-crash values on reboot. The
	// new process always takes a baseline, so neither makes a cross-boot delta.
	restored.record(snapshot(start.Add(10*time.Second), "eth0.2", 9000000, 9000000))
	restored.record(snapshot(start.Add(12*time.Second), "eth0.2", 9000030, 9000040))
	assertTotals(t, queryTest(t, restored, "30m", 1500), 230, 80, 4)
}

func TestTornSlotCorruptionAndTruncatedTailRecovery(t *testing.T) {
	for _, damage := range []string{"checksum", "torn", "tail"} {
		t.Run(damage, func(t *testing.T) {
			dir := t.TempDir()
			c := newTestCollector(t, dir)
			start := alignedTime()
			c.record(snapshot(start, "eth0.2", 100, 100))
			c.record(snapshot(start.Add(2*time.Second), "eth0.2", 300, 140))
			if err := c.Flush(); err != nil {
				t.Fatal(err)
			}
			c.record(snapshot(start.Add(4*time.Second), "eth0.2", 700, 220))
			if err := c.Flush(); err != nil {
				t.Fatal(err)
			}
			if err := c.Close(); err != nil {
				t.Fatal(err)
			}
			r := c.rings[0]
			index := int((start.Unix() / r.seconds) % int64(r.capacity))
			// The second write is generation 2, in slot zero. Slot one retains the
			// previously committed 2 seconds even if the second record is destroyed.
			offset := int64(headerSize + index*2*recordSize)
			f, err := os.OpenFile(r.file.Name(), os.O_RDWR, 0600)
			if err != nil {
				t.Fatal(err)
			}
			switch damage {
			case "checksum":
				_, err = f.WriteAt([]byte{0xfe}, offset+17)
			case "torn":
				_, err = f.WriteAt(make([]byte, 23), offset)
			case "tail":
				info, _ := f.Stat()
				err = f.Truncate(info.Size() - 10)
			}
			if err != nil {
				t.Fatal(err)
			}
			if err = f.Close(); err != nil {
				t.Fatal(err)
			}
			restored := newTestCollector(t, dir)
			restored.now = func() time.Time { return start.Add(4 * time.Second) }
			h := queryTest(t, restored, "30m", 1500)
			wantRX, wantTX, wantCoverage := uint64(200), uint64(40), float64(2)
			if damage == "tail" {
				wantRX = 600
				wantTX = 120
				wantCoverage = 4
			}
			assertTotals(t, h, wantRX, wantTX, wantCoverage)
			if h.Error == "" || !h.Persistent {
				t.Fatal("recovery not disclosed", h.Error)
			}
			info, err := os.Stat(r.file.Name())
			if err != nil {
				t.Fatal(err)
			}
			if info.Size() != int64(headerSize+r.capacity*recordSize*2) {
				t.Fatal("tail not repaired")
			}
		})
	}
}

func TestStorageErrorsAreVisibleAndHeaderIsNotSilentlyReplaced(t *testing.T) {
	dir := t.TempDir()
	c := newTestCollector(t, dir)
	start := alignedTime()
	c.record(snapshot(start, "eth0.2", 100, 100))
	c.record(snapshot(start.Add(2*time.Second), "eth0.2", 200, 200))
	c.now = func() time.Time { return start.Add(2 * time.Second) }
	if err := c.rings[0].file.Close(); err != nil {
		t.Fatal(err)
	}
	if err := c.Flush(); err == nil {
		t.Fatal("write error hidden")
	}
	h := queryTest(t, c, "30m", 1500)
	if h.Persistent || h.Error == "" || !h.Enabled {
		t.Fatal("write failure became silent volatile fallback", h)
	}
	_ = c.Close()
	f, err := os.OpenFile(filepath.Join(dir, "wan-30s.ring"), os.O_RDWR, 0600)
	if err != nil {
		t.Fatal(err)
	}
	_, err = f.WriteAt([]byte("bad!"), 0)
	if err != nil {
		t.Fatal(err)
	}
	_ = f.Close()
	if _, err = New(Options{DataDir: dir, Source: &fakeSource{}}); err == nil {
		t.Fatal("corrupt header silently reset")
	}
	if _, err = New(Options{Source: &fakeSource{}}); err == nil {
		t.Fatal("implicit volatile fallback")
	}
	if _, err = New(Options{DataDir: t.TempDir()}); err == nil {
		t.Fatal("missing source accepted")
	}
	file := filepath.Join(t.TempDir(), "not-a-directory")
	if err = os.WriteFile(file, []byte("x"), 0600); err != nil {
		t.Fatal(err)
	}
	if _, err = New(Options{DataDir: file, Source: &fakeSource{}}); err == nil {
		t.Fatal("invalid storage accepted")
	}
}

func TestQueryBoundsCancellationClosedAndFixedMemory(t *testing.T) {
	c := newTestCollector(t, t.TempDir())
	c.now = func() time.Time { return alignedTime() }
	for _, max := range []int{-1, 0, MaxPoints + 1, math.MaxInt} {
		if _, err := c.Query(context.Background(), "1y", max); !errors.Is(err, ErrMaxPoints) {
			t.Fatal(max, err)
		}
	}
	if _, err := c.Query(context.Background(), "2y", 1500); !errors.Is(err, ErrRange) {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := c.Query(ctx, "1y", 1500); !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
	for _, max := range []int{1, 2, 17, 1500, MaxPoints} {
		h := queryTest(t, c, "1y", max)
		if len(h.Samples) > max || h.OldestAt != nil || h.Summary.CoverageSeconds != 0 {
			t.Fatal(h)
		}
	}
	if err := c.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := c.Query(context.Background(), "30m", 1500); !errors.Is(err, ErrClosed) {
		t.Fatal(err)
	}
	if err := c.Flush(); !errors.Is(err, ErrClosed) {
		t.Fatal(err)
	}
	if err := c.Close(); err != nil {
		t.Fatal("non-idempotent close", err)
	}
}

func TestCentralCollectorWithoutSubscribersAndParentCancellation(t *testing.T) {
	source := &fakeSource{entered: make(chan struct{}, 2)}
	c, err := New(Options{DataDir: t.TempDir(), Source: source})
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	c.Start(ctx)
	c.Start(ctx) // repeated Start must not create duplicate collectors
	select {
	case <-source.entered:
	case <-time.After(time.Second):
		t.Fatal("no central collection without subscribers")
	}
	if source.calls.Load() != 1 {
		t.Fatal("duplicate start", source.calls.Load())
	}
	cancel()
	select {
	case <-c.done:
	case <-time.After(time.Second):
		t.Fatal("collector did not stop")
	}
	if err := c.Close(); err != nil {
		t.Fatal(err)
	}
}

func TestNoTemporaryArchivesOrUnboundedInterfaceFiles(t *testing.T) {
	dir := t.TempDir()
	c := newTestCollector(t, dir)
	start := alignedTime()
	for i := 0; i < 100; i++ {
		c.record(snapshot(start.Add(time.Duration(i)*2*time.Second), strings.Repeat("w", i%60+1), uint64(i), uint64(i)))
	}
	if err := c.Flush(); err != nil {
		t.Fatal(err)
	}
	files, err := os.ReadDir(dir)
	if err != nil {
		t.Fatal(err)
	}
	if len(files) != 3 {
		t.Fatal("interface names or archives grew disk", files)
	}
}

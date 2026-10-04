package traffic

import (
	"context"
	"crypto/sha256"
	"encoding/binary"
	"errors"
	"fmt"
	"hash/crc32"
	"os"
	"path/filepath"
	"reflect"
	"testing"
	"time"
)

var commonRangePresets = []struct {
	name          string
	duration      time.Duration
	nativeSeconds int64
}{
	{"30m", 30 * time.Minute, 30},
	{"1h", time.Hour, 30},
	{"3h", 3 * time.Hour, 30},
	{"6h", 6 * time.Hour, 30},
	{"10h", 10 * time.Hour, 30},
	{"12h", 12 * time.Hour, 30},
	{"1d", 24 * time.Hour, 30},
	{"3d", 3 * 24 * time.Hour, 300},
	{"7d", 7 * 24 * time.Hour, 300},
	{"30d", 30 * 24 * time.Hour, 300},
	{"180d", 180 * 24 * time.Hour, 3600},
	{"1y", 365 * 24 * time.Hour, 3600},
}

func TestCommonRangePresetValidation(t *testing.T) {
	if len(ranges) != len(commonRangePresets) {
		t.Fatalf("unexpected presets: %v", ranges)
	}
	for _, preset := range commonRangePresets {
		t.Run(preset.name, func(t *testing.T) {
			if ranges[preset.name] != preset.duration {
				t.Fatalf("duration=%v want=%v", ranges[preset.name], preset.duration)
			}
			for _, maxPoints := range []int{1, DefaultMaxPoints, MaxPoints} {
				if err := ValidateQuery(preset.name, maxPoints); err != nil {
					t.Fatal(err)
				}
			}
			for _, maxPoints := range []int{-1, 0, MaxPoints + 1} {
				if err := ValidateQuery(preset.name, maxPoints); !errors.Is(err, ErrMaxPoints) {
					t.Fatalf("maxPoints=%d err=%v", maxPoints, err)
				}
			}
		})
	}
	for _, name := range []string{"", "24h", "48h", "2d", "2y"} {
		if err := ValidateQuery(name, DefaultMaxPoints); !errors.Is(err, ErrRange) {
			t.Fatalf("noncanonical range=%q err=%v", name, err)
		}
	}
	if MaxPoints != 2000 || DefaultMaxPoints != 1500 {
		t.Fatal("range extension changed query bounds")
	}
}

func TestCommonRangePresetsUseOneExistingTierAndBoundResolution(t *testing.T) {
	c := newTestCollector(t, t.TempDir())
	start := alignedTime()
	c.now = func() time.Time { return start.Add(90 * time.Second) }
	// Deliberately distinct tier totals catch overlaps or selection of the wrong
	// tier. Real collection writes the same deltas to each tier.
	for i, r := range c.rings {
		r.add(start, start.Add(8*time.Second), uint64(100+i), uint64(10+i))
	}
	for _, preset := range commonRangePresets {
		t.Run(preset.name, func(t *testing.T) {
			var tierIndex int
			for i, r := range c.rings {
				if r.seconds == preset.nativeSeconds {
					tierIndex = i
				}
			}
			for _, maxPoints := range []int{1, 17, DefaultMaxPoints, MaxPoints} {
				h := queryTest(t, c, preset.name, maxPoints)
				assertTotals(t, h, uint64(100+tierIndex), uint64(10+tierIndex), 8)
				nativeCount := int64(preset.duration/time.Second)/preset.nativeSeconds + 1
				factor := (nativeCount + int64(maxPoints) - 1) / int64(maxPoints)
				wantResolution := preset.nativeSeconds * factor
				wantPoints := (nativeCount + factor - 1) / factor
				if h.Range != preset.name || h.ResolutionSeconds != wantResolution || int64(len(h.Samples)) != wantPoints || len(h.Samples) > maxPoints {
					t.Fatalf("range=%s maxPoints=%d resolution=%d points=%d want=%d,%d", h.Range, maxPoints, h.ResolutionSeconds, len(h.Samples), wantResolution, wantPoints)
				}
				if h.OldestAt == nil || !h.OldestAt.Equal(start) {
					t.Fatal("range choice fabricated earlier coverage", h.OldestAt)
				}
			}
		})
	}
}

func TestCommonRangePresetsReturnMeasuredTotalsAcrossRetainedSpans(t *testing.T) {
	c := newTestCollector(t, t.TempDir())
	end := alignedTime()
	for _, r := range c.rings {
		// Fill one bucket beyond each capacity to exercise wrap-around. These
		// synthetic measured intervals test capacity, not elapsed live history.
		from := end.Add(-time.Duration(int64(r.capacity+1)*r.seconds) * time.Second)
		for at := from; at.Before(end); at = at.Add(time.Duration(r.seconds) * time.Second) {
			r.add(at, at.Add(time.Duration(r.seconds)*time.Second), uint64(r.seconds)*10, uint64(r.seconds)*2)
		}
	}
	c.now = func() time.Time { return end }
	for _, preset := range commonRangePresets {
		t.Run(preset.name, func(t *testing.T) {
			seconds := uint64(preset.duration / time.Second)
			for _, maxPoints := range []int{137, DefaultMaxPoints, MaxPoints} {
				h := queryTest(t, c, preset.name, maxPoints)
				assertTotals(t, h, seconds*10, seconds*2, float64(seconds))
				if len(h.Samples) > maxPoints || h.OldestAt == nil || !h.OldestAt.Equal(end.Add(-400*24*time.Hour)) {
					t.Fatal("retained range exceeded its bounds or lost the old tier", len(h.Samples), h.OldestAt)
				}
			}
		})
	}
}

func TestCommonRangePresetsKeepCounterTotalsAndMissingCoverage(t *testing.T) {
	c := newTestCollector(t, t.TempDir())
	start := alignedTime()
	for _, r := range c.rings {
		r.add(start, start.Add(2*time.Second), 200, 20)
		r.add(start.Add(time.Minute), start.Add(66*time.Second), 1800, 180)
	}
	c.now = func() time.Time { return start.Add(90 * time.Second) }
	for _, preset := range commonRangePresets {
		t.Run(preset.name, func(t *testing.T) {
			h := queryTest(t, c, preset.name, 1)
			assertTotals(t, h, 2000, 200, 8)
			if len(h.Samples) != 1 || h.Samples[0].RX != 250 || h.Samples[0].TX != 25 || h.Samples[0].RXPeak != 300 || h.Samples[0].TXPeak != 30 {
				t.Fatal("downsampling summed rates instead of observed bytes/coverage", h.Samples)
			}
			h = queryTest(t, c, preset.name, MaxPoints)
			assertTotals(t, h, 2000, 200, 8)
			var uncovered int
			for _, sample := range h.Samples {
				if sample.CoverageSeconds == 0 {
					uncovered++
					if sample.RX != 0 || sample.TX != 0 || sample.RXBytes != 0 || sample.TXBytes != 0 {
						t.Fatal("uncovered bucket gained bytes", sample)
					}
				}
				if sample.CoverageSeconds > float64(h.ResolutionSeconds) {
					t.Fatal("coverage exceeded bucket length", sample)
				}
			}
			if uncovered == 0 || h.OldestAt == nil || !h.OldestAt.Equal(start) {
				t.Fatal("missing historical coverage was filled", h.OldestAt, uncovered)
			}
		})
	}
}

func TestCommonRangePresetsDoNotRewriteLegacyRingFiles(t *testing.T) {
	dir := t.TempDir()
	start := alignedTime()
	// Build the pre-extension physical layout independently of layouts and
	// DiskBytes. The fixture holds eight measured seconds, not a year of data.
	legacyLayouts := []struct {
		seconds  int64
		capacity int
		size     int
	}{
		{30, 5760, 737344},
		{300, 8640, 1105984},
		{3600, 9600, 1228864},
	}
	for _, layout := range legacyLayouts {
		data := make([]byte, layout.size)
		copy(data[:8], "WANRING\x00")
		binary.LittleEndian.PutUint64(data[8:16], uint64(layout.seconds))
		binary.LittleEndian.PutUint64(data[16:24], uint64(layout.capacity))
		binary.LittleEndian.PutUint32(data[60:64], crc32.ChecksumIEEE(data[:60]))
		b := bucket{start: start.Unix(), rx: 2000, tx: 200, coverage: uint64(8 * time.Second), rxPeak: 300, txPeak: 30, generation: 1, endMillis: 8000, valid: true}
		record := encodeBucket(b)
		index := int((b.start / layout.seconds) % int64(layout.capacity))
		offset := 64 + (index*2+1)*64
		copy(data[offset:offset+64], record[:])
		path := filepath.Join(dir, fmt.Sprintf("wan-%ds.ring", layout.seconds))
		if err := os.WriteFile(path, data, 0600); err != nil {
			t.Fatal(err)
		}
	}
	if DiskBytes != 3072192 || headerSize != 64 || recordSize != 64 {
		t.Fatal("range extension changed the disk format or budget")
	}
	type fileState struct {
		size int64
		hash [32]byte
	}
	states := func() map[string]fileState {
		t.Helper()
		entries, err := os.ReadDir(dir)
		if err != nil {
			t.Fatal(err)
		}
		result := make(map[string]fileState, len(entries))
		var total int64
		for _, entry := range entries {
			info, err := entry.Info()
			if err != nil {
				t.Fatal(err)
			}
			data, err := os.ReadFile(filepath.Join(dir, entry.Name()))
			if err != nil {
				t.Fatal(err)
			}
			total += info.Size()
			result[entry.Name()] = fileState{info.Size(), sha256.Sum256(data)}
		}
		if len(result) != 3 || total != 3072192 {
			t.Fatalf("ring files=%d bytes=%d", len(result), total)
		}
		return result
	}
	before := states()
	c, err := New(Options{
		DataDir: dir,
		Source:  &fakeSource{},
		StorageAdmission: func(_ context.Context, _ string, growth int64, recovery bool) (func(), error) {
			if growth != 0 || recovery {
				t.Fatalf("existing ranges requested disk growth=%d recovery=%v", growth, recovery)
			}
			return func() {}, nil
		},
		writeAt: func(_ *os.File, _ []byte, _ int64) (int, error) {
			t.Error("complete old rings requested allocation/backfill")
			return 0, errors.New("unexpected ring rewrite")
		},
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = c.Close() })
	c.now = func() time.Time { return start.Add(90 * time.Second) }
	for _, preset := range commonRangePresets {
		h := queryTest(t, c, preset.name, DefaultMaxPoints)
		assertTotals(t, h, 2000, 200, 8)
		if !h.Enabled || !h.Persistent || h.RetentionDays != 400 || h.Error != "" || h.OldestAt == nil || !h.OldestAt.Equal(start) || h.LastFlushAt != nil {
			t.Fatalf("preset=%s misrepresented restored coverage or durability: %+v", preset.name, h)
		}
	}
	if err := c.Flush(); err != nil {
		t.Fatal(err)
	}
	if err := c.Close(); err != nil {
		t.Fatal(err)
	}
	if after := states(); !reflect.DeepEqual(before, after) {
		t.Fatalf("query/open/clean flush changed old ring hashes or sizes: before=%v after=%v", before, after)
	}
}

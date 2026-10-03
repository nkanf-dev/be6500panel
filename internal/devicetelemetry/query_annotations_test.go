package devicetelemetry

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
	"testing"
	"time"
)

func TestAnnotationSearchMatchesBeforeDeviceLimitWithoutChangingIdentity(t *testing.T) {
	now := time.Date(2026, 10, 3, 0, 0, 0, 0, time.UTC)
	c := makeCollector(t, &now)
	rows := make([]Observation, MaxDevices)
	for i := range rows {
		rows[i] = observed(fmt.Sprintf("02:00:00:00:00:%02X", i), 100, 200)
		rows[i].Name = fmt.Sprintf("android-%03d", i)
		rows[i].Counters[0].Address = fmt.Sprintf("192.0.2.%d", i+1)
	}
	recordAt(c, now, rows...)
	now = now.Add(SampleInterval)
	for i := range rows {
		rows[i].Counters[0].RX += uint64(MaxDevices - i)
		rows[i].Counters[0].TX++
	}
	recordAt(c, now, rows...)
	last := rows[MaxDevices-1].ID
	base := query(t, c, "24h", 288, 64, "")
	for _, row := range base.Devices {
		if row.ID == last {
			t.Fatal("fixture last row must lie beyond first64")
		}
	}
	extra := map[string]string{last: "客厅电视 影视用途 media"}
	for _, search := range []string{"客厅电视", "影视用途", "media", "android-127", last} {
		found, err := c.QueryWithSearchText(context.Background(), "24h", 288, 64, search, extra)
		if err != nil || found.DeviceCount != 128 || found.MatchedCount != 1 || found.Truncated || len(found.Devices) != 1 || found.Devices[0].ID != last || found.Groups[0].DeviceCount != 1 {
			t.Fatal(search, found, err)
		}
		encoded, _ := json.Marshal(found)
		if strings.Contains(string(encoded), "客厅电视") || strings.Contains(string(encoded), "影视用途") {
			t.Fatal("private annotation text was returned in telemetry")
		}
	}
	plain := query(t, c, "24h", 288, 64, "客厅电视")
	if plain.MatchedCount != 0 {
		t.Fatal("legacy Query unexpectedly changed search semantics", plain)
	}
	unchanged := query(t, c, "24h", 288, 64, "")
	if unchanged.DeviceCount != 128 || len(unchanged.Devices) != 64 || !unchanged.Truncated {
		t.Fatal(unchanged)
	}
}
func TestAnnotationSearchBoundsAndCancellation(t *testing.T) {
	now := time.Now()
	c := makeCollector(t, &now)
	for _, extra := range []map[string]string{{"bad": "name"}, {"02:00:00:00:00:01": strings.Repeat("x", 8193)}, {"02:00:00:00:00:01": string([]byte{255})}} {
		if _, err := c.QueryWithSearchText(context.Background(), "24h", 288, 64, "name", extra); err != ErrQuery {
			t.Fatal(err)
		}
	}
	tooMany := map[string]string{}
	for i := 0; i < 257; i++ {
		tooMany[fmt.Sprintf("02:00:00:00:%02X:%02X", i/256, i%256)] = "name"
	}
	if _, err := c.QueryWithSearchText(context.Background(), "24h", 288, 64, "name", tooMany); err != ErrQuery {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := c.QueryWithSearchText(ctx, "24h", 288, 64, "name", nil); err != context.Canceled {
		t.Fatal(err)
	}
}

func TestCompleteMACSearchKeepsExactIdentityEvenIfOtherNotesContainIt(t *testing.T) {
	now := time.Now()
	c := makeCollector(t, &now)
	a := observed("02:00:00:00:00:01", 100, 200)
	b := observed("02:00:00:00:00:02", 100, 200)
	b.Counters[0].Address = "192.0.2.2"
	recordAt(c, now, a, b)
	now = now.Add(SampleInterval)
	a.Counters[0].RX++
	b.Counters[0].RX += 10000
	recordAt(c, now, a, b)
	extra := map[string]string{b.ID: "former owner " + a.ID}
	for _, search := range []string{a.ID, strings.ToLower(a.ID), "02-00-00-00-00-01"} {
		found, err := c.QueryWithSearchText(context.Background(), "24h", 288, 1, search, extra)
		if err != nil || found.MatchedCount != 1 || len(found.Devices) != 1 || found.Devices[0].ID != a.ID {
			t.Fatal(search, found, err)
		}
	}
}

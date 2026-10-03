package httpapi

import (
	"context"
	"encoding/json"
	"net/http/httptest"
	"strings"
	"testing"

	"be6500panel/internal/devicetelemetry"
)

type deviceHistorySource struct{ calls int }

func (s *deviceHistorySource) Snapshot(context.Context) (devicetelemetry.Snapshot, error) {
	s.calls++
	return devicetelemetry.Snapshot{}, nil
}
func TestDeviceActivityDisabledAndParameters(t *testing.T) {
	w := httptest.NewRecorder()
	DeviceActivity(w, httptest.NewRequest("GET", "/api/devices/activity", nil), nil)
	var history devicetelemetry.History
	if w.Code != 200 || json.Unmarshal(w.Body.Bytes(), &history) != nil || history.Enabled || history.Persistent || history.Range != "24h" || history.Source != "trafficd" || history.Devices == nil || history.Groups == nil || history.State != "unavailable" {
		t.Fatal(w.Code, w.Body.String())
	}
	for _, query := range []string{"range=1y", "maxPoints=0", "maxPoints=289", "maxPoints=no", "maxPoints=", "limit=0", "limit=65", "range=24h&range=7d", "limit=1&limit=2", "search=" + strings.Repeat("a", 65), "command=ubus"} {
		w = httptest.NewRecorder()
		DeviceActivity(w, httptest.NewRequest("GET", "/api/devices/activity?"+query, nil), nil)
		if w.Code != 400 {
			t.Fatal(query, w.Code, w.Body.String())
		}
	}
}
func TestDeviceActivityReadsHistoryOnlyAndCancellation(t *testing.T) {
	source := &deviceHistorySource{}
	c, err := devicetelemetry.New(source)
	if err != nil {
		t.Fatal(err)
	}
	defer c.Close()
	for _, query := range []string{"range=24h&limit=1&maxPoints=1", "range=7d&limit=64&maxPoints=288", "range=30m&search=02%3A00%3A00%3A00%3A00%3A01"} {
		w := httptest.NewRecorder()
		DeviceActivity(w, httptest.NewRequest("GET", "/api/devices/activity?"+query, nil), c)
		var h devicetelemetry.History
		if w.Code != 200 || json.Unmarshal(w.Body.Bytes(), &h) != nil || !h.Enabled || h.Persistent || h.State != "waiting" || h.Direction != "vendor-rx-tx" {
			t.Fatal(w.Code, w.Body.String())
		}
	}
	if source.calls != 0 {
		t.Fatal("browser triggered source reads", source.calls)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	w := httptest.NewRecorder()
	DeviceActivity(w, httptest.NewRequest("GET", "/api/devices/activity", nil).WithContext(ctx), c)
	if w.Code != 503 {
		t.Fatal(w.Code)
	}
	if err = c.Close(); err != nil {
		t.Fatal(err)
	}
	w = httptest.NewRecorder()
	DeviceActivity(w, httptest.NewRequest("GET", "/api/devices/activity", nil), c)
	if w.Code != 503 {
		t.Fatal(w.Code)
	}
}

package httpapi

import (
	"context"
	"encoding/json"
	"net/http/httptest"
	"testing"

	"be6500panel/internal/router"
	"be6500panel/internal/traffic"
)

type historySource struct{}

func (historySource) Snapshot(context.Context) (router.Snapshot, error) {
	return router.Snapshot{}, nil
}

func TestTrafficHistoryDisabledContract(t *testing.T) {
	w := httptest.NewRecorder()
	TrafficHistory(w, httptest.NewRequest("GET", "/api/traffic/history?range=1y&maxPoints=1500", nil), nil)
	if w.Code != 200 {
		t.Fatal(w.Code, w.Body.String())
	}
	var h traffic.History
	if err := json.Unmarshal(w.Body.Bytes(), &h); err != nil {
		t.Fatal(err)
	}
	if h.Enabled || h.Persistent || h.Range != "1y" || h.Samples == nil || h.Error == "" {
		t.Fatal(h)
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(w.Body.Bytes(), &raw); err != nil {
		t.Fatal(err)
	}
	for _, key := range []string{"enabled", "persistent", "retentionDays", "source", "range", "resolutionSeconds", "samples", "summary"} {
		if _, ok := raw[key]; !ok {
			t.Fatal("missing contract field", key)
		}
	}
}

func TestTrafficHistoryQueryValidationAndBounds(t *testing.T) {
	for _, query := range []string{"range=2y", "range=1y&maxPoints=0", "maxPoints=2001", "maxPoints=-1", "maxPoints=999999999999999999999999", "maxPoints=no", "maxPoints=", "maxPoints=1&maxPoints=2", "range=30m&range=1y"} {
		w := httptest.NewRecorder()
		TrafficHistory(w, httptest.NewRequest("GET", "/api/traffic/history?"+query, nil), nil)
		if w.Code != 400 {
			t.Fatal(query, w.Code, w.Body.String())
		}
	}
	c, err := traffic.New(traffic.Options{DataDir: t.TempDir(), Source: historySource{}})
	if err != nil {
		t.Fatal(err)
	}
	defer c.Close()
	for _, query := range []string{"", "range=30m&maxPoints=1", "range=1y&maxPoints=1500"} {
		w := httptest.NewRecorder()
		TrafficHistory(w, httptest.NewRequest("GET", "/api/traffic/history?"+query, nil), c)
		if w.Code != 200 {
			t.Fatal(query, w.Code, w.Body.String())
		}
		var h traffic.History
		if err = json.Unmarshal(w.Body.Bytes(), &h); err != nil {
			t.Fatal(err)
		}
		if !h.Enabled || !h.Persistent || h.RetentionDays < 365 || len(h.Samples) > 1500 {
			t.Fatal(h)
		}
	}
}

func TestTrafficHistoryCancelAndClosed(t *testing.T) {
	c, err := traffic.New(traffic.Options{DataDir: t.TempDir(), Source: historySource{}})
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	w := httptest.NewRecorder()
	r := httptest.NewRequest("GET", "/api/traffic/history", nil).WithContext(ctx)
	TrafficHistory(w, r, c)
	if w.Code != 503 {
		t.Fatal(w.Code)
	}
	if err = c.Close(); err != nil {
		t.Fatal(err)
	}
	w = httptest.NewRecorder()
	TrafficHistory(w, httptest.NewRequest("GET", "/api/traffic/history", nil), c)
	if w.Code != 503 {
		t.Fatal(w.Code)
	}
}

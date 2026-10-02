package httpapi

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"be6500panel/internal/telemetry"
	"be6500panel/internal/traffic"
)

func TestConnectedCollectorsDisabledRemainHonest(t *testing.T) {
	s, ts := testServer(t, "")
	defer ts.Close()
	defer s.Close()
	s.trafficError = "持久存储空间不足，实时状态仍可使用"
	for _, path := range []string{"/api/proxy/metrics", "/api/traffic/history?range=1y"} {
		w := httptest.NewRecorder()
		s.ServeHTTP(w, httptest.NewRequest(http.MethodGet, path, nil))
		if w.Code != 200 {
			t.Fatalf("%s: %d %s", path, w.Code, w.Body.String())
		}
		if path == "/api/proxy/metrics" {
			var value telemetry.Snapshot
			if err := json.Unmarshal(w.Body.Bytes(), &value); err != nil {
				t.Fatal(err)
			}
			if value.State != "unavailable" || len(value.Connections) != 0 || len(value.Traffic) != 0 {
				t.Fatal("disabled metrics fabricated data")
			}
		} else {
			var value traffic.History
			if err := json.Unmarshal(w.Body.Bytes(), &value); err != nil {
				t.Fatal(err)
			}
			if value.Enabled || value.Persistent || value.Error != s.trafficError || len(value.Samples) != 0 || value.RetentionDays != traffic.RetentionDays {
				t.Fatal("configured storage error lost")
			}
		}
	}
	for _, query := range []string{"?range=invalid", "?maxPoints=0", "?maxPoints=notnumber", "?maxPoints=", "?range=1d&range=1y"} {
		w := httptest.NewRecorder()
		s.ServeHTTP(w, httptest.NewRequest(http.MethodGet, "/api/traffic/history"+query, nil))
		if w.Code != 400 {
			t.Fatalf("unavailable collector accepted invalid query %s", query)
		}
	}
}

func TestCollectorRoutesRequireAuthAndStrictWrites(t *testing.T) {
	protected, server := testServer(t, "private-test-password")
	defer server.Close()
	defer protected.Close()
	for _, path := range []string{"/api/proxy/metrics", "/api/traffic/history?range=30m"} {
		w := httptest.NewRecorder()
		protected.ServeHTTP(w, httptest.NewRequest(http.MethodGet, path, nil))
		if w.Code != 401 {
			t.Fatalf("%s unprotected", path)
		}
	}
	s, ts := testServer(t, "")
	defer ts.Close()
	defer s.Close()
	for _, body := range []string{`{"url":"https://example.com"}`, `{"nodeId":"unselected"}`, `{"URL":"https://example.com"}`} {
		w := httptest.NewRecorder()
		r := httptest.NewRequest(http.MethodPost, "/api/proxy/probe", strings.NewReader(body))
		r.Header.Set("Content-Type", "application/json")
		s.ServeHTTP(w, r)
		if w.Code != 400 {
			t.Fatalf("probe accepted selector: %d", w.Code)
		}
	}
	w := httptest.NewRecorder()
	r := httptest.NewRequest(http.MethodPost, "/api/proxy/probe", strings.NewReader(`{}`))
	r.Header.Set("Content-Type", "application/json")
	s.ServeHTTP(w, r)
	if w.Code != 503 {
		t.Fatalf("disabled collector probe=%d", w.Code)
	}
}

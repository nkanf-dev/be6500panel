package httpapi

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"be6500panel/internal/requesttrace"
)

func TestRequestTracesGETDoesNotProbeAndBoundedContract(t *testing.T) {
	recorder := httptest.NewRecorder()
	HandleRequestTraces(recorder, httptest.NewRequest("GET", "/api/proxy/request-traces", nil), requesttrace.New(requesttrace.Config{}))
	var snap requesttrace.Snapshot
	if recorder.Code != 200 || json.Unmarshal(recorder.Body.Bytes(), &snap) != nil || len(snap.Traces) != 0 || len(snap.Targets) != 2 || snap.Limits.Capacity != 64 || snap.Running {
		t.Fatal("wrong idle contract", recorder.Code, recorder.Body.String())
	}
}
func TestRequestTracesPOSTExactFixedInput(t *testing.T) {
	c := requesttrace.New(requesttrace.Config{})
	for _, body := range []string{
		`{}`, `{"targetId":"google204"}`, `{"targetId":"google204","route":null}`,
		`{"targetId":"google204","route":"direct","proxyURL":"http://evil:2080"}`,
		`{"targetId":"https://evil.example","route":"direct"}`,
		`{"targetId":"google204","route":"direct","targetId":"cloudflare"}`,
		`{"targetId":"google204","Route":"direct"}`,
		`{"targetId":"google204","route":"auto"}`,
	} {
		req := httptest.NewRequest("POST", "/api/proxy/request-traces", strings.NewReader(body))
		req.Header.Set("Content-Type", "application/json")
		w := httptest.NewRecorder()
		HandleRequestTraces(w, req, c)
		if w.Code != 400 {
			t.Fatalf("unbounded input accepted: %s -> %d %s", body, w.Code, w.Body.String())
		}
	}
	if len(c.Snapshot().Traces) != 0 {
		t.Fatal("invalid input ran probes")
	}
	req := httptest.NewRequest("POST", "/api/proxy/request-traces", strings.NewReader(`{"targetId":"google204","route":"proxy"}`))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	HandleRequestTraces(w, req, c)
	if w.Code != 503 || !strings.Contains(w.Body.String(), "request_trace_unavailable") {
		t.Fatal("missing accepted provider not unavailable", w.Code, w.Body.String())
	}
}
func TestRequestTracesMethodQueryAndBodyBounds(t *testing.T) {
	c := requesttrace.New(requesttrace.Config{})
	for _, method := range []string{http.MethodGet, http.MethodPost} {
		w := httptest.NewRecorder()
		HandleRequestTraces(w, httptest.NewRequest(method, "/api/proxy/request-traces?targetId=evil", nil), c)
		if w.Code != 400 {
			t.Fatal("query selected probe", w.Code)
		}
	}
	w := httptest.NewRecorder()
	HandleRequestTraces(w, httptest.NewRequest("DELETE", "/api/proxy/request-traces", nil), c)
	if w.Code != 405 {
		t.Fatal("unsupported method accepted")
	}
	req := httptest.NewRequest("POST", "/api/proxy/request-traces", strings.NewReader(strings.Repeat("x", 1025)))
	req.Header.Set("Content-Type", "application/json")
	w = httptest.NewRecorder()
	HandleRequestTraces(w, req, c)
	if w.Code != 413 {
		t.Fatal("request input exceeds 1KiB", w.Code)
	}
	w = httptest.NewRecorder()
	HandleRequestTraces(w, httptest.NewRequest("GET", "/api/proxy/request-traces", nil), nil)
	if w.Code != 503 {
		t.Fatal("nil collector accepted")
	}
}

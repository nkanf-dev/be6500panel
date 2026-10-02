package httpapi

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestProductionDisabledContracts(t *testing.T) {
	s, ts := testServer(t, "")
	defer ts.Close()
	defer s.Close()
	for _, path := range []string{"/api/runtime", "/api/configuration/status", "/api/proxy/nodes"} {
		w := httptest.NewRecorder()
		s.ServeHTTP(w, httptest.NewRequest("GET", path, nil))
		if w.Code != 200 {
			t.Fatalf("%s=%d", path, w.Code)
		}
	}
	for _, path := range []string{"/api/runtime/start", "/api/configuration/commit", "/api/proxy/capture"} {
		w := httptest.NewRecorder()
		req := httptest.NewRequest("POST", path, strings.NewReader(`{}`))
		req.Header.Set("Content-Type", "application/json")
		s.ServeHTTP(w, req)
		if w.Code != 503 {
			t.Fatalf("%s=%d", path, w.Code)
		}
	}
	w := httptest.NewRecorder()
	s.ServeHTTP(w, httptest.NewRequest("GET", "/api/router", nil))
	if w.Code != 503 {
		t.Fatal(w.Code)
	}
}
func TestLargeEndpointDecoderKeepsStrictness(t *testing.T) {
	input := struct {
		Config string `json:"config"`
	}{}
	req := httptest.NewRequest(http.MethodPost, "/x", strings.NewReader(`{"config":"`+strings.Repeat("x", 128<<10)+`"}`))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	if !decodeJSONLimit(w, req, &input, 2<<20, "config") {
		t.Fatal(w.Body.String())
	}
	req = httptest.NewRequest(http.MethodPost, "/x", strings.NewReader(`{"config":"x","Config":"override"}`))
	req.Header.Set("Content-Type", "application/json")
	w = httptest.NewRecorder()
	if decodeJSONLimit(w, req, &input, 2<<20, "config") {
		t.Fatal("case override")
	}
}

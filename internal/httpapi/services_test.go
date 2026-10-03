package httpapi

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"be6500panel/internal/router"
)

func serviceHTTPFixture(t *testing.T) *router.ServiceObserver {
	t.Helper()
	root := t.TempDir()
	path := filepath.Join(root, "var/run/be6500panel")
	os.MkdirAll(path, 0700)
	os.WriteFile(filepath.Join(path, "ubus-service-list.json"), []byte(`{"be6500-rescue":{"instances":{"main":{"running":false}}}}`), 0600)
	os.MkdirAll(filepath.Join(root, "etc/init.d"), 0700)
	return router.NewServiceObserver(root)
}
func TestServiceHTTPObservationAndUnavailable(t *testing.T) {
	r := httptest.NewRequest(http.MethodGet, "/api/system/services", nil)
	w := httptest.NewRecorder()
	ServiceObservation(w, r, serviceHTTPFixture(t))
	if w.Code != 200 {
		t.Fatal(w.Code, w.Body.String())
	}
	var s router.ServiceSnapshot
	if err := json.Unmarshal(w.Body.Bytes(), &s); err != nil {
		t.Fatal(err)
	}
	if s.Stale || s.SampledAt == nil || len(s.Services) == 0 {
		t.Fatalf("%+v", s)
	}
	w = httptest.NewRecorder()
	ServiceObservation(w, r, nil)
	if w.Code != 503 {
		t.Fatal(w.Code)
	}
}
func TestServiceHTTPActionBoundedBodyAndRefusals(t *testing.T) {
	observer := serviceHTTPFixture(t)
	for _, tc := range []struct {
		body   string
		status int
		code   string
	}{
		{`{"service":"be6500-rescue","action":"stop","confirmImpact":true}`, 400, "service_action_not_allowed"},
		{`{"service":"ddns","action":"start","confirmImpact":false}`, 409, "fixture_read_only"},
		{`{"service":"dnsmasq","action":"reload","confirmImpact":false}`, 409, "service_impact_confirmation_required"},
		{`{"service":"ddns","action":"start","shell":"reboot"}`, 400, "invalid_input"},
		{strings.Repeat(" ", 1<<20), 413, "body_too_large"},
	} {
		w := httptest.NewRecorder()
		r := httptest.NewRequest(http.MethodPost, "/api/system/services/action", strings.NewReader(tc.body))
		r.Header.Set("Content-Type", "application/json")
		ServiceAction(w, r, observer)
		if w.Code != tc.status || !strings.Contains(w.Body.String(), tc.code) {
			t.Fatalf("%q -> %d %s", tc.body[:min(len(tc.body), 80)], w.Code, w.Body.String())
		}
	}
}

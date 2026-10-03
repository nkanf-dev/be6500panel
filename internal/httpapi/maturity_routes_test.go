package httpapi

import (
	"be6500panel/internal/deviceannotations"
	"be6500panel/internal/requesttrace"
	"be6500panel/internal/router"
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestMaturityRoutesRequireSession(t *testing.T) {
	_, ts := testServer(t, "correct")
	for _, path := range []string{"/api/devices/activity", "/api/proxy/request-traces", "/api/system/services", DeviceAnnotationsPath} {
		res := request(t, ts, "GET", path, "", nil)
		if res.StatusCode != http.StatusUnauthorized {
			t.Fatalf("%s: %s", path, res.Status)
		}
	}
	for _, path := range []string{"/api/proxy/request-traces", "/api/system/services/action", "/api/runtime/restart", DeviceAnnotationsPath} {
		res := request(t, ts, "POST", path, `{}`, nil)
		if res.StatusCode != http.StatusUnauthorized {
			t.Fatalf("%s: %s", path, res.Status)
		}
		req, _ := http.NewRequest("POST", ts.URL+path, strings.NewReader(`{}`))
		req.Header.Set("Origin", "https://evil.invalid")
		req.Header.Set("Content-Type", "application/json")
		response, err := ts.Client().Do(req)
		if err != nil {
			t.Fatal(err)
		}
		response.Body.Close()
		if response.StatusCode != http.StatusForbidden {
			t.Fatalf("origin %s: %s", path, response.Status)
		}
	}
}
func TestMaturityGETNeverStartsDiagnosticsOrActions(t *testing.T) {
	srv, ts := testServer(t, "")
	calls := 0
	srv.requestTraces = requesttrace.New(requesttrace.Config{ProxyProvider: func(context.Context) (requesttrace.ProxyEndpoint, error) {
		calls++
		return requesttrace.ProxyEndpoint{}, requesttrace.ErrUnavailable
	}})
	srv.services = router.NewServiceObserver(t.TempDir())
	for i := 0; i < 3; i++ {
		res := request(t, ts, "GET", "/api/proxy/request-traces", "", nil)
		body := decode(t, res)
		if res.StatusCode != 200 || body["traces"] == nil || len(body["traces"].([]any)) != 0 || calls != 0 {
			t.Fatal(res.Status, body, calls)
		}
	}
	res := request(t, ts, "GET", "/api/devices/activity", "", nil)
	if res.StatusCode != 200 || decode(t, res)["enabled"] != false {
		t.Fatal(res.Status)
	}
	res = request(t, ts, "GET", "/api/system/services", "", nil)
	if res.StatusCode != 200 || decode(t, res)["source"] != "procd/ubus + proc" {
		t.Fatal(res.Status)
	}
	res = request(t, ts, "POST", "/api/proxy/request-traces", `{"targetId":"google204","route":"proxy"}`, nil)
	if res.StatusCode != 503 || calls != 1 {
		t.Fatal(res.Status, calls)
	}
}
func TestNativeActionsNeedConfigurationCoordinator(t *testing.T) {
	srv, ts := testServer(t, "")
	srv.services = router.NewServiceObserver(t.TempDir())
	res := request(t, ts, "POST", "/api/system/services/action", `{"service":"dnsmasq","action":"restart","confirmImpact":true}`, nil)
	if res.StatusCode != 503 || decode(t, res)["error"].(map[string]any)["code"] != "control_unavailable" {
		t.Fatal(res.Status)
	}
	req := httptest.NewRequest("POST", "/api/system/services/action", strings.NewReader(`{"service":"be6500-rescue","action":"stop","confirmImpact":false}`))
	req.Header.Set("Content-Type", "application/json")
	response := httptest.NewRecorder()
	ServiceAction(response, req, srv.services)
	if response.Code != 400 || !strings.Contains(response.Body.String(), "service_action_not_allowed") {
		t.Fatal(response.Code, response.Body.String())
	}
}

func TestDeviceAnnotationSaveReadbackAndConflict(t *testing.T) {
	srv, ts := testServer(t, "")
	store, err := deviceannotations.New(deviceannotations.Options{DataDir: t.TempDir()})
	if err != nil {
		t.Fatal(err)
	}
	srv.deviceAnnotations = DeviceAnnotationsHandler(store)
	body := `{"mac":"02:00:00:00:00:01","label":"客厅测试电视","note":"独立回归备注","tags":["media"],"expectedRevision":0}`
	res := request(t, ts, "POST", DeviceAnnotationsPath, body, nil)
	saved := decode(t, res)
	if res.StatusCode != 200 || saved["revision"] != float64(1) {
		t.Fatal(res.Status, saved)
	}
	res = request(t, ts, "GET", DeviceAnnotationsPath, "", nil)
	read := decode(t, res)
	serialized, _ := json.Marshal(read["devices"])
	if res.StatusCode != 200 || read["revision"] != float64(1) || !strings.Contains(string(serialized), "客厅测试电视") {
		t.Fatal(res.Status, read)
	}
	res = request(t, ts, "POST", DeviceAnnotationsPath, body, nil)
	conflict := decode(t, res)
	if res.StatusCode != 409 || conflict["error"].(map[string]any)["code"] != "revision_conflict" {
		t.Fatal(res.Status, conflict)
	}
}

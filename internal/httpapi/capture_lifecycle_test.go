package httpapi

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"be6500panel/internal/capture"
	"be6500panel/internal/proxy"
	"be6500panel/internal/router"
)

func TestRouterPeerComesFromActualConnectionNotForwardedHeaders(t *testing.T) {
	server, ts := testServer(t, "")
	defer ts.Close()
	defer server.Close()
	server.router = router.New(t.TempDir())
	for _, peer := range []string{"192.0.2.10:52000", "[2001:db8::10]:53000"} {
		req := httptest.NewRequest(http.MethodGet, "/api/router", nil)
		req.RemoteAddr = peer
		req.Header.Set("X-Forwarded-For", "198.51.100.99")
		req.Header.Set("Forwarded", "for=198.51.100.99")
		w := httptest.NewRecorder()
		server.ServeHTTP(w, req)
		var response struct {
			CurrentClientIP string `json:"currentClientIP"`
		}
		if err := json.Unmarshal(w.Body.Bytes(), &response); err != nil {
			t.Fatal(err)
		}
		if response.CurrentClientIP != peerIP(req) || response.CurrentClientIP == "198.51.100.99" || response.CurrentClientIP == "" {
			t.Fatalf("wrong peer: %+v", response)
		}
	}
	req := httptest.NewRequest(http.MethodGet, "/api/router", nil)
	req.RemoteAddr = "not-a-peer"
	req.Header.Set("X-Forwarded-For", "198.51.100.99")
	w := httptest.NewRecorder()
	server.ServeHTTP(w, req)
	if strings.Contains(w.Body.String(), "currentClientIP") {
		t.Fatal("invented peer address", w.Body.String())
	}
}
func TestCaptureAPIStatusRetainsPendingDesiredAndDELETEPersistsDisable(t *testing.T) {
	server, ts := testServer(t, "")
	defer ts.Close()
	defer server.Close()
	dir := t.TempDir()
	controller, err := capture.New(dir, func(context.Context, []string) ([]byte, error) {
		t.Fatal("pending scope must not run kernel commands")
		return nil, nil
	})
	if err != nil {
		t.Fatal(err)
	}
	server.capture = controller
	controller.SetBuilder(func(ctx context.Context, d capture.Desired) (proxy.RulesPlanInput, []capture.Client, error) {
		return proxy.RulesPlanInput{}, []capture.Client{{MAC: d.Devices[0].MAC}}, errors.New("capture_device_unresolved")
	})
	if _, err = controller.Select(context.Background(), capture.Desired{Devices: []capture.DeviceSelection{{MAC: "02:00:00:00:00:10"}}, IPv6: proxy.IPv6Direct}); err == nil {
		t.Fatal("unresolved should remain pending")
	}
	w := httptest.NewRecorder()
	server.ServeHTTP(w, httptest.NewRequest(http.MethodGet, "/api/proxy/capture", nil))
	var response capture.Status
	if err = json.Unmarshal(w.Body.Bytes(), &response); err != nil {
		t.Fatal(err)
	}
	if w.Code != 200 || !response.Desired || response.Active || response.Clients[0].MAC != "02:00:00:00:00:10" || response.Clients[0].IP != "" || response.Error != "capture_device_unresolved" || response.State != "suspended" {
		t.Fatalf("pending contract lost: %+v", response)
	}
	w = httptest.NewRecorder()
	server.ServeHTTP(w, httptest.NewRequest(http.MethodDelete, "/api/proxy/capture", nil))
	if w.Code != 200 || controller.Status().Desired {
		t.Fatal(w.Body.String())
	}
	reopened, err := capture.New(dir, nil)
	if err != nil || reopened.Status().Desired {
		t.Fatal("DELETE did not disable boot intent", err)
	}
}

func TestNativeConfigurationOperationDisablesSavedScopeBeforeWrites(t *testing.T) {
	server, ts := testServer(t, "")
	defer ts.Close()
	defer server.Close()
	dir := t.TempDir()
	controller, err := capture.New(dir, func(context.Context, []string) ([]byte, error) {
		t.Fatal("unresolved scope ran kernel commands")
		return nil, nil
	})
	if err != nil {
		t.Fatal(err)
	}
	server.capture = controller
	controller.SetBuilder(func(ctx context.Context, d capture.Desired) (proxy.RulesPlanInput, []capture.Client, error) {
		return proxy.RulesPlanInput{}, []capture.Client{{MAC: d.Devices[0].MAC}}, errors.New("capture_device_unresolved")
	})
	_, _ = controller.Select(context.Background(), capture.Desired{Devices: []capture.DeviceSelection{{MAC: "02:00:00:00:00:10"}}, IPv6: proxy.IPv6Direct})
	err = server.nativeConfigurationOperation(context.Background(), func(context.Context) error {
		if controller.Status().Desired {
			t.Fatal("native writes raced saved scope")
		}
		reopened, err := capture.New(dir, nil)
		if err != nil || reopened.Status().Desired {
			t.Fatal("disable not persisted before native writes", err)
		}
		return errors.New("simulated native rollback failure")
	})
	if err == nil || controller.Status().Desired || len(controller.Status().Clients) != 1 {
		t.Fatal("native error restored capture or erased selected checkboxes")
	}
}

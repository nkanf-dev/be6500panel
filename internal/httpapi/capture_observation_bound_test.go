package httpapi

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"sync"
	"testing"
	"time"

	"be6500panel/internal/capture"
	"be6500panel/internal/proxy"
)

func TestCaptureGETDoesNotBlockDELETEWhileBuilderWaits(t *testing.T) {
	srv, ts := testServer(t, "")
	defer ts.Close()
	defer srv.Close()
	dir := t.TempDir()
	desired := capture.Desired{Enabled: true, Devices: []capture.DeviceSelection{{MAC: "02:be:65:00:00:fa"}}, IPv6: proxy.IPv6Direct}
	raw, _ := json.Marshal(desired)
	if err := os.WriteFile(filepath.Join(dir, "capture-desired.json"), raw, 0600); err != nil {
		t.Fatal(err)
	}
	controller, err := capture.New(dir, func(context.Context, []string) ([]byte, error) {
		t.Fatal("no installed plan must run no kernel commands")
		return nil, nil
	})
	if err != nil {
		t.Fatal(err)
	}
	srv.capture = controller
	entered := make(chan struct{})
	release := make(chan struct{})
	var once sync.Once
	t.Cleanup(func() { once.Do(func() { close(release) }) })
	controller.SetBuilder(func(ctx context.Context, d capture.Desired) (proxy.RulesPlanInput, []capture.Client, error) {
		close(entered)
		select {
		case <-ctx.Done():
			return proxy.RulesPlanInput{}, nil, ctx.Err()
		case <-release:
			return proxy.RulesPlanInput{}, []capture.Client{{MAC: "02:be:65:00:00:fa"}}, errors.New("capture_backend_not_ready")
		}
	})
	getDone := make(chan *httptest.ResponseRecorder, 1)
	go func() {
		reply := httptest.NewRecorder()
		srv.ServeHTTP(reply, httptest.NewRequest(http.MethodGet, "/api/proxy/capture", nil))
		getDone <- reply
	}()
	select {
	case <-entered:
	case <-time.After(time.Second):
		t.Fatal("GET did not enter builder")
	}
	deleteDone := make(chan *httptest.ResponseRecorder, 1)
	go func() {
		reply := httptest.NewRecorder()
		srv.ServeHTTP(reply, httptest.NewRequest(http.MethodDelete, "/api/proxy/capture", nil))
		deleteDone <- reply
	}()
	select {
	case reply := <-deleteDone:
		if reply.Code != 200 {
			t.Fatal(reply.Code, reply.Body.String())
		}
	case <-time.After(400 * time.Millisecond):
		t.Fatal("GET pinned capture lock and blocked DELETE")
	}
	once.Do(func() { close(release) })
	select {
	case reply := <-getDone:
		var state capture.Status
		if reply.Code != 200 || json.Unmarshal(reply.Body.Bytes(), &state) != nil || state.Desired || state.Active {
			t.Fatal("stale GET resurrected state", reply.Code, reply.Body.String())
		}
	case <-time.After(time.Second):
		t.Fatal("GET did not return after stale builder completed")
	}
	restored, err := capture.New(dir, nil)
	if err != nil || restored.Desired().Enabled {
		t.Fatal("GET/DELETE did not keep persistent off", err)
	}
}

package httpapi

import (
	"be6500panel/internal/control"
	managedruntime "be6500panel/internal/runtime"
	"context"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
	"time"
)

func TestRestartRouteFixedOwnerAndStateReadback(t *testing.T) {
	srv, ts := testServer(t, "")
	runtime, err := managedruntime.New(managedruntime.Options{DataDir: t.TempDir(), RunDir: t.TempDir()})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { runtime.Close() })
	srv.runtime = runtime
	for _, service := range []string{"be6500-rescue", "dropbear", "network", "../../bin/sh"} {
		res := request(t, ts, "POST", "/api/runtime/restart", `{"service":"`+service+`"}`, nil)
		result := decode(t, res)
		if res.StatusCode != 400 || result["error"].(map[string]any)["code"] != "invalid_service" {
			t.Fatal(service, res.Status, result)
		}
	}
	for _, service := range []string{managedruntime.SingBox, managedruntime.FRPC} {
		res := request(t, ts, "POST", "/api/runtime/restart", `{"service":"`+service+`"}`, nil)
		result := decode(t, res)
		if res.StatusCode != 409 || result["error"].(map[string]any)["code"] != "not_configured" {
			t.Fatal(service, res.Status, result)
		}
		state, err := runtime.Status(service)
		if err != nil || state.Desired || state.PID != 0 {
			t.Fatal(state, err)
		}
	}
}
func TestRestartRouteRefusesProvisionalConfiguration(t *testing.T) {
	srv, ts := testServer(t, "")
	manager := installMaintenanceFixture(t, srv)
	runtime, err := managedruntime.New(managedruntime.Options{DataDir: t.TempDir(), RunDir: t.TempDir()})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { runtime.Close() })
	srv.runtime = runtime
	docs, err := manager.Documents(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	content := ""
	for _, d := range docs.Documents {
		if d.Module == "network" {
			content = d.Content
		}
	}
	draft, err := manager.Stage(context.Background(), control.StageRequest{Module: "network", Content: strings.Replace(content, "192.0.2.1", "192.0.2.2", 1), Generation: docs.Generation})
	if err != nil {
		t.Fatal(err)
	}
	op, err := manager.Commit(context.Background(), control.CommitRequest{DraftIDs: []string{draft.ID}, Generation: draft.Generation, AcknowledgeRisks: true})
	if err != nil {
		t.Fatal(err)
	}
	res := request(t, ts, "POST", "/api/runtime/restart", `{"service":"sing-box"}`, nil)
	result := decode(t, res)
	if res.StatusCode != http.StatusConflict || result["error"].(map[string]any)["code"] != "configuration_pending" {
		t.Fatal(res.Status, result)
	}
	if _, err := manager.Rollback(context.Background(), op.ID); err != nil {
		t.Fatal(err)
	}
}

type blockedRestartBody struct {
	entered chan struct{}
	release chan struct{}
	once    sync.Once
	reader  io.Reader
}

func (b *blockedRestartBody) Read(p []byte) (int, error) {
	b.once.Do(func() { close(b.entered); <-b.release })
	return b.reader.Read(p)
}
func (b *blockedRestartBody) Close() error { return nil }
func TestRestartRefusesNativeCommitCompletedWhileBodyWasLoading(t *testing.T) {
	srv, _ := testServer(t, "")
	manager := installMaintenanceFixture(t, srv)
	runtime, err := managedruntime.New(managedruntime.Options{DataDir: t.TempDir(), RunDir: t.TempDir()})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { runtime.Close() })
	srv.runtime = runtime
	body := &blockedRestartBody{entered: make(chan struct{}), release: make(chan struct{}), reader: strings.NewReader(`{"service":"sing-box"}`)}
	var releaseOnce sync.Once
	release := func() { releaseOnce.Do(func() { close(body.release) }) }
	t.Cleanup(release)
	req := httptest.NewRequest("POST", "/api/runtime/restart", nil)
	req.Body = body
	req.Header.Set("Content-Type", "application/json")
	response := httptest.NewRecorder()
	done := make(chan struct{})
	go func() { srv.ServeHTTP(response, req); close(done) }()
	select {
	case <-body.entered:
	case <-time.After(2 * time.Second):
		t.Fatal("request did not pass outer gate")
	}
	docs, err := manager.Documents(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	content := ""
	for _, d := range docs.Documents {
		if d.Module == "network" {
			content = d.Content
		}
	}
	draft, err := manager.Stage(context.Background(), control.StageRequest{Module: "network", Content: strings.Replace(content, "192.0.2.1", "192.0.2.2", 1), Generation: docs.Generation})
	if err != nil {
		t.Fatal(err)
	}
	var operation control.Operation
	err = runtime.ResourceOperation(context.Background(), managedruntime.SingBox, func(ctx context.Context) error {
		var e error
		operation, e = manager.Commit(ctx, control.CommitRequest{DraftIDs: []string{draft.ID}, Generation: draft.Generation, AcknowledgeRisks: true})
		return e
	})
	if err != nil {
		t.Fatal(err)
	}
	if operation.State != "pending_confirmation" {
		t.Fatal(operation)
	}
	release()
	select {
	case <-done:
	case <-time.After(2 * time.Second):
		t.Fatal("restart did not finish")
	}
	if response.Code != 409 || !strings.Contains(response.Body.String(), "configuration_pending") {
		t.Fatal(response.Code, response.Body.String())
	}
	state, err := runtime.Status(managedruntime.SingBox)
	if err != nil || state.PID != 0 || state.Desired || state.State != managedruntime.NotConfigured {
		t.Fatal(state, err)
	}
	if _, err := manager.Rollback(context.Background(), operation.ID); err != nil {
		t.Fatal(err)
	}
}

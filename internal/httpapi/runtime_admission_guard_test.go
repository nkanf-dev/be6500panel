package httpapi

import (
	"be6500panel/internal/control"
	managedruntime "be6500panel/internal/runtime"
	"context"
	"encoding/json"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"
)

func pendingNativeGuardFixture(t *testing.T, manager *control.Manager, runtime *managedruntime.Manager) control.Operation {
	t.Helper()
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
		var commitErr error
		operation, commitErr = manager.Commit(ctx, control.CommitRequest{DraftIDs: []string{draft.ID}, Generation: draft.Generation, AcknowledgeRisks: true})
		return commitErr
	})
	if err != nil || operation.State != "pending_confirmation" {
		t.Fatal(operation, err)
	}
	return operation
}
func TestRuntimeMutationsRecheckNativeGateAfterDelayedBody(t *testing.T) {
	for _, test := range []struct{ path, body string }{
		{"/api/runtime/acquire", `{"service":"frpc","artifact":{"url":"https://example.invalid/frpc.gz","sha256":"` + strings.Repeat("a", 64) + `","compression":"gzip","version":"fixture"}}`},
		{"/api/runtime/configure", `{"service":"frpc","config":"serverAddr = \"example.invalid\"\nserverPort = 7000\n","generation":0}`},
		{"/api/runtime/restore", `{"service":"frpc","generation":0}`},
		{"/api/runtime/start", `{"service":"frpc"}`},
	} {
		t.Run(test.path, func(t *testing.T) {
			srv, _ := testServer(t, "")
			manager := installMaintenanceFixture(t, srv)
			runtime, err := managedruntime.New(managedruntime.Options{DataDir: t.TempDir(), RunDir: t.TempDir()})
			if err != nil {
				t.Fatal(err)
			}
			t.Cleanup(func() { runtime.Close() })
			srv.runtime = runtime
			srv.dataDir = t.TempDir()
			desiredPath := filepath.Join(srv.dataDir, "desired-services.json")
			priorDesired := []byte(`{"sing-box":false,"frpc":false}`)
			if err := os.WriteFile(desiredPath, priorDesired, 0600); err != nil {
				t.Fatal(err)
			}
			body := &blockedRestartBody{entered: make(chan struct{}), release: make(chan struct{}), reader: strings.NewReader(test.body)}
			var once sync.Once
			release := func() { once.Do(func() { close(body.release) }) }
			t.Cleanup(release)
			req := httptest.NewRequest("POST", test.path, nil)
			req.Body = body
			req.Header.Set("Content-Type", "application/json")
			response := httptest.NewRecorder()
			finished := make(chan struct{})
			go func() { srv.ServeHTTP(response, req); close(finished) }()
			select {
			case <-body.entered:
			case <-time.After(2 * time.Second):
				t.Fatal("request did not pass outer gate")
			}
			operation := pendingNativeGuardFixture(t, manager, runtime)
			release()
			select {
			case <-finished:
			case <-time.After(2 * time.Second):
				t.Fatal("refused mutation did not finish")
			}
			if response.Code != 409 || !strings.Contains(response.Body.String(), "configuration_pending") {
				t.Fatal(response.Code, response.Body.String())
			}
			state, err := runtime.Status(managedruntime.FRPC)
			if err != nil || state.Configured || state.ArtifactAvailable || state.Generation != 0 || state.Desired || state.PID != 0 || state.State != managedruntime.NotConfigured {
				t.Fatal(state, err)
			}
			after, err := os.ReadFile(desiredPath)
			if err != nil || string(after) != string(priorDesired) {
				t.Fatal("refused mutation persisted desired state", string(after), err)
			}
			if _, err := manager.Rollback(context.Background(), operation.ID); err != nil {
				t.Fatal(err)
			}
		})
	}
}
func TestProxySelectionRechecksNativeGateAfterCompilationBeforeAdmission(t *testing.T) {
	srv, ts := policyServer(t)
	manager := installMaintenanceFixture(t, srv)
	stagePolicyRuleSets(t, srv.dataDir)
	priorDesired := []byte(`{"sing-box":false,"frpc":false}`)
	desiredPath := filepath.Join(srv.dataDir, "desired-services.json")
	if err := os.WriteFile(desiredPath, priorDesired, 0600); err != nil {
		t.Fatal(err)
	}
	entered := make(chan struct{})
	releaseAdmission := make(chan struct{})
	var once sync.Once
	release := func() { once.Do(func() { close(releaseAdmission) }) }
	t.Cleanup(release)
	srv.storageAdmission = func(ctx context.Context, path string, bytes int64, recovery bool) (func(), error) {
		close(entered)
		select {
		case <-releaseAdmission:
			return func() {}, nil
		case <-ctx.Done():
			return nil, ctx.Err()
		}
	}
	sub := srv.proxyState.subscription
	revision := summarizeProxyPolicy(sub).Revision
	body := selectPolicyBody(sub.Nodes[0].ID, revision)
	completed := make(chan *httptest.ResponseRecorder, 1)
	go func() {
		req := httptest.NewRequest("POST", ts.URL+"/api/proxy/select", strings.NewReader(body))
		req.Header.Set("Content-Type", "application/json")
		response := httptest.NewRecorder()
		srv.ServeHTTP(response, req)
		completed <- response
	}()
	select {
	case <-entered:
	case <-time.After(2 * time.Second):
		t.Fatal("proxy did not reach pre-runtime admission")
	}
	operation := pendingNativeGuardFixture(t, manager, srv.runtime)
	release()
	var response *httptest.ResponseRecorder
	select {
	case response = <-completed:
	case <-time.After(2 * time.Second):
		t.Fatal("refused proxy apply did not finish")
	}
	if response.Code != 409 {
		t.Fatal(response.Code, response.Body.String())
	}
	var payload map[string]any
	if err := json.Unmarshal(response.Body.Bytes(), &payload); err != nil {
		t.Fatal(err)
	}
	if payload["error"].(map[string]any)["code"] != "configuration_pending" {
		t.Fatal(payload)
	}
	state, err := srv.runtime.Status(managedruntime.SingBox)
	if err != nil || state.Configured || state.Generation != 0 || state.Desired || state.PID != 0 {
		t.Fatal(state, err)
	}
	if _, err := os.Stat(filepath.Join(srv.dataDir, "proxy-selection.json")); !os.IsNotExist(err) {
		t.Fatal("refused apply saved selection", err)
	}
	if srv.proxyState.selected != "" {
		t.Fatal("refused apply changed in-memory selection")
	}
	after, err := os.ReadFile(desiredPath)
	if err != nil || string(after) != string(priorDesired) {
		t.Fatal("refused proxy apply saved desired state", string(after), err)
	}
	if _, err := manager.Rollback(context.Background(), operation.ID); err != nil {
		t.Fatal(err)
	}
}

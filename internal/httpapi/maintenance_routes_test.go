package httpapi

import (
	"be6500panel/internal/control"
	"be6500panel/internal/maintenance"
	"be6500panel/internal/router"
	managedruntime "be6500panel/internal/runtime"
	"context"
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func installMaintenanceFixture(t *testing.T, srv *Server) *control.Manager {
	t.Helper()
	root := t.TempDir()
	dir := filepath.Join(root, "etc", "config")
	if err := os.MkdirAll(dir, 0700); err != nil {
		t.Fatal(err)
	}
	docs := map[string]string{
		"network":  "config interface 'lan'\n option proto 'static'\n option ipaddr '192.0.2.1'\n",
		"wireless": "config wifi-device 'radio0'\n option disabled '1'\n",
		"dhcp":     "config dnsmasq\n option domain 'lan'\n",
		"firewall": "config defaults\n option input 'ACCEPT'\n option output 'ACCEPT'\n option forward 'REJECT'\n",
		"system":   "config system\n option hostname 'fixture-router'\n",
		"dropbear": "config dropbear\n option Port '22'\n",
	}
	for name, text := range docs {
		if err := os.WriteFile(filepath.Join(dir, name), []byte(text), 0600); err != nil {
			t.Fatal(err)
		}
	}
	manager, err := control.New(control.Options{Root: root, DataDir: t.TempDir(), Runner: func(context.Context, string, ...string) ([]byte, error) { return []byte{}, nil }, Reload: func(context.Context, string) error { return nil }, Verify: func(context.Context, []string) error { return nil }, ConfirmationTimeout: time.Hour})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { manager.Close() })
	srv.control = manager
	srv.maintenance = maintenance.New(maintenance.Options{Native: manager, Metadata: func(context.Context) (maintenance.Metadata, error) {
		return maintenance.Metadata{Model: "fixture", Build: "fixture-build"}, nil
	}})
	return manager
}
func TestMaintenanceHTTPStageDoesNotApplyOrStart(t *testing.T) {
	srv, ts := testServer(t, "")
	manager := installMaintenanceFixture(t, srv)
	res := request(t, ts, "POST", "/api/maintenance/backup", `{"scopes":["system"]}`, nil)
	original, err := io.ReadAll(res.Body)
	if err != nil {
		t.Fatal(err)
	}
	if res.StatusCode != 200 || res.Header.Get("Cache-Control") != "no-store" || !strings.Contains(res.Header.Get("Content-Disposition"), "attachment;") {
		t.Fatal(res.Status, res.Header)
	}
	envelope, err := maintenance.Decode(original)
	if err != nil {
		t.Fatal(err)
	}
	docs, err := manager.Documents(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	systemBefore := ""
	for _, d := range docs.Documents {
		if d.Module == "system" {
			systemBefore = d.Content
		}
	}
	if envelope.Documents[0].Content != systemBefore || len(envelope.Scopes) != 1 {
		t.Fatal(envelope)
	}
	baselineGeneration := manager.Status().Generation
	envelope.Documents[0].Content = strings.Replace(systemBefore, "fixture-router", "fixture-router-edited", 1)
	envelope.Documents[0].Digest = fmt.Sprintf("%x", sha256.Sum256([]byte(envelope.Documents[0].Content)))
	modified, err := json.Marshal(envelope)
	if err != nil {
		t.Fatal(err)
	}
	res = request(t, ts, "POST", "/api/maintenance/import/preview", string(modified), nil)
	preview := decode(t, res)
	if res.StatusCode != 200 {
		t.Fatal(res.Status, preview)
	}
	res = request(t, ts, "DELETE", "/api/maintenance/import/preview?id="+preview["id"].(string), "", nil)
	if res.StatusCode != 200 || decode(t, res)["discarded"] != true {
		t.Fatal(res.Status)
	}
	res = request(t, ts, "POST", "/api/maintenance/import/stage", `{"previewId":"`+preview["id"].(string)+fmt.Sprintf(`","generation":%d,"modules":["system"],"acknowledgeModelMismatch":false}`, baselineGeneration), nil)
	if res.StatusCode != 404 || decode(t, res)["error"].(map[string]any)["code"] != "preview_not_found" {
		t.Fatal(res.Status)
	}
	res = request(t, ts, "POST", "/api/maintenance/import/preview", string(modified), nil)
	freshPreview := decode(t, res)
	if res.StatusCode != 200 {
		t.Fatal(res.Status, freshPreview)
	}
	freshStage, err := json.Marshal(map[string]any{"previewId": freshPreview["id"], "generation": baselineGeneration, "modules": []string{"system"}, "acknowledgeModelMismatch": false})
	if err != nil {
		t.Fatal(err)
	}
	res = request(t, ts, "POST", "/api/maintenance/import/stage", string(freshStage), nil)
	stageResult := decode(t, res)
	if res.StatusCode != 200 || len(stageResult["drafts"].([]any)) != 1 {
		t.Fatal(res.Status, stageResult)
	}
	res = request(t, ts, "GET", "/api/configuration/drafts", "", nil)
	queued := decode(t, res)
	if res.StatusCode != 200 || len(queued["drafts"].([]any)) != 1 {
		t.Fatal(res.Status, queued)
	}
	for _, scope := range []string{"/data/ssh", "shadow", "device-names", "rescue"} {
		body, _ := json.Marshal(map[string]any{"scopes": []string{scope}})
		res = request(t, ts, "POST", "/api/maintenance/backup", string(body), nil)
		if res.StatusCode != 400 {
			t.Fatalf("scope=%s status=%s", scope, res.Status)
		}
	}
	after, err := manager.Documents(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	for _, d := range after.Documents {
		if d.Module == "system" && d.Content != systemBefore {
			t.Fatal("preview changed live config")
		}
	}
	drafts, err := manager.Drafts(context.Background())
	if err != nil || len(drafts) != 1 || manager.Status().Generation != baselineGeneration {
		t.Fatal(drafts, err, manager.Status())
	}
}
func TestMaintenancePrivateRoutesAuthAndOrigin(t *testing.T) {
	srv, ts := testServer(t, "correct")
	installMaintenanceFixture(t, srv)
	for _, entry := range []struct{ method, path string }{
		{"POST", "/api/maintenance/backup"}, {"POST", "/api/maintenance/import/preview"}, {"POST", "/api/maintenance/import/stage"}, {"DELETE", "/api/maintenance/import/preview?id=x"},
	} {
		res := request(t, ts, entry.method, entry.path, `{}`, nil)
		if res.StatusCode != http.StatusUnauthorized {
			t.Fatal(entry, res.Status)
		}
		req, _ := http.NewRequest(entry.method, ts.URL+entry.path, strings.NewReader(`{}`))
		req.Header.Set("Content-Type", "application/json")
		req.Header.Set("Origin", "https://evil.invalid")
		response, err := ts.Client().Do(req)
		if err != nil {
			t.Fatal(err)
		}
		response.Body.Close()
		if response.StatusCode != http.StatusForbidden {
			t.Fatal(entry, response.Status)
		}
	}
}

func TestNativeServiceActionsRejectPendingConfigurationAndBusyLane(t *testing.T) {
	srv, ts := testServer(t, "")
	manager := installMaintenanceFixture(t, srv)
	runtime, err := managedruntime.New(managedruntime.Options{DataDir: t.TempDir(), RunDir: t.TempDir()})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { runtime.Close() })
	srv.runtime = runtime
	srv.services = router.NewServiceObserver(t.TempDir())
	requestBody := `{"service":"dnsmasq","action":"restart","confirmImpact":true}`
	admitted := make(chan struct{})
	release := make(chan struct{})
	completed := make(chan error, 1)
	go func() {
		completed <- runtime.ResourceOperation(context.Background(), managedruntime.SingBox, func(context.Context) error { close(admitted); <-release; return nil })
	}()
	<-admitted
	res := request(t, ts, "POST", "/api/system/services/action", requestBody, nil)
	payload := decode(t, res)
	close(release)
	if err := <-completed; err != nil {
		t.Fatal(err)
	}
	if res.StatusCode != 409 || payload["error"].(map[string]any)["code"] != "operation_busy" {
		t.Fatal(res.Status, payload)
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
	staged, err := manager.Stage(context.Background(), control.StageRequest{Module: "network", Content: strings.Replace(content, "192.0.2.1", "192.0.2.2", 1), Generation: docs.Generation})
	if err != nil {
		t.Fatal(err)
	}
	operation, err := manager.Commit(context.Background(), control.CommitRequest{DraftIDs: []string{staged.ID}, Generation: staged.Generation, AcknowledgeRisks: true})
	if err != nil || operation.State != "pending_confirmation" {
		t.Fatal(operation, err)
	}
	res = request(t, ts, "POST", "/api/system/services/action", requestBody, nil)
	payload = decode(t, res)
	if res.StatusCode != 409 || payload["error"].(map[string]any)["code"] != "configuration_pending" {
		t.Fatal(res.Status, payload)
	}
	if _, err := manager.Rollback(context.Background(), operation.ID); err != nil {
		t.Fatal(err)
	}
}

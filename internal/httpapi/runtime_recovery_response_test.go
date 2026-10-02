package httpapi

import (
	"encoding/json"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"

	managedruntime "be6500panel/internal/runtime"
)

func TestRuntimeFailureKeepsAuthoritativeRecoveredState(t *testing.T) {
	s, ts := testServer(t, "")
	defer ts.Close()
	defer s.Close()
	s.dataDir = t.TempDir()
	state := managedruntime.Status{Service: managedruntime.SingBox, State: managedruntime.Running, Generation: 7, Desired: true, Restored: true}
	w := httptest.NewRecorder()
	s.runtimeResult(w, managedruntime.SingBox, "config_committed", state, managedruntime.ErrReadiness)
	if w.Code != 503 {
		t.Fatalf("HTTP %d", w.Code)
	}
	var body struct {
		Error  apiError
		Status managedruntime.Status
	}
	if err := json.Unmarshal(w.Body.Bytes(), &body); err != nil {
		t.Fatal(err)
	}
	if !body.Status.Restored || !body.Status.Desired || body.Status.Generation != 7 || body.Status.State != managedruntime.Running {
		t.Fatal("recovered state discarded")
	}
	raw, err := os.ReadFile(filepath.Join(s.dataDir, "desired-services.json"))
	if err != nil {
		t.Fatal(err)
	}
	var desired map[string]bool
	if json.Unmarshal(raw, &desired) != nil || !desired[managedruntime.SingBox] {
		t.Fatal("recovered desired state not persisted")
	}
}

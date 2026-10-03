package httpapi

import (
	"be6500panel/internal/nodeprobe"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestNodeProbeRoutesRequireSessionAndSameOrigin(t *testing.T) {
	_, ts := testServer(t, "correct")
	for _, method := range []string{"GET", "POST", "DELETE"} {
		response := request(t, ts, method, NodeProbesPath, `{}`, nil)
		if response.StatusCode != http.StatusUnauthorized {
			t.Fatal(method, response.Status)
		}
		if method != "GET" {
			req, _ := http.NewRequest(method, ts.URL+NodeProbesPath, strings.NewReader(`{}`))
			req.Header.Set("Content-Type", "application/json")
			req.Header.Set("Origin", "https://evil.invalid")
			res, err := ts.Client().Do(req)
			if err != nil {
				t.Fatal(err)
			}
			res.Body.Close()
			if res.StatusCode != http.StatusForbidden {
				t.Fatal(method, res.Status)
			}
		}
	}
}
func TestNodeProbeGETNoArtifactIsPureAndNodesRevisionSurvivesDecode(t *testing.T) {
	srv, ts := policyServer(t)
	prior := srv.proxyState.revision
	if prior == "" {
		t.Fatal("missing node revision")
	}
	for i := 0; i < 3; i++ {
		res := request(t, ts, "GET", NodeProbesPath, "", nil)
		var view nodeprobe.Snapshot
		if res.StatusCode != 200 || json.NewDecoder(res.Body).Decode(&view) != nil || view.Available || view.UnavailableCode != "artifact_unavailable" || view.Running || len(view.Results) != 0 || view.Revision != prior {
			t.Fatal(res.Status, view)
		}
	}
	nodes := decode(t, request(t, ts, "GET", "/api/proxy/nodes", "", nil))
	if nodes["revision"] != prior {
		t.Fatal(nodes["revision"])
	}
	sub := srv.proxyState.subscription
	payload := []byte(`{"content":`)
	content, _ := json.Marshal(policySubscription)
	payload = append(payload, content...)
	payload = append(payload, '}')
	res := request(t, ts, "POST", "/api/proxy/import", string(payload), nil)
	fresh := decode(t, res)
	if res.StatusCode != 200 || fresh["revision"] == prior || fresh["revision"] == "" {
		t.Fatal(res.Status, fresh["revision"])
	}
	if len(srv.proxyState.subscription.Nodes) != len(sub.Nodes) {
		t.Fatal("import changed test node count")
	}
	state, err := srv.runtime.Status("sing-box")
	if err != nil || state.Configured || state.ArtifactAvailable || state.Generation != 0 || state.Desired || state.PID != 0 {
		t.Fatal(state, err)
	}
}
func TestNodeProbeStartSerializesSubscriptionAdmission(t *testing.T) {
	srv, _ := policyServer(t)
	srv.proxyState.mutationMu.Lock()
	defer srv.proxyState.mutationMu.Unlock()
	request := httptest.NewRequest("POST", NodeProbesPath, strings.NewReader(`{"all":true,"nodeIds":[],"revision":"`+srv.proxyState.revision+`"}`))
	request.Header.Set("Content-Type", "application/json")
	response := httptest.NewRecorder()
	srv.ServeHTTP(response, request)
	if response.Code != 409 || !strings.Contains(response.Body.String(), "proxy_mutation_pending") {
		t.Fatal(response.Code, response.Body.String())
	}
	if srv.nodeProbes.Snapshot().Running {
		t.Fatal("busy import started probe job")
	}
}

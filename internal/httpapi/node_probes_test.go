package httpapi

import (
	"be6500panel/internal/nodeprobe"
	"be6500panel/internal/proxy"
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
)

func TestNodeProbeHandlerGETNeverAcquiresAndStrictPOST(t *testing.T) {
	var acquired atomic.Int32
	manager, err := nodeprobe.New(nodeprobe.Config{Nodes: func() (nodeprobe.NodeSet, error) {
		return nodeprobe.NodeSet{Revision: "rev", Nodes: []proxy.Node{{ID: "n"}}}, nil
	}, AcquireLease: func(context.Context) (nodeprobe.CoreLease, error) {
		acquired.Add(1)
		return nodeprobe.CoreLease{}, nodeprobe.ErrUnavailable
	}})
	if err != nil {
		t.Fatal(err)
	}
	defer manager.Close()
	for i := 0; i < 3; i++ {
		w := httptest.NewRecorder()
		HandleNodeProbes(w, httptest.NewRequest("GET", NodeProbesPath, nil), manager)
		if w.Code != 200 {
			t.Fatal(w.Code)
		}
	}
	if acquired.Load() != 0 {
		t.Fatal("GET acquired artifact")
	}
	cases := []struct {
		body string
		code int
	}{{`{"all":true,"nodeIds":[],"revision":"rev","url":"https://private"}`, 400}, {`{"all":true,"nodeIds":[],"revision":"rev","corePath":"/private"}`, 400}, {`{"all":true,"all":false,"nodeIds":[],"revision":"rev"}`, 400}, {`{"all":true,"nodeIds":[],"Revision":"rev"}`, 400}, {`{"all":true,"nodeIds":[],"revision":"old"}`, 409}, {`{"all":false,"nodeIds":[],"revision":"rev"}`, 400}, {`{"all":true,"nodeIds":["n"],"revision":"rev"}`, 400}, {`{"all":true,"nodeIds":[],"revision":"rev"}`, 503}}
	for _, item := range cases {
		w := httptest.NewRecorder()
		r := httptest.NewRequest("POST", NodeProbesPath, strings.NewReader(item.body))
		r.Header.Set("Content-Type", "application/json")
		HandleNodeProbes(w, r, manager)
		if w.Code != item.code {
			t.Fatalf("%s: %d %s", item.body, w.Code, w.Body.String())
		}
		if strings.Contains(w.Body.String(), "https://private") || strings.Contains(w.Body.String(), "/private") {
			t.Fatal("private input exposed")
		}
	}
	if acquired.Load() != 1 {
		t.Fatal("invalid admission acquired artifact")
	}
	w := httptest.NewRecorder()
	HandleNodeProbes(w, httptest.NewRequest("DELETE", NodeProbesPath, nil), manager)
	if w.Code != 200 {
		t.Fatal(w.Code)
	}
	var view nodeprobe.Snapshot
	if json.Unmarshal(w.Body.Bytes(), &view) != nil || view.Running {
		t.Fatal("cancel snapshot invalid")
	}
}
func TestNodeProbeHandlerUnavailableAndMethodBodyLimits(t *testing.T) {
	w := httptest.NewRecorder()
	HandleNodeProbes(w, httptest.NewRequest("GET", NodeProbesPath, nil), nil)
	if w.Code != http.StatusServiceUnavailable {
		t.Fatal(w.Code)
	}
	manager, _ := nodeprobe.New(nodeprobe.Config{Nodes: func() (nodeprobe.NodeSet, error) { return nodeprobe.NodeSet{Revision: "r"}, nil }})
	defer manager.Close()
	w = httptest.NewRecorder()
	HandleNodeProbes(w, httptest.NewRequest("PATCH", NodeProbesPath, nil), manager)
	if w.Code != 405 || w.Header().Get("Allow") != "GET, POST, DELETE" {
		t.Fatal("method boundary")
	}
	w = httptest.NewRecorder()
	r := httptest.NewRequest("POST", NodeProbesPath, strings.NewReader(strings.Repeat("x", 41*1024)))
	r.Header.Set("Content-Type", "application/json")
	HandleNodeProbes(w, r, manager)
	if w.Code != 413 {
		t.Fatal("body unbounded", w.Code)
	}
}

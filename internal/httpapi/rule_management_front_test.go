package httpapi

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"be6500panel/internal/localrules"
	"be6500panel/internal/proxy"
	managedruntime "be6500panel/internal/runtime"
	"be6500panel/internal/storage"
)

// This fixture is HTTP only. There is no runtime manager, verifier, subprocess,
// external DNS, or router. Accepted configurations are literal offline bytes.
type ruleFrontOld struct {
	mu                                        sync.Mutex
	dir                                       string
	sub                                       proxy.Subscription
	accepted                                  []byte
	state                                     managedruntime.Status
	revision, selected                        string
	calls                                     []string
	cookies                                   []string
	configured                                [][]byte
	generations                               []uint64
	failCode                                  string
	failHTTP                                  int
	failRestored, failRecovery, wrongReadback bool
	blockReads                                bool
}

func (o *ruleFrontOld) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	o.mu.Lock()
	defer o.mu.Unlock()
	o.calls = append(o.calls, r.Method+" "+r.URL.Path)
	o.cookies = append(o.cookies, r.Header.Get("Cookie"))
	authenticated := r.Header.Get("Cookie") == "be6500panel_session=one" || r.Header.Get("Cookie") == "be6500panel_session=two"
	if r.URL.Path == "/api/session/login" {
		if !sameOrigin(r) {
			fail(w, 403, "origin_rejected", "wrong forwarded origin")
			return
		}
		http.SetCookie(w, &http.Cookie{Name: sessionCookie, Value: "one", Path: "/", HttpOnly: true, SameSite: http.SameSiteStrictMode})
		writeJSON(w, 200, map[string]bool{"authenticated": true})
		return
	}
	if r.URL.Path == "/api/session/logout" {
		http.SetCookie(w, &http.Cookie{Name: sessionCookie, Value: "", Path: "/", MaxAge: -1, HttpOnly: true})
		writeJSON(w, 200, map[string]bool{"authenticated": false})
		return
	}
	if r.URL.Path == "/api/session" {
		writeJSON(w, 200, map[string]bool{"authenticated": authenticated, "authRequired": true})
		return
	}
	if !authenticated {
		fail(w, 401, "unauthenticated", "private fixture token")
		return
	}
	if !safeMethod(r.Method) && !sameOrigin(r) {
		fail(w, 403, "origin_rejected", "wrong forwarded origin")
		return
	}
	switch r.URL.Path {
	case "/api/runtime":
		if o.blockReads {
			<-r.Context().Done()
			return
		}
		writeJSON(w, 200, map[string]any{"enabled": true, "services": []managedruntime.Status{o.state}})
	case "/api/runtime/config":
		writeJSON(w, 200, map[string]any{"service": managedruntime.SingBox, "config": string(o.accepted), "generation": o.state.Generation})
	case "/api/runtime/configure":
		var input struct {
			Service    string `json:"service"`
			Config     string `json:"config"`
			Generation uint64 `json:"generation"`
		}
		if !decodeJSON(w, r, &input, "service", "config", "generation") {
			return
		}
		o.configured = append(o.configured, []byte(input.Config))
		o.generations = append(o.generations, input.Generation)
		if input.Service != managedruntime.SingBox || input.Generation != o.state.Generation {
			fail(w, 409, "generation_conflict", "private original body")
			return
		}
		if o.failCode != "" {
			state := o.state
			state.Restored, state.NeedsRecovery = o.failRestored, o.failRecovery
			writeJSON(w, o.failHTTP, struct {
				Error  apiError              `json:"error"`
				Status managedruntime.Status `json:"status"`
			}{apiError{o.failCode, "PRIVATE verifier output and configuration"}, state})
			return
		}
		o.accepted = []byte(input.Config)
		o.state.Generation++
		if o.wrongReadback {
			o.accepted = append(o.accepted, '\n')
		}
		writeJSON(w, 200, o.state)
	case "/api/proxy/nodes":
		// The old selected cache is deliberately stale after a front selection.
		writeJSON(w, 200, map[string]any{"nodes": o.sub.PublicNodes(), "selectedNodeId": o.selected, "revision": o.revision})
	case "/api/proxy/import":
		var input struct {
			Content string `json:"content"`
		}
		if !decodeJSONLimit(w, r, &input, 3<<20) {
			return
		}
		sub, err := proxy.ParseClashYAML(strings.NewReader(input.Content))
		if err != nil {
			fail(w, 422, "subscription_invalid", "private subscription")
			return
		}
		if err := os.WriteFile(filepath.Join(o.dir, "subscription.yaml"), []byte(input.Content), 0600); err != nil {
			fail(w, 500, "storage_failed", "private filesystem")
			return
		}
		o.sub, o.selected, o.revision = sub, "", "old-probe-revision-2"
		// Keep the selection FILE to reproduce the original panel's behavior.
		writeJSON(w, 200, map[string]string{"revision": o.revision})
	case "/api/events":
		w.Header().Set("Content-Type", "text/event-stream")
		_, _ = io.WriteString(w, "data: ready\n\n")
		w.(http.Flusher).Flush()
		<-r.Context().Done()
	case "/api/echo":
		writeJSON(w, 200, map[string]string{"host": r.Host, "origin": r.Header.Get("Origin"), "cookie": r.Header.Get("Cookie")})
	default:
		fail(w, 404, "not_found", "No fake core action exists")
	}
}
func ruleFrontFixture(t *testing.T) (*RuleManagementFront, *httptest.Server, *ruleFrontOld) {
	t.Helper()
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "subscription.yaml"), []byte(policySubscription), 0600); err != nil {
		t.Fatal(err)
	}
	stagePolicyRuleSets(t, dir)
	sub := parsedPolicy(t, policySubscription)
	s := &Server{dataDir: dir}
	refs, err := s.ruleSetReferences()
	if err != nil {
		t.Fatal(err)
	}
	out, err := proxy.CompileNative(proxy.CompileInput{Node: sub.Nodes[0], Rules: []proxy.Rule{{Kind: proxy.RuleMatch, Target: proxy.TargetProxy}}, RuleSets: refs,
		Endpoints: []string{"192.0.2.8"}, ManagementIPs: []string{"192.168.31.1"}, IPv6: proxy.IPv6Direct, Failure: proxy.FailureDirect,
		Ports: proxy.Ports{Mixed: 2080, TProxy: 7893, DNS: 1053}, MixedListenAddress: "192.168.31.1", DNSListenAddress: "192.168.31.1"})
	if err != nil {
		t.Fatal(err)
	}
	saved, _ := json.Marshal(map[string]string{"nodeId": sub.Nodes[0].ID})
	if err := os.WriteFile(filepath.Join(dir, "proxy-selection.json"), saved, 0600); err != nil {
		t.Fatal(err)
	}
	old := &ruleFrontOld{dir: dir, sub: sub, accepted: out.Config, revision: "old-probe-revision-1", selected: sub.Nodes[0].ID,
		state: managedruntime.Status{Service: managedruntime.SingBox, State: managedruntime.Running, Configured: true, ArtifactAvailable: true, Generation: 9, PID: 7858, Desired: true}}
	upstream := httptest.NewServer(old)
	t.Cleanup(upstream.Close)
	u, _ := url.Parse(upstream.URL)
	budget, err := storage.New(storage.Options{})
	if err != nil {
		t.Fatal(err)
	}
	front, err := newRuleManagementFront(RuleManagementFrontConfig{DataDir: dir, Logger: slog.New(slog.NewTextHandler(io.Discard, nil)), StorageAdmission: budget.Admit}, u)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(front.Close)
	ts := httptest.NewServer(front)
	t.Cleanup(ts.Close)
	return front, ts, old
}
func ruleFrontRequest(t *testing.T, ts *httptest.Server, method, path, body, cookie string) *http.Response {
	t.Helper()
	req, err := http.NewRequest(method, ts.URL+path, strings.NewReader(body))
	if err != nil {
		t.Fatal(err)
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Cookie", cookie)
	if !safeMethod(method) {
		req.Header.Set("Origin", ts.URL)
	}
	res, err := ts.Client().Do(req)
	if err != nil {
		t.Fatal(err)
	}
	return res
}
func ruleFrontRead(t *testing.T, ts *httptest.Server) localRulesReadback {
	t.Helper()
	res := ruleFrontRequest(t, ts, "GET", LocalRulesPath, "", "be6500panel_session=one")
	defer res.Body.Close()
	var body localRulesReadback
	if res.StatusCode != 200 || json.NewDecoder(res.Body).Decode(&body) != nil {
		t.Fatal(res.Status)
	}
	return body
}
func ruleFrontSave(t *testing.T, ts *httptest.Server, policy proxy.Policy) localRulesReadback {
	t.Helper()
	res := ruleFrontRequest(t, ts, "POST", LocalRulesPath, policyBody(t, policy), "be6500panel_session=one")
	defer res.Body.Close()
	var body localRulesReadback
	if res.StatusCode != 200 || json.NewDecoder(res.Body).Decode(&body) != nil {
		t.Fatal(res.Status)
	}
	return body
}
func TestRuleManagementFrontConstructorReadSavePreviewNeverConfigure(t *testing.T) {
	front, ts, old := ruleFrontFixture(t)
	old.mu.Lock()
	calls := len(old.calls)
	old.mu.Unlock()
	if calls != 0 || front.server.runtime != nil || front.server.auth != nil || front.server.control != nil || front.server.nodeProbes != nil || front.server.sampler != nil || front.server.capture != nil {
		t.Fatal("constructor created a core owner or contacted upstream")
	}
	initial := ruleFrontRead(t, ts)
	if initial.Applied.State != "unknown" || initial.RuntimeGeneration != 9 {
		t.Fatal(initial)
	}
	var wg sync.WaitGroup
	for i := 0; i < 16; i++ {
		wg.Add(1)
		go func() { defer wg.Done(); ruleFrontRead(t, ts) }()
	}
	wg.Wait()
	preview := ruleFrontRequest(t, ts, "POST", LocalRulesPreviewPath, policyBody(t, gptLocalPolicy()), "be6500panel_session=two")
	var effective proxy.EffectivePolicy
	if preview.StatusCode != 200 || json.NewDecoder(preview.Body).Decode(&effective) != nil {
		t.Fatal(preview.Status)
	}
	preview.Body.Close()
	if effective.Rules[0].Value != "gpt.kanglives.top" || len(front.server.localRules.Snapshot().Policy.Rules) != 0 {
		t.Fatal("preview saved or did not merge")
	}
	saved := ruleFrontSave(t, ts, gptLocalPolicy())
	if saved.Applied.State != "unknown" || saved.Draft.Revision == initial.Draft.Revision || ruleFrontRead(t, ts).Draft.Revision != saved.Draft.Revision {
		t.Fatal(saved)
	}
	info, err := os.Stat(filepath.Join(front.server.dataDir, localrules.FileName))
	if err != nil || info.Mode().Perm() != 0600 {
		t.Fatal("draft privacy", err)
	}
	old.mu.Lock()
	defer old.mu.Unlock()
	if len(old.configured) != 0 || old.state.Generation != 9 || old.state.PID != 7858 {
		t.Fatal("editor changed core")
	}
	for _, call := range old.calls {
		if strings.HasPrefix(call, "POST ") {
			t.Fatal("read/save/preview sent upstream mutation", call)
		}
	}
}
func TestRuleManagementFrontDelegatesAuthOriginCookiesAndNormalAPI(t *testing.T) {
	front, ts, old := ruleFrontFixture(t)
	for _, path := range []string{LocalRulesPath, LocalRulesPreviewPath, LocalRulesApplyPath, "/api/proxy/select", "/api/proxy/import", "/api/proxy/nodes"} {
		method := routes[path]
		assertLocalError(t, ruleFrontRequest(t, ts, method, path, `{}`, ""), 401, "unauthenticated")
	}
	login := ruleFrontRequest(t, ts, "POST", "/api/session/login", `{"password":"fixture"}`, "")
	if login.StatusCode != 200 || len(login.Cookies()) != 1 || login.Cookies()[0].Path != "/" || !login.Cookies()[0].HttpOnly {
		t.Fatal("session cookie not preserved")
	}
	login.Body.Close()
	for _, cookie := range []string{"be6500panel_session=one", "be6500panel_session=two"} {
		res := ruleFrontRequest(t, ts, "POST", "/api/echo", `{}`, cookie)
		body := decode(t, res)
		if body["host"] != front.upstream.Host || body["origin"] != front.upstream.String() || body["cookie"] != cookie {
			t.Fatal(body)
		}
	}
	logout := ruleFrontRequest(t, ts, "POST", "/api/session/logout", `{}`, "be6500panel_session=one")
	if len(logout.Cookies()) != 1 || logout.Cookies()[0].MaxAge != -1 {
		t.Fatal("logout cookie not preserved")
	}
	logout.Body.Close()
	saved := ruleFrontSave(t, ts, gptLocalPolicy())
	before, _ := os.ReadFile(filepath.Join(front.server.dataDir, localrules.FileName))
	old.mu.Lock()
	count := len(old.calls)
	old.mu.Unlock()
	for _, path := range []string{LocalRulesPath, LocalRulesPreviewPath, LocalRulesApplyPath, "/api/proxy/select", "/api/proxy/import", "/api/runtime/configure", "/api/session/login"} {
		for _, origin := range []string{"http://foreign.test", front.upstream.String(), "null"} {
			req := httptest.NewRequest("POST", "http://front.test:8788"+path, strings.NewReader(policyBody(t, gptLocalPolicy())))
			req.Header.Set("Cookie", "be6500panel_session=one")
			req.Header.Set("Origin", origin)
			rec := httptest.NewRecorder()
			front.ServeHTTP(rec, req)
			if rec.Code != 403 {
				t.Fatal(path, origin, rec.Code, rec.Body.String())
			}
		}
	}
	req := httptest.NewRequest("POST", "http://front.test:8788/api/echo", strings.NewReader(`{}`))
	req.Header.Set("Sec-Fetch-Site", "cross-site")
	rec := httptest.NewRecorder()
	front.ServeHTTP(rec, req)
	if rec.Code != 403 {
		t.Fatal(rec.Code)
	}
	after, _ := os.ReadFile(filepath.Join(front.server.dataDir, localrules.FileName))
	old.mu.Lock()
	defer old.mu.Unlock()
	if len(old.calls) != count || !bytes.Equal(before, after) || front.server.localRules.Snapshot().Revision != saved.Draft.Revision {
		t.Fatal("unsafe origin touched upstream or draft")
	}
}
func TestRuleManagementFrontApplyDelegatesExactConfigAndAttestsReadback(t *testing.T) {
	front, ts, old := ruleFrontFixture(t)
	saved := ruleFrontSave(t, ts, gptLocalPolicy())
	res := ruleFrontRequest(t, ts, "POST", LocalRulesApplyPath, applyLocalBody(t, saved.Draft.Revision, 9, saved.PolicySummary.Revision), "be6500panel_session=one")
	body := decode(t, res)
	if res.StatusCode != 200 || body["applied"] != true {
		t.Fatal(res.StatusCode, body)
	}
	state := ruleFrontRead(t, ts)
	if state.Applied.State != "known" || state.Applied.Generation != 10 || state.Applied.Revision != saved.Draft.Revision || state.RuntimeGeneration != 10 {
		t.Fatal(state)
	}
	old.mu.Lock()
	defer old.mu.Unlock()
	if len(old.configured) != 1 || old.generations[0] != 9 || !bytes.Equal(old.configured[0], old.accepted) || nativeConfigHash(old.accepted) != body["configSHA256"] || !strings.Contains(string(old.accepted), "gpt.kanglives.top") {
		t.Fatal("wrong delegated bytes or generation")
	}
	raw, _ := json.Marshal(body)
	for _, private := range []string{"11111111-1111", "AAAAAAAA", "outbounds", "PRIVATE"} {
		if strings.Contains(string(raw), private) {
			t.Fatal("apply leaked private config", private)
		}
	}
	manifest, _ := os.ReadFile(filepath.Join(front.server.dataDir, localRulesAppliedFile))
	var actual localRulesManifest
	if json.Unmarshal(manifest, &actual) != nil || actual.NativeSHA256 != nativeConfigHash(old.accepted) || actual.Generation != 10 {
		t.Fatal("false manifest")
	}
}
func TestRuleManagementFrontFailedApplyPreservesUpstreamStatusAndNeverRetries(t *testing.T) {
	for _, tc := range []struct {
		code               string
		status             int
		restored, recovery bool
	}{{"config_check_failed", 422, false, false}, {"readiness_failed", 503, true, false}, {"runtime_operation_failed", 500, false, true}, {"configuration_pending", 409, false, false}} {
		t.Run(tc.code, func(t *testing.T) {
			front, ts, old := ruleFrontFixture(t)
			saved := ruleFrontSave(t, ts, gptLocalPolicy())
			old.mu.Lock()
			old.failCode, old.failHTTP, old.failRestored, old.failRecovery = tc.code, tc.status, tc.restored, tc.recovery
			old.mu.Unlock()
			res := ruleFrontRequest(t, ts, "POST", LocalRulesApplyPath, applyLocalBody(t, saved.Draft.Revision, 9, saved.PolicySummary.Revision), "be6500panel_session=one")
			body := decode(t, res)
			status := body["status"].(map[string]any)
			if res.StatusCode != tc.status || body["error"].(map[string]any)["code"] != tc.code || status["restored"] != tc.restored || status["needsRecovery"] != tc.recovery || status["generation"] != float64(9) || body["applied"] != nil {
				t.Fatal(res.StatusCode, body)
			}
			raw, _ := json.Marshal(body)
			if strings.Contains(string(raw), "PRIVATE") {
				t.Fatal("raw upstream error leaked")
			}
			if ruleFrontRead(t, ts).Applied.State != "unknown" {
				t.Fatal("failure attested applied")
			}
			if _, err := os.Stat(filepath.Join(front.server.dataDir, localRulesAppliedFile)); !errors.Is(err, os.ErrNotExist) {
				t.Fatal("failed configure wrote manifest", err)
			}
			old.mu.Lock()
			defer old.mu.Unlock()
			if len(old.configured) != 1 {
				t.Fatal("configure retried")
			}
		})
	}
}
func TestRuleManagementFrontAcceptedReadbackMismatchStaysUnknown(t *testing.T) {
	front, ts, old := ruleFrontFixture(t)
	saved := ruleFrontSave(t, ts, gptLocalPolicy())
	old.mu.Lock()
	old.wrongReadback = true
	old.mu.Unlock()
	assertLocalError(t, ruleFrontRequest(t, ts, "POST", LocalRulesApplyPath, applyLocalBody(t, saved.Draft.Revision, 9, saved.PolicySummary.Revision), "be6500panel_session=one"), 500, "local_rules_applied_unknown")
	if ruleFrontRead(t, ts).Applied.State != "unknown" {
		t.Fatal("mismatched config attested applied")
	}
	if _, err := os.Stat(filepath.Join(front.server.dataDir, localRulesAppliedFile)); !errors.Is(err, os.ErrNotExist) {
		t.Fatal("mismatch wrote manifest")
	}
}
func TestRuleManagementFrontImportRefreshesSameStateKeepsDraftAndProbeRevision(t *testing.T) {
	front, ts, old := ruleFrontFixture(t)
	initial := ruleFrontRead(t, ts)
	policy := gptLocalPolicy()
	policy.SubscriptionEdits = append(policy.SubscriptionEdits, proxy.SubscriptionEdit{ID: "disable-old", SourceFingerprint: initial.SubscriptionRules[0].Fingerprint, Disabled: true})
	saved := ruleFrontSave(t, ts, policy)
	p := front.server.proxyState
	changed := strings.Replace(policySubscription, "DOMAIN,example.test,DIRECT", "DOMAIN,changed.test,DIRECT", 1)
	raw, _ := json.Marshal(map[string]string{"content": changed})
	imported := ruleFrontRequest(t, ts, "POST", "/api/proxy/import", string(raw), "be6500panel_session=one")
	body := decode(t, imported)
	if imported.StatusCode != 200 || body["selectedNodeId"] != "" || body["revision"] != "old-probe-revision-2" || front.server.proxyState != p {
		t.Fatal("import retained old selection or changed owner", body)
	}
	read := ruleFrontRead(t, ts)
	if read.Draft.Revision != saved.Draft.Revision || read.SubscriptionRevision == saved.SubscriptionRevision {
		t.Fatal("import lost draft or kept subscription")
	}
	orphan := false
	for _, d := range read.Preview.Diagnostics {
		if d.Code == "orphaned-edit" {
			orphan = true
		}
	}
	if !orphan {
		t.Fatal("orphan hidden")
	}
	persisted, _ := os.ReadFile(filepath.Join(front.server.dataDir, "proxy-selection.json"))
	if !strings.Contains(string(persisted), initialNodeID(front)) {
		t.Fatal("fixture did not retain stale selection file")
	}
	nodes := decode(t, ruleFrontRequest(t, ts, "GET", "/api/proxy/nodes", "", "be6500panel_session=one"))
	if nodes["selectedNodeId"] != "" || nodes["revision"] != "old-probe-revision-2" {
		t.Fatal(nodes)
	}
	old.mu.Lock()
	defer old.mu.Unlock()
	if len(old.configured) != 0 {
		t.Fatal("import configured runtime")
	}
}
func initialNodeID(front *RuleManagementFront) string {
	front.server.proxyState.mu.Lock()
	defer front.server.proxyState.mu.Unlock()
	return front.server.proxyState.subscription.Nodes[0].ID
}
func TestRuleManagementFrontSelectMergesLocalPolicyAndUsesFrontSelectedTruth(t *testing.T) {
	front, ts, old := ruleFrontFixture(t)
	saved := ruleFrontSave(t, ts, gptLocalPolicy())
	nodeID := initialNodeID(front)
	old.mu.Lock()
	old.selected = "stale-old-cache"
	old.mu.Unlock()
	selected := ruleFrontRequest(t, ts, "POST", "/api/proxy/select", selectPolicyBody(nodeID, saved.PolicySummary.Revision), "be6500panel_session=two")
	body := decode(t, selected)
	if selected.StatusCode != 200 {
		t.Fatal(selected.StatusCode, body)
	}
	nodes := decode(t, ruleFrontRequest(t, ts, "GET", "/api/proxy/nodes", "", "be6500panel_session=one"))
	if nodes["selectedNodeId"] != nodeID || nodes["revision"] != "old-probe-revision-1" {
		t.Fatal("front returned stale old selected/revision", nodes)
	}
	state := ruleFrontRead(t, ts)
	if state.Applied.State != "known" || state.Applied.Revision != saved.Draft.Revision || state.Applied.Generation != 10 {
		t.Fatal(state)
	}
	old.mu.Lock()
	defer old.mu.Unlock()
	if len(old.configured) != 1 || !strings.Contains(string(old.configured[0]), "gpt.kanglives.top") || old.generations[0] != 9 || old.selected != "stale-old-cache" {
		t.Fatal("select did not merge or rewrote old cache")
	}
}
func TestRuleManagementFrontCancelledTimeoutSafeErrorsAndStreaming(t *testing.T) {
	front, ts, old := ruleFrontFixture(t)
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	req := httptest.NewRequest("GET", LocalRulesPath, nil).WithContext(ctx)
	req.Header.Set("Cookie", "be6500panel_session=one")
	s := front.requestServer(req)
	_, _, err := s.ruleConfig(managedruntime.SingBox)
	rec := httptest.NewRecorder()
	s.ruleRuntimeError(rec, err)
	if rec.Code != 409 || !strings.Contains(rec.Body.String(), "operation_cancelled") {
		t.Fatal(rec.Code, rec.Body.String())
	}
	old.mu.Lock()
	old.blockReads = true
	old.mu.Unlock()
	front.reads.Timeout = 20 * time.Millisecond
	_, err = s.ruleStatus(managedruntime.SingBox) // canceled request remains canceled
	if !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
	live := httptest.NewRequest("GET", LocalRulesPath, nil)
	live.Header.Set("Cookie", "be6500panel_session=one")
	_, err = front.requestServer(live).ruleStatus(managedruntime.SingBox)
	rec = httptest.NewRecorder()
	s.ruleRuntimeError(rec, err)
	if rec.Code != 504 || !strings.Contains(rec.Body.String(), "operation_timeout") || strings.Contains(rec.Body.String(), front.upstream.String()) {
		t.Fatal(rec.Code, rec.Body.String())
	}
	old.mu.Lock()
	old.blockReads = false
	old.mu.Unlock()
	streamCtx, stop := context.WithCancel(context.Background())
	defer stop()
	stream, _ := http.NewRequestWithContext(streamCtx, "GET", ts.URL+"/api/events", nil)
	stream.Header.Set("Cookie", "be6500panel_session=one")
	res, err := ts.Client().Do(stream)
	if err != nil {
		t.Fatal(err)
	}
	defer res.Body.Close()
	line, err := bufio.NewReader(res.Body).ReadString('\n')
	if err != nil || line != "data: ready\n" || res.Header.Get("Content-Type") != "text/event-stream" {
		t.Fatal("SSE buffered or changed", line, err)
	}
	stop()
}

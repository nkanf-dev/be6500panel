package httpapi

import (
	"context"
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"be6500panel/internal/proxy"
	managedruntime "be6500panel/internal/runtime"
)

const policySubscription = `proxies:
  - name: test-node
    type: vless
    server: 192.0.2.8
    port: 443
    uuid: 11111111-1111-4111-8111-111111111111
    tls: true
    servername: example.test
    flow: xtls-rprx-vision
    client-fingerprint: chrome
    udp: true
    reality-opts:
      public-key: AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA
      short-id: "abcd"
rules:
  - DOMAIN,example.test,DIRECT
  - PROCESS-NAME,private-app,PROXY
  - GEOIP,US,DIRECT
  - MATCH,PROXY
  - DOMAIN-SUFFIX,late.test,DIRECT
`

func parsedPolicy(t *testing.T, content string) proxy.Subscription {
	t.Helper()
	sub, err := proxy.ParseClashYAML(strings.NewReader(content))
	if err != nil {
		t.Fatal(err)
	}
	return sub
}

func TestProxyPolicySummaryCountsEachOmittedRuleOnce(t *testing.T) {
	sub := parsedPolicy(t, policySubscription)
	// Diagnostic multiplicity must not inflate the number of omitted rules.
	sub.Diagnostics = append(sub.Diagnostics, sub.Diagnostics[0])
	sub.Diagnostics = append(sub.Diagnostics, proxy.Diagnostic{Scope: "node", Index: 99, Code: "unsupported-node", Message: "private"})
	summary := summarizeProxyPolicy(sub)
	if summary.Total != 5 || summary.Supported != 2 || summary.Omitted != 3 {
		t.Fatalf("inaccurate rule counts: %+v", summary)
	}
	want := []struct {
		index int
		code  string
	}{{1, "unsupported-process-rule"}, {2, "unsupported-rule"}, {4, "unreachable-rule"}}
	for i, omitted := range summary.OmittedRules {
		if omitted.Index != want[i].index || omitted.Code != want[i].code {
			t.Fatalf("wrong omission %+v", omitted)
		}
	}
	raw, _ := json.Marshal(summary)
	if strings.Contains(string(raw), "private") || strings.Contains(string(raw), "11111111") || strings.Contains(string(raw), "AAAAAAAA") {
		t.Fatal("policy response leaked original input")
	}
	if len(summary.Revision) != 64 {
		t.Fatal("missing stable policy revision")
	}
}

func TestProxyPolicyRevisionDependsOnRulesNotNodeSecrets(t *testing.T) {
	base := parsedPolicy(t, policySubscription)
	original := summarizeProxyPolicy(base).Revision
	for _, content := range []string{
		strings.Replace(policySubscription, "192.0.2.8", "192.0.2.9", 1),
		strings.Replace(policySubscription, "11111111-1111-4111-8111-111111111111", "22222222-2222-4222-8222-222222222222", 1),
		strings.Replace(policySubscription, "private-app", "another-private-app", 1),
	} {
		if summarizeProxyPolicy(parsedPolicy(t, content)).Revision != original {
			t.Fatal("same reviewed policy changed for non-policy input")
		}
	}
	for _, content := range []string{
		strings.Replace(policySubscription, "DOMAIN,example.test,DIRECT", "DOMAIN,other.test,DIRECT", 1),
		strings.Replace(policySubscription, "PROCESS-NAME,private-app,PROXY", "DOMAIN,app.test,PROXY", 1),
	} {
		if summarizeProxyPolicy(parsedPolicy(t, content)).Revision == original {
			t.Fatal("policy change did not invalidate review")
		}
	}
	base.FakeIP = true
	if summarizeProxyPolicy(base).Revision == original {
		t.Fatal("DNS policy change did not invalidate review")
	}
}

func policyServer(t *testing.T) (*Server, *httptest.Server) {
	t.Helper()
	s, ts := testServer(t, "")
	dir := t.TempDir()
	s.dataDir = filepath.Join(dir, "panel")
	if err := os.MkdirAll(s.dataDir, 0700); err != nil {
		t.Fatal(err)
	}
	manager, err := managedruntime.New(managedruntime.Options{DataDir: filepath.Join(dir, "runtime-data"), RunDir: filepath.Join(dir, "runtime-run"), LocalSourceRoot: s.dataDir})
	if err != nil {
		t.Fatal(err)
	}
	s.runtime = manager
	t.Cleanup(func() { _ = manager.Close() })
	s.proxyState = newProxyState(s.dataDir)
	s.proxyState.subscription = parsedPolicy(t, policySubscription)
	return s, ts
}

func selectPolicyBody(nodeID, revision string) string {
	input := map[string]any{"nodeId": nodeID, "ipv6": "direct", "failure": "direct", "ports": map[string]int{"mixed": 2080, "tproxy": 7893, "dns": 1053}}
	if revision != "" {
		input["acknowledgedRevision"] = revision
	}
	raw, _ := json.Marshal(input)
	return string(raw)
}

func TestProxyPolicySelectionRequiresActualCurrentAcknowledgment(t *testing.T) {
	s, ts := policyServer(t)
	summary := summarizeProxyPolicy(s.proxyState.subscription)
	nodeID := s.proxyState.subscription.Nodes[0].ID
	for _, test := range []struct{ revision, code string }{
		{"", "policy_acknowledgment_required"},
		{strings.Repeat("a", 64), "policy_revision_changed"},
		{summary.Revision, "rules_unavailable"},
	} {
		res := request(t, ts, http.MethodPost, "/api/proxy/select", selectPolicyBody(nodeID, test.revision), nil)
		if res.StatusCode != 409 {
			t.Fatalf("HTTP %d", res.StatusCode)
		}
		body := decode(t, res)
		if body["error"].(map[string]any)["code"] != test.code {
			t.Fatalf("unexpected result %+v", body)
		}
		// Missing SRS deliberately stops the acknowledged request before any DNS, core or writes.
		state, err := s.runtime.Status(managedruntime.SingBox)
		if err != nil || state.Generation != 0 || state.Configured {
			t.Fatal("review or refused apply wrote runtime state")
		}
		if _, err := os.Stat(filepath.Join(s.dataDir, "proxy-selection.json")); !os.IsNotExist(err) {
			t.Fatal("review wrote selection")
		}
	}
}

func TestProxyPolicyChangedSubscriptionInvalidatesAcknowledgment(t *testing.T) {
	s, ts := policyServer(t)
	previous := summarizeProxyPolicy(s.proxyState.subscription).Revision
	content := strings.Replace(policySubscription, "DOMAIN,example.test,DIRECT", "DOMAIN,changed.test,DIRECT", 1)
	raw, _ := json.Marshal(map[string]string{"content": content})
	imported := request(t, ts, http.MethodPost, "/api/proxy/import", string(raw), nil)
	if imported.StatusCode != 200 {
		t.Fatalf("import HTTP %d", imported.StatusCode)
	}
	var body struct {
		PolicySummary proxyPolicySummary `json:"policySummary"`
	}
	if err := json.NewDecoder(imported.Body).Decode(&body); err != nil {
		t.Fatal(err)
	}
	if body.PolicySummary.Revision == previous || body.PolicySummary.Omitted != 3 {
		t.Fatal("import did not expose current policy")
	}
	res := request(t, ts, http.MethodPost, "/api/proxy/select", selectPolicyBody(s.proxyState.subscription.Nodes[0].ID, previous), nil)
	if res.StatusCode != 409 || decode(t, res)["error"].(map[string]any)["code"] != "policy_revision_changed" {
		t.Fatal("stale acknowledgment accepted")
	}
}

func TestProxyPolicyLegacySupportedSelectionAndStrictDecoder(t *testing.T) {
	s, ts := policyServer(t)
	s.proxyState.subscription = parsedPolicy(t, strings.Split(policySubscription, "rules:")[0]+"rules:\n  - MATCH,PROXY\n")
	nodeID := s.proxyState.subscription.Nodes[0].ID
	res := request(t, ts, http.MethodPost, "/api/proxy/select", selectPolicyBody(nodeID, ""), nil)
	if res.StatusCode != 409 || decode(t, res)["error"].(map[string]any)["code"] != "rules_unavailable" {
		t.Fatal("supported legacy body blocked by acknowledgment")
	}
	valid := selectPolicyBody(nodeID, strings.Repeat("a", 64))
	for _, body := range []string{
		strings.Replace(valid, "acknowledgedRevision", "AcknowledgedRevision", 1),
		strings.Replace(valid, `"acknowledgedRevision":"`+strings.Repeat("a", 64)+`"`, `"acknowledgedRevision":true`, 1),
		strings.Replace(valid, `"acknowledgedRevision":"`+strings.Repeat("a", 64)+`"`, `"acknowledgedRevision":null`, 1),
		strings.Replace(valid, `"acknowledgedRevision":"`+strings.Repeat("a", 64)+`"`, `"acceptUnsupportedRules":true`, 1),
	} {
		res := request(t, ts, http.MethodPost, "/api/proxy/select", body, nil)
		if res.StatusCode != 400 {
			t.Fatalf("loose contract accepted HTTP %d", res.StatusCode)
		}
	}
}

func TestProxyPolicyNodesReadIsPureAndIncludesSummary(t *testing.T) {
	s, ts := policyServer(t)
	for i := 0; i < 2; i++ {
		res := request(t, ts, http.MethodGet, "/api/proxy/nodes", "", nil)
		if res.StatusCode != 200 {
			t.Fatal(res.Status)
		}
		body := decode(t, res)
		summary := body["policySummary"].(map[string]any)
		if summary["total"] != float64(5) || summary["omitted"] != float64(3) {
			t.Fatal(summary)
		}
	}
	if s.proxyState.selected != "" {
		t.Fatal("read selected a node")
	}
	files, err := os.ReadDir(s.dataDir)
	if err != nil || len(files) != 0 {
		t.Fatal("review created private files")
	}
}

func TestProxyPolicyMutationLaneKeepsReadResponsive(t *testing.T) {
	s, ts := policyServer(t)
	sub := s.proxyState.subscription
	revision := summarizeProxyPolicy(sub).Revision
	s.proxyState.mutationMu.Lock()
	defer s.proxyState.mutationMu.Unlock()
	read := request(t, ts, http.MethodGet, "/api/proxy/nodes", "", nil)
	if read.StatusCode != 200 {
		t.Fatal("ongoing mutation blocked review read")
	}
	_ = decode(t, read)
	raw, _ := json.Marshal(map[string]string{"content": policySubscription})
	for _, mutation := range []struct{ path, body string }{
		{"/api/proxy/select", selectPolicyBody(sub.Nodes[0].ID, revision)},
		{"/api/proxy/import", string(raw)},
	} {
		res := request(t, ts, http.MethodPost, mutation.path, mutation.body, nil)
		if res.StatusCode != 409 || decode(t, res)["error"].(map[string]any)["code"] != "proxy_mutation_pending" {
			t.Fatal("concurrent mutation was not rejected")
		}
	}
	if summarizeProxyPolicy(s.proxyState.subscription).Revision != revision {
		t.Fatal("rejected mutation changed review")
	}
	files, err := os.ReadDir(s.dataDir)
	if err != nil || len(files) != 0 {
		t.Fatal("rejected mutation wrote private files")
	}
}

func TestProxyPolicyRevisionStableAfterReload(t *testing.T) {
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "subscription.yaml"), []byte(policySubscription), 0600); err != nil {
		t.Fatal(err)
	}
	first := newProxyState(dir)
	second := newProxyState(dir)
	if first.loadFailed || second.loadFailed {
		t.Fatal("fixture did not load")
	}
	if summarizeProxyPolicy(first.subscription).Revision != summarizeProxyPolicy(second.subscription).Revision {
		t.Fatal("reload invalidated unchanged policy")
	}
}

func stagePolicyRuleSets(t *testing.T, dir string) {
	t.Helper()
	refs := []proxy.RuleSetReference{}
	raw := []byte("offline-controlled-set-fixture")
	for _, item := range []struct{ tag, kind string }{{"cn-domain", "domain"}, {"cn-ip", "ip"}} {
		path := filepath.Join(dir, item.tag+".srs")
		if err := os.WriteFile(path, raw, 0600); err != nil {
			t.Fatal(err)
		}
		refs = append(refs, proxy.RuleSetReference{Tag: item.tag, Kind: item.kind, Path: path, SHA256: fmt.Sprintf("%x", sha256.Sum256(raw)), MaxBytes: 1024})
	}
	encoded, err := json.Marshal(refs)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "rule-sets.json"), encoded, 0600); err != nil {
		t.Fatal(err)
	}
}

func TestProxyPolicyAcknowledgedSelectionStillVerifiesCompiler(t *testing.T) {
	s, ts := policyServer(t)
	stagePolicyRuleSets(t, s.dataDir)
	sub := s.proxyState.subscription
	revision := summarizeProxyPolicy(sub).Revision
	nodeID := sub.Nodes[0].ID // literal endpoint; no DNS traffic in this offline test
	res := request(t, ts, http.MethodPost, "/api/proxy/select", selectPolicyBody(nodeID, ""), nil)
	if res.StatusCode != 409 || decode(t, res)["error"].(map[string]any)["code"] != "policy_acknowledgment_required" {
		t.Fatal("staged rules bypassed review")
	}
	invalid := strings.Replace(selectPolicyBody(nodeID, revision), `"ipv6":"direct"`, `"ipv6":"invalid"`, 1)
	res = request(t, ts, http.MethodPost, "/api/proxy/select", invalid, nil)
	if res.StatusCode != 422 || decode(t, res)["error"].(map[string]any)["code"] != "proxy_configuration_invalid" {
		t.Fatal("acknowledgment bypassed native validation")
	}
	res = request(t, ts, http.MethodPost, "/api/proxy/select", selectPolicyBody(nodeID, revision), nil)
	if res.StatusCode != 409 || decode(t, res)["error"].(map[string]any)["code"] != "artifact_unavailable" {
		t.Fatal("matching review did not reach native compilation")
	}
	state, err := s.runtime.Status(managedruntime.SingBox)
	if err != nil || state.Generation != 0 || state.Configured {
		t.Fatal("missing core was falsely reported as configured")
	}
}

func TestProxyPolicySelectionKeepsAuthenticationAndOriginBoundary(t *testing.T) {
	_, ts := testServer(t, "policy-secret")
	body := selectPolicyBody("node", strings.Repeat("a", 64))
	res := request(t, ts, http.MethodPost, "/api/proxy/select", body, nil)
	if res.StatusCode != 401 || decode(t, res)["error"].(map[string]any)["code"] != "unauthenticated" {
		t.Fatal("policy acknowledgment bypassed authentication")
	}
	req, err := http.NewRequest(http.MethodPost, ts.URL+"/api/proxy/select", strings.NewReader(body))
	if err != nil {
		t.Fatal(err)
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Origin", "https://untrusted.example.test")
	res, err = ts.Client().Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer res.Body.Close()
	if res.StatusCode != 403 || decode(t, res)["error"].(map[string]any)["code"] != "origin_rejected" {
		t.Fatal("policy acknowledgment bypassed origin check")
	}
}

func TestProxyPolicyOnlyExplicitReviewedApplyCommitsSelection(t *testing.T) {
	s, ts := policyServer(t)
	stagePolicyRuleSets(t, s.dataDir)
	// This fixed local verifier does not run a core or create network sockets.
	verifier := []byte("#!/bin/sh\n[ \"$1\" = check ] && [ \"$2\" = -c ] && exit 0\nexit 1\n")
	path := filepath.Join(s.dataDir, "offline-check-fixture")
	if err := os.WriteFile(path, verifier, 0700); err != nil {
		t.Fatal(err)
	}
	_, err := s.runtime.Acquire(context.Background(), managedruntime.SingBox, managedruntime.Artifact{URL: "file://" + path, SHA256: fmt.Sprintf("%x", sha256.Sum256(verifier)), Compression: "none", Version: "offline-test"})
	if err != nil {
		t.Fatal(err)
	}
	sub := s.proxyState.subscription
	revision := summarizeProxyPolicy(sub).Revision
	res := request(t, ts, http.MethodPost, "/api/proxy/select", selectPolicyBody(sub.Nodes[0].ID, ""), nil)
	if res.StatusCode != 409 || decode(t, res)["error"].(map[string]any)["code"] != "policy_acknowledgment_required" {
		t.Fatal("available core bypassed review")
	}
	res = request(t, ts, http.MethodPost, "/api/proxy/select", selectPolicyBody(sub.Nodes[0].ID, revision), nil)
	if res.StatusCode != 200 {
		t.Fatalf("reviewed apply HTTP %d: %s", res.StatusCode, res.Body)
	}
	var body struct {
		Status      managedruntime.Status `json:"status"`
		Diagnostics []proxy.Diagnostic    `json:"diagnostics"`
	}
	if err := json.NewDecoder(res.Body).Decode(&body); err != nil {
		t.Fatal(err)
	}
	if !body.Status.Configured || body.Status.Generation != 1 || body.Status.State != managedruntime.Stopped || body.Status.Desired {
		t.Fatal("offline accepted configuration falsely reported live")
	}
	foundProcess := false
	for _, diagnostic := range body.Diagnostics {
		if diagnostic.Scope == "rule" && diagnostic.Index == 1 && diagnostic.Code == "ignored-subscription-rule" {
			foundProcess = true
		}
	}
	if !foundProcess {
		t.Fatal("PROCESS-NAME omission missing from compiler result")
	}
	accepted, _, err := s.runtime.Config(managedruntime.SingBox)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(accepted), "private-app") || strings.Contains(string(accepted), "process_name") || strings.Contains(string(accepted), "late.test") {
		t.Fatal("omitted rule reached native configuration")
	}
	if !strings.Contains(string(accepted), "example.test") {
		t.Fatal("supported domain rule absent")
	}
	saved, err := os.ReadFile(filepath.Join(s.dataDir, "proxy-selection.json"))
	if err != nil {
		t.Fatal(err)
	}
	var selection struct {
		NodeID string `json:"nodeId"`
	}
	if err := json.Unmarshal(saved, &selection); err != nil || selection.NodeID != sub.Nodes[0].ID {
		t.Fatal("explicit apply did not save selection")
	}
}

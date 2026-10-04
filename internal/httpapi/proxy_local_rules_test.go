package httpapi

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"be6500panel/internal/core"
	"be6500panel/internal/localrules"
	"be6500panel/internal/modules"
	"be6500panel/internal/proxy"
	managedruntime "be6500panel/internal/runtime"
	"be6500panel/internal/storage"
)

func localPolicyServer(t *testing.T) (*Server, *httptest.Server) {
	t.Helper()
	s, ts := policyServer(t)
	var err error
	s.localRules, err = localrules.New(localrules.Options{DataDir: s.dataDir, Context: s.ctx, StorageAdmission: s.storageAdmission})
	if err != nil {
		t.Fatal(err)
	}
	return s, ts
}
func gptLocalPolicy() proxy.Policy {
	return proxy.Policy{Rules: []proxy.LocalRule{{ID: "gpt-direct", Enabled: true, Label: "GPT direct", Note: "", Rule: proxy.Rule{Kind: proxy.RuleDomain, Value: "gpt.kanglives.top", Target: proxy.TargetDirect}}}, SubscriptionEdits: []proxy.SubscriptionEdit{}}
}
func policyBody(t *testing.T, p proxy.Policy) string {
	t.Helper()
	raw, err := json.Marshal(map[string]any{"policy": p})
	if err != nil {
		t.Fatal(err)
	}
	return string(raw)
}
func readLocalPolicy(t *testing.T, ts *httptest.Server) localRulesReadback {
	t.Helper()
	res := request(t, ts, http.MethodGet, LocalRulesPath, "", nil)
	if res.StatusCode != 200 {
		t.Fatalf("read HTTP %d: %+v", res.StatusCode, decode(t, res))
	}
	var body localRulesReadback
	if err := json.NewDecoder(res.Body).Decode(&body); err != nil {
		t.Fatal(err)
	}
	return body
}
func saveLocalPolicy(t *testing.T, ts *httptest.Server, p proxy.Policy) localRulesReadback {
	t.Helper()
	res := request(t, ts, http.MethodPost, LocalRulesPath, policyBody(t, p), nil)
	if res.StatusCode != 200 {
		t.Fatalf("save HTTP %d: %+v", res.StatusCode, decode(t, res))
	}
	var body localRulesReadback
	if err := json.NewDecoder(res.Body).Decode(&body); err != nil {
		t.Fatal(err)
	}
	return body
}
func applyLocalBody(t *testing.T, draft string, generation uint64, ack string) string {
	t.Helper()
	body := map[string]any{"revision": draft, "generation": generation}
	if ack != "" {
		body["acknowledgedRevision"] = ack
	}
	raw, _ := json.Marshal(body)
	return string(raw)
}
func acquireLocalVerifier(t *testing.T, s *Server, raw string) {
	t.Helper()
	path := filepath.Join(s.dataDir, "local-offline-verifier")
	if err := os.WriteFile(path, []byte(raw), 0700); err != nil {
		t.Fatal(err)
	}
	_, err := s.runtime.Acquire(context.Background(), managedruntime.SingBox, managedruntime.Artifact{URL: "file://" + path, SHA256: nativeConfigHash([]byte(raw)), Compression: "none", Version: "offline-fixture"})
	if err != nil {
		t.Fatal(err)
	}
}

const localOfflineVerifier = "#!/bin/sh\n[ \"$1\" = check ] && [ \"$2\" = -c ] && exit 0\nexit 1\n"

func selectLocalNode(t *testing.T, s *Server, ts *httptest.Server) {
	t.Helper()
	sub := s.proxyState.subscription
	res := request(t, ts, http.MethodPost, "/api/proxy/select", selectPolicyBody(sub.Nodes[0].ID, summarizeProxyPolicy(sub).Revision), nil)
	if res.StatusCode != 200 {
		t.Fatalf("select HTTP %d: %+v", res.StatusCode, decode(t, res))
	}
	_ = decode(t, res)
}
func assertLocalError(t *testing.T, res *http.Response, status int, code string) {
	t.Helper()
	body := decode(t, res)
	if res.StatusCode != status || body["error"].(map[string]any)["code"] != code {
		t.Fatalf("HTTP %d: %+v", res.StatusCode, body)
	}
}

func TestLocalRulesReadSavePreviewAreIndependentAndPure(t *testing.T) {
	s, ts := localPolicyServer(t)
	initial := readLocalPolicy(t, ts)
	if len(initial.Draft.Policy.Rules) != 0 || len(initial.Draft.Policy.SubscriptionEdits) != 0 || len(initial.Draft.Revision) != 64 || initial.Applied.State != "unknown" || initial.RuntimeGeneration != 0 || initial.PolicySummary.Omitted != 3 {
		t.Fatal(initial)
	}
	files, err := os.ReadDir(s.dataDir)
	if err != nil || len(files) != 0 {
		t.Fatal("GET seeded storage", files, err)
	}
	local := gptLocalPolicy()
	preview := request(t, ts, http.MethodPost, LocalRulesPreviewPath, policyBody(t, local), nil)
	if preview.StatusCode != 200 {
		t.Fatal(preview.Status)
	}
	var merged proxy.EffectivePolicy
	if err := json.NewDecoder(preview.Body).Decode(&merged); err != nil {
		t.Fatal(err)
	}
	if merged.Rules[0].Value != "gpt.kanglives.top" || merged.Rules[0].Target != proxy.TargetDirect || merged.Provenance[0].Layer != "local" {
		t.Fatal(merged)
	}
	if len(s.localRules.Snapshot().Policy.Rules) != 0 {
		t.Fatal("preview saved draft")
	}
	files, _ = os.ReadDir(s.dataDir)
	if len(files) != 0 {
		t.Fatal("preview wrote disk")
	}
	saved := saveLocalPolicy(t, ts, local)
	if saved.Draft.Revision == initial.Draft.Revision || len(saved.Draft.Policy.Rules) != 1 || saved.Applied.State != "unknown" || saved.RuntimeGeneration != 0 {
		t.Fatal(saved)
	}
	state, _ := s.runtime.Status(managedruntime.SingBox)
	if state.Configured || state.Generation != 0 || state.PID != 0 {
		t.Fatal("draft configured runtime", state)
	}
	doc, err := os.Stat(filepath.Join(s.dataDir, localrules.FileName))
	if err != nil || doc.Mode().Perm() != 0600 {
		t.Fatal("draft not private", doc, err)
	}
	raw, _ := json.Marshal(saved)
	for _, private := range []string{"11111111-1111", "AAAAAAAAAAA", "private-app", "outbounds", "argv"} {
		if strings.Contains(string(raw), private) {
			t.Fatal("private input leaked", private)
		}
	}
}

func TestLocalRulesStrictBoundedJSONAndValidationKeepDraft(t *testing.T) {
	s, ts := localPolicyServer(t)
	original := saveLocalPolicy(t, ts, gptLocalPolicy()).Draft.Revision
	body := policyBody(t, gptLocalPolicy())
	for _, invalid := range []string{
		strings.Replace(body, "\"policy\"", "\"Policy\"", 1),
		strings.Replace(body, "\"target\":\"direct\"", "\"target\":\"direct\",\"Target\":\"proxy\"", 1),
		strings.Replace(body, "\"enabled\":true", "\"enabled\":null", 1),
		strings.Replace(body, "\"kind\":\"domain\"", "\"kind\":\"domain\",\"kind\":\"match\"", 1),
		body + " {}", `{"policy":{"rules":null,"subscriptionEdits":[]}}`,
	} {
		res := request(t, ts, http.MethodPost, LocalRulesPath, invalid, nil)
		if res.StatusCode != 400 {
			t.Fatalf("loose JSON accepted HTTP %d: %s", res.StatusCode, invalid)
		}
		_ = decode(t, res)
	}
	for _, invalid := range []string{`{"policy":{"rules":[],"subscriptionEdits":[]},"config":"private"}`, `{"policy":{"rules":[]}}`} {
		res := request(t, ts, http.MethodPost, LocalRulesPath, invalid, nil)
		if res.StatusCode != 400 && res.StatusCode != 422 {
			t.Fatal(res.Status)
		}
		_ = decode(t, res)
	}
	invalid := gptLocalPolicy()
	invalid.Rules[0].Rule.Kind = "process-name"
	assertLocalError(t, request(t, ts, http.MethodPost, LocalRulesPath, policyBody(t, invalid), nil), 422, "local_rules_invalid")
	oversized := `{"policy":{"rules":[],"subscriptionEdits":[]},"padding":"` + strings.Repeat("x", localrules.MaxFileBytes) + `"}`
	assertLocalError(t, request(t, ts, http.MethodPost, LocalRulesPath, oversized, nil), 413, "body_too_large")
	if s.localRules.Snapshot().Revision != original {
		t.Fatal("invalid save lost policy")
	}
}

func TestLocalRulesStorageFailureKeepsDraft(t *testing.T) {
	s, ts := localPolicyServer(t)
	original := saveLocalPolicy(t, ts, gptLocalPolicy()).Draft
	admission := func(context.Context, string, int64, bool) (func(), error) { return nil, storage.ErrInsufficientSpace }
	var err error
	s.localRules, err = localrules.New(localrules.Options{DataDir: s.dataDir, Context: s.ctx, StorageAdmission: admission})
	if err != nil {
		t.Fatal(err)
	}
	candidate := gptLocalPolicy()
	candidate.Rules[0].Rule.Target = proxy.TargetProxy
	assertLocalError(t, request(t, ts, http.MethodPost, LocalRulesPath, policyBody(t, candidate), nil), 409, "storage_insufficient")
	if s.localRules.Snapshot().Revision != original.Revision {
		t.Fatal("storage refusal replaced draft")
	}
	state, _ := s.runtime.Status(managedruntime.SingBox)
	if state.Configured {
		t.Fatal("storage refusal configured runtime")
	}
}

func TestLocalRulesMalformedDocumentDoesNotAbortPanelOrDropOverlay(t *testing.T) {
	dir := t.TempDir()
	raw := []byte(`{"rules":"private-corruption","subscriptionEdits":[]}`)
	if err := os.WriteFile(filepath.Join(dir, localrules.FileName), raw, 0600); err != nil {
		t.Fatal(err)
	}
	system := modules.NewSystem(true)
	sampler := core.NewSampler(system.Observe, time.Hour)
	s, err := New(Config{System: system, Sampler: sampler, DataDir: dir})
	if err != nil {
		t.Fatal("malformed draft aborted panel", err)
	}
	t.Cleanup(s.Close)
	if s.localRules != nil || s.localRulesError == nil {
		t.Fatal("malformed overlay silently erased")
	}
	for _, path := range []string{LocalRulesPath, LocalRulesPreviewPath, LocalRulesApplyPath} {
		method := http.MethodPost
		if path == LocalRulesPath {
			method = http.MethodGet
		}
		req := httptest.NewRequest(method, path, strings.NewReader(`{"policy":{"rules":[],"subscriptionEdits":[]}}`))
		req.Header.Set("Content-Type", "application/json")
		rec := httptest.NewRecorder()
		s.ServeHTTP(rec, req)
		if rec.Code != 503 || !strings.Contains(rec.Body.String(), "local_rules_unavailable") || strings.Contains(rec.Body.String(), "private-corruption") {
			t.Fatal(rec.Code, rec.Body.String())
		}
	}
	rec := httptest.NewRecorder()
	s.ServeHTTP(rec, httptest.NewRequest("GET", "/api/health", nil))
	if rec.Code != 200 {
		t.Fatal("panel health blocked")
	}
	if _, err := s.localPolicySnapshot(); err == nil {
		t.Fatal("normal selection would drop invalid overlay")
	}
	after, _ := os.ReadFile(filepath.Join(dir, localrules.FileName))
	if !bytes.Equal(raw, after) {
		t.Fatal("malformed draft modified")
	}
}

func TestLocalRulesSubscriptionRefreshKeepsOrphanEditsVisible(t *testing.T) {
	s, ts := localPolicyServer(t)
	state := readLocalPolicy(t, ts)
	policy := gptLocalPolicy()
	policy.SubscriptionEdits = append(policy.SubscriptionEdits, proxy.SubscriptionEdit{ID: "disable-old", SourceFingerprint: state.SubscriptionRules[0].Fingerprint, Disabled: true})
	saved := saveLocalPolicy(t, ts, policy)
	changed := strings.Replace(policySubscription, "DOMAIN,example.test,DIRECT", "DOMAIN,changed.test,DIRECT", 1)
	raw, _ := json.Marshal(map[string]string{"content": changed})
	res := request(t, ts, http.MethodPost, "/api/proxy/import", string(raw), nil)
	if res.StatusCode != 200 {
		t.Fatal(res.Status)
	}
	_ = decode(t, res)
	refreshed := readLocalPolicy(t, ts)
	if refreshed.Draft.Revision != saved.Draft.Revision || refreshed.SubscriptionRevision == saved.SubscriptionRevision || len(refreshed.Draft.Policy.SubscriptionEdits) != 1 {
		t.Fatal("refresh erased overlay", refreshed)
	}
	orphan := false
	for _, d := range refreshed.Preview.Diagnostics {
		if d.Code == "orphaned-edit" {
			orphan = true
		}
	}
	if !orphan || refreshed.Preview.Rules[0].Value != "gpt.kanglives.top" || refreshed.Preview.Rules[1].Value != "changed.test" {
		t.Fatal("orphan applied to unrelated rule", refreshed.Preview)
	}
	if s.proxyState.selected != "" {
		t.Fatal("refresh arbitrarily selected a node")
	}
}

func TestLocalRulesApplyRequiresRevisionGenerationNodeIdentityAndAcknowledgment(t *testing.T) {
	s, ts := localPolicyServer(t)
	stagePolicyRuleSets(t, s.dataDir)
	acquireLocalVerifier(t, s, localOfflineVerifier)
	saved := saveLocalPolicy(t, ts, gptLocalPolicy())
	assertLocalError(t, request(t, ts, "POST", LocalRulesApplyPath, applyLocalBody(t, saved.Draft.Revision, 0, saved.PolicySummary.Revision), nil), 409, "selected_node_unavailable")
	selectLocalNode(t, s, ts)
	state := readLocalPolicy(t, ts)
	for _, tc := range []struct {
		revision   string
		generation uint64
		ack, code  string
	}{
		{strings.Repeat("a", 64), state.RuntimeGeneration, state.PolicySummary.Revision, "local_rules_revision_changed"},
		{state.Draft.Revision, state.RuntimeGeneration - 1, state.PolicySummary.Revision, "generation_conflict"},
		{state.Draft.Revision, state.RuntimeGeneration, "", "policy_acknowledgment_required"},
		{state.Draft.Revision, state.RuntimeGeneration, strings.Repeat("a", 64), "policy_revision_changed"},
	} {
		assertLocalError(t, request(t, ts, "POST", LocalRulesApplyPath, applyLocalBody(t, tc.revision, tc.generation, tc.ack), nil), 409, tc.code)
	}
	original := s.proxyState.subscription.Nodes[0].RealityPublicKey
	s.proxyState.subscription.Nodes[0].RealityPublicKey = strings.Repeat("B", 43)
	assertLocalError(t, request(t, ts, "POST", LocalRulesApplyPath, applyLocalBody(t, state.Draft.Revision, state.RuntimeGeneration, state.PolicySummary.Revision), nil), 409, "selected_node_changed")
	s.proxyState.subscription.Nodes[0].RealityPublicKey = original
	after, _ := s.runtime.Status(managedruntime.SingBox)
	if after.Generation != state.RuntimeGeneration {
		t.Fatal("refused apply changed runtime")
	}
	for _, invalid := range []string{`{"revision":"` + state.Draft.Revision + `"}`, `{"revision":"` + state.Draft.Revision + `","generation":null}`, `{"revision":"` + state.Draft.Revision + `","generation":1,"config":"private"}`} {
		res := request(t, ts, "POST", LocalRulesApplyPath, invalid, nil)
		if res.StatusCode != 400 {
			t.Fatal(res.Status)
		}
		_ = decode(t, res)
	}
}

func TestLocalRulesApplyPreservesActualSettingsAndTracksAppliedNotDraft(t *testing.T) {
	s, ts := localPolicyServer(t)
	stagePolicyRuleSets(t, s.dataDir)
	acquireLocalVerifier(t, s, localOfflineVerifier)
	selectLocalNode(t, s, ts)
	accepted, generation, err := s.runtime.Config(managedruntime.SingBox)
	if err != nil {
		t.Fatal(err)
	}
	var config map[string]any
	if json.Unmarshal(accepted, &config) != nil {
		t.Fatal("config fixture")
	}
	for _, v := range config["inbounds"].([]any) {
		listener := v.(map[string]any)
		switch listener["tag"] {
		case "mixed-in":
			listener["listen_port"] = float64(2092)
			listener["listen"] = "127.0.0.1"
		case "dns-in":
			listener["listen_port"] = float64(6451)
		case "tun-in":
			listener["interface_name"] = "b6p-custom"
			listener["address"] = []string{"172.31.254.253/30"}
		}
	}
	dns := config["dns"].(map[string]any)
	dns["cache_capacity"] = float64(789)
	dns["strategy"] = "ipv4_only"
	for _, v := range dns["servers"].([]any) {
		server := v.(map[string]any)
		switch server["tag"] {
		case "dns-direct":
			server["server"] = "9.9.9.9"
			server["tls"].(map[string]any)["server_name"] = "dns.quad9.net"
		case "dns-proxy":
			server["server"] = "8.8.8.8"
			server["tls"].(map[string]any)["server_name"] = "dns.google"
		}
	}
	config["experimental"] = map[string]any{"clash_api": map[string]any{"external_controller": "127.0.0.1:9090", "secret": "local-private-token"}}
	changed, _ := json.Marshal(config)
	status, err := s.runtime.Configure(context.Background(), managedruntime.SingBox, changed, generation)
	if err != nil {
		t.Fatal(err)
	}
	saved := saveLocalPolicy(t, ts, gptLocalPolicy())
	if saved.Applied.State != "unknown" {
		t.Fatal("advanced config falsely mapped old manifest", saved.Applied)
	}
	res := request(t, ts, "POST", LocalRulesApplyPath, applyLocalBody(t, saved.Draft.Revision, status.Generation, saved.PolicySummary.Revision), nil)
	if res.StatusCode != 200 {
		t.Fatalf("apply HTTP %d: %+v", res.StatusCode, decode(t, res))
	}
	body := decode(t, res)
	if body["applied"] != true || body["draftRevision"] != saved.Draft.Revision {
		t.Fatal(body)
	}
	actual, gen, err := s.runtime.Config(managedruntime.SingBox)
	if err != nil || nativeConfigHash(actual) != body["configSHA256"] {
		t.Fatal("accepted readback mismatch", err)
	}
	var compiled map[string]any
	_ = json.Unmarshal(actual, &compiled)
	if !strings.Contains(string(actual), "b6p-custom") || !strings.Contains(string(actual), "172.31.254.253/30") || !strings.Contains(string(actual), "local-private-token") {
		t.Fatal("lost tun or telemetry settings")
	}
	if compiled["dns"].(map[string]any)["cache_capacity"] != float64(789) || compiled["dns"].(map[string]any)["strategy"] != "ipv4_only" {
		t.Fatal("lost resolver settings")
	}
	options, err := acceptedProxyCompileInput(actual)
	if err != nil || options.Ports.Mixed != 2092 || options.Ports.DNS != 6451 || options.DirectDNS.Server != "9.9.9.9" || options.ProxyDNS.Server != "8.8.8.8" {
		t.Fatal("lost native settings", options, err)
	}
	rules := compiled["route"].(map[string]any)["rules"].([]any)
	localAt, subscriptionAt, sniffAt := -1, -1, -1
	for i, v := range rules {
		rule := v.(map[string]any)
		if rule["action"] == "sniff" {
			sniffAt = i
		}
		if domains, ok := rule["domain"].([]any); ok && len(domains) == 1 {
			if domains[0] == "gpt.kanglives.top" {
				if localAt != -1 || rule["outbound"] != "direct" {
					t.Fatal("duplicate/wrong overlay")
				}
				localAt = i
			}
			if domains[0] == "example.test" && rule["outbound"] == "direct" {
				subscriptionAt = i
			}
		}
	}
	if sniffAt < 0 || localAt <= sniffAt || subscriptionAt <= localAt {
		t.Fatal("overlay precedence broke native prelude", sniffAt, localAt, subscriptionAt)
	}
	read := readLocalPolicy(t, ts)
	if read.Applied.State != "known" || read.Applied.Revision != saved.Draft.Revision || read.Applied.Generation != gen {
		t.Fatal(read.Applied)
	}
	newer := gptLocalPolicy()
	newer.Rules[0].Rule.Target = proxy.TargetProxy
	draft := saveLocalPolicy(t, ts, newer)
	if draft.Draft.Revision == read.Applied.Revision || draft.Applied.Revision != read.Applied.Revision {
		t.Fatal("saved draft falsely active", draft)
	}
	selectLocalNode(t, s, ts)
	selected := readLocalPolicy(t, ts)
	if selected.Applied.Revision != draft.Draft.Revision {
		t.Fatal("nodeSelect dropped current policy", selected)
	}
	manifest := filepath.Join(s.dataDir, localRulesAppliedFile)
	info, _ := os.Stat(manifest)
	if info.Mode().Perm() != 0600 {
		t.Fatal("manifest not private")
	}
	_ = os.Remove(manifest)
	if readLocalPolicy(t, ts).Applied.State != "unknown" {
		t.Fatal("missing manifest claimed success")
	}
}

func TestLocalRulesFailedCheckAndManifestFailureKeepActualStatus(t *testing.T) {
	s, ts := localPolicyServer(t)
	stagePolicyRuleSets(t, s.dataDir)
	verifier := "#!/bin/sh\n[ \"$1\" = check ] || exit 1\ngrep -q fail.local.test \"$3\" && exit 1\nexit 0\n"
	acquireLocalVerifier(t, s, verifier)
	selectLocalNode(t, s, ts)
	prior := readLocalPolicy(t, ts)
	manifestPath := filepath.Join(s.dataDir, localRulesAppliedFile)
	oldManifest, _ := os.ReadFile(manifestPath)
	policy := gptLocalPolicy()
	policy.Rules[0].Rule.Value = "fail.local.test"
	draft := saveLocalPolicy(t, ts, policy)
	assertLocalError(t, request(t, ts, "POST", LocalRulesApplyPath, applyLocalBody(t, draft.Draft.Revision, prior.RuntimeGeneration, draft.PolicySummary.Revision), nil), 422, "config_check_failed")
	after := readLocalPolicy(t, ts)
	if after.Applied.Revision != prior.Applied.Revision || after.RuntimeGeneration != prior.RuntimeGeneration {
		t.Fatal("failed candidate claimed applied", after)
	}
	unchanged, _ := os.ReadFile(manifestPath)
	if !bytes.Equal(oldManifest, unchanged) {
		t.Fatal("failed check changed manifest")
	}
	draft = saveLocalPolicy(t, ts, gptLocalPolicy())
	s.storageAdmission = func(context.Context, string, int64, bool) (func(), error) { return nil, storage.ErrInsufficientSpace }
	res := request(t, ts, "POST", LocalRulesApplyPath, applyLocalBody(t, draft.Draft.Revision, after.RuntimeGeneration, draft.PolicySummary.Revision), nil)
	assertLocalError(t, res, 500, "local_rules_applied_unknown")
	actual, _ := s.runtime.Status(managedruntime.SingBox)
	if actual.Generation != after.RuntimeGeneration+1 || !actual.Configured {
		t.Fatal("manifest failure hid accepted config", actual)
	}
	if readLocalPolicy(t, ts).Applied.State != "unknown" {
		t.Fatal("unconfirmed manifest claimed applied")
	}
	raw, gen, err := s.runtime.Config(managedruntime.SingBox)
	if err != nil {
		t.Fatal(err)
	}
	snapshot := s.localRules.Snapshot()
	if err := s.recordLocalRulesApplied(context.Background(), snapshot, proxy.CompileOutput{Config: raw, SHA256: strings.Repeat("a", 64)}, managedruntime.Status{Generation: gen}); err == nil {
		t.Fatal("bad readback hash attested")
	}
	unchanged, _ = os.ReadFile(manifestPath)
	if !bytes.Equal(oldManifest, unchanged) {
		t.Fatal("uncertainty overwrote manifest")
	}
}

func TestLocalRulesFailedLiveApplyRestoresWithoutClaimingNewPolicy(t *testing.T) {
	s, ts := localPolicyServer(t)
	stagePolicyRuleSets(t, s.dataDir)
	_ = s.runtime.Close()
	manager, err := managedruntime.New(managedruntime.Options{DataDir: t.TempDir(), RunDir: t.TempDir(), LocalSourceRoot: s.dataDir, TermGrace: 50 * time.Millisecond, PreStartHook: func(_ context.Context, _ string, raw []byte) error {
		if bytes.Contains(raw, []byte("gpt.kanglives.top")) {
			return errors.New("fixed offline candidate failure")
		}
		return nil
	}, ReadyHook: func(context.Context, string) error { return nil }})
	if err != nil {
		t.Fatal(err)
	}
	s.runtime = manager
	t.Cleanup(func() { _ = manager.Close() })
	// One idle local fake process; no native core, socket or kernel operations.
	acquireLocalVerifier(t, s, "#!/bin/sh\n[ \"$1\" = check ] && exit 0\n[ \"$1\" = run ] && exec /usr/bin/tail -f /dev/null\nexit 1\n")
	selectLocalNode(t, s, ts)
	first, err := manager.Start(context.Background(), managedruntime.SingBox)
	if err != nil {
		t.Fatal(err)
	}
	oldManifest, _ := os.ReadFile(filepath.Join(s.dataDir, localRulesAppliedFile))
	draft := saveLocalPolicy(t, ts, gptLocalPolicy())
	res := request(t, ts, "POST", LocalRulesApplyPath, applyLocalBody(t, draft.Draft.Revision, first.Generation, draft.PolicySummary.Revision), nil)
	if res.StatusCode == 200 {
		t.Fatal("failed restored candidate claimed applied")
	}
	body := decode(t, res)
	status := body["status"].(map[string]any)
	if status["restored"] != true || status["state"] != "running" || body["applied"] == true {
		t.Fatal("restored status hidden", body)
	}
	read := readLocalPolicy(t, ts)
	if read.Applied.State == "known" && read.Applied.Revision == draft.Draft.Revision {
		t.Fatal("restored draft claimed active")
	}
	manifest, _ := os.ReadFile(filepath.Join(s.dataDir, localRulesAppliedFile))
	if !bytes.Equal(manifest, oldManifest) {
		t.Fatal("failed live candidate wrote manifest")
	}
	accepted, _, err := manager.Config(managedruntime.SingBox)
	if err != nil || bytes.Contains(accepted, []byte("gpt.kanglives.top")) {
		t.Fatal("restore did not retain old rules", err)
	}
}

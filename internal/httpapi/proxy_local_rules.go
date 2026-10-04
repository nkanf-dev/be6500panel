package httpapi

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"net/netip"
	"os"
	"path/filepath"
	"reflect"

	"be6500panel/internal/localrules"
	"be6500panel/internal/proxy"
	managedruntime "be6500panel/internal/runtime"
	"be6500panel/internal/storage"
)

const (
	LocalRulesPath        = "/api/proxy/local-rules"
	LocalRulesPreviewPath = LocalRulesPath + "/preview"
	LocalRulesApplyPath   = LocalRulesPath + "/apply"
	localRulesAppliedFile = "local-proxy-rules-applied.json"
)

type localSubscriptionRule struct {
	Fingerprint string     `json:"fingerprint"`
	Rule        proxy.Rule `json:"rule"`
}
type localRulesApplied struct {
	State      string `json:"state"`
	Revision   string `json:"revision,omitempty"`
	Generation uint64 `json:"generation,omitempty"`
}
type localRulesManifest struct {
	Revision     string `json:"revision"`
	NativeSHA256 string `json:"nativeSHA256"`
	Generation   uint64 `json:"generation"`
}

var reflectManifestType = reflect.TypeOf(localRulesManifest{})

type localRulesReadback struct {
	Draft                localrules.Snapshot     `json:"draft"`
	SubscriptionRevision string                  `json:"subscriptionRevision"`
	SubscriptionRules    []localSubscriptionRule `json:"subscriptionRules"`
	Preview              proxy.EffectivePolicy   `json:"preview"`
	Applied              localRulesApplied       `json:"applied"`
	RuntimeGeneration    uint64                  `json:"runtimeGeneration"`
	PolicySummary        proxyPolicySummary      `json:"policySummary"`
}

func (s *Server) localRulesAvailable(w http.ResponseWriter) bool {
	if s.localRules == nil || s.localRulesError != nil {
		fail(w, 503, "local_rules_unavailable", "本地规则存储不可用，请检查配置存储")
		return false
	}
	return true
}

// A panel without persistent storage has no overlay. A malformed existing
// draft is different: normal node selections must refuse, never drop it.
func (s *Server) localPolicySnapshot() (localrules.Snapshot, error) {
	if s.localRulesError != nil {
		return localrules.Snapshot{}, s.localRulesError
	}
	if s.localRules != nil {
		return s.localRules.Snapshot(), nil
	}
	policy := proxy.ClonePolicy(proxy.Policy{})
	revision, err := proxy.PolicyRevision(policy)
	return localrules.Snapshot{Policy: policy, Revision: revision}, err
}

func localPolicyPreview(sub proxy.Subscription, policy proxy.Policy) (proxy.EffectivePolicy, error) {
	out, err := proxy.MergeEffectivePolicy(sub.Rules, policy)
	if err != nil {
		return out, err
	}
	// Subscription diagnostics are fixed public reasons, not source YAML text.
	for _, omission := range summarizeProxyPolicy(sub).OmittedRules {
		out.Diagnostics = append(out.Diagnostics, proxy.Diagnostic{Scope: "subscription", Index: omission.Index, Code: omission.Code, Message: proxyPolicyReasonMessage(omission.Code)})
	}
	return out, nil
}

func (s *Server) proxyLocalRules(w http.ResponseWriter, r *http.Request) {
	if !s.localRulesAvailable(w) {
		return
	}
	if r.Method == http.MethodPost {
		policy, ok := decodeLocalPolicy(w, r)
		if !ok {
			return
		}
		p := s.proxyState
		if !p.mutationMu.TryLock() {
			fail(w, 409, "proxy_mutation_pending", "订阅或节点配置正在保存，请稍后重试")
			return
		}
		defer p.mutationMu.Unlock()
		if _, err := s.localRules.Save(r.Context(), policy); err != nil {
			if errors.Is(err, storage.ErrInsufficientSpace) || errors.Is(err, storage.ErrMeasurement) {
				fail(w, 409, "storage_insufficient", "存储空间不足或不可测量，本地规则保存未确认")
			} else {
				fail(w, 500, "storage_failed", "本地规则保存未确认，请重新读取草稿")
			}
			return
		}
	}
	s.writeLocalRulesReadback(w)
}

func decodeLocalPolicy(w http.ResponseWriter, r *http.Request) (proxy.Policy, bool) {
	var input struct {
		Policy proxy.Policy `json:"policy"`
	}
	if !decodeJSONLimit(w, r, &input, localrules.MaxFileBytes, "policy") {
		return proxy.Policy{}, false
	}
	if input.Policy.Rules == nil || input.Policy.SubscriptionEdits == nil || proxy.ValidatePolicy(input.Policy) != nil {
		fail(w, 422, "local_rules_invalid", "本地规则字段、匹配条件或数量无效")
		return proxy.Policy{}, false
	}
	return proxy.ClonePolicy(input.Policy), true
}

func (s *Server) writeLocalRulesReadback(w http.ResponseWriter) {
	draft := s.localRules.Snapshot()
	p := s.proxyState
	p.mu.Lock()
	sub := p.subscription
	unavailable := p.loadFailed
	p.mu.Unlock()
	if unavailable {
		fail(w, 503, "rules_unavailable", "订阅规则不可用")
		return
	}
	preview, err := localPolicyPreview(sub, draft.Policy)
	fingerprints, refErr := proxy.SubscriptionFingerprints(sub.Rules)
	if err != nil || refErr != nil {
		fail(w, 503, "rules_unavailable", "规则预览不可用")
		return
	}
	rules := make([]localSubscriptionRule, 0, len(sub.Rules))
	for i, rule := range sub.Rules {
		rules = append(rules, localSubscriptionRule{fingerprints[i], rule})
	}
	summary := summarizeProxyPolicy(sub)
	applied, generation := s.readLocalRulesApplied()
	writeJSON(w, 200, localRulesReadback{Draft: draft, SubscriptionRevision: summary.Revision, SubscriptionRules: rules, Preview: preview, Applied: applied, RuntimeGeneration: generation, PolicySummary: summary})
}

func (s *Server) proxyLocalRulesPreview(w http.ResponseWriter, r *http.Request) {
	if !s.localRulesAvailable(w) {
		return
	}
	policy, ok := decodeLocalPolicy(w, r)
	if !ok {
		return
	}
	p := s.proxyState
	p.mu.Lock()
	sub, unavailable := p.subscription, p.loadFailed
	p.mu.Unlock()
	if unavailable {
		fail(w, 503, "rules_unavailable", "订阅规则不可用")
		return
	}
	out, err := localPolicyPreview(sub, policy)
	if err != nil {
		fail(w, 422, "local_rules_invalid", "本地规则无法合并")
		return
	}
	writeJSON(w, 200, out)
}

func (s *Server) readLocalRulesApplied() (localRulesApplied, uint64) {
	unknown := localRulesApplied{State: "unknown"}
	if s.runtime == nil {
		return unknown, 0
	}
	state, err := s.runtime.Status(managedruntime.SingBox)
	if err != nil {
		return unknown, 0
	}
	raw, generation, err := s.runtime.Config(managedruntime.SingBox)
	if err != nil {
		return unknown, state.Generation
	}
	path := filepath.Join(s.dataDir, localRulesAppliedFile)
	info, err := os.Lstat(path)
	if err != nil || !info.Mode().IsRegular() || info.Size() > 4096 {
		return unknown, generation
	}
	f, err := os.Open(path)
	if err != nil {
		return unknown, generation
	}
	defer f.Close()
	opened, err := f.Stat()
	if err != nil || !os.SameFile(info, opened) || !opened.Mode().IsRegular() {
		return unknown, generation
	}
	doc, err := io.ReadAll(io.LimitReader(f, 4097))
	if err != nil || len(doc) > 4096 || exactFields(doc, reflectManifestType) != nil {
		return unknown, generation
	}
	decoder := json.NewDecoder(bytes.NewReader(doc))
	if checkJSONValue(decoder) != nil {
		return unknown, generation
	}
	if _, err = decoder.Token(); err != io.EOF {
		return unknown, generation
	}
	var manifest localRulesManifest
	if json.Unmarshal(doc, &manifest) != nil || !isPolicyHash(manifest.Revision) || !isPolicyHash(manifest.NativeSHA256) || manifest.Generation != generation || nativeConfigHash(raw) != manifest.NativeSHA256 {
		return unknown, generation
	}
	return localRulesApplied{State: "known", Revision: manifest.Revision, Generation: generation}, generation
}

func isPolicyHash(value string) bool {
	raw, err := hex.DecodeString(value)
	return err == nil && len(raw) == sha256.Size && hex.EncodeToString(raw) == value
}
func nativeConfigHash(raw []byte) string {
	sum := sha256.Sum256(raw)
	return hex.EncodeToString(sum[:])
}

// Called only after ConfigureGuarded succeeds. A check, saved draft or a
// failed/restored candidate is never applied evidence.
func (s *Server) recordLocalRulesApplied(ctx context.Context, draft localrules.Snapshot, out proxy.CompileOutput, state managedruntime.Status) error {
	raw, generation, err := s.runtime.Config(managedruntime.SingBox)
	if err != nil || generation != state.Generation || nativeConfigHash(raw) != out.SHA256 || !bytes.Equal(raw, out.Config) {
		return errors.New("local_rules_readback_failed")
	}
	manifest, err := json.Marshal(localRulesManifest{Revision: draft.Revision, NativeSHA256: out.SHA256, Generation: generation})
	if err != nil {
		return err
	}
	return s.writePrivate(ctx, filepath.Join(s.dataDir, localRulesAppliedFile), manifest, false)
}

func localRulesConfiguredUncertain(w http.ResponseWriter, state managedruntime.Status, revision, hash string) {
	writeJSON(w, 500, struct {
		Error         apiError              `json:"error"`
		Status        managedruntime.Status `json:"status"`
		DraftRevision string                `json:"draftRevision"`
		SHA256        string                `json:"configSHA256"`
	}{apiError{Code: "local_rules_applied_unknown", Message: "运行配置已接受，但规则应用记录未确认，请重新读取运行状态"}, state, revision, hash})
}

func (s *Server) proxyLocalRulesApply(w http.ResponseWriter, r *http.Request) {
	if !s.localRulesAvailable(w) || !s.runtimeMutationAllowed(w) || !s.runtimeEnabled(w) {
		return
	}
	var input struct {
		Revision             string `json:"revision"`
		Generation           uint64 `json:"generation"`
		AcknowledgedRevision string `json:"acknowledgedRevision"`
	}
	if !decodeJSON(w, r, &input, "revision", "generation") {
		return
	}
	p := s.proxyState
	if !p.mutationMu.TryLock() {
		fail(w, 409, "proxy_mutation_pending", "订阅或节点配置正在保存，请稍后重试")
		return
	}
	defer p.mutationMu.Unlock()
	draft := s.localRules.Snapshot()
	if input.Revision != draft.Revision {
		fail(w, 409, "local_rules_revision_changed", "本地规则草稿已变更，请重新读取")
		return
	}
	p.mu.Lock()
	sub, selected, unavailable := p.subscription, p.selected, p.loadFailed
	p.mu.Unlock()
	if unavailable {
		fail(w, 503, "rules_unavailable", "订阅规则不可用")
		return
	}
	var node proxy.Node
	for _, candidate := range sub.Nodes {
		if candidate.ID == selected {
			node = candidate
			break
		}
	}
	if node.ID == "" {
		fail(w, 409, "selected_node_unavailable", "当前选择节点不可用，请先明确选择节点")
		return
	}
	state, err := s.runtime.Status(managedruntime.SingBox)
	if err != nil {
		s.runtimeError(w, err)
		return
	}
	if state.Generation != input.Generation {
		s.runtimeError(w, managedruntime.ErrGeneration)
		return
	}
	accepted, generation, err := s.runtime.Config(managedruntime.SingBox)
	if err != nil {
		s.runtimeError(w, err)
		return
	}
	if generation != input.Generation {
		s.runtimeError(w, managedruntime.ErrGeneration)
		return
	}
	if !acceptedSelectedNodeMatches(accepted, node) {
		fail(w, 409, "selected_node_changed", "当前运行节点与订阅选择不一致，请重新核对节点")
		return
	}
	summary := summarizeProxyPolicy(sub)
	if code, message := checkProxyPolicyAcknowledgment(summary, input.AcknowledgedRevision); code != "" {
		fail(w, 409, code, message)
		return
	}
	effective, err := proxy.MergeEffectivePolicy(sub.Rules, draft.Policy)
	if err != nil {
		fail(w, 422, "local_rules_invalid", "本地规则无法合并")
		return
	}
	refs, err := s.ruleSetReferences()
	if err != nil {
		fail(w, 409, "rules_unavailable", "国内规则集尚未就绪")
		return
	}
	options, err := acceptedProxyCompileInput(accepted)
	if err != nil {
		fail(w, 409, "proxy_configuration_invalid", "当前代理设置无法保持，请检查高级设置")
		return
	}
	out, err := compileEffectiveProxy(node, sub, effective, refs, options, accepted, input.AcknowledgedRevision == summary.Revision)
	if err != nil {
		fail(w, 422, "proxy_configuration_invalid", "代理策略生成失败")
		return
	}
	state, err = s.runtime.ConfigureGuarded(r.Context(), managedruntime.SingBox, out.Config, input.Generation, s.runtimeMutationGuard)
	if err != nil {
		s.runtimeResult(w, managedruntime.SingBox, "config_committed", state, err)
		return
	}
	if err = s.recordLocalRulesApplied(r.Context(), draft, out, state); err != nil {
		localRulesConfiguredUncertain(w, state, draft.Revision, out.SHA256)
		return
	}
	writeJSON(w, 200, struct {
		Status        managedruntime.Status `json:"status"`
		DraftRevision string                `json:"draftRevision"`
		SHA256        string                `json:"configSHA256"`
		Applied       bool                  `json:"applied"`
	}{state, draft.Revision, out.SHA256, true})
}

// Compare all selected-node transport credentials, not just a label or UUID.
func acceptedSelectedNodeMatches(raw []byte, node proxy.Node) bool {
	var cfg struct {
		Outbounds []struct {
			Type   string `json:"type"`
			Tag    string `json:"tag"`
			Server string `json:"server"`
			Port   uint16 `json:"server_port"`
			UUID   string `json:"uuid"`
			Flow   string `json:"flow"`
			TLS    struct {
				Enabled    bool   `json:"enabled"`
				ServerName string `json:"server_name"`
				UTLS       struct {
					Enabled     bool   `json:"enabled"`
					Fingerprint string `json:"fingerprint"`
				} `json:"utls"`
				Reality struct {
					Enabled   bool   `json:"enabled"`
					PublicKey string `json:"public_key"`
					ShortID   string `json:"short_id"`
				} `json:"reality"`
			} `json:"tls"`
		} `json:"outbounds"`
	}
	if json.Unmarshal(raw, &cfg) != nil {
		return false
	}
	matches := 0
	for _, out := range cfg.Outbounds {
		if out.Tag != "proxy" {
			continue
		}
		if out.Type != "vless" || out.Server != node.Server || out.Port != node.Port || out.UUID != node.UUID || out.Flow != node.Flow || !out.TLS.Enabled || out.TLS.ServerName != node.ServerName || !out.TLS.UTLS.Enabled || out.TLS.UTLS.Fingerprint != node.Fingerprint || !out.TLS.Reality.Enabled || out.TLS.Reality.PublicKey != node.RealityPublicKey || out.TLS.Reality.ShortID != node.RealityShortID {
			return false
		}
		matches++
	}
	return matches == 1
}

// Extract actual accepted settings for a rule-only change. No endpoint DNS
// lookup, default-node fallback, listener migration or kernel writes occur.
func acceptedProxyCompileInput(raw []byte) (proxy.CompileInput, error) {
	bad := func() (proxy.CompileInput, error) {
		return proxy.CompileInput{}, errors.New("accepted_proxy_settings_invalid")
	}
	mode, tun, err := proxyDatapathSelection(nil, nil, raw)
	if err != nil {
		return bad()
	}
	in := proxy.CompileInput{Datapath: mode, RoutedTUN: tun, IPv6: proxy.IPv6Direct, Failure: proxy.FailureDirect}
	var cfg struct {
		Inbounds []struct {
			Type   string `json:"type"`
			Tag    string `json:"tag"`
			Listen string `json:"listen"`
			Port   uint16 `json:"listen_port"`
		} `json:"inbounds"`
		DNS struct {
			Servers []struct {
				Type   string `json:"type"`
				Tag    string `json:"tag"`
				Server string `json:"server"`
				Port   uint16 `json:"server_port"`
				Detour string `json:"detour"`
				TLS    struct {
					Enabled    bool   `json:"enabled"`
					ServerName string `json:"server_name"`
				} `json:"tls"`
			} `json:"servers"`
			Rules []struct {
				Server     string   `json:"server"`
				Domains    []string `json:"domain"`
				Suffixes   []string `json:"domain_suffix"`
				QueryTypes []string `json:"query_type"`
			} `json:"rules"`
		} `json:"dns"`
		Route struct {
			Rules []struct {
				Outbound string   `json:"outbound"`
				Action   string   `json:"action"`
				IPs      []string `json:"ip_cidr"`
				Domains  []string `json:"domain"`
				Inbounds []string `json:"inbound"`
			} `json:"rules"`
		} `json:"route"`
	}
	if json.Unmarshal(raw, &cfg) != nil {
		return bad()
	}
	mixed, dns, tunCount := 0, 0, 0
	for _, listener := range cfg.Inbounds {
		switch listener.Tag {
		case "mixed-in":
			if listener.Type != "mixed" || listener.Listen == "" || listener.Port == 0 {
				return bad()
			}
			mixed++
			in.Ports.Mixed = listener.Port
			in.MixedListenAddress = listener.Listen
		case "dns-in":
			if listener.Type != "direct" || listener.Listen == "" || listener.Port == 0 {
				return bad()
			}
			dns++
			in.Ports.DNS = listener.Port
			in.DNSListenAddress = listener.Listen
		case "tun-in":
			if listener.Type != "tun" {
				return bad()
			}
			tunCount++
		default:
			return bad()
		}
	}
	if mixed != 1 || dns != 1 || tunCount != 1 {
		return bad()
	}
	in.Ports.TProxy = 7893
	local := proxy.LocalDNSConfig{}
	directCount, proxyCount, localCount := 0, 0, 0
	for _, server := range cfg.DNS.Servers {
		switch server.Tag {
		case "dns-direct", "dns-proxy":
			if server.Type != "tls" || !server.TLS.Enabled {
				return bad()
			}
			endpoint := proxy.DNSEndpoint{Server: server.Server, Port: server.Port, ServerName: server.TLS.ServerName}
			if server.Tag == "dns-direct" {
				if server.Detour != "direct" {
					return bad()
				}
				directCount++
				in.DirectDNS = endpoint
			} else {
				if server.Detour != "proxy" {
					return bad()
				}
				proxyCount++
				in.ProxyDNS = endpoint
			}
		case "dns-local":
			if server.Type != "udp" || server.Detour != "direct" || server.Server == "" || server.Port == 0 {
				return bad()
			}
			localCount++
			local.Server, local.Port = server.Server, server.Port
		case "dns-fake":
			if server.Type != "fakeip" || in.FakeIP {
				return bad()
			}
			in.FakeIP = true
		default:
			return bad()
		}
	}
	if directCount != 1 || proxyCount != 1 || localCount != 1 {
		return bad()
	}
	for _, rule := range cfg.DNS.Rules {
		if rule.Server == "dns-local" && len(rule.QueryTypes) == 0 {
			local.Domains = append(local.Domains, rule.Suffixes...)
			local.Hostnames = append(local.Hostnames, rule.Domains...)
		}
	}
	// The first native safety rules are generated outside the editable list.
	if len(cfg.Route.Rules) < 3 || cfg.Route.Rules[0].Action != "hijack-dns" || len(cfg.Route.Rules[0].Inbounds) != 1 || cfg.Route.Rules[0].Inbounds[0] != "dns-in" || cfg.Route.Rules[1].Outbound != "direct" {
		return bad()
	}
	for _, value := range cfg.Route.Rules[1].IPs {
		prefix, err := netip.ParsePrefix(value)
		if err != nil || prefix.Bits() != prefix.Addr().BitLen() {
			return bad()
		}
		in.Endpoints = append(in.Endpoints, prefix.Addr().String())
	}
	if len(in.Endpoints) == 0 || cfg.Route.Rules[2].Outbound != "direct" {
		return bad()
	}
	in.BootstrapDomains = append([]string{}, cfg.Route.Rules[2].Domains...)
	// Listener addresses remain actual router management intent. The loopback
	// local resolver is already mandatory; a non-loopback local authority is too.
	for _, value := range []string{in.MixedListenAddress, in.DNSListenAddress, local.Server} {
		address, err := netip.ParseAddr(value)
		if err != nil {
			return bad()
		}
		if !address.IsUnspecified() && !address.IsLoopback() {
			in.ManagementIPs = append(in.ManagementIPs, address.String())
		}
	}
	in.LocalDNS = &local
	return in, nil
}

func compileEffectiveProxy(node proxy.Node, sub proxy.Subscription, effective proxy.EffectivePolicy, refs []proxy.RuleSetReference, in proxy.CompileInput, accepted []byte, acknowledged bool) (proxy.CompileOutput, error) {
	in.Node, in.Rules, in.Overrides, in.RuleSets, in.Diagnostics, in.AcceptUnsupportedRules = node, effective.Rules, nil, refs, sub.Diagnostics, acknowledged
	out, err := proxy.CompileNative(in)
	if err != nil {
		return out, err
	}
	if len(accepted) > 0 {
		out.Config, err = preserveAcceptedProxySettings(out.Config, accepted)
		if err != nil {
			return proxy.CompileOutput{}, err
		}
		out.Config, err = preserveLocalTelemetry(out.Config, accepted)
		if err != nil {
			return proxy.CompileOutput{}, err
		}
		out.SHA256 = nativeConfigHash(out.Config)
	}
	out.Diagnostics = append(out.Diagnostics, effective.Diagnostics...)
	return out, nil
}

// Keep non-policy accepted DNS/dialer settings. Only rules, controlled set
// references and explicit node/listener intent are regenerated by compilation.
func preserveAcceptedProxySettings(candidate, accepted []byte) ([]byte, error) {
	var next, current map[string]json.RawMessage
	if json.Unmarshal(candidate, &next) != nil || json.Unmarshal(accepted, &current) != nil {
		return nil, errors.New("accepted_proxy_settings_invalid")
	}
	for _, name := range []string{"dns", "route"} {
		var generated, existing map[string]json.RawMessage
		if json.Unmarshal(next[name], &generated) != nil || json.Unmarshal(current[name], &existing) != nil {
			return nil, errors.New("accepted_proxy_settings_invalid")
		}
		for key, value := range existing {
			if key != "rules" && (name != "route" || key != "rule_set") {
				generated[key] = value
			}
		}
		next[name], _ = json.Marshal(generated)
	}
	if raw := current["log"]; len(raw) > 0 {
		next["log"] = raw
	}
	var generatedListeners, existingListeners []map[string]json.RawMessage
	if json.Unmarshal(next["inbounds"], &generatedListeners) != nil || json.Unmarshal(current["inbounds"], &existingListeners) != nil {
		return nil, errors.New("accepted_proxy_settings_invalid")
	}
	for _, old := range existingListeners {
		for _, listener := range generatedListeners {
			if !bytes.Equal(old["tag"], listener["tag"]) || !bytes.Equal(old["type"], listener["type"]) {
				continue
			}
			for key, value := range old {
				if _, generated := listener[key]; !generated {
					listener[key] = value
				}
			}
		}
	}
	next["inbounds"], _ = json.Marshal(generatedListeners)
	var generated, existing []map[string]json.RawMessage
	if json.Unmarshal(next["outbounds"], &generated) != nil || json.Unmarshal(current["outbounds"], &existing) != nil {
		return nil, errors.New("accepted_proxy_settings_invalid")
	}
	for _, old := range existing {
		var tag string
		_ = json.Unmarshal(old["tag"], &tag)
		for _, out := range generated {
			var nextTag string
			_ = json.Unmarshal(out["tag"], &nextTag)
			if tag != nextTag {
				continue
			}
			for key, value := range old {
				switch key {
				case "type", "tag", "server", "server_port", "uuid", "flow", "tls":
					continue
				}
				out[key] = value
			}
		}
	}
	next["outbounds"], _ = json.Marshal(generated)
	return json.Marshal(next)
}

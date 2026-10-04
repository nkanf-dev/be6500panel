package httpapi

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io"
	"net"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"be6500panel/internal/proxy"
	managedruntime "be6500panel/internal/runtime"
)

type proxyState struct {
	mutationMu   sync.Mutex // serialize subscription replacement with reviewed selection
	mu           sync.Mutex
	subscription proxy.Subscription
	selected     string
	dataDir      string
	loadFailed   bool
	revision     string
}

func newProxyState(dataDir string) *proxyState {
	p := &proxyState{dataDir: dataDir, revision: freshNodeRevision(), subscription: proxy.Subscription{Nodes: []proxy.Node{}, Rules: []proxy.Rule{}, Diagnostics: []proxy.Diagnostic{}}}
	if dataDir != "" {
		raw, err := os.ReadFile(filepath.Join(dataDir, "subscription.yaml"))
		if err == nil {
			sub, parseErr := proxy.ParseClashYAML(strings.NewReader(string(raw)))
			if parseErr == nil {
				p.subscription = sub
			} else {
				p.loadFailed = true
			}
		}
		state, _ := os.ReadFile(filepath.Join(dataDir, "proxy-selection.json"))
		var saved struct {
			NodeID string `json:"nodeId"`
		}
		if json.Unmarshal(state, &saved) == nil {
			p.selected = saved.NodeID
		}
	}
	return p
}
func (s *Server) proxyNodes(w http.ResponseWriter, r *http.Request) {
	p := s.proxyState
	p.mu.Lock()
	defer p.mu.Unlock()
	diagnostics := append([]proxy.Diagnostic{}, p.subscription.Diagnostics...)
	if p.loadFailed {
		diagnostics = append(diagnostics, proxy.Diagnostic{Scope: "subscription", Index: -1, Code: "subscription_unavailable", Message: "持久订阅读取失败"})
	}
	selected := p.selected
	if !s.ruleRecordedNodeMatchesAccepted(selected, p.subscription.Nodes) {
		selected = ""
	}
	writeJSON(w, 200, struct {
		Nodes         []proxy.PublicNode `json:"nodes"`
		Diagnostics   []proxy.Diagnostic `json:"diagnostics"`
		Selected      string             `json:"selectedNodeId"`
		Revision      string             `json:"revision"`
		PolicySummary proxyPolicySummary `json:"policySummary"`
	}{p.subscription.PublicNodes(), diagnostics, selected, p.revision, summarizeProxyPolicy(p.subscription)})
}
func (s *Server) proxyImport(w http.ResponseWriter, r *http.Request) {
	if !s.runtimeEnabled(w) {
		return
	}
	var input struct {
		URL     string `json:"url"`
		Content string `json:"content"`
	}
	if !decodeJSONLimit(w, r, &input, 3<<20) {
		return
	}
	if (input.URL == "") == (input.Content == "") {
		fail(w, 400, "invalid_input", "提供订阅 URL 或内容之一")
		return
	}
	raw := []byte(input.Content)
	if input.URL != "" {
		var err error
		raw, err = fetchPrivateSubscription(r.Context(), input.URL)
		if err != nil {
			fail(w, 502, "subscription_fetch_failed", "订阅下载失败")
			return
		}
	}
	sub, err := proxy.ParseClashYAML(strings.NewReader(string(raw)))
	if err != nil {
		fail(w, 422, "subscription_invalid", "订阅格式或节点参数无效")
		return
	}
	if len(sub.Nodes) == 0 {
		fail(w, 422, "no_compatible_nodes", "没有兼容的 VLESS 节点")
		return
	}
	if s.dataDir == "" {
		fail(w, 503, "configuration_unavailable", "配置存储未启用")
		return
	}
	p := s.proxyState
	if !p.mutationMu.TryLock() {
		fail(w, http.StatusConflict, "proxy_mutation_pending", "订阅或节点配置正在保存，请稍后重试")
		return
	}
	defer p.mutationMu.Unlock()
	if err = s.writePrivate(r.Context(), filepath.Join(s.dataDir, "subscription.yaml"), raw, false); err != nil {
		fail(w, 500, "storage_failed", "订阅保存失败")
		return
	}
	p.mu.Lock()
	p.subscription = sub
	p.loadFailed = false
	p.selected = ""
	p.revision = freshNodeRevision()
	revision := p.revision
	p.mu.Unlock()
	if s.nodeProbes != nil {
		s.nodeProbes.Invalidate(revision)
	}
	s.logger.Info("Subscription imported", "module", "proxy", "code", "subscription_imported", "nodes", len(sub.Nodes))
	s.proxyNodes(w, r)
}
func fetchPrivateSubscription(ctx context.Context, source string) ([]byte, error) {
	u, err := url.Parse(source)
	if err != nil || u.Scheme != "https" || u.Hostname() == "" || u.User != nil {
		return nil, errors.New("subscription URL invalid")
	}
	client := &http.Client{Timeout: 45 * time.Second, CheckRedirect: func(req *http.Request, via []*http.Request) error {
		if len(via) >= 3 || req.URL.Scheme != "https" || req.URL.User != nil {
			return errors.New("redirect invalid")
		}
		return nil
	}}
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, source, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("User-Agent", "be6500panel")
	res, err := client.Do(req)
	if err != nil {
		return nil, err
	}
	defer res.Body.Close()
	if res.StatusCode != 200 {
		return nil, errors.New("subscription unavailable")
	}
	raw, err := io.ReadAll(io.LimitReader(res.Body, proxy.MaxSubscriptionBytes+1))
	if err != nil || len(raw) > proxy.MaxSubscriptionBytes {
		return nil, errors.New("subscription exceeds limit")
	}
	return raw, nil
}
func (s *Server) proxySelect(w http.ResponseWriter, r *http.Request) {
	if !s.runtimeMutationAllowed(w) {
		return
	}
	if !s.ruleRuntimeEnabled(w) {
		return
	}
	var input struct {
		NodeID               string                 `json:"nodeId"`
		Datapath             *proxy.DatapathMode    `json:"datapath"`
		RoutedTUN            *proxy.RoutedTUNConfig `json:"routedTUN"`
		IPv6                 proxy.IPv6Mode         `json:"ipv6"`
		Failure              proxy.FailurePolicy    `json:"failure"`
		AcknowledgedRevision string                 `json:"acknowledgedRevision"`
		Ports                struct {
			Mixed  uint16 `json:"mixed"`
			TProxy uint16 `json:"tproxy"`
			DNS    uint16 `json:"dns"`
		} `json:"ports"`
	}
	if !decodeJSON(w, r, &input, "nodeId", "ipv6", "failure", "ports") {
		return
	}
	p := s.proxyState
	if !p.mutationMu.TryLock() {
		fail(w, http.StatusConflict, "proxy_mutation_pending", "订阅或节点配置正在保存，请稍后重试")
		return
	}
	defer p.mutationMu.Unlock()
	p.mu.Lock()
	sub := p.subscription
	var node proxy.Node
	found := false
	for _, candidate := range sub.Nodes {
		if candidate.ID == input.NodeID {
			node = candidate
			found = true
			break
		}
	}
	p.mu.Unlock()
	if !found {
		fail(w, 404, "node_not_found", "节点不存在")
		return
	}
	draft, err := s.localPolicySnapshot()
	if err != nil {
		fail(w, 503, "local_rules_unavailable", "本地规则存储不可用，请检查配置存储")
		return
	}
	effective, err := proxy.MergeEffectivePolicy(sub.Rules, draft.Policy)
	if err != nil {
		fail(w, 422, "local_rules_invalid", "本地规则无法合并")
		return
	}
	summary := summarizeProxyPolicy(sub)
	if code, message := checkProxyPolicyAcknowledgment(summary, input.AcknowledgedRevision); code != "" {
		fail(w, http.StatusConflict, code, message)
		return
	}
	refs, err := s.ruleSetReferences()
	if err != nil {
		fail(w, 409, "rules_unavailable", "国内规则集尚未就绪")
		return
	}
	ctx, cancel := context.WithTimeout(r.Context(), 15*time.Second)
	defer cancel()
	ips, err := net.DefaultResolver.LookupIPAddr(ctx, node.Server)
	if err != nil {
		fail(w, 502, "endpoint_resolution_failed", "节点地址解析失败")
		return
	}
	endpoints := []string{}
	for _, ip := range ips {
		if ip.Zone == "" {
			endpoints = append(endpoints, ip.IP.String())
		}
	}
	// The product has one reliable IPv4 routed-TUN path. IPv6 remains the
	// original router direct path; no retired socket backend is regenerated.
	if input.IPv6 != "" && input.IPv6 != proxy.IPv6Direct {
		fail(w, 422, "proxy_configuration_invalid", "当前网关使用 IPv6 直连")
		return
	}
	input.IPv6 = proxy.IPv6Direct
	input.Ports.TProxy = 7893 // unused internal recovery-era field, not a listener
	dnsBind := "192.168.31.1"
	state, err := s.ruleStatus(managedruntime.SingBox)
	if err != nil {
		s.ruleRuntimeError(w, err)
		return
	}
	var accepted []byte
	if state.Configured {
		accepted, _, err = s.ruleConfig(managedruntime.SingBox)
		if err != nil {
			s.ruleRuntimeError(w, err)
			return
		}
	}
	datapath, tunConfig, err := proxyDatapathSelection(input.Datapath, input.RoutedTUN, accepted)
	if err != nil {
		fail(w, 422, "proxy_datapath_invalid", "代理接管后端与已接受配置不一致")
		return
	}
	options := proxy.CompileInput{Datapath: datapath, RoutedTUN: tunConfig, Endpoints: endpoints, ManagementIPs: []string{"192.168.31.1"}, IPv6: input.IPv6, Failure: input.Failure, Ports: proxy.Ports{Mixed: input.Ports.Mixed, TProxy: input.Ports.TProxy, DNS: input.Ports.DNS}, MixedListenAddress: "192.168.31.1", DNSListenAddress: dnsBind}
	if state.Configured {
		current, settingsErr := acceptedProxyCompileInput(accepted)
		if settingsErr != nil {
			fail(w, 409, "proxy_configuration_invalid", "当前代理设置无法保持，请检查高级设置")
			return
		}
		// Node selection retains actual DNS authority, resolver strategy and
		// listener bind intent; the explicit request still owns ports/backend.
		options.DirectDNS, options.ProxyDNS, options.LocalDNS, options.FakeIP = current.DirectDNS, current.ProxyDNS, current.LocalDNS, current.FakeIP
		options.MixedListenAddress, options.DNSListenAddress = current.MixedListenAddress, current.DNSListenAddress
		options.ManagementIPs = current.ManagementIPs
		for _, host := range current.BootstrapDomains {
			if host != node.Server {
				options.BootstrapDomains = append(options.BootstrapDomains, host)
			}
		}
	}
	out, err := compileEffectiveProxy(node, sub, effective, refs, options, accepted, input.AcknowledgedRevision == summary.Revision)
	if err != nil {
		fail(w, 422, "proxy_configuration_invalid", "代理策略生成失败")
		return
	}
	saved, _ := json.Marshal(struct {
		NodeID    string         `json:"nodeId"`
		IPv6      proxy.IPv6Mode `json:"ipv6"`
		Ports     proxy.Ports    `json:"ports"`
		Endpoints []string       `json:"endpoints"`
	}{input.NodeID, input.IPv6, proxy.Ports{Mixed: input.Ports.Mixed, TProxy: input.Ports.TProxy, DNS: input.Ports.DNS}, endpoints})
	if s.storageAdmission != nil {
		release, admissionErr := s.storageAdmission(r.Context(), s.dataDir, int64(len(saved))+4096, false)
		if admissionErr != nil {
			fail(w, 409, "storage_insufficient", "存储空间不足，未切换节点；请先释放空间")
			return
		}
		defer release()
	}
	state, err = s.ruleConfigure(r.Context(), managedruntime.SingBox, out.Config, state.Generation)
	if err != nil {
		s.ruleRuntimeResult(w, state, err)
		return
	}
	// Only exact accepted readback can attest to the policy used by selection.
	if s.localRules != nil {
		if err = s.recordLocalRulesApplied(r.Context(), draft, out, state); err != nil {
			p.mu.Lock()
			p.selected = ""
			p.mu.Unlock()
			localRulesConfiguredUncertain(w, state, draft.Revision, out.SHA256)
			return
		}
	}
	// Admission is held across Configure, so a later write uses the same reservation.
	if err = writePrivateFile(filepath.Join(s.dataDir, "proxy-selection.json"), saved); err != nil {
		p.mu.Lock()
		p.selected = ""
		p.mu.Unlock()
		writeJSON(w, 500, struct {
			Error  apiError              `json:"error"`
			Status managedruntime.Status `json:"status"`
		}{apiError{Code: "storage_failed", Message: "节点配置已接受，但选择记录未保存。当前节点标记未知，请刷新运行状态。"}, state})
		return
	}
	p.mu.Lock()
	p.selected = input.NodeID
	p.mu.Unlock()
	s.logger.Info("Proxy configuration committed", "code", "proxy_config_committed", "module", "proxy")
	writeJSON(w, 200, struct {
		Status      managedruntime.Status `json:"status"`
		SHA256      string                `json:"configSHA256"`
		Diagnostics []proxy.Diagnostic    `json:"diagnostics"`
	}{state, out.SHA256, out.Diagnostics})
}
func writePrivateFile(path string, raw []byte) error {
	if err := os.MkdirAll(filepath.Dir(path), 0700); err != nil {
		return err
	}
	file, err := os.CreateTemp(filepath.Dir(path), ".pending-")
	if err != nil {
		return err
	}
	name := file.Name()
	defer os.Remove(name)
	if err = file.Chmod(0600); err == nil {
		_, err = file.Write(raw)
	}
	if err == nil {
		err = file.Sync()
	}
	if closeErr := file.Close(); err == nil {
		err = closeErr
	}
	if err != nil {
		return err
	}
	if err = os.Rename(name, path); err != nil {
		return err
	}
	dir, err := os.Open(filepath.Dir(path))
	if err != nil {
		return err
	}
	defer dir.Close()
	return dir.Sync()
}
func (s *Server) ruleSetReferences() ([]proxy.RuleSetReference, error) {
	file, err := os.Open(filepath.Join(s.dataDir, "rule-sets.json"))
	if err != nil {
		return nil, err
	}
	defer file.Close()
	var refs []proxy.RuleSetReference
	decoder := json.NewDecoder(io.LimitReader(file, 32<<10))
	decoder.DisallowUnknownFields()
	if err = decoder.Decode(&refs); err != nil {
		return nil, err
	}
	if len(refs) < 2 {
		return nil, errors.New("CN sets missing")
	}
	for _, ref := range refs {
		if err = proxy.VerifyRuleSet(ref); err != nil {
			return nil, err
		}
	}
	return refs, nil
}

// A new import always invalidates measured node results, including credential-
// only changes. Random tokens contain no subscription text or credential hash.
func freshNodeRevision() string {
	var token [16]byte
	if _, err := rand.Read(token[:]); err != nil {
		return ""
	}
	return hex.EncodeToString(token[:])
}

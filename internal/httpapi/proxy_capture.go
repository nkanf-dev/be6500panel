package httpapi

import (
	"encoding/json"
	"net/http"
	"os"
	"path/filepath"

	"be6500panel/internal/proxy"
	managedruntime "be6500panel/internal/runtime"
)

func (s *Server) proxyCapture(w http.ResponseWriter, r *http.Request) {
	if s.capture == nil {
		fail(w, 503, "capture_unavailable", "透明接管未启用")
		return
	}
	if r.Method == http.MethodGet {
		status, err := s.capture.Reconcile(r.Context())
		if err != nil {
			s.logger.Warn("Capture state needs recovery", "code", "capture_reconcile_failed", "module", "proxy")
		}
		writeJSON(w, 200, status)
		return
	}
	if r.Method == http.MethodDelete {
		if err := s.capture.Cleanup(r.Context()); err != nil {
			fail(w, 500, "cleanup_failed", "接管规则撤回失败")
			return
		}
		writeJSON(w, 200, s.capture.Status())
		return
	}
	if !s.runtimeEnabled(w) {
		return
	}
	state, err := s.runtime.Status(managedruntime.SingBox)
	if err != nil || state.State != managedruntime.Running {
		fail(w, 409, "proxy_not_running", "先启动代理服务")
		return
	}
	var input struct {
		ClientIPv4 string         `json:"clientIPv4"`
		ClientIPv6 string         `json:"clientIPv6"`
		IPv6       proxy.IPv6Mode `json:"ipv6"`
	}
	if !decodeJSON(w, r, &input, "clientIPv4", "ipv6") {
		return
	}
	raw, err := os.ReadFile(filepath.Join(s.dataDir, "proxy-selection.json"))
	if err != nil {
		fail(w, 409, "proxy_not_selected", "先 Commit 节点配置")
		return
	}
	var selected struct {
		IPv6      proxy.IPv6Mode `json:"ipv6"`
		Ports     proxy.Ports    `json:"ports"`
		Endpoints []string       `json:"endpoints"`
	}
	if json.Unmarshal(raw, &selected) != nil || selected.IPv6 != input.IPv6 {
		fail(w, 409, "policy_mismatch", "接管策略与已提交配置不一致")
		return
	}
	inputPlan := proxy.RulesPlanInput{ClientIPv4: input.ClientIPv4, ClientIPv6: input.ClientIPv6, LANInterface: "br-lan", Ports: selected.Ports, IPv6: input.IPv6, Failure: proxy.FailureDirect, EndpointIPs: selected.Endpoints, ManagementIPs: []string{"192.168.31.1"}, RouterDNSAddresses: []string{"192.168.31.1"}}
	status, err := s.capture.Apply(r.Context(), inputPlan)
	if err != nil {
		s.logger.Warn("Capture activation failed: "+err.Error(), "code", "capture_failed", "module", "proxy")
		fail(w, 409, "capture_failed", "接管应用失败，查看日志与规则状态")
		return
	}
	s.logger.Info("Single-client capture active", "code", "capture_active", "module", "proxy")
	writeJSON(w, 200, status)
}

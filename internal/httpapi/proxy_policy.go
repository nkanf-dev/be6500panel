package httpapi

import (
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"sort"

	"be6500panel/internal/proxy"
)

type proxyPolicyOmission struct {
	Index   int    `json:"index"`
	Code    string `json:"code"`
	Message string `json:"message"`
}

type proxyPolicyReason struct {
	Code    string `json:"code"`
	Count   int    `json:"count"`
	Message string `json:"message"`
}

// proxyPolicySummary describes compilation eligibility, not runtime health or
// rule hits. Each source rule contributes exactly once, even when validation
// produces more than one diagnostic for its index.
type proxyPolicySummary struct {
	Total        int                   `json:"total"`
	Supported    int                   `json:"supported"`
	Omitted      int                   `json:"omitted"`
	Reasons      []proxyPolicyReason   `json:"reasons"`
	OmittedRules []proxyPolicyOmission `json:"omittedRules"`
	Revision     string                `json:"revision"`
}

func summarizeProxyPolicy(sub proxy.Subscription) proxyPolicySummary {
	summary := proxyPolicySummary{Reasons: []proxyPolicyReason{}, OmittedRules: []proxyPolicyOmission{}}
	omissions := map[int]proxyPolicyOmission{}
	indices := map[int]bool{}
	for _, diagnostic := range sub.Diagnostics {
		if diagnostic.Scope != "rule" {
			continue
		}
		indices[diagnostic.Index] = true
		omission := proxyPolicyOmission{Index: diagnostic.Index, Code: diagnostic.Code, Message: proxyPolicyReasonMessage(diagnostic.Code)}
		previous, exists := omissions[diagnostic.Index]
		if !exists || omission.Code < previous.Code {
			omissions[diagnostic.Index] = omission
		}
	}
	terminal := false
	for _, rule := range sub.Rules {
		indices[rule.Index] = true
		if terminal {
			if _, exists := omissions[rule.Index]; !exists {
				omissions[rule.Index] = proxyPolicyOmission{Index: rule.Index, Code: "unreachable-rule", Message: proxyPolicyReasonMessage("unreachable-rule")}
			}
		}
		if rule.Kind == proxy.RuleMatch {
			terminal = true
		}
	}
	for _, omission := range omissions {
		summary.OmittedRules = append(summary.OmittedRules, omission)
	}
	sort.Slice(summary.OmittedRules, func(i, j int) bool { return summary.OmittedRules[i].Index < summary.OmittedRules[j].Index })
	counts := map[string]int{}
	for _, omission := range summary.OmittedRules {
		counts[omission.Code]++
	}
	codes := make([]string, 0, len(counts))
	for code := range counts {
		codes = append(codes, code)
	}
	sort.Strings(codes)
	for _, code := range codes {
		summary.Reasons = append(summary.Reasons, proxyPolicyReason{Code: code, Count: counts[code], Message: proxyPolicyReasonMessage(code)})
	}
	summary.Total = len(indices)
	summary.Omitted = len(summary.OmittedRules)
	summary.Supported = summary.Total - summary.Omitted
	// Hash only normalized policy and fixed omission semantics. Node IDs,
	// endpoint credentials, original YAML, and diagnostic text are excluded.
	// Replacing a node alone preserves review; routing/DNS/selector semantics
	// change the revision. The same parsed policy remains stable after restart.
	type omittedIdentity struct {
		Index int    `json:"index"`
		Code  string `json:"code"`
	}
	omittedIdentities := make([]omittedIdentity, 0, len(summary.OmittedRules))
	for _, omission := range summary.OmittedRules {
		omittedIdentities = append(omittedIdentities, omittedIdentity{omission.Index, omission.Code})
	}
	semantics := struct {
		Rules              []proxy.Rule      `json:"rules"`
		Omissions          []omittedIdentity `json:"omissions"`
		FakeIP             bool              `json:"fakeIP"`
		SelectedNodePolicy bool              `json:"selectedNodePolicy"`
	}{sub.Rules, omittedIdentities, sub.FakeIP, sub.GroupCount > 0}
	raw, _ := json.Marshal(semantics)
	summary.Revision = fmt.Sprintf("%x", sha256.Sum256(raw))
	return summary
}

func proxyPolicyReasonMessage(code string) string {
	switch code {
	case "unsupported-process-rule":
		return "路由器网关不能识别 LAN 客户端的应用进程；进程规则不会参与分流"
	case "unsupported-rule":
		return "规则类型或地区规则集不受当前原生编译器支持"
	case "invalid-rule":
		return "规则参数无效，无法生成原生路由规则"
	case "unknown-rule-target":
		return "规则目标不在当前节点或选择组中"
	case "unreachable-rule":
		return "规则位于终止 MATCH 规则之后，不会参与分流"
	default:
		return "规则无法生成原生路由规则"
	}
}

func checkProxyPolicyAcknowledgment(summary proxyPolicySummary, revision string) (string, string) {
	if revision != "" && revision != summary.Revision {
		return "policy_revision_changed", "订阅分流策略已变更，请刷新并重新核对规则遗漏"
	}
	if summary.Omitted > 0 && revision == "" {
		return "policy_acknowledgment_required", "存在不会参与分流的规则，请核对遗漏并明确确认后再保存节点配置"
	}
	return "", ""
}

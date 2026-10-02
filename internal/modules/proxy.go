package modules

import (
	"be6500panel/internal/core"
	"fmt"
)

type Proxy struct{}
type ProxyInput struct {
	Mode          string `json:"mode"`
	DNSStrategy   string `json:"dnsStrategy"`
	IPv6Policy    string `json:"ipv6Policy"`
	FailurePolicy string `json:"failurePolicy"`
	NodeCount     int    `json:"nodeCount"`
}

func (Proxy) Descriptor() core.Module {
	return core.Module{ID: "proxy", Title: "Proxy", Description: "Credential-free policy validation and coordinated planning.", State: "ready", Capabilities: []core.Capability{{ID: "plan", Title: "Policy plan", Supported: true}, {ID: "apply", Title: "Run proxy", Supported: false, Reason: "Proxy runtime and network capture are not integrated."}}}
}
func (Proxy) Plan(input ProxyInput, c *core.Coordinator) (core.OperationPlan, error) {
	if !oneOf(input.Mode, "split", "global", "direct") {
		return core.OperationPlan{}, fmt.Errorf("mode must be split, global or direct")
	}
	if !oneOf(input.DNSStrategy, "split", "direct") {
		return core.OperationPlan{}, fmt.Errorf("dnsStrategy must be split or direct")
	}
	if !oneOf(input.IPv6Policy, "follow", "direct", "block") {
		return core.OperationPlan{}, fmt.Errorf("ipv6Policy must be follow, direct or block")
	}
	if !oneOf(input.FailurePolicy, "direct", "block-proxy") {
		return core.OperationPlan{}, fmt.Errorf("failurePolicy must be direct or block-proxy")
	}
	if input.NodeCount < 0 || input.NodeCount > 4096 || (input.Mode != "direct" && input.NodeCount == 0) {
		return core.OperationPlan{}, fmt.Errorf("nodeCount must be 0..4096; split/global require at least one node")
	}
	steps := []core.PlanStep{
		{Module: "network", Action: "exclude", Detail: "Keep management addresses, LAN destinations and configured node endpoints outside capture."},
		{Module: "dns", Action: "plan", Detail: "Coordinate resolver path with dnsStrategy=" + input.DNSStrategy + "; avoid resolver/capture loops."},
		{Module: "network", Action: "plan", Detail: "Plan IPv6 handling with ipv6Policy=" + input.IPv6Policy + "; network retains ownership of interfaces and routes."},
		{Module: "firewall", Action: "plan", Detail: "Request capture/forwarding contributions with failurePolicy=" + input.FailurePolicy + "; firewall owns chains and marks."},
		{Module: "proxy", Action: "validate", Detail: fmt.Sprintf("Validate %s policy for %d credential-free node placeholders; no runtime starts.", input.Mode, input.NodeCount)},
	}
	warnings := []string{"IP alone cannot reliably classify domestic/foreign destinations; explicit overrides and maintained domain/IP policies are required.", "Management, LAN and node-endpoint exclusions are plan requirements, not installed rules.", "CPU/memory/throughput costs are unmeasured; no proxy runtime or network changes are performed."}
	if input.IPv6Policy == "follow" {
		warnings = append(warnings, "Following policy on IPv6 requires a verified capture and DNS path before future apply.")
	}
	if input.FailurePolicy == "direct" {
		warnings = append(warnings, "Direct fallback can send traffic outside the proxy if the runtime fails.")
	}
	return c.Plan(fmt.Sprintf("Read-only %s proxy plan (%d nodes)", input.Mode, input.NodeCount), steps, warnings)
}
func oneOf(value string, allowed ...string) bool {
	for _, item := range allowed {
		if value == item {
			return true
		}
	}
	return false
}

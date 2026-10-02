package modules

import (
	"fmt"
	"net"
	"strings"
	"unicode"

	"be6500panel/internal/core"
)

type FRPC struct{}
type FRPCProxy struct {
	Name         string   `json:"name"`
	Type         string   `json:"type"`
	LocalAddress string   `json:"localAddress"`
	LocalPort    int      `json:"localPort"`
	RemotePort   *int     `json:"remotePort,omitempty"`
	Domains      []string `json:"domains,omitempty"`
}
type FRPCInput struct {
	ServerAddress string      `json:"serverAddress"`
	ServerPort    int         `json:"serverPort"`
	TLS           bool        `json:"tls"`
	Transport     string      `json:"transport"`
	Proxies       []FRPCProxy `json:"proxies"`
}

func (FRPC) Descriptor() core.Module {
	return core.Module{ID: "frpc", Title: "frpc", Description: "Reverse-tunnel validation and exposure planning.", State: "ready", Capabilities: []core.Capability{{ID: "plan", Title: "Tunnel plan", Supported: true}, {ID: "observe", Title: "Runtime observation", Supported: false, Reason: FRPCReason}, {ID: "apply", Title: "Run tunnel", Supported: false, Reason: "frpc runtime is not integrated."}}}
}
func (FRPC) Plan(input FRPCInput, c *core.Coordinator) (core.OperationPlan, error) {
	if !validHost(input.ServerAddress) {
		return core.OperationPlan{}, fmt.Errorf("serverAddress must be an IP address or DNS host, without scheme, credentials or port")
	}
	if !validPort(input.ServerPort) {
		return core.OperationPlan{}, fmt.Errorf("serverPort must be 1..65535")
	}
	if !oneOf(input.Transport, "tcp", "quic") {
		return core.OperationPlan{}, fmt.Errorf("transport must be tcp or quic")
	}
	if len(input.Proxies) < 1 || len(input.Proxies) > 64 {
		return core.OperationPlan{}, fmt.Errorf("proxies must contain 1..64 entries")
	}
	names := map[string]bool{}
	for _, proxy := range input.Proxies {
		if !validName(proxy.Name) || names[proxy.Name] {
			return core.OperationPlan{}, fmt.Errorf("proxy names must be unique, 1..64 ASCII letters/digits/dot/underscore/hyphen")
		}
		names[proxy.Name] = true
		if !oneOf(proxy.Type, "tcp", "udp", "http", "https") {
			return core.OperationPlan{}, fmt.Errorf("proxy type must be tcp, udp, http or https")
		}
		if !validHost(proxy.LocalAddress) || !validPort(proxy.LocalPort) {
			return core.OperationPlan{}, fmt.Errorf("each proxy requires a valid localAddress and localPort 1..65535")
		}
		if proxy.Type == "tcp" || proxy.Type == "udp" {
			if proxy.RemotePort == nil || !validPort(*proxy.RemotePort) {
				return core.OperationPlan{}, fmt.Errorf("tcp/udp proxies require remotePort 1..65535")
			}
			if len(proxy.Domains) > 0 {
				return core.OperationPlan{}, fmt.Errorf("domains are only valid for http/https proxies")
			}
		} else {
			if proxy.RemotePort != nil {
				return core.OperationPlan{}, fmt.Errorf("remotePort is only valid for tcp/udp proxies")
			}
			if len(proxy.Domains) < 1 || len(proxy.Domains) > 64 {
				return core.OperationPlan{}, fmt.Errorf("http/https proxies require 1..64 domains")
			}
			seen := map[string]bool{}
			for _, domain := range proxy.Domains {
				key := strings.ToLower(domain)
				if !validDNS(domain) || seen[key] {
					return core.OperationPlan{}, fmt.Errorf("domains must be valid unique DNS names, without schemes or paths")
				}
				seen[key] = true
			}
		}
	}
	warnings := []string{"Reverse tunnels can expose local services to the public internet; explicit authorization and access control are required.", "No frpc process, listener, firewall rule or public endpoint is created by this plan.", "Server authentication credentials are intentionally excluded; future credentials must remain private."}
	if !input.TLS {
		warnings = append(warnings, "TLS is disabled in this proposal; verify transport encryption before any future deployment.")
	}
	steps := []core.PlanStep{
		{Module: "frpc", Action: "validate", Detail: fmt.Sprintf("Validate %d %s tunnel definitions; no server connection is made.", len(input.Proxies), input.Transport)},
		{Module: "network", Action: "plan", Detail: "Review server reachability and management exclusions; do not change interfaces or routes."},
		{Module: "firewall", Action: "review", Detail: "Review public exposure and service access controls before any future apply."},
		{Module: "frpc", Action: "review", Detail: "Verify server permissions, authentication, transport security and per-service acceptance before runtime integration."},
	}
	return c.Plan(fmt.Sprintf("Read-only frpc plan (%d tunnels)", len(input.Proxies)), steps, warnings)
}
func validPort(port int) bool    { return port >= 1 && port <= 65535 }
func validHost(host string) bool { return net.ParseIP(host) != nil || validDNS(host) }
func validName(name string) bool {
	if len(name) < 1 || len(name) > 64 {
		return false
	}
	for _, r := range name {
		if r > unicode.MaxASCII || !(r >= 'a' && r <= 'z' || r >= 'A' && r <= 'Z' || r >= '0' && r <= '9' || r == '.' || r == '_' || r == '-') {
			return false
		}
	}
	return true
}
func validDNS(host string) bool {
	if len(host) < 1 || len(host) > 253 {
		return false
	}
	if strings.HasSuffix(host, ".") {
		host = strings.TrimSuffix(host, ".")
	}
	for _, label := range strings.Split(host, ".") {
		if len(label) < 1 || len(label) > 63 || label[0] == '-' || label[len(label)-1] == '-' {
			return false
		}
		for _, r := range label {
			if !(r >= 'a' && r <= 'z' || r >= 'A' && r <= 'Z' || r >= '0' && r <= '9' || r == '-') {
				return false
			}
		}
	}
	return true
}

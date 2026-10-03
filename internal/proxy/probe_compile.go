package proxy

import (
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"strconv"
)

const MaxProbeNodes = 256

// ProbeCompileInput is private. A separate local HTTP proxy user selects each
// node, never the active core's selector. No router ingress or policy is copied.
type ProbeCompileInput struct {
	Nodes         []Node `json:"-"`
	ListenAddress string `json:"-"`
	Password      string `json:"-"`
}

func (ProbeCompileInput) String() string     { return "private isolated probe compiler input" }
func (i ProbeCompileInput) GoString() string { return i.String() }

func ProbeTag(index int) string { return "probe-" + strconv.Itoa(index) }

// CompileProbe emits a single RAM-bounded isolated sing-box configuration.
// auth_user is the mixed inbound's authenticated username, not a subscription
// selector. Every admitted user routes to exactly one credential-preserving node.
func CompileProbe(in ProbeCompileInput) ([]byte, error) {
	host, portText, err := net.SplitHostPort(in.ListenAddress)
	port, parseErr := strconv.ParseUint(portText, 10, 16)
	if err != nil || parseErr != nil || host != "127.0.0.1" || port == 0 || len(in.Password) < 32 || len(in.Password) > 128 || len(in.Nodes) == 0 || len(in.Nodes) > MaxProbeNodes {
		return nil, errors.New("invalid isolated probe input")
	}
	resolver := map[string]any{"server": "probe-dns", "timeout": "3s", "strategy": "prefer_ipv4"}
	users := make([]map[string]any, 0, len(in.Nodes))
	outbounds := make([]map[string]any, 0, len(in.Nodes)+1)
	rules := make([]map[string]any, 0, len(in.Nodes)+1)
	seen := map[string]bool{}
	for i, n := range in.Nodes {
		if n.ID == "" || len(n.ID) > 128 || seen[n.ID] || validateNode(n) != nil {
			return nil, errors.New("invalid isolated probe node")
		}
		seen[n.ID] = true
		tag := ProbeTag(i)
		users = append(users, map[string]any{"username": tag, "password": in.Password})
		rules = append(rules, map[string]any{"auth_user": []string{tag}, "outbound": tag})
		outbounds = append(outbounds, map[string]any{"type": "vless", "tag": tag, "server": n.Server, "server_port": n.Port, "uuid": n.UUID, "flow": n.Flow, "packet_encoding": "xudp", "domain_resolver": resolver, "connect_timeout": "3s", "tcp_fast_open": true, "tls": map[string]any{"enabled": true, "server_name": n.ServerName, "utls": map[string]any{"enabled": true, "fingerprint": n.Fingerprint}, "reality": map[string]any{"enabled": true, "public_key": n.RealityPublicKey, "short_id": n.RealityShortID}}})
	}
	// The direct outbound is only for authenticated DNS bootstrap. Unknown users
	// are rejected, so a missing node route cannot silently measure direct access.
	outbounds = append(outbounds, map[string]any{"type": "direct", "tag": "probe-bootstrap", "domain_resolver": resolver})
	rules = append(rules, map[string]any{"action": "reject"})
	config := map[string]any{
		"log":       map[string]any{"disabled": true},
		"dns":       map[string]any{"servers": []map[string]any{dnsServer("probe-dns", DNSEndpoint{Server: "223.5.5.5", Port: 853, ServerName: "dns.alidns.com"}, "probe-bootstrap")}, "final": "probe-dns", "cache_capacity": 256, "timeout": "3s", "strategy": "prefer_ipv4"},
		"inbounds":  []map[string]any{{"type": "mixed", "tag": "probe-in", "listen": "127.0.0.1", "listen_port": uint16(port), "users": users}},
		"outbounds": outbounds,
		"route":     map[string]any{"rules": rules, "auto_detect_interface": true, "default_domain_resolver": resolver},
	}
	data, err := json.Marshal(config)
	if err != nil {
		return nil, fmt.Errorf("cannot encode isolated probe configuration")
	}
	return append(data, '\n'), nil
}

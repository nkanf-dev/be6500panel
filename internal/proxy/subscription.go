package proxy

import (
	"bytes"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"fmt"
	"io"
	"net/netip"
	"regexp"
	"strconv"
	"strings"

	"gopkg.in/yaml.v3"
)

var uuidPattern = regexp.MustCompile(`^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$`)

// ParseClashYAML reads at most MaxSubscriptionBytes+1 bytes. It imports only
// native VLESS/TCP/REALITY/Vision/Chrome-uTLS nodes and known routing rules.
// Invalid/unsupported entries become indexed, redacted diagnostics. A compiler
// must explicitly acknowledge ignored rules rather than silently activating.
func ParseClashYAML(r io.Reader) (Subscription, error) {
	var out Subscription
	if r == nil {
		return out, fmt.Errorf("subscription reader is required")
	}
	data, err := io.ReadAll(io.LimitReader(r, MaxSubscriptionBytes+1))
	if err != nil {
		return out, fmt.Errorf("cannot read subscription")
	}
	if len(data) > MaxSubscriptionBytes {
		return out, fmt.Errorf("subscription exceeds %d bytes", MaxSubscriptionBytes)
	}
	if err = preflightYAML(data); err != nil {
		return out, err
	}
	dec := yaml.NewDecoder(bytes.NewReader(data))
	var doc yaml.Node
	if err = dec.Decode(&doc); err != nil {
		return out, fmt.Errorf("invalid subscription YAML")
	}
	var extra yaml.Node
	if err = dec.Decode(&extra); err != io.EOF {
		return out, fmt.Errorf("subscription must contain one YAML document")
	}
	count := 0
	if err = validateYAML(&doc, 0, &count); err != nil {
		return out, err
	}
	if len(doc.Content) != 1 || doc.Content[0].Kind != yaml.MappingNode {
		return out, fmt.Errorf("subscription must be a mapping")
	}
	root := doc.Content[0]
	proxies := field(root, "proxies")
	if proxies == nil || proxies.Kind != yaml.SequenceNode {
		return out, fmt.Errorf("subscription proxies must be a sequence")
	}
	if len(proxies.Content) > MaxNodes {
		return out, fmt.Errorf("subscription exceeds %d nodes", MaxNodes)
	}
	names := map[string]bool{}
	for i, item := range proxies.Content {
		n, err := parseNode(item)
		if err != nil {
			out.Diagnostics = append(out.Diagnostics, Diagnostic{"node", i, "unsupported-node", err.Error()})
			continue
		}
		if names[n.Name] {
			out.Diagnostics = append(out.Diagnostics, Diagnostic{"node", i, "duplicate-node", "duplicate node name"})
			continue
		}
		names[n.Name] = true
		// Stable IDs identify the private node without revealing its original label.
		h := sha256.Sum256([]byte(n.Server + ":" + strconv.Itoa(int(n.Port)) + "\x00" + n.UUID + "\x00" + n.Name))
		n.ID = hex.EncodeToString(h[:8])
		out.Nodes = append(out.Nodes, n)
	}
	groups := field(root, "proxy-groups")
	if groups != nil {
		if groups.Kind != yaml.SequenceNode || len(groups.Content) > 128 {
			return out, fmt.Errorf("invalid or excessive selector groups")
		}
		for _, group := range groups.Content {
			name := scalar(field(group, "name"))
			typ := scalar(field(group, "type"))
			if name == "" || len(name) > 256 {
				return out, fmt.Errorf("invalid selector group name")
			}
			names[name] = true
			out.GroupCount++
			if typ != "select" && typ != "url-test" && typ != "fallback" && typ != "load-balance" {
				out.Diagnostics = append(out.Diagnostics, Diagnostic{"group", out.GroupCount - 1, "unsupported-group", "group is not a known selector"})
			}
		}
		if out.GroupCount > 0 {
			out.Diagnostics = append(out.Diagnostics, Diagnostic{"subscription", -1, "selected-node-policy", "named selectors collapse to the explicitly selected node; no concurrent node probes"})
		}
	}
	rules := field(root, "rules")
	if rules != nil {
		if rules.Kind != yaml.SequenceNode || len(rules.Content) > MaxRules {
			return out, fmt.Errorf("invalid or excessive subscription rules")
		}
		for i, item := range rules.Content {
			if item.Kind != yaml.ScalarNode || item.Tag != "!!str" {
				return out, fmt.Errorf("rule %d must be a string", i)
			}
			rule, code, message := parseRule(item.Value, names)
			rule.Index = i
			if code != "" {
				out.Diagnostics = append(out.Diagnostics, Diagnostic{"rule", i, code, message})
				continue
			}
			out.Rules = append(out.Rules, rule)
		}
	}
	dns := field(root, "dns")
	out.FakeIP = scalar(field(dns, "enhanced-mode")) == "fake-ip"
	if len(out.Nodes) == 0 {
		return out, fmt.Errorf("subscription contains no supported VLESS nodes")
	}
	return out, nil
}

// preflightYAML bounds the work *before* yaml.v3 allocates an AST. The public
// decoder has no streaming event/AST allocation limit. A byte limit alone is
// not sufficient: a 2 MiB flow sequence can otherwise allocate >170 MiB.
//
// Count conservative lexical units, including punctuation inside quoted text
// (overcounting is intentional). Each scalar/collection/null node requires a
// run or delimiter. This conservative budget bounds tree amplification before
// decoding. Collections and indentation have separate admission limits. This is an admission filter, not a second
// YAML parser: yaml.v3 still decides all actual syntax and semantic values.
const maxYAMLLexicalUnits = 65536

func preflightYAML(data []byte) error {
	units, run := 0, 0
	for _, c := range data {
		word := c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' || c == '_'
		if word {
			if run == 0 {
				units++
			}
			run++
			if run > 8192 {
				return fmt.Errorf("subscription YAML scalar admission limit exceeded")
			}
		} else {
			units++
			run = 0
		}
		if units > maxYAMLLexicalUnits {
			return fmt.Errorf("subscription YAML lexical budget exceeded")
		}
	}
	// Syntax-agnostic structural admission avoids relying on quote parsing
	// for a resource limit. Count even harmless brackets in names/comments.
	// Accepted subscription syntax normally uses only a few nesting levels.
	opens := 0
	for _, c := range data {
		if c == '[' || c == '{' {
			opens++
			if opens > 1024 {
				return fmt.Errorf("subscription YAML collection admission limit exceeded")
			}
		}
	}

	indents := []int{0}
	seenContent := false
	for start := 0; start < len(data); {
		end := start
		for end < len(data) && data[end] != '\n' {
			end++
		}
		line := bytes.TrimSuffix(data[start:end], []byte{'\r'})
		start = end + 1
		trimmed := bytes.TrimSpace(line)
		if len(trimmed) == 0 || trimmed[0] == '#' || trimmed[0] == '%' {
			continue
		}
		// Column-zero document markers are rejected before allocating any second
		// AST. Markers inside a quoted/block scalar can be rejected conservatively.
		if bytes.HasPrefix(line, []byte("---")) && (len(line) == 3 || line[3] <= 32 || line[3] == '#') {
			if seenContent {
				return fmt.Errorf("subscription must contain one YAML document")
			}
			continue
		}
		seenContent = true
		indent := len(line) - len(bytes.TrimLeft(line, " "))
		for len(indents) > 1 && indent < indents[len(indents)-1] {
			indents = indents[:len(indents)-1]
		}
		if indent > indents[len(indents)-1] {
			indents = append(indents, indent)
			if len(indents) > 32 {
				return fmt.Errorf("subscription YAML indentation admission limit exceeded")
			}
		}
	}

	return nil
}
func validateYAML(n *yaml.Node, depth int, count *int) error {
	*count++
	if depth > 32 || *count > 100000 {
		return fmt.Errorf("subscription YAML nesting or node limit exceeded")
	}
	if n.Kind == yaml.AliasNode || n.Anchor != "" {
		return fmt.Errorf("subscription YAML aliases and anchors are not supported")
	}
	if n.Kind == yaml.ScalarNode && len(n.Value) > 8192 {
		return fmt.Errorf("subscription YAML scalar limit exceeded")
	}
	if n.Kind == yaml.MappingNode {
		seen := map[string]bool{}
		for i := 0; i < len(n.Content); i += 2 {
			k := n.Content[i]
			if k.Kind != yaml.ScalarNode || k.Tag != "!!str" || k.Value == "<<" || seen[k.Value] {
				return fmt.Errorf("subscription YAML keys must be unique strings")
			}
			seen[k.Value] = true
		}
	}
	for _, child := range n.Content {
		if err := validateYAML(child, depth+1, count); err != nil {
			return err
		}
	}
	return nil
}
func field(n *yaml.Node, key string) *yaml.Node {
	if n == nil || n.Kind != yaml.MappingNode {
		return nil
	}
	for i := 0; i < len(n.Content); i += 2 {
		if n.Content[i].Value == key {
			return n.Content[i+1]
		}
	}
	return nil
}
func scalar(n *yaml.Node) string {
	if n == nil || n.Kind != yaml.ScalarNode {
		return ""
	}
	return n.Value
}
func boolValue(n *yaml.Node) bool { return n != nil && n.Tag == "!!bool" && n.Value == "true" }

func parseNode(item *yaml.Node) (Node, error) {
	var n Node
	if item.Kind != yaml.MappingNode {
		return n, fmt.Errorf("node must be a mapping")
	}
	if scalar(field(item, "type")) != "vless" {
		return n, fmt.Errorf("only VLESS is supported")
	}
	network := scalar(field(item, "network"))
	if network != "" && network != "tcp" {
		return n, fmt.Errorf("only native TCP transport is supported")
	}
	if !boolValue(field(item, "tls")) {
		return n, fmt.Errorf("TLS is required")
	}
	if boolValue(field(item, "skip-cert-verify")) {
		return n, fmt.Errorf("insecure TLS is not supported")
	}
	n.Name = scalar(field(item, "name"))
	n.Server = scalar(field(item, "server"))
	n.UUID = scalar(field(item, "uuid"))
	port, err := strconv.ParseUint(scalar(field(item, "port")), 10, 16)
	if err != nil || port == 0 {
		return n, fmt.Errorf("invalid server port")
	}
	n.Port = uint16(port)
	n.ServerName = scalar(field(item, "servername"))
	if n.ServerName == "" {
		n.ServerName = scalar(field(item, "sni"))
	}
	n.Flow = scalar(field(item, "flow"))
	n.Fingerprint = scalar(field(item, "client-fingerprint"))
	n.UDP = boolValue(field(item, "udp"))
	reality := field(item, "reality-opts")
	n.RealityPublicKey = scalar(field(reality, "public-key"))
	n.RealityShortID = scalar(field(reality, "short-id"))
	if n.Name == "" || len(n.Name) > 256 {
		return n, fmt.Errorf("node name is required and bounded")
	}
	if err = validateNode(n); err != nil {
		return n, err
	}
	// Options that change transport cannot be silently discarded.
	for _, key := range []string{"ws-opts", "grpc-opts", "http-opts", "h2-opts", "smux", "dialer-proxy"} {
		if field(item, key) != nil {
			return n, fmt.Errorf("unsupported transport or multiplex option")
		}
	}
	return n, nil
}
func validateNode(n Node) error {
	if !validHost(n.Server) || n.Port == 0 {
		return fmt.Errorf("invalid server endpoint")
	}
	if !validDomain(n.ServerName) {
		return fmt.Errorf("REALITY certificate DNS identity is required")
	}
	if !uuidPattern.MatchString(n.UUID) {
		return fmt.Errorf("invalid VLESS UUID")
	}
	if n.Flow != "xtls-rprx-vision" {
		return fmt.Errorf("only xtls-rprx-vision flow is supported")
	}
	if n.Fingerprint != "chrome" {
		return fmt.Errorf("only Chrome uTLS fingerprint is supported")
	}
	public, err := base64.RawURLEncoding.DecodeString(n.RealityPublicKey)
	if err != nil || len(public) != 32 {
		return fmt.Errorf("invalid REALITY public key")
	}
	short, err := hex.DecodeString(n.RealityShortID)
	if err != nil || len(short) > 8 {
		return fmt.Errorf("invalid REALITY short ID")
	}
	if !n.UDP {
		return fmt.Errorf("VLESS UDP/XUDP must be enabled")
	}
	return nil
}
func validHost(host string) bool {
	if a, err := netip.ParseAddr(host); err == nil {
		return a.Zone() == "" && !a.IsUnspecified() && !a.IsMulticast()
	}
	return validDomain(host)
}
func validDomain(domain string) bool {
	if len(domain) == 0 || len(domain) > 253 || strings.HasSuffix(domain, ".") {
		return false
	}
	for _, part := range strings.Split(domain, ".") {
		if len(part) == 0 || len(part) > 63 || part[0] == '-' || part[len(part)-1] == '-' {
			return false
		}
		for _, r := range part {
			if (r < 'a' || r > 'z') && (r < 'A' || r > 'Z') && (r < '0' || r > '9') && r != '-' {
				return false
			}
		}
	}
	return true
}
func parseRule(text string, names map[string]bool) (Rule, string, string) {
	var out Rule
	parts := strings.Split(text, ",")
	for i := range parts {
		parts[i] = strings.TrimSpace(parts[i])
	}
	if len(parts) < 2 || len(parts) > 4 {
		return out, "invalid-rule", "invalid rule fields"
	}
	typ := strings.ToUpper(parts[0])
	targetIndex := 2
	switch typ {
	case "DOMAIN":
		out.Kind = RuleDomain
	case "DOMAIN-SUFFIX":
		out.Kind = RuleDomainSuffix
	case "DOMAIN-KEYWORD":
		out.Kind = RuleDomainKeyword
	case "IP-CIDR", "IP-CIDR6":
		out.Kind = RuleIPCIDR
	case "MATCH", "FINAL":
		out.Kind = RuleMatch
		targetIndex = 1
	case "GEOIP":
		if len(parts) >= 3 && strings.EqualFold(parts[1], "CN") {
			out.Kind = RuleSet
			out.Value = "cn-ip"
		} else {
			return out, "unsupported-rule", "only CN GEOIP has a controlled SRS mapping"
		}
	case "GEOSITE":
		if len(parts) >= 3 && strings.EqualFold(parts[1], "CN") {
			out.Kind = RuleSet
			out.Value = "cn-domain"
		} else {
			return out, "unsupported-rule", "only CN GEOSITE has a controlled SRS mapping"
		}
	case "PROCESS-NAME", "PROCESS-PATH":
		return out, "unsupported-process-rule", "process rules cannot classify forwarded LAN clients"
	default:
		return out, "unsupported-rule", "unsupported rule type"
	}
	if len(parts) <= targetIndex {
		return out, "invalid-rule", "missing rule target"
	}
	if len(parts) > targetIndex+2 {
		return out, "invalid-rule", "excess rule fields"
	}
	if len(parts) == targetIndex+2 {
		if parts[targetIndex+1] != "no-resolve" || out.Kind != RuleIPCIDR && out.Value != "cn-ip" {
			return out, "invalid-rule", "unsupported rule option"
		}
		out.NoResolve = true
	}
	target := parts[targetIndex]
	switch strings.ToUpper(target) {
	case "DIRECT":
		out.Target = TargetDirect
	case "REJECT", "REJECT-DROP":
		out.Target = TargetBlock
	case "PROXY":
		out.Target = TargetProxy
	default:
		if names[target] {
			out.Target = TargetProxy
		} else {
			return out, "unknown-rule-target", "rule target is not a known node or selector"
		}
	}
	if out.Kind != RuleMatch && out.Kind != RuleSet {
		out.Value = parts[1]
	}
	if err := validateRule(out); err != nil {
		return Rule{}, "invalid-rule", err.Error()
	}
	return out, "", ""
}
func validateRule(r Rule) error {
	if r.Target != TargetDirect && r.Target != TargetProxy && r.Target != TargetBlock {
		return fmt.Errorf("invalid rule target")
	}
	switch r.Kind {
	case RuleDomain, RuleDomainSuffix:
		if !validDomain(r.Value) {
			return fmt.Errorf("invalid rule domain")
		}
	case RuleDomainKeyword:
		if len(r.Value) == 0 || len(r.Value) > 253 || strings.ContainsAny(r.Value, "\r\n\x00") {
			return fmt.Errorf("invalid rule keyword")
		}
	case RuleIPCIDR:
		p, err := netip.ParsePrefix(r.Value)
		if err != nil || p.Addr().Is4In6() {
			return fmt.Errorf("invalid rule IP prefix")
		}
	case RuleSet:
		if r.Value != "cn-domain" && r.Value != "cn-ip" && r.Value != "proxy-domain" {
			return fmt.Errorf("unknown controlled rule set")
		}
	case RuleMatch:
		if r.Value != "" {
			return fmt.Errorf("MATCH rule must not have a value")
		}
	default:
		return fmt.Errorf("unsupported native rule type")
	}
	return nil
}

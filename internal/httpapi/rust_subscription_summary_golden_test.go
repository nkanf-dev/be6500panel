package httpapi

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"be6500panel/internal/proxy"
)

// This source-only oracle parses synthetic strings. It never reads subscription
// files, starts a core, or accesses router/network state. Ordinary tests skip;
// generation requires RUST_SUBSCRIPTION_GOLDEN_OUTPUT, and read-only comparison
// requires RUST_SUBSCRIPTION_GOLDEN_COMPARE. Set exactly one to an absolute path.
const rustSubscriptionGoldenNode = `  - name: synthetic-node
    type: vless
    server: node.example.test
    port: 443
    uuid: 00000000-1111-4222-8333-444444444444
    network: tcp
    tls: true
    udp: true
    flow: xtls-rprx-vision
    client-fingerprint: chrome
    servername: cert.example.test
    reality-opts:
      public-key: AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE
      short-id: aabbccdd
`

// Node's own JSON deliberately redacts compiler input. This DTO explicitly
// copies every private compiler field from the actual parsed fake-only Node.
type rustSubscriptionGoldenPrivateNode struct {
	ID               string `json:"id"`
	Name             string `json:"name"`
	Server           string `json:"server"`
	Port             uint16 `json:"port"`
	UUID             string `json:"uuid"`
	ServerName       string `json:"serverName"`
	RealityPublicKey string `json:"realityPublicKey"`
	RealityShortID   string `json:"realityShortID"`
	Fingerprint      string `json:"fingerprint"`
	Flow             string `json:"flow"`
	UDP              bool   `json:"udp"`
}

type rustSubscriptionGoldenSubscription struct {
	Nodes       []rustSubscriptionGoldenPrivateNode `json:"nodes"`
	PublicNodes []proxy.PublicNode                  `json:"publicNodes"`
	Rules       []proxy.Rule                        `json:"rules"`
	Diagnostics []proxy.Diagnostic                  `json:"diagnostics"`
	GroupCount  int                                 `json:"groupCount"`
	FakeIP      bool                                `json:"fakeIP"`
}

type rustSubscriptionGoldenCase struct {
	Name          string                              `json:"name"`
	YAML          string                              `json:"yaml"`
	Valid         bool                                `json:"valid"`
	Subscription  *rustSubscriptionGoldenSubscription `json:"subscription,omitempty"`
	PolicySummary *proxyPolicySummary                 `json:"policySummary,omitempty"`
	Error         string                              `json:"error,omitempty"`
}

type rustSubscriptionGoldenInput struct {
	name  string
	yaml  string
	valid bool
}

func rustSubscriptionGoldenInputs() []rustSubscriptionGoldenInput {
	base := "proxies:\n" + rustSubscriptionGoldenNode
	with := func(tail string) string { return base + tail }
	changed := func(old, replacement string) string {
		return strings.Replace(rustSubscriptionGoldenNode, old, replacement, 1)
	}
	nodeFailures := "proxies:\n" + rustSubscriptionGoldenNode
	for i, change := range [][2]string{
		{"type: vless", "type: shadowsocks"},
		{"network: tcp", "network: ws"},
		{"tls: true", "tls: false"},
		{"tls: true", "tls: \"true\""},
		{"tls: true", "tls: True"},
		{"tls: true", "tls: true\n    skip-cert-verify: true"},
		{"udp: true", "udp: false"},
		{"udp: true", "udp: \"true\""},
		{"network: tcp", "network: tcp\n    ws-opts: {}"},
		{"network: tcp", "network: tcp\n    grpc-opts: {}"},
		{"network: tcp", "network: tcp\n    http-opts: {}"},
		{"network: tcp", "network: tcp\n    h2-opts: {}"},
		{"network: tcp", "network: tcp\n    smux: false"},
		{"network: tcp", "network: tcp\n    dialer-proxy: synthetic-selector"},
	} {
		block := changed(change[0], change[1])
		nodeFailures += strings.Replace(block, "name: synthetic-node", fmt.Sprintf("name: unsupported-%d", i), 1)
	}
	nodeValidation := "proxies:\n" + rustSubscriptionGoldenNode
	for i, change := range [][2]string{
		{"server: node.example.test", "server: 0.0.0.0"},
		{"server: node.example.test", "server: node.example.test/path"},
		{"port: 443", "port: 0"},
		{"port: 443", "port: 65536"},
		{"port: 443", "port: 443.0"},
		{"port: 443", "port: 0x1bb"},
		{"00000000-1111-4222-8333-444444444444", "synthetic-invalid-uuid"},
		{"servername: cert.example.test", "servername: cert.example.test."},
		{"flow: xtls-rprx-vision", "flow: synthetic-flow"},
		{"client-fingerprint: chrome", "client-fingerprint: firefox"},
		{"AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE", "synthetic-invalid-key"},
		{"short-id: aabbccdd", "short-id: abc"},
		{"short-id: aabbccdd", "short-id: aabbccddeeff001122"},
	} {
		block := changed(change[0], change[1])
		nodeValidation += strings.Replace(block, "name: synthetic-node", fmt.Sprintf("name: invalid-%d", i), 1)
	}
	nodeValidation += changed("name: synthetic-node", "name: "+strings.Repeat("n", 257))
	nodeValidation += "  - synthetic-non-mapping-node\n"

	return []rustSubscriptionGoldenInput{
		{"supported-ordered", with(`proxy-groups:
  - name: selected
    type: select
    proxies: [synthetic-node]
dns:
  enhanced-mode: fake-ip
rules:
  - DOMAIN,first.example.test,DIRECT
  - DOMAIN-SUFFIX,second.example.test,selected
  - IP-CIDR,203.0.113.0/24,DIRECT,no-resolve
  - PROCESS-NAME,synthetic-process,DIRECT
  - DOMAIN-KEYWORD,third,REJECT
  - GEOIP,CN,DIRECT
  - MATCH,selected
`), true},
		{"nil-rules-and-diagnostics", base, true},
		{"empty-rules-retain-nil", with("rules: []\n"), true},
		{"unicode-labels-id-and-json-escaping", "proxies:\n" + changed("name: synthetic-node", "name: '🇨🇳 合成<&>节点\"'") + strings.Replace(changed("name: synthetic-node", "name: '第二合成节点 é'"), "server: node.example.test", "server: 2001:db8::7", 1) + `proxy-groups:
  - name: 合成选择组
    type: select
    proxies: ['🇨🇳 合成<&>节点"', '第二合成节点 é']
rules:
  - DOMAIN,example.test,合成选择组
  - 'DOMAIN-KEYWORD,<&>合成,DIRECT'
  - "DOMAIN-KEYWORD,line\u2028separator\u2029end,REJECT"
`, true},
		{"duplicate-valid-node-index", "proxies:\n" + rustSubscriptionGoldenNode + strings.Replace(rustSubscriptionGoldenNode, "port: 443", "port: 8443", 1) + "rules:\n  - MATCH,synthetic-node\n", true},
		{"unsupported-nodes-indexed", nodeFailures, true},
		{"invalid-node-parameters-indexed", nodeValidation, true},
		{"direct-uniform-nested-groups", with(`proxy-groups:
  - name: direct-leaf
    type: select
    proxies: [DIRECT, direct]
  - name: direct-parent
    type: fallback
    proxies: [direct-leaf, DIRECT]
rules:
  - DOMAIN,one.example.test,direct-leaf
  - MATCH,direct-parent
`), true},
		{"reject-uniform-nested-groups", with(`proxy-groups:
  - name: reject-leaf
    type: url-test
    proxies: [REJECT, REJECT-DROP]
  - name: reject-parent
    type: load-balance
    proxies: [reject-leaf, reject]
rules:
  - DOMAIN,one.example.test,reject-leaf
  - MATCH,reject-parent
`), true},
		{"mixed-cyclic-unknown-empty-groups", with(`proxy-groups:
  - name: mixed
    type: select
    proxies: [DIRECT, REJECT]
  - name: cycle-a
    type: select
    proxies: [cycle-b]
  - name: cycle-b
    type: select
    proxies: [cycle-a]
  - name: unknown-member
    type: select
    proxies: [synthetic-missing]
  - name: empty
    type: select
    proxies: []
rules:
  - DOMAIN,mixed.example.test,mixed
  - DOMAIN,cycle.example.test,cycle-a
  - DOMAIN,unknown.example.test,unknown-member
  - DOMAIN,empty.example.test,empty
  - MATCH,PROXY
`), true},
		{"unsupported-group-code-and-unknown-target", with(`proxy-groups:
  - name: unsupported
    type: synthetic-selector-type
    proxies: [DIRECT]
rules:
  - DOMAIN,one.example.test,unsupported
  - DOMAIN,two.example.test,synthetic-absent-selector
`), true},
		{"omissions-terminal-and-reason-order", with(`rules:
  - PROCESS-NAME,synthetic-process,DIRECT
  - PROCESS-PATH,/synthetic/process,DIRECT
  - RULE-SET,synthetic-rules,DIRECT
  - GEOIP,US,DIRECT
  - GEOSITE,synthetic-region,DIRECT
  - DOMAIN,invalid/domain,DIRECT
  - IP-CIDR,synthetic-prefix,DIRECT
  - DOMAIN,known.example.test,synthetic-unknown
  - DOMAIN,option.example.test,DIRECT,no-resolve
  - DOMAIN,excess.example.test,DIRECT,synthetic-option,synthetic-extra
  - DOMAIN
  - MATCH,DIRECT
  - DOMAIN,unreachable.example.test,PROXY
  - PROCESS-NAME,synthetic-unreachable-process,DIRECT
  - FINAL,REJECT
`), true},
		{"known-rules-normalization-and-no-resolve", with(`rules:
  - ' domain , exact.example.test , direct '
  - DOMAIN-SUFFIX,suffix.example.test,PROXY
  - DOMAIN-KEYWORD,keyword,REJECT-DROP
  - IP-CIDR,203.0.113.5/24,DIRECT,no-resolve
  - IP-CIDR6,2001:db8::7/32,DIRECT,no-resolve
  - GEOIP,cn,DIRECT,no-resolve
  - GEOSITE,cN,PROXY
  - FINAL,synthetic-node
`), true},
		{"fake-ip-without-rules", with("dns:\n  enhanced-mode: fake-ip\n"), true},
		{"unsupported-dns-and-ignored-options-bounded", with(`dns:
  enhanced-mode: synthetic-dns-mode
  nameserver: [https://dns.example.test/query, tls://203.0.113.53]
  fake-ip-filter: ['*.example.test']
  nested-ignored: {enabled: true, number: 7, values: [null, false, 'scalar']}
profile:
  store-selected: true
unrecognized-option: {quoted: 'synthetic-only', list: [a, b, c]}
rules:
  - MATCH,DIRECT
`), true},
		{"quoted-scalars-port-and-sni-fallback", "proxies:\n" + strings.Replace(strings.Replace(strings.Replace(strings.Replace(rustSubscriptionGoldenNode, "port: 443", "port: '00443'", 1), "servername: cert.example.test", "sni: 'cert.example.test'", 1), "short-id: aabbccdd", "short-id: ''", 1), "type: vless", "type: 'vless'", 1) + "rules:\n  - 'MATCH,PROXY'\n", true},
		{"maximum-label-and-short-id", "proxies:\n" + strings.Replace(changed("name: synthetic-node", "name: "+strings.Repeat("n", 256)), "short-id: aabbccdd", "short-id: aabbccddeeff0011", 1), true},
		{"no-supported-nodes", "proxies:\n" + changed("udp: true", "udp: false"), false},
		{"empty-proxies", "proxies: []\n", false},
		{"missing-proxies", "rules: []\n", false},
		{"nonsequence-proxies", "proxies: {name: synthetic-node}\n", false},
		{"scalar-integer-rule", with("rules: [123]\n"), false},
		{"nonsequence-rules", with("rules: {type: MATCH}\n"), false},
		{"anchor-rejected", with("extra: &synthetic-anchor [value]\n"), false},
		{"alias-rejected", with("extra: &synthetic-anchor value\nother: *synthetic-anchor\n"), false},
		{"duplicate-key-rejected", with("rules: []\nrules: []\n"), false},
		{"merge-key-rejected", with("extra: {'<<': {synthetic: value}}\n"), false},
		{"nonstring-key-rejected", with("extra: {123: synthetic-value}\n"), false},
		{"multiple-documents-rejected", base + "---\nproxies: []\n", false},
		{"malformed-yaml-rejected", with("extra: [synthetic-value\n"), false},
		{"malformed-unicode-escape-rejected", with("extra: \"\\uD800\"\n"), false},
		{"nonmapping-document", "[synthetic-value]\n", false},
		{"invalid-group-name", with("proxy-groups:\n  - type: select\n    proxies: [DIRECT]\n"), false},
		{"nonsequence-groups", with("proxy-groups: {}\n"), false},
	}
}

func rustSubscriptionGoldenMode(t *testing.T) (output, compare string) {
	t.Helper()
	output = os.Getenv("RUST_SUBSCRIPTION_GOLDEN_OUTPUT")
	compare = os.Getenv("RUST_SUBSCRIPTION_GOLDEN_COMPARE")
	if output == "" && compare == "" {
		t.Skip("source-only synthetic oracle requires explicit output or read-only comparison env")
	}
	if output != "" && compare != "" {
		t.Fatal("set exactly one subscription golden env")
	}
	if output != "" && !filepath.IsAbs(output) || compare != "" && !filepath.IsAbs(compare) {
		t.Fatal("subscription golden env path must be absolute")
	}
	return output, compare
}

func TestRustSubscriptionGolden(t *testing.T) {
	output, compare := rustSubscriptionGoldenMode(t)
	fixture := struct {
		Version int                          `json:"version"`
		Cases   []rustSubscriptionGoldenCase `json:"cases"`
	}{Version: 1, Cases: []rustSubscriptionGoldenCase{}}
	seen := map[string]bool{}
	for _, input := range rustSubscriptionGoldenInputs() {
		if seen[input.name] {
			t.Fatal("duplicate synthetic fixture case name")
		}
		seen[input.name] = true
		entry := rustSubscriptionGoldenCase{Name: input.name, YAML: input.yaml}
		sub, err := proxy.ParseClashYAML(strings.NewReader(input.yaml))
		entry.Valid = err == nil
		if entry.Valid != input.valid {
			t.Fatalf("synthetic fixture %s changed validity", input.name)
		}
		if err != nil {
			entry.Error = err.Error() // ParseClashYAML returns fixed safe messages.
		} else {
			// Do not normalize nil slices: summary revision hashes Go rules:null.
			var nodes []rustSubscriptionGoldenPrivateNode
			for _, n := range sub.Nodes {
				nodes = append(nodes, rustSubscriptionGoldenPrivateNode{
					ID: n.ID, Name: n.Name, Server: n.Server, Port: n.Port,
					UUID: n.UUID, ServerName: n.ServerName,
					RealityPublicKey: n.RealityPublicKey, RealityShortID: n.RealityShortID,
					Fingerprint: n.Fingerprint, Flow: n.Flow, UDP: n.UDP,
				})
			}
			entry.Subscription = &rustSubscriptionGoldenSubscription{
				Nodes: nodes, PublicNodes: sub.PublicNodes(), Rules: sub.Rules,
				Diagnostics: sub.Diagnostics, GroupCount: sub.GroupCount, FakeIP: sub.FakeIP,
			}
			summary := summarizeProxyPolicy(sub)
			entry.PolicySummary = &summary
		}
		fixture.Cases = append(fixture.Cases, entry)
	}
	data, err := json.MarshalIndent(fixture, "", "  ")
	if err != nil {
		t.Fatal("cannot encode synthetic subscription fixture")
	}
	data = append(data, '\n')
	if output != "" {
		if err := os.WriteFile(output, data, 0600); err != nil {
			t.Fatal("cannot write requested synthetic fixture")
		}
		return
	}
	before, err := os.Stat(compare)
	if err != nil {
		t.Fatal("cannot stat comparison fixture")
	}
	want, err := os.ReadFile(compare)
	if err != nil {
		t.Fatal("cannot read comparison fixture")
	}
	if !bytes.Equal(want, data) {
		t.Fatal("synthetic Go subscription fixture differs; regenerate with explicit output env")
	}
	after, err := os.Stat(compare)
	if err != nil || before.Size() != after.Size() || !before.ModTime().Equal(after.ModTime()) {
		t.Fatal("comparison fixture metadata changed during read-only check")
	}
}

// Resource refusals stay in source instead of bloating the checked-in JSON.
// They run only in the same explicit synthetic-oracle mode and never write.
func TestRustSubscriptionGoldenFiniteBounds(t *testing.T) {
	rustSubscriptionGoldenMode(t)
	base := "proxies:\n" + rustSubscriptionGoldenNode
	var nested strings.Builder
	nested.WriteString(base)
	for i := 0; i < 33; i++ {
		nested.WriteString(strings.Repeat(" ", i) + "extra:\n")
	}
	cases := []struct {
		name, yaml, error string
	}{
		{"bytes", strings.Repeat("x", proxy.MaxSubscriptionBytes+1), fmt.Sprintf("subscription exceeds %d bytes", proxy.MaxSubscriptionBytes)},
		{"nodes", "proxies:\n" + strings.Repeat("  - synthetic-item\n", proxy.MaxNodes+1), fmt.Sprintf("subscription exceeds %d nodes", proxy.MaxNodes)},
		{"rules", base + "rules:\n" + strings.Repeat("  - MATCH,DIRECT\n", proxy.MaxRules+1), "invalid or excessive subscription rules"},
		{"groups", base + "proxy-groups:\n" + strings.Repeat("  - {name: synthetic-group, type: select, proxies: [DIRECT]}\n", 129), "invalid or excessive selector groups"},
		{"scalar-admission", base + "extra: " + strings.Repeat("x", 8193) + "\n", "subscription YAML scalar admission limit exceeded"},
		{"scalar-decoded", base + "extra: '" + strings.Repeat("x ", 4097) + "'\n", "subscription YAML scalar limit exceeded"},
		{"lexical", base + "extra: [" + strings.Repeat("0,", 66000) + "0]\n", "subscription YAML lexical budget exceeded"},
		{"collections", base + "extra: " + strings.Repeat("[", 1025) + "0" + strings.Repeat("]", 1025) + "\n", "subscription YAML collection admission limit exceeded"},
		{"depth", base + "extra: " + strings.Repeat("[", 33) + "0" + strings.Repeat("]", 33) + "\n", "subscription YAML nesting or node limit exceeded"},
		{"indentation", nested.String(), "subscription YAML indentation admission limit exceeded"},
		{"invalid-utf8", base + "extra: " + string([]byte{0xff}) + "\n", "subscription YAML must be valid UTF-8"},
	}
	for _, input := range cases {
		t.Run(input.name, func(t *testing.T) {
			_, err := proxy.ParseClashYAML(strings.NewReader(input.yaml))
			if err == nil || err.Error() != input.error {
				t.Fatal("synthetic resource refusal changed")
			}
		})
	}
}

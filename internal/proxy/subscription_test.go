package proxy

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"strings"
	"testing"
)

const syntheticUUID = "00000000-1111-4222-8333-444444444444"
const syntheticKey = "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE"
const syntheticYAML = `proxies:
  - name: synthetic-node
    type: vless
    server: example.com
    port: 443
    uuid: 00000000-1111-4222-8333-444444444444
    network: tcp
    tls: true
    udp: true
    flow: xtls-rprx-vision
    client-fingerprint: chrome
    servername: www.example.com
    reality-opts:
      public-key: AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE
      short-id: aabbccdd
proxy-groups:
  - name: selected
    type: select
    proxies: [synthetic-node]
dns:
  enhanced-mode: fake-ip
rules:
  - DOMAIN,first.example.com,DIRECT
  - DOMAIN-SUFFIX,second.example.com,selected
  - IP-CIDR,203.0.113.0/24,DIRECT,no-resolve
  - PROCESS-NAME,synthetic-process,DIRECT
  - DOMAIN-KEYWORD,third,REJECT
  - GEOIP,CN,DIRECT
  - MATCH,selected
`

func testNode(t *testing.T) Node {
	t.Helper()
	sub, err := ParseClashYAML(strings.NewReader(syntheticYAML))
	if err != nil {
		t.Fatal(err)
	}
	return sub.Nodes[0]
}
func TestParseNativeParametersAndOrderedRules(t *testing.T) {
	sub, err := ParseClashYAML(strings.NewReader(syntheticYAML))
	if err != nil {
		t.Fatal(err)
	}
	if len(sub.Nodes) != 1 || len(sub.Rules) != 6 || sub.GroupCount != 1 || !sub.FakeIP {
		t.Fatalf("unexpected summary: %v", sub)
	}
	n := sub.Nodes[0]
	if n.Name != "synthetic-node" || n.Server != "example.com" || n.Port != 443 || n.UUID != syntheticUUID || n.RealityPublicKey != syntheticKey || n.RealityShortID != "aabbccdd" || n.ServerName != "www.example.com" || n.Flow != "xtls-rprx-vision" || n.Fingerprint != "chrome" || !n.UDP {
		t.Fatal("native parameters were not retained")
	}
	want := []int{0, 1, 2, 4, 5, 6}
	for i, r := range sub.Rules {
		if r.Index != want[i] {
			t.Fatalf("rule order changed: %v", sub.Rules)
		}
	}
	if !sub.Rules[2].NoResolve || sub.Rules[4].Value != "cn-ip" || sub.Rules[1].Target != TargetProxy {
		t.Fatal("rule semantics changed")
	}
	found := false
	for _, d := range sub.Diagnostics {
		if d.Code == "unsupported-process-rule" && d.Index == 3 {
			found = true
		}
	}
	if !found {
		t.Fatal("process rule was silently discarded")
	}
}
func TestPublicNodeProjectionAndFormatting(t *testing.T) {
	sub, _ := ParseClashYAML(strings.NewReader(syntheticYAML))
	n := sub.Nodes[0]
	view := sub.PublicNodes()[0]
	if view.Label != n.Name || view.Server != n.Server || view.Port != n.Port || view.ID == "" {
		t.Fatal("authenticated public node is incomplete")
	}
	for _, v := range []any{sub, n, sub.PublicNodes()} {
		data, err := json.Marshal(v)
		if err != nil {
			t.Fatal(err)
		}
		for _, secret := range []string{n.UUID, n.ServerName, n.RealityPublicKey, n.RealityShortID} {
			if bytes.Contains(data, []byte(secret)) {
				t.Fatal("private parameters leaked through JSON")
			}
		}
	}
	for _, v := range []any{n, sub} {
		for _, format := range []string{"%v", "%+v", "%#v"} {
			s := fmt.Sprintf(format, v)
			if strings.Contains(s, n.UUID) || strings.Contains(s, n.RealityPublicKey) {
				t.Fatal("private parameters leaked through formatting")
			}
		}
	}
}
func TestUnsupportedNodesAndRulesHaveSafeDiagnostics(t *testing.T) {
	bad := strings.Replace(syntheticYAML, "network: tcp", "network: ws", 1)
	if _, err := ParseClashYAML(strings.NewReader(bad)); err == nil {
		t.Fatal("unsupported node accepted")
	}
	text := strings.Replace(syntheticYAML, "- MATCH,selected", "- RULE-SET,secret-user-token,DIRECT\n  - IP-CIDR,bad-token,DIRECT\n  - DOMAIN,example.net,unknown-secret\n  - MATCH,selected", 1)
	sub, err := ParseClashYAML(strings.NewReader(text))
	if err != nil {
		t.Fatal(err)
	}
	data, _ := json.Marshal(sub.Diagnostics)
	for _, secret := range []string{"secret-user-token", "bad-token", "unknown-secret", "synthetic-process"} {
		if bytes.Contains(data, []byte(secret)) {
			t.Fatal("source text leaked through diagnostics")
		}
	}
	if len(sub.Diagnostics) != 5 {
		t.Fatalf("missing unsupported diagnostics: %v", sub.Diagnostics)
	}
}
func TestNodeValidation(t *testing.T) {
	changes := []struct{ old, new string }{
		{"port: 443", "port: 65536"}, {syntheticUUID, "private-invalid-token"},
		{syntheticKey, "private-invalid-key"}, {"short-id: aabbccdd", "short-id: abc"},
		{"flow: xtls-rprx-vision", "flow: unsupported"}, {"client-fingerprint: chrome", "client-fingerprint: firefox"},
		{"tls: true", "tls: false"}, {"udp: true", "udp: false"}, {"server: example.com", "server: example.com/path"},
		{"servername: www.example.com", "servername: private invalid"}, {"network: tcp", "network: tcp\n    ws-opts: {}"},
	}
	for _, change := range changes {
		t.Run(change.new, func(t *testing.T) {
			_, err := ParseClashYAML(strings.NewReader(strings.Replace(syntheticYAML, change.old, change.new, 1)))
			if err == nil {
				t.Fatal("invalid node accepted")
			}
			if strings.Contains(err.Error(), "private") {
				t.Fatal("private invalid value leaked")
			}
		})
	}
}
func TestBoundedYAMLAndAmbiguity(t *testing.T) {
	cases := []string{
		strings.Repeat("x", MaxSubscriptionBytes+1), syntheticYAML + "\n---\nproxies: []",
		"proxies: &private []", "proxies: []\nproxies: []",
		"proxies: []\nextra: " + strings.Repeat("[", 1025) + "0" + strings.Repeat("]", 1025),
		"proxies: []\nextra: " + strings.Repeat("a", 8193),
		"proxies: [{name: a}]\nrules: [123]",
	}
	for i, s := range cases {
		t.Run(fmt.Sprint(i), func(t *testing.T) {
			if _, err := ParseClashYAML(strings.NewReader(s)); err == nil {
				t.Fatal("ambiguous or excessive YAML accepted")
			}
		})
	}
	if _, err := ParseClashYAML(nil); err == nil {
		t.Fatal("nil reader accepted")
	}
	if _, err := ParseClashYAML(errorReader{}); err == nil || strings.Contains(err.Error(), "private") {
		t.Fatal("reader error unsafe")
	}
}

type errorReader struct{}

func (errorReader) Read([]byte) (int, error) { return 0, errors.New("private subscription URL") }
func TestSyntheticProductionSizedSubscription(t *testing.T) {
	var b strings.Builder
	b.WriteString("proxies:\n")
	nodeBlock := syntheticYAML[strings.Index(syntheticYAML, "  - name:"):strings.Index(syntheticYAML, "proxy-groups:")]
	for i := 0; i < 170; i++ {
		b.WriteString(strings.Replace(nodeBlock, "synthetic-node", fmt.Sprintf("synthetic-%d", i), 1))
	}
	b.WriteString("proxy-groups:\n")
	for i := 0; i < 10; i++ {
		fmt.Fprintf(&b, "  - name: selector-%d\n    type: select\n    proxies: [synthetic-0]\n", i)
	}
	b.WriteString("rules:\n")
	for i := 0; i < 1020; i++ {
		fmt.Fprintf(&b, "  - DOMAIN-SUFFIX,n%d.example.com,selector-0\n", i)
	}
	sub, err := ParseClashYAML(strings.NewReader(b.String()))
	if err != nil {
		t.Fatal(err)
	}
	if len(sub.Nodes) != 170 || sub.GroupCount != 10 || len(sub.Rules) != 1020 {
		t.Fatalf("unexpected synthetic scale: %v", sub)
	}
}
func TestKnownRules(t *testing.T) {
	for _, s := range []string{"DOMAIN,example.com,DIRECT", "DOMAIN-SUFFIX,example.com,PROXY", "DOMAIN-KEYWORD,example,REJECT", "IP-CIDR6,2001:db8::/32,DIRECT,no-resolve", "GEOSITE,CN,DIRECT", "FINAL,PROXY"} {
		if _, code, _ := parseRule(s, nil); code != "" {
			t.Fatalf("known rule rejected: %s", s)
		}
	}
	for _, s := range []string{"DOMAIN,example.com,DIRECT,no-resolve", "MATCH,PROXY,no-resolve", "IP-CIDR,invalid,DIRECT", "DOMAIN,example.com,DIRECT,unknown", "PROCESS-PATH,/synthetic,DIRECT", "GEOIP,US,DIRECT", "GEOSITE,unknown,DIRECT", "DOMAIN,example.com,unknown"} {
		if _, code, _ := parseRule(s, nil); code == "" {
			t.Fatalf("unknown rule accepted: %s", s)
		}
	}
}

func TestSubscriptionReaderDoesNotConsumeBeyondLimit(t *testing.T) {
	r := &countingReader{left: MaxSubscriptionBytes + 1000}
	if _, err := ParseClashYAML(r); err == nil {
		t.Fatal("oversized subscription accepted")
	}
	if r.read != MaxSubscriptionBytes+1 {
		t.Fatalf("consumed %d bytes", r.read)
	}
}

type countingReader struct{ left, read int }

func (r *countingReader) Read(p []byte) (int, error) {
	if r.left == 0 {
		return 0, io.EOF
	}
	n := len(p)
	if n > r.left {
		n = r.left
	}
	for i := 0; i < n; i++ {
		p[i] = 'x'
	}
	r.read += n
	r.left -= n
	return n, nil
}

func TestPreflightRejectsASTAmplificationBeforeDecode(t *testing.T) {
	hostile := []byte("proxies: []\nextra: [" + strings.Repeat("0,", 900000) + "0]\n")
	if len(hostile) > MaxSubscriptionBytes {
		t.Fatal("fixture must stay under byte cap")
	}
	if err := preflightYAML(hostile); err == nil || !strings.Contains(err.Error(), "lexical budget") {
		t.Fatal("hostile AST amplification passed admission")
	}
	if _, err := ParseClashYAML(bytes.NewReader(hostile)); err == nil || !strings.Contains(err.Error(), "lexical budget") {
		t.Fatal("hostile AST reached YAML parser")
	}
	// Collection/null-node punctuation, deeply nested flow, and indentation
	// must be rejected by admission rather than after a full tree was built.
	for _, data := range []string{
		"proxies: []\nextra: [" + strings.Repeat(",", maxYAMLLexicalUnits) + "]",
		"proxies: []\nextra: " + strings.Repeat("[", 1025) + "0" + strings.Repeat("]", 1025),
		"proxies: []\n---\nextra: [" + strings.Repeat("0,", 100) + "0]",
	} {
		if err := preflightYAML([]byte(data)); err == nil {
			t.Fatal("excessive or multi-document YAML passed preflight")
		}
	}
	// Long whitespace sequences do not create a huge split-lines allocation.
	if err := preflightYAML([]byte(strings.Repeat("\n", MaxSubscriptionBytes))); err == nil {
		t.Fatal("whitespace amplification passed lexical admission")
	}
}
func FuzzParseClashYAML(f *testing.F) {
	f.Add([]byte(syntheticYAML))
	f.Add([]byte("proxies: []"))
	f.Add([]byte("proxies: &x [*x]"))
	f.Fuzz(func(t *testing.T, data []byte) {
		if len(data) > MaxSubscriptionBytes+1 {
			t.Skip()
		}
		_, _ = ParseClashYAML(io.LimitReader(bytes.NewReader(data), MaxSubscriptionBytes+1))
	})
}

package proxy

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

func decodeConfig(t *testing.T, out CompileOutput) map[string]any {
	t.Helper()
	var m map[string]any
	if err := json.Unmarshal(out.Config, &m); err != nil {
		t.Fatal(err)
	}
	return m
}
func maps(v any) []map[string]any {
	a := v.([]any)
	out := make([]map[string]any, len(a))
	for i := range a {
		out[i] = a[i].(map[string]any)
	}
	return out
}
func stagedRefs() []RuleSetReference {
	return []RuleSetReference{
		{Tag: "cn-domain", Kind: "domain", Path: "/tmp/be6500-proxy/cn-domain.srs", SHA256: strings.Repeat("01", 32), SourceURL: "https://example.com/cn-domain.srs", MaxBytes: 2 << 20},
		{Tag: "cn-ip", Kind: "ip", Path: "/tmp/be6500-proxy/cn-ip.srs", SHA256: strings.Repeat("02", 32), SourceURL: "https://example.com/cn-ip.srs", MaxBytes: 2 << 20},
		{Tag: "proxy-domain", Kind: "domain", Path: "/tmp/be6500-proxy/proxy-domain.srs", SHA256: strings.Repeat("03", 32), SourceURL: "https://example.com/proxy-domain.srs", MaxBytes: 2 << 20},
	}
}
func TestCompileNativeCurrentKeysAndTransport(t *testing.T) {
	n := testNode(t)
	out, err := CompileNative(CompileInput{Node: n})
	if err != nil {
		t.Fatal(err)
	}
	m := decodeConfig(t, out)
	if out.CoreVersion != "1.14.2" || out.Failure != FailureDirect || out.IPv6 != IPv6Direct {
		t.Fatal("defaults or native version changed")
	}
	sum := sha256.Sum256(out.Config)
	if out.SHA256 != hex.EncodeToString(sum[:]) {
		t.Fatal("hash does not cover exact bytes")
	}
	ins := maps(m["inbounds"])
	if len(ins) != 3 {
		t.Fatal("missing listeners")
	}
	for i, want := range []string{"mixed", "tproxy", "direct"} {
		if ins[i]["type"] != want || ins[i]["network"] != nil {
			t.Fatalf("TCP/UDP listener changed: %v", ins[i])
		}
	}
	if ins[0]["listen"] != "127.0.0.1" || ins[2]["listen"] != "127.0.0.1" {
		t.Fatal("explicit defaults must be loopback")
	}
	outs := maps(m["outbounds"])
	proxy := outs[1]
	if proxy["type"] != "vless" || proxy["uuid"] != n.UUID || proxy["server"] != n.Server || proxy["server_port"] != float64(n.Port) || proxy["flow"] != n.Flow || proxy["packet_encoding"] != "xudp" || proxy["network"] != nil || proxy["transport"] != nil {
		t.Fatal("native VLESS TCP/XUDP fields changed")
	}
	tls := proxy["tls"].(map[string]any)
	if tls["enabled"] != true || tls["server_name"] != n.ServerName || tls["utls"].(map[string]any)["fingerprint"] != "chrome" || tls["reality"].(map[string]any)["public_key"] != n.RealityPublicKey {
		t.Fatal("native TLS parameters changed")
	}
	dns := m["dns"].(map[string]any)
	if dns["cache_capacity"] != float64(1024) || dns["timeout"] != "5s" || dns["reverse_mapping"] != true {
		t.Fatal("DNS resources not bounded")
	}
	servers := maps(dns["servers"])
	for _, s := range servers {
		if s["type"] != "tls" || s["address"] != nil || s["address_resolver"] != nil || s["server"] == nil || s["tls"].(map[string]any)["server_name"] == nil {
			t.Fatal("legacy or unauthenticated DNS fields")
		}
	}
	if servers[0]["detour"] != "direct" || servers[1]["detour"] != "proxy" {
		t.Fatal("resolver paths changed")
	}
	route := m["route"].(map[string]any)
	rules := maps(route["rules"])
	if rules[0]["action"] != "hijack-dns" || rules[len(rules)-1]["outbound"] != "proxy" {
		t.Fatal("DNS listener/default routing changed")
	}
	for _, bad := range []string{`"clash_api"`, `"geoip"`, `"geosite"`, `"sniff_override_destination"`, `"store_fakeip"`, `"gvisor"`, `"tun"`, `"fakeip"`, `"domain_strategy"`} {
		if bytes.Contains(out.Config, []byte(bad)) {
			t.Fatalf("obsolete/unbuilt key emitted: %s", bad)
		}
	}
}
func TestCompilerRedactsPublicResults(t *testing.T) {
	n := testNode(t)
	out, err := CompileNative(CompileInput{Node: n, Diagnostics: []Diagnostic{{Scope: "rule", Index: 99, Code: "private-credential", Message: n.UUID}}, AcceptUnsupportedRules: true})
	if err != nil {
		t.Fatal(err)
	}
	raw, err := json.Marshal(out)
	if err != nil {
		t.Fatal(err)
	}
	for _, secret := range []string{n.UUID, n.RealityPublicKey, n.RealityShortID, "private-credential"} {
		if bytes.Contains(raw, []byte(secret)) {
			t.Fatal("compiler output JSON leaks private config")
		}
		for _, format := range []string{"%v", "%+v", "%#v"} {
			if strings.Contains(fmt.Sprintf(format, out), secret) {
				t.Fatal("compiler output formatting leaks private config")
			}
		}
	}
	if !bytes.Contains(out.Config, []byte(n.UUID)) {
		t.Fatal("private config lost required UUID")
	}
}
func TestCompilerRulesPreserveOrderAndBypassPriority(t *testing.T) {
	n := testNode(t)
	rules := []Rule{
		{Kind: RuleDomain, Value: "direct.example.com", Target: TargetDirect, Index: 0},
		{Kind: RuleDomainSuffix, Value: "proxy.example.com", Target: TargetProxy, Index: 1},
		{Kind: RuleIPCIDR, Value: "203.0.113.0/24", Target: TargetBlock, NoResolve: true, Index: 2},
		{Kind: RuleMatch, Target: TargetProxy, Index: 3},
	}
	out, err := CompileNative(CompileInput{Node: n, Rules: rules, Overrides: []Rule{{Kind: RuleDomain, Value: "override.example.com", Target: TargetBlock}}, RuleSets: stagedRefs(), Endpoints: []string{"203.0.113.7"}, ManagementIPs: []string{"192.168.31.1"}})
	if err != nil {
		t.Fatal(err)
	}
	route := decodeConfig(t, out)["route"].(map[string]any)
	nativeRules := maps(route["rules"])
	pos := func(key, value string) int {
		for i, r := range nativeRules {
			a, ok := r[key].([]any)
			if ok {
				for _, s := range a {
					if s == value {
						return i
					}
				}
			}
		}
		return -1
	}
	privatePos := pos("ip_cidr", "192.168.0.0/16")
	bootPos := pos("domain", "example.com")
	managementPos := pos("ip_cidr", "192.168.31.1/32")
	overridePos := pos("domain", "override.example.com")
	directPos := pos("domain", "direct.example.com")
	proxyPos := pos("domain_suffix", "proxy.example.com")
	ipPos := pos("ip_cidr", "203.0.113.0/24")
	cnDomainPos := pos("rule_set", "cn-domain")
	proxyDomainPos := pos("rule_set", "proxy-domain")
	cnIPPos := pos("rule_set", "cn-ip")
	if managementPos < 0 || bootPos < 0 || privatePos < 0 || overridePos <= privatePos || overridePos <= bootPos || directPos <= overridePos || proxyPos <= directPos || ipPos <= proxyPos || cnDomainPos <= ipPos || proxyDomainPos <= cnDomainPos || cnIPPos <= proxyDomainPos {
		t.Fatalf("ordered policy changed: %v", nativeRules)
	}
	if nativeRules[ipPos-1]["action"] == "resolve" {
		t.Fatal("no-resolve CIDR triggers eager resolution")
	}
	if nativeRules[cnIPPos-1]["action"] != "resolve" {
		t.Fatal("unclassified names cannot reach CN-IP fallback")
	}
	if nativeRules[len(nativeRules)-1]["outbound"] != "proxy" {
		t.Fatal("MATCH moved before defaults")
	}
	if bytes.Contains(out.Config, []byte("https://example.com")) || bytes.Contains(out.Config, []byte("download_detour")) {
		t.Fatal("compiler downloads rule sets")
	}
}
func TestFakeIPNativeDNSHasRealNonAddressFallback(t *testing.T) {
	out, err := CompileNative(CompileInput{Node: testNode(t), FakeIP: true, IPv6: IPv6Follow, RuleSets: stagedRefs(), Rules: []Rule{{Kind: RuleDomain, Value: "foreign.example.com", Target: TargetProxy}}})
	if err != nil {
		t.Fatal(err)
	}
	m := decodeConfig(t, out)
	dns := m["dns"].(map[string]any)
	servers := maps(dns["servers"])
	fake := servers[2]
	if fake["type"] != "fakeip" || fake["inet4_range"] != "198.18.0.0/15" || fake["inet6_range"] != "fc00::/18" || dns["fakeip"] != nil {
		t.Fatal("fake-IP transport not native")
	}
	rules := maps(dns["rules"])
	found := false
	for i, r := range rules {
		domains, ok := r["domain"].([]any)
		if !ok || domains[0] != "foreign.example.com" {
			continue
		}
		if r["server"] != "dns-fake" || !reflect.DeepEqual(r["query_type"], []any{"A", "AAAA"}) {
			continue
		}
		if rules[i+1]["server"] != "dns-proxy" || rules[i+1]["query_type"] != nil {
			t.Fatal("TXT/HTTPS/MX queries lose proxy intent")
		}
		found = true
	}
	if !found {
		t.Fatal("missing foreign fake-IP policy")
	}
}
func TestIPv6PolicyAndSeparateListenerBinds(t *testing.T) {
	for _, mode := range []IPv6Mode{IPv6Follow, IPv6Direct, IPv6Block} {
		t.Run(string(mode), func(t *testing.T) {
			out, err := CompileNative(CompileInput{Node: testNode(t), IPv6: mode, FakeIP: true, DNSListenAddress: "::"})
			if err != nil {
				t.Fatal(err)
			}
			m := decodeConfig(t, out)
			ins := maps(m["inbounds"])
			if ins[0]["listen"] != "127.0.0.1" || ins[2]["listen"] != "::" {
				t.Fatal("DNS wildcard widens mixed listener")
			}
			if mode == IPv6Follow && ins[1]["listen"] != "::" {
				t.Fatal("missing dual-stack TPROXY listener")
			}
			if mode != IPv6Follow {
				fake := maps(m["dns"].(map[string]any)["servers"])[2]
				if fake["inet6_range"] != nil {
					t.Fatal("fake IPv6 allocation when not captured")
				}
			}
			if mode == IPv6Block {
				found := false
				for _, r := range maps(m["route"].(map[string]any)["rules"]) {
					if r["ip_version"] == float64(6) && r["action"] == "reject" {
						found = true
					}
				}
				if !found {
					t.Fatal("missing IPv6 block intent")
				}
			}
		})
	}
}
func TestCompilerRejectsInvalidInputs(t *testing.T) {
	n := testNode(t)
	inputs := []CompileInput{
		{Node: n, Failure: FailureBlockProxy}, {Node: n, Failure: "unknown"}, {Node: n, IPv6: "unknown"},
		{Node: n, Ports: Ports{1, 1, 2}}, {Node: n, Ports: Ports{1, 0, 2}},
		{Node: n, ListenAddress: "not-an-ip"}, {Node: n, IPv6: IPv6Follow, TProxyListenAddress: "127.0.0.1"},
		{Node: n, Endpoints: []string{"example.com"}}, {Node: n, ManagementIPs: []string{"fe80::1%eth0"}},
		{Node: n, BootstrapDomains: []string{"https://private-token.example.com"}},
		{Node: n, DirectDNS: DNSEndpoint{Server: "resolver.example.com", Port: 853, ServerName: "example.com"}},
		{Node: n, Rules: []Rule{{Kind: RuleSet, Value: "cn-ip", Target: TargetDirect}}},
		{Node: n, Rules: []Rule{{Kind: RuleDomain, Value: "example.com", Target: "unknown"}}},
		{Node: n, Diagnostics: []Diagnostic{{Scope: "rule", Index: 8, Message: "private-token"}}},
	}
	for i, in := range inputs {
		t.Run(fmt.Sprint(i), func(t *testing.T) {
			out, err := CompileNative(in)
			if err == nil || len(out.Config) != 0 {
				t.Fatal("invalid compiler input accepted")
			}
			if strings.Contains(err.Error(), "private-token") {
				t.Fatal("private invalid input leaked")
			}
		})
	}
}
func TestCompilerDeterministicAndDoesNotMutateInput(t *testing.T) {
	in := CompileInput{Node: testNode(t), RuleSets: stagedRefs(), BootstrapDomains: []string{"b.example.com", "a.example.com", "b.example.com"}, Endpoints: []string{"203.0.113.8", "203.0.113.7"}}
	saved := append([]string{}, in.BootstrapDomains...)
	savedRefs := append([]RuleSetReference{}, in.RuleSets...)
	first, err := CompileNative(in)
	if err != nil {
		t.Fatal(err)
	}
	second, err := CompileNative(in)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(first.Config, second.Config) || first.SHA256 != second.SHA256 {
		t.Fatal("nondeterministic compilation")
	}
	if !reflect.DeepEqual(saved, in.BootstrapDomains) || !reflect.DeepEqual(savedRefs, in.RuleSets) {
		t.Fatal("compiler mutated caller data")
	}
	in.Endpoints = []string{"203.0.113.7", "203.0.113.8", "203.0.113.7"}
	in.BootstrapDomains = []string{"a.example.com", "b.example.com"}
	in.RuleSets[0], in.RuleSets[2] = in.RuleSets[2], in.RuleSets[0]
	third, err := CompileNative(in)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(first.Config, third.Config) {
		t.Fatal("equivalent unordered bypass inputs change configuration")
	}
}
func TestControlledRuleSetValidationAndChecksum(t *testing.T) {
	refs := stagedRefs()
	bad := []RuleSetReference{refs[0], refs[0], refs[0], refs[0], refs[0], refs[0], refs[0], refs[0]}
	bad[0].Tag = "arbitrary"
	bad[1].Kind = "ip"
	bad[2].Path = "relative.srs"
	bad[3].SHA256 = "invalid"
	bad[4].MaxBytes = 9 << 20
	bad[5].SourceURL = "https://private:token@example.com/x.srs"
	bad[6].SourceURL = "http://example.com/x.srs"
	bad[7].SourceURL = "https://example.com/x.srs?token=private"
	for _, ref := range bad {
		if _, err := CompileNative(CompileInput{Node: testNode(t), RuleSets: []RuleSetReference{ref}}); err == nil {
			t.Fatal("uncontrolled rule set accepted")
		}
	}
	if _, err := CompileNative(CompileInput{Node: testNode(t), RuleSets: []RuleSetReference{refs[0], refs[0]}}); err == nil {
		t.Fatal("duplicate rule set accepted")
	}
	path := filepath.Join(t.TempDir(), "cn-domain.srs")
	data := []byte("synthetic SRS bytes for integrity test only")
	if err := os.WriteFile(path, data, 0600); err != nil {
		t.Fatal(err)
	}
	h := sha256.Sum256(data)
	ref := RuleSetReference{Tag: "cn-domain", Kind: "domain", Path: path, SHA256: hex.EncodeToString(h[:]), MaxBytes: 64}
	if err := VerifyRuleSet(ref); err != nil {
		t.Fatal(err)
	}
	ref.SHA256 = strings.Repeat("00", 32)
	if err := VerifyRuleSet(ref); err == nil {
		t.Fatal("checksum mismatch accepted")
	}
	ref.MaxBytes = 1
	if err := VerifyRuleSet(ref); err == nil {
		t.Fatal("oversize SRS accepted")
	}
}
func TestUnsupportedRuleAcknowledgementAndUnreachableRules(t *testing.T) {
	sub, err := ParseClashYAML(strings.NewReader(syntheticYAML))
	if err != nil {
		t.Fatal(err)
	}
	in := CompileInput{Node: sub.Nodes[0], Rules: sub.Rules, Diagnostics: sub.Diagnostics, RuleSets: stagedRefs()}
	if _, err := CompileNative(in); err == nil {
		t.Fatal("unsupported process rule silently activated")
	}
	in.AcceptUnsupportedRules = true
	in.Rules = append(in.Rules, Rule{Kind: RuleDomain, Value: "unreachable.example.com", Target: TargetBlock, Index: 7})
	out, err := CompileNative(in)
	if err != nil {
		t.Fatal(err)
	}
	if bytes.Contains(out.Config, []byte("unreachable.example.com")) {
		t.Fatal("unreachable rule incorrectly affects DNS")
	}
	found := false
	for _, d := range out.Diagnostics {
		if d.Code == "unreachable-rule" && d.Index == 7 {
			found = true
		}
	}
	if !found {
		t.Fatal("missing unreachable rule diagnostic")
	}
}

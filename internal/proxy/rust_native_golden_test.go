package proxy

import (
	"bytes"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

// Source-only references for the Rust pure compiler. Every credential, address,
// rule-set path and diagnostic below is public synthetic data. This generator
// calls only CompileNative; it never invokes VerifyRuleSet, a core executable,
// a router, an API, a store or a network interface. Absolute synthetic SRS paths
// are metadata only and do not need to exist.
//
// Ordinary tests never write fixtures. Explicit generation / read-only check:
//
//	BE6500_RUST_NATIVE_GOLDEN_OUTPUT=/absolute/output.json go test -p 1 ./internal/proxy -run '^TestRustNativeGoldenFixtures$' -count=1
//	BE6500_RUST_NATIVE_GOLDEN_COMPARE=/absolute/existing.json go test -p 1 ./internal/proxy -run '^TestRustNativeGoldenFixtures$' -count=1
//
// Do not marshal CompileInput or Node: their JSON deliberately redacts private
// fields. These explicit fake-input DTOs preserve all fields, zeros and nils.
const (
	rustNativeGoldenOutputEnv  = "BE6500_RUST_NATIVE_GOLDEN_OUTPUT"
	rustNativeGoldenCompareEnv = "BE6500_RUST_NATIVE_GOLDEN_COMPARE"
)

type rustNativeGoldenNode struct {
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

type rustNativeGoldenRule struct {
	Kind      RuleKind `json:"kind"`
	Value     string   `json:"value"`
	Target    Target   `json:"target"`
	NoResolve bool     `json:"noResolve"`
	Index     int      `json:"index"`
}

type rustNativeGoldenRuleSet struct {
	Tag       string `json:"tag"`
	Kind      string `json:"kind"`
	Path      string `json:"path"`
	SHA256    string `json:"sha256"`
	SourceURL string `json:"sourceURL"`
	MaxBytes  int64  `json:"maxBytes"`
}

type rustNativeGoldenPorts struct {
	Mixed  uint16 `json:"mixed"`
	TProxy uint16 `json:"tProxy"`
	DNS    uint16 `json:"dns"`
}

type rustNativeGoldenDNS struct {
	Server     string `json:"server"`
	Port       uint16 `json:"port"`
	ServerName string `json:"serverName"`
}

type rustNativeGoldenLocalDNS struct {
	Server    string   `json:"server"`
	Port      uint16   `json:"port"`
	Domains   []string `json:"domains"`
	Hostnames []string `json:"hostnames"`
}

type rustNativeGoldenTUN struct {
	InterfaceName string `json:"interfaceName"`
	Address       string `json:"address"`
}

type rustNativeGoldenDiagnostic struct {
	Scope   string `json:"scope"`
	Index   int    `json:"index"`
	Code    string `json:"code"`
	Message string `json:"message"`
}

type rustNativeGoldenInput struct {
	Node                   rustNativeGoldenNode         `json:"node"`
	Rules                  []rustNativeGoldenRule       `json:"rules"`
	Overrides              []rustNativeGoldenRule       `json:"overrides"`
	RuleSets               []rustNativeGoldenRuleSet    `json:"ruleSets"`
	Endpoints              []string                     `json:"endpoints"`
	BootstrapDomains       []string                     `json:"bootstrapDomains"`
	ManagementIPs          []string                     `json:"managementIPs"`
	Datapath               DatapathMode                 `json:"datapath"`
	RoutedTUN              *rustNativeGoldenTUN         `json:"routedTUN"`
	IPv6                   IPv6Mode                     `json:"ipv6"`
	Failure                FailurePolicy                `json:"failure"`
	Ports                  rustNativeGoldenPorts        `json:"ports"`
	ListenAddress          string                       `json:"listenAddress"`
	MixedListenAddress     string                       `json:"mixedListenAddress"`
	TProxyListenAddress    string                       `json:"tProxyListenAddress"`
	DNSListenAddress       string                       `json:"dnsListenAddress"`
	DirectDNS              rustNativeGoldenDNS          `json:"directDNS"`
	ProxyDNS               rustNativeGoldenDNS          `json:"proxyDNS"`
	LocalDNS               *rustNativeGoldenLocalDNS    `json:"localDNS"`
	FakeIP                 bool                         `json:"fakeIP"`
	AcceptUnsupportedRules bool                         `json:"acceptUnsupportedRules"`
	Diagnostics            []rustNativeGoldenDiagnostic `json:"diagnostics"`
}

func rustNativeGoldenMapSlice[A, B any](source []A, convert func(A) B) []B {
	if source == nil {
		return nil
	}
	out := make([]B, len(source))
	for i, item := range source {
		out[i] = convert(item)
	}
	return out
}

func rustNativeGoldenStrings(source []string) []string {
	return rustNativeGoldenMapSlice(source, func(value string) string { return value })
}

func (dto rustNativeGoldenInput) native() CompileInput {
	n := dto.Node
	in := CompileInput{
		Node: Node{ID: n.ID, Name: n.Name, Server: n.Server, Port: n.Port,
			UUID: n.UUID, ServerName: n.ServerName, RealityPublicKey: n.RealityPublicKey,
			RealityShortID: n.RealityShortID, Fingerprint: n.Fingerprint, Flow: n.Flow, UDP: n.UDP},
		Rules:     rustNativeGoldenMapSlice(dto.Rules, rustNativeGoldenRule.native),
		Overrides: rustNativeGoldenMapSlice(dto.Overrides, rustNativeGoldenRule.native),
		RuleSets: rustNativeGoldenMapSlice(dto.RuleSets, func(ref rustNativeGoldenRuleSet) RuleSetReference {
			return RuleSetReference{Tag: ref.Tag, Kind: ref.Kind, Path: ref.Path,
				SHA256: ref.SHA256, SourceURL: ref.SourceURL, MaxBytes: ref.MaxBytes}
		}),
		Endpoints:        rustNativeGoldenStrings(dto.Endpoints),
		BootstrapDomains: rustNativeGoldenStrings(dto.BootstrapDomains),
		ManagementIPs:    rustNativeGoldenStrings(dto.ManagementIPs),
		Datapath:         dto.Datapath, IPv6: dto.IPv6, Failure: dto.Failure,
		Ports:         Ports{Mixed: dto.Ports.Mixed, TProxy: dto.Ports.TProxy, DNS: dto.Ports.DNS},
		ListenAddress: dto.ListenAddress, MixedListenAddress: dto.MixedListenAddress,
		TProxyListenAddress: dto.TProxyListenAddress, DNSListenAddress: dto.DNSListenAddress,
		DirectDNS: dto.DirectDNS.native(), ProxyDNS: dto.ProxyDNS.native(),
		FakeIP: dto.FakeIP, AcceptUnsupportedRules: dto.AcceptUnsupportedRules,
		Diagnostics: rustNativeGoldenMapSlice(dto.Diagnostics, func(d rustNativeGoldenDiagnostic) Diagnostic {
			return Diagnostic{Scope: d.Scope, Index: d.Index, Code: d.Code, Message: d.Message}
		}),
	}
	if dto.RoutedTUN != nil {
		in.RoutedTUN = &RoutedTUNConfig{InterfaceName: dto.RoutedTUN.InterfaceName, Address: dto.RoutedTUN.Address}
	}
	if dto.LocalDNS != nil {
		in.LocalDNS = &LocalDNSConfig{Server: dto.LocalDNS.Server, Port: dto.LocalDNS.Port,
			Domains: rustNativeGoldenStrings(dto.LocalDNS.Domains), Hostnames: rustNativeGoldenStrings(dto.LocalDNS.Hostnames)}
	}
	return in
}

func (dto rustNativeGoldenRule) native() Rule {
	return Rule{Kind: dto.Kind, Value: dto.Value, Target: dto.Target, NoResolve: dto.NoResolve, Index: dto.Index}
}

func (dto rustNativeGoldenDNS) native() DNSEndpoint {
	return DNSEndpoint{Server: dto.Server, Port: dto.Port, ServerName: dto.ServerName}
}

type rustNativeGoldenSourceCase struct {
	name  string
	input rustNativeGoldenInput
	valid bool
}

type rustNativeGoldenCase struct {
	Name             string                `json:"name"`
	Input            rustNativeGoldenInput `json:"input"`
	Valid            bool                  `json:"valid"`
	Config           string                `json:"config,omitempty"`
	SHA256           string                `json:"sha256,omitempty"`
	CoreVersion      string                `json:"coreVersion,omitempty"`
	Diagnostics      *[]Diagnostic         `json:"diagnostics,omitempty"`
	EndpointHosts    *[]string             `json:"endpointHosts,omitempty"`
	RequiredFeatures *[]string             `json:"requiredFeatures,omitempty"`
	IPv6             IPv6Mode              `json:"ipv6,omitempty"`
	Failure          FailurePolicy         `json:"failure,omitempty"`
	Error            string                `json:"error,omitempty"`
}

type rustNativeGoldenDocument struct {
	Version int                    `json:"version"`
	Cases   []rustNativeGoldenCase `json:"cases"`
}

func rustNativeGoldenDefaultInput() rustNativeGoldenInput {
	// A public fake UUID and an all-zero 32-byte REALITY key. No configuration
	// file, environment credential or existing subscription is read.
	return rustNativeGoldenInput{Node: rustNativeGoldenNode{
		ID: "public-fake-node", Name: "Public synthetic node", Server: "node.example", Port: 443,
		UUID: "00000000-0000-4000-8000-000000000001", ServerName: "certificate.example",
		RealityPublicKey: base64.RawURLEncoding.EncodeToString(make([]byte, 32)),
		RealityShortID:   "01020304", Fingerprint: "chrome", Flow: "xtls-rprx-vision", UDP: true,
	}}
}

func rustNativeGoldenRefs() []rustNativeGoldenRuleSet {
	return []rustNativeGoldenRuleSet{
		{Tag: "proxy-domain", Kind: "domain", Path: "/synthetic/be6500-native/proxy-domain.srs", SHA256: strings.Repeat("03", 32), SourceURL: "https://assets.example/proxy-domain.srs", MaxBytes: 2 << 20},
		{Tag: "cn-ip", Kind: "ip", Path: "/synthetic/be6500-native/cn-ip.srs", SHA256: strings.Repeat("02", 32), SourceURL: "https://assets.example/cn-ip.srs", MaxBytes: 2 << 20},
		{Tag: "cn-domain", Kind: "domain", Path: "/synthetic/be6500-native/cn-domain.srs", SHA256: strings.Repeat("01", 32), SourceURL: "https://assets.example/cn-domain.srs", MaxBytes: 2 << 20},
	}
}

func rustNativeGoldenInputs() []rustNativeGoldenSourceCase {
	cases := []rustNativeGoldenSourceCase{}
	add := func(name string, valid bool, change func(*rustNativeGoldenInput)) {
		in := rustNativeGoldenDefaultInput()
		if change != nil {
			change(&in)
		}
		cases = append(cases, rustNativeGoldenSourceCase{name: name, input: in, valid: valid})
	}
	add("default-native-tun", true, nil)
	add("custom-tun-ports-separate-binds", true, func(in *rustNativeGoldenInput) {
		in.Datapath, in.IPv6, in.Failure = DatapathRoutedTUN, IPv6Direct, FailureDirect
		in.RoutedTUN = &rustNativeGoldenTUN{InterfaceName: "b6p-fixture_1", Address: "10.99.0.1/30"}
		in.Ports = rustNativeGoldenPorts{Mixed: 7893, TProxy: 7893, DNS: 1153}
		in.ListenAddress, in.MixedListenAddress, in.DNSListenAddress = "0.0.0.0", "127.0.0.2", "::"
		in.TProxyListenAddress = "public-ignored-tproxy-address"
		in.LocalDNS = &rustNativeGoldenLocalDNS{Port: 5353}
		in.Rules, in.Overrides, in.RuleSets = []rustNativeGoldenRule{}, []rustNativeGoldenRule{}, []rustNativeGoldenRuleSet{}
		in.Endpoints, in.BootstrapDomains, in.ManagementIPs = []string{}, []string{}, []string{}
		in.Diagnostics = []rustNativeGoldenDiagnostic{}
	})
	add("custom-dns-router-authority-aliases-ptr", true, func(in *rustNativeGoldenInput) {
		in.Node.Server = "192.0.2.2"
		in.DirectDNS = rustNativeGoldenDNS{Server: "192.0.2.53", Port: 8853, ServerName: "direct-resolver.example"}
		in.ProxyDNS = rustNativeGoldenDNS{Server: "2001:db8::53", Port: 9853, ServerName: "proxy-resolver.example"}
		in.ManagementIPs = []string{"192.168.31.1"}
		in.LocalDNS = &rustNativeGoldenLocalDNS{Server: "192.168.31.1", Domains: []string{"Office.Home.", "office.home"}, Hostnames: []string{"Router.Office.Home.", "router.office.home", "LEASE-ONE."}}
		in.Rules = []rustNativeGoldenRule{
			{Kind: RuleDomainSuffix, Value: "office.home", Target: TargetBlock, Index: 4},
			{Kind: RuleDomain, Value: "miwifi.com", Target: TargetProxy, Index: 9},
		}
	})
	add("fake-ip-domain-block-direct-proxy", true, func(in *rustNativeGoldenInput) {
		in.FakeIP, in.RuleSets = true, rustNativeGoldenRefs()
		in.Rules = []rustNativeGoldenRule{
			{Kind: RuleDomain, Value: "blocked.example", Target: TargetBlock, Index: 11},
			{Kind: RuleDomainSuffix, Value: "direct.example", Target: TargetDirect, Index: 13},
			{Kind: RuleDomainKeyword, Value: "proxy-key", Target: TargetProxy, Index: 17},
			{Kind: RuleMatch, Target: TargetProxy, Index: 19},
		}
	})
	add("ordered-matchers-no-resolve-controlled-sets", true, func(in *rustNativeGoldenInput) {
		in.RuleSets = rustNativeGoldenRefs()
		in.Overrides = []rustNativeGoldenRule{
			{Kind: RuleDomain, Value: "override.example", Target: TargetBlock, Index: 501},
			{Kind: RuleDomainSuffix, Value: "override-direct.example", Target: TargetDirect, Index: 502},
		}
		in.Rules = []rustNativeGoldenRule{
			{Kind: RuleDomain, Value: "UPPER.Example", Target: TargetDirect, Index: 7},
			{Kind: RuleDomainSuffix, Value: "proxy.example", Target: TargetProxy, Index: 11},
			{Kind: RuleDomainKeyword, Value: "公开<>&\u2028\u2029", Target: TargetBlock, Index: 13},
			{Kind: RuleIPCIDR, Value: "192.0.2.129/24", Target: TargetDirect, NoResolve: true, Index: 17},
			{Kind: RuleIPCIDR, Value: "2001:db8::1/64", Target: TargetBlock, Index: 19},
			{Kind: RuleSet, Value: "cn-ip", Target: TargetDirect, NoResolve: true, Index: 23},
			{Kind: RuleSet, Value: "cn-ip", Target: TargetBlock, Index: 29},
			{Kind: RuleSet, Value: "cn-domain", Target: TargetDirect, Index: 31},
			{Kind: RuleSet, Value: "proxy-domain", Target: TargetProxy, Index: 37},
		}
	})
	add("endpoint-bootstrap-bypass-partial-cn-domain", true, func(in *rustNativeGoldenInput) {
		in.Node.Server = "198.51.100.9"
		in.Endpoints = []string{"203.0.113.7", "2001:db8::7", "203.0.113.7"}
		in.ManagementIPs = []string{"192.168.31.1", "fd00::1"}
		in.BootstrapDomains = []string{"z-bootstrap.example", "a-bootstrap.example", "a-bootstrap.example"}
		in.RuleSets = rustNativeGoldenRefs()[2:]
		in.Overrides = []rustNativeGoldenRule{
			{Kind: RuleIPCIDR, Value: "203.0.113.7/32", Target: TargetBlock, NoResolve: true, Index: 40},
			{Kind: RuleDomain, Value: "a-bootstrap.example", Target: TargetBlock, Index: 41},
		}
	})
	add("partial-cn-ip-resolving-fallback", true, func(in *rustNativeGoldenInput) {
		in.RuleSets = rustNativeGoldenRefs()[1:2]
		in.Rules = []rustNativeGoldenRule{{Kind: RuleDomain, Value: "explicit-proxy.example", Target: TargetProxy, Index: 2}}
	})
	for _, target := range []Target{TargetDirect, TargetBlock, TargetProxy} {
		add("terminal-match-"+string(target), true, func(in *rustNativeGoldenInput) {
			in.RuleSets = rustNativeGoldenRefs()
			in.Rules = []rustNativeGoldenRule{
				{Kind: RuleDomain, Value: "before-match.example", Target: TargetProxy, Index: 5},
				{Kind: RuleMatch, Target: target, Index: 7},
				{Kind: RuleDomain, Value: "unreachable.example", Target: TargetDirect, Index: 99},
			}
		})
	}
	add("unsupported-diagnostic-refused", false, func(in *rustNativeGoldenInput) {
		in.Diagnostics = []rustNativeGoldenDiagnostic{{Scope: "rule", Index: 17, Code: "public-original-code", Message: "public-original-message"}}
	})
	add("unsupported-diagnostics-acknowledged-fixed-order", true, func(in *rustNativeGoldenInput) {
		in.AcceptUnsupportedRules, in.FakeIP = true, true
		in.Diagnostics = []rustNativeGoldenDiagnostic{
			{Scope: "node", Index: 0, Code: "public-original-code", Message: "public-original-message"},
			{Scope: "rule", Index: 17, Code: "public-original-code", Message: "public-original-message"},
			{Scope: "config", Index: -1, Code: "public-original-code", Message: "public-original-message"},
			{Scope: "rule", Index: 23, Code: "public-original-code", Message: "public-original-message"},
		}
		in.Rules = []rustNativeGoldenRule{{Kind: RuleMatch, Target: TargetDirect, Index: 29}, {Kind: RuleDomain, Value: "omitted.example", Target: TargetBlock, Index: 31}}
	})
	add("invalid-node-uuid", false, func(in *rustNativeGoldenInput) { in.Node.UUID = "public-invalid-uuid" })
	add("invalid-node-reality-key", false, func(in *rustNativeGoldenInput) { in.Node.RealityPublicKey = "public-invalid-key" })
	add("invalid-node-fingerprint", false, func(in *rustNativeGoldenInput) { in.Node.Fingerprint = "public-invalid-fingerprint" })
	add("invalid-node-server", false, func(in *rustNativeGoldenInput) { in.Node.Server = "https://public-invalid-server.example" })
	add("invalid-node-flow", false, func(in *rustNativeGoldenInput) { in.Node.Flow = "public-invalid-flow" })
	add("retired-tproxy-datapath", false, func(in *rustNativeGoldenInput) { in.Datapath = DatapathTPROXY })
	add("unsupported-ipv6-follow", false, func(in *rustNativeGoldenInput) { in.IPv6 = IPv6Follow })
	add("unsupported-ipv6-block", false, func(in *rustNativeGoldenInput) { in.IPv6 = IPv6Block })
	add("unsupported-failure-block-proxy", false, func(in *rustNativeGoldenInput) { in.Failure = FailureBlockProxy })
	add("listener-port-collision", false, func(in *rustNativeGoldenInput) { in.Ports = rustNativeGoldenPorts{Mixed: 2080, DNS: 2080} })
	add("invalid-tun-network-host", false, func(in *rustNativeGoldenInput) {
		in.RoutedTUN = &rustNativeGoldenTUN{InterfaceName: "b6p-tun", Address: "172.31.255.252/30"}
	})
	add("tun-management-prefix-collision", false, func(in *rustNativeGoldenInput) { in.ManagementIPs = []string{"172.31.255.254"} })
	add("local-dns-core-listener-collision", false, func(in *rustNativeGoldenInput) { in.LocalDNS = &rustNativeGoldenLocalDNS{Port: 1053} })
	add("invalid-controlled-set-tag", false, func(in *rustNativeGoldenInput) {
		in.RuleSets = rustNativeGoldenRefs()[:1]
		in.RuleSets[0].Tag = "public-invalid-tag"
	})
	add("invalid-controlled-set-path", false, func(in *rustNativeGoldenInput) {
		in.RuleSets = rustNativeGoldenRefs()[:1]
		in.RuleSets[0].Path = "/synthetic/be6500-native/../public-invalid-path.srs"
	})
	add("invalid-controlled-set-hash", false, func(in *rustNativeGoldenInput) {
		in.RuleSets = rustNativeGoldenRefs()[:1]
		in.RuleSets[0].SHA256 = "public-invalid-hash"
	})
	add("invalid-controlled-set-provenance", false, func(in *rustNativeGoldenInput) {
		in.RuleSets = rustNativeGoldenRefs()[:1]
		in.RuleSets[0].SourceURL = "https://public-user:public-password@assets.example/proxy-domain.srs?public-token=yes"
	})
	add("node-udp-disabled-rejected", false, func(in *rustNativeGoldenInput) { in.Node.UDP = false })
	return cases
}

func rustNativeGoldenBuild(t *testing.T) rustNativeGoldenDocument {
	t.Helper()
	document := rustNativeGoldenDocument{Version: 1, Cases: []rustNativeGoldenCase{}}
	names := map[string]bool{}
	for _, source := range rustNativeGoldenInputs() {
		if names[source.name] {
			t.Fatal("duplicate native golden case", source.name)
		}
		names[source.name] = true
		in, original := source.input.native(), source.input.native()
		out, err := CompileNative(in)
		if !reflect.DeepEqual(in, original) {
			t.Fatal("reference compiler changed caller input", source.name)
		}
		if (err == nil) != source.valid {
			t.Fatalf("%s: unexpected synthetic validation result: %v", source.name, err)
		}
		fixture := rustNativeGoldenCase{Name: source.name, Input: source.input, Valid: err == nil}
		if err != nil {
			if len(out.Config) != 0 || out.SHA256 != "" {
				t.Fatal("invalid native input returned configuration", source.name)
			}
			fixture.Error = err.Error() // Actual safe Go error, never a guessed expectation.
			for _, marker := range []string{"public-invalid", "public-original", "public-user", "public-password", "public-token", source.input.Node.UUID, source.input.Node.RealityPublicKey, source.input.Node.RealityShortID} {
				if marker != "" && strings.Contains(fixture.Error, marker) {
					t.Fatal("native error echoed synthetic credential or diagnostic material", source.name)
				}
			}
		} else {
			if !json.Valid(out.Config) || !bytes.HasSuffix(out.Config, []byte("\n")) {
				t.Fatal("reference output must be exact JSON with final newline", source.name)
			}
			sum := sha256.Sum256(out.Config)
			if out.SHA256 != hex.EncodeToString(sum[:]) {
				t.Fatal("reference SHA256 does not cover exact config bytes", source.name)
			}
			fixture.Config, fixture.SHA256, fixture.CoreVersion = string(out.Config), out.SHA256, out.CoreVersion
			fixture.Diagnostics, fixture.EndpointHosts, fixture.RequiredFeatures = &out.Diagnostics, &out.EndpointHosts, &out.RequiredFeatures
			fixture.IPv6, fixture.Failure = out.IPv6, out.Failure
			publicMetadata, marshalErr := json.Marshal(out)
			if marshalErr != nil {
				t.Fatal(marshalErr)
			}
			for _, marker := range []string{source.input.Node.UUID, source.input.Node.RealityPublicKey, source.input.Node.RealityShortID, "public-original"} {
				if bytes.Contains(publicMetadata, []byte(marker)) {
					t.Fatal("native output metadata echoed synthetic credential or diagnostic material", source.name)
				}
			}
		}
		document.Cases = append(document.Cases, fixture)
	}
	return document
}

func rustNativeGoldenBytes(t *testing.T) []byte {
	t.Helper()
	// Config and its hash come from CompileNative. Preserve Go JSON field order
	// and escaping, plus its exact embedded MarshalIndent config and newline.
	raw, err := json.MarshalIndent(rustNativeGoldenBuild(t), "", "  ")
	if err != nil {
		t.Fatal(err)
	}
	return append(raw, '\n')
}

func TestRustNativeGoldenSourceCases(t *testing.T) {
	document := rustNativeGoldenBuild(t)
	if len(document.Cases) != 30 {
		t.Fatal("review the compact 30-case source matrix if it changes")
	}
	cases := map[string]rustNativeGoldenCase{}
	for _, fixture := range document.Cases {
		cases[fixture.Name] = fixture
	}
	first := cases["default-native-tun"]
	if first.Input.RoutedTUN != nil || first.Input.LocalDNS != nil || first.Input.Rules != nil || first.Input.Ports != (rustNativeGoldenPorts{}) {
		t.Fatal("default input DTO lost explicit zeros or nils")
	}
	explicit := rustNativeGoldenDefaultInput()
	explicit.Datapath, explicit.IPv6, explicit.Failure = DatapathRoutedTUN, IPv6Direct, FailureDirect
	explicit.RoutedTUN = &rustNativeGoldenTUN{InterfaceName: "b6p-tun", Address: "172.31.255.253/30"}
	explicit.Ports = rustNativeGoldenPorts{Mixed: 2080, TProxy: 65535, DNS: 1053}
	explicit.TProxyListenAddress = "public-ignored-tproxy-address"
	out, err := CompileNative(explicit.native())
	if err != nil || first.Config != string(out.Config) || first.SHA256 != out.SHA256 {
		t.Fatal("explicit defaults or ignored retired listener fields changed default output")
	}
	if *cases["ordered-matchers-no-resolve-controlled-sets"].Diagnostics != nil {
		t.Fatal("nil output diagnostics must stay JSON null, not [] or omitted")
	}
	acknowledged := *cases["unsupported-diagnostics-acknowledged-fixed-order"].Diagnostics
	want := []Diagnostic{
		{Scope: "rule", Index: 17, Code: "ignored-subscription-rule", Message: "unsupported subscription rule explicitly acknowledged and omitted"},
		{Scope: "rule", Index: 23, Code: "ignored-subscription-rule", Message: "unsupported subscription rule explicitly acknowledged and omitted"},
		{Scope: "config", Index: -1, Code: "cn-rules-incomplete", Message: "domestic split needs staged cn-domain and cn-ip SRS; missing classifications use the default proxy policy"},
		{Scope: "rule", Index: 31, Code: "unreachable-rule", Message: "rule follows terminal MATCH and is unreachable"},
		{Scope: "config", Index: -1, Code: "fakeip-memory", Message: "fake-IP identities use an unbounded native RAM map; restart or direct-failure withdrawal needs coordinated stale DNS cleanup before recapture"},
	}
	if !reflect.DeepEqual(acknowledged, want) {
		t.Fatal("fixed safe diagnostic order changed")
	}
	raw := rustNativeGoldenBytes(t)
	if !bytes.Equal(raw, rustNativeGoldenBytes(t)) {
		t.Fatal("native fixture generation is not byte-deterministic")
	}
	var decoded rustNativeGoldenDocument
	if err := json.Unmarshal(raw, &decoded); err != nil {
		t.Fatal("cannot decode explicit native fixture DTO", err)
	}
	// encoding/json decodes a null slice pointer as a nil pointer. Restore the
	// valid-output wrapper only; the underlying nil diagnostic slice stays nil.
	for i := range decoded.Cases {
		if decoded.Cases[i].Valid && decoded.Cases[i].Diagnostics == nil {
			var diagnostics []Diagnostic
			decoded.Cases[i].Diagnostics = &diagnostics
		}
	}
	if !reflect.DeepEqual(decoded, document) {
		t.Fatal("explicit DTO round-trip lost private synthetic fields, zeros, nils or output bytes")
	}
	for _, escaped := range []string{`\u003c`, `\u003e`, `\u0026`, `\u2028`, `\u2029`} {
		if !bytes.Contains([]byte(cases["ordered-matchers-no-resolve-controlled-sets"].Config), []byte(escaped)) {
			t.Fatal("Go native JSON escaping missing", escaped)
		}
	}
}

func TestRustNativeGoldenFixtures(t *testing.T) {
	outputPath, comparePath := os.Getenv(rustNativeGoldenOutputEnv), os.Getenv(rustNativeGoldenCompareEnv)
	if outputPath == "" && comparePath == "" {
		t.Skip("explicit golden output or compare path is required; ordinary tests do not write fixtures")
	}
	if outputPath != "" && comparePath != "" {
		t.Fatal("choose either explicit output or read-only compare, not both")
	}
	selectedPath := outputPath
	if selectedPath == "" {
		selectedPath = comparePath
	}
	if !filepath.IsAbs(selectedPath) {
		t.Fatal("golden output and compare paths must be absolute")
	}
	raw := rustNativeGoldenBytes(t)
	if outputPath != "" {
		if err := os.WriteFile(outputPath, raw, 0o644); err != nil {
			t.Fatal(err)
		}
		return
	}
	existing, err := os.ReadFile(comparePath)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(existing, raw) {
		t.Fatal("Go native golden differs; regenerate explicitly and review the fixture change")
	}
}

package proxy

import (
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

func nativeRuleMatchesValue(rule map[string]any, key, value string) bool {
	values, ok := rule[key].([]any)
	if !ok {
		return false
	}
	for _, item := range values {
		if item == value {
			return true
		}
	}
	return false
}

func nativeDNSServer(t *testing.T, config map[string]any, tag string) map[string]any {
	t.Helper()
	for _, server := range maps(config["dns"].(map[string]any)["servers"]) {
		if server["tag"] == tag {
			return server
		}
	}
	t.Fatalf("missing DNS server %s", tag)
	return nil
}

func TestNativeLocalDNSDefaultsPreserveRouterResolver(t *testing.T) {
	out, err := CompileNative(CompileInput{Node: testNode(t)})
	if err != nil {
		t.Fatal(err)
	}
	config := decodeConfig(t, out)
	local := nativeDNSServer(t, config, "dns-local")
	if local["type"] != "udp" || local["server"] != "127.0.0.1" || local["server_port"] != float64(53) || local["detour"] != "direct" {
		t.Fatalf("LAN names do not use original dnsmasq UDP/TCP path: %v", local)
	}
	if local["domain_resolver"] != nil || local["tls"] != nil {
		t.Fatal("local resolver must use a literal IP without DoT or recursive bootstrap")
	}
	for _, want := range []struct{ tag, address, identity, detour string }{
		{"dns-direct", "223.5.5.5", "dns.alidns.com", "direct"},
		{"dns-proxy", "1.1.1.1", "cloudflare-dns.com", "proxy"},
	} {
		server := nativeDNSServer(t, config, want.tag)
		if server["type"] != "tls" || server["server"] != want.address || server["server_port"] != float64(853) || server["detour"] != want.detour || server["tls"].(map[string]any)["server_name"] != want.identity {
			t.Fatalf("public authenticated resolver changed: %v", server)
		}
	}
}

func TestNativeLocalDNSPrecedesImportedPolicyAndIPv6CatchAll(t *testing.T) {
	for _, mode := range []IPv6Mode{"", IPv6Direct, IPv6Follow, IPv6Block} {
		t.Run(string(mode), func(t *testing.T) {
			out, err := CompileNative(CompileInput{Node: testNode(t), IPv6: mode, FakeIP: true, Rules: []Rule{
				{Kind: RuleDomainSuffix, Value: "lan", Target: TargetBlock},
				{Kind: RuleDomain, Value: "miwifi.com", Target: TargetProxy},
				{Kind: RuleDomain, Value: "foreign.example.com", Target: TargetProxy},
				{Kind: RuleDomain, Value: "domestic.example.com", Target: TargetDirect},
				{Kind: RuleMatch, Target: TargetProxy},
			}})
			if mode == IPv6Follow || mode == IPv6Block {
				if err == nil || len(out.Config) != 0 || !strings.Contains(err.Error(), "routed-tun supports only IPv6 direct") {
					t.Fatalf("unsupported IPv6 DNS path must not compile: %v", err)
				}
				return
			}
			if err != nil {
				t.Fatal(err)
			}
			config := decodeConfig(t, out)
			dnsRules := maps(config["dns"].(map[string]any)["rules"])
			for _, want := range []struct{ key, value string }{
				{"domain_suffix", "lan"}, {"domain_suffix", "local"}, {"domain_suffix", "localhost"},
				{"domain", "xiaoqiang"}, {"domain", "miwifi.com"}, {"domain", "www.miwifi.com"},
				{"domain", "router.miwifi.com"}, {"domain", "www.router.miwifi.com"}, {"domain_regex", "^[^.]+$"},
			} {
				first := -1
				for i, rule := range dnsRules {
					if nativeRuleMatchesValue(rule, want.key, want.value) {
						first = i
						break
					}
				}
				if first < 0 || dnsRules[first]["server"] != "dns-local" || dnsRules[first]["query_type"] != nil || dnsRules[first]["disable_cache"] != true || dnsRules[first]["rewrite_ttl"] != nil {
					t.Fatalf("LAN name policy lost dnsmasq authority: %s %v", want.value, dnsRules)
				}
				for i, rule := range dnsRules {
					if rule["query_type"] != nil && i < first {
						t.Fatalf("IPv6/fake-IP catch-all hides local DNS: %v", dnsRules)
					}
				}
			}
			routeRules := maps(config["route"].(map[string]any)["rules"])
			for _, want := range []struct{ key, value string }{{"domain_suffix", "lan"}, {"domain", "miwifi.com"}, {"domain_regex", "^[^.]+$"}} {
				first := -1
				for i, rule := range routeRules {
					if nativeRuleMatchesValue(rule, want.key, want.value) {
						first = i
						break
					}
				}
				if first < 0 || routeRules[first]["action"] != "resolve" || routeRules[first]["server"] != "dns-local" || first+1 >= len(routeRules) || routeRules[first+1]["outbound"] != "direct" || !nativeRuleMatchesValue(routeRules[first+1], want.key, want.value) {
					t.Fatalf("explicit local hostnames cannot resolve and route direct before subscription MATCH: %v", routeRules)
				}
			}
			foreign := false
			for i, rule := range dnsRules {
				if nativeRuleMatchesValue(rule, "domain", "foreign.example.com") && rule["server"] == "dns-fake" && i+1 < len(dnsRules) && dnsRules[i+1]["server"] == "dns-proxy" {
					foreign = true
				}
			}
			if !foreign || config["dns"].(map[string]any)["final"] != "dns-proxy" {
				t.Fatal("foreign fake-IP and authenticated fallback policy changed")
			}
			for _, name := range []string{"domestic.example.com", "dns.alidns.com", "example.com"} {
				found := false
				for _, rule := range dnsRules {
					if nativeRuleMatchesValue(rule, "domain", name) && rule["server"] == "dns-direct" {
						found = true
						break
					}
				}
				if !found {
					t.Fatalf("public domestic/bootstrap DoT intent changed for %s", name)
				}
			}
		})
	}
}

func TestNativePrivatePTRUsesLocalDNS(t *testing.T) {
	out, err := CompileNative(CompileInput{Node: testNode(t), Rules: []Rule{{Kind: RuleMatch, Target: TargetProxy}}})
	if err != nil {
		t.Fatal(err)
	}
	rules := maps(decodeConfig(t, out)["dns"].(map[string]any)["rules"])
	for _, suffix := range []string{"10.in-addr.arpa", "16.172.in-addr.arpa", "31.172.in-addr.arpa", "168.192.in-addr.arpa", "127.in-addr.arpa", "254.169.in-addr.arpa", "64.100.in-addr.arpa", "127.100.in-addr.arpa", "c.f.ip6.arpa", "d.f.ip6.arpa", "8.e.f.ip6.arpa", "b.e.f.ip6.arpa"} {
		found := false
		for _, rule := range rules {
			if nativeRuleMatchesValue(rule, "domain_suffix", suffix) && reflect.DeepEqual(rule["query_type"], []any{"PTR"}) && rule["server"] == "dns-local" && rule["disable_cache"] == true {
				found = true
			}
		}
		if !found {
			t.Fatalf("private reverse namespace escaped local resolver: %s", suffix)
		}
	}
	for _, rule := range rules {
		if rule["server"] == "dns-local" && (nativeRuleMatchesValue(rule, "domain_suffix", "in-addr.arpa") || nativeRuleMatchesValue(rule, "domain_suffix", "ip6.arpa")) {
			t.Fatal("public reverse DNS sent to dnsmasq")
		}
	}
}

func TestNativeDNSHijackIsIngressOnly(t *testing.T) {
	out, err := CompileNative(CompileInput{Node: testNode(t)})
	if err != nil {
		t.Fatal(err)
	}
	rules := maps(decodeConfig(t, out)["route"].(map[string]any)["rules"])
	port53 := false
	for _, rule := range rules {
		if rule["action"] != "hijack-dns" {
			continue
		}
		if rule["inbound"] == nil {
			t.Fatal("DNS hijack is not scoped to a client ingress")
		}
		if nativeRuleMatchesValue(rule, "inbound", "dns-in") {
			continue
		}
		if !reflect.DeepEqual(rule["inbound"], []any{"mixed-in", "tun-in"}) || !reflect.DeepEqual(rule["port"], []any{float64(53)}) {
			t.Fatalf("unexpected DNS hijack scope: %v", rule)
		}
		port53 = true
	}
	if !port53 {
		t.Fatal("mixed/transparent client DNS lost interception")
	}
}

func TestNativeConfiguredLocalDNSAndNoInputMutation(t *testing.T) {
	local := &LocalDNSConfig{Server: "192.168.31.1", Domains: []string{"Office.Home.", "office.home"}, Hostnames: []string{"Router.Office.Home.", "router.office.home"}}
	saved := *local
	saved.Domains = append([]string{}, local.Domains...)
	saved.Hostnames = append([]string{}, local.Hostnames...)
	in := CompileInput{Node: testNode(t), LocalDNS: local, ManagementIPs: []string{"192.168.31.1"}}
	out, err := CompileNative(in)
	if err != nil {
		t.Fatal(err)
	}
	config := decodeConfig(t, out)
	server := nativeDNSServer(t, config, "dns-local")
	if server["server"] != "192.168.31.1" || server["server_port"] != float64(53) {
		t.Fatal("caller router resolver ignored")
	}
	foundDomain, foundHost := false, false
	for _, rule := range maps(config["dns"].(map[string]any)["rules"]) {
		if rule["server"] == "dns-local" {
			foundDomain = foundDomain || nativeRuleMatchesValue(rule, "domain_suffix", "office.home")
			foundHost = foundHost || nativeRuleMatchesValue(rule, "domain", "router.office.home")
		}
	}
	if !foundDomain || !foundHost || !reflect.DeepEqual(saved, *local) {
		t.Fatal("actual router LAN domains/aliases not preserved or input mutated")
	}
	in.LocalDNS = &LocalDNSConfig{Server: "192.168.31.1", Port: 53, Domains: []string{"office.home"}, Hostnames: []string{"router.office.home"}}
	second, err := CompileNative(in)
	if err != nil || string(second.Config) != string(out.Config) {
		t.Fatal("equivalent local router settings change compiled bytes")
	}
}

func TestNativeRejectsLocalDNSLoopsAndInvalidSettings(t *testing.T) {
	for _, local := range []*LocalDNSConfig{
		{Server: "private-token.example.com"}, {Server: "0.0.0.0"}, {Server: "224.0.0.1"}, {Server: "fe80::1%br-lan"}, {Server: "::ffff:127.0.0.1"},
		{Server: "223.5.5.5"}, {Server: "192.168.31.2"}, {Port: 1053}, {Port: 2080},
		{Domains: []string{"https://private-token.example.com"}}, {Hostnames: []string{"bad name"}},
		{Domains: make([]string, 33)}, {Hostnames: make([]string, 65)},
	} {
		out, err := CompileNative(CompileInput{Node: testNode(t), LocalDNS: local})
		if err == nil || len(out.Config) != 0 {
			t.Fatalf("unsafe local DNS settings accepted: %+v", local)
		}
		if strings.Contains(err.Error(), "private-token") {
			t.Fatal("invalid local DNS error leaks caller input")
		}
	}
	out, err := CompileNative(CompileInput{Node: testNode(t), Ports: Ports{Mixed: 2080, TProxy: 7893, DNS: 53}, DNSListenAddress: "::"})
	if err == nil || len(out.Config) != 0 {
		t.Fatal("wildcard DNS port53 loops with original dnsmasq")
	}
}

// Set SING_BOX_CHECK to a locally executable target-core build for an offline
// schema/construction check. The command never starts listeners or changes DNS.
// NATIVE_DNS_FIXTURE_DIR optionally saves synthetic fixtures for a target CPU
// check performed separately; this test never invokes SSH or deploys a core.
func TestNativeLocalDNSTargetCoreCheck(t *testing.T) {
	binary := os.Getenv("SING_BOX_CHECK")
	directory := os.Getenv("NATIVE_DNS_FIXTURE_DIR")
	if binary == "" && directory == "" {
		t.Skip("SING_BOX_CHECK or NATIVE_DNS_FIXTURE_DIR is not set")
	}
	for _, mode := range []IPv6Mode{IPv6Follow, IPv6Direct, IPv6Block} {
		for _, fake := range []bool{false, true} {
			out, err := CompileNative(CompileInput{Node: testNode(t), IPv6: mode, FakeIP: fake, LocalDNS: &LocalDNSConfig{Domains: []string{"office.home"}, Hostnames: []string{"router.office.home"}}, Rules: []Rule{{Kind: RuleMatch, Target: TargetProxy}}})
			if mode == IPv6Follow || mode == IPv6Block {
				if err == nil || len(out.Config) != 0 || !strings.Contains(err.Error(), "routed-tun supports only IPv6 direct") {
					t.Fatalf("unsupported IPv6 DNS path must not compile: %v", err)
				}
				continue
			}
			if err != nil {
				t.Fatal(err)
			}
			path := filepath.Join(t.TempDir(), "native.json")
			if directory != "" {
				name := "native-local-dns-" + string(mode)
				if fake {
					name += "-fake"
				}
				path = filepath.Join(directory, name+".json")
			}
			if err = os.WriteFile(path, out.Config, 0600); err != nil {
				t.Fatal(err)
			}
			if binary != "" {
				if output, checkErr := exec.Command(binary, "check", "-c", path).CombinedOutput(); checkErr != nil {
					t.Fatalf("target check mode=%s fake=%t failed: %v\n%s", mode, fake, checkErr, output)
				}
			}
		}
	}
	if binary == "" {
		t.Skip("synthetic fixtures saved; target core check not run (SING_BOX_CHECK is not set)")
	}
}

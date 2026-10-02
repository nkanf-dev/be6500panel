package proxy

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"net/netip"
	"net/url"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// CompileNative emits sing-box 1.14.2 native JSON. Config is private and must be
// checked by the target core and persisted atomically as mode 0600 by the owner.
// Compilation never resolves hosts, opens sockets, downloads, or applies rules.
func CompileNative(in CompileInput) (CompileOutput, error) {
	var out CompileOutput
	if err := validateNode(in.Node); err != nil {
		return out, err
	}
	if in.IPv6 == "" {
		in.IPv6 = IPv6Direct
	}
	if in.IPv6 != IPv6Direct && in.IPv6 != IPv6Follow && in.IPv6 != IPv6Block {
		return out, fmt.Errorf("invalid IPv6 policy")
	}
	if in.Failure == "" {
		in.Failure = FailureDirect
	}
	if in.Failure == FailureBlockProxy {
		return out, fmt.Errorf("block-proxy requires a surviving classification authority; not supported")
	}
	if in.Failure != FailureDirect {
		return out, fmt.Errorf("invalid failure policy")
	}
	if in.Ports == (Ports{}) {
		in.Ports = Ports{Mixed: 2080, TProxy: 7893, DNS: 1053}
	}
	if in.Ports.Mixed == 0 || in.Ports.TProxy == 0 || in.Ports.DNS == 0 || in.Ports.Mixed == in.Ports.TProxy || in.Ports.Mixed == in.Ports.DNS || in.Ports.TProxy == in.Ports.DNS {
		return out, fmt.Errorf("listener ports must be nonzero and distinct")
	}
	if in.MixedListenAddress == "" {
		in.MixedListenAddress = in.ListenAddress
	}
	if in.MixedListenAddress == "" {
		in.MixedListenAddress = "127.0.0.1"
	}
	if in.TProxyListenAddress == "" {
		in.TProxyListenAddress = in.ListenAddress
	}
	if in.TProxyListenAddress == "" {
		in.TProxyListenAddress = "127.0.0.1"
		if in.IPv6 == IPv6Follow {
			in.TProxyListenAddress = "::"
		}
	}
	if in.DNSListenAddress == "" {
		in.DNSListenAddress = in.ListenAddress
	}
	if in.DNSListenAddress == "" {
		in.DNSListenAddress = "127.0.0.1"
	}
	for _, s := range []string{in.MixedListenAddress, in.TProxyListenAddress, in.DNSListenAddress} {
		listen, err := netip.ParseAddr(s)
		if err != nil || listen.Zone() != "" || listen.IsMulticast() {
			return out, fmt.Errorf("invalid listener address")
		}
	}
	if in.IPv6 == IPv6Follow {
		listen, _ := netip.ParseAddr(in.TProxyListenAddress)
		if !listen.Is6() {
			return out, fmt.Errorf("IPv6 follow requires a dual-stack IPv6 TPROXY listener address")
		}
	}
	var err error

	if in.DirectDNS == (DNSEndpoint{}) {
		in.DirectDNS = DNSEndpoint{Server: "223.5.5.5", Port: 853, ServerName: "dns.alidns.com"}
	}
	if in.ProxyDNS == (DNSEndpoint{}) {
		in.ProxyDNS = DNSEndpoint{Server: "1.1.1.1", Port: 853, ServerName: "cloudflare-dns.com"}
	}
	if err = validateDNS(in.DirectDNS); err != nil {
		return out, err
	}
	if err = validateDNS(in.ProxyDNS); err != nil {
		return out, err
	}
	localDNS, err := compileLocalDNS(in)
	if err != nil {
		return out, err
	}
	if len(in.Rules)+len(in.Overrides) > MaxRules || len(in.Endpoints) > 256 || len(in.BootstrapDomains) > 256 || len(in.ManagementIPs) > 128 {
		return out, fmt.Errorf("compiler input limit exceeded")
	}
	for _, d := range in.Diagnostics {
		// Do not echo caller-supplied diagnostic text, which can contain private data.
		if d.Scope == "rule" && !in.AcceptUnsupportedRules {
			return out, fmt.Errorf("unsupported subscription rule at index %d requires explicit acknowledgement", d.Index)
		}
		if d.Scope == "rule" {
			out.Diagnostics = append(out.Diagnostics, Diagnostic{"rule", d.Index, "ignored-subscription-rule", "unsupported subscription rule explicitly acknowledged and omitted"})
		}
	}
	all := append(append([]Rule{}, in.Overrides...), in.Rules...)
	for i, r := range all {
		if err = validateRule(r); err != nil {
			return out, fmt.Errorf("invalid native rule at position %d", i)
		}
	}
	ruleSets, tags, err := compileRuleSets(in.RuleSets)
	if err != nil {
		return out, err
	}
	for _, r := range all {
		if r.Kind == RuleSet && !tags[r.Value] {
			return out, fmt.Errorf("rule references unstaged controlled set %s", r.Value)
		}
	}
	if !tags["cn-domain"] || !tags["cn-ip"] {
		out.Diagnostics = append(out.Diagnostics, Diagnostic{"config", -1, "cn-rules-incomplete", "domestic split needs staged cn-domain and cn-ip SRS; missing classifications use the default proxy policy"})
	}
	bypassIPs := append(append([]string{}, in.ManagementIPs...), in.Endpoints...)
	if a, err := netip.ParseAddr(in.Node.Server); err == nil {
		bypassIPs = append(bypassIPs, a.String())
	}
	bypassIPs = append(bypassIPs, in.DirectDNS.Server)
	bypass, err := addressPrefixes(bypassIPs)
	if err != nil {
		return out, err
	}
	// Keep resolver bootstrap identity direct even when the node uses a literal
	// IP. Readiness queries for this identity must not depend on the proxy path.
	bootstrap := append([]string{in.DirectDNS.ServerName}, in.BootstrapDomains...)
	if _, err = netip.ParseAddr(in.Node.Server); err != nil {
		bootstrap = append(bootstrap, in.Node.Server)
	}
	for _, host := range bootstrap {
		if !validDomain(host) {
			return out, fmt.Errorf("invalid bootstrap domain")
		}
	}
	bootstrap = sortedUnique(bootstrap)
	// These are explicit bypasses, not a deprecated geoip private database.
	private := []string{"0.0.0.0/8", "10.0.0.0/8", "100.64.0.0/10", "127.0.0.0/8", "169.254.0.0/16", "172.16.0.0/12", "192.168.0.0/16", "192.0.0.0/24", "198.18.0.0/15", "224.0.0.0/4", "240.0.0.0/4", "::/128", "::1/128", "fe80::/10", "fc00::/7", "ff00::/8"}

	routeRules := []map[string]any{{"inbound": []string{"dns-in"}, "action": "hijack-dns"}}
	if len(bypass) > 0 {
		routeRules = append(routeRules, map[string]any{"ip_cidr": bypass, "outbound": "direct"})
	}
	if len(bootstrap) > 0 {
		routeRules = append(routeRules, map[string]any{"domain": bootstrap, "outbound": "direct"})
	}
	// Hijack only client ingress, not DNS transport traffic. A DNS transport
	// detour calls the outbound dialer directly, without re-entering route rules;
	// its router-originated sockets also avoid the planner's LAN PREROUTING hooks.
	routeRules = append(routeRules, map[string]any{"inbound": []string{"mixed-in", "tproxy-in"}, "port": []uint16{53}, "action": "hijack-dns"})
	routeRules = append(routeRules, map[string]any{"ip_cidr": private, "outbound": "direct"})
	// Explicit mixed-proxy hostnames need local resolution too: merely routing
	// them direct would use the direct outbound's public DoT bootstrap resolver.
	for _, match := range localDNS.matches() {
		resolve := cloneNativeMatch(match)
		resolve["action"], resolve["server"], resolve["timeout"], resolve["disable_cache"] = "resolve", "dns-local", "5s", true
		routeRules = append(routeRules, resolve, routeAction(match, TargetDirect))
	}
	if in.IPv6 == IPv6Direct {
		routeRules = append(routeRules, map[string]any{"ip_version": 6, "outbound": "direct"})
	}
	if in.IPv6 == IPv6Block {
		routeRules = append(routeRules, map[string]any{"ip_version": 6, "action": "reject"})
	}
	routeRules = append(routeRules, map[string]any{"inbound": []string{"mixed-in", "tproxy-in"}, "action": "sniff", "sniffer": []string{"http", "tls", "dns"}, "timeout": "300ms"})
	dnsRules := []map[string]any{}
	if len(bootstrap) > 0 {
		dnsRules = append(dnsRules, map[string]any{"domain": bootstrap, "server": "dns-direct", "rewrite_ttl": 300})
	}
	for _, match := range localDNS.matches() {
		match["server"], match["disable_cache"] = "dns-local", true
		dnsRules = append(dnsRules, match)
	}
	// Reverse LAN lookups must reach dnsmasq lease/hosts records as well. Match
	// only private reverse namespaces, never all public in-addr.arpa/ip6.arpa.
	dnsRules = append(dnsRules, map[string]any{"query_type": []string{"PTR"}, "domain_suffix": localReverseSuffixes(), "server": "dns-local", "disable_cache": true})
	if in.IPv6 == IPv6Block {
		dnsRules = append(dnsRules, map[string]any{"query_type": []string{"AAAA"}, "action": "predefined", "rcode": "NOERROR"})
	}
	if in.IPv6 == IPv6Direct {
		dnsRules = append(dnsRules, map[string]any{"query_type": []string{"AAAA"}, "server": "dns-direct", "rewrite_ttl": 300})
	}
	defaultTarget := TargetProxy
	fallbackAdded := false
	addFallback := func() {
		if fallbackAdded {
			return
		}
		fallbackAdded = true
		if tags["cn-domain"] {
			routeRules = append(routeRules, map[string]any{"rule_set": []string{"cn-domain"}, "outbound": "direct"})
			dnsRules = append(dnsRules, map[string]any{"rule_set": []string{"cn-domain"}, "server": "dns-direct", "rewrite_ttl": 300})
		}
		if tags["proxy-domain"] {
			routeRules = append(routeRules, map[string]any{"rule_set": []string{"proxy-domain"}, "outbound": "proxy"})
			dnsRules = appendDNSRules(dnsRules, map[string]any{"rule_set": []string{"proxy-domain"}}, TargetProxy, in.FakeIP)
		}
		// Resolve only after terminal domain intent. This permits CN-IP fallback for
		// unclassified names without overriding matched direct/proxy domain intent.
		if tags["cn-ip"] {
			routeRules = append(routeRules, map[string]any{"action": "resolve", "server": "dns-proxy", "timeout": "5s"})
			routeRules = append(routeRules, map[string]any{"rule_set": []string{"cn-ip"}, "outbound": "direct"})
		}
	}
	matchSeen := false
	for _, r := range all {
		if matchSeen {
			out.Diagnostics = append(out.Diagnostics, Diagnostic{"rule", r.Index, "unreachable-rule", "rule follows terminal MATCH and is unreachable"})
			continue
		}
		if r.Kind == RuleMatch {
			addFallback()
			defaultTarget = r.Target
			matchSeen = true
		}
		if (r.Kind == RuleIPCIDR || r.Kind == RuleSet && r.Value == "cn-ip") && !r.NoResolve {
			routeRules = append(routeRules, map[string]any{"action": "resolve", "server": "dns-proxy", "timeout": "5s"})
		}
		routeRules = append(routeRules, routeAction(ruleMatch(r), r.Target))
		if isDomainRule(r) || r.Kind == RuleMatch {
			dnsRules = appendDNSRules(dnsRules, ruleMatch(r), r.Target, in.FakeIP)
		}
	}
	addFallback()
	if !matchSeen {
		routeRules = append(routeRules, routeAction(map[string]any{}, defaultTarget))
		dnsRules = appendDNSRules(dnsRules, map[string]any{}, defaultTarget, in.FakeIP)
	}

	// A/AAAA fake answers are generated by the new native transport, never by
	// legacy dns.fakeip. Other query types still use real authenticated DNS.
	servers := []map[string]any{dnsServer("dns-direct", in.DirectDNS, "direct"), dnsServer("dns-proxy", in.ProxyDNS, "proxy"), {"type": "udp", "tag": "dns-local", "server": localDNS.Server, "server_port": localDNS.Port, "detour": "direct"}}
	if in.FakeIP {
		fake := map[string]any{"type": "fakeip", "tag": "dns-fake", "inet4_range": "198.18.0.0/15"}
		if in.IPv6 == IPv6Follow {
			fake["inet6_range"] = "fc00::/18"
		}
		servers = append(servers, fake)
		out.Diagnostics = append(out.Diagnostics, Diagnostic{"config", -1, "fakeip-memory", "fake-IP identities use an unbounded native RAM map; restart or direct-failure withdrawal needs coordinated stale DNS cleanup before recapture"})
	}
	strategy := "prefer_ipv4"
	if in.IPv6 == IPv6Block {
		strategy = "ipv4_only"
	}
	resolver := func() map[string]any {
		return map[string]any{"server": "dns-direct", "timeout": "5s", "strategy": strategy}
	}
	config := map[string]any{
		"log": map[string]any{"level": "warn", "timestamp": true},
		"dns": map[string]any{"servers": servers, "rules": dnsRules, "final": "dns-proxy", "cache_capacity": 1024, "timeout": "5s", "reverse_mapping": true, "strategy": strategy},
		"inbounds": []map[string]any{
			{"type": "mixed", "tag": "mixed-in", "listen": in.MixedListenAddress, "listen_port": in.Ports.Mixed},
			{"type": "tproxy", "tag": "tproxy-in", "listen": in.TProxyListenAddress, "listen_port": in.Ports.TProxy, "udp_timeout": "2m", "udp_nat_max": 1024},
			{"type": "direct", "tag": "dns-in", "listen": in.DNSListenAddress, "listen_port": in.Ports.DNS},
		},
		"outbounds": []map[string]any{
			{"type": "direct", "tag": "direct", "domain_resolver": resolver()},
			{"type": "vless", "tag": "proxy", "server": in.Node.Server, "server_port": in.Node.Port, "uuid": in.Node.UUID, "flow": in.Node.Flow, "packet_encoding": "xudp", "domain_resolver": resolver(), "connect_timeout": "10s", "tcp_fast_open": true, "tls": map[string]any{"enabled": true, "server_name": in.Node.ServerName, "utls": map[string]any{"enabled": true, "fingerprint": in.Node.Fingerprint}, "reality": map[string]any{"enabled": true, "public_key": in.Node.RealityPublicKey, "short_id": in.Node.RealityShortID}}},
		},
		"route": map[string]any{"rules": routeRules, "rule_set": ruleSets, "final": "proxy", "auto_detect_interface": true, "default_domain_resolver": resolver()},
	}
	out.Config, err = json.MarshalIndent(config, "", "  ")
	if err != nil {
		return CompileOutput{}, fmt.Errorf("cannot encode native configuration")
	}
	out.Config = append(out.Config, '\n')
	hash := sha256.Sum256(out.Config)
	out.SHA256 = hex.EncodeToString(hash[:])
	out.CoreVersion = CoreVersion
	out.EndpointHosts = sortedUnique(append(append([]string{in.Node.Server}, in.Endpoints...), bootstrap...))
	out.RequiredFeatures = []string{"with_utls", "badlinkname", "tcp_fast_open", "tproxy_tcp_udp", "tls_dns"}
	out.IPv6 = in.IPv6
	out.Failure = in.Failure
	return out, nil
}

// compileLocalDNS defaults to the router's original loopback dnsmasq, not the
// system resolver (which could point back at sing-box). Extra router addresses
// are admitted only from management intent and may not share a core port.
func compileLocalDNS(in CompileInput) (LocalDNSConfig, error) {
	local := LocalDNSConfig{Server: "127.0.0.1", Port: 53}
	if in.LocalDNS != nil {
		local = *in.LocalDNS
		if local.Server == "" {
			local.Server = "127.0.0.1"
		}
		if local.Port == 0 {
			local.Port = 53
		}
	}
	addr, err := netip.ParseAddr(local.Server)
	if err != nil || addr.Zone() != "" || addr.Is4In6() || addr.IsUnspecified() || addr.IsMulticast() || addr.IsLinkLocalUnicast() || local.Server == "255.255.255.255" {
		return LocalDNSConfig{}, fmt.Errorf("local DNS requires a literal loopback or router management address")
	}
	local.Server = addr.String()
	if !addr.IsLoopback() {
		managed := false
		for _, value := range in.ManagementIPs {
			if management, parseErr := netip.ParseAddr(value); parseErr == nil && management == addr {
				managed = true
				break
			}
		}
		if !managed {
			return LocalDNSConfig{}, fmt.Errorf("local DNS address must belong to router ManagementIPs")
		}
	}
	if local.Port == in.Ports.DNS || local.Port == in.Ports.Mixed || local.Port == in.Ports.TProxy {
		return LocalDNSConfig{}, fmt.Errorf("local DNS must not point to a core listener port")
	}
	if len(local.Domains) > 32 || len(local.Hostnames) > 64 {
		return LocalDNSConfig{}, fmt.Errorf("local DNS domain or hostname limit exceeded")
	}
	local.Domains, err = localDNSNames([]string{"lan", "local", "localhost"}, local.Domains)
	if err != nil {
		return LocalDNSConfig{}, err
	}
	// These exact aliases are generated by the RN02 factory dnsmasq service;
	// do not divert other public MiWiFi service subdomains from public DoT.
	local.Hostnames, err = localDNSNames([]string{"xiaoqiang", "miwifi.com", "www.miwifi.com", "router.miwifi.com", "www.router.miwifi.com"}, local.Hostnames)
	if err != nil {
		return LocalDNSConfig{}, err
	}
	return local, nil
}

func localDNSNames(defaults, extra []string) ([]string, error) {
	values := append([]string{}, defaults...)
	for _, name := range extra {
		name = strings.ToLower(strings.TrimSuffix(name, "."))
		if !validDomain(name) {
			return nil, fmt.Errorf("invalid local DNS domain or hostname")
		}
		values = append(values, name)
	}
	return sortedUnique(values), nil
}

func (local LocalDNSConfig) matches() []map[string]any {
	return []map[string]any{
		{"domain_suffix": local.Domains},
		{"domain": local.Hostnames},
		// dnsmasq can resolve bare DHCP lease names with no domain suffix.
		{"domain_regex": []string{"^[^.]+$"}},
	}
}

func cloneNativeMatch(match map[string]any) map[string]any {
	clone := make(map[string]any, len(match))
	for key, value := range match {
		clone[key] = value
	}
	return clone
}

func localReverseSuffixes() []string {
	suffixes := []string{"10.in-addr.arpa", "168.192.in-addr.arpa", "127.in-addr.arpa", "254.169.in-addr.arpa", "c.f.ip6.arpa", "d.f.ip6.arpa", "8.e.f.ip6.arpa", "9.e.f.ip6.arpa", "a.e.f.ip6.arpa", "b.e.f.ip6.arpa", "1." + strings.Repeat("0.", 31) + "ip6.arpa"}
	for second := 16; second <= 31; second++ {
		suffixes = append(suffixes, fmt.Sprintf("%d.172.in-addr.arpa", second))
	}
	for second := 64; second <= 127; second++ {
		suffixes = append(suffixes, fmt.Sprintf("%d.100.in-addr.arpa", second))
	}
	return sortedUnique(suffixes)
}

func validateDNS(e DNSEndpoint) error {
	a, err := netip.ParseAddr(e.Server)
	if err != nil || a.Zone() != "" || a.IsUnspecified() || a.IsMulticast() || e.Port == 0 || !validDomain(e.ServerName) {
		return fmt.Errorf("DNS requires a literal IP, nonzero TLS port and certificate DNS identity")
	}
	return nil
}
func dnsServer(tag string, e DNSEndpoint, detour string) map[string]any {
	return map[string]any{"type": "tls", "tag": tag, "server": e.Server, "server_port": e.Port, "detour": detour, "tls": map[string]any{"enabled": true, "server_name": e.ServerName, "min_version": "1.2"}}
}
func ruleMatch(r Rule) map[string]any {
	m := map[string]any{}
	switch r.Kind {
	case RuleDomain:
		m["domain"] = []string{r.Value}
	case RuleDomainSuffix:
		m["domain_suffix"] = []string{r.Value}
	case RuleDomainKeyword:
		m["domain_keyword"] = []string{r.Value}
	case RuleIPCIDR:
		m["ip_cidr"] = []string{r.Value}
	case RuleSet:
		m["rule_set"] = []string{r.Value}
	}
	return m
}
func routeAction(m map[string]any, t Target) map[string]any {
	if t == TargetBlock {
		m["action"] = "reject"
	} else {
		m["outbound"] = string(t)
	}
	return m
}
func isDomainRule(r Rule) bool {
	return r.Kind == RuleDomain || r.Kind == RuleDomainSuffix || r.Kind == RuleDomainKeyword || r.Kind == RuleSet && r.Value != "cn-ip"
}
func appendDNSRules(rules []map[string]any, match map[string]any, target Target, fake bool) []map[string]any {
	if target == TargetBlock {
		match["action"] = "reject"
		return append(rules, match)
	}
	server := "dns-direct"
	if target == TargetProxy {
		server = "dns-proxy"
	}
	if target == TargetProxy && fake {
		addrMatch := map[string]any{}
		for k, v := range match {
			addrMatch[k] = v
		}
		addrMatch["query_type"] = []string{"A", "AAAA"}
		addrMatch["server"] = "dns-fake"
		addrMatch["rewrite_ttl"] = 60
		rules = append(rules, addrMatch)
	}
	match["server"] = server
	match["rewrite_ttl"] = 300
	return append(rules, match)
}

func addressPrefixes(values []string) ([]string, error) {
	out := []string{}
	for _, s := range values {
		a, err := netip.ParseAddr(s)
		if err != nil || a.Zone() != "" || a.Is4In6() || a.IsUnspecified() || a.IsMulticast() {
			return nil, fmt.Errorf("invalid management or bootstrap IP")
		}
		bits := 128
		if a.Is4() {
			bits = 32
		}
		out = append(out, netip.PrefixFrom(a, bits).String())
	}
	return sortedUnique(out), nil
}
func sortedUnique(values []string) []string {
	out := append([]string{}, values...)
	sort.Strings(out)
	n := 0
	for _, s := range out {
		if n == 0 || out[n-1] != s {
			out[n] = s
			n++
		}
	}
	return out[:n]
}

// VerifyRuleSet opens a bounded local file and verifies the controlled hash. It
// is deliberately separate from pure CompileNative; activation must reverify
// all files immediately before check/start, then retain the last accepted set.
func VerifyRuleSet(ref RuleSetReference) error {
	if _, _, err := compileRuleSets([]RuleSetReference{ref}); err != nil {
		return err
	}
	f, err := os.Open(ref.Path)
	if err != nil {
		return fmt.Errorf("cannot open staged rule set")
	}
	defer f.Close()
	info, err := f.Stat()
	if err != nil || !info.Mode().IsRegular() || info.Size() > ref.MaxBytes {
		return fmt.Errorf("staged rule set size or type invalid")
	}
	h := sha256.New()
	n, err := io.Copy(h, io.LimitReader(f, ref.MaxBytes+1))
	if err != nil || n > ref.MaxBytes {
		return fmt.Errorf("cannot read bounded staged rule set")
	}
	if hex.EncodeToString(h.Sum(nil)) != strings.ToLower(ref.SHA256) {
		return fmt.Errorf("staged rule set checksum mismatch")
	}
	return nil
}
func compileRuleSets(refs []RuleSetReference) ([]map[string]any, map[string]bool, error) {
	out := []map[string]any{}
	tags := map[string]bool{}
	if len(refs) > 3 {
		return nil, nil, fmt.Errorf("at most three controlled rule sets are supported")
	}
	sorted := append([]RuleSetReference{}, refs...)
	sort.Slice(sorted, func(i, j int) bool { return sorted[i].Tag < sorted[j].Tag })
	for _, ref := range sorted {
		kind := "domain"
		if ref.Tag == "cn-ip" {
			kind = "ip"
		}
		if ref.Tag != "cn-domain" && ref.Tag != "cn-ip" && ref.Tag != "proxy-domain" || tags[ref.Tag] || ref.Kind != kind {
			return nil, nil, fmt.Errorf("invalid controlled rule set tag or kind")
		}
		if !filepath.IsAbs(ref.Path) || filepath.Clean(ref.Path) != ref.Path || filepath.Ext(ref.Path) != ".srs" || strings.ContainsAny(ref.Path, "\r\n\x00") {
			return nil, nil, fmt.Errorf("rule set requires an absolute clean SRS path")
		}
		hash, err := hex.DecodeString(ref.SHA256)
		if err != nil || len(hash) != 32 || ref.MaxBytes <= 0 || ref.MaxBytes > 8<<20 {
			return nil, nil, fmt.Errorf("rule set requires a pinned SHA256 and bounded size")
		}
		if ref.SourceURL != "" {
			u, err := url.Parse(ref.SourceURL)
			if err != nil || u.Scheme != "https" || u.Hostname() == "" || u.User != nil || u.RawQuery != "" || u.Fragment != "" {
				return nil, nil, fmt.Errorf("rule set provenance must be credential-free HTTPS")
			}
		}
		tags[ref.Tag] = true
		out = append(out, map[string]any{"type": "local", "tag": ref.Tag, "format": "binary", "path": ref.Path})
	}
	return out, tags, nil
}

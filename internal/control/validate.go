package control

import (
	"fmt"
	"net"
	"strconv"
	"strings"
	"unicode"
)

type section struct {
	kind, name string
	values     map[string][]string
}
type parsed []section

// tokens recognizes UCI quoting, escapes, adjacent quoted strings and comments.
// It is deliberately not a shell parser; config/option/list are the only commands.
func tokens(content string) ([][]string, error) {
	var records [][]string
	var words []string
	var word strings.Builder
	started, escaped := false, false
	var quote rune
	flushWord := func() {
		if started {
			words = append(words, word.String())
			word.Reset()
			started = false
		}
	}
	flushLine := func() {
		flushWord()
		if len(words) > 0 {
			records = append(records, words)
			words = nil
		}
	}
	runes := []rune(content)
	for i := 0; i < len(runes); i++ {
		c := runes[i]
		if c == 0 || (unicode.IsControl(c) && c != '\n' && c != '\r' && c != '\t') {
			return nil, fmt.Errorf("control character")
		}
		if escaped {
			if c != '\n' {
				word.WriteRune(c)
				started = true
			}
			escaped = false
			continue
		}
		if quote != 0 {
			if c == quote {
				quote = 0
				continue
			}
			if c == '\\' && quote == '"' {
				escaped = true
				continue
			}
			word.WriteRune(c)
			continue
		}
		switch c {
		case '\\':
			escaped = true
			started = true
		case '\'', '"':
			quote = c
			started = true
		case '#':
			for i < len(runes) && runes[i] != '\n' {
				i++
			}
			flushLine()
		case '\n':
			flushLine()
		case ' ', '\t', '\r':
			flushWord()
		case ';', '`':
			return nil, fmt.Errorf("invalid unquoted token")
		default:
			word.WriteRune(c)
			started = true
		}
	}
	if escaped || quote != 0 {
		return nil, fmt.Errorf("unterminated quote or escape")
	}
	flushLine()
	return records, nil
}
func identifier(s string) bool {
	if s == "" || len(s) > 128 {
		return false
	}
	for _, c := range s {
		if !(c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' || c == '_' || c == '-') {
			return false
		}
	}
	return true
}
func parse(content string) (parsed, error) {
	rows, err := tokens(content)
	if err != nil {
		return nil, err
	}
	out := parsed{}
	names := map[string]bool{}
	for _, r := range rows {
		switch r[0] {
		case "config":
			if len(r) < 2 || len(r) > 3 || !identifier(r[1]) {
				return nil, fmt.Errorf("invalid config")
			}
			name := ""
			if len(r) == 3 {
				name = r[2]
				if !identifier(name) || names[name] {
					return nil, fmt.Errorf("invalid section name")
				}
				names[name] = true
			}
			out = append(out, section{kind: r[1], name: name, values: map[string][]string{}})
		case "option", "list":
			if len(out) == 0 || len(r) != 3 || !identifier(r[1]) {
				return nil, fmt.Errorf("invalid option or list")
			}
			s := &out[len(out)-1]
			if r[0] == "option" {
				s.values[r[1]] = []string{r[2]}
			} else {
				s.values[r[1]] = append(s.values[r[1]], r[2])
			}
		default:
			return nil, fmt.Errorf("unsupported UCI command")
		}
		if len(out) > 512 {
			return nil, fmt.Errorf("too many sections")
		}
	}
	return out, nil
}
func issue(code, message string) Issue { return Issue{Code: code, Message: message} }
func validate(module, content string) (parsed, []Issue) {
	p, err := parse(content)
	if err != nil {
		return nil, []Issue{issue("uci_syntax", "Only valid native config, option and list statements are accepted.")}
	}
	errors := []Issue{}
	add := func(key string) {
		errors = append(errors, issue("invalid_field", "Invalid "+module+" field: "+key+"."))
	}
	for _, s := range p {
		for key, values := range s.values {
			for _, value := range values {
				if !validNativeField(module, s.kind, key, value) {
					add(key)
				}
			}
		}
		if module == "wireless" && s.kind == "wifi-iface" {
			ssid := one(s, "ssid")
			if len(ssid) > 32 || strings.ContainsAny(ssid, "\r\n") {
				add("ssid")
			}
			encryption := strings.ToLower(one(s, "encryption"))
			key := one(s, "key")
			if (strings.HasPrefix(encryption, "psk") || strings.HasPrefix(encryption, "sae")) && key != "" && !(len(key) >= 8 && len(key) <= 63 || len(key) == 64 && hexadecimal(key)) {
				add("key")
			}
		}
		if module == "network" && s.kind == "interface" {
			ip := net.ParseIP(one(s, "ipaddr"))
			mask := one(s, "netmask")
			if ip != nil && ip.To4() == nil && mask != "" {
				add("netmask")
			}
		}
	}
	return p, errors
}

// Native scalar checks are scoped to exact, case-sensitive UCI namespaces.
// Compound values (forwarding rules, rates, leases, device ports, select tokens)
// and vendor extensions remain native text; this is not service validation.
func validNativeField(module, kind, key, v string) bool {
	switch module + "/" + kind {
	case "network/interface":
		switch key {
		case "ip6assign":
			return integer(v, 0, 128)
		case "peerdns", "defaultroute", "delegate", "auto", "force_link", "disabled":
			return boolean(v)
		case "mtu":
			return integer(v, 576, 65535)
		case "metric", "demand":
			return unsignedInteger(v, 0)
		case "ipaddr", "ip6addr", "gateway", "ip6gw", "broadcast", "dns":
			return ips(v)
		case "netmask":
			return netmask(v)
		case "macaddr":
			return macs(v, false)
		}
	case "network/device":
		switch key {
		case "vid":
			return integer(v, 1, 4094)
		case "mtu":
			return integer(v, 576, 65535)
		case "ipv6", "stp", "igmp_snooping", "multicast_querier", "bridge_empty", "vlan_filtering", "disabled":
			return boolean(v)
		case "ageing_time":
			return unsignedInteger(v, 0)
		case "priority":
			return integer(v, 0, 65535)
		case "macaddr":
			return macs(v, false)
		}
	case "network/bridge-vlan":
		switch key {
		case "vlan":
			return integer(v, 1, 4094)
		case "local":
			return boolean(v)
		}
	case "network/switch":
		switch key {
		case "reset", "enable_vlan", "enable_mirror_rx", "enable_mirror_tx":
			return boolean(v)
		case "mirror_source_port", "mirror_monitor_port":
			return unsignedInteger(v, 0)
		}
	case "network/switch_vlan":
		switch key {
		case "vlan":
			return unsignedInteger(v, 0)
		case "vid":
			return integer(v, 1, 4094)
		}
	case "network/route":
		switch key {
		case "metric":
			return unsignedInteger(v, 0)
		case "mtu":
			return integer(v, 576, 65535)
		case "onlink", "disabled":
			return boolean(v)
		case "target", "gateway", "source":
			return ips(v)
		case "netmask":
			return netmask(v)
		}
	case "network/route6":
		switch key {
		case "metric":
			return unsignedInteger(v, 0)
		case "mtu":
			return integer(v, 576, 65535)
		case "onlink", "disabled":
			return boolean(v)
		case "target", "gateway", "source":
			return ips(v)
		}
	case "network/rule":
		switch key {
		case "priority", "goto":
			return unsignedInteger(v, 0)
		case "invert", "disabled":
			return boolean(v)
		case "suppress_prefixlength":
			return integer(v, 0, 128)
		case "src", "dest":
			return ips(v)
		}
	case "network/rule6":
		switch key {
		case "priority", "goto":
			return unsignedInteger(v, 0)
		case "invert", "disabled":
			return boolean(v)
		case "suppress_prefixlength":
			return integer(v, 0, 128)
		case "src", "dest":
			return ips(v)
		}
	case "network/globals":
		switch key {
		case "ula_prefix":
			return ips(v)
		}
	case "wireless/wifi-device":
		switch key {
		case "txpower":
			return integer(v, 0, 40)
		case "disabled", "legacy_rates", "noscan":
			return boolean(v)
		case "beacon_int":
			return integer(v, 15, 65535)
		case "distance":
			return unsignedInteger(v, 0)
		case "macaddr":
			return macs(v, false)
		case "channel":
			return v == "auto" || integer(v, 1, 233)
		}
	case "wireless/wifi-iface":
		switch key {
		case "disabled", "hidden", "isolate", "wds", "wmm", "ieee80211r", "ieee80211k", "mesh_fwding":
			return boolean(v)
		case "maxassoc":
			return unsignedInteger(v, 0)
		case "dtim_period":
			return integer(v, 1, 255)
		case "auth_port", "acct_port":
			return integer(v, 1, 65535)
		case "bssid", "maclist":
			return macs(v, false)
		}
	case "dhcp/dnsmasq":
		switch key {
		case "domainneeded", "boguspriv", "filterwin2k", "localise_queries", "rebind_protection", "rebind_localhost", "expandhosts", "authoritative", "readethers", "noresolv", "nohosts", "nonwildcard", "localservice", "strictorder", "allservers", "logqueries", "logdhcp":
			return boolean(v)
		case "port", "queryport":
			return integer(v, 0, 65535)
		case "cachesize", "dnsforwardmax", "dhcpleasemax":
			return unsignedInteger(v, 0)
		case "ednspacket_max":
			return integer(v, 512, 65535)
		case "domain":
			return domain(v)
		case "listen_address":
			return ips(v)
		}
	case "dhcp/dhcp":
		switch key {
		case "start", "limit":
			return integer(v, 0, 65535)
		case "ignore", "force", "dynamicdhcp", "master", "ra_slaac":
			return boolean(v)
		case "ra_mininterval", "ra_maxinterval", "ra_lifetime":
			return unsignedInteger(v, 0)
		case "ra_mtu":
			return integer(v, 1280, 65535)
		case "dns":
			return ips(v)
		case "domain":
			return domain(v)
		case "netmask":
			return netmask(v)
		}
	case "dhcp/host":
		switch key {
		case "dns", "broadcast":
			return boolean(v)
		case "ip":
			return v == "ignore" || ips(v)
		case "mac":
			return macs(v, true)
		}
	case "dhcp/domain":
		switch key {
		case "ip":
			return ips(v)
		}
	case "dhcp/odhcpd":
		switch key {
		case "maindhcp":
			return boolean(v)
		case "loglevel":
			return integer(v, 0, 7)
		}
	case "dhcp/cname":
		switch key {
		case "ttl":
			return unsignedInteger(v, 0)
		}
	case "dhcp/boot":
		switch key {
		case "serveraddress":
			return ips(v)
		}
	case "dhcp/relay":
		switch key {
		case "local_addr", "server_addr":
			return ips(v)
		}
	case "dhcp/srvhost":
		switch key {
		case "port":
			return integer(v, 1, 65535)
		case "class", "weight":
			return integer(v, 0, 65535)
		}
	case "dhcp/mxhost":
		switch key {
		case "pref":
			return integer(v, 0, 65535)
		case "domain":
			return domain(v)
		}
	case "firewall/defaults":
		switch key {
		case "synflood_protect", "drop_invalid", "flow_offloading", "flow_offloading_hw", "disable_ipv6":
			return boolean(v)
		case "input", "forward", "output":
			return v == "ACCEPT" || v == "REJECT" || v == "DROP"
		}
	case "firewall/zone":
		switch key {
		case "masq", "masq6", "mtu_fix", "log", "enabled":
			return boolean(v)
		case "input", "forward", "output":
			return v == "ACCEPT" || v == "REJECT" || v == "DROP"
		}
	case "firewall/forwarding":
		switch key {
		case "enabled":
			return boolean(v)
		}
	case "firewall/rule":
		switch key {
		case "enabled", "utc_time":
			return boolean(v)
		case "limit_burst":
			return unsignedInteger(v, 0)
		case "src_ip", "dest_ip":
			return ips(v)
		case "src_mac":
			return macs(v, false)
		case "src_port", "dest_port":
			return ports(v)
		}
	case "firewall/redirect":
		switch key {
		case "enabled", "reflection":
			return boolean(v)
		case "limit_burst":
			return unsignedInteger(v, 0)
		case "src_ip", "dest_ip", "src_dip":
			return ips(v)
		case "src_mac":
			return macs(v, false)
		case "src_port", "dest_port", "src_dport":
			return ports(v)
		}
	case "firewall/nat":
		switch key {
		case "enabled":
			return boolean(v)
		case "limit_burst":
			return unsignedInteger(v, 0)
		case "src_ip", "dest_ip", "snat_ip":
			return ips(v)
		case "src_mac":
			return macs(v, false)
		case "src_port", "dest_port", "snat_port":
			return ports(v)
		}
	case "firewall/include":
		switch key {
		case "enabled", "reload", "fw4_compatible":
			return boolean(v)
		}
	case "firewall/ipset":
		switch key {
		case "maxelem":
			return unsignedInteger(v, 1)
		case "timeout":
			return unsignedInteger(v, 0)
		case "enabled":
			return boolean(v)
		}
	case "system/system":
		switch key {
		case "log_size":
			return unsignedInteger(v, 0)
		case "log_port":
			return integer(v, 1, 65535)
		case "log_remote":
			return boolean(v)
		case "conloglevel", "cronloglevel":
			return integer(v, 0, 8)
		case "hostname":
			return domain(v)
		}
	case "system/timeserver":
		switch key {
		case "enabled", "enable_server", "use_dhcp":
			return boolean(v)
		}
	case "system/led":
		switch key {
		case "default":
			return boolean(v)
		case "delayon", "delayoff", "interval":
			return unsignedInteger(v, 0)
		}
	case "dropbear/dropbear":
		switch key {
		case "Port":
			return integer(v, 1, 65535)
		case "PasswordAuth", "RootPasswordAuth", "RootLogin", "GatewayPorts", "enable", "mdns":
			return boolean(v)
		case "IdleTimeout", "SSHKeepAlive", "MaxAuthTries":
			return unsignedInteger(v, 0)
		}
	}
	return true
}

// An absent upper bound in the native field schema is not an arbitrary panel
// limit. Parse unsigned values independently of the router's 32-bit int width.
func unsignedInteger(v string, min uint64) bool {
	n, err := strconv.ParseUint(v, 10, 64)
	return err == nil && n >= min
}

func macs(v string, wildcard bool) bool {
	fields := strings.Fields(v)
	if len(fields) == 0 {
		return false
	}
	for _, value := range fields {
		if wildcard && value == "*" {
			continue
		}
		if wildcard && strings.Contains(value, "*") {
			octets := strings.Split(value, ":")
			if len(octets) != 6 {
				return false
			}
			for _, octet := range octets {
				if octet != "*" && (len(octet) != 2 || !hexadecimal(octet)) {
					return false
				}
			}
			continue
		}
		if len(value) != 17 {
			return false
		}
		address, err := net.ParseMAC(value)
		if err != nil || len(address) != 6 {
			return false
		}
	}
	return true
}
func one(s section, k string) string {
	v := s.values[k]
	if len(v) == 0 {
		return ""
	}
	return v[len(v)-1]
}
func integer(v string, min, max int) bool {
	n, e := strconv.Atoi(v)
	return e == nil && n >= min && n <= max
}
func boolean(v string) bool {
	switch strings.ToLower(v) {
	case "0", "1", "true", "false", "on", "off", "yes", "no":
		return true
	}
	return false
}
func disabled(s section) bool {
	switch strings.ToLower(one(s, "disabled")) {
	case "1", "true", "on", "yes":
		return true
	}
	return false
}
func hexadecimal(v string) bool {
	for _, c := range v {
		if !strings.ContainsRune("0123456789abcdefABCDEF", c) {
			return false
		}
	}
	return true
}
func ports(v string) bool {
	fields := strings.Fields(strings.ReplaceAll(v, ",", " "))
	if len(fields) == 0 {
		return false
	}
	for _, f := range fields {
		f = strings.TrimPrefix(f, "!")
		ends := strings.FieldsFunc(f, func(c rune) bool { return c == '-' || c == ':' })
		if len(ends) < 1 || len(ends) > 2 {
			return false
		}
		for _, e := range ends {
			if !integer(e, 1, 65535) {
				return false
			}
		}
		if len(ends) == 2 {
			a, _ := strconv.Atoi(ends[0])
			b, _ := strconv.Atoi(ends[1])
			if a > b {
				return false
			}
		}
	}
	return true
}
func ips(v string) bool {
	fields := strings.Fields(v)
	if len(fields) == 0 {
		return false
	}
	for _, f := range fields {
		f = strings.TrimPrefix(f, "!")
		if net.ParseIP(f) != nil {
			continue
		}
		if _, _, err := net.ParseCIDR(f); err == nil {
			continue
		}
		return false
	}
	return true
}
func netmask(v string) bool {
	if integer(v, 0, 32) {
		return true
	}
	ip := net.ParseIP(v)
	if ip == nil || ip.To4() == nil {
		return false
	}
	ones, bits := net.IPMask(ip.To4()).Size()
	return bits == 32 && ones >= 0
}
func domain(v string) bool {
	if len(v) == 0 || len(v) > 253 {
		return false
	}
	v = strings.TrimSuffix(v, ".")
	v = strings.TrimPrefix(v, "*.")
	for _, label := range strings.Split(v, ".") {
		if len(label) == 0 || len(label) > 63 || label[0] == '-' || label[len(label)-1] == '-' {
			return false
		}
		for _, c := range label {
			if !(c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' || c == '-') {
				return false
			}
		}
	}
	return true
}

func risk(module, before, after string) []Issue {
	a, _ := parse(before)
	b, _ := parse(after)
	out := []Issue{}
	add := func(code, message string) {
		for _, r := range out {
			if r.Code == code {
				return
			}
		}
		out = append(out, issue(code, message))
	}
	switch module {
	case "network":
		if before != after {
			add("network_reload", "Network reload can disconnect LAN and Wi-Fi management sessions.")
		}
		if sectionFields(a, "interface", "lan", "ipaddr", "ip6addr", "netmask", "device", "ifname", "proto", "disabled") != sectionFields(b, "interface", "lan", "ipaddr", "ip6addr", "netmask", "device", "ifname", "proto", "disabled") {
			add("management_network", "LAN address, mask or management interface changes can disconnect this panel.")
		}
		if sectionFields(a, "interface", "wan", "device", "ifname", "proto", "disabled") != sectionFields(b, "interface", "wan", "device", "ifname", "proto", "disabled") || sectionFields(a, "interface", "wan6", "device", "ifname", "proto", "disabled") != sectionFields(b, "interface", "wan6", "device", "ifname", "proto", "disabled") {
			add("wan_replacement", "WAN interface changes can interrupt connectivity.")
		}
	case "dropbear":
		if managementFields(a) != managementFields(b) {
			add("management_service", "SSH port, authentication or service availability changes can remove recovery access.")
		}
	case "system":
		if managementFields(a) != managementFields(b) {
			add("management_service", "Management service settings can disconnect this panel.")
		}
	case "firewall":
		if firewallFields(a) != firewallFields(b) {
			add("firewall_policy", "Firewall input, zone membership or broad policies can block management or forwarding.")
		}
	case "wireless":
		if primaryWiFiFields(a) != primaryWiFiFields(b) {
			add("wifi_access", "Primary Wi-Fi credentials or radio settings can disconnect this browser.")
		}
		if activeWiFi(a) > 0 && activeWiFi(b) == 0 {
			add("all_wifi_disabled", "Disabling all primary Wi-Fi access can disconnect clients.")
		}
	}
	return out
}
func sectionFields(p parsed, kind, name string, keys ...string) string {
	var out strings.Builder
	for _, s := range p {
		if s.kind == kind && s.name == name {
			out.WriteString("present|")
			for _, k := range keys {
				fmt.Fprintf(&out, "%s=%q;", k, s.values[k])
			}
		}
	}
	return out.String()
}
func managementFields(p parsed) string {
	var out strings.Builder
	for _, s := range p {
		fmt.Fprintf(&out, "%s:%s|", s.kind, s.name)
		for _, k := range []string{"Port", "Interface", "PasswordAuth", "RootPasswordAuth", "RootLogin", "port", "disabled", "enabled", "auth", "rootdisabled", "listen_http", "listen_https"} {
			fmt.Fprintf(&out, "%s=%q;", k, s.values[k])
		}
	}
	return out.String()
}
func firewallFields(p parsed) string {
	var out strings.Builder
	for _, s := range p {
		if s.kind == "defaults" || s.kind == "zone" {
			fmt.Fprintf(&out, "%s:%s:%s:%s:%s:%s;", s.kind, s.name, one(s, "name"), one(s, "input"), one(s, "forward"), one(s, "output"))
			if s.kind == "zone" {
				for _, key := range []string{"network", "device"} {
					fmt.Fprintf(&out, "%s=%q;", key, s.values[key])
				}
			}
		}
		if s.kind != "rule" || disabled(s) || firewallRuleDisabled(s) {
			continue
		}
		switch one(s, "target") {
		case "DROP", "REJECT":
			// An INPUT rule can deny this panel even when both IP filters are
			// present. Only a concrete destination zone scopes a rule to forwarding.
			if firewallInputRule(s) || one(s, "dest_ip") == "" || one(s, "src_ip") == "" {
				fmt.Fprintf(&out, "deny:%q;", s.values)
			}
		case "ACCEPT":
			// Removing or narrowing an INPUT exception can also block access
			// beneath an unchanged DROP/REJECT zone or default input policy.
			if firewallInputRule(s) {
				fmt.Fprintf(&out, "allow:%q;", s.values)
			}
		}
	}
	return out.String()
}
func firewallRuleDisabled(s section) bool {
	switch strings.ToLower(one(s, "enabled")) {
	case "0", "false", "off", "no":
		return true
	}
	return false
}
func firewallInputRule(s section) bool {
	destination := false
	for _, value := range s.values["dest"] {
		for _, zone := range strings.Fields(value) {
			if zone == "*" {
				return true
			}
			destination = true
		}
	}
	return !destination
}
func primaryWiFiFields(p parsed) string {
	var out strings.Builder
	for _, s := range p {
		if s.kind == "wifi-device" {
			fmt.Fprintf(&out, "radio:%s:%v;", s.name, s.values)
			continue
		}
		if s.kind != "wifi-iface" {
			continue
		}
		// Known guest/backhaul interfaces do not define the primary client session.
		ifname := one(s, "ifname")
		if strings.Contains(strings.ToLower(ifname), "guest") || strings.HasPrefix(ifname, "bh") || strings.HasSuffix(ifname, ".1") {
			continue
		}
		fmt.Fprintf(&out, "iface:%s:%s;", s.name, ifname)
		for _, k := range []string{"ssid", "key", "encryption", "disabled", "device", "network", "mode"} {
			fmt.Fprintf(&out, "%s=%q;", k, s.values[k])
		}
	}
	return out.String()
}
func activeWiFi(p parsed) int {
	devices := map[string]bool{}
	n := 0
	for _, s := range p {
		if s.kind == "wifi-device" {
			devices[s.name] = !disabled(s)
		}
	}
	for _, s := range p {
		if s.kind == "wifi-iface" && !disabled(s) {
			enabled, known := devices[one(s, "device")]
			if !known || enabled {
				n++
			}
		}
	}
	return n
}

// A whole-document unified hunk is bounded and accurately preserves every changed line.
func diff(module, before, after string) string {
	if before == after {
		return ""
	}
	lines := func(s string) []string {
		if s == "" {
			return nil
		}
		return strings.Split(strings.TrimSuffix(s, "\n"), "\n")
	}
	a, b := lines(before), lines(after)
	var out strings.Builder
	fmt.Fprintf(&out, "--- a/%s\n+++ b/%s\n@@ -1,%d +1,%d @@\n", module, module, len(a), len(b))
	for _, l := range a {
		fmt.Fprintf(&out, "-%s\n", l)
	}
	for _, l := range b {
		fmt.Fprintf(&out, "+%s\n", l)
	}
	if after != "" && !strings.HasSuffix(after, "\n") {
		out.WriteString("\\ No newline at end of file\n")
	}
	return out.String()
}

// Cross-document references use the candidate network/wireless namespace, not
// whichever values happen to remain in live configuration during a commit.
func validateReferences(candidates, combined map[string]string) []Issue {
	network, _ := parse(combined["network"])
	interfaces := map[string]bool{}
	for _, s := range network {
		if s.kind == "interface" && s.name != "" {
			interfaces[s.name] = true
		}
	}
	issues := []Issue{}
	add := func(module, key string) {
		issues = append(issues, issue("invalid_reference", "Unknown "+module+" configuration reference: "+key+"."))
	}
	for module, content := range candidates {
		p, _ := parse(content)
		if module == "wireless" {
			radios := map[string]bool{}
			for _, s := range p {
				if s.kind == "wifi-device" && s.name != "" {
					radios[s.name] = true
				}
			}
			for _, s := range p {
				if s.kind == "wifi-iface" {
					device := one(s, "device")
					if device != "" && !radios[device] {
						add(module, "device")
					}
					for _, v := range s.values["network"] {
						for _, name := range strings.Fields(v) {
							if !interfaces[name] {
								add(module, "network")
							}
						}
					}
				}
			}
		}
		if module == "dhcp" {
			for _, s := range p {
				if s.kind == "dhcp" {
					name := one(s, "interface")
					if name != "" && !interfaces[name] {
						add(module, "interface")
					}
				}
			}
		}
		if module == "firewall" {
			for _, s := range p {
				if s.kind == "zone" {
					for _, v := range s.values["network"] {
						for _, name := range strings.Fields(v) {
							if !interfaces[name] {
								add(module, "network")
							}
						}
					}
				}
			}
		}
	}
	return issues
}

// Preserve factory-owned include sections, but do not turn the raw editor into
// a user-chosen script runner. Project-owned includes require a separate owner.
func validateExecutionChanges(module, before, after string) []Issue {
	a, _ := parse(before)
	b, _ := parse(after)
	if module == "firewall" {
		existing := map[string]int{}
		for _, s := range a {
			if s.kind == "include" {
				existing[fmt.Sprintf("%q:%q", s.name, s.values)]++
			}
		}
		for _, s := range b {
			if s.kind == "include" {
				key := fmt.Sprintf("%q:%q", s.name, s.values)
				if existing[key] == 0 {
					return []Issue{issue("execution_hook_not_allowed", "User-selected firewall script includes are not supported by configuration transactions.")}
				}
				existing[key]--
			}
		}
		for _, remaining := range existing {
			if remaining > 0 {
				return []Issue{issue("factory_include_removed", "Factory-owned firewall includes must be preserved by configuration transactions.")}
			}
		}
	}
	if module == "network" {
		existing := map[string]bool{}
		for _, s := range a {
			if s.kind == "interface" {
				existing[one(s, "proto")] = true
			}
		}
		for _, s := range b {
			if s.kind == "interface" {
				proto := one(s, "proto")
				if proto == "" || existing[proto] {
					continue
				}
				switch proto {
				case "dhcp", "static", "pppoe", "none", "dhcpv6", "l2tp", "pptp":
				default:
					return []Issue{issue("protocol_not_registered", "New network protocols must use a supported native protocol.")}
				}
			}
		}
	}
	return nil
}

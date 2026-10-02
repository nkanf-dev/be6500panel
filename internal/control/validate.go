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
		for k, vals := range s.values {
			key := strings.ToLower(k)
			for _, v := range vals {
				switch key {
				case "port", "src_port", "dest_port", "localport", "remoteport", "external_port", "internal_port":
					if !ports(v) {
						add(k)
					}
				case "ipaddr", "ip6addr", "gateway", "ip6gw", "src_ip", "dest_ip", "ip", "ip6", "dns":
					if !ips(v) {
						add(k)
					}
				case "netmask":
					if !netmask(v) {
						add(k)
					}
				case "mac", "macaddr", "src_mac", "dest_mac":
					for _, m := range strings.Fields(v) {
						if len(m) != 17 {
							add(k)
							break
						}
						if _, err := net.ParseMAC(m); err != nil {
							add(k)
							break
						}
					}
				case "disabled", "enabled", "passwordauth", "rootpasswordauth", "rootlogin":
					if !boolean(v) {
						add(k)
					}
				case "hostname", "domain":
					if !domain(v) {
						add(k)
					}
				case "channel":
					if module == "wireless" && v != "auto" && !integer(v, 1, 233) {
						add(k)
					}
				case "txpower":
					if module == "wireless" && !integer(v, 0, 40) {
						add(k)
					}
				case "start", "limit":
					if module == "dhcp" && !integer(v, 0, 65535) {
						add(k)
					}
				case "mtu":
					if !integer(v, 576, 65535) {
						add(k)
					}
				case "input", "forward", "output":
					if module == "firewall" && s.kind != "rule" && v != "ACCEPT" && v != "REJECT" && v != "DROP" {
						add(k)
					}
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
		if module == "network" {
			ip := net.ParseIP(one(s, "ipaddr"))
			mask := one(s, "netmask")
			if ip != nil && ip.To4() == nil && mask != "" {
				add("netmask")
			}
		}
	}
	return p, errors
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
			add("firewall_policy", "Default or broad firewall policies can block management or forwarding.")
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
		}
		if s.kind == "rule" && (one(s, "target") == "DROP" || one(s, "target") == "REJECT") && (one(s, "dest_ip") == "" || one(s, "src_ip") == "") {
			fmt.Fprintf(&out, "deny:%v;", s.values)
		}
	}
	return out.String()
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
				existing[fmt.Sprintf("%s:%v", s.name, s.values)]++
			}
		}
		for _, s := range b {
			if s.kind == "include" {
				key := fmt.Sprintf("%s:%v", s.name, s.values)
				if existing[key] == 0 {
					return []Issue{issue("execution_hook_not_allowed", "User-selected firewall script includes are not supported by configuration transactions.")}
				}
				existing[key]--
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

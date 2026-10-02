package control

import (
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
)

func nativeFieldDocument(kind, key, value string) string {
	return fmt.Sprintf("config %s 'fixture'\n option %s '%s'\n", kind, key, value)
}

func assertNativeField(t *testing.T, module, kind, key, value string, valid bool) {
	t.Helper()
	_, issues := validate(module, nativeFieldDocument(kind, key, value))
	if got := len(issues) == 0; got != valid {
		t.Fatalf("%s/%s/%s = %q valid=%v, want %v; issues=%#v", module, kind, key, value, got, valid, issues)
	}
	for _, issue := range issues {
		if issue.Code != "invalid_field" {
			t.Fatalf("unexpected field diagnostic: %#v", issue)
		}
	}
}

// These 157 scalar fields and their ranges match field-schema.ts. They are
// explicit native module/section/key tuples, not inferred from key names.
func TestNativeFrontendBooleanMatrix(t *testing.T) {
	groups := []struct{ module, kind, keys string }{
		{"network", "interface", "peerdns defaultroute delegate auto force_link disabled"},
		{"network", "device", "ipv6 stp igmp_snooping multicast_querier bridge_empty vlan_filtering disabled"},
		{"network", "bridge-vlan", "local"},
		{"network", "switch", "reset enable_vlan enable_mirror_rx enable_mirror_tx"},
		{"network", "route", "onlink disabled"},
		{"network", "route6", "onlink disabled"},
		{"network", "rule", "invert disabled"},
		{"network", "rule6", "invert disabled"},
		{"wireless", "wifi-device", "disabled legacy_rates noscan"},
		{"wireless", "wifi-iface", "disabled hidden isolate wds wmm ieee80211r ieee80211k mesh_fwding"},
		{"dhcp", "dnsmasq", "domainneeded boguspriv filterwin2k localise_queries rebind_protection rebind_localhost expandhosts authoritative readethers noresolv nohosts nonwildcard localservice strictorder allservers logqueries logdhcp"},
		{"dhcp", "dhcp", "ignore force dynamicdhcp master ra_slaac"},
		{"dhcp", "host", "dns broadcast"},
		{"dhcp", "odhcpd", "maindhcp"},
		{"firewall", "defaults", "synflood_protect drop_invalid flow_offloading flow_offloading_hw disable_ipv6"},
		{"firewall", "zone", "masq masq6 mtu_fix log enabled"},
		{"firewall", "forwarding", "enabled"},
		{"firewall", "rule", "enabled utc_time"},
		{"firewall", "redirect", "enabled reflection"},
		{"firewall", "nat", "enabled"},
		{"firewall", "include", "enabled reload fw4_compatible"},
		{"firewall", "ipset", "enabled"},
		{"system", "system", "log_remote"},
		{"system", "timeserver", "enabled enable_server use_dhcp"},
		{"system", "led", "default"},
		{"dropbear", "dropbear", "PasswordAuth RootPasswordAuth RootLogin GatewayPorts enable mdns"},
	}
	for _, group := range groups {
		for _, key := range strings.Fields(group.keys) {
			t.Run(group.module+"/"+group.kind+"/"+key, func(t *testing.T) {
				for _, value := range []string{"0", "1", "true", "false", "on", "off", "yes", "no"} {
					assertNativeField(t, group.module, group.kind, key, value, true)
				}
				for _, value := range []string{"2", "maybe", "192.0.2.1", ""} {
					assertNativeField(t, group.module, group.kind, key, value, false)
				}
				assertNativeField(t, group.module, "vendor", key, "vendor-mode", true)
				assertNativeField(t, group.module, group.kind, "vendor_"+key, "vendor-mode", true)
			})
		}
	}
}

func TestNativeFrontendNumberMatrix(t *testing.T) {
	groups := []struct {
		module, kind, keys string
		min, max           int
	}{
		{"network", "interface", "ip6assign", 0, 128},
		{"network", "interface", "mtu", 576, 65535},
		{"network", "interface", "metric demand", 0, 0},
		{"network", "device", "vid", 1, 4094},
		{"network", "device", "mtu", 576, 65535},
		{"network", "device", "ageing_time", 0, 0},
		{"network", "device", "priority", 0, 65535},
		{"network", "bridge-vlan", "vlan", 1, 4094},
		{"network", "switch", "mirror_source_port mirror_monitor_port", 0, 0},
		{"network", "switch_vlan", "vlan", 0, 0},
		{"network", "switch_vlan", "vid", 1, 4094},
		{"network", "route", "metric", 0, 0},
		{"network", "route", "mtu", 576, 65535},
		{"network", "route6", "metric", 0, 0},
		{"network", "route6", "mtu", 576, 65535},
		{"network", "rule", "priority goto", 0, 0},
		{"network", "rule", "suppress_prefixlength", 0, 128},
		{"network", "rule6", "priority goto", 0, 0},
		{"network", "rule6", "suppress_prefixlength", 0, 128},
		{"wireless", "wifi-device", "txpower", 0, 40},
		{"wireless", "wifi-device", "beacon_int", 15, 65535},
		{"wireless", "wifi-device", "distance", 0, 0},
		{"wireless", "wifi-iface", "maxassoc", 0, 0},
		{"wireless", "wifi-iface", "dtim_period", 1, 255},
		{"wireless", "wifi-iface", "auth_port acct_port", 1, 65535},
		{"dhcp", "dnsmasq", "port queryport", 0, 65535},
		{"dhcp", "dnsmasq", "cachesize dnsforwardmax dhcpleasemax", 0, 0},
		{"dhcp", "dnsmasq", "ednspacket_max", 512, 65535},
		{"dhcp", "dhcp", "start limit", 0, 65535},
		{"dhcp", "dhcp", "ra_mininterval ra_maxinterval ra_lifetime", 0, 0},
		{"dhcp", "dhcp", "ra_mtu", 1280, 65535},
		{"dhcp", "odhcpd", "loglevel", 0, 7},
		{"dhcp", "cname", "ttl", 0, 0},
		{"dhcp", "srvhost", "port", 1, 65535},
		{"dhcp", "srvhost", "class weight", 0, 65535},
		{"dhcp", "mxhost", "pref", 0, 65535},
		{"firewall", "rule", "limit_burst", 0, 0},
		{"firewall", "redirect", "limit_burst", 0, 0},
		{"firewall", "nat", "limit_burst", 0, 0},
		{"firewall", "ipset", "maxelem", 1, 0},
		{"firewall", "ipset", "timeout", 0, 0},
		{"system", "system", "log_size", 0, 0},
		{"system", "system", "log_port", 1, 65535},
		{"system", "system", "conloglevel cronloglevel", 0, 8},
		{"system", "led", "delayon delayoff interval", 0, 0},
		{"dropbear", "dropbear", "Port", 1, 65535},
		{"dropbear", "dropbear", "IdleTimeout SSHKeepAlive MaxAuthTries", 0, 0},
	}
	for _, group := range groups {
		for _, key := range strings.Fields(group.keys) {
			t.Run(group.module+"/"+group.kind+"/"+key, func(t *testing.T) {
				assertNativeField(t, group.module, group.kind, key, strconv.Itoa(group.min), true)
				if group.max != 0 {
					assertNativeField(t, group.module, group.kind, key, strconv.Itoa(group.max), true)
					assertNativeField(t, group.module, group.kind, key, strconv.Itoa(group.max+1), false)
				} else {
					assertNativeField(t, group.module, group.kind, key, "65536", true)
				}
				assertNativeField(t, group.module, group.kind, key, strconv.Itoa(group.min-1), false)
				for _, value := range []string{"1.5", "10/second", "auto", "", "18446744073709551616"} {
					assertNativeField(t, group.module, group.kind, key, value, false)
				}
				assertNativeField(t, group.module, "vendor", key, "vendor-mode", true)
				assertNativeField(t, group.module, group.kind, "vendor_"+key, "vendor-mode", true)
			})
		}
	}
}

func TestNativeDHCPSectionSemantics(t *testing.T) {
	for _, test := range []struct {
		kind, key, value string
		valid            bool
	}{
		{"dnsmasq", "port", "0", true},
		{"dnsmasq", "queryport", "0", true},
		{"dnsmasq", "port", "65536", false},
		{"host", "dns", "1", true},
		{"host", "dns", "yes", true},
		{"host", "dns", "192.0.2.53", false},
		{"host", "ip", "ignore", true},
		{"host", "ip", "192.0.2.10", true},
		{"host", "ip", "not-an-ip", false},
		{"domain", "ip", "ignore", false},
		{"dhcp", "dns", "2001:db8::53", true},
		{"dhcp", "dns", "1", false},
		{"dhcp", "limit", "150", true},
		{"dhcp", "limit", "10/second", false},
		{"dnsmasq", "limit", "vendor-rate", true},
		{"srvhost", "port", "0", false},
	} {
		t.Run(test.kind+"/"+test.key+"/"+test.value, func(t *testing.T) {
			assertNativeField(t, "dhcp", test.kind, test.key, test.value, test.valid)
		})
	}
	// This was already accepted. Keep firewall rate syntax independent of DHCP count.
	assertNativeField(t, "firewall", "rule", "limit", "10/second", true)
}

func TestNativeUnknownFieldContextRemainsText(t *testing.T) {
	for _, test := range []struct{ module, kind, key string }{
		{"network", "interface", "Port"},
		{"network", "interface", "port"},
		{"network", "interface", "hostname"},
		{"network", "device", "ipaddr"},
		{"wireless", "wifi-iface", "channel"},
		{"wireless", "wifi-device", "dns"},
		{"dhcp", "host", "limit"},
		{"dhcp", "dnsmasq", "ip"},
		{"firewall", "rule", "input"},
		{"firewall", "zone", "Port"},
		{"system", "system", "PasswordAuth"},
		{"system", "system", "dns"},
		{"dropbear", "dropbear", "port"},
		{"dropbear", "dropbear", "passwordauth"},
	} {
		assertNativeField(t, test.module, test.kind, test.key, "vendor-text", true)
	}
	for _, module := range modules {
		for _, key := range []string{"port", "Port", "ipaddr", "dns", "macaddr", "disabled", "hostname", "mtu", "input", "limit"} {
			assertNativeField(t, module, "vendor", key, "vendor-text", true)
		}
	}
}

func TestNativeStageExistingContentUnrelatedEdit(t *testing.T) {
	existing := []Document{
		{"network", testNetwork + "config device 'bridge'\n option name 'br-lan'\n option type 'bridge'\n list ports 'lan1'\nconfig vendor 'vendor_network'\n option port 'automatic'\n option ipaddr 'factory-managed'\n option disabled 'conditional'\n"},
		{"wireless", testWireless + "config vendor 'vendor_wireless'\n option channel 'factory-auto'\n option txpower 'dynamic'\n option macaddr 'auto'\n"},
		{"dhcp", "config dnsmasq\n option port '0'\n option queryport '0'\n list server '/example.test/192.0.2.53#5353'\nconfig host 'lease'\n option mac '02:00:00:00:00:01'\n option ip 'ignore'\n option dns '1'\nconfig vendor 'vendor_dhcp'\n option limit 'unlimited'\n"},
		{"firewall", "config defaults\n option input 'ACCEPT'\n option output 'ACCEPT'\n option forward 'REJECT'\nconfig rule 'limited'\n option target 'ACCEPT'\n option limit '10/second'\n option src_port '!80 443 1000-2000'\nconfig vendor 'vendor_firewall'\n option input 'factory-chain'\n"},
		{"system", "config system\n option hostname 'fixture-router'\n option log_ip 'syslog.example.test'\nconfig timeserver 'ntp'\n option enabled '1'\n list server 'time.example.test'\nconfig vendor 'vendor_system'\n option hostname 'Factory Router'\n option enabled 'automatic'\n"},
		{"dropbear", "config dropbear\n option Port '22'\n option PasswordAuth 'on'\n option IdleTimeout '0'\n option port 'vendor-socket'\nconfig vendor 'vendor_dropbear'\n option Port 'automatic'\n option RootLogin 'root-policy'\n"},
	}
	for _, document := range existing {
		t.Run(document.Module, func(t *testing.T) {
			f := newFixture(t)
			if err := os.WriteFile(filepath.Join(f.root, "etc", "config", document.Module), []byte(document.Content), 0600); err != nil {
				t.Fatal(err)
			}
			m := openFixture(t, f)
			candidate := document.Content + " option vendor_note 'unrelated edit'\n"
			d := stage(t, m, document.Module, candidate)
			if !d.Valid || len(d.Errors) != 0 || d.Diff == "" {
				t.Fatalf("valid native document blocked: %#v", d)
			}
			if readFixture(t, f, document.Module) != document.Content || len(f.reloads) != 0 {
				t.Fatal("Stage changed live config or reloaded")
			}
			if len(f.calls) != 1 || f.calls[0][0] != "/sbin/uci" || f.calls[0][len(f.calls[0])-1] != document.Module {
				t.Fatalf("native isolated validation bypassed: %v", f.calls)
			}
		})
	}
}

func TestNativeAddressAndCompositeMatrix(t *testing.T) {
	for _, test := range []struct {
		module, kind, key, value string
		valid                    bool
	}{
		{"network", "interface", "ipaddr", "192.0.2.1/24 198.51.100.1/24", true},
		{"network", "interface", "ip6addr", "2001:db8::1/64", true},
		{"network", "interface", "dns", "192.0.2.53 2001:db8::53", true},
		{"network", "interface", "netmask", "24", true},
		{"network", "interface", "netmask", "255.255.255.0", true},
		{"network", "interface", "netmask", "255.0.255.0", false},
		{"network", "interface", "gateway", "not-an-ip", false},
		{"network", "interface", "keepalive", "5 10", true},
		{"network", "interface", "reqprefix", "auto", true},
		{"network", "interface", "reqprefix", "no", true},
		{"network", "device", "ports", "lan1 lan2", true},
		{"network", "bridge-vlan", "ports", "lan1:u* lan2:t", true},
		{"network", "switch_vlan", "ports", "0 1 6t", true},
		{"network", "route", "target", "192.0.2.0/24", true},
		{"network", "route6", "target", "2001:db8::/48", true},
		{"network", "rule", "src", "192.0.2.0/24", true},
		{"network", "rule6", "dest", "2001:db8::/48", true},
		{"network", "globals", "ula_prefix", "fd00:1234:5678::/48", true},
		{"wireless", "wifi-device", "channel", "auto", true},
		{"wireless", "wifi-device", "channel", "233", true},
		{"wireless", "wifi-device", "channel", "234", false},
		{"wireless", "wifi-device", "channel", "automatic", false},
		{"wireless", "wifi-iface", "maclist", "02:00:00:00:00:01 02:00:00:00:00:02", true},
		{"wireless", "wifi-iface", "bssid", "not-a-mac", false},
		{"dhcp", "host", "mac", "02:00:00:*:*:*", true},
		{"dhcp", "host", "mac", "*", true},
		{"dhcp", "host", "mac", "02:00:zz:*:*:*", false},
		{"dhcp", "host", "mac", "02:00:00:00:00:01 02:00:00:00:00:02", true},
		{"dhcp", "host", "hostid", "a0ff", true},
		{"dhcp", "host", "leasetime", "infinite", true},
		{"dhcp", "dnsmasq", "server", "/example.test/192.0.2.53#5353", true},
		{"dhcp", "dnsmasq", "address", "/example.test/192.0.2.1", true},
		{"dhcp", "dnsmasq", "local", "/lan/", true},
		{"dhcp", "dhcp", "leasetime", "12h", true},
		{"dhcp", "dhcp", "dhcp_option", "6,192.0.2.53", true},
		{"dhcp", "cname", "target", "router.lan", true},
		{"dhcp", "boot", "serveraddress", "192.0.2.2", true},
		{"dhcp", "relay", "server_addr", "192.0.2.2", true},
		{"dhcp", "srvhost", "srv", "_sip._tcp.example.test", true},
		{"firewall", "defaults", "input", "ACCEPT", true},
		{"firewall", "defaults", "forward", "DROP", true},
		{"firewall", "zone", "output", "REJECT", true},
		{"firewall", "zone", "output", "ALLOW", false},
		{"firewall", "rule", "src_ip", "!192.0.2.0/24", true},
		{"firewall", "rule", "src_port", "!80 443 1000-2000", true},
		{"firewall", "rule", "dest_port", "80:90,443", true},
		{"firewall", "rule", "dest_port", "65536", false},
		{"firewall", "rule", "dest_port", "9000-8000", false},
		{"firewall", "redirect", "src_dport", "443", true},
		{"firewall", "redirect", "src_dport", "not-a-port", false},
		{"firewall", "nat", "snat_port", "2000-3000", true},
		{"firewall", "nat", "snat_ip", "192.0.2.2", true},
		{"firewall", "rule", "limit", "10/second", true},
		{"firewall", "zone", "log_limit", "10/minute", true},
		{"firewall", "ipset", "entry", "192.0.2.0/24,tcp:443", true},
		{"system", "system", "hostname", "fixture-router", true},
		{"system", "system", "hostname", "name with space", false},
		{"system", "system", "log_ip", "syslog.example.test", true},
		{"system", "system", "timezone", "CST-8", true},
		{"system", "timeserver", "server", "time.example.test", true},
		{"system", "led", "mode", "link tx rx", true},
		{"dropbear", "dropbear", "keyfile", "/etc/dropbear/dropbear_ed25519_host_key", true},
		{"dropbear", "dropbear", "Interface", "lan", true},
	} {
		t.Run(test.module+"/"+test.kind+"/"+test.key+"/"+test.value, func(t *testing.T) {
			assertNativeField(t, test.module, test.kind, test.key, test.value, test.valid)
		})
	}
	_, issues := validate("network", "config interface 'lan'\n list ipaddr '192.0.2.1/24'\n list ipaddr '198.51.100.1/24'\n list dns '192.0.2.53'\n list dns '2001:db8::53'\n")
	if len(issues) != 0 {
		t.Fatalf("native list addresses rejected: %v", issues)
	}
	_, issues = validate("firewall", "config rule\n list src_port '80'\n list src_port '!443'\n list src_ip '192.0.2.0/24'\n list src_ip '!198.51.100.1'\n")
	if len(issues) != 0 {
		t.Fatalf("native list matches rejected: %v", issues)
	}
}

func TestNativeWiFiCredentialsRemainContextual(t *testing.T) {
	for _, test := range []struct {
		kind, encryption, key string
		valid                 bool
	}{
		{"wifi-iface", "psk2", "synthetic-pass", true},
		{"wifi-iface", "psk2+ccmp", "synthetic-pass", true},
		{"wifi-iface", "sae", "synthetic-pass", true},
		{"wifi-iface", "psk2", strings.Repeat("a", 64), true},
		{"wifi-iface", "psk2", "short", false},
		{"wifi-iface", "psk2", strings.Repeat("z", 64), false},
		{"wifi-iface", "wep-open", "1", true},
		{"wifi-iface", "wpa2", "", true},
		{"wifi-iface", "vendor-encryption", "vendor-key", true},
		{"vendor", "psk2", "short", true},
	} {
		content := nativeFieldDocument(test.kind, "encryption", test.encryption) + " option key '" + test.key + "'\n"
		_, issues := validate("wireless", content)
		if (len(issues) == 0) != test.valid {
			t.Fatalf("%s/%s credential valid=%v, want %v", test.kind, test.encryption, len(issues) == 0, test.valid)
		}
	}
	assertNativeField(t, "wireless", "wifi-iface", "ssid", strings.Repeat("a", 33), false)
	assertNativeField(t, "wireless", "wifi-iface", "ssid", strings.Repeat("a", 32), true)
	assertNativeField(t, "wireless", "wifi-device", "ssid", strings.Repeat("a", 33), true)
	assertNativeField(t, "wireless", "wifi-iface", "ssid", strings.Repeat("界", 11), false)
}

func TestNativeStageMalformedKnownFieldsRemainInvalid(t *testing.T) {
	for _, test := range []struct{ module, kind, key, value string }{
		{"network", "interface", "ipaddr", "not-an-ip"},
		{"wireless", "wifi-device", "channel", "999"},
		{"dhcp", "dnsmasq", "port", "65536"},
		{"dhcp", "host", "dns", "not-a-boolean"},
		{"dhcp", "host", "ip", "not-an-ip"},
		{"firewall", "rule", "dest_port", "65536"},
		{"system", "system", "hostname", "name with space"},
		{"dropbear", "dropbear", "Port", "0"},
	} {
		t.Run(test.module+"/"+test.kind+"/"+test.key, func(t *testing.T) {
			f := newFixture(t)
			m := openFixture(t, f)
			before := readFixture(t, f, test.module)
			d := stage(t, m, test.module, nativeFieldDocument(test.kind, test.key, test.value))
			if d.Valid || len(d.Errors) != 1 || d.Errors[0].Code != "invalid_field" {
				t.Fatalf("malformed known field accepted: %#v", d)
			}
			if readFixture(t, f, test.module) != before || len(f.reloads) != 0 {
				t.Fatal("Stage changed live config or reloaded")
			}
		})
	}
}

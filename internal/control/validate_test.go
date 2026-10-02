package control

import (
	"context"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

const firewallDefaults = "config defaults\n option input 'ACCEPT'\n option forward 'REJECT'\n option output 'ACCEPT'\n"
const firewallManagementMatch = " option src 'lan'\n option src_ip '192.0.2.10'\n option dest_ip '192.0.2.1'\n option proto 'tcp'\n option dest_port '8787'\n"
const firewallLANZone = "config zone 'lan_zone'\n option name 'lan'\n option network 'lan'\n option device 'br-lan'\n option input 'ACCEPT'\n option forward 'ACCEPT'\n option output 'ACCEPT'\n"
const firewallVendorInclude = "config include 'vendor'\n option path '/etc/vendor-firewall'\n option type 'script'\n option enabled '1'\n"
const firewallAnonymousInclude = "config include\n option path '/etc/vendor-helper'\n option type 'script'\n"

func firewallIssueCodes(issues []Issue) []string {
	codes := make([]string, len(issues))
	for i, issue := range issues {
		codes[i] = issue.Code
	}
	return codes
}

func TestFirewallRuleConnectivityRisks(t *testing.T) {
	for _, target := range []string{"DROP", "REJECT"} {
		t.Run(target, func(t *testing.T) {
			rule := "config rule 'access'\n option target '" + target + "'\n"
			active := rule + firewallManagementMatch
			disabled := active + " option enabled '0'\n"
			cases := []struct {
				name, before, after string
				wantRisk            bool
			}{
				{"local-source-and-router-destination", "", active, true},
				{"wildcard-destination-zone", "", active + " option dest '*'\n", true},
				{"empty-destination-zone", "", active + " option dest ''\n", true},
				{"wildcard-in-destination-list", "", active + " list dest 'wan'\n list dest '*'\n", true},
				{"local-address-lists", "", rule + " list src_ip '192.0.2.10'\n list src_ip '198.51.100.0/24'\n list dest_ip '192.0.2.1'\n list dest_ip '2001:db8::1'\n", true},
				{"local-inverted-addresses", "", rule + " option src_ip '!192.0.2.20'\n option dest_ip '!192.0.2.2'\n", true},
				{"local-ipv6-addresses", "", rule + " option src_ip '2001:db8::10'\n option dest_ip '2001:db8::1'\n", true},
				{"local-other-service-port", "", strings.ReplaceAll(active, "8787", "22"), true},
				{"local-source-edit", active, strings.ReplaceAll(active, "192.0.2.10", "192.0.2.11"), true},
				{"local-router-destination-edit", active, strings.ReplaceAll(active, "192.0.2.1'", "192.0.2.2'"), true},
				{"local-port-edit", active, strings.ReplaceAll(active, "8787", "443"), true},
				{"local-unmodified", active, active, false},
				{"constrained-forwarding", "", active + " option dest 'wan'\n", false},
				{"constrained-forwarding-destination-edit", active + " option dest 'wan'\n", strings.ReplaceAll(active, "192.0.2.1'", "198.51.100.1'") + " option dest 'wan'\n", false},
				{"constrained-forwarding-zones", "", active + " list dest 'wan'\n list dest 'vpn'\n", false},
				{"broad-forwarding", "", rule + " option src 'lan'\n option dest 'wan'\n", true},
				{"disabled-rule", "", active + " option disabled '1'\n", false},
				{"disabled-rule-edit", active + " option disabled 'yes'\n", strings.ReplaceAll(active, "8787", "443") + " option disabled 'yes'\n", false},
				{"not-enabled-rule", "", disabled, false},
				{"not-enabled-rule-edit", disabled, strings.ReplaceAll(disabled, "192.0.2.10", "192.0.2.11"), false},
				{"enabled-rule-activation", disabled, active + " option enabled '1'\n", true},
				{"disabled-rule-activation", active + " option disabled '1'\n", active + " option disabled '0'\n", true},
			}
			for _, tc := range cases {
				t.Run(tc.name, func(t *testing.T) {
					before, after := firewallDefaults+tc.before, firewallDefaults+tc.after
					if _, issues := validate("firewall", after); len(issues) != 0 {
						t.Fatalf("native fields rejected: %v", issues)
					}
					if issues := validateExecutionChanges("firewall", before, after); len(issues) != 0 {
						t.Fatalf("legitimate rule edit rejected: %v", issues)
					}
					codes := firewallIssueCodes(risk("firewall", before, after))
					want := []string{}
					if tc.wantRisk {
						want = []string{"firewall_policy"}
					}
					if !reflect.DeepEqual(codes, want) {
						t.Fatalf("risk codes %v, want %v", codes, want)
					}
				})
			}
		})
	}
}

func TestFirewallInputAllowExceptionRisks(t *testing.T) {
	base := strings.ReplaceAll(firewallDefaults, "input 'ACCEPT'", "input 'DROP'") + strings.ReplaceAll(firewallLANZone, "input 'ACCEPT'", "input 'DROP'")
	allow := "config rule 'management'\n option target 'ACCEPT'\n" + firewallManagementMatch
	cases := []struct {
		name, before, after string
		wantRisk            bool
	}{
		{"allow-removal-under-drop", allow, "", true},
		{"allow-port-narrowing", strings.ReplaceAll(allow, "8787", "22 8787"), strings.ReplaceAll(allow, "8787", "22"), true},
		{"allow-source-narrowing", strings.ReplaceAll(allow, "192.0.2.10", "192.0.2.0/24"), allow, true},
		{"allow-destination-change", allow, strings.ReplaceAll(allow, "192.0.2.1'", "192.0.2.2'"), true},
		{"wildcard-allow-removal", allow + " option dest '*'\n", "", true},
		{"allow-disabled", allow, allow + " option enabled '0'\n", true},
		{"allow-unchanged", allow, allow, false},
		{"disabled-allow-removal", allow + " option disabled '1'\n", "", false},
		{"disabled-allow-edit", allow + " option enabled '0'\n", strings.ReplaceAll(allow, "8787", "22") + " option enabled '0'\n", false},
		{"scoped-forwarding-allow-removal", allow + " option dest 'wan'\n", "", false},
		{"scoped-forwarding-allow-edit", allow + " option dest 'wan'\n", strings.ReplaceAll(allow, "8787", "22") + " option dest 'wan'\n", false},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if _, issues := validate("firewall", base+tc.after); len(issues) != 0 {
				t.Fatalf("allow exception edit rejected: %v", issues)
			}
			codes := firewallIssueCodes(risk("firewall", base+tc.before, base+tc.after))
			want := []string{}
			if tc.wantRisk {
				want = []string{"firewall_policy"}
			}
			if !reflect.DeepEqual(codes, want) {
				t.Fatalf("risk codes %v, want %v", codes, want)
			}
		})
	}
}

func TestFirewallZoneConnectivityRisks(t *testing.T) {
	cases := []struct {
		name, before, after string
		wantRisk            bool
	}{
		{"network-membership-change", firewallLANZone, strings.ReplaceAll(firewallLANZone, "network 'lan'", "network 'wan'"), true},
		{"network-membership-removal", firewallLANZone, strings.ReplaceAll(firewallLANZone, " option network 'lan'\n", ""), true},
		{"network-membership-addition", firewallLANZone, strings.ReplaceAll(firewallLANZone, "network 'lan'", "network 'lan guest'"), true},
		{"network-list-member-change", strings.ReplaceAll(firewallLANZone, " option network 'lan'", " list network 'lan'\n list network 'guest'"), strings.ReplaceAll(firewallLANZone, " option network 'lan'", " list network 'lan'\n list network 'wan'"), true},
		{"device-membership-change", firewallLANZone, strings.ReplaceAll(firewallLANZone, "device 'br-lan'", "device 'eth1'"), true},
		{"device-membership-removal", firewallLANZone, strings.ReplaceAll(firewallLANZone, " option device 'br-lan'\n", ""), true},
		{"device-list-member-addition", firewallLANZone, strings.ReplaceAll(firewallLANZone, " option device 'br-lan'", " list device 'br-lan'\n list device 'eth1'"), true},
		{"input-policy-drop", firewallLANZone, strings.ReplaceAll(firewallLANZone, "input 'ACCEPT'", "input 'DROP'"), true},
		{"input-policy-reject", firewallLANZone, strings.ReplaceAll(firewallLANZone, "input 'ACCEPT'", "input 'REJECT'"), true},
		{"forward-policy", firewallLANZone, strings.ReplaceAll(firewallLANZone, "forward 'ACCEPT'", "forward 'DROP'"), true},
		{"output-policy", firewallLANZone, strings.ReplaceAll(firewallLANZone, "output 'ACCEPT'", "output 'REJECT'"), true},
		{"unchanged-zone", firewallLANZone, firewallLANZone, false},
		{"unrelated-expert-field", firewallLANZone, firewallLANZone + " option log '1'\n", false},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if _, issues := validate("firewall", tc.after); len(issues) != 0 {
				t.Fatalf("native zone fields rejected: %v", issues)
			}
			codes := firewallIssueCodes(risk("firewall", tc.before, tc.after))
			want := []string{}
			if tc.wantRisk {
				want = []string{"firewall_policy"}
			}
			if !reflect.DeepEqual(codes, want) {
				t.Fatalf("risk codes %v, want %v", codes, want)
			}
		})
	}
}

func TestFirewallForwardingTargetFieldsRemainEditable(t *testing.T) {
	before := firewallDefaults + "config redirect 'service'\n option src 'wan'\n option dest 'lan'\n option src_dport '8080'\n option dest_ip '192.0.2.2'\n option dest_port '80'\n option target 'DNAT'\n"
	after := strings.ReplaceAll(strings.ReplaceAll(before, "192.0.2.2", "192.0.2.3"), "dest_port '80'", "dest_port '443'")
	if _, issues := validate("firewall", after); len(issues) != 0 {
		t.Fatalf("forwarding target validation: %v", issues)
	}
	if issues := validateExecutionChanges("firewall", before, after); len(issues) != 0 {
		t.Fatalf("forwarding target rejected: %v", issues)
	}
	if issues := risk("firewall", before, after); len(issues) != 0 {
		t.Fatalf("scoped forwarding target requires acknowledgment: %v", issues)
	}
}

func TestFirewallFactoryIncludeMultiset(t *testing.T) {
	base := firewallDefaults + firewallVendorInclude + firewallAnonymousInclude
	duplicates := firewallDefaults + firewallAnonymousInclude + firewallAnonymousInclude
	reformattedVendor := "# Preserved factory hook\nconfig include vendor\n option enabled 1\n option type \"script\"\n option path \"/etc/vendor-firewall\"\n"
	cases := []struct {
		name, before, after, wantCode string
	}{
		{"unchanged", base, base, ""},
		{"no-includes", firewallDefaults, firewallDefaults + " option log '1'\n", ""},
		{"section-order-and-formatting", base, firewallAnonymousInclude + reformattedVendor + firewallDefaults, ""},
		{"unrelated-rule-added", base, base + "config rule\n option target 'ACCEPT'\n", ""},
		{"remove-all", base, firewallDefaults, "factory_include_removed"},
		{"remove-named", base, firewallDefaults + firewallAnonymousInclude, "factory_include_removed"},
		{"remove-anonymous", base, firewallDefaults + firewallVendorInclude, "factory_include_removed"},
		{"change-path", base, strings.ReplaceAll(base, "/etc/vendor-firewall", "/tmp/user-script"), "execution_hook_not_allowed"},
		{"change-type", base, strings.ReplaceAll(base, "type 'script'", "type 'nftables'"), "execution_hook_not_allowed"},
		{"disable-vendor", base, strings.ReplaceAll(base, "enabled '1'", "enabled '0'"), "execution_hook_not_allowed"},
		{"remove-vendor-field", base, strings.ReplaceAll(base, " option enabled '1'\n", ""), "execution_hook_not_allowed"},
		{"add-vendor-field", base, strings.ReplaceAll(base, " option enabled '1'\n", " option enabled '1'\n option reload '1'\n"), "execution_hook_not_allowed"},
		{"rename-vendor-section", base, strings.ReplaceAll(base, "include 'vendor'", "include 'renamed'"), "execution_hook_not_allowed"},
		{"add-include", base, base + "config include\n option path '/tmp/user-script'\n", "execution_hook_not_allowed"},
		{"add-identical-include", base, base + firewallAnonymousInclude, "execution_hook_not_allowed"},
		{"duplicates-preserved", duplicates, firewallAnonymousInclude + firewallDefaults + firewallAnonymousInclude, ""},
		{"one-duplicate-removed", duplicates, firewallDefaults + firewallAnonymousInclude, "factory_include_removed"},
		{"duplicate-added", duplicates, duplicates + firewallAnonymousInclude, "execution_hook_not_allowed"},
		{"duplicate-replaced", duplicates, firewallDefaults + firewallAnonymousInclude + firewallVendorInclude, "execution_hook_not_allowed"},
		{"field-value-boundaries", firewallDefaults + firewallAnonymousInclude + " list arguments 'a b'\n", firewallDefaults + firewallAnonymousInclude + " list arguments 'a'\n list arguments 'b'\n", "execution_hook_not_allowed"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if _, issues := validate("firewall", tc.after); len(issues) != 0 {
				t.Fatalf("test candidate is not native UCI: %v", issues)
			}
			issues := validateExecutionChanges("firewall", tc.before, tc.after)
			if tc.wantCode == "" {
				if len(issues) != 0 {
					t.Fatalf("equivalent factory includes rejected: %v", issues)
				}
			} else if len(issues) != 1 || issues[0].Code != tc.wantCode {
				t.Fatalf("issues %v, want %s", issues, tc.wantCode)
			}
		})
	}
}

func TestFirewallManagerConnectivityRequiresAckAndRetainsRollback(t *testing.T) {
	cases := []struct {
		name, before, after string
	}{
		{"management-drop", firewallDefaults, firewallDefaults + "config rule\n option target 'DROP'\n" + firewallManagementMatch},
		{"management-reject", firewallDefaults, firewallDefaults + "config rule\n option target 'REJECT'\n" + firewallManagementMatch},
		{"wildcard-management-drop", firewallDefaults, firewallDefaults + "config rule\n option target 'DROP'\n option dest '*'\n" + firewallManagementMatch},
		{"zone-network-membership", firewallDefaults + firewallLANZone, firewallDefaults + strings.ReplaceAll(firewallLANZone, " option network 'lan'\n", "")},
		{"zone-device-membership", firewallDefaults + firewallLANZone, firewallDefaults + strings.ReplaceAll(firewallLANZone, "device 'br-lan'", "device 'eth1'")},
		{"zone-input-policy", firewallDefaults + firewallLANZone, firewallDefaults + strings.ReplaceAll(firewallLANZone, "input 'ACCEPT'", "input 'REJECT'")},
		{"management-allow-removal", strings.ReplaceAll(firewallDefaults, "input 'ACCEPT'", "input 'DROP'") + "config rule\n option target 'ACCEPT'\n" + firewallManagementMatch, strings.ReplaceAll(firewallDefaults, "input 'ACCEPT'", "input 'DROP'")},
		{"management-allow-port-narrowing", strings.ReplaceAll(firewallDefaults, "input 'ACCEPT'", "input 'DROP'") + "config rule\n option target 'ACCEPT'\n" + strings.ReplaceAll(firewallManagementMatch, "8787", "22 8787"), strings.ReplaceAll(firewallDefaults, "input 'ACCEPT'", "input 'DROP'") + "config rule\n option target 'ACCEPT'\n" + strings.ReplaceAll(firewallManagementMatch, "8787", "22")},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			f := newFixture(t)
			if err := os.WriteFile(filepath.Join(f.root, "etc", "config", "firewall"), []byte(tc.before), 0644); err != nil {
				t.Fatal(err)
			}
			m := openFixture(t, f)
			d := stage(t, m, "firewall", tc.after)
			if !d.Valid || !reflect.DeepEqual(firewallIssueCodes(d.Risks), []string{"firewall_policy"}) {
				t.Fatalf("invalid risk draft: %#v", d)
			}
			if readFixture(t, f, "firewall") != tc.before || len(f.reloads) != 0 {
				t.Fatal("staging changed live firewall")
			}
			_, err := commit(t, m, d, false)
			errorCode(t, err, "risk_acknowledgement_required")
			if readFixture(t, f, "firewall") != tc.before || len(f.reloads) != 0 || m.Status().PendingCommit != nil {
				t.Fatal("refused commit changed live firewall")
			}
			op, err := commit(t, m, d, true)
			if err != nil || op.State != "pending_confirmation" || op.Deadline == nil || m.Status().PendingCommit == nil {
				t.Fatalf("expected provisional firewall commit: %#v %v", op, err)
			}
			if readFixture(t, f, "firewall") != tc.after {
				t.Fatal("provisional firewall was not applied")
			}
			rolled, err := m.Rollback(context.Background(), op.ID)
			if err != nil || rolled.State != "rolled_back" || m.Status().PendingCommit != nil {
				t.Fatalf("pending firewall rollback failed: %#v %v", rolled, err)
			}
			if readFixture(t, f, "firewall") != tc.before || !reflect.DeepEqual(f.reloads, []string{"firewall", "firewall"}) {
				t.Fatal("rollback did not restore and reload the prior firewall")
			}
		})
	}
}

func TestFirewallManagerInvalidIncludesNeverWriteLive(t *testing.T) {
	before := firewallDefaults + firewallVendorInclude + firewallAnonymousInclude + firewallAnonymousInclude
	cases := []struct {
		name, after, wantCode string
	}{
		{"remove-vendor", firewallDefaults + firewallAnonymousInclude + firewallAnonymousInclude, "factory_include_removed"},
		{"remove-one-duplicate", firewallDefaults + firewallVendorInclude + firewallAnonymousInclude, "factory_include_removed"},
		{"change-vendor", strings.ReplaceAll(before, "/etc/vendor-firewall", "/tmp/user-script"), "execution_hook_not_allowed"},
		{"add-include", before + "config include\n option path '/tmp/user-script'\n", "execution_hook_not_allowed"},
		{"add-duplicate", before + firewallAnonymousInclude, "execution_hook_not_allowed"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			f := newFixture(t)
			if err := os.WriteFile(filepath.Join(f.root, "etc", "config", "firewall"), []byte(before), 0644); err != nil {
				t.Fatal(err)
			}
			m := openFixture(t, f)
			d := stage(t, m, "firewall", tc.after)
			if d.Valid || len(d.Errors) != 1 || d.Errors[0].Code != tc.wantCode {
				t.Fatalf("invalid include candidate accepted: %#v", d)
			}
			_, err := commit(t, m, d, true)
			errorCode(t, err, "invalid_candidate")
			if readFixture(t, f, "firewall") != before || len(f.reloads) != 0 || m.Status().PendingCommit != nil {
				t.Fatal("invalid include edit wrote or reloaded live firewall")
			}
		})
	}
}

func TestFirewallManagerSafeEditsCommitWithoutAcknowledgment(t *testing.T) {
	before := firewallDefaults + firewallVendorInclude + firewallAnonymousInclude + firewallAnonymousInclude
	cases := []struct {
		name, after string
	}{
		{"disabled-input-rule", before + "config rule\n option target 'DROP'\n option enabled '0'\n" + firewallManagementMatch},
		{"constrained-forwarding-rule", before + "config rule\n option target 'REJECT'\n option dest 'wan'\n" + firewallManagementMatch},
		{"reordered-preserved-includes", firewallAnonymousInclude + firewallVendorInclude + firewallDefaults + firewallAnonymousInclude + "config rule\n option target 'ACCEPT'\n option dest 'wan'\n"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			f := newFixture(t)
			if err := os.WriteFile(filepath.Join(f.root, "etc", "config", "firewall"), []byte(before), 0644); err != nil {
				t.Fatal(err)
			}
			m := openFixture(t, f)
			d := stage(t, m, "firewall", tc.after)
			if !d.Valid || len(d.Risks) != 0 {
				t.Fatalf("safe native edit rejected or marked risky: %#v", d)
			}
			op, err := commit(t, m, d, false)
			if err != nil || op.State != "committed" || op.Deadline != nil || m.Status().PendingCommit != nil {
				t.Fatalf("safe native edit requires acknowledgment: %#v %v", op, err)
			}
			if readFixture(t, f, "firewall") != tc.after || !reflect.DeepEqual(f.reloads, []string{"firewall"}) {
				t.Fatal("safe native edit did not apply")
			}
		})
	}
}

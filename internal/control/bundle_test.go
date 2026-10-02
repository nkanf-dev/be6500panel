package control

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

const bundleGuestNetwork = "config interface 'guest'\n option proto 'static'\n option ipaddr '192.0.2.1'\n option netmask '255.255.255.0'\n"
const bundleGuestDHCP = "config dhcp 'guest'\n option interface 'guest'\n option start '100'\n option limit '50'\n"

func TestDependentDraftsCommitAsOneAtomicBundle(t *testing.T) {
	f := newFixture(t)
	m := openFixture(t, f)
	before := storageAdmissionLive(t, f)
	network := stage(t, m, "network", testNetwork+bundleGuestNetwork)
	dhcp := stage(t, m, "dhcp", testDHCP+bundleGuestDHCP)
	wireless := stage(t, m, "wireless", testWireless+" option network 'guest'\n")
	firewallText := firewallDefaults + "config zone 'guest'\n option name 'guest'\n list network 'guest'\n option input 'REJECT'\n option forward 'REJECT'\n option output 'ACCEPT'\n"
	firewall := stage(t, m, "firewall", firewallText)
	for _, d := range []Draft{dhcp, wireless, firewall} {
		if !d.Valid || len(d.Errors) != 0 || len(d.Dependencies) == 0 || d.Dependencies[0].Code != "invalid_reference" {
			t.Fatalf("native-valid dependency not selectable: %#v", d)
		}
	}
	if len(f.reloads) != 0 {
		t.Fatal("staging a dependent bundle reloaded live services")
	}
	for module, prior := range before {
		if readFixture(t, f, module) != prior.Content {
			t.Fatalf("staging changed %s", module)
		}
	}
	op, err := m.Commit(context.Background(), CommitRequest{DraftIDs: []string{network.ID, dhcp.ID, wireless.ID, firewall.ID}, Generation: network.Generation, AcknowledgeRisks: true})
	if err != nil || op.State != "pending_confirmation" {
		t.Fatalf("atomic dependency bundle rejected: %#v %v", op, err)
	}
	if readFixture(t, f, "network") != testNetwork+bundleGuestNetwork || readFixture(t, f, "dhcp") != testDHCP+bundleGuestDHCP || readFixture(t, f, "firewall") != firewallText {
		t.Fatal("complete selected bundle was not installed")
	}
	rolled, err := m.Rollback(context.Background(), op.ID)
	if err != nil || rolled.State != "rolled_back" {
		t.Fatalf("bundle recovery failed: %#v %v", rolled, err)
	}
	for module, prior := range before {
		if readFixture(t, f, module) != prior.Content {
			t.Fatalf("bundle recovery did not restore %s", module)
		}
	}
}

func TestDependentDraftNeedsSelectedDependencyBeforeAnyWrite(t *testing.T) {
	f := newFixture(t)
	m := openFixture(t, f)
	network := stage(t, m, "network", testNetwork+bundleGuestNetwork)
	dhcp := stage(t, m, "dhcp", testDHCP+bundleGuestDHCP)
	wrongNetwork := stage(t, m, "network", testNetwork+strings.ReplaceAll(bundleGuestNetwork, "guest", "unrelated"))
	for _, ids := range [][]string{{dhcp.ID}, {wrongNetwork.ID, dhcp.ID}} {
		_, err := m.Commit(context.Background(), CommitRequest{DraftIDs: ids, Generation: network.Generation, AcknowledgeRisks: true})
		errorCode(t, err, "invalid_reference")
		if readFixture(t, f, "network") != testNetwork || readFixture(t, f, "dhcp") != testDHCP || len(f.reloads) != 0 {
			t.Fatal("unresolved selected dependency wrote live documents")
		}
		if _, err := os.Stat(filepath.Join(f.data, "journal.json")); !os.IsNotExist(err) {
			t.Fatal("unresolved selected dependency began a journal")
		}
	}
	if _, err := m.Commit(context.Background(), CommitRequest{DraftIDs: []string{network.ID, dhcp.ID}, Generation: network.Generation, AcknowledgeRisks: true}); err != nil {
		t.Fatalf("rejected subset consumed dependency drafts: %v", err)
	}
}

func TestNativeInvalidityIsNotDeferredAsDependency(t *testing.T) {
	cases := []struct{ module, text, code string }{
		{"dhcp", testDHCP + "config dhcp 'guest'\n option interface 'guest'\n option start 'not-a-number'\n", "invalid_field"},
		{"dhcp", testDHCP + "config dhcp 'guest'\n option interface 'guest\n", "uci_syntax"},
		{"firewall", "config include\n option path '/tmp/user-script'\n", "execution_hook_not_allowed"},
	}
	for _, tc := range cases {
		t.Run(tc.code, func(t *testing.T) {
			f := newFixture(t)
			m := openFixture(t, f)
			d := stage(t, m, tc.module, tc.text)
			if d.Valid || len(d.Errors) == 0 || d.Errors[0].Code != tc.code || len(d.Dependencies) != 0 {
				t.Fatalf("native invalidity was deferred: %#v", d)
			}
			_, err := commit(t, m, d, true)
			errorCode(t, err, "invalid_candidate")
			if len(f.reloads) != 0 {
				t.Fatal("native-invalid draft reloaded live services")
			}
		})
	}
}

func TestNativeUCIRejectionStillBlocksDependentDraft(t *testing.T) {
	f := newFixture(t)
	m := openFixture(t, f)
	f.mu.Lock()
	f.runErr = &Error{Code: "synthetic", Message: "synthetic UCI rejection"}
	f.mu.Unlock()
	d := stage(t, m, "dhcp", testDHCP+bundleGuestDHCP)
	if d.Valid || len(d.Errors) != 1 || d.Errors[0].Code != "uci_validation_failed" || len(d.Dependencies) != 0 {
		t.Fatalf("UCI invalidity was deferred: %#v", d)
	}
}

package control

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// Every fixture operation stays in private temporary roots. No host UCI reload runs.
func installReverseReferenceFixture(t *testing.T, f *fixture, module, content string) {
	t.Helper()
	if err := os.WriteFile(filepath.Join(f.root, "etc", "config", module), []byte(content), 0644); err != nil {
		t.Fatal(err)
	}
}

func TestCommitRejectsBrokenReferencesInUnselectedDocumentsBeforeJournalOrWrite(t *testing.T) {
	for _, tc := range []struct{ name, module, content string }{
		{"dhcp", "dhcp", testDHCP + bundleGuestDHCP},
		{"wireless", "wireless", testWireless + " option network 'guest'\n"},
		{"firewall", "firewall", firewallDefaults + "config zone 'guest'\n option name 'guest'\n list network 'guest'\n"},
		{"vendor baseline does not mask new break", "dhcp", testDHCP + "config dhcp 'vendor'\n option interface 'vendor_missing'\n" + bundleGuestDHCP},
		{"wireless field list", "wireless", testWireless + " option network 'lan guest'\n"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			f := newFixture(t)
			installReverseReferenceFixture(t, f, "network", testNetwork+bundleGuestNetwork)
			installReverseReferenceFixture(t, f, tc.module, tc.content)
			m := openFixture(t, f)
			draft := stage(t, m, "network", testNetwork)
			before := storageAdmissionLive(t, f)
			_, err := commit(t, m, draft, true)
			errorCode(t, err, "invalid_reference")
			if m.Status().Operation != nil || len(f.reloads) != 0 {
				t.Fatal("broken reverse reference began a transaction or reload")
			}
			for module, prior := range before {
				if readFixture(t, f, module) != prior.Content {
					t.Fatalf("broken reverse reference changed %s", module)
				}
			}
			if _, err := os.Stat(filepath.Join(f.data, "journal.json")); !os.IsNotExist(err) {
				t.Fatal("broken reverse reference persisted a journal")
			}
			drafts, err := m.Drafts(context.Background())
			if err != nil || len(drafts) != 1 || drafts[0].ID != draft.ID {
				t.Fatal("rejected commit consumed the draft")
			}
		})
	}
}

func TestCommitAllowsDependencyRemovalWhenEveryAffectedReferenceIsSelected(t *testing.T) {
	f := newFixture(t)
	installReverseReferenceFixture(t, f, "network", testNetwork+bundleGuestNetwork)
	installReverseReferenceFixture(t, f, "dhcp", testDHCP+bundleGuestDHCP)
	installReverseReferenceFixture(t, f, "wireless", testWireless+" option network 'guest'\n")
	installReverseReferenceFixture(t, f, "firewall", firewallDefaults+"config zone 'guest'\n option name 'guest'\n list network 'guest'\n")
	m := openFixture(t, f)
	network := stage(t, m, "network", testNetwork)
	dhcp := stage(t, m, "dhcp", testDHCP)
	wireless := stage(t, m, "wireless", testWireless)
	firewall := stage(t, m, "firewall", firewallDefaults)
	op, err := m.Commit(context.Background(), CommitRequest{DraftIDs: []string{network.ID, dhcp.ID, wireless.ID, firewall.ID}, Generation: network.Generation, AcknowledgeRisks: true})
	if err != nil || op.State != "pending_confirmation" {
		t.Fatalf("consistent selected removal bundle rejected: %#v %v", op, err)
	}
	if readFixture(t, f, "network") != testNetwork || readFixture(t, f, "dhcp") != testDHCP {
		t.Fatal("selected removal bundle not installed")
	}
}

func TestCommitPreservesUnchangedVendorReferencesOutsideSelectedDocuments(t *testing.T) {
	for _, changedModule := range []string{"system", "network"} {
		t.Run(changedModule, func(t *testing.T) {
			f := newFixture(t)
			installReverseReferenceFixture(t, f, "dhcp", testDHCP+"config dhcp 'vendor'\n option interface 'vendor_missing'\n")
			installReverseReferenceFixture(t, f, "wireless", testWireless+" option network 'vendor_missing'\n")
			installReverseReferenceFixture(t, f, "firewall", firewallDefaults+"config zone 'vendor'\n option name 'vendor'\n list network 'vendor_missing'\n")
			m := openFixture(t, f)
			var text string
			if changedModule == "system" {
				text = "config system\n option hostname 'new-fixture'\n"
			} else {
				text = strings.ReplaceAll(testNetwork, "192.168.31.1", "192.168.32.1")
			}
			d := stage(t, m, changedModule, text)
			_, err := commit(t, m, d, true)
			if err != nil {
				t.Fatalf("untouched vendor baseline blocked unrelated commit: %v", err)
			}
			if readFixture(t, f, "dhcp") != testDHCP+"config dhcp 'vendor'\n option interface 'vendor_missing'\n" {
				t.Fatal("untouched vendor document changed")
			}
		})
	}
}

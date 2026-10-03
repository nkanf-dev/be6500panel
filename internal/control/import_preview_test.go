package control

import (
	"context"
	"os"
	"path/filepath"
	"reflect"
	"testing"
)

func TestImportPreviewPureValidationAndDependencySet(t *testing.T) {
	f := newFixture(t)
	m := openFixture(t, f)
	current, err := m.Documents(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	before := storageAdmissionLive(t, f)
	selected := []Document{{Module: "network", Content: testNetwork + bundleGuestNetwork}, {Module: "dhcp", Content: testDHCP + bundleGuestDHCP}}
	preview := PreviewDocuments(current.Documents, selected)
	if len(preview) != 2 {
		t.Fatal(preview)
	}
	for _, draft := range preview {
		if !draft.Valid || len(draft.Errors) > 0 || len(draft.Dependencies) > 0 || draft.ID != "" || draft.Diff == "" {
			t.Fatalf("pure selected bundle %#v", draft)
		}
	}
	partial := PreviewDocuments(current.Documents, selected[1:])
	if !partial[0].Valid || len(partial[0].Dependencies) == 0 {
		t.Fatalf("missing selected dependency %#v", partial)
	}
	bad := PreviewDocuments(current.Documents, []Document{{Module: "dhcp", Content: "option start '1'\n"}, {Module: "firewall", Content: "config include\n option path '/tmp/script'\n"}})
	if bad[0].Valid || bad[0].Errors[0].Code != "uci_syntax" || bad[1].Valid || bad[1].Errors[0].Code != "execution_hook_not_allowed" {
		t.Fatal(bad)
	}
	drafts, err := m.Drafts(context.Background())
	if err != nil || len(drafts) != 0 {
		t.Fatalf("preview staged %#v %v", drafts, err)
	}
	for module, prior := range before {
		if readFixture(t, f, module) != prior.Content {
			t.Fatalf("preview wrote %s", module)
		}
	}
	if len(f.reloads) > 0 {
		t.Fatal("preview reloaded native services")
	}
	entries, err := os.ReadDir(f.data)
	if err != nil {
		t.Fatal(err)
	}
	for _, entry := range entries {
		if entry.IsDir() || entry.Name() == "journal.json" {
			t.Fatalf("preview created side effect %s", filepath.Join(f.data, entry.Name()))
		}
	}
}
func TestImportPreviewUnknownVendorTextUnchanged(t *testing.T) {
	before := "# vendor data\nconfig custom 'unknown'\n option vendor_extension 'unmodeled values'\n"
	after := before + " option another 'preserved'\n"
	preview := PreviewDocuments([]Document{{Module: "system", Content: before}}, []Document{{Module: "system", Content: after}})
	if !preview[0].Valid || preview[0].Diff == "" || len(preview[0].Errors) > 0 {
		t.Fatal(preview)
	}
}

func TestImportNetworkPreviewChecksNewReverseReferencesWithoutRejectingExistingVendorData(t *testing.T) {
	live := []Document{
		{Module: "network", Content: "config interface 'lan'\nconfig interface 'guest'\n"},
		{Module: "wireless", Content: "config wifi-device 'radio'\nconfig wifi-iface\n option device 'radio'\n option network 'lan already_unknown'\n"},
		{Module: "dhcp", Content: "config dhcp\n option interface 'guest'\n"},
		{Module: "firewall", Content: "config zone\n list network 'lan'\n"},
	}
	preview := PreviewDocuments(live, []Document{{Module: "network", Content: "config interface 'lan'\n"}})
	if !preview[0].Valid || len(preview[0].Dependencies) != 1 || preview[0].Dependencies[0].Code != "invalid_reference" {
		t.Fatal("orphaned unchanged DHCP reference not detected", preview)
	}
	preview = PreviewDocuments(live, []Document{{Module: "network", Content: "config interface 'lan'\n"}, {Module: "dhcp", Content: ""}})
	if len(preview[0].Dependencies) > 0 {
		t.Fatal("selected reference removal should resolve bundle", preview)
	}
	preview = PreviewDocuments(live, []Document{{Module: "network", Content: "config interface 'guest'\n"}})
	if len(preview[0].Dependencies) != 2 {
		t.Fatal("existing unknown reference masked newly broken wireless/firewall reference", preview)
	}
	preview = PreviewDocuments(live, []Document{{Module: "network", Content: live[0].Content + "config interface 'extra'\n"}})
	if len(preview[0].Dependencies) > 0 {
		t.Fatal("existing unknown vendor reference must not block unrelated network import", preview)
	}
}

func TestImportPreviewDiagnosticsRemainDeterministic(t *testing.T) {
	candidates := []Document{{Module: "network", Content: "config interface 'lan'\n option mtu 'invalid'\n option ipaddr 'invalid'\n option metric 'invalid'\n"}}
	first := PreviewDocuments(nil, candidates)
	for i := 0; i < 40; i++ {
		next := PreviewDocuments(nil, candidates)
		if !reflect.DeepEqual(first, next) {
			t.Fatalf("nondeterministic diagnostics %#v %#v", first, next)
		}
	}
}

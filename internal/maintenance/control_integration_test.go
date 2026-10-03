package maintenance

import (
	"be6500panel/internal/control"
	"context"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

func TestRealControlImportStagesSelectedBundleWithSharedAdmissionAndNoLiveApply(t *testing.T) {
	root := t.TempDir()
	data := filepath.Join(t.TempDir(), "private-control")
	live := filepath.Join(root, "etc", "config")
	if err := os.MkdirAll(live, 0700); err != nil {
		t.Fatal(err)
	}
	initial := map[string]string{
		"network":  "config interface 'lan'\n option proto 'static'\n option ipaddr '192.0.2.1'\n",
		"wireless": "config wifi-device 'radio0'\nconfig wifi-iface 'main'\n option device 'radio0'\n option network 'lan'\n",
		"dhcp":     "config dhcp 'lan'\n option interface 'lan'\n",
		"firewall": "config defaults\n option input 'ACCEPT'\n",
		"system":   "config system\n option hostname 'synthetic'\n",
		"dropbear": "config dropbear\n option Port '22'\n",
	}
	for module, text := range initial {
		if err := os.WriteFile(filepath.Join(live, module), []byte(text), 0600); err != nil {
			t.Fatal(err)
		}
	}
	admitted, commands, reloaded := 0, 0, 0
	manager, err := control.New(control.Options{Root: root, DataDir: data, Runner: func(ctx context.Context, bin string, args ...string) ([]byte, error) {
		commands++
		if bin != "/sbin/uci" || len(args) != 7 || args[0] != "-s" || args[1] != "-c" || !strings.HasPrefix(args[2], data+string(filepath.Separator)) || args[3] != "-P" || args[2] != args[4] || args[5] != "show" {
			t.Fatalf("unexpected native command %s %#v", bin, args)
		}
		return nil, nil
	}, Reload: func(context.Context, string) error { reloaded++; return nil }, StorageAdmission: func(ctx context.Context, path string, bytes int64, recovery bool) (func(), error) {
		admitted++
		return func() {}, nil
	}})
	if err != nil {
		t.Fatal(err)
	}
	defer manager.Close()
	service := New(Options{Native: manager, Metadata: func(context.Context) (Metadata, error) { return Metadata{Model: "RN02", Build: "test"}, nil }})
	envelope := backupEnvelope(t, service, "network", "dhcp")
	envelope.Documents[0].Content += "config interface 'guest'\n option proto 'static'\n option ipaddr '198.51.100.1'\n"
	envelope.Documents[1].Content += "config dhcp 'guest'\n option interface 'guest'\n"
	beforeEntries, err := os.ReadDir(data)
	if err != nil {
		t.Fatal(err)
	}
	beforeAdmission := admitted
	p, err := service.Preview(context.Background(), encode(t, envelope))
	if err != nil {
		t.Fatal(err)
	}
	afterEntries, err := os.ReadDir(data)
	if err != nil {
		t.Fatal(err)
	}
	if commands != 0 || admitted != beforeAdmission || !reflect.DeepEqual(beforeEntries, afterEntries) {
		t.Fatal("preview executed native or allocated private storage")
	}
	result, err := service.Stage(context.Background(), StageRequest{PreviewID: p.ID, Generation: p.Generation, Modules: []string{"network", "dhcp"}})
	if err != nil || len(result.Drafts) != 2 {
		t.Fatalf("%#v %v", result, err)
	}
	if commands != 2 || admitted <= beforeAdmission || reloaded != 0 {
		t.Fatalf("stage admission=%d commands=%d reloads=%d", admitted, commands, reloaded)
	}
	if !result.Drafts[1].Valid || len(result.Drafts[1].Dependencies) == 0 {
		t.Fatal("dependent native draft not preserved for selected commit", result.Drafts)
	}
	for module, text := range initial {
		raw, err := os.ReadFile(filepath.Join(live, module))
		if err != nil || string(raw) != text {
			t.Fatalf("Stage applied %s", module)
		}
	}
	for _, draft := range result.Drafts {
		if draft.Generation != p.Generation || draft.ID == "" {
			t.Fatal("wrong stage CAS", draft)
		}
	}
	if _, err := os.Stat(filepath.Join(data, "journal.json")); !os.IsNotExist(err) {
		t.Fatal("Stage began Apply transaction")
	}
	info, err := os.Stat(filepath.Join(data, "state.json"))
	if err != nil || info.Mode().Perm() != 0600 {
		t.Fatalf("private draft state permissions %v %v", info, err)
	}
}

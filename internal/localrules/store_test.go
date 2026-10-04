package localrules

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"sync"
	"testing"

	"be6500panel/internal/proxy"
	"be6500panel/internal/storage"
)

func draftPolicy(value string) proxy.Policy {
	rule := proxy.Rule{Kind: proxy.RuleDomain, Value: value, Target: proxy.TargetDirect}
	return proxy.Policy{Rules: []proxy.LocalRule{{ID: "gpt-direct", Enabled: true, Label: "GPT direct", Rule: rule}},
		SubscriptionEdits: []proxy.SubscriptionEdit{{ID: "edit", SourceFingerprint: strings.Repeat("0", 64) + ":1", Replacement: &rule}}}
}

func newTestStore(t *testing.T) *Store {
	t.Helper()
	s, err := New(Options{DataDir: t.TempDir()})
	if err != nil {
		t.Fatal(err)
	}
	return s
}

func TestStoreRoundTripPrivateIndependentAndOwned(t *testing.T) {
	dir := t.TempDir()
	unrelated := filepath.Join(dir, "main-config.json")
	if err := os.WriteFile(unrelated, []byte("private-main-config"), 0600); err != nil {
		t.Fatal(err)
	}
	var charged int64
	var released bool
	s, err := New(Options{DataDir: dir, StorageAdmission: func(ctx context.Context, path string, bytes int64, recovery bool) (func(), error) {
		if path != dir || recovery || ctx == nil {
			t.Fatal("wrong storage admission", path, recovery)
		}
		charged = bytes
		return func() { released = true }, nil
	}})
	if err != nil || len(s.Snapshot().Policy.Rules) != 0 || len(s.Snapshot().Revision) != 64 {
		t.Fatal("empty store", err)
	}
	policy := draftPolicy("gpt.kanglives.top")
	accepted, err := s.Save(context.Background(), policy)
	if err != nil || !released {
		t.Fatal("save", err)
	}
	path := filepath.Join(dir, FileName)
	raw, _ := os.ReadFile(path)
	info, _ := os.Stat(path)
	directory, _ := os.Stat(dir)
	if charged != int64(len(raw)) || info.Mode().Perm() != 0600 || directory.Mode().Perm() != 0700 {
		t.Fatal("private mode or full admission missing")
	}
	var fields map[string]json.RawMessage
	if json.Unmarshal(raw, &fields) != nil || len(fields) != 2 || fields["rules"] == nil || fields["subscriptionEdits"] == nil {
		t.Fatal("file must contain independent policy only", string(raw))
	}
	before := s.Snapshot()
	policy.Rules[0].Rule.Value = "caller.example"
	policy.SubscriptionEdits[0].Replacement.Value = "caller.example"
	accepted.Policy.Rules[0].Rule.Value = "output.example"
	accepted.Policy.SubscriptionEdits[0].Replacement.Value = "output.example"
	if !reflect.DeepEqual(s.Snapshot(), before) {
		t.Fatal("snapshot aliases caller or return slices/pointers")
	}
	readback, err := New(Options{DataDir: dir})
	if err != nil || !reflect.DeepEqual(readback.Snapshot(), before) {
		t.Fatal("restart readback", err)
	}
	main, _ := os.ReadFile(unrelated)
	if string(main) != "private-main-config" {
		t.Fatal("draft save changed main config")
	}
	cleared, err := s.Save(context.Background(), proxy.Policy{})
	if err != nil || len(cleared.Policy.Rules) != 0 || cleared.Policy.Rules == nil || cleared.Policy.SubscriptionEdits == nil {
		t.Fatal("clear overlay", err, cleared)
	}
}

func TestStorePrecommitFailurePreservesAcceptedPolicyAndDisk(t *testing.T) {
	for _, step := range []string{"admission", "write", "short-write", "file-sync", "directory-sync", "rename", "cancel-after-write"} {
		t.Run(step, func(t *testing.T) {
			s := newTestStore(t)
			old, err := s.Save(context.Background(), draftPolicy("old.example"))
			if err != nil {
				t.Fatal(err)
			}
			path := filepath.Join(s.dir, FileName)
			oldRaw, _ := os.ReadFile(path)
			private := errors.New("secret disk pathname or private content")
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			switch step {
			case "admission":
				s.admission = func(context.Context, string, int64, bool) (func(), error) { return nil, storage.ErrInsufficientSpace }
			case "write":
				s.write = func(*os.File, []byte) (int, error) { return 0, private }
			case "short-write":
				s.write = func(f *os.File, raw []byte) (int, error) { return f.Write(raw[:1]) }
			case "file-sync":
				s.syncFile = func(*os.File) error { return private }
			case "directory-sync":
				s.syncDir = func(*os.File) error { return private }
			case "rename":
				s.rename = func(string, string) error { return private }
			case "cancel-after-write":
				s.write = func(f *os.File, raw []byte) (int, error) { n, err := f.Write(raw); cancel(); return n, err }
			}
			got, err := s.Save(ctx, draftPolicy("new.example"))
			current, _ := os.ReadFile(path)
			if err == nil || strings.Contains(err.Error(), "secret") || !reflect.DeepEqual(got, old) || !reflect.DeepEqual(s.Snapshot(), old) || string(current) != string(oldRaw) {
				t.Fatal("failed save erased old policy", err, got)
			}
			files, _ := filepath.Glob(filepath.Join(s.dir, ".local-proxy-rules-*"))
			if len(files) != 0 {
				t.Fatal("failed temporary leaked", files)
			}
		})
	}
}

func TestStorePostCommitSyncReturnsActualCommittedSnapshot(t *testing.T) {
	s := newTestStore(t)
	if _, err := s.Save(context.Background(), draftPolicy("old.example")); err != nil {
		t.Fatal(err)
	}
	calls := 0
	s.syncDir = func(f *os.File) error {
		calls++
		if calls == 2 {
			return errors.New("private fsync failure")
		}
		return f.Sync()
	}
	got, err := s.Save(context.Background(), draftPolicy("new.example"))
	if !errors.Is(err, ErrStorage) || got.Policy.Rules[0].Rule.Value != "new.example" || !reflect.DeepEqual(got, s.Snapshot()) {
		t.Fatal("post-rename error falsely reports old policy", err, got)
	}
	readback, readErr := New(Options{DataDir: s.dir})
	if readErr != nil || !reflect.DeepEqual(got, readback.Snapshot()) {
		t.Fatal("postcommit readback mismatch", readErr)
	}
}

func TestStoreRejectsSymlinkNonregularMalformedBoundedFiles(t *testing.T) {
	for _, kind := range []string{"symlink", "directory", "oversized", "invalid-json", "duplicate-fields", "unknown-field", "null-rules", "null-document", "invalid-policy", "bad-reference", "deep"} {
		t.Run(kind, func(t *testing.T) {
			dir := t.TempDir()
			path := filepath.Join(dir, FileName)
			var raw []byte
			switch kind {
			case "symlink":
				outside := filepath.Join(t.TempDir(), "secret.json")
				if err := os.WriteFile(outside, []byte(`{"rules":[],"subscriptionEdits":[]}`), 0600); err != nil {
					t.Fatal(err)
				}
				if err := os.Symlink(outside, path); err != nil {
					t.Fatal(err)
				}
			case "directory":
				if err := os.Mkdir(path, 0700); err != nil {
					t.Fatal(err)
				}
			case "oversized":
				raw = []byte(strings.Repeat("x", MaxFileBytes+1))
			case "invalid-json":
				raw = []byte(`{"secret":`)
			case "duplicate-fields":
				raw = []byte(`{"rules":[],"rules":[],"subscriptionEdits":[]}`)
			case "unknown-field":
				raw = []byte(`{"rules":[],"subscriptionEdits":[],"secret":"credential"}`)
			case "null-rules":
				raw = []byte(`{"rules":null,"subscriptionEdits":[]}`)
			case "null-document":
				raw = []byte(`null`)
			case "invalid-policy":
				policy := draftPolicy("https://secret.example")
				raw, _ = json.Marshal(policy)
			case "bad-reference":
				policy := draftPolicy("example.com")
				policy.SubscriptionEdits[0].SourceFingerprint = "0"
				raw, _ = json.Marshal(policy)
			case "deep":
				raw = []byte(strings.Repeat("[", 20) + "0" + strings.Repeat("]", 20))
			}
			if raw != nil {
				if err := os.WriteFile(path, raw, 0600); err != nil {
					t.Fatal(err)
				}
			}
			_, err := New(Options{DataDir: dir})
			if !errors.Is(err, ErrStorage) || strings.Contains(err.Error(), "secret") || strings.Contains(err.Error(), dir) {
				t.Fatal("unsafe document error", err)
			}
		})
	}
	dir := t.TempDir()
	link := filepath.Join(t.TempDir(), "link")
	if err := os.Symlink(dir, link); err != nil {
		t.Fatal(err)
	}
	if _, err := New(Options{DataDir: link}); !errors.Is(err, ErrStorage) {
		t.Fatal("directory symlink accepted", err)
	}
}

func TestLoadFailureAndSymlinkSavePreserveOldSnapshot(t *testing.T) {
	s := newTestStore(t)
	old, err := s.Save(context.Background(), draftPolicy("old.example"))
	if err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(s.dir, FileName)
	if err := os.WriteFile(path, []byte("malformed private document"), 0600); err != nil {
		t.Fatal(err)
	}
	if _, err := s.Load(context.Background()); !errors.Is(err, ErrStorage) || !reflect.DeepEqual(s.Snapshot(), old) {
		t.Fatal("failed load cleared state", err)
	}
	if err := os.Remove(path); err != nil {
		t.Fatal(err)
	}
	outside := filepath.Join(t.TempDir(), "outside")
	if err := os.WriteFile(outside, []byte("outside"), 0600); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(outside, path); err != nil {
		t.Fatal(err)
	}
	if _, err := s.Save(context.Background(), draftPolicy("new.example")); !errors.Is(err, ErrStorage) || !reflect.DeepEqual(s.Snapshot(), old) {
		t.Fatal("symlink save changed state", err)
	}
	raw, _ := os.ReadFile(outside)
	if string(raw) != "outside" {
		t.Fatal("symlink destination changed")
	}
}

func TestStoreValidatesLimitsAndSizeBeforeAdmissionOrWrite(t *testing.T) {
	s := newTestStore(t)
	calls := 0
	s.admission = func(context.Context, string, int64, bool) (func(), error) { calls++; return func() {}, nil }
	invalid := draftPolicy("example.com")
	invalid.Rules[0].ID = "secret/id"
	if _, err := s.Save(context.Background(), invalid); !errors.Is(err, proxy.ErrInvalidPolicy) || calls != 0 {
		t.Fatal("invalid policy reached storage", err)
	}
	large := proxy.Policy{}
	for i := 0; i < proxy.MaxLocalRules; i++ {
		large.Rules = append(large.Rules, proxy.LocalRule{ID: fmt.Sprintf("r%d", i), Label: strings.Repeat("界", 64), Note: strings.Repeat("界", 256), Rule: proxy.Rule{Kind: proxy.RuleDomain, Value: "example.com", Target: proxy.TargetDirect}})
	}
	if proxy.ValidatePolicy(large) != nil {
		t.Fatal("large typed policy unexpectedly invalid")
	}
	if _, err := s.Save(context.Background(), large); !errors.Is(err, ErrDocumentSize) || calls != 0 {
		t.Fatal("file size bound absent or after admission", err)
	}
	if _, err := os.Stat(filepath.Join(s.dir, FileName)); !errors.Is(err, os.ErrNotExist) {
		t.Fatal("invalid candidate created file", err)
	}
	if _, err := s.Save(nil, proxy.Policy{}); !errors.Is(err, ErrInvalidInput) {
		t.Fatal("nil context accepted", err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := s.Save(ctx, proxy.Policy{}); !errors.Is(err, context.Canceled) || calls != 0 {
		t.Fatal("canceled candidate wrote", err)
	}
	if _, err := New(Options{}); !errors.Is(err, ErrInvalidInput) {
		t.Fatal("unprovided data directory accepted", err)
	}
}

func TestStoreConcurrentSnapshotsAndSaves(t *testing.T) {
	s := newTestStore(t)
	var wg sync.WaitGroup
	for i := 0; i < 4; i++ {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			for j := 0; j < 5; j++ {
				if _, err := s.Save(context.Background(), draftPolicy(fmt.Sprintf("d%d-%d.example", i, j))); err != nil {
					t.Error(err)
				}
				snapshot := s.Snapshot()
				if len(snapshot.Policy.Rules) != 1 {
					t.Error("partial snapshot")
				}
				snapshot.Policy.Rules[0].Rule.Value = "caller.example"
			}
		}(i)
	}
	wg.Wait()
	if _, err := s.Load(context.Background()); err != nil {
		t.Fatal(err)
	}
}

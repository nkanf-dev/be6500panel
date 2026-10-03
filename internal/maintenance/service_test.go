package maintenance

import (
	"be6500panel/internal/control"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"testing"
	"time"
)

type fakeNative struct {
	generation           uint64
	documents            []control.Document
	pending              bool
	stages               []control.StageRequest
	drafts               map[string]control.Draft
	deletes              []string
	failAt               int
	invalidAt            int
	cancelAt             int
	cancel               context.CancelFunc
	cleanupFailure       bool
	cleanupAbsent        bool
	generationAfterStage bool
}

func (f *fakeNative) Documents(ctx context.Context) (control.DocumentSet, error) {
	if err := ctx.Err(); err != nil {
		return control.DocumentSet{}, err
	}
	generation := f.generation
	if f.generationAfterStage && len(f.stages) > 0 {
		generation++
	}
	docs := control.DocumentSet{Generation: generation, Documents: append([]control.Document{}, f.documents...)}
	if f.pending {
		docs.PendingCommit = &control.PendingCommit{ID: "synthetic", Deadline: time.Now()}
	}
	return docs, nil
}
func (f *fakeNative) Stage(ctx context.Context, r control.StageRequest) (control.Draft, error) {
	if err := ctx.Err(); err != nil {
		return control.Draft{}, err
	}
	f.stages = append(f.stages, r)
	if len(f.stages) == f.failAt {
		return control.Draft{}, &control.Error{Code: "storage_insufficient", Message: "Synthetic storage rejection."}
	}
	d := control.Draft{ID: fmt.Sprintf("owned-%d", len(f.stages)), Module: r.Module, Generation: r.Generation, Valid: len(f.stages) != f.invalidAt, Errors: []control.Issue{}, Risks: []control.Issue{}}
	f.drafts[d.ID] = d
	if len(f.stages) == f.cancelAt {
		f.cancel()
	}
	return d, nil
}
func (f *fakeNative) DeleteDraft(ctx context.Context, id string) error {
	if ctx.Err() != nil {
		return ctx.Err()
	}
	f.deletes = append(f.deletes, id)
	if f.cleanupAbsent {
		delete(f.drafts, id)
		return &control.Error{Code: "draft_not_found", Message: "Synthetic draft already absent."}
	}
	if f.cleanupFailure {
		return &control.Error{Code: "storage_failed", Message: "Synthetic cleanup failure."}
	}
	delete(f.drafts, id)
	return nil
}
func fixture(t *testing.T) (*Service, *fakeNative, *time.Time) {
	t.Helper()
	f := &fakeNative{generation: 7, drafts: map[string]control.Draft{}, documents: []control.Document{
		{Module: "network", Content: "config interface 'lan'\n option proto 'static'\n option ipaddr '192.0.2.1'\n"},
		{Module: "wireless", Content: "config wifi-device 'radio0'\n option channel 'auto'\nconfig wifi-iface 'main'\n option device 'radio0'\n option network 'lan'\n option key 'fakewifi123'\n"},
		{Module: "dhcp", Content: "config dhcp 'lan'\n option interface 'lan'\n option start '100'\n"},
		{Module: "firewall", Content: "config defaults\n option input 'ACCEPT'\n"},
		{Module: "system", Content: "# vendor comment\nconfig system\n option hostname 'synthetic'\n option vendor_value 'unchanged'\n"},
		{Module: "dropbear", Content: "config dropbear\n option Port '22'\n"},
	}}
	now := time.Date(2026, 10, 3, 0, 0, 0, 0, time.UTC)
	s := New(Options{Native: f, Metadata: func(context.Context) (Metadata, error) { return Metadata{Model: "RN02", Build: "test-build"}, nil }, Runtime: func(ctx context.Context, service string) (RuntimeDocument, error) {
		if service == "frpc" {
			return RuntimeDocument{Content: "serverAddr = \"example.invalid\"\nauth.token = \"fake-token\"\n", Generation: 3}, nil
		}
		return RuntimeDocument{Content: `{"outbounds":[],"synthetic":"fake-private-config"}`, Generation: 4}, nil
	}, Now: func() time.Time { return now }})
	return s, f, &now
}
func backupEnvelope(t *testing.T, s *Service, scopes ...string) Envelope {
	t.Helper()
	raw, err := s.Backup(context.Background(), scopes)
	if err != nil {
		t.Fatal(err)
	}
	out, err := Decode(raw)
	if err != nil {
		t.Fatal(err)
	}
	return out
}
func encode(t *testing.T, e Envelope) []byte {
	t.Helper()
	for i := range e.Documents {
		e.Documents[i].Digest = digest(e.Documents[i].Content)
	}
	raw, err := json.Marshal(e)
	if err != nil {
		t.Fatal(err)
	}
	return raw
}
func expectCode(t *testing.T, err error, code string) {
	t.Helper()
	var own *Error
	var native *control.Error
	got := ""
	if errors.As(err, &own) {
		got = own.Code
	}
	if errors.As(err, &native) {
		got = native.Code
	}
	if got != code {
		t.Fatalf("code=%s want=%s err=%v", got, code, err)
	}
}
func changedBackup(t *testing.T, s *Service) []byte {
	e := backupEnvelope(t, s, "system", "dropbear")
	e.Documents[0].Content += " option timezone 'UTC'\n"
	e.Documents[1].Content += " option SSHKeepAlive '60'\n"
	return encode(t, e)
}
func TestScopedBackupRoundTripNativeAndExplicitRuntime(t *testing.T) {
	s, f, _ := fixture(t)
	e := backupEnvelope(t, s, NativeScopes...)
	if e.Model != "RN02" || e.Build != "test-build" || e.Generation != 7 || e.CreatedAt.IsZero() || len(e.Documents) != 6 {
		t.Fatal(e)
	}
	if len(f.stages) != 0 {
		t.Fatal("backup staged native documents")
	}
	for _, document := range e.Documents {
		if document.Digest != digest(document.Content) {
			t.Fatal("digest")
		}
		if strings.Contains(document.Module, "runtime") {
			t.Fatal("runtime included implicitly")
		}
	}
	raw, err := s.Backup(context.Background(), append(append([]string{}, NativeScopes...), "runtime.frpc", "runtime.sing-box"))
	if err != nil {
		t.Fatal(err)
	}
	runtime, err := Decode(raw)
	if err != nil || len(runtime.Documents) != 8 {
		t.Fatalf("%#v %v", runtime, err)
	}
	if !strings.Contains(runtime.Documents[6].Content, "fake-token") || runtime.Documents[6].Generation != 3 {
		t.Fatal("accepted private token was not backed up")
	}
	for _, forbidden := range []string{"/data/ssh", "passwd", "shadow", "private-key", "rescue", "../network"} {
		_, err := s.Backup(context.Background(), []string{forbidden})
		expectCode(t, err, "invalid_scope")
	}
}
func TestStrictDecodeAndContentBounds(t *testing.T) {
	s, _, _ := fixture(t)
	e := backupEnvelope(t, s, "system")
	raw := encode(t, e)
	cases := []struct {
		name string
		raw  []byte
		code string
	}{
		{"duplicateRoot", []byte(strings.Replace(string(raw), `"model":"RN02"`, `"model":"RN02","model":"other"`, 1)), "invalid_backup"},
		{"wrongCase", []byte(strings.Replace(string(raw), `"model"`, `"Model"`, 1)), "invalid_backup"},
		{"duplicateDocumentField", []byte(strings.Replace(string(raw), `"module":"system"`, `"module":"system","module":"system"`, 1)), "invalid_backup"},
		{"unknown", []byte(strings.Replace(string(raw), `"model":"RN02"`, `"model":"RN02","path":"/etc/shadow"`, 1)), "invalid_backup"},
		{"trailing", append(append([]byte{}, raw...), []byte(` {}`)...), "invalid_backup"},
		{"null", []byte(strings.Replace(string(raw), `"scopes":["system"]`, `"scopes":null`, 1)), "invalid_backup"},
		{"aggregateLimit", []byte(strings.Repeat(" ", MaxBackupBytes+1)), "backup_too_large"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) { _, err := Decode(tc.raw); expectCode(t, err, tc.code) })
	}
	e.Documents[0].Digest = strings.Repeat("0", 64)
	bad, _ := json.Marshal(e)
	_, err := Decode(bad)
	expectCode(t, err, "digest_mismatch")
	e.Documents[0].Content = strings.Repeat("x", control.MaxDocumentBytes+1)
	_, err = Decode(encode(t, e))
	expectCode(t, err, "document_too_large")
	e = backupEnvelope(t, s, "system")
	e.Scopes = append(e.Scopes, "network")
	_, err = Decode(encode(t, e))
	expectCode(t, err, "missing_component")
	e = backupEnvelope(t, s, "system", "dropbear")
	e.Documents[1] = e.Documents[0]
	_, err = Decode(encode(t, e))
	expectCode(t, err, "duplicate_module")
}
func TestPreviewExactDiffAndNoMutations(t *testing.T) {
	s, f, _ := fixture(t)
	raw := changedBackup(t, s)
	p, err := s.Preview(context.Background(), raw)
	if err != nil {
		t.Fatal(err)
	}
	if p.Generation != 7 || p.Summary.Modified != 2 || len(p.Changes) != 2 || p.ID == "" {
		t.Fatal(p)
	}
	for _, change := range p.Changes {
		if !change.Stageable || !change.Valid || change.Diff == "" || change.BeforeDigest == change.AfterDigest {
			t.Fatal(change)
		}
	}
	if !strings.Contains(p.Changes[0].Diff, "vendor_value") || len(f.stages) != 0 || len(f.deletes) != 0 {
		t.Fatal("preview lost unknown data or mutated")
	}
	e := backupEnvelope(t, s, "system")
	e.Documents[0].Content = "option orphan 'value'\n"
	p, err = s.Preview(context.Background(), encode(t, e))
	if err != nil {
		t.Fatal(err)
	}
	if p.Changes[0].Valid || p.Changes[0].Stageable || p.Changes[0].Errors[0].Code != "uci_syntax" {
		t.Fatal(p)
	}
}
func TestStageBundlePreservesGenerationAndConsumesPreview(t *testing.T) {
	s, f, _ := fixture(t)
	f.drafts["unrelated"] = control.Draft{ID: "unrelated"}
	p, err := s.Preview(context.Background(), changedBackup(t, s))
	if err != nil {
		t.Fatal(err)
	}
	staged, err := s.Stage(context.Background(), StageRequest{PreviewID: p.ID, Generation: p.Generation, Modules: []string{"dropbear", "system"}})
	if err != nil || len(staged.Drafts) != 2 || f.stages[0].Module != "system" || f.stages[1].Generation != p.Generation {
		t.Fatalf("%#v %v", staged, err)
	}
	if _, ok := f.drafts["unrelated"]; !ok {
		t.Fatal("unrelated draft touched")
	}
	_, err = s.Stage(context.Background(), StageRequest{PreviewID: p.ID, Generation: p.Generation, Modules: []string{"system"}})
	expectCode(t, err, "preview_not_found")
}
func TestModelMismatchAndGenerationAreExplicitStageGates(t *testing.T) {
	s, f, _ := fixture(t)
	e := backupEnvelope(t, s, "system")
	e.Model = "OTHER"
	e.Documents[0].Content += " option timezone 'UTC'\n"
	p, err := s.Preview(context.Background(), encode(t, e))
	if err != nil || !p.ModelMismatch {
		t.Fatalf("%#v %v", p, err)
	}
	request := StageRequest{PreviewID: p.ID, Generation: p.Generation, Modules: []string{"system"}}
	_, err = s.Stage(context.Background(), request)
	expectCode(t, err, "model_mismatch")
	request.AcknowledgeModelMismatch = true
	request.Generation++
	_, err = s.Stage(context.Background(), request)
	expectCode(t, err, "generation_conflict")
	request.Generation = p.Generation
	f.generation++
	_, err = s.Stage(context.Background(), request)
	expectCode(t, err, "generation_conflict")
	if len(f.stages) != 0 {
		t.Fatal("gated operation staged")
	}
}
func TestStageSelectionChecksDependenciesAgainstOnlyChosenGroups(t *testing.T) {
	s, f, _ := fixture(t)
	e := backupEnvelope(t, s, "network", "dhcp")
	e.Documents[0].Content += "config interface 'guest'\n option proto 'static'\n"
	e.Documents[1].Content += "config dhcp 'guest'\n option interface 'guest'\n"
	p, err := s.Preview(context.Background(), encode(t, e))
	if err != nil {
		t.Fatal(err)
	}
	_, err = s.Stage(context.Background(), StageRequest{PreviewID: p.ID, Generation: p.Generation, Modules: []string{"dhcp"}})
	expectCode(t, err, "invalid_reference")
	if len(f.stages) != 0 {
		t.Fatal("missing selected dependency staged")
	}
	staged, err := s.Stage(context.Background(), StageRequest{PreviewID: p.ID, Generation: p.Generation, Modules: []string{"dhcp", "network"}})
	if err != nil || len(staged.Drafts) != 2 || f.stages[0].Module != "network" {
		t.Fatalf("%#v %v", staged, err)
	}
}
func TestPartialStageCleansOnlyOwnedIDsIncludingCancelledRequest(t *testing.T) {
	for _, scenario := range []string{"storage", "invalid", "cancel", "generation", "cleanup"} {
		t.Run(scenario, func(t *testing.T) {
			s, f, _ := fixture(t)
			p, err := s.Preview(context.Background(), changedBackup(t, s))
			if err != nil {
				t.Fatal(err)
			}
			f.drafts["unrelated"] = control.Draft{ID: "unrelated"}
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			f.cancel = cancel
			switch scenario {
			case "storage":
				f.failAt = 2
			case "invalid":
				f.invalidAt = 2
			case "cancel":
				f.cancelAt = 1
			case "generation":
				f.generationAfterStage = true
			case "cleanup":
				f.failAt = 2
				f.cleanupFailure = true
			}
			_, err = s.Stage(ctx, StageRequest{PreviewID: p.ID, Generation: p.Generation, Modules: []string{"system", "dropbear"}})
			if err == nil {
				t.Fatal("failure hidden")
			}
			if _, ok := f.drafts["unrelated"]; !ok {
				t.Fatal("cleanup deleted unrelated draft")
			}
			if scenario == "cleanup" {
				var e *Error
				if !errors.As(err, &e) || e.Code != "import_cleanup_failed" || e.CauseCode != "storage_insufficient" || len(e.RetainedDraftIDs) != 1 {
					t.Fatalf("%#v %v", e, err)
				}
			} else if len(f.drafts) != 1 {
				t.Fatalf("owned drafts abandoned: %#v", f.drafts)
			}
		})
	}
}
func TestPreviewStoreBoundedExpiryAndDiscard(t *testing.T) {
	s, _, now := fixture(t)
	raw := changedBackup(t, s)
	ids := []string{}
	for i := 0; i < MaxPreviews; i++ {
		p, err := s.Preview(context.Background(), raw)
		if err != nil {
			t.Fatal(err)
		}
		ids = append(ids, p.ID)
	}
	_, err := s.Preview(context.Background(), raw)
	expectCode(t, err, "preview_limit")
	s.Discard(ids[0])
	if _, err := s.Preview(context.Background(), raw); err != nil {
		t.Fatal(err)
	}
	*now = now.Add(PreviewLifetime)
	_, err = s.Stage(context.Background(), StageRequest{PreviewID: ids[1], Generation: 7, Modules: []string{"system"}})
	expectCode(t, err, "preview_not_found")
	if _, err := s.Preview(context.Background(), raw); err != nil {
		t.Fatal(err)
	}
	s.Clear()
	if len(s.previews) != 0 {
		t.Fatal("private previews retained")
	}
}
func TestRuntimeScopeIsExplicitPreviewNotNativeStageOrAutoStart(t *testing.T) {
	s, f, _ := fixture(t)
	e := backupEnvelope(t, s, "runtime.frpc")
	e.Documents[0].Content += "# preserved extra option\n"
	p, err := s.Preview(context.Background(), encode(t, e))
	if err != nil {
		t.Fatal(err)
	}
	if p.Changes[0].Stageable || p.Changes[0].Diff != "" || p.Summary.Modified != 1 {
		t.Fatal(p)
	}
	_, err = s.Stage(context.Background(), StageRequest{PreviewID: p.ID, Generation: p.Generation, Modules: []string{"runtime.frpc"}})
	expectCode(t, err, "invalid_selection")
	s.options.Runtime = nil
	p, err = s.Preview(context.Background(), encode(t, e))
	if err != nil {
		t.Fatal(err)
	}
	if p.Changes[0].Kind != "uncompared" || p.Summary.Uncompared != 1 || p.Changes[0].BeforeDigest != "" {
		t.Fatal("unknown falsely compared", p)
	}
	if len(f.stages) != 0 {
		t.Fatal("runtime import staged native or started service")
	}
}
func TestPendingSnapshotNeverClaimsAcceptedBackup(t *testing.T) {
	s, f, _ := fixture(t)
	raw := changedBackup(t, s)
	f.pending = true
	_, err := s.Backup(context.Background(), []string{"system"})
	expectCode(t, err, "confirmation_pending")
	_, err = s.Preview(context.Background(), raw)
	expectCode(t, err, "confirmation_pending")
}

func TestCancelledRuntimePreviewDoesNotRetainUnreachablePrivateContent(t *testing.T) {
	s, _, _ := fixture(t)
	raw, err := s.Backup(context.Background(), []string{"runtime.frpc"})
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	s.options.Runtime = func(context.Context, string) (RuntimeDocument, error) {
		cancel()
		return RuntimeDocument{}, context.Canceled
	}
	_, err = s.Preview(ctx, raw)
	if !errors.Is(err, context.Canceled) || len(s.previews) != 0 {
		t.Fatalf("cancelled private preview retained %#v %v", s.previews, err)
	}
}
func TestPartialCleanupAlreadyAbsentIsNotReportedRetained(t *testing.T) {
	s, f, _ := fixture(t)
	p, err := s.Preview(context.Background(), changedBackup(t, s))
	if err != nil {
		t.Fatal(err)
	}
	f.failAt = 2
	f.cleanupAbsent = true
	_, err = s.Stage(context.Background(), StageRequest{PreviewID: p.ID, Generation: p.Generation, Modules: []string{"system", "dropbear"}})
	expectCode(t, err, "storage_insufficient")
	if len(f.drafts) != 0 {
		t.Fatal("already-absent draft counted retained")
	}
}

func TestClearRejectsPreviewAlreadyReadingPrivateSnapshot(t *testing.T) {
	s, _, _ := fixture(t)
	raw := changedBackup(t, s)
	originalMetadata := s.options.Metadata
	entered, resume := make(chan struct{}), make(chan struct{})
	s.options.Metadata = func(ctx context.Context) (Metadata, error) {
		close(entered)
		select {
		case <-resume:
			return originalMetadata(ctx)
		case <-ctx.Done():
			return Metadata{}, ctx.Err()
		}
	}
	type result struct {
		preview Preview
		err     error
	}
	finished := make(chan result, 1)
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	go func() { preview, err := s.Preview(ctx, raw); finished <- result{preview: preview, err: err} }()
	select {
	case <-entered:
	case <-ctx.Done():
		t.Fatal("preview did not enter snapshot")
	}
	s.Clear()
	close(resume)
	var got result
	select {
	case got = <-finished:
	case <-ctx.Done():
		t.Fatal("preview did not finish after Clear")
	}
	expectCode(t, got.err, "preview_cleared")
	if got.preview.ID != "" || len(s.previews) != 0 {
		t.Fatalf("old-session private preview published %#v %#v", got.preview, s.previews)
	}
	s.options.Metadata = originalMetadata
	oldPreview, err := s.Preview(context.Background(), raw)
	if err != nil {
		t.Fatal(err)
	}
	s.Clear()
	_, err = s.Stage(context.Background(), StageRequest{PreviewID: oldPreview.ID, Generation: oldPreview.Generation, Modules: []string{"system"}})
	expectCode(t, err, "preview_not_found")
	current, err := s.Preview(context.Background(), raw)
	if err != nil || current.ID == "" || len(s.previews) != 1 {
		t.Fatalf("new preview after Clear unavailable %#v %v", current, err)
	}
}

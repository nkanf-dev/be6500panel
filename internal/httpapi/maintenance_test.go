package httpapi

import (
	"be6500panel/internal/control"
	"be6500panel/internal/maintenance"
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

type maintenanceNativeFixture struct {
	docs        []control.Document
	stageCalls  int
	fail        bool
	cleanupFail bool
}

func (f *maintenanceNativeFixture) Documents(ctx context.Context) (control.DocumentSet, error) {
	return control.DocumentSet{Generation: 5, Documents: f.docs}, nil
}
func (f *maintenanceNativeFixture) Stage(ctx context.Context, r control.StageRequest) (control.Draft, error) {
	f.stageCalls++
	if f.fail && f.stageCalls == 2 {
		return control.Draft{}, &control.Error{Code: "storage_insufficient", Message: "Synthetic storage failure."}
	}
	return control.Draft{ID: fmt.Sprintf("owned-%d", f.stageCalls), Module: r.Module, Generation: 5, Valid: true, Errors: []control.Issue{}, Risks: []control.Issue{}}, nil
}
func (f *maintenanceNativeFixture) DeleteDraft(ctx context.Context, id string) error {
	if f.cleanupFail {
		return &control.Error{Code: "storage_failed", Message: "Synthetic cleanup failure."}
	}
	return nil
}
func maintenanceHTTPFixture() (*maintenance.Service, *maintenanceNativeFixture) {
	native := &maintenanceNativeFixture{docs: []control.Document{}}
	for _, module := range maintenance.NativeScopes {
		native.docs = append(native.docs, control.Document{Module: module, Content: "config vendor\n option extension 'fake-private-value'\n"})
	}
	return maintenance.New(maintenance.Options{Native: native, Metadata: func(context.Context) (maintenance.Metadata, error) {
		return maintenance.Metadata{Model: "RN02", Build: "test"}, nil
	}}), native
}
func maintenanceRequest(method, path, body string) *http.Request {
	r := httptest.NewRequest(method, path, strings.NewReader(body))
	r.Header.Set("Content-Type", "application/json")
	return r
}
func TestMaintenanceDownloadUsesPrivateAttachmentOnly(t *testing.T) {
	service, native := maintenanceHTTPFixture()
	w := httptest.NewRecorder()
	HandleMaintenanceBackup(w, maintenanceRequest("POST", "/api/maintenance/backup", `{"scopes":["system"]}`), service)
	if w.Code != 200 || w.Header().Get("Cache-Control") != "no-store" || !strings.HasPrefix(w.Header().Get("Content-Disposition"), `attachment; filename="be6500panel-backup-`) {
		t.Fatalf("%d %#v %s", w.Code, w.Header(), w.Body.String())
	}
	backup, err := maintenance.Decode(w.Body.Bytes())
	if err != nil || len(backup.Documents) != 1 || backup.Documents[0].Module != "system" {
		t.Fatalf("%#v %v", backup, err)
	}
	if native.stageCalls != 0 {
		t.Fatal("download staged configuration")
	}
	for _, body := range []string{`{"scopes":["system"],"scopes":["dropbear"]}`, `{"Scopes":["system"]}`, `{"scopes":["/data/ssh"]}`} {
		w = httptest.NewRecorder()
		HandleMaintenanceBackup(w, maintenanceRequest("POST", "/api/maintenance/backup", body), service)
		if w.Code != 400 {
			t.Fatalf("%s -> %d", body, w.Code)
		}
	}
}
func TestMaintenancePreviewRejectsRawDuplicatesAndBodyLimit(t *testing.T) {
	service, native := maintenanceHTTPFixture()
	raw, err := service.Backup(context.Background(), []string{"system"})
	if err != nil {
		t.Fatal(err)
	}
	duplicate := strings.Replace(string(raw), `"module":"system"`, `"module":"system","module":"system"`, 1)
	w := httptest.NewRecorder()
	HandleMaintenancePreview(w, maintenanceRequest("POST", "/api/maintenance/import/preview", duplicate), service)
	if w.Code != 400 || native.stageCalls != 0 {
		t.Fatalf("%d %s", w.Code, w.Body.String())
	}
	w = httptest.NewRecorder()
	HandleMaintenancePreview(w, maintenanceRequest("POST", "/api/maintenance/import/preview", strings.Repeat(" ", maintenance.MaxBackupBytes+1)), service)
	if w.Code != 413 {
		t.Fatalf("%d %s", w.Code, w.Body.String())
	}
	w = httptest.NewRecorder()
	r := maintenanceRequest("POST", "/api/maintenance/import/preview", string(raw))
	r.Header.Set("Content-Type", "application/x-tar")
	HandleMaintenancePreview(w, r, service)
	if w.Code != 415 {
		t.Fatal(w.Code)
	}
}
func TestMaintenancePreviewStageAndDiscardStrictContracts(t *testing.T) {
	service, native := maintenanceHTTPFixture()
	raw, err := service.Backup(context.Background(), []string{"system"})
	if err != nil {
		t.Fatal(err)
	}
	var envelope maintenance.Envelope
	if err = json.Unmarshal(raw, &envelope); err != nil {
		t.Fatal(err)
	}
	envelope.Documents[0].Content += " option another 'preserved'\n"
	// Generate the SHA256 from a fresh scoped fixture, not arbitrary path input.
	native.docs[4].Content = envelope.Documents[0].Content
	raw, err = service.Backup(context.Background(), []string{"system"})
	if err != nil {
		t.Fatal(err)
	}
	native.docs[4].Content = "config vendor\n option extension 'fake-private-value'\n"
	w := httptest.NewRecorder()
	HandleMaintenancePreview(w, maintenanceRequest("POST", "/api/maintenance/import/preview", string(raw)), service)
	if w.Code != 200 {
		t.Fatalf("%d %s", w.Code, w.Body.String())
	}
	var p maintenance.Preview
	if err = json.Unmarshal(w.Body.Bytes(), &p); err != nil {
		t.Fatal(err)
	}
	if native.stageCalls != 0 || len(p.Changes) != 1 || !p.Changes[0].Stageable {
		t.Fatal("preview mutated or did not show change", p)
	}
	for _, body := range []string{fmt.Sprintf(`{"previewId":%q,"generation":5,"modules":["system"],"acknowledgeModelMismatch":false,"generation":5}`, p.ID), fmt.Sprintf(`{"previewId":%q,"generation":5,"modules":["system"]}`, p.ID)} {
		w = httptest.NewRecorder()
		HandleMaintenanceStage(w, maintenanceRequest("POST", "/api/maintenance/import/stage", body), service)
		if w.Code != 400 || native.stageCalls != 0 {
			t.Fatalf("%d %s", w.Code, w.Body.String())
		}
	}
	w = httptest.NewRecorder()
	HandleMaintenancePreview(w, maintenanceRequest("DELETE", "/api/maintenance/import/preview?id="+p.ID, ""), service)
	if w.Code != 200 {
		t.Fatal(w.Code)
	}
	w = httptest.NewRecorder()
	body := fmt.Sprintf(`{"previewId":%q,"generation":5,"modules":["system"],"acknowledgeModelMismatch":false}`, p.ID)
	HandleMaintenanceStage(w, maintenanceRequest("POST", "/api/maintenance/import/stage", body), service)
	if w.Code != 404 || native.stageCalls != 0 {
		t.Fatalf("%d %s", w.Code, w.Body.String())
	}
}
func TestMaintenanceCleanupFailureCarriesOwnedIDsAndSafeCause(t *testing.T) {
	service, native := maintenanceHTTPFixture()
	native.docs[4].Content += " option other 'fake-value'\n"
	native.docs[5].Content += " option other 'fake-value'\n"
	raw, err := service.Backup(context.Background(), []string{"system", "dropbear"})
	if err != nil {
		t.Fatal(err)
	}
	native.docs[4].Content = "config vendor\n"
	native.docs[5].Content = "config vendor\n"
	p, err := service.Preview(context.Background(), raw)
	if err != nil {
		t.Fatal(err)
	}
	native.fail = true
	native.cleanupFail = true
	body, _ := json.Marshal(maintenance.StageRequest{PreviewID: p.ID, Generation: 5, Modules: []string{"system", "dropbear"}})
	w := httptest.NewRecorder()
	HandleMaintenanceStage(w, maintenanceRequest("POST", "/api/maintenance/import/stage", string(body)), service)
	if w.Code != 500 || !bytes.Contains(w.Body.Bytes(), []byte(`"retainedDraftIds":["owned-1"]`)) || !bytes.Contains(w.Body.Bytes(), []byte(`"causeCode":"storage_insufficient"`)) || bytes.Contains(w.Body.Bytes(), []byte("fake-private-value")) {
		t.Fatalf("%d %s", w.Code, w.Body.String())
	}
}

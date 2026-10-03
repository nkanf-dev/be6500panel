package httpapi

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"be6500panel/internal/deviceannotations"
	"be6500panel/internal/storage"
)

func annotationHTTP(t *testing.T, opts deviceannotations.Options) (http.Handler, *deviceannotations.Store) {
	t.Helper()
	if opts.DataDir == "" {
		opts.DataDir = t.TempDir()
	}
	s, err := deviceannotations.New(opts)
	if err != nil {
		t.Fatal(err)
	}
	return DeviceAnnotationsHandler(s), s
}
func annotationRequest(handler http.Handler, method, body string) *httptest.ResponseRecorder {
	r := httptest.NewRequest(method, "http://router.local/api/devices/annotations", strings.NewReader(body))
	if body != "" {
		r.Header.Set("Content-Type", "application/json")
	}
	w := httptest.NewRecorder()
	handler.ServeHTTP(w, r)
	return w
}
func annotationBody(revision uint64) string {
	raw, _ := json.Marshal(deviceannotations.UpdateRequest{MAC: "aa-bb-cc-dd-ee-ff", Label: "Desk", Note: "password=LAN plain note", Tags: []string{"work"}, ExpectedRevision: revision})
	return string(raw)
}
func assertAnnotationError(t *testing.T, w *httptest.ResponseRecorder, status int, code string) {
	t.Helper()
	var envelope errorEnvelope
	if err := json.Unmarshal(w.Body.Bytes(), &envelope); err != nil {
		t.Fatal(err)
	}
	if w.Code != status || envelope.Error.Code != code {
		t.Fatalf("status %d body %s", w.Code, w.Body.String())
	}
}

func TestDeviceAnnotationsGETAndPOSTContract(t *testing.T) {
	handler, store := annotationHTTP(t, deviceannotations.Options{})
	w := annotationRequest(handler, http.MethodGet, "")
	if w.Code != 200 || strings.TrimSpace(w.Body.String()) != `{"revision":0,"devices":{}}` {
		t.Fatalf("empty: %d %s", w.Code, w.Body.String())
	}
	w = annotationRequest(handler, http.MethodPost, annotationBody(0))
	if w.Code != 200 {
		t.Fatalf("%d %s", w.Code, w.Body.String())
	}
	var got deviceannotations.Snapshot
	if err := json.Unmarshal(w.Body.Bytes(), &got); err != nil {
		t.Fatal(err)
	}
	if got.Revision != 1 || got.Devices["AA:BB:CC:DD:EE:FF"].Note != "password=LAN plain note" {
		t.Fatalf("save: %#v", got)
	}
	get := annotationRequest(handler, http.MethodGet, "")
	if !bytes.Equal(w.Body.Bytes(), get.Body.Bytes()) {
		t.Fatal("GET differs from accepted save response")
	}
	for name, value := range map[string]string{"Content-Type": "application/json; charset=utf-8", "Cache-Control": "no-store"} {
		if get.Header().Get(name) != value {
			t.Fatalf("missing %s", name)
		}
	}
	before, err := store.Snapshot(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	w = annotationRequest(handler, http.MethodPost, annotationBody(0))
	assertAnnotationError(t, w, 409, "revision_conflict")
	after, _ := store.Snapshot(context.Background())
	if after.Revision != before.Revision {
		t.Fatal("conflict changed revision")
	}
	w = annotationRequest(handler, http.MethodPost, `{"mac":"AA:BB:CC:DD:EE:FF","label":"","note":"","tags":[],"expectedRevision":1}`)
	if w.Code != 200 || strings.TrimSpace(w.Body.String()) != `{"revision":2,"devices":{}}` {
		t.Fatalf("delete: %d %s", w.Code, w.Body.String())
	}
}

func TestDeviceAnnotationsStrictRequestContract(t *testing.T) {
	handler, _ := annotationHTTP(t, deviceannotations.Options{})
	cases := []struct {
		body   string
		status int
		code   string
	}{
		{`{}`, 400, "invalid_input"},
		{`{"mac":"AA:BB:CC:DD:EE:FF","label":"x","note":"","tags":[]}`, 400, "invalid_input"},
		{`{"mac":"AA:BB:CC:DD:EE:FF","label":"x","note":"","tags":null,"expectedRevision":0}`, 400, "invalid_input"},
		{`{"mac":"AA:BB:CC:DD:EE:FF","label":"x","note":"","tags":[],"expectedRevision":null}`, 400, "invalid_input"},
		{`{"MAC":"AA:BB:CC:DD:EE:FF","label":"x","note":"","tags":[],"expectedRevision":0}`, 400, "invalid_input"},
		{`{"mac":"AA:BB:CC:DD:EE:FF","label":"x","note":"","tags":[],"expectedRevision":0,"extra":1}`, 400, "invalid_json"},
		{`{"mac":"AA:BB:CC:DD:EE:FF","label":"x","note":"","tags":[],"expectedRevision":0,"expectedRevision":1}`, 400, "invalid_json"},
		{annotationBody(0) + `{}`, 400, "invalid_json"},
		{`[]`, 400, "invalid_json"},
		{`{"mac":"192.168.1.2","label":"x","note":"","tags":[],"expectedRevision":0}`, 400, "invalid_input"},
		{`{"mac":"AA:BB:CC:DD:EE:FF:00:11","label":"x","note":"","tags":[],"expectedRevision":0}`, 400, "invalid_input"},
		{`{"mac":"AA:BB:CC:DD:EE:FF","label":"x","note":"","tags":[],"expectedRevision":-1}`, 400, "invalid_json"},
		{`{"mac":"AA:BB:CC:DD:EE:FF","label":"x","note":"","tags":[],"expectedRevision":9007199254740992}`, 400, "invalid_input"},
		{`{"mac":"AA:BB:CC:DD:EE:FF","label":"` + strings.Repeat("猫", 81) + `","note":"","tags":[],"expectedRevision":0}`, 400, "invalid_input"},
		{strings.Repeat(" ", MaxBodyBytes+1), 413, "body_too_large"},
	}
	for i, test := range cases {
		t.Run(strings.Join([]string{"case", string(rune('A' + i))}, ""), func(t *testing.T) {
			assertAnnotationError(t, annotationRequest(handler, http.MethodPost, test.body), test.status, test.code)
		})
	}
	r := httptest.NewRequest(http.MethodPost, "http://router.local/api/devices/annotations", strings.NewReader(annotationBody(0)))
	w := httptest.NewRecorder()
	handler.ServeHTTP(w, r)
	assertAnnotationError(t, w, 415, "unsupported_media_type")
}

func TestDeviceAnnotationsUnavailableAndAllowedMethods(t *testing.T) {
	handler, _ := annotationHTTP(t, deviceannotations.Options{})
	for _, method := range []string{http.MethodDelete, http.MethodPut, http.MethodPatch, http.MethodHead, http.MethodOptions} {
		w := annotationRequest(handler, method, "")
		assertAnnotationError(t, w, 405, "method_not_allowed")
		if w.Header().Get("Allow") != "GET, POST" {
			t.Fatal("missing method contract")
		}
	}
	assertAnnotationError(t, annotationRequest(DeviceAnnotationsHandler(nil), http.MethodGet, ""), 503, "annotations_unavailable")
}

func TestDeviceAnnotationsStorageErrorsAndCancellation(t *testing.T) {
	for _, measurement := range []bool{false, true} {
		t.Run(map[bool]string{false: "space", true: "measurement"}[measurement], func(t *testing.T) {
			dir := t.TempDir()
			denial := storage.ErrInsufficientSpace
			if measurement {
				denial = storage.ErrMeasurement
			}
			handler, store := annotationHTTP(t, deviceannotations.Options{DataDir: dir, StorageAdmission: func(context.Context, string, int64, bool) (func(), error) { return nil, denial }})
			w := annotationRequest(handler, http.MethodPost, annotationBody(0))
			assertAnnotationError(t, w, 507, "storage_insufficient")
			got, _ := store.Snapshot(context.Background())
			if got.Revision != 0 {
				t.Fatal("denial changed revision")
			}
		})
	}
	t.Run("cancel", func(t *testing.T) {
		handler, _ := annotationHTTP(t, deviceannotations.Options{})
		ctx, cancel := context.WithCancel(context.Background())
		cancel()
		r := httptest.NewRequest(http.MethodPost, "http://router.local/api/devices/annotations", strings.NewReader(annotationBody(0))).WithContext(ctx)
		r.Header.Set("Content-Type", "application/json")
		w := httptest.NewRecorder()
		handler.ServeHTTP(w, r)
		assertAnnotationError(t, w, 408, "cancelled")
	})
	t.Run("filesystem", func(t *testing.T) {
		dir := t.TempDir()
		handler, _ := annotationHTTP(t, deviceannotations.Options{DataDir: dir})
		if err := os.Mkdir(filepath.Join(dir, deviceannotations.FileName), 0700); err != nil {
			t.Fatal(err)
		}
		w := annotationRequest(handler, http.MethodPost, annotationBody(0))
		assertAnnotationError(t, w, 500, "storage_failed")
		if strings.Contains(w.Body.String(), dir) {
			t.Fatal("private path exposed")
		}
	})
	t.Run("other admission error", func(t *testing.T) {
		handler, _ := annotationHTTP(t, deviceannotations.Options{StorageAdmission: func(context.Context, string, int64, bool) (func(), error) {
			return nil, errors.New("private admission detail")
		}})
		w := annotationRequest(handler, http.MethodPost, annotationBody(0))
		assertAnnotationError(t, w, 500, "annotations_failed")
		if strings.Contains(w.Body.String(), "private admission detail") {
			t.Fatal("admission details exposed")
		}
	})
}

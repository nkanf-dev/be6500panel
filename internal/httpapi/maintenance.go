package httpapi

import (
	"be6500panel/internal/control"
	"be6500panel/internal/maintenance"
	"context"
	"errors"
	"fmt"
	"io"
	"mime"
	"net/http"
	"time"
)

// HandleMaintenanceBackup must be mounted behind the server's authentication
// and same-origin guard. It returns private JSON as an explicit attachment.
func HandleMaintenanceBackup(w http.ResponseWriter, r *http.Request, service *maintenance.Service) {
	if !maintenanceEnabled(w, service) {
		return
	}
	var input struct {
		Scopes []string `json:"scopes"`
	}
	if !decodeJSON(w, r, &input, "scopes") {
		return
	}
	raw, err := service.Backup(r.Context(), input.Scopes)
	if err != nil {
		maintenanceError(w, err)
		return
	}
	w.Header().Set("Cache-Control", "no-store")
	w.Header().Set("X-Content-Type-Options", "nosniff")
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	w.Header().Set("Content-Disposition", fmt.Sprintf(`attachment; filename="be6500panel-backup-%s.json"`, time.Now().UTC().Format("20060102-150405")))
	w.WriteHeader(http.StatusOK)
	_, _ = w.Write(raw)
}

// HandleMaintenancePreview accepts the original JSON file, never a reserialized
// browser object. Duplicate-key rejection applies to the complete raw backup.
// DELETE discards only one memory preview and never removes any private drafts.
func HandleMaintenancePreview(w http.ResponseWriter, r *http.Request, service *maintenance.Service) {
	if !maintenanceEnabled(w, service) {
		return
	}
	if r.Method == http.MethodDelete {
		service.Discard(r.URL.Query().Get("id"))
		writeJSON(w, 200, struct {
			Discarded bool `json:"discarded"`
		}{true})
		return
	}
	media, _, err := mime.ParseMediaType(r.Header.Get("Content-Type"))
	if err != nil || media != "application/json" {
		fail(w, 415, "unsupported_media_type", "Content-Type must be application/json.")
		return
	}
	r.Body = http.MaxBytesReader(w, r.Body, maintenance.MaxBackupBytes)
	raw, err := io.ReadAll(r.Body)
	if err != nil {
		var tooLarge *http.MaxBytesError
		if errors.As(err, &tooLarge) {
			fail(w, 413, "body_too_large", "Backup exceeds the 2 MiB JSON limit.")
		} else {
			fail(w, 400, "invalid_json", "Cannot read JSON backup.")
		}
		return
	}
	preview, err := service.Preview(r.Context(), raw)
	if err != nil {
		maintenanceError(w, err)
		return
	}
	w.Header().Set("Cache-Control", "no-store")
	writeJSON(w, 200, preview)
}
func HandleMaintenanceStage(w http.ResponseWriter, r *http.Request, service *maintenance.Service) {
	if !maintenanceEnabled(w, service) {
		return
	}
	var input maintenance.StageRequest
	if !decodeJSON(w, r, &input, "previewId", "generation", "modules", "acknowledgeModelMismatch") {
		return
	}
	staged, err := service.Stage(r.Context(), input)
	if err != nil {
		maintenanceError(w, err)
		return
	}
	w.Header().Set("Cache-Control", "no-store")
	writeJSON(w, 200, staged)
}
func maintenanceEnabled(w http.ResponseWriter, service *maintenance.Service) bool {
	if service == nil {
		fail(w, 503, "maintenance_unavailable", "Configuration backup is not enabled.")
		return false
	}
	return true
}
func maintenanceError(w http.ResponseWriter, err error) {
	code, message := "maintenance_failed", "Configuration backup or import failed."
	var typed *maintenance.Error
	var native *control.Error
	switch {
	case errors.As(err, &typed):
		code, message = typed.Code, typed.Message
	case errors.As(err, &native):
		code, message = native.Code, native.Message
	case errors.Is(err, context.Canceled):
		code, message = "operation_cancelled", "Configuration import was cancelled."
	case errors.Is(err, context.DeadlineExceeded):
		code, message = "operation_timeout", "Configuration import timed out."
	}
	status := http.StatusBadRequest
	switch code {
	case "preview_not_found":
		status = http.StatusNotFound
	case "generation_conflict", "confirmation_pending", "model_mismatch", "draft_limit", "preview_limit", "operation_cancelled":
		status = http.StatusConflict
	case "backup_too_large", "document_too_large":
		status = http.StatusRequestEntityTooLarge
	case "invalid_candidate", "invalid_reference", "uci_validation_failed", "invalid_field", "uci_syntax":
		status = http.StatusUnprocessableEntity
	case "maintenance_unavailable", "metadata_unavailable", "runtime_unavailable", "snapshot_unavailable", "validation_unavailable", "storage_insufficient":
		status = http.StatusServiceUnavailable
	case "storage_failed", "import_cleanup_failed", "maintenance_failed":
		status = http.StatusInternalServerError
	case "operation_timeout":
		status = http.StatusGatewayTimeout
	}
	if typed != nil && len(typed.RetainedDraftIDs) > 0 {
		writeJSON(w, status, struct {
			Error            *maintenance.Error `json:"error"`
			RetainedDraftIDs []string           `json:"retainedDraftIds"`
		}{typed, typed.RetainedDraftIDs})
		return
	}
	fail(w, status, code, message)
}

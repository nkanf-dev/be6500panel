package httpapi

import (
	managedruntime "be6500panel/internal/runtime"
	"context"
	"errors"
	"net/http"

	"be6500panel/internal/control"
)

func (s *Server) controlEnabled(w http.ResponseWriter) bool {
	if s.control == nil {
		fail(w, 503, "control_unavailable", "配置控制未启用")
		return false
	}
	return true
}
func (s *Server) configDocuments(w http.ResponseWriter, r *http.Request) {
	if !s.controlEnabled(w) {
		return
	}
	docs, err := s.control.Documents(r.Context())
	if err != nil {
		s.controlError(w, err)
		return
	}
	writeJSON(w, 200, docs)
}
func (s *Server) configStage(w http.ResponseWriter, r *http.Request) {
	if !s.controlEnabled(w) {
		return
	}
	var input control.StageRequest
	if !decodeJSONLimit(w, r, &input, 2<<20, "module", "content", "generation") {
		return
	}
	draft, err := s.control.Stage(r.Context(), input)
	if err != nil {
		s.controlError(w, err)
		return
	}
	s.logger.Info("Configuration staged", "code", "configuration_staged", "module", input.Module)
	writeJSON(w, 200, draft)
}
func (s *Server) configDrafts(w http.ResponseWriter, r *http.Request) {
	if !s.controlEnabled(w) {
		return
	}
	if r.Method == "DELETE" {
		if err := s.control.DeleteDraft(r.Context(), r.URL.Query().Get("id")); err != nil {
			s.controlError(w, err)
			return
		}
		writeJSON(w, 200, struct {
			Deleted bool `json:"deleted"`
		}{true})
		return
	}
	drafts, err := s.control.Drafts(r.Context())
	if err != nil {
		s.controlError(w, err)
		return
	}
	writeJSON(w, 200, struct {
		Drafts []control.Draft `json:"drafts"`
	}{drafts})
}
func (s *Server) configCommit(w http.ResponseWriter, r *http.Request) {
	if !s.controlEnabled(w) {
		return
	}
	var input control.CommitRequest
	if !decodeJSON(w, r, &input, "draftIds", "generation", "acknowledgeRisks") {
		return
	}
	var operation control.Operation
	captureDisabled := false
	err := s.nativeConfigurationCommit(r.Context(), input, func(ctx context.Context, networkChange bool) error {
		if networkChange && s.capture != nil {
			captureDisabled = s.capture.Status().Desired
			if err := s.capture.DisableRetainingSelection(ctx); err != nil {
				return &control.Error{Code: "capture_cleanup_failed", Message: "Capture must be withdrawn before native configuration changes."}
			}
			if captureDisabled {
				s.logger.Warn("Capture disabled for native configuration; explicitly Apply devices again after confirmation", "code", "capture_disabled_for_native_config", "module", "proxy")
			}
		}
		var err error
		operation, err = s.control.Commit(ctx, input)
		return err
	})
	if err != nil {
		s.logger.Warn("Configuration commit failed", "code", "configuration_commit_failed", "module", "configuration")
		if operation.ID != "" {
			var typed *control.Error
			if errors.As(err, &typed) {
				writeJSON(w, 409, struct {
					Error           apiError          `json:"error"`
					Operation       control.Operation `json:"operation"`
					CaptureDisabled bool              `json:"captureDisabled,omitempty"`
					Warning         string            `json:"warning,omitempty"`
				}{apiError{Code: typed.Code, Message: typed.Message}, operation, captureDisabled, captureDisabledWarning(captureDisabled)})
				return
			}
		}
		s.controlError(w, err)
		return
	}
	s.logger.Info("Configuration commit completed", "code", operation.State, "module", "configuration")
	writeJSON(w, 200, struct {
		control.Operation
		CaptureDisabled bool   `json:"captureDisabled,omitempty"`
		Warning         string `json:"warning,omitempty"`
	}{operation, captureDisabled, captureDisabledWarning(captureDisabled)})
}
func (s *Server) configConfirm(w http.ResponseWriter, r *http.Request, rollback bool) {
	if !s.controlEnabled(w) {
		return
	}
	var input struct {
		ID string `json:"id"`
	}
	if !decodeJSON(w, r, &input, "id") {
		return
	}
	var operation control.Operation
	var err error
	if rollback {
		err = s.nativeConfigurationOperation(r.Context(), func(ctx context.Context) error {
			var err error
			operation, err = s.control.Rollback(ctx, input.ID)
			return err
		})
	} else {
		operation, err = s.control.Confirm(r.Context(), input.ID)
	}
	if err != nil {
		s.controlError(w, err)
		return
	}
	s.logger.Info("Configuration operation completed", "code", operation.State, "module", "configuration")
	writeJSON(w, 200, operation)
}
func (s *Server) configStatus(w http.ResponseWriter, r *http.Request) {
	if s.control == nil {
		writeJSON(w, 200, struct {
			Enabled    bool   `json:"enabled"`
			Generation uint64 `json:"generation"`
		}{false, 0})
		return
	}
	writeJSON(w, 200, s.control.Status())
}
func (s *Server) controlError(w http.ResponseWriter, err error) {
	var typed *control.Error
	if errors.As(err, &typed) {
		status := 400
		switch typed.Code {
		case "generation_conflict", "operation_busy", "pending_confirmation", "risk_ack_required", "capture_cleanup_failed":
			status = 409
		case "not_found", "draft_not_found", "operation_not_found":
			status = 404
		case "validation_failed", "reload_unsupported":
			status = 422
		case "reload_failed", "verification_failed", "rollback_failed", "storage_failed":
			status = 500
		}
		fail(w, status, typed.Code, typed.Message)
		return
	}
	fail(w, 500, "configuration_failed", "配置操作失败")
}

// Native network transactions disable saved capture before writing UCI. This
// prevents automatic runtime restoration during provisional apply/rollback.
// The user must explicitly Apply the observed scope again after confirmation.
func (s *Server) nativeConfigurationOperation(ctx context.Context, operation func(context.Context) error) error {
	apply := func(ctx context.Context) error {
		if s.capture != nil {
			if err := s.capture.DisableRetainingSelection(ctx); err != nil {
				return &control.Error{Code: "capture_cleanup_failed", Message: "Capture must be withdrawn before native configuration changes."}
			}
		}
		return operation(ctx)
	}
	if s.runtime != nil {
		err := s.runtime.ResourceOperation(ctx, managedruntime.SingBox, apply)
		if errors.Is(err, managedruntime.ErrBusy) {
			return &control.Error{Code: "operation_busy", Message: "Another runtime operation is in progress."}
		}
		return err
	}
	return apply(ctx)
}

func captureDisabledWarning(disabled bool) string {
	if disabled {
		return "Device capture was disabled for native network changes. Confirm or roll back, then explicitly Apply the selected devices again."
	}
	return ""
}

// Cheap generation/draft/risk checks run before withdrawal. Commit still performs
// authoritative native validation under its own lock before any UCI write.
func (s *Server) nativeConfigurationCommit(ctx context.Context, request control.CommitRequest, commit func(context.Context, bool) error) error {
	operation := func(ctx context.Context) error {
		status := s.control.Status()
		if status.Generation != request.Generation {
			return &control.Error{Code: "generation_conflict", Message: "Configuration changed; refresh before committing."}
		}
		if status.PendingCommit != nil {
			return &control.Error{Code: "pending_confirmation", Message: "Confirm or roll back the pending operation first."}
		}
		drafts, err := s.control.Drafts(ctx)
		if err != nil {
			return err
		}
		if len(request.DraftIDs) == 0 || len(request.DraftIDs) > 6 {
			return &control.Error{Code: "invalid_commit", Message: "Select one draft per changed module."}
		}
		ids := map[string]bool{}
		modules := map[string]bool{}
		networkChange := false
		for _, id := range request.DraftIDs {
			if ids[id] {
				return &control.Error{Code: "invalid_commit", Message: "Draft identifiers must be unique."}
			}
			ids[id] = true
			found := false
			for _, draft := range drafts {
				if draft.ID != id {
					continue
				}
				found = true
				if !draft.Valid {
					return &control.Error{Code: "validation_failed", Message: "Selected draft is invalid."}
				}
				if draft.Generation != request.Generation {
					return &control.Error{Code: "generation_conflict", Message: "Selected draft is stale."}
				}
				if modules[draft.Module] {
					return &control.Error{Code: "invalid_commit", Message: "Select one draft per module."}
				}
				modules[draft.Module] = true
				if len(draft.Risks) > 0 && !request.AcknowledgeRisks {
					return &control.Error{Code: "risk_ack_required", Message: "Acknowledge native connectivity risks before committing."}
				}
				switch draft.Module {
				case "network", "wireless", "dhcp", "firewall":
					networkChange = true
				}
			}
			if !found {
				return &control.Error{Code: "draft_not_found", Message: "Selected draft was not found."}
			}
		}
		return commit(ctx, networkChange)
	}
	if s.runtime != nil {
		err := s.runtime.ResourceOperation(ctx, managedruntime.SingBox, operation)
		if errors.Is(err, managedruntime.ErrBusy) {
			return &control.Error{Code: "operation_busy", Message: "Another runtime operation is in progress."}
		}
		return err
	}
	return operation(ctx)
}

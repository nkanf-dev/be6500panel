package httpapi

import (
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
	if !s.configBeforeCommit(w, r) {
		return
	}
	operation, err := s.control.Commit(r.Context(), input)
	if err != nil {
		s.logger.Warn("Configuration commit failed", "code", "configuration_commit_failed", "module", "configuration")
		if operation.ID != "" {
			var typed *control.Error
			if errors.As(err, &typed) {
				writeJSON(w, 409, struct {
					Error     apiError          `json:"error"`
					Operation control.Operation `json:"operation"`
				}{apiError{Code: typed.Code, Message: typed.Message}, operation})
				return
			}
		}
		s.controlError(w, err)
		return
	}
	s.logger.Info("Configuration commit completed", "code", operation.State, "module", "configuration")
	writeJSON(w, 200, operation)
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
		operation, err = s.control.Rollback(r.Context(), input.ID)
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
		case "generation_conflict", "operation_busy", "pending_confirmation", "risk_ack_required":
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

func (s *Server) configBeforeCommit(w http.ResponseWriter, r *http.Request) bool {
	if s.capture == nil {
		return true
	}
	if err := s.capture.Cleanup(r.Context()); err != nil {
		fail(w, 409, "capture_cleanup_failed", "先撤回代理接管规则")
		return false
	}
	return true
}

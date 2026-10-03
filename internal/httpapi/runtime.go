package httpapi

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"

	managedruntime "be6500panel/internal/runtime"
)

type runtimeServiceRequest struct {
	Service string `json:"service"`
}

func (s *Server) runtimeEnabled(w http.ResponseWriter) bool {
	if s.runtime == nil {
		fail(w, 503, "runtime_unavailable", "运行管理未启用")
		return false
	}
	return true
}
func (s *Server) runtimes(w http.ResponseWriter, r *http.Request) {
	states := make([]managedruntime.Status, 0, 2)
	if s.runtime != nil {
		for _, id := range []string{managedruntime.SingBox, managedruntime.FRPC} {
			state, err := s.runtime.Status(id)
			if err == nil {
				states = append(states, state)
			}
		}
	}
	writeJSON(w, 200, struct {
		Enabled  bool                    `json:"enabled"`
		Services []managedruntime.Status `json:"services"`
	}{s.runtime != nil, states})
}
func (s *Server) runtimeAcquire(w http.ResponseWriter, r *http.Request) {
	if !s.runtimeMutationAllowed(w) {
		return
	}
	if !s.runtimeEnabled(w) {
		return
	}
	var input struct {
		Service  string                  `json:"service"`
		Artifact managedruntime.Artifact `json:"artifact"`
	}
	if !decodeJSON(w, r, &input, "service", "artifact") {
		return
	}
	state, err := s.runtime.AcquireGuarded(r.Context(), input.Service, input.Artifact, s.runtimeMutationGuard)
	s.runtimeResult(w, input.Service, "artifact_acquired", state, err)
}
func (s *Server) runtimeConfigure(w http.ResponseWriter, r *http.Request) {
	if !s.runtimeMutationAllowed(w) {
		return
	}
	if !s.runtimeEnabled(w) {
		return
	}
	var input struct {
		Service    string `json:"service"`
		Config     string `json:"config"`
		Generation uint64 `json:"generation"`
	}
	if !decodeJSONLimit(w, r, &input, 2<<20, "service", "config", "generation") {
		return
	}
	state, err := s.runtime.ConfigureGuarded(r.Context(), input.Service, []byte(input.Config), input.Generation, s.runtimeMutationGuard)
	s.runtimeResult(w, input.Service, "config_committed", state, err)
}
func (s *Server) runtimeConfig(w http.ResponseWriter, r *http.Request) {
	if !s.runtimeEnabled(w) {
		return
	}
	id := r.URL.Query().Get("service")
	raw, generation, err := s.runtime.Config(id)
	if err != nil {
		s.runtimeError(w, err)
		return
	}
	writeJSON(w, 200, struct {
		Service    string `json:"service"`
		Config     string `json:"config"`
		Generation uint64 `json:"generation"`
	}{id, string(raw), generation})
}
func (s *Server) runtimeAction(w http.ResponseWriter, r *http.Request, start bool) {
	if start && !s.runtimeMutationAllowed(w) {
		return
	}
	if !s.runtimeEnabled(w) {
		return
	}
	var input runtimeServiceRequest
	if !decodeJSON(w, r, &input, "service") {
		return
	}
	var state managedruntime.Status
	var err error
	code := "runtime_stopped"
	if start {
		state, err = s.runtime.StartGuarded(r.Context(), input.Service, s.runtimeMutationGuard)
		code = "runtime_started"
	} else {
		state, err = s.runtime.Stop(r.Context(), input.Service)
	}
	if err == nil {
		if persistErr := s.persistDesired(input.Service, start); persistErr != nil {
			fail(w, 500, "storage_failed", "开机状态保存失败")
			return
		}
	}
	s.runtimeResult(w, input.Service, code, state, err)
}
func (s *Server) runtimeRestore(w http.ResponseWriter, r *http.Request) {
	if !s.runtimeMutationAllowed(w) {
		return
	}
	if !s.runtimeEnabled(w) {
		return
	}
	var input struct {
		Service    string `json:"service"`
		Generation uint64 `json:"generation"`
	}
	if !decodeJSON(w, r, &input, "service", "generation") {
		return
	}
	state, err := s.runtime.RestoreGuarded(r.Context(), input.Service, input.Generation, s.runtimeMutationGuard)
	s.runtimeResult(w, input.Service, "runtime_config_restored", state, err)
}
func (s *Server) runtimeResult(w http.ResponseWriter, service, code string, state managedruntime.Status, err error) {
	// A refused admission must not persist desired state or report success.
	if runtimeGuardRefusal(w, err) {
		return
	}
	if state.Service != "" && (code == "config_committed" || code == "runtime_config_restored" || code == "artifact_acquired" || state.Restored || state.NeedsRecovery) {
		if persistErr := s.persistDesired(service, state.Desired); persistErr != nil && err == nil {
			fail(w, 500, "storage_failed", "运行目标保存失败；请刷新运行状态")
			return
		}
	}
	if err != nil {
		s.logger.Warn("Runtime operation failed", "module", service, "code", "runtime_operation_failed")
		if state.Service != "" {
			recorder := httptest.NewRecorder()
			s.runtimeError(recorder, err)
			var envelope errorEnvelope
			if json.Unmarshal(recorder.Body.Bytes(), &envelope) == nil {
				if state.Restored {
					envelope.Error.Message += "；已恢复上一可运行配置"
				}
				if state.NeedsRecovery {
					envelope.Error.Message += "；恢复未完成，请检查运行状态"
				}
				writeJSON(w, recorder.Code, struct {
					Error  apiError              `json:"error"`
					Status managedruntime.Status `json:"status"`
				}{envelope.Error, state})
				return
			}
		}
		s.runtimeError(w, err)
		return
	}
	s.logger.Info("Runtime operation completed", "module", service, "code", code)
	writeJSON(w, 200, state)
}
func (s *Server) runtimeError(w http.ResponseWriter, err error) {
	switch {
	case errors.Is(err, managedruntime.ErrGeneration):
		fail(w, 409, "generation_conflict", "配置已更新，请重新读取")
	case errors.Is(err, managedruntime.ErrBusy):
		fail(w, 409, "operation_busy", "另一操作正在执行")
	case errors.Is(err, managedruntime.ErrService):
		fail(w, 400, "invalid_service", "服务类型无效")
	case errors.Is(err, managedruntime.ErrNotConfigured):
		fail(w, 409, "not_configured", "尚未配置")
	case errors.Is(err, managedruntime.ErrNoArtifact):
		fail(w, 409, "artifact_unavailable", "运行文件尚未就绪")
	case errors.Is(err, managedruntime.ErrArtifactCompressedLimit):
		fail(w, 422, "artifact_compressed_limit", "下载文件超过大小限制")
	case errors.Is(err, managedruntime.ErrArtifactUncompressedLimit):
		fail(w, 422, "artifact_uncompressed_limit", "解压文件超过大小限制")
	case errors.Is(err, managedruntime.ErrReadiness):
		fail(w, 503, "readiness_failed", "本地监听未就绪")
	case errors.Is(err, managedruntime.ErrCheck):
		fail(w, 422, "config_check_failed", "配置校验失败，请查看运行诊断")
	case errors.Is(err, context.DeadlineExceeded):
		fail(w, 504, "operation_timeout", "操作超时")
	case errors.Is(err, context.Canceled):
		fail(w, 409, "operation_cancelled", "操作已取消")
	default:
		fail(w, 500, "runtime_operation_failed", "运行操作失败")
	}
}

func (s *Server) persistDesired(service string, desired bool) error {
	s.desiredMu.Lock()
	defer s.desiredMu.Unlock()
	if s.dataDir == "" {
		return nil
	}
	path := filepath.Join(s.dataDir, "desired-services.json")
	values := map[string]bool{}
	if raw, err := os.ReadFile(path); err == nil {
		if err = json.Unmarshal(raw, &values); err != nil {
			return err
		}
	}
	values[service] = desired
	raw, err := json.Marshal(values)
	if err != nil {
		return err
	}
	return s.writePrivate(s.ctx, path, raw, !desired)
}

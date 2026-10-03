package httpapi

import (
	"be6500panel/internal/control"
	"context"
	"errors"
	"net/http"
)

func (s *Server) runtimeRestart(w http.ResponseWriter, r *http.Request) {
	if !s.runtimeMutationAllowed(w) || !s.runtimeEnabled(w) {
		return
	}
	var input runtimeServiceRequest
	if !decodeJSON(w, r, &input, "service") {
		return
	}
	state, err := s.runtime.RestartGuarded(r.Context(), input.Service, func(ctx context.Context) error {
		if err := ctx.Err(); err != nil {
			return err
		}
		if s.control != nil {
			status := s.control.Status()
			if !status.Enabled || status.ErrorCode != "" || status.PendingCommit != nil {
				return &control.Error{Code: "configuration_pending", Message: "请先完成或恢复当前网络配置，再重启服务"}
			}
		}
		return nil
	})
	var guardError *control.Error
	if errors.As(err, &guardError) {
		fail(w, http.StatusConflict, guardError.Code, guardError.Message)
		return
	}
	s.runtimeResult(w, input.Service, "runtime_restarted", state, err)
}

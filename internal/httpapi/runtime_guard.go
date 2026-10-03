package httpapi

import (
	"be6500panel/internal/control"
	"context"
	"errors"
	"net/http"
)

func (s *Server) runtimeMutationGuard(ctx context.Context) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	if s.control == nil {
		return nil
	}
	state := s.control.Status()
	if !state.Enabled || state.ErrorCode != "" || state.PendingCommit != nil {
		return &control.Error{Code: "configuration_pending", Message: "请先完成或恢复当前网络配置，再修改服务运行状态"}
	}
	return nil
}
func runtimeGuardRefusal(w http.ResponseWriter, err error) bool {
	var refusal *control.Error
	if !errors.As(err, &refusal) {
		return false
	}
	fail(w, http.StatusConflict, refusal.Code, refusal.Message)
	return true
}
func (s *Server) runtimeMutationAllowed(w http.ResponseWriter) bool {
	return !runtimeGuardRefusal(w, s.runtimeMutationGuard(context.Background()))
}

package httpapi

import "net/http"

func (s *Server) runtimeMutationAllowed(w http.ResponseWriter) bool {
	if s.control == nil {
		return true
	}
	state := s.control.Status()
	if !state.Enabled || state.ErrorCode != "" || state.PendingCommit != nil {
		fail(w, http.StatusConflict, "configuration_pending", "请先完成或恢复当前网络配置，再修改代理运行状态")
		return false
	}
	return true
}

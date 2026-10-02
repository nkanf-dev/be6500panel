package httpapi

import (
	"net/http"
)

func (s *Server) routerSnapshot(w http.ResponseWriter, r *http.Request) {
	if s.router == nil {
		fail(w, 503, "router_unavailable", "设备适配未启用")
		return
	}
	snapshot, err := s.router.Snapshot(r.Context())
	if err != nil {
		fail(w, 503, "observation_unavailable", "设备采样不可用")
		return
	}
	writeJSON(w, 200, snapshot)
}

package httpapi

import (
	"errors"
	"net/http"

	"be6500panel/internal/telemetry"
)

func (s *Server) proxyMetrics(w http.ResponseWriter, r *http.Request) {
	if s.telemetry == nil {
		writeJSON(w, http.StatusOK, telemetry.Unavailable("代理观测未启用；需要支持本机观测接口的核心"))
		return
	}
	writeJSON(w, http.StatusOK, s.telemetry.Snapshot())
}

func (s *Server) proxyProbe(w http.ResponseWriter, r *http.Request) {
	var input struct{}
	if !decodeJSON(w, r, &input) {
		return
	}
	if s.telemetry == nil {
		fail(w, http.StatusServiceUnavailable, "telemetry_unavailable", "代理观测未启用")
		return
	}
	if err := s.telemetry.Probe(r.Context()); err != nil {
		switch {
		case errors.Is(err, telemetry.ErrBusy):
			fail(w, http.StatusConflict, "probe_busy", "当前节点探测正在进行")
		case errors.Is(err, telemetry.ErrCooldown):
			w.Header().Set("Retry-After", "10")
			fail(w, http.StatusTooManyRequests, "probe_cooldown", "稍后再探测当前节点")
		case errors.Is(err, telemetry.ErrUnavailable):
			fail(w, http.StatusServiceUnavailable, "telemetry_unavailable", "代理观测接口未就绪")
		default:
			fail(w, http.StatusBadGateway, "probe_failed", "当前节点探测失败；请查看观测状态")
		}
		return
	}
	writeJSON(w, http.StatusOK, s.telemetry.Snapshot())
}

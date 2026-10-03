package httpapi

import (
	"errors"
	"net/http"

	"be6500panel/internal/requesttrace"
)

// HandleRequestTraces is mounted by Server behind its existing authentication
// and same-origin mutation checks. GET never starts a network request. POST
// accepts only the explicit preset ID and route, never a URL or listener address.
func HandleRequestTraces(w http.ResponseWriter, r *http.Request, collector *requesttrace.Collector) {
	if collector == nil {
		fail(w, 503, "request_trace_unavailable", "网络诊断未启用")
		return
	}
	switch r.Method {
	case http.MethodGet:
		if r.URL.RawQuery != "" {
			fail(w, 400, "invalid_input", "诊断历史不接受查询参数")
			return
		}
		writeJSON(w, 200, collector.Snapshot())
	case http.MethodPost:
		if r.URL.RawQuery != "" {
			fail(w, 400, "invalid_input", "诊断测试不接受查询参数")
			return
		}
		var input requesttrace.Input
		if !decodeJSONLimit(w, r, &input, 1024, "targetId", "route") {
			return
		}
		trace, err := collector.Run(r.Context(), input)
		switch {
		case errors.Is(err, requesttrace.ErrInput):
			fail(w, 400, "invalid_input", "请选择固定诊断目标与直连或当前代理链路")
		case errors.Is(err, requesttrace.ErrBusy):
			fail(w, 409, "request_trace_busy", "网络诊断正在进行，请等待当前测试完成")
		case errors.Is(err, requesttrace.ErrUnavailable):
			fail(w, 503, "request_trace_unavailable", "当前已接受的代理监听不可用；请检查核心与原生配置")
		case err != nil:
			fail(w, 500, "request_trace_failed", "无法记录诊断结果")
		default:
			writeJSON(w, 200, trace)
		}
	default:
		w.Header().Set("Allow", "GET, POST")
		fail(w, 405, "method_not_allowed", "Method is not allowed for this endpoint.")
	}
}

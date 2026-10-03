package httpapi

import (
	"net/http"
	"strings"

	"be6500panel/internal/router"
)

// ServiceObservation is intended for an authenticated GET /api/system/services
// route. Reuse one observer per server so clients share the bounded source cache.
func ServiceObservation(w http.ResponseWriter, r *http.Request, observer *router.ServiceObserver) {
	if observer == nil {
		fail(w, 503, "service_observation_unavailable", "服务状态采样未启用")
		return
	}
	snapshot, err := observer.Snapshot(r.Context())
	if err != nil {
		fail(w, 503, "service_observation_unavailable", "服务状态采样中断")
		return
	}
	writeJSON(w, 200, snapshot)
}

// ServiceAction is intended for authenticated same-origin POST routing only.
// The root server must also hold runtimeResourceOperation and check control
// enabled/error/pending state before calling this helper.
func ServiceAction(w http.ResponseWriter, r *http.Request, observer *router.ServiceObserver) {
	if observer == nil {
		fail(w, 503, "service_observation_unavailable", "服务状态采样未启用")
		return
	}
	var input router.ServiceActionRequest
	if !decodeJSONLimit(w, r, &input, 1024, "service", "action", "confirmImpact") {
		return
	}
	result, err := observer.Action(r.Context(), input)
	if err == nil {
		writeJSON(w, 200, result)
		return
	}
	status := http.StatusConflict
	if result.ErrorCode == "service_action_not_allowed" {
		status = http.StatusBadRequest
	}
	if strings.HasPrefix(result.ErrorCode, "service_command_") {
		status = http.StatusServiceUnavailable
	}
	writeJSON(w, status, struct {
		router.ServiceActionResult
		Error apiError `json:"error"`
	}{result, apiError{Code: result.ErrorCode, Message: "服务操作未完成，请查看当前实际状态；未报告远端健康。"}})
}

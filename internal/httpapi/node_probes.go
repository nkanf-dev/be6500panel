package httpapi

import (
	"be6500panel/internal/nodeprobe"
	"errors"
	"net/http"
)

const NodeProbesPath = "/api/proxy/node-probes"

// HandleNodeProbes is mounted behind the server's authentication and same-origin
// guards. GET only returns stored outcomes. Only explicit POST starts a job.
func HandleNodeProbes(w http.ResponseWriter, r *http.Request, manager *nodeprobe.Manager) {
	if manager == nil {
		fail(w, http.StatusServiceUnavailable, "probes_unavailable", "节点测速未启用；请先在运行管理中获取已校验的 sing-box 运行文件。")
		return
	}
	switch r.Method {
	case http.MethodGet:
		writeJSON(w, http.StatusOK, manager.Snapshot())
	case http.MethodDelete:
		writeJSON(w, http.StatusOK, manager.Cancel())
	case http.MethodPost:
		var input nodeprobe.StartInput
		if !decodeJSONLimit(w, r, &input, 40*1024, "all", "nodeIds", "revision") {
			return
		}
		snapshot, err := manager.Start(r.Context(), input)
		if err != nil {
			switch {
			case errors.Is(err, nodeprobe.ErrBusy):
				fail(w, 409, "probe_busy", "节点测速正在进行；请先停止当前任务。")
			case errors.Is(err, nodeprobe.ErrRevision):
				fail(w, 409, "revision_mismatch", "订阅已变化，请刷新节点列表后再测速。")
			case errors.Is(err, nodeprobe.ErrNodes):
				fail(w, 400, "invalid_nodes", "请选择当前订阅节点；每批最多 256 个，全部测速不会截断节点。")
			case errors.Is(err, nodeprobe.ErrClosed):
				fail(w, 503, "probe_closed", "节点测速服务已停止。")
			default:
				fail(w, 503, "artifact_unavailable", "请先在运行管理中获取已校验的 sing-box 运行文件；无需启动或切换当前代理。")
			}
			return
		}
		writeJSON(w, http.StatusAccepted, snapshot)
	default:
		w.Header().Set("Allow", "GET, POST, DELETE")
		fail(w, 405, "method_not_allowed", "Method is not allowed for this endpoint.")
	}
}

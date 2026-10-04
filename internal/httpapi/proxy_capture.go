package httpapi

import (
	"context"
	"errors"
	"net"
	"net/http"
	"strings"
	"time"

	"be6500panel/internal/capture"
	"be6500panel/internal/proxy"
	managedruntime "be6500panel/internal/runtime"
)

func (s *Server) proxyCapture(w http.ResponseWriter, r *http.Request) {
	if s.capture == nil {
		fail(w, 503, "capture_unavailable", "透明接管未启用")
		return
	}
	if r.Method == http.MethodGet {
		observeCtx, cancelObserve := context.WithTimeout(r.Context(), 3*time.Second)
		defer cancelObserve()
		// ReconcileDesired snapshots through TryLock and covers both enabled and
		// empty/off intent. Do not block on Desired() before its finite read path.
		status, err := s.capture.ReconcileDesired(observeCtx)
		if err != nil {
			code := status.Error
			if !strings.HasPrefix(code, "capture_") || len(code) > 80 {
				code = "capture_observation_failed"
			}
			s.logger.Warn("Capture verification requires attention", "code", code, "module", "proxy")
		}
		writeJSON(w, 200, status)
		return
	}
	if r.Method == http.MethodDelete {
		if err := s.capture.Disable(r.Context()); err != nil {
			status := s.capture.Status()
			code, message := "cleanup_failed", "接管清理尚未完成，请重试撤回并检查当前状态"
			if status.Error == "capture_disable_not_persisted" {
				code, message = status.Error, "已停止自动恢复接管，但关闭状态尚未保存；请重试撤回。保存成功前不要重启面板或路由器。"
			}
			writeJSON(w, 500, struct {
				Error   apiError       `json:"error"`
				Capture capture.Status `json:"capture"`
			}{apiError{Code: code, Message: message}, status})
			return
		}
		s.logger.Info("Capture withdrawn", "code", "capture_disabled", "module", "proxy")
		writeJSON(w, 200, s.capture.Status())
		return
	}
	if !s.runtimeEnabled(w) {
		return
	}
	var input struct {
		Scope      proxy.CaptureScope        `json:"scope"`
		Devices    []capture.DeviceSelection `json:"devices"`
		ClientIPv4 string                    `json:"clientIPv4"`
		ClientIPv6 string                    `json:"clientIPv6"`
		IPv6       proxy.IPv6Mode            `json:"ipv6"`
	}
	if !decodeJSON(w, r, &input, "ipv6") {
		return
	}
	if input.Scope != "" && input.Scope != proxy.CaptureScopeDevices && input.Scope != proxy.CaptureScopeGateway {
		fail(w, 400, "invalid_input", "接管范围无效")
		return
	}
	if input.Scope == proxy.CaptureScopeGateway && (len(input.Devices) > 0 || input.ClientIPv4 != "" || input.ClientIPv6 != "" || input.IPv6 != proxy.IPv6Direct) {
		fail(w, 400, "invalid_input", "网关范围使用观测到的 IPv4 局域网网段")
		return
	}
	// Legacy explicit IPv4 selects a stable observed MAC. An unresolved address
	// cannot become a saved identity and capture a different DHCP client later.
	if len(input.Devices) == 0 && input.ClientIPv4 != "" && s.router != nil {
		if observation, err := s.router.CaptureObservation(r.Context()); err == nil {
			for _, device := range observation.Devices {
				if device.IP == input.ClientIPv4 && device.Eligible {
					input.Devices = []capture.DeviceSelection{{MAC: device.MAC}}
					input.ClientIPv4 = ""
					break
				}
			}
		}
	}
	if len(input.Devices) == 0 && input.ClientIPv4 != "" {
		fail(w, 409, "capture_client_unresolved", "设备地址未在当前 LAN 观测中匹配，请选择已发现设备")
		return
	}
	var status capture.Status
	err := s.runtime.ReadyOperation(r.Context(), managedruntime.SingBox, func(ctx context.Context) error {
		if s.control != nil {
			configuration := s.control.Status()
			if !configuration.Enabled || configuration.ErrorCode != "" || configuration.PendingCommit != nil {
				return errors.New("capture_configuration_pending")
			}
		}
		desired := capture.Desired{Scope: input.Scope, Devices: input.Devices, ClientIPv4: input.ClientIPv4, ClientIPv6: input.ClientIPv6, IPv6: input.IPv6}
		if input.Scope == proxy.CaptureScopeGateway {
			if s.router == nil {
				return errors.New("capture_lan_unavailable")
			}
			observed, err := s.router.CaptureObservation(ctx)
			if err != nil {
				return errors.New("capture_lan_unavailable")
			}
			desired, err = gatewayDesiredFromObservation(observed)
			if err != nil {
				return err
			}
		}
		var err error
		status, err = s.capture.Select(ctx, desired)
		return err
	})
	if err != nil {
		s.logger.Warn("Capture activation failed", "code", "capture_failed", "module", "proxy")
		if errors.Is(err, managedruntime.ErrReadiness) {
			fail(w, 409, "proxy_not_running", "先启动代理服务")
			return
		}
		if errors.Is(err, managedruntime.ErrBusy) {
			s.runtimeError(w, err)
			return
		}
		fail(w, 409, "capture_failed", "接管应用失败，查看日志与规则状态")
		return
	}
	s.logger.Info("Gateway or diagnostic capture active", "code", "capture_active", "module", "proxy")
	writeJSON(w, 200, status)
}

// peerIP ignores forwarded headers. Only the actual HTTP connection peer is
// eligible for the current-terminal marker.
func peerIP(r *http.Request) string {
	host, _, err := net.SplitHostPort(r.RemoteAddr)
	if err != nil {
		return ""
	}
	ip := net.ParseIP(host)
	if ip == nil {
		return ""
	}
	return ip.String()
}

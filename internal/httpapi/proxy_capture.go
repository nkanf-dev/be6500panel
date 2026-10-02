package httpapi

import (
	"context"
	"errors"
	"net"
	"net/http"

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
		var status capture.Status
		var err error
		if s.capture.Desired().Enabled {
			status, err = s.capture.ReconcileDesired(r.Context())
		} else {
			status, err = s.capture.Reconcile(r.Context())
		}
		if err != nil {
			s.logger.Warn("Capture state needs recovery", "code", "capture_reconcile_failed", "module", "proxy")
		}
		writeJSON(w, 200, status)
		return
	}
	if r.Method == http.MethodDelete {
		if err := s.capture.Disable(r.Context()); err != nil {
			fail(w, 500, "cleanup_failed", "接管规则撤回失败")
			return
		}
		writeJSON(w, 200, s.capture.Status())
		return
	}
	if !s.runtimeEnabled(w) {
		return
	}
	var input struct {
		Devices    []capture.DeviceSelection `json:"devices"`
		ClientIPv4 string                    `json:"clientIPv4"`
		ClientIPv6 string                    `json:"clientIPv6"`
		IPv6       proxy.IPv6Mode            `json:"ipv6"`
	}
	if !decodeJSON(w, r, &input, "ipv6") {
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
		if s.control != nil && s.control.Status().PendingCommit != nil {
			return errors.New("capture_configuration_pending")
		}
		var err error
		status, err = s.capture.Select(ctx, capture.Desired{Devices: input.Devices, ClientIPv4: input.ClientIPv4, ClientIPv6: input.ClientIPv6, IPv6: input.IPv6})
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
	s.logger.Info("Selected-device capture active", "code", "capture_active", "module", "proxy")
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

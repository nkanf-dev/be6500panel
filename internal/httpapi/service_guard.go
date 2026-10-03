package httpapi

import (
	managedruntime "be6500panel/internal/runtime"
	"context"
	"net/http"
)

// Native actions share the network transaction lane with accepted runtime and
// UCI changes. The authoritative configuration gate is checked after admission,
// not just before waiting for a lane. Rescue is excluded by the observer's fixed
// allowlist, independently of this UI/API guard.
func (s *Server) nativeServiceAction(w http.ResponseWriter, r *http.Request) {
	if !s.controlEnabled(w) {
		return
	}
	if s.runtime == nil {
		fail(w, http.StatusServiceUnavailable, "runtime_unavailable", "服务操作协调器未启用")
		return
	}
	err := s.runtime.ResourceOperation(r.Context(), managedruntime.SingBox, func(ctx context.Context) error {
		if !s.runtimeMutationAllowed(w) {
			return nil
		}
		ServiceAction(w, r.WithContext(ctx), s.services)
		return nil
	})
	if err != nil {
		s.runtimeError(w, err)
	}
}

package httpapi

import "net/http"

func (s *Server) runtimeRestart(w http.ResponseWriter, r *http.Request) {
	if !s.runtimeMutationAllowed(w) || !s.runtimeEnabled(w) {
		return
	}
	var input runtimeServiceRequest
	if !decodeJSON(w, r, &input, "service") {
		return
	}
	state, err := s.runtime.RestartGuarded(r.Context(), input.Service, s.runtimeMutationGuard)
	s.runtimeResult(w, input.Service, "runtime_restarted", state, err)
}

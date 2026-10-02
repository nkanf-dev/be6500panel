package httpapi

import (
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"time"

	"be6500panel/internal/core"
)

func (s *Server) events(w http.ResponseWriter, r *http.Request) {
	if _, ok := w.(http.Flusher); !ok {
		fail(w, 500, "stream_unavailable", "Streaming is unavailable.")
		return
	}
	ch, unsubscribe, err := s.sampler.Subscribe()
	if err != nil {
		code := "observation_unavailable"
		if errors.Is(err, core.ErrTooManySubscribers) {
			code = "stream_limit"
		}
		fail(w, 503, code, "System event stream is unavailable.")
		return
	}
	defer unsubscribe()
	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.Header().Set("X-Accel-Buffering", "no")
	controller := http.NewResponseController(w)
	// Flush headers immediately. Limit any slow-client write to five seconds.
	if err := controller.SetWriteDeadline(time.Now().Add(5 * time.Second)); err != nil && !errors.Is(err, http.ErrNotSupported) {
		return
	}
	if err := controller.Flush(); err != nil {
		return
	}
	ticker := time.NewTicker(s.heartbeat)
	defer ticker.Stop()
	for {
		var message string
		select {
		case <-r.Context().Done():
			return
		case <-s.ctx.Done():
			return
		case event, ok := <-ch:
			if !ok {
				return
			}
			if !s.auth.authenticated(r) {
				return
			}
			data, err := json.Marshal(event.Snapshot)
			if err != nil {
				return
			}
			message = fmt.Sprintf("id: %d\nevent: snapshot\ndata: %s\n\n", event.ID, data)
		case <-ticker.C:
			if !s.auth.authenticated(r) {
				return
			}
			message = ": heartbeat\n\n"
		}
		if err := controller.SetWriteDeadline(time.Now().Add(5 * time.Second)); err != nil && !errors.Is(err, http.ErrNotSupported) {
			return
		}
		if _, err := fmt.Fprint(w, message); err != nil {
			return
		}
		if err := controller.Flush(); err != nil {
			return
		}
	}
}

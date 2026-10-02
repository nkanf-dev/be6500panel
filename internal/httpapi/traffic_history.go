package httpapi

import (
	"errors"
	"net/http"
	"strconv"

	"be6500panel/internal/traffic"
)

// TrafficHistory serves the authenticated route wired by Server. It also
// remains independently testable without starting a browser or collector.
func TrafficHistory(w http.ResponseWriter, r *http.Request, collector *traffic.Collector) {
	query := r.URL.Query()
	name := query.Get("range")
	if name == "" {
		name = "30m"
	}
	maxPoints := traffic.DefaultMaxPoints
	if values, ok := query["maxPoints"]; ok {
		if len(values) != 1 {
			fail(w, 400, "invalid_input", "Provide one maxPoints value.")
			return
		}
		var err error
		maxPoints, err = strconv.Atoi(values[0])
		if err != nil {
			fail(w, 400, "invalid_input", "maxPoints must be an integer from 1 to 2000.")
			return
		}
	}
	if len(query["range"]) > 1 {
		fail(w, 400, "invalid_input", "Provide one range value.")
		return
	}
	if err := traffic.ValidateQuery(name, maxPoints); err != nil {
		fail(w, 400, "invalid_input", err.Error())
		return
	}
	if collector == nil {
		writeJSON(w, 200, traffic.Disabled(name))
		return
	}
	history, err := collector.Query(r.Context(), name, maxPoints)
	if err != nil {
		if errors.Is(err, traffic.ErrRange) || errors.Is(err, traffic.ErrMaxPoints) {
			fail(w, 400, "invalid_input", err.Error())
			return
		}
		fail(w, 503, "history_unavailable", "Traffic history is unavailable.")
		return
	}
	writeJSON(w, 200, history)
}

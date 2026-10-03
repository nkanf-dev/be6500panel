package httpapi

import (
	"net/http"
	"strconv"

	"be6500panel/internal/devicetelemetry"
)

// DeviceActivity serves the authenticated GET route wired by Server. Refresh
// queries memory and never invokes trafficd or starts browser-driven collection.
func DeviceActivity(w http.ResponseWriter, r *http.Request, collector *devicetelemetry.Collector) {
	query := r.URL.Query()
	for key, values := range query {
		if (key != "range" && key != "maxPoints" && key != "limit" && key != "search") || len(values) != 1 {
			fail(w, 400, "invalid_input", "Provide one value for each supported device history parameter.")
			return
		}
	}
	name := query.Get("range")
	if name == "" {
		name = "24h"
	}
	maxPoints, limit := 288, 32
	for key, target := range map[string]*int{"maxPoints": &maxPoints, "limit": &limit} {
		if value, ok := query[key]; ok {
			parsed, err := strconv.Atoi(value[0])
			if err != nil {
				fail(w, 400, "invalid_input", devicetelemetry.ErrQuery.Error())
				return
			}
			*target = parsed
		}
	}
	search := query.Get("search")
	if err := devicetelemetry.ValidateQuery(name, maxPoints, limit, search); err != nil {
		fail(w, 400, "invalid_input", err.Error())
		return
	}
	if collector == nil {
		writeJSON(w, 200, devicetelemetry.Disabled(name))
		return
	}
	history, err := collector.Query(r.Context(), name, maxPoints, limit, search)
	if err != nil {
		fail(w, 503, "observation_unavailable", "Device traffic history is unavailable.")
		return
	}
	writeJSON(w, 200, history)
}

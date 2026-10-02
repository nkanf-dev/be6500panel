package httpapi

import (
	"be6500panel/internal/traffic"
	"net/http"
	"strconv"
)

func (s *Server) trafficHistory(w http.ResponseWriter, r *http.Request) {
	if s.traffic == nil && s.trafficError != "" {
		query := r.URL.Query()
		name := query.Get("range")
		if name == "" {
			name = "30m"
		}
		maxPoints := traffic.DefaultMaxPoints
		if len(query["range"]) > 1 || len(query["maxPoints"]) > 1 {
			TrafficHistory(w, r, nil)
			return
		}
		if value := query.Get("maxPoints"); value != "" {
			var err error
			maxPoints, err = strconv.Atoi(value)
			if err != nil {
				TrafficHistory(w, r, nil)
				return
			}
		} else if _, exists := query["maxPoints"]; exists {
			TrafficHistory(w, r, nil)
			return
		}
		if err := traffic.ValidateQuery(name, maxPoints); err != nil {
			TrafficHistory(w, r, nil)
			return
		}
		h := traffic.Disabled(name)
		h.RetentionDays = traffic.RetentionDays
		h.Error = s.trafficError
		writeJSON(w, http.StatusOK, h)
		return
	}
	TrafficHistory(w, r, s.traffic)
}

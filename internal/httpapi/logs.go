package httpapi

import (
	"be6500panel/internal/core"
	"net/http"
	"net/url"
	"strconv"
)

func (s *Server) logsResponse(w http.ResponseWriter, r *http.Request) {
	query, err := url.ParseQuery(r.URL.RawQuery)
	if err != nil {
		fail(w, 400, "invalid_input", "Invalid query parameters.")
		return
	}
	limit := 100
	for key, values := range query {
		if key != "limit" || len(values) != 1 {
			fail(w, 400, "invalid_input", "Only one limit parameter is accepted.")
			return
		}
	}
	if text, ok := query["limit"]; ok {
		limit, err = strconv.Atoi(text[0])
		if err != nil || limit < 1 || limit > core.LogCapacity {
			fail(w, 400, "invalid_input", "limit must be 1..500.")
			return
		}
	}
	writeJSON(w, 200, struct {
		Entries  []core.LogEntry `json:"entries"`
		Capacity int             `json:"capacity"`
	}{s.logs.Entries(limit), core.LogCapacity})
}

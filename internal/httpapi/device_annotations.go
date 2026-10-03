package httpapi

import (
	"context"
	"errors"
	"net/http"

	"be6500panel/internal/deviceannotations"
	"be6500panel/internal/storage"
)

const DeviceAnnotationsPath = "/api/devices/annotations"

// DeviceAnnotationsHandler serves the fixed annotation GET/POST contract.
// Registration: add DeviceAnnotationsPath as a GET route, allow POST for that
// route, then dispatch here AFTER Server's existing origin and authentication
// checks. Do not register this helper as an unauthenticated/public route.
// Construct one shared handler/store at startup, not one Store per request.
func DeviceAnnotationsHandler(store *deviceannotations.Store) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Cache-Control", "no-store")
		if r.Method != http.MethodGet && r.Method != http.MethodPost {
			w.Header().Set("Allow", "GET, POST")
			fail(w, http.StatusMethodNotAllowed, "method_not_allowed", "Method is not allowed for this endpoint.")
			return
		}
		if store == nil {
			fail(w, http.StatusServiceUnavailable, "annotations_unavailable", "Persistent device annotations are not available.")
			return
		}
		var snapshot deviceannotations.Snapshot
		var err error
		if r.Method == http.MethodGet {
			snapshot, err = store.Snapshot(r.Context())
		} else {
			var input deviceannotations.UpdateRequest
			if !decodeJSON(w, r, &input, "mac", "label", "note", "tags", "expectedRevision") {
				return
			}
			snapshot, err = store.Save(r.Context(), input)
		}
		if err != nil {
			deviceAnnotationError(w, err)
			return
		}
		writeJSON(w, http.StatusOK, snapshot)
	})
}

func deviceAnnotationError(w http.ResponseWriter, err error) {
	switch {
	case errors.Is(err, deviceannotations.ErrConflict):
		fail(w, http.StatusConflict, "revision_conflict", "Device annotations changed; refresh before saving.")
	case errors.Is(err, deviceannotations.ErrRevisionExhausted):
		fail(w, http.StatusConflict, "revision_exhausted", "Device annotation revision limit reached.")
	case errors.Is(err, deviceannotations.ErrLimit):
		fail(w, http.StatusBadRequest, "annotation_limit", "Device annotations are limited to 256 MAC addresses.")
	case errors.Is(err, deviceannotations.ErrInvalidInput):
		fail(w, http.StatusBadRequest, "invalid_input", "Provide a six-byte MAC address, label up to 80 characters, note up to 1000 characters, and at most 8 tags of up to 32 characters.")
	case errors.Is(err, context.Canceled), errors.Is(err, context.DeadlineExceeded):
		fail(w, http.StatusRequestTimeout, "cancelled", "Device annotation operation was cancelled.")
	case errors.Is(err, storage.ErrInsufficientSpace), errors.Is(err, storage.ErrMeasurement):
		fail(w, http.StatusInsufficientStorage, "storage_insufficient", "Persistent storage needs free space for safe configuration recovery.")
	case errors.Is(err, deviceannotations.ErrStorage):
		fail(w, http.StatusInternalServerError, "storage_failed", "Cannot persist or load device annotations.")
	default:
		fail(w, http.StatusInternalServerError, "annotations_failed", "Device annotation operation failed.")
	}
}

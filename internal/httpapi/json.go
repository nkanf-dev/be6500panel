package httpapi

import (
	"bytes"
	"encoding/json"
	"errors"
	"io"
	"mime"
	"net/http"
	"reflect"
)

const MaxBodyBytes = 64 * 1024

type errorEnvelope struct {
	Error apiError `json:"error"`
}
type apiError struct {
	Code    string `json:"code"`
	Message string `json:"message"`
}

func writeJSON(w http.ResponseWriter, status int, value any) {
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(value)
}
func fail(w http.ResponseWriter, status int, code, message string) {
	writeJSON(w, status, errorEnvelope{Error: apiError{Code: code, Message: message}})
}

func decodeJSON(w http.ResponseWriter, r *http.Request, dst any, required ...string) bool {
	media, _, err := mime.ParseMediaType(r.Header.Get("Content-Type"))
	if err != nil || media != "application/json" {
		fail(w, 415, "unsupported_media_type", "Content-Type must be application/json.")
		return false
	}
	r.Body = http.MaxBytesReader(w, r.Body, MaxBodyBytes)
	body, err := io.ReadAll(r.Body)
	if err != nil {
		var tooLarge *http.MaxBytesError
		if errors.As(err, &tooLarge) {
			fail(w, 413, "body_too_large", "Request body exceeds 64 KiB.")
		} else {
			fail(w, 400, "invalid_json", "Cannot read JSON body.")
		}
		return false
	}
	decoder := json.NewDecoder(bytes.NewReader(body))
	if err := checkJSONValue(decoder); err != nil {
		fail(w, 400, "invalid_json", "Provide one JSON object with unique field names.")
		return false
	}
	if _, err := decoder.Token(); err != io.EOF {
		fail(w, 400, "invalid_json", "Provide only one JSON object.")
		return false
	}
	var object map[string]json.RawMessage
	if err := json.Unmarshal(body, &object); err != nil || object == nil {
		fail(w, 400, "invalid_json", "JSON body must be an object.")
		return false
	}
	for _, field := range required {
		v, ok := object[field]
		if !ok || bytes.Equal(bytes.TrimSpace(v), []byte("null")) {
			fail(w, 400, "invalid_input", "Required fields must be present and non-null.")
			return false
		}
	}
	if err := exactFields(body, reflect.TypeOf(dst)); err != nil {
		fail(w, 400, "invalid_json", "JSON fields or types do not match the request contract.")
		return false
	}
	decoder = json.NewDecoder(bytes.NewReader(body))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(dst); err != nil {
		fail(w, 400, "invalid_json", "JSON fields or types do not match the request contract.")
		return false
	}
	return true
}

// Reject duplicate names at every object level before struct decoding. This also
// bounds nesting so adversarial input does not cause a deep recursive walk.
func checkJSONValue(decoder *json.Decoder) error { return checkJSONDepth(decoder, 0) }
func checkJSONDepth(decoder *json.Decoder, depth int) error {
	if depth > 32 {
		return errors.New("JSON too deeply nested")
	}
	token, err := decoder.Token()
	if err != nil {
		return err
	}
	delimiter, ok := token.(json.Delim)
	if !ok {
		return nil
	}
	if delimiter == '{' {
		names := map[string]bool{}
		for decoder.More() {
			token, err := decoder.Token()
			if err != nil {
				return err
			}
			key, ok := token.(string)
			if !ok || names[key] {
				return errors.New("duplicate key")
			}
			names[key] = true
			if err := checkJSONDepth(decoder, depth+1); err != nil {
				return err
			}
		}
	} else if delimiter == '[' {
		for decoder.More() {
			if err := checkJSONDepth(decoder, depth+1); err != nil {
				return err
			}
		}
	} else {
		return errors.New("unexpected delimiter")
	}
	_, err = decoder.Token()
	return err
}

// encoding/json matches struct keys case-insensitively. Contracts do not: check
// exact json tag names at every level so variants cannot override named fields.
func exactFields(raw json.RawMessage, kind reflect.Type) error {
	for kind.Kind() == reflect.Pointer {
		kind = kind.Elem()
	}
	if bytes.Equal(bytes.TrimSpace(raw), []byte("null")) {
		return errors.New("null field")
	}
	switch kind.Kind() {
	case reflect.Struct:
		var fields map[string]json.RawMessage
		if err := json.Unmarshal(raw, &fields); err != nil || fields == nil {
			return errors.New("expected object")
		}
		known := map[string]reflect.Type{}
		for i := 0; i < kind.NumField(); i++ {
			field := kind.Field(i)
			name := field.Tag.Get("json")
			for j, ch := range name {
				if ch == ',' {
					name = name[:j]
					break
				}
			}
			if name != "" && name != "-" {
				known[name] = field.Type
			}
		}
		for name, value := range fields {
			fieldType, ok := known[name]
			if !ok {
				return errors.New("unknown field")
			}
			if err := exactFields(value, fieldType); err != nil {
				return err
			}
		}
	case reflect.Slice, reflect.Array:
		var values []json.RawMessage
		if err := json.Unmarshal(raw, &values); err != nil {
			return err
		}
		for _, value := range values {
			if err := exactFields(value, kind.Elem()); err != nil {
				return err
			}
		}
	}
	return nil
}

package maintenance

import (
	"be6500panel/internal/control"
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io"
	"unicode/utf8"
)

func digest(content string) string {
	sum := sha256.Sum256([]byte(content))
	return hex.EncodeToString(sum[:])
}
func native(scope string) bool {
	for _, module := range NativeScopes {
		if module == scope {
			return true
		}
	}
	return false
}
func runtimeService(scope string) string {
	switch scope {
	case "runtime.frpc":
		return "frpc"
	case "runtime.sing-box":
		return "sing-box"
	}
	return ""
}
func validScopes(scopes []string) error {
	if len(scopes) == 0 || len(scopes) > 8 {
		return failure("invalid_scope", "Select one or more supported configuration scopes.")
	}
	seen := map[string]bool{}
	for _, scope := range scopes {
		if seen[scope] {
			return failure("duplicate_scope", "Backup scopes must be unique.")
		}
		if !native(scope) && runtimeService(scope) == "" {
			return failure("invalid_scope", "Only the six native configuration modules and explicit runtime settings are supported.")
		}
		seen[scope] = true
	}
	return nil
}

// Decode rejects duplicate keys (at every depth), non-exact field names, nulls,
// extra members, unsupported scope and mismatched content digests. A backup is
// data, not a tar member list, path, command or file restoration instruction.
func Decode(raw []byte) (Envelope, error) {
	var envelope Envelope
	if len(raw) > MaxBackupBytes {
		return envelope, failure("backup_too_large", "Backup exceeds the 2 MiB JSON limit.")
	}
	if !utf8.Valid(raw) {
		return envelope, failure("invalid_backup", "Backup must contain valid UTF-8 JSON.")
	}
	decoder := json.NewDecoder(bytes.NewReader(raw))
	if err := uniqueJSON(decoder, 0); err != nil {
		return envelope, failure("invalid_backup", "Provide one JSON backup with unique field names.")
	}
	if _, err := decoder.Token(); err != io.EOF {
		return envelope, failure("invalid_backup", "Provide exactly one JSON backup object.")
	}
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(raw, &fields); err != nil || !requiredFields(fields, []string{"model", "build", "createdAt", "generation", "scopes", "documents"}, nil) {
		return envelope, failure("invalid_backup", "Backup fields must match the scoped configuration contract.")
	}
	var documents []map[string]json.RawMessage
	if err := json.Unmarshal(fields["documents"], &documents); err != nil || len(documents) == 0 || len(documents) > 8 {
		return envelope, failure("invalid_backup", "Backup must contain its selected configuration documents.")
	}
	for _, document := range documents {
		if !requiredFields(document, []string{"module", "content", "digest"}, []string{"generation"}) {
			return envelope, failure("invalid_backup", "Document fields must match the scoped configuration contract.")
		}
	}
	decoder = json.NewDecoder(bytes.NewReader(raw))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(&envelope); err != nil {
		return envelope, failure("invalid_backup", "Backup fields or values are malformed.")
	}
	if err := validateEnvelope(envelope); err != nil {
		return Envelope{}, err
	}
	return envelope, nil
}
func requiredFields(fields map[string]json.RawMessage, required, optional []string) bool {
	if fields == nil {
		return false
	}
	known := map[string]bool{}
	for _, name := range required {
		value, ok := fields[name]
		if !ok || bytes.Equal(bytes.TrimSpace(value), []byte("null")) {
			return false
		}
		known[name] = true
	}
	for _, name := range optional {
		known[name] = true
	}
	for name, value := range fields {
		if !known[name] || bytes.Equal(bytes.TrimSpace(value), []byte("null")) {
			return false
		}
	}
	return true
}
func uniqueJSON(decoder *json.Decoder, depth int) error {
	if depth > 16 {
		return errors.New("nesting limit")
	}
	token, err := decoder.Token()
	if err != nil {
		return err
	}
	delimiter, ok := token.(json.Delim)
	if !ok {
		return nil
	}
	switch delimiter {
	case '{':
		seen := map[string]bool{}
		for decoder.More() {
			token, err := decoder.Token()
			if err != nil {
				return err
			}
			key, ok := token.(string)
			if !ok || seen[key] {
				return errors.New("duplicate key")
			}
			seen[key] = true
			if err := uniqueJSON(decoder, depth+1); err != nil {
				return err
			}
		}
	case '[':
		for decoder.More() {
			if err := uniqueJSON(decoder, depth+1); err != nil {
				return err
			}
		}
	default:
		return errors.New("unexpected delimiter")
	}
	_, err = decoder.Token()
	return err
}
func validateEnvelope(envelope Envelope) error {
	if envelope.Model == "" || len(envelope.Model) > 128 || envelope.Build == "" || len(envelope.Build) > 256 || envelope.CreatedAt.IsZero() || envelope.Generation == 0 {
		return failure("invalid_manifest", "Backup must identify the router model, build, time and accepted generation.")
	}
	if err := validScopes(envelope.Scopes); err != nil {
		return err
	}
	if len(envelope.Documents) != len(envelope.Scopes) {
		return failure("missing_component", "Each selected scope must contain exactly one document.")
	}
	scopes := map[string]bool{}
	for _, scope := range envelope.Scopes {
		scopes[scope] = true
	}
	seen := map[string]bool{}
	for _, document := range envelope.Documents {
		if seen[document.Module] {
			return failure("duplicate_module", "Backup documents must use unique module names.")
		}
		if !scopes[document.Module] {
			return failure("invalid_scope", "Document is outside the selected supported scopes.")
		}
		seen[document.Module] = true
		max := MaxRuntimeDocumentBytes
		if native(document.Module) {
			max = control.MaxDocumentBytes
		}
		if !utf8.ValidString(document.Content) {
			return failure("invalid_backup", "Configuration documents must contain valid UTF-8 text.")
		}
		if len(document.Content) > max {
			return failure("document_too_large", "A selected configuration exceeds its document limit.")
		}
		if document.Digest != digest(document.Content) {
			return failure("digest_mismatch", "Document digest does not match its exact content.")
		}
		if runtimeService(document.Module) != "" && document.Generation == 0 {
			return failure("invalid_manifest", "Runtime settings must identify their accepted generation.")
		}
	}
	return nil
}

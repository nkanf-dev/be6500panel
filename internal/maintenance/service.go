package maintenance

import (
	"be6500panel/internal/control"
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"errors"
	"sort"
	"sync"
	"time"
)

type storedPreview struct {
	envelope   Envelope
	generation uint64
	expires    time.Time
	bytes      int
}
type Service struct {
	mu       sync.Mutex
	options  Options
	previews map[string]storedPreview
	// clearEpoch prevents work started before an auth reset from publishing
	// private content after Clear, even when the request context remains live.
	clearEpoch uint64
}

func New(options Options) *Service {
	if options.Now == nil {
		options.Now = time.Now
	}
	return &Service{options: options, previews: map[string]storedPreview{}}
}

func (s *Service) snapshot(ctx context.Context) (control.DocumentSet, Metadata, error) {
	if err := ctx.Err(); err != nil {
		return control.DocumentSet{}, Metadata{}, err
	}
	if s.options.Native == nil || s.options.Metadata == nil {
		return control.DocumentSet{}, Metadata{}, failure("maintenance_unavailable", "Configuration backup is not available on this host.")
	}
	documents, err := s.options.Native.Documents(ctx)
	if err != nil {
		return documents, Metadata{}, err
	}
	if documents.PendingCommit != nil {
		return documents, Metadata{}, failure("confirmation_pending", "Confirm or restore the provisional configuration before backup or import.")
	}
	metadata, err := s.options.Metadata(ctx)
	if err != nil || metadata.Model == "" || metadata.Build == "" {
		return documents, metadata, failure("metadata_unavailable", "Cannot identify the current router model and build.")
	}
	seen := map[string]bool{}
	for _, document := range documents.Documents {
		if !native(document.Module) || seen[document.Module] || len(document.Content) > control.MaxDocumentBytes {
			return documents, metadata, failure("snapshot_unavailable", "Native configuration snapshot is incomplete or invalid.")
		}
		seen[document.Module] = true
	}
	if len(seen) != len(NativeScopes) || documents.Generation == 0 {
		return documents, metadata, failure("snapshot_unavailable", "Native configuration snapshot is incomplete or invalid.")
	}
	return documents, metadata, nil
}

// Backup reads only supported accepted documents. Returned bytes are downloaded
// manually; this service has no archive directory or automatic flash history.
func (s *Service) Backup(ctx context.Context, scopes []string) ([]byte, error) {
	if err := validScopes(scopes); err != nil {
		return nil, err
	}
	current, metadata, err := s.snapshot(ctx)
	if err != nil {
		return nil, err
	}
	envelope := Envelope{Model: metadata.Model, Build: metadata.Build, CreatedAt: s.options.Now().UTC(), Generation: current.Generation, Scopes: append([]string{}, scopes...), Documents: []Document{}}
	documents := map[string]string{}
	for _, document := range current.Documents {
		documents[document.Module] = document.Content
	}
	for _, scope := range scopes {
		document := Document{Module: scope}
		if native(scope) {
			document.Content = documents[scope]
		} else {
			if s.options.Runtime == nil {
				return nil, failure("runtime_unavailable", "The explicitly selected runtime settings are unavailable.")
			}
			runtime, err := s.options.Runtime(ctx, runtimeService(scope))
			if err != nil {
				return nil, failure("runtime_unavailable", "Cannot read the explicitly selected accepted runtime settings.")
			}
			document.Content, document.Generation = runtime.Content, runtime.Generation
		}
		document.Digest = digest(document.Content)
		envelope.Documents = append(envelope.Documents, document)
	}
	if err := validateEnvelope(envelope); err != nil {
		return nil, err
	}
	// Refuse a mixed native snapshot if an external editor committed during reads.
	after, err := s.options.Native.Documents(ctx)
	if err != nil {
		return nil, err
	}
	if after.Generation != current.Generation || after.PendingCommit != nil {
		return nil, failure("generation_conflict", "Configuration changed during backup; download a fresh snapshot.")
	}
	for _, document := range envelope.Documents {
		if !native(document.Module) {
			accepted, err := s.options.Runtime(ctx, runtimeService(document.Module))
			if err != nil || accepted.Generation != document.Generation || digest(accepted.Content) != document.Digest {
				return nil, failure("generation_conflict", "Runtime settings changed during backup; download a fresh snapshot.")
			}
		}
	}
	raw, err := json.Marshal(envelope)
	if err != nil {
		return nil, failure("backup_failed", "Cannot encode the scoped configuration backup.")
	}
	if len(raw) > MaxBackupBytes {
		return nil, failure("backup_too_large", "Selected settings exceed the 2 MiB JSON backup limit; export smaller groups.")
	}
	return raw, nil
}

func (s *Service) pruneLocked() {
	now := s.options.Now()
	for id, preview := range s.previews {
		if !now.Before(preview.expires) {
			delete(s.previews, id)
		}
	}
}
func privateID() (string, error) {
	raw := make([]byte, 16)
	if _, err := rand.Read(raw); err != nil {
		return "", failure("preview_unavailable", "Cannot allocate a private import preview.")
	}
	return hex.EncodeToString(raw), nil
}

// Clear removes all in-memory private imports. Root may use it on shutdown or
// authentication reset; expiry and admission remain bounded without a worker.
func (s *Service) Clear() {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.clearEpoch++
	s.previews = map[string]storedPreview{}
}
func (s *Service) Preview(ctx context.Context, raw []byte) (Preview, error) {
	s.mu.Lock()
	epoch := s.clearEpoch
	s.mu.Unlock()
	envelope, err := Decode(raw)
	if err != nil {
		return Preview{}, err
	}
	current, metadata, err := s.snapshot(ctx)
	if err != nil {
		return Preview{}, err
	}
	out, err := s.assess(ctx, envelope, current, metadata)
	if err != nil {
		return Preview{}, err
	}
	if err := ctx.Err(); err != nil {
		return Preview{}, err
	}
	out.ID, err = privateID()
	if err != nil {
		return Preview{}, err
	}
	out.ExpiresAt = s.options.Now().UTC().Add(PreviewLifetime)
	contentBytes := 0
	for _, document := range envelope.Documents {
		contentBytes += len(document.Content)
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	s.pruneLocked()
	if err := ctx.Err(); err != nil {
		return Preview{}, err
	}
	if epoch != s.clearEpoch {
		return Preview{}, failure("preview_cleared", "Private import previews were cleared; upload the backup again in the current session.")
	}
	total := contentBytes
	for _, preview := range s.previews {
		total += preview.bytes
	}
	if len(s.previews) >= MaxPreviews || total > MaxPreviewContentBytes {
		return Preview{}, failure("preview_limit", "Private import preview limit reached; discard old previews or wait for expiry.")
	}
	s.previews[out.ID] = storedPreview{envelope: envelope, generation: current.Generation, expires: out.ExpiresAt, bytes: contentBytes}
	return out, nil
}

// Discard frees only the identified memory-only preview. It removes no drafts.
func (s *Service) Discard(id string) { s.mu.Lock(); defer s.mu.Unlock(); delete(s.previews, id) }
func (s *Service) assess(ctx context.Context, envelope Envelope, current control.DocumentSet, metadata Metadata) (Preview, error) {
	out := Preview{Generation: current.Generation, SourceModel: envelope.Model, CurrentModel: metadata.Model, ModelMismatch: envelope.Model != metadata.Model, Changes: []Change{}, Warnings: []Issue{}}
	if out.ModelMismatch {
		out.Warnings = append(out.Warnings, Issue{Code: "model_mismatch", Message: "Backup model differs from this router. Review compatibility and explicitly acknowledge before staging."})
	}
	if envelope.Build != metadata.Build {
		out.Warnings = append(out.Warnings, Issue{Code: "build_mismatch", Message: "Backup build differs from this router. Unknown native vendor data is preserved; review before Apply."})
	}
	out.Warnings = append(out.Warnings, Issue{Code: "private_configuration", Message: "Backup and native diffs may contain Wi-Fi keys, runtime tokens and private configuration. Keep them private."}, Issue{Code: "native_validation_deferred", Message: "Preview checks native syntax and selected references without writing. Stage performs authoritative native UCI and platform checks before the existing Apply workflow."})
	before := map[string]string{}
	for _, document := range current.Documents {
		before[document.Module] = document.Content
	}
	candidates := []control.Document{}
	for _, document := range envelope.Documents {
		if native(document.Module) {
			candidates = append(candidates, control.Document{Module: document.Module, Content: document.Content})
		}
	}
	validations := map[string]control.Draft{}
	for _, validation := range control.PreviewDocuments(current.Documents, candidates) {
		validations[validation.Module] = validation
	}
	for _, document := range envelope.Documents {
		if err := ctx.Err(); err != nil {
			return Preview{}, err
		}
		text := before[document.Module]
		comparable := true
		change := Change{Module: document.Module, AfterBytes: len(document.Content), AfterDigest: document.Digest, Valid: true, Errors: []Issue{}, Dependencies: []Issue{}, Risks: []Issue{}}
		if native(document.Module) {
			validation := validations[document.Module]
			change.Valid, change.Diff, change.Errors, change.Dependencies, change.Risks = validation.Valid, validation.Diff, validation.Errors, validation.Dependencies, validation.Risks
		} else {
			change.Valid = false
			change.Errors = append(change.Errors, Issue{Code: "runtime_validation_deferred", Message: "Runtime native validation is not performed in this preview. Restore through the matching service editor and its explicit generation-checked verification."})
			if s.options.Runtime != nil {
				runtime, err := s.options.Runtime(ctx, runtimeService(document.Module))
				if ctx.Err() != nil {
					return Preview{}, ctx.Err()
				}
				if errors.Is(err, context.Canceled) || errors.Is(err, context.DeadlineExceeded) {
					return Preview{}, err
				}
				if err == nil {
					text = runtime.Content
				} else {
					comparable = false
					out.Warnings = append(out.Warnings, Issue{Code: "runtime_current_unavailable", Message: "Accepted runtime settings are unavailable; compare this scope in its service editor before restoring."})
				}
			} else {
				comparable = false
				out.Warnings = append(out.Warnings, Issue{Code: "runtime_current_unavailable", Message: "Runtime settings are unavailable on this host; this scope is not staged by native import."})
			}
			out.Warnings = append(out.Warnings, Issue{Code: "runtime_restore_separate", Message: "Runtime settings are previewed only in this import step. Restore them through the corresponding service configuration with generation confirmation; no service starts automatically."})
			// Runtime content is private and preview-only; do not duplicate tokens in
			// textual diffs. Exact SHA256 and byte comparison still reflects all bytes.
		}
		change.BeforeBytes, change.BeforeDigest = len(text), digest(text)
		switch {
		case !comparable:
			change.Kind = "uncompared"
			change.BeforeDigest = ""
			out.Summary.Uncompared++
		case text == document.Content:
			change.Kind = "unchanged"
			out.Summary.Unchanged++
		case text == "":
			change.Kind = "added"
			out.Summary.Added++
		case document.Content == "":
			change.Kind = "deleted"
			out.Summary.Deleted++
		default:
			change.Kind = "modified"
			out.Summary.Modified++
		}
		change.Stageable = native(document.Module) && change.Kind != "unchanged" && change.Valid
		out.Changes = append(out.Changes, change)
	}
	return out, nil
}
func safeCode(err error) string {
	var maintenance *Error
	if errors.As(err, &maintenance) {
		return maintenance.Code
	}
	var native *control.Error
	if errors.As(err, &native) {
		return native.Code
	}
	if errors.Is(err, context.Canceled) {
		return "operation_cancelled"
	}
	if errors.Is(err, context.DeadlineExceeded) {
		return "operation_timeout"
	}
	return "import_stage_failed"
}
func (s *Service) cleanup(drafts []control.Draft, cause error) error {
	// Request cancellation must not abandon private drafts created by this call.
	// DeleteDraft can wait behind the control owner's transaction mutex before
	// it checks ctx. The select bounds this caller even during a long Apply.
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	retained := []string{}
	for i, draft := range drafts {
		completed := make(chan error, 1)
		go func(id string) { completed <- s.options.Native.DeleteDraft(ctx, id) }(draft.ID)
		select {
		case err := <-completed:
			var nativeError *control.Error
			alreadyAbsent := errors.As(err, &nativeError) && nativeError.Code == "draft_not_found"
			if err != nil && !alreadyAbsent {
				retained = append(retained, draft.ID)
			}
		case <-ctx.Done():
			for _, remaining := range drafts[i:] {
				retained = append(retained, remaining.ID)
			}
			return &Error{Code: "import_cleanup_failed", Message: "Import staging failed and cleanup timed out. Cleanup may still be in progress; the listed private draft IDs are not confirmed removed. Refresh the configuration draft queue before retrying.", CauseCode: safeCode(cause), CleanupPending: true, RetainedDraftIDs: retained}
		}
	}
	if len(retained) > 0 {
		return &Error{Code: "import_cleanup_failed", Message: "Import staging failed and some private drafts could not be removed. Review the configuration draft queue before retrying.", CauseCode: safeCode(cause), RetainedDraftIDs: retained}
	}
	return cause
}
func (s *Service) Stage(ctx context.Context, request StageRequest) (StageResult, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.pruneLocked()
	saved, ok := s.previews[request.PreviewID]
	if !ok {
		return StageResult{}, failure("preview_not_found", "Import preview expired or was discarded; preview the backup again.")
	}
	if request.Generation != saved.generation {
		return StageResult{}, failure("generation_conflict", "Stage must match the generation shown in the import preview.")
	}
	if len(request.Modules) == 0 || len(request.Modules) > len(NativeScopes) {
		return StageResult{}, failure("invalid_selection", "Select changed native configuration groups before staging.")
	}
	current, metadata, err := s.snapshot(ctx)
	if err != nil {
		return StageResult{}, err
	}
	if current.Generation != saved.generation {
		return StageResult{}, failure("generation_conflict", "Configuration changed after preview; preview the backup again.")
	}
	if saved.envelope.Model != metadata.Model && !request.AcknowledgeModelMismatch {
		return StageResult{}, failure("model_mismatch", "Explicitly acknowledge the router model mismatch before staging.")
	}
	chosen := map[string]bool{}
	for _, module := range request.Modules {
		if !native(module) || chosen[module] {
			return StageResult{}, failure("invalid_selection", "Select unique native modules only; runtime settings use their dedicated configuration workflow.")
		}
		chosen[module] = true
	}
	selected := []control.Document{}
	for _, document := range saved.envelope.Documents {
		if chosen[document.Module] {
			selected = append(selected, control.Document{Module: document.Module, Content: document.Content})
		}
	}
	if len(selected) != len(chosen) {
		return StageResult{}, failure("invalid_selection", "Selected native group is missing from this backup.")
	}
	// Validate the chosen bundle, not an unselected dependency in the backup.
	validations := control.PreviewDocuments(current.Documents, selected)
	for _, validation := range validations {
		if !validation.Valid {
			return StageResult{}, failure("invalid_candidate", "A selected native document has preview errors.")
		}
		if len(validation.Dependencies) > 0 {
			return StageResult{}, failure("invalid_reference", "Selected groups omit a required native dependency; include its configuration group and preview again.")
		}
		if validation.Diff == "" {
			return StageResult{}, failure("no_changes", "Only changed native groups may be staged.")
		}
	}
	// Match existing native commit order, irrespective of upload or selection order.
	sort.Slice(selected, func(i, j int) bool { return scopeIndex(selected[i].Module) < scopeIndex(selected[j].Module) })
	drafts := []control.Draft{}
	for _, document := range selected {
		draft, err := s.options.Native.Stage(ctx, control.StageRequest{Module: document.Module, Content: document.Content, Generation: current.Generation})
		if draft.ID != "" {
			drafts = append(drafts, draft)
		}
		if err != nil {
			return StageResult{}, s.cleanup(drafts, err)
		}
		if !draft.Valid {
			cause := failure("invalid_candidate", "Native UCI or platform validation rejected an imported document. No imported draft was accepted.")
			if len(draft.Errors) > 0 {
				cause = failure(draft.Errors[0].Code, draft.Errors[0].Message)
			}
			return StageResult{}, s.cleanup(drafts, cause)
		}
	}
	after, err := s.options.Native.Documents(ctx)
	if err != nil {
		return StageResult{}, s.cleanup(drafts, err)
	}
	if after.Generation != current.Generation || after.PendingCommit != nil {
		return StageResult{}, s.cleanup(drafts, failure("generation_conflict", "Configuration changed while staging; imported drafts were discarded."))
	}
	delete(s.previews, request.PreviewID)
	return StageResult{Generation: current.Generation, Drafts: drafts, Warnings: []Issue{{Code: "apply_required", Message: "Native groups are staged together only. Review their risks in the configuration draft queue and explicitly Apply the selected bundle."}}}, nil
}
func scopeIndex(scope string) int {
	for i, module := range NativeScopes {
		if module == scope {
			return i
		}
	}
	return len(NativeScopes)
}

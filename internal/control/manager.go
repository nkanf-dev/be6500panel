package control

import (
	"bytes"
	"context"
	"errors"
	"io"
	"log/slog"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"syscall"
	"time"
)

// Manager serializes live mutations. A journal is retained for the last operation.
type Manager struct {
	mu               sync.Mutex
	root, dataDir    string
	logger           *slog.Logger
	runner           Runner
	verify           func(context.Context, []string) error
	reloadHook       func(context.Context, string) error
	timeout          time.Duration
	ctx              context.Context
	cancel           context.CancelFunc
	wg               sync.WaitGroup
	wake             chan struct{}
	closed           bool
	lockFile         *os.File
	closeOnce        sync.Once
	disk             diskState
	journal          *journal
	recoveryError    string
	storageAdmission func(context.Context, string, int64, bool) (func(), error)
	storageReserved  bool // Protected by mu; transaction writes share one reservation.
	storageRecovery  bool // Includes recovery state and journal completion writes.
}

func New(o Options) (*Manager, error) {
	if o.DataDir == "" {
		return nil, failure("data_dir_required", "A private control data directory is required.")
	}
	root := o.Root
	if root == "" {
		root = "/"
	}
	var err error
	root, err = filepath.Abs(root)
	if err != nil {
		return nil, err
	}
	data, err := filepath.Abs(o.DataDir)
	if err != nil {
		return nil, err
	}
	liveDir := filepath.Join(root, "etc", "config")
	if data == liveDir || strings.HasPrefix(data, liveDir+string(filepath.Separator)) {
		return nil, failure("unsafe_data_dir", "Draft storage must be outside live UCI configuration.")
	}
	if err = os.MkdirAll(data, 0700); err != nil {
		return nil, failure("storage_failed", "Cannot create private control storage.")
	}
	info, err := os.Lstat(data)
	if err != nil || !info.IsDir() || info.Mode()&os.ModeSymlink != 0 {
		return nil, failure("unsafe_data_dir", "Control storage must be a private directory.")
	}
	if err = os.Chmod(data, 0700); err != nil {
		return nil, err
	}
	lock, err := lockStore(data)
	if err != nil {
		return nil, err
	}
	if err = cleanTemps(data); err != nil {
		unlockStore(lock)
		return nil, err
	}
	if o.Runner == nil {
		o.Runner = runCommand
	}
	if o.Logger == nil {
		o.Logger = slog.New(slog.NewTextHandler(io.Discard, nil))
	}
	if o.ConfirmationTimeout <= 0 {
		o.ConfirmationTimeout = 120 * time.Second
	}
	ctx, cancel := context.WithCancel(context.Background())
	m := &Manager{root: root, dataDir: data, runner: o.Runner, logger: o.Logger, verify: o.Verify, reloadHook: o.Reload, timeout: o.ConfirmationTimeout, ctx: ctx, cancel: cancel, wake: make(chan struct{}, 1), lockFile: lock, storageAdmission: o.StorageAdmission}
	fail := func(e error) (*Manager, error) { cancel(); unlockStore(lock); return nil, e }
	err = readJSON(filepath.Join(data, "state.json"), &m.disk)
	if os.IsNotExist(err) {
		m.disk = diskState{Generation: 1, Drafts: []storedDraft{}}
	} else if err != nil {
		return fail(failure("state_corrupt", "Cannot load private configuration state."))
	}
	if err = validateStored(m.disk); err != nil {
		return fail(failure("state_corrupt", "Private draft state is invalid."))
	}
	recovered := false
	var j journal
	err = readJSON(filepath.Join(data, "journal.json"), &j)
	if err == nil {
		if err = validateJournal(&j); err != nil {
			return fail(failure("journal_corrupt", "Rollback journal is invalid; changes are disabled."))
		}
		m.journal = &j
		if j.Phase != "committed" && j.Phase != "rolled_back" {
			// An interrupted or unconfirmed transaction is never trusted after restart.
			if _, err = m.rollbackLocked(ctx); err != nil {
				return fail(err)
			}
			recovered = true
		}
	} else if !os.IsNotExist(err) {
		return fail(failure("journal_corrupt", "Cannot load rollback journal; changes are disabled."))
	}
	if _, err = m.syncGeneration(); err != nil {
		return fail(err)
	}
	m.storageRecovery = recovered
	if err = m.saveState(); err != nil {
		return fail(err)
	}
	m.storageRecovery = false
	m.wg.Add(1)
	go m.deadlineLoop()
	return m, nil
}

// Close cancels subprocesses and stops the deadline worker. A provisional journal
// stays on disk and is rolled back by New after restart.
func (m *Manager) Close() error {
	m.closeOnce.Do(func() {
		m.cancel()
		m.mu.Lock()
		m.closed = true
		m.mu.Unlock()
		m.wg.Wait()
		unlockStore(m.lockFile)
	})
	return nil
}
func (m *Manager) check(ctx context.Context) error {
	if m.closed || m.ctx.Err() != nil {
		return failure("closed", "Configuration manager is closed.")
	}
	if ctx.Err() != nil {
		return failure("cancelled", "Configuration operation was cancelled.")
	}
	if m.recoveryError != "" {
		return failure(m.recoveryError, "Rollback recovery must succeed before new changes.")
	}
	return nil
}
func (m *Manager) operationContext(ctx context.Context) (context.Context, context.CancelFunc) {
	child, cancel := context.WithCancel(ctx)
	stop := context.AfterFunc(m.ctx, cancel)
	return child, func() { stop(); cancel() }
}
func (m *Manager) signal() {
	select {
	case m.wake <- struct{}{}:
	default:
	}
}
func (m *Manager) pending() *PendingCommit {
	if m.journal == nil || m.journal.Phase != "pending" || m.journal.Operation.Deadline == nil {
		return nil
	}
	return &PendingCommit{ID: m.journal.Operation.ID, Deadline: *m.journal.Operation.Deadline}
}
func (m *Manager) Status() Status {
	m.mu.Lock()
	defer m.mu.Unlock()
	return Status{Enabled: !m.closed && m.recoveryError == "", Generation: m.disk.Generation, PendingCommit: m.pending(), ErrorCode: m.recoveryError}
}
func (m *Manager) syncGeneration() (map[string]snapshot, error) {
	live, fingerprint, err := m.readLive()
	if err != nil {
		return nil, err
	}
	if m.disk.Fingerprint == "" {
		m.disk.Fingerprint = fingerprint
	} else if fingerprint != m.disk.Fingerprint {
		if m.pending() != nil {
			return nil, failure("generation_conflict", "Live configuration changed during a provisional commit.")
		}
		m.disk.Generation++
		m.disk.Fingerprint = fingerprint
		if err = m.saveState(); err != nil {
			return nil, err
		}
	}
	return live, nil
}
func (m *Manager) Documents(ctx context.Context) (DocumentSet, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	if err := m.check(ctx); err != nil {
		return DocumentSet{}, err
	}
	live, err := m.syncGeneration()
	if err != nil {
		return DocumentSet{}, err
	}
	out := DocumentSet{Generation: m.disk.Generation, Documents: []Document{}, PendingCommit: m.pending()}
	for _, module := range modules {
		out.Documents = append(out.Documents, Document{Module: module, Content: live[module].Content})
	}
	return out, nil
}
func (m *Manager) Stage(ctx context.Context, r StageRequest) (Draft, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	if err := m.check(ctx); err != nil {
		return Draft{}, err
	}
	if !allowed(r.Module) {
		return Draft{}, failure("module_not_allowed", "This native configuration module is not editable.")
	}
	if len(r.Content) > MaxDocumentBytes {
		return Draft{}, failure("document_too_large", "Native configuration exceeds the document limit.")
	}
	live, err := m.syncGeneration()
	if err != nil {
		return Draft{}, err
	}
	if r.Generation != m.disk.Generation {
		return Draft{}, failure("generation_conflict", "Configuration changed; refresh before staging.")
	}
	if m.pending() != nil {
		return Draft{}, failure("confirmation_pending", "Confirm or roll back the pending commit first.")
	}
	total := len(r.Content)
	for _, d := range m.disk.Drafts {
		total += len(d.Content)
	}
	if len(m.disk.Drafts) >= MaxDrafts || total > maxDraftContentBytes {
		return Draft{}, failure("draft_limit", "Remove old drafts before staging more configuration.")
	}
	id, err := randomID()
	if err != nil {
		return Draft{}, failure("storage_failed", "Cannot create a private draft identifier.")
	}
	_, issues := validate(r.Module, r.Content)
	dependencies := []Issue{}
	if len(issues) == 0 {
		ctx, cancel := m.operationContext(ctx)
		// A new interface and its DHCP/Wi-Fi/firewall references can be staged
		// independently and committed as one bundle. Native invalidity is never
		// deferred; only namespace references wait for the selected candidate set.
		issues = m.validateNativeSet(ctx, map[string]string{r.Module: r.Content}, live, true)
		cancel()
		if len(issues) == 0 {
			combined := make(map[string]string, len(modules))
			for _, module := range modules {
				combined[module] = live[module].Content
			}
			combined[r.Module] = r.Content
			dependencies = validateReferences(map[string]string{r.Module: r.Content}, combined)
		}
	}
	for _, diagnostic := range issues {
		if diagnostic.Code == "storage_insufficient" || diagnostic.Code == "cancelled" {
			return Draft{}, failure(diagnostic.Code, diagnostic.Message)
		}
	}
	d := Draft{ID: id, Module: r.Module, Generation: r.Generation, Diff: diff(r.Module, live[r.Module].Content, r.Content), Risks: risk(r.Module, live[r.Module].Content, r.Content), Valid: len(issues) == 0, Errors: issues, Dependencies: dependencies, CreatedAt: time.Now().UTC()}
	old := m.disk.Drafts
	m.disk.Drafts = append(m.disk.Drafts, storedDraft{Draft: d, Content: r.Content})
	if err = m.saveState(); err != nil {
		m.disk.Drafts = old
		return Draft{}, err
	}
	return cloneDraft(d), nil
}
func cloneDraft(d Draft) Draft {
	d.Risks = append([]Issue{}, d.Risks...)
	d.Errors = append([]Issue{}, d.Errors...)
	d.Dependencies = append([]Issue{}, d.Dependencies...)
	return d
}
func (m *Manager) Drafts(ctx context.Context) ([]Draft, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	if err := m.check(ctx); err != nil {
		return nil, err
	}
	out := []Draft{}
	for _, d := range m.disk.Drafts {
		out = append(out, cloneDraft(d.Draft))
	}
	return out, nil
}
func (m *Manager) DeleteDraft(ctx context.Context, id string) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	if err := m.check(ctx); err != nil {
		return err
	}
	if !safeID(id) {
		return failure("draft_not_found", "Private draft does not exist.")
	}
	for i, d := range m.disk.Drafts {
		if d.ID == id {
			old := m.disk.Drafts
			m.disk.Drafts = append(append([]storedDraft{}, old[:i]...), old[i+1:]...)
			if err := m.saveState(); err != nil {
				m.disk.Drafts = old
				return err
			}
			return nil
		}
	}
	return failure("draft_not_found", "Private draft does not exist.")
}
func (m *Manager) validateNative(ctx context.Context, candidates map[string]string, live map[string]snapshot) []Issue {
	return m.validateNativeSet(ctx, candidates, live, false)
}

func (m *Manager) validateNativeSet(ctx context.Context, candidates map[string]string, live map[string]snapshot, deferReferences bool) []Issue {
	dir, err := os.MkdirTemp(m.dataDir, "candidate-")
	if err != nil {
		return []Issue{issue("validation_unavailable", "Cannot create isolated UCI validation directory.")}
	}
	var releaseStorage func()
	defer func() {
		os.RemoveAll(dir)
		if releaseStorage != nil {
			releaseStorage()
		}
	}()
	// Validate cross-document references against the complete isolated candidate set.
	combined := make(map[string]string, len(modules))
	for _, module := range modules {
		combined[module] = live[module].Content
	}
	for module, content := range candidates {
		combined[module] = content
		if issues := validateExecutionChanges(module, live[module].Content, content); len(issues) > 0 {
			return issues
		}
	}
	if !deferReferences {
		if issues := validateReferences(candidates, combined); len(issues) > 0 {
			return issues
		}
	}
	var candidateBytes int64
	for _, text := range combined {
		candidateBytes += temporaryBytes(len(text))
	}
	if releaseStorage, err = m.admitStorage(ctx, dir, candidateBytes, false); err != nil {
		var typed *Error
		if errors.As(err, &typed) {
			return []Issue{issue(typed.Code, typed.Message)}
		}
		return []Issue{issue("validation_unavailable", "Cannot reserve isolated native validation storage.")}
	}
	for _, module := range modules {
		text := combined[module]
		if err = atomicWrite(filepath.Join(dir, module), []byte(text), 0600); err != nil {
			return []Issue{issue("validation_unavailable", "Cannot store isolated native configuration.")}
		}
	}
	out := []Issue{}
	for _, module := range modules {
		if _, ok := candidates[module]; !ok {
			continue
		}
		// -P prevents the default live delta namespace from being used as savedir.
		call, cancel := context.WithTimeout(ctx, 10*time.Second)
		_, err = m.runner(call, "/sbin/uci", "-s", "-c", dir, "-P", dir, "show", module)
		cancel()
		if err != nil {
			code := "uci_validation_failed"
			message := "Native UCI rejected the isolated candidate."
			if ctx.Err() != nil {
				code = "cancelled"
				message = "Configuration validation was cancelled."
			} else if errors.Is(err, exec.ErrNotFound) || errors.Is(err, os.ErrNotExist) {
				code = "validation_unavailable"
				message = "Native UCI validation is unavailable."
			}
			out = append(out, issue(code, message))
		}
	}
	return out
}

var reloadCommands = map[string][]string{
	"network":  {"/etc/init.d/network", "reload"},
	"wireless": {"/sbin/wifi", "reload"},
	"dhcp":     {"/etc/init.d/dnsmasq", "reload"},
	"firewall": {"/etc/init.d/firewall", "reload"},
	"system":   {"/etc/init.d/system", "reload"},
	"dropbear": {"/etc/init.d/dropbear", "reload"},
}

func (m *Manager) preflightReload(ctx context.Context, changed []string) error {
	if m.reloadHook != nil {
		return nil
	}
	for _, module := range changed {
		if _, ok := reloadCommands[module]; !ok {
			return failure("reload_unsupported", "This module has no supported reload operation.")
		}
		if module == "system" {
			call, cancel := context.WithTimeout(ctx, 5*time.Second)
			out, err := m.runner(call, "/sbin/uci", "-q", "get", "xiaoqiang.common.INITTED")
			cancel()
			if err != nil || strings.TrimSpace(string(out)) != "YES" {
				return failure("system_reload_unsafe", "Factory initialization must be complete before system reload.")
			}
		}
	}
	return nil
}
func (m *Manager) reload(ctx context.Context, changed []string) error {
	var first error
	for _, module := range changed {
		call, cancel := context.WithTimeout(ctx, 20*time.Second)
		var err error
		if m.reloadHook != nil {
			err = m.reloadHook(call, module)
		} else {
			args := reloadCommands[module]
			if len(args) == 0 {
				err = failure("reload_unsupported", "Module reload is unavailable.")
			} else {
				_, err = m.runner(call, args[0], args[1:]...)
			}
		}
		cancel()
		if err != nil {
			m.logFailure("reload", module, err)
		}
		if err != nil && first == nil {
			first = failure("reload_failed", "A fixed module reload failed.")
		}
	}
	return first
}
func (m *Manager) verifyApplied(ctx context.Context, changed []string) error {
	if m.verify == nil {
		return nil
	}
	call, cancel := context.WithTimeout(ctx, 15*time.Second)
	defer cancel()
	if err := m.verify(call, append([]string{}, changed...)); err != nil {
		return failure("verification_failed", "Applied configuration verification failed.")
	}
	return nil
}
func (m *Manager) verifyDocuments(candidates map[string]string) error {
	for module, text := range candidates {
		live, err := readDocument(m.livePath(module))
		if err != nil || !live.Exists || live.Content != text {
			return failure("verification_failed", "Reload changed an installed configuration document.")
		}
	}
	return nil
}

// Reload scripts must not silently change or recreate restored documents.
// Compare existence, text and permissions against the retained prior snapshot.
func (m *Manager) verifyRestored(prior map[string]snapshot) error {
	for module, expected := range prior {
		live, err := readDocument(m.livePath(module))
		if err != nil || live.Exists != expected.Exists || live.Exists && (live.Content != expected.Content || live.Mode != expected.Mode&0777) {
			return failure("verification_failed", "Reload changed a restored configuration document.")
		}
	}
	return nil
}

func cloneOperation(o Operation) Operation {
	o.ChangedModules = append([]string{}, o.ChangedModules...)
	if o.Deadline != nil {
		d := *o.Deadline
		o.Deadline = &d
	}
	return o
}
func (m *Manager) Commit(ctx context.Context, r CommitRequest) (Operation, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	if err := m.check(ctx); err != nil {
		return Operation{}, err
	}
	live, err := m.syncGeneration()
	if err != nil {
		return Operation{}, err
	}
	if r.Generation != m.disk.Generation {
		return Operation{}, failure("generation_conflict", "Configuration changed; refresh and stage new drafts.")
	}
	if m.pending() != nil {
		return Operation{}, failure("confirmation_pending", "Confirm or roll back the pending commit first.")
	}
	if len(r.DraftIDs) == 0 || len(r.DraftIDs) > len(modules) {
		return Operation{}, failure("invalid_commit", "Select one draft per changed module.")
	}
	candidates := map[string]string{}
	ids := map[string]bool{}
	changed := []string{}
	before := map[string]snapshot{}
	riskRequired := false
	for _, id := range r.DraftIDs {
		if !safeID(id) || ids[id] {
			return Operation{}, failure("invalid_commit", "Commit draft identifiers must be unique.")
		}
		ids[id] = true
		found := false
		for _, d := range m.disk.Drafts {
			if d.ID == id {
				found = true
				if d.Generation != r.Generation {
					return Operation{}, failure("generation_conflict", "Selected draft was staged against old configuration.")
				}
				if !d.Valid {
					return Operation{}, failure("invalid_candidate", "An invalid draft cannot be committed.")
				}
				if _, exists := candidates[d.Module]; exists {
					return Operation{}, failure("duplicate_module", "Select only one draft for each module.")
				}
				candidates[d.Module] = d.Content
				if d.Content != live[d.Module].Content {
					changed = append(changed, d.Module)
					before[d.Module] = live[d.Module]
					if len(risk(d.Module, live[d.Module].Content, d.Content)) > 0 {
						riskRequired = true
					}
				}
				break
			}
		}
		if !found {
			return Operation{}, failure("draft_not_found", "A selected private draft does not exist.")
		}
	}
	if len(changed) == 0 {
		return Operation{}, failure("no_changes", "The selected drafts have no live configuration changes.")
	}
	// Stable dependency order: network, wireless, DHCP, firewall, system, SSH.
	sort.Slice(changed, func(i, j int) bool { return moduleIndex(changed[i]) < moduleIndex(changed[j]) })
	if riskRequired && !r.AcknowledgeRisks {
		return Operation{}, failure("risk_acknowledgement_required", "Acknowledge the listed connectivity risks before committing.")
	}
	ctx, cancel := m.operationContext(ctx)
	defer cancel()
	for module, text := range candidates {
		if _, issues := validate(module, text); len(issues) > 0 {
			return Operation{}, failure("invalid_candidate", "Selected native configuration is invalid.")
		}
	}
	if issues := m.validateNative(ctx, candidates, live); len(issues) > 0 {
		return Operation{}, failure(issues[0].Code, issues[0].Message)
	}
	if err = m.preflightReload(ctx, changed); err != nil {
		return Operation{}, err
	}
	// CAS again after validation/hooks: outside tools may have edited live files.
	_, fingerprint, err := m.readLive()
	if err != nil {
		return Operation{}, err
	}
	if fingerprint != m.disk.Fingerprint {
		return Operation{}, failure("generation_conflict", "Live configuration changed during commit validation.")
	}
	id, err := randomID()
	if err != nil {
		return Operation{}, failure("storage_failed", "Cannot allocate configuration operation.")
	}
	op := Operation{ID: id, State: "committed", Generation: m.disk.Generation + 1, ChangedModules: changed}
	if riskRequired {
		deadline := time.Now().UTC().Add(m.timeout)
		op.State = "pending_confirmation"
		op.Deadline = &deadline
	}
	nextJournal := journal{Operation: op, Phase: "applying", Before: before, BaseGeneration: m.disk.Generation}
	releaseStorage, err := m.reserveCommitStorage(ctx, nextJournal, candidates)
	if err != nil {
		return Operation{}, err
	}
	defer m.finishStorageReservation(releaseStorage)
	previousJournal := m.journal
	m.journal = &nextJournal
	if err = m.saveJournal(); err != nil {
		m.journal = previousJournal
		return Operation{}, err
	}
	failApply := func(cause error) (Operation, error) {
		// Request cancellation must not cancel recovery. Close cancels recovery itself.
		recovery, cancel := context.WithTimeout(m.ctx, 90*time.Second)
		defer cancel()
		rolled, rollbackErr := m.rollbackLocked(recovery)
		if rollbackErr != nil {
			return rolled, rollbackErr
		}
		return rolled, cause
	}
	for _, module := range changed {
		if ctx.Err() != nil {
			return failApply(failure("cancelled", "Configuration commit was cancelled."))
		}
		if err = m.writeDocument(ctx, m.livePath(module), []byte(candidates[module]), 0600); err != nil {
			m.logFailure("write_live", module, err)
			return failApply(failure("apply_failed", "Cannot replace a native configuration document."))
		}
	}
	if err = m.reload(ctx, changed); err != nil {
		return failApply(err)
	}
	if err = m.verifyApplied(ctx, changed); err != nil {
		return failApply(err)
	}
	// Even without an injected platform hook, verify that all installed documents
	// still match the candidate after reload. Scripts must not silently alter them.
	if err = m.verifyDocuments(candidates); err != nil {
		return failApply(err)
	}
	_, fingerprint, err = m.readLive()
	if err != nil {
		return failApply(err)
	}
	m.disk.Generation = op.Generation
	m.disk.Fingerprint = fingerprint
	if err = m.saveState(); err != nil {
		return failApply(err)
	}
	if riskRequired {
		m.journal.Phase = "pending"
	} else {
		m.journal.Phase = "committed"
	}
	if err = m.saveJournal(); err != nil {
		return failApply(err)
	}
	// Consumed drafts are removed only after the accepted operation is durable.
	kept := []storedDraft{}
	for _, d := range m.disk.Drafts {
		if !ids[d.ID] {
			kept = append(kept, d)
		}
	}
	old := m.disk.Drafts
	m.disk.Drafts = kept
	if err = m.saveState(); err != nil {
		m.disk.Drafts = old
		return failApply(err)
	}
	m.logger.Info("Configuration commit completed", "code", op.State, "operation", op.ID, "generation", op.Generation)
	m.signal()
	return cloneOperation(op), nil
}
func moduleIndex(module string) int {
	for i, s := range modules {
		if module == s {
			return i
		}
	}
	return len(modules)
}
func (m *Manager) Confirm(ctx context.Context, id string) (Operation, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	if err := m.check(ctx); err != nil {
		return Operation{}, err
	}
	if m.journal == nil || m.journal.Operation.ID != id {
		return Operation{}, failure("operation_not_found", "Configuration operation does not exist.")
	}
	if m.journal.Phase == "committed" {
		return cloneOperation(m.journal.Operation), nil
	}
	if m.journal.Phase != "pending" {
		return cloneOperation(m.journal.Operation), failure("not_pending", "This operation is not awaiting reachability confirmation.")
	}
	if !time.Now().Before(*m.journal.Operation.Deadline) {
		recovery, cancel := context.WithTimeout(m.ctx, 90*time.Second)
		defer cancel()
		return m.rollbackLocked(recovery)
	}
	// The HTTP owner calls Confirm only for an authenticated request received at
	// the new/current admin address. No unauthenticated offline confirmation API.
	if _, err := m.syncGeneration(); err != nil {
		return Operation{}, err
	}
	ctx, cancel := m.operationContext(ctx)
	defer cancel()
	if err := m.verifyApplied(ctx, m.journal.Operation.ChangedModules); err != nil {
		return Operation{}, err
	}
	old := *m.journal
	m.journal.Phase = "committed"
	m.journal.Operation.State = "committed"
	m.journal.Operation.Deadline = nil
	if err := m.saveJournal(); err != nil {
		*m.journal = old
		return Operation{}, err
	}
	m.signal()
	return cloneOperation(m.journal.Operation), nil
}
func (m *Manager) Rollback(ctx context.Context, id string) (Operation, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.closed || m.ctx.Err() != nil {
		return Operation{}, failure("closed", "Configuration manager is closed.")
	}
	if ctx.Err() != nil {
		return Operation{}, failure("cancelled", "Configuration operation was cancelled.")
	}
	if m.journal == nil || m.journal.Operation.ID != id {
		return Operation{}, failure("operation_not_found", "Configuration operation does not exist.")
	}
	if m.journal.Phase == "rolled_back" {
		return cloneOperation(m.journal.Operation), nil
	}
	if m.journal.Phase == "committed" {
		if _, err := m.syncGeneration(); err != nil {
			return Operation{}, err
		}
		if m.disk.Generation != m.journal.Operation.Generation {
			return Operation{}, failure("generation_conflict", "Newer configuration cannot be overwritten by an old rollback.")
		}
	}
	// Once recovery starts, complete it even if the browser disconnects.
	recovery, cancel := context.WithTimeout(m.ctx, 90*time.Second)
	defer cancel()
	return m.rollbackLocked(recovery)
}
func (m *Manager) rollbackLocked(ctx context.Context) (Operation, error) {
	// Wake the supervisor on both success and failure. A failed recovery must
	// remain supervised even if it started manually or during a failed apply.
	defer m.signal()
	j := m.journal
	if j == nil {
		return Operation{}, failure("operation_not_found", "Rollback journal does not exist.")
	}
	if j.Phase == "rolled_back" {
		return cloneOperation(j.Operation), nil
	}
	wasReserved := m.storageReserved
	releaseStorage, err := m.reserveRollbackStorage(ctx, *j)
	if err != nil {
		j.Phase = "rolling_back"
		m.recoveryError = "rollback_failed"
		return cloneOperation(j.Operation), failure("rollback_failed", "Persistent storage must be available before configuration recovery can be retried.")
	}
	if !wasReserved {
		defer m.finishStorageReservation(releaseStorage)
	}
	wasRecovery := m.storageRecovery
	m.storageRecovery = true
	defer func() { m.storageRecovery = wasRecovery }()
	j.Phase = "rolling_back"
	if err := m.saveJournal(); err != nil {
		m.recoveryError = "rollback_failed"
		return cloneOperation(j.Operation), err
	}
	var first error
	for _, module := range j.Operation.ChangedModules {
		s := j.Before[module]
		var err error
		if s.Exists {
			err = m.writeDocument(ctx, m.livePath(module), []byte(s.Content), os.FileMode(s.Mode)&0777)
		} else {
			err = os.Remove(m.livePath(module))
			if os.IsNotExist(err) {
				err = nil
			}
			if err == nil {
				err = syncDir(filepath.Dir(m.livePath(module)))
			}
		}
		if err != nil {
			m.logFailure("restore_snapshot", module, err)
		}
		if err != nil && first == nil {
			first = err
		}
	}
	// Reload every affected module even if one restoration failed, so modules
	// whose prior documents were restored do not retain the failed candidate.
	if reloadErr := m.reload(ctx, j.Operation.ChangedModules); first == nil {
		first = reloadErr
	}
	if verifyErr := m.verifyRestored(j.Before); first == nil {
		first = verifyErr
	}
	if first != nil {
		m.recoveryError = "rollback_failed"
		m.logger.Error("Configuration rollback needs recovery", "code", "rollback_failed", "operation", j.Operation.ID)
		return cloneOperation(j.Operation), failure("rollback_failed", "Prior documents are restored where possible; module recovery must be retried.")
	}
	_, fingerprint, err := m.readLive()
	if err != nil {
		m.recoveryError = "rollback_failed"
		return cloneOperation(j.Operation), err
	}
	generation := j.BaseGeneration + 2
	if m.disk.Generation >= generation {
		generation = m.disk.Generation + 1
	}
	m.disk.Generation = generation
	m.disk.Fingerprint = fingerprint
	if err = m.saveState(); err != nil {
		m.recoveryError = "rollback_failed"
		return cloneOperation(j.Operation), err
	}
	j.Phase = "rolled_back"
	j.Operation.State = "rolled_back"
	j.Operation.Generation = generation
	j.Operation.Deadline = nil
	if err = m.saveJournal(); err != nil {
		j.Phase = "rolling_back"
		m.recoveryError = "rollback_failed"
		return cloneOperation(j.Operation), err
	}
	m.recoveryError = ""
	m.logger.Info("Prior configuration restored", "code", "rolled_back", "operation", j.Operation.ID, "generation", generation)
	return cloneOperation(j.Operation), nil
}

const (
	rollbackRetryInitial = time.Second
	rollbackRetryMaximum = 30 * time.Second
)

func nextRollbackRetryDelay(previous time.Duration) time.Duration {
	if previous <= 0 {
		return rollbackRetryInitial
	}
	if previous >= rollbackRetryMaximum/2 {
		return rollbackRetryMaximum
	}
	return previous * 2
}

func (m *Manager) deadlineLoop() {
	defer m.wg.Done()
	var retryID string
	var retryAt time.Time
	var retryDelay time.Duration
	for {
		m.mu.Lock()
		pending := m.pending()
		recovering := m.recoveryError != "" && m.journal != nil && m.journal.Phase == "rolling_back"
		var operationID string
		if recovering {
			operationID = m.journal.Operation.ID
		}
		m.mu.Unlock()

		var due time.Time
		if recovering {
			if retryID != operationID {
				retryID = operationID
				retryDelay = nextRollbackRetryDelay(0)
				retryAt = time.Now().Add(retryDelay)
			}
			due = retryAt
		} else {
			retryID, retryAt, retryDelay = "", time.Time{}, 0
			if pending != nil {
				due = pending.Deadline
			}
		}
		var timer *time.Timer
		var tick <-chan time.Time
		if !due.IsZero() {
			delay := time.Until(due)
			if delay < 0 {
				delay = 0
			}
			timer = time.NewTimer(delay)
			tick = timer.C
		}
		select {
		case <-m.ctx.Done():
			if timer != nil {
				timer.Stop()
			}
			return
		case <-m.wake:
			if timer != nil {
				timer.Stop()
			}
			continue
		case <-tick:
			m.mu.Lock()
			if m.ctx.Err() != nil {
				m.mu.Unlock()
				return
			}
			p := m.pending()
			expired := p != nil && !time.Now().Before(p.Deadline)
			retryDue := m.recoveryError != "" && m.journal != nil && m.journal.Phase == "rolling_back" && m.journal.Operation.ID == retryID && !time.Now().Before(retryAt)
			if expired || retryDue {
				ctx, cancel := context.WithTimeout(m.ctx, 90*time.Second)
				_, err := m.rollbackLocked(ctx)
				cancel()
				if err != nil {
					if retryID != m.journal.Operation.ID {
						retryDelay = 0
					}
					retryID = m.journal.Operation.ID
					retryDelay = nextRollbackRetryDelay(retryDelay)
					retryAt = time.Now().Add(retryDelay)
				}
			}
			m.mu.Unlock()
		}
	}
}

type boundedOutput struct{ bytes.Buffer }

func (b *boundedOutput) Write(p []byte) (int, error) {
	n := len(p)
	remaining := 256<<10 - b.Len()
	if remaining > 0 {
		if len(p) > remaining {
			p = p[:remaining]
		}
		_, _ = b.Buffer.Write(p)
	}
	return n, nil
}
func runCommand(ctx context.Context, path string, args ...string) ([]byte, error) {
	cmd := exec.CommandContext(ctx, path, args...)
	cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}
	cmd.Cancel = func() error {
		if cmd.Process == nil {
			return os.ErrProcessDone
		}
		return syscall.Kill(-cmd.Process.Pid, syscall.SIGKILL)
	}
	cmd.WaitDelay = 2 * time.Second
	// Reload scripts can start background processes that inherit stdout/stderr.
	// Real file descriptors avoid os/exec copy pipes and false ErrWaitDelay after
	// the script has already exited successfully. Only the fixed initialization
	// query needs bounded captured output; configuration output is never retained.
	discard, err := os.OpenFile(os.DevNull, os.O_WRONLY, 0)
	if err != nil {
		return nil, err
	}
	defer discard.Close()
	cmd.Stdout, cmd.Stderr = discard, discard
	capture := path == "/sbin/uci" && len(args) == 3 && args[0] == "-q" && args[1] == "get" && args[2] == "xiaoqiang.common.INITTED"
	if !capture {
		return nil, cmd.Run()
	}
	out := &boundedOutput{}
	cmd.Stdout = out
	err = cmd.Run()
	return out.Bytes(), err
}

// diagnosticCause reports only fixed error classes, never a command's output,
// path, arguments or private native configuration.
func diagnosticCause(err error) string {
	switch {
	case err == nil:
		return "none"
	case errors.Is(err, exec.ErrWaitDelay):
		return "exec_wait_delay"
	case errors.Is(err, context.DeadlineExceeded):
		return "deadline"
	case errors.Is(err, context.Canceled):
		return "cancelled"
	case errors.Is(err, syscall.EINVAL):
		return "errno_einval"
	case errors.Is(err, syscall.EXDEV):
		return "errno_exdev"
	case errors.Is(err, syscall.EBUSY):
		return "errno_ebusy"
	case errors.Is(err, syscall.EROFS):
		return "errno_erofs"
	case errors.Is(err, syscall.ENOSPC):
		return "errno_enospc"
	case errors.Is(err, syscall.EACCES), errors.Is(err, syscall.EPERM):
		return "errno_permission"
	case errors.Is(err, syscall.EIO):
		return "errno_eio"
	case errors.Is(err, os.ErrNotExist):
		return "errno_enoent"
	}
	var exit *exec.ExitError
	if errors.As(err, &exit) {
		return "exec_exit"
	}
	var typed *Error
	if errors.As(err, &typed) {
		return typed.Code
	}
	return "operation_error"
}
func (m *Manager) logFailure(step, module string, err error) {
	storageStep := "none"
	var storage *storageError
	if errors.As(err, &storage) {
		storageStep = storage.step
	}
	m.logger.Error("Configuration step failed", "code", "configuration_step_failed", "step", step, "storageStep", storageStep, "module", module, "cause", diagnosticCause(err))
}

package control

import (
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"syscall"
)

const maxStoreBytes = 3 << 20
const maxDraftContentBytes = 512 << 10

type storedDraft struct {
	Draft
	Content string `json:"content"`
}
type diskState struct {
	Generation  uint64        `json:"generation"`
	Fingerprint string        `json:"fingerprint"`
	Drafts      []storedDraft `json:"drafts"`
}
type snapshot struct {
	Exists  bool   `json:"exists"`
	Content string `json:"content"`
	Mode    uint32 `json:"mode"`
}
type journal struct {
	Operation      Operation           `json:"operation"`
	Phase          string              `json:"phase"`
	Before         map[string]snapshot `json:"before"`
	BaseGeneration uint64              `json:"baseGeneration"`
}

func randomID() (string, error) {
	b := make([]byte, 16)
	if _, err := rand.Read(b); err != nil {
		return "", err
	}
	return hex.EncodeToString(b), nil
}
func safeID(id string) bool {
	if len(id) != 32 {
		return false
	}
	_, err := hex.DecodeString(id)
	return err == nil
}

// storageError carries a fixed operation name but does not format private paths.
type storageError struct {
	step  string
	cause error
}

func (e *storageError) Error() string { return e.step }
func (e *storageError) Unwrap() error { return e.cause }
func storageFailure(step string, err error) error {
	if err == nil {
		return nil
	}
	return &storageError{step: step, cause: err}
}
func syncDir(path string) error {
	f, err := os.Open(path)
	if err != nil {
		return storageFailure("open_directory", err)
	}
	defer f.Close()
	return storageFailure("sync_directory", f.Sync())
}
func atomicWrite(path string, data []byte, mode os.FileMode) error {
	dir := filepath.Dir(path)
	f, err := os.CreateTemp(dir, ".control-")
	if err != nil {
		return storageFailure("create_temporary", err)
	}
	name := f.Name()
	defer os.Remove(name)
	if err = f.Chmod(mode); err != nil {
		f.Close()
		return storageFailure("chmod_temporary", err)
	}
	if _, err = f.Write(data); err != nil {
		f.Close()
		return storageFailure("write_temporary", err)
	}
	if err = f.Sync(); err != nil {
		f.Close()
		return storageFailure("sync_file", err)
	}
	if err = f.Close(); err != nil {
		return storageFailure("close_temporary", err)
	}
	if err = os.Rename(name, path); err != nil {
		return storageFailure("rename_document", err)
	}
	return syncDir(dir)
}
func writeJSON(path string, value any) error {
	b, err := json.Marshal(value)
	if err != nil {
		return err
	}
	if len(b) > maxStoreBytes {
		return fmt.Errorf("store limit")
	}
	return atomicWrite(path, b, 0600)
}
func readJSON(path string, value any) error {
	f, err := os.Open(path)
	if err != nil {
		return err
	}
	defer f.Close()
	b, err := io.ReadAll(io.LimitReader(f, maxStoreBytes+1))
	if err != nil {
		return err
	}
	if len(b) > maxStoreBytes {
		return fmt.Errorf("store limit")
	}
	return json.Unmarshal(b, value)
}
func readDocument(path string) (snapshot, error) {
	info, err := os.Lstat(path)
	if os.IsNotExist(err) {
		return snapshot{Mode: 0600}, nil
	}
	if err != nil {
		return snapshot{}, err
	}
	if !info.Mode().IsRegular() || info.Size() > MaxDocumentBytes {
		return snapshot{}, fmt.Errorf("unsafe document")
	}
	f, err := os.Open(path)
	if err != nil {
		return snapshot{}, err
	}
	defer f.Close()
	b, err := io.ReadAll(io.LimitReader(f, MaxDocumentBytes+1))
	if err != nil {
		return snapshot{}, err
	}
	if len(b) > MaxDocumentBytes {
		return snapshot{}, fmt.Errorf("document limit")
	}
	return snapshot{Exists: true, Content: string(b), Mode: uint32(info.Mode().Perm())}, nil
}
func (m *Manager) livePath(module string) string {
	return filepath.Join(m.root, "etc", "config", module)
}
func (m *Manager) readLive() (map[string]snapshot, string, error) {
	live := map[string]snapshot{}
	h := sha256.New()
	for _, module := range modules {
		s, err := readDocument(m.livePath(module))
		if err != nil {
			return nil, "", failure("document_unavailable", "Cannot read a bounded native configuration document.")
		}
		live[module] = s
		io.WriteString(h, module+"\x00"+strconv.FormatBool(s.Exists)+"\x00"+s.Content+"\x00")
	}
	return live, hex.EncodeToString(h.Sum(nil)), nil
}
func (m *Manager) saveState() error {
	if err := writeJSON(filepath.Join(m.dataDir, "state.json"), m.disk); err != nil {
		return failure("storage_failed", "Cannot persist private configuration state.")
	}
	return nil
}
func (m *Manager) saveJournal() error {
	if err := writeJSON(filepath.Join(m.dataDir, "journal.json"), m.journal); err != nil {
		return failure("storage_failed", "Cannot persist configuration rollback journal.")
	}
	return nil
}
func validateStored(d diskState) error {
	if len(d.Drafts) > MaxDrafts {
		return fmt.Errorf("too many drafts")
	}
	total := 0
	ids := map[string]bool{}
	for _, s := range d.Drafts {
		if !safeID(s.ID) || ids[s.ID] || !allowed(s.Module) || len(s.Content) > MaxDocumentBytes || len(s.Diff) > 2*MaxDocumentBytes+4096 {
			return fmt.Errorf("invalid draft")
		}
		ids[s.ID] = true
		total += len(s.Content)
	}
	if total > maxDraftContentBytes {
		return fmt.Errorf("draft bytes limit")
	}
	return nil
}
func validateJournal(j *journal) error {
	if !safeID(j.Operation.ID) || len(j.Before) == 0 || len(j.Before) > len(modules) || len(j.Operation.ChangedModules) != len(j.Before) {
		return fmt.Errorf("invalid journal")
	}
	seen := map[string]bool{}
	for _, module := range j.Operation.ChangedModules {
		s, ok := j.Before[module]
		if !allowed(module) || seen[module] || !ok || len(s.Content) > MaxDocumentBytes {
			return fmt.Errorf("invalid snapshot")
		}
		seen[module] = true
	}
	switch j.Phase {
	case "applying", "pending", "committed", "rolling_back", "rolled_back":
	default:
		return fmt.Errorf("invalid phase")
	}
	return nil
}
func cleanTemps(dataDir string) error {
	entries, err := os.ReadDir(dataDir)
	if err != nil {
		return err
	}
	for _, e := range entries {
		if strings.HasPrefix(e.Name(), ".control-") || strings.HasPrefix(e.Name(), "candidate-") {
			if err := os.RemoveAll(filepath.Join(dataDir, e.Name())); err != nil {
				return err
			}
		}
	}
	return nil
}

// A second process must not clean validation directories or overwrite a journal
// while the first manager is committing. This lock is released on process exit.
func lockStore(dataDir string) (*os.File, error) {
	path := filepath.Join(dataDir, ".lock")
	if info, err := os.Lstat(path); err == nil && !info.Mode().IsRegular() {
		return nil, failure("unsafe_data_dir", "Control lock must be a regular private file.")
	}
	f, err := os.OpenFile(path, os.O_CREATE|os.O_RDWR, 0600)
	if err != nil {
		return nil, failure("storage_failed", "Cannot open private control lock.")
	}
	if err = f.Chmod(0600); err != nil {
		f.Close()
		return nil, err
	}
	if err = syscall.Flock(int(f.Fd()), syscall.LOCK_EX|syscall.LOCK_NB); err != nil {
		f.Close()
		return nil, failure("manager_busy", "Another configuration manager owns this data directory.")
	}
	return f, nil
}
func unlockStore(f *os.File) {
	if f != nil {
		_ = syscall.Flock(int(f.Fd()), syscall.LOCK_UN)
		_ = f.Close()
	}
}

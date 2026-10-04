// Package localrules persists an independent bounded proxy policy draft. Saving
// this file never compiles configuration, changes active policy or uses network
// access. The Root owns explicit preview/apply and supplies the data directory.
package localrules

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sync"
	"syscall"

	"be6500panel/internal/proxy"
	"be6500panel/internal/storage"
)

const (
	FileName     = "local-proxy-rules.json"
	MaxFileBytes = 256 << 10
)

var (
	ErrStorage      = errors.New("local proxy rule storage unavailable")
	ErrInvalidInput = errors.New("invalid local proxy rule store input")
	ErrDocumentSize = errors.New("local proxy policy document exceeds storage limit")
)

type Options struct {
	// DataDir is required. Keep one Store per Root-provided directory.
	DataDir string
	// Share the normal admission gate with all other persistent writers.
	StorageAdmission storage.Admission
	// Context optionally bounds startup loading; Save and Load take contexts.
	Context context.Context
}

type Snapshot struct {
	Policy   proxy.Policy `json:"policy"`
	Revision string       `json:"revision"`
}

// Store has a single-owner ordinary mutex, not a CAS or migration framework.
// In-memory state changes only after a successful atomic rename. A post-rename
// directory-sync error returns the committed snapshot with an error, never a
// false old state. Pre-commit failure leaves the old file and policy intact.
type Store struct {
	mu        sync.Mutex
	dir       string
	dirInfo   os.FileInfo
	state     Snapshot
	admission storage.Admission
	write     func(*os.File, []byte) (int, error)
	syncFile  func(*os.File) error
	syncDir   func(*os.File) error
	rename    func(string, string) error
}

func storageFailure(step string) error { return fmt.Errorf("%w: %s", ErrStorage, step) }

// openNoFollow also uses NONBLOCK so a swapped FIFO cannot hang a bounded read.
// The panel targets Unix (the router is Linux); native tests also run on macOS.
func openNoFollow(path string, directory bool) (*os.File, error) {
	flags := syscall.O_RDONLY | syscall.O_NOFOLLOW | syscall.O_NONBLOCK | syscall.O_CLOEXEC
	if directory {
		flags |= syscall.O_DIRECTORY
	}
	fd, err := syscall.Open(path, flags, 0)
	if err != nil {
		return nil, storageFailure("open private path")
	}
	return os.NewFile(uintptr(fd), path), nil
}

func privateDir(path string) (os.FileInfo, error) {
	info, err := os.Lstat(path)
	if err != nil && !errors.Is(err, os.ErrNotExist) {
		return nil, storageFailure("inspect directory")
	}
	if err == nil && (!info.IsDir() || info.Mode()&os.ModeSymlink != 0) {
		return nil, storageFailure("unsafe directory")
	}
	if err = os.MkdirAll(path, 0700); err != nil {
		return nil, storageFailure("create directory")
	}
	dir, err := openNoFollow(path, true)
	if err != nil {
		return nil, err
	}
	defer dir.Close()
	if err = dir.Chmod(0700); err != nil {
		return nil, storageFailure("private directory")
	}
	info, err = dir.Stat()
	if err != nil || !info.IsDir() {
		return nil, storageFailure("inspect private directory")
	}
	return info, nil
}

// New loads only local-proxy-rules.json. It never loads main config, nodes or
// subscription sources. A missing file means an empty overlay; malformed or
// unsafe existing state is returned as a fixed error, never silently reset.
func New(opts Options) (*Store, error) {
	if opts.DataDir == "" {
		return nil, ErrInvalidInput
	}
	ctx := opts.Context
	if ctx == nil {
		ctx = context.Background()
	}
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	dir, err := filepath.Abs(opts.DataDir)
	if err != nil {
		return nil, storageFailure("data directory")
	}
	info, err := privateDir(dir)
	if err != nil {
		return nil, err
	}
	s := &Store{dir: dir, dirInfo: info, admission: opts.StorageAdmission,
		write:    func(f *os.File, raw []byte) (int, error) { return f.Write(raw) },
		syncFile: func(f *os.File) error { return f.Sync() },
		syncDir:  func(f *os.File) error { return f.Sync() }, rename: os.Rename}
	if _, err = s.Load(ctx); err != nil {
		return nil, err
	}
	return s, nil
}

func cloneSnapshot(s Snapshot) Snapshot {
	return Snapshot{Policy: proxy.ClonePolicy(s.Policy), Revision: s.Revision}
}

func makeSnapshot(policy proxy.Policy) (Snapshot, error) {
	revision, err := proxy.PolicyRevision(policy)
	if err != nil {
		return Snapshot{}, err
	}
	return Snapshot{Policy: proxy.ClonePolicy(policy), Revision: revision}, nil
}

func (s *Store) openDirectory() (*os.File, error) {
	dir, err := openNoFollow(s.dir, true)
	if err != nil {
		return nil, err
	}
	info, err := dir.Stat()
	if err != nil || !info.IsDir() || !os.SameFile(info, s.dirInfo) {
		dir.Close()
		return nil, storageFailure("changed private directory")
	}
	return dir, nil
}

// Snapshot returns an owned clone and does not access disk.
func (s *Store) Snapshot() Snapshot {
	s.mu.Lock()
	defer s.mu.Unlock()
	return cloneSnapshot(s.state)
}

// Load explicitly reloads the independent overlay under the store lock. A
// failed reload preserves the previous snapshot instead of erasing the draft.
func (s *Store) Load(ctx context.Context) (Snapshot, error) {
	if ctx == nil {
		return Snapshot{}, ErrInvalidInput
	}
	if err := ctx.Err(); err != nil {
		return Snapshot{}, err
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if err := ctx.Err(); err != nil {
		return Snapshot{}, err
	}
	dir, err := s.openDirectory()
	if err != nil {
		return cloneSnapshot(s.state), err
	}
	defer dir.Close()
	policy, err := readPolicy(ctx, filepath.Join(s.dir, FileName))
	if err != nil {
		return cloneSnapshot(s.state), err
	}
	candidate, err := makeSnapshot(policy)
	if err != nil {
		return cloneSnapshot(s.state), storageFailure("invalid policy")
	}
	s.state = candidate
	return cloneSnapshot(s.state), nil
}

// Save validates and sizes a complete draft before storage admission or any
// file write. It never writes native config or activates rules. The revision
// is a content SHA256 for UI dirty/readback checks, not a generation counter.
func (s *Store) Save(ctx context.Context, policy proxy.Policy) (Snapshot, error) {
	if ctx == nil {
		return Snapshot{}, ErrInvalidInput
	}
	if err := ctx.Err(); err != nil {
		return Snapshot{}, err
	}
	candidate, err := makeSnapshot(policy)
	if err != nil {
		return Snapshot{}, err
	}
	raw, err := json.Marshal(candidate.Policy)
	if err != nil || len(raw) > MaxFileBytes {
		return Snapshot{}, ErrDocumentSize
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if err := ctx.Err(); err != nil {
		return cloneSnapshot(s.state), err
	}
	release, err := s.admit(ctx, int64(len(raw)))
	if err != nil {
		return cloneSnapshot(s.state), err
	}
	defer release()
	committed, err := s.atomicWrite(ctx, raw)
	if committed {
		s.state = candidate
	}
	return cloneSnapshot(s.state), err
}

func (s *Store) admit(ctx context.Context, size int64) (func(), error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	if s.admission == nil {
		return func() {}, nil
	}
	release, err := s.admission(ctx, s.dir, size, false)
	if err != nil {
		if release != nil {
			release()
		}
		if ctx.Err() != nil {
			return nil, ctx.Err()
		}
		if errors.Is(err, storage.ErrInsufficientSpace) {
			return nil, storage.ErrInsufficientSpace
		}
		if errors.Is(err, storage.ErrMeasurement) {
			return nil, storage.ErrMeasurement
		}
		return nil, storageFailure("admission")
	}
	if release == nil {
		release = func() {}
	}
	return release, nil
}

func safeDocument(path string) error {
	info, err := os.Lstat(path)
	if errors.Is(err, os.ErrNotExist) {
		return nil
	}
	if err != nil || !info.Mode().IsRegular() || info.Size() > MaxFileBytes {
		return storageFailure("unsafe document")
	}
	return nil
}

func (s *Store) atomicWrite(ctx context.Context, raw []byte) (committed bool, err error) {
	defer func() {
		if !committed && ctx.Err() != nil {
			err = ctx.Err()
		}
	}()
	if err := ctx.Err(); err != nil {
		return false, err
	}
	dir, err := s.openDirectory()
	if err != nil {
		return false, err
	}
	defer dir.Close()
	path := filepath.Join(s.dir, FileName)
	if err = safeDocument(path); err != nil {
		return false, err
	}
	// Detect unsupported directory fsync before changing the accepted file.
	if err = s.syncDir(dir); err != nil {
		return false, storageFailure("sync directory")
	}
	f, err := os.CreateTemp(s.dir, ".local-proxy-rules-")
	if err != nil {
		return false, storageFailure("create temporary")
	}
	temporary := f.Name()
	defer os.Remove(temporary)
	defer f.Close()
	if err = f.Chmod(0600); err != nil {
		return false, storageFailure("private temporary")
	}
	for offset := 0; offset < len(raw); {
		if err = ctx.Err(); err != nil {
			return false, err
		}
		end := offset + (32 << 10)
		if end > len(raw) {
			end = len(raw)
		}
		n, writeErr := s.write(f, raw[offset:end])
		if writeErr != nil || n != end-offset {
			return false, storageFailure("write temporary")
		}
		offset = end
	}
	if err = ctx.Err(); err != nil {
		return false, err
	}
	if err = s.syncFile(f); err != nil {
		return false, storageFailure("sync temporary")
	}
	if err = f.Close(); err != nil {
		return false, storageFailure("close temporary")
	}
	if err = ctx.Err(); err != nil {
		return false, err
	}
	if err = safeDocument(path); err != nil {
		return false, err
	}
	// Rename is the commit boundary; later cancellation cannot undo it.
	if err = s.rename(temporary, path); err != nil {
		return false, storageFailure("rename document")
	}
	if err = s.syncDir(dir); err != nil {
		return true, storageFailure("sync committed directory")
	}
	return true, nil
}

func readPolicy(ctx context.Context, path string) (proxy.Policy, error) {
	empty := proxy.ClonePolicy(proxy.Policy{})
	info, err := os.Lstat(path)
	if errors.Is(err, os.ErrNotExist) {
		return empty, nil
	}
	if err != nil || !info.Mode().IsRegular() || info.Size() > MaxFileBytes {
		return proxy.Policy{}, storageFailure("unsafe document")
	}
	f, err := openNoFollow(path, false)
	if err != nil {
		return proxy.Policy{}, err
	}
	defer f.Close()
	openedInfo, err := f.Stat()
	if err != nil || !openedInfo.Mode().IsRegular() || !os.SameFile(info, openedInfo) || openedInfo.Size() > MaxFileBytes {
		return proxy.Policy{}, storageFailure("changed document")
	}
	raw, err := io.ReadAll(io.LimitReader(f, MaxFileBytes+1))
	if err != nil || len(raw) > MaxFileBytes {
		return proxy.Policy{}, storageFailure("read limit")
	}
	if err = ctx.Err(); err != nil {
		return proxy.Policy{}, err
	}
	if !json.Valid(raw) || !uniqueJSON(raw) {
		return proxy.Policy{}, storageFailure("invalid document")
	}
	var fields map[string]json.RawMessage
	if json.Unmarshal(raw, &fields) != nil || len(fields) != 2 || fields["rules"] == nil || fields["subscriptionEdits"] == nil || bytes.Equal(bytes.TrimSpace(fields["rules"]), []byte("null")) || bytes.Equal(bytes.TrimSpace(fields["subscriptionEdits"]), []byte("null")) {
		return proxy.Policy{}, storageFailure("document fields")
	}
	decoder := json.NewDecoder(bytes.NewReader(raw))
	decoder.DisallowUnknownFields()
	var policy proxy.Policy
	if decoder.Decode(&policy) != nil || proxy.ValidatePolicy(policy) != nil {
		return proxy.Policy{}, storageFailure("invalid policy")
	}
	if err = f.Chmod(0600); err != nil {
		return proxy.Policy{}, storageFailure("private document")
	}
	if err = ctx.Err(); err != nil {
		return proxy.Policy{}, err
	}
	return proxy.ClonePolicy(policy), nil
}

// uniqueJSON rejects duplicate object keys and bounds corrupt nesting. The
// typed decoder then rejects unknown fields; no migration/version path exists.
func uniqueJSON(raw []byte) bool {
	decoder := json.NewDecoder(bytes.NewReader(raw))
	var walk func(int) error
	walk = func(depth int) error {
		if depth > 12 {
			return ErrStorage
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
					return ErrStorage
				}
				seen[key] = true
				if err := walk(depth + 1); err != nil {
					return err
				}
			}
		case '[':
			for decoder.More() {
				if err := walk(depth + 1); err != nil {
					return err
				}
			}
		default:
			return ErrStorage
		}
		_, err = decoder.Token()
		return err
	}
	if walk(0) != nil {
		return false
	}
	_, err := decoder.Token()
	return err == io.EOF
}

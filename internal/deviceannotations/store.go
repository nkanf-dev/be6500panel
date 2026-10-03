package deviceannotations

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
)

// Store serializes snapshots and CAS updates through a context-aware gate. Each
// accepted state owns its data; callers cannot mutate the store through slices.
// One process/Store owns this file, as with the panel's other persistent stores.
type Store struct {
	dir       string
	gate      chan struct{}
	state     Snapshot
	admission func(context.Context, string, int64, bool) (func(), error)
	write     func(*os.File, []byte) (int, error) // deterministic I/O failure test seams
	sync      func(*os.File) error
	rename    func(string, string) error
}

func storageFailure(step string) error { return fmt.Errorf("%w: %s", ErrStorage, step) }

// New loads a bounded existing document without allocating a candidate file.
// A missing file is an empty revision-zero store; corrupt state is never reset.
func New(opts Options) (*Store, error) {
	if opts.DataDir == "" {
		opts.DataDir = DefaultDataDir
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
	if err = os.MkdirAll(dir, 0700); err != nil {
		return nil, storageFailure("create directory")
	}
	info, err := os.Lstat(dir)
	if err != nil || !info.IsDir() || info.Mode()&os.ModeSymlink != 0 {
		return nil, storageFailure("unsafe directory")
	}
	if err = os.Chmod(dir, 0700); err != nil {
		return nil, storageFailure("private directory")
	}
	s := &Store{dir: dir, gate: make(chan struct{}, 1), state: Snapshot{Devices: map[string]Annotation{}}, admission: opts.StorageAdmission,
		write: func(f *os.File, b []byte) (int, error) { return f.Write(b) }, sync: func(f *os.File) error { return f.Sync() }, rename: os.Rename}
	s.gate <- struct{}{}
	s.state, err = load(ctx, filepath.Join(dir, FileName))
	if err != nil {
		return nil, err
	}
	if err = ctx.Err(); err != nil {
		return nil, err
	}
	return s, nil
}

func (s *Store) acquire(ctx context.Context) error {
	if ctx == nil {
		return ErrInvalidInput
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	select {
	case <-ctx.Done():
		return ctx.Err()
	case <-s.gate:
		if err := ctx.Err(); err != nil {
			s.release()
			return err
		}
		return nil
	}
}
func (s *Store) release() { s.gate <- struct{}{} }

func (s *Store) Snapshot(ctx context.Context) (Snapshot, error) {
	if err := s.acquire(ctx); err != nil {
		return Snapshot{}, err
	}
	defer s.release()
	return cloneSnapshot(s.state), nil
}

// Save changes one canonical MAC at the expected global revision. Empty label,
// note and tags delete the entry, so the bounded store can reclaim old devices.
// Every successful update advances the revision, including an empty deletion.
func (s *Store) Save(ctx context.Context, input UpdateRequest) (Snapshot, error) {
	mac, err := CanonicalMAC(input.MAC)
	if err != nil || input.ExpectedRevision > MaxRevision {
		return Snapshot{}, ErrInvalidInput
	}
	annotation := Annotation{Label: input.Label, Note: input.Note, Tags: input.Tags}
	if err = validateAnnotation(annotation); err != nil {
		return Snapshot{}, err
	}
	annotation = cloneAnnotation(annotation)
	if err = s.acquire(ctx); err != nil {
		return Snapshot{}, err
	}
	defer s.release()
	if s.state.Revision != input.ExpectedRevision {
		return Snapshot{}, ErrConflict
	}
	if s.state.Revision == MaxRevision {
		return Snapshot{}, ErrRevisionExhausted
	}
	candidate := cloneSnapshot(s.state)
	empty := annotation.Label == "" && annotation.Note == "" && len(annotation.Tags) == 0
	if empty {
		delete(candidate.Devices, mac)
	} else {
		if _, exists := candidate.Devices[mac]; !exists && len(candidate.Devices) >= MaxDevices {
			return Snapshot{}, ErrLimit
		}
		candidate.Devices[mac] = annotation
	}
	candidate.Revision++
	raw, err := json.Marshal(candidate)
	if err != nil || len(raw) > MaxFileBytes {
		return Snapshot{}, storageFailure("document limit")
	}
	release, err := s.admit(ctx, int64(len(raw)))
	if err != nil {
		return Snapshot{}, err
	}
	defer release()
	committed, err := s.atomicWrite(ctx, raw)
	// Rename is the commit point. A later cancellation must not report an
	// uncommitted save, and a directory sync error cannot rewind that rename.
	if committed {
		s.state = candidate
		return cloneSnapshot(s.state), err
	}
	return Snapshot{}, err
}

func (s *Store) admit(ctx context.Context, size int64) (func(), error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	if s.admission == nil {
		return func() {}, nil
	}
	// Shared admission applies block rounding and metadata overhead itself.
	// Reserve the whole temporary, never a net file-size difference.
	release, err := s.admission(ctx, s.dir, size, false)
	if err != nil {
		if release != nil {
			release()
		}
		if ctx.Err() != nil {
			return nil, ctx.Err()
		}
		return nil, err
	}
	if release == nil {
		release = func() {}
	}
	return release, nil
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
	path := filepath.Join(s.dir, FileName)
	if info, err := os.Lstat(path); err == nil {
		if !info.Mode().IsRegular() {
			return false, storageFailure("unsafe document")
		}
	} else if !errors.Is(err, os.ErrNotExist) {
		return false, storageFailure("inspect document")
	}
	directory, err := os.Open(s.dir)
	if err != nil {
		return false, storageFailure("open directory")
	}
	defer directory.Close()
	f, err := os.CreateTemp(s.dir, ".device-names-")
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
		if writeErr != nil {
			return false, storageFailure("write temporary")
		}
		if n != end-offset {
			return false, storageFailure("short write")
		}
		offset = end
	}
	if err = ctx.Err(); err != nil {
		return false, err
	}
	if err = s.sync(f); err != nil {
		return false, storageFailure("sync temporary")
	}
	if err = f.Close(); err != nil {
		return false, storageFailure("close temporary")
	}
	if err = ctx.Err(); err != nil {
		return false, err
	}
	if err = s.rename(temporary, path); err != nil {
		return false, storageFailure("rename document")
	}
	if err = directory.Sync(); err != nil {
		return true, storageFailure("sync directory")
	}
	return true, nil
}

func load(ctx context.Context, path string) (Snapshot, error) {
	empty := Snapshot{Devices: map[string]Annotation{}}
	info, err := os.Lstat(path)
	if errors.Is(err, os.ErrNotExist) {
		return empty, nil
	}
	if err != nil || !info.Mode().IsRegular() || info.Size() > MaxFileBytes {
		return Snapshot{}, storageFailure("unsafe document")
	}
	f, err := os.Open(path)
	if err != nil {
		return Snapshot{}, storageFailure("open document")
	}
	defer f.Close()
	raw, err := io.ReadAll(io.LimitReader(f, MaxFileBytes+1))
	if err != nil || len(raw) > MaxFileBytes {
		return Snapshot{}, storageFailure("read limit")
	}
	if err = ctx.Err(); err != nil {
		return Snapshot{}, err
	}
	if !json.Valid(raw) || !uniqueJSON(raw) {
		return Snapshot{}, storageFailure("invalid document")
	}
	var object map[string]json.RawMessage
	if json.Unmarshal(raw, &object) != nil || len(object) != 2 || object["revision"] == nil || object["devices"] == nil {
		return Snapshot{}, storageFailure("document fields")
	}
	var state Snapshot
	if json.Unmarshal(object["revision"], &state.Revision) != nil || bytes.Equal(bytes.TrimSpace(object["revision"]), []byte("null")) || state.Revision > MaxRevision {
		return Snapshot{}, storageFailure("revision limit")
	}
	var devices map[string]json.RawMessage
	if json.Unmarshal(object["devices"], &devices) != nil || devices == nil || len(devices) > MaxDevices {
		return Snapshot{}, storageFailure("device limit")
	}
	state.Devices = make(map[string]Annotation, len(devices))
	for mac, rawAnnotation := range devices {
		canonical, err := CanonicalMAC(mac)
		if err != nil || canonical != mac {
			return Snapshot{}, storageFailure("MAC key")
		}
		var fields map[string]json.RawMessage
		if json.Unmarshal(rawAnnotation, &fields) != nil || len(fields) != 3 || fields["label"] == nil || fields["note"] == nil || fields["tags"] == nil {
			return Snapshot{}, storageFailure("annotation fields")
		}
		for _, value := range fields {
			if bytes.Equal(bytes.TrimSpace(value), []byte("null")) {
				return Snapshot{}, storageFailure("null annotation")
			}
		}
		var a Annotation
		if json.Unmarshal(rawAnnotation, &a) != nil || validateAnnotation(a) != nil {
			return Snapshot{}, storageFailure("annotation limit")
		}
		state.Devices[mac] = cloneAnnotation(a)
	}
	if err = ctx.Err(); err != nil {
		return Snapshot{}, err
	}
	if err = f.Chmod(0600); err != nil {
		return Snapshot{}, storageFailure("private document")
	}
	return state, nil
}

// Bounded duplicate detection also rejects deeply nested corrupt documents.
func uniqueJSON(raw []byte) bool {
	d := json.NewDecoder(bytes.NewReader(raw))
	var walk func(int) error
	walk = func(depth int) error {
		if depth > 16 {
			return errors.New("nesting")
		}
		token, err := d.Token()
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
			for d.More() {
				key, err := d.Token()
				if err != nil {
					return err
				}
				name, ok := key.(string)
				if !ok || seen[name] {
					return errors.New("duplicate")
				}
				seen[name] = true
				if err = walk(depth + 1); err != nil {
					return err
				}
			}
		case '[':
			for d.More() {
				if err := walk(depth + 1); err != nil {
					return err
				}
			}
		default:
			return errors.New("delimiter")
		}
		_, err = d.Token()
		return err
	}
	if walk(0) != nil {
		return false
	}
	_, err := d.Token()
	return err == io.EOF
}

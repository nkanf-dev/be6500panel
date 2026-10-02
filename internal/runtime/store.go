package runtime

import (
	"be6500panel/internal/storage"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strconv"
	"syscall"
)

type configRecord struct {
	Generation uint64 `json:"generation"`
	File       string `json:"file"`
	SHA256     string `json:"sha256"`
	Ready      bool   `json:"ready,omitempty"`
}
type diskState struct {
	Generation uint64        `json:"generation"`
	Current    *configRecord `json:"current,omitempty"`
	LastGood   *configRecord `json:"lastGood,omitempty"`
	Artifact   *Artifact     `json:"artifact,omitempty"`
}

func privateDir(path string) error {
	if err := os.MkdirAll(path, 0700); err != nil {
		return err
	}
	info, err := os.Lstat(path)
	if err != nil {
		return err
	}
	if !info.IsDir() || info.Mode()&os.ModeSymlink != 0 {
		return errors.New("runtime directory must be a real directory")
	}
	return os.Chmod(path, 0700)
}

func atomicWrite(path string, data []byte, mode os.FileMode, syncDirectory func(string) error) error {
	return atomicWritePrivate(Options{}, path, data, mode, syncDirectory)
}
func atomicWritePrivate(opts Options, path string, data []byte, mode os.FileMode, syncDirectory func(string) error) (err error) {
	defer func() { err = storageWriteError(err) }()
	if err := writeContext(opts).Err(); err != nil {
		return err
	}
	dir := filepath.Dir(path)
	f, err := os.CreateTemp(dir, ".atomic-")
	if err != nil {
		return err
	}
	name := f.Name()
	defer os.Remove(name)
	defer f.Close()
	if err = f.Chmod(mode); err != nil {
		return err
	}
	if n, writeErr := writePrivate(opts, f, data); writeErr != nil {
		return writeErr
	} else if n != len(data) {
		return io.ErrShortWrite
	}
	if err = f.Sync(); err != nil {
		return err
	}
	if err = f.Close(); err != nil {
		return err
	}
	if err = writeContext(opts).Err(); err != nil {
		return err
	}
	if err = os.Rename(name, path); err != nil {
		return err
	}
	// Rename is the transaction boundary. A post-commit sync failure must never
	// be confused with a failed commit or followed by pruning older snapshots.
	if err := syncDirectory(dir); err != nil {
		return ErrDurability
	}
	return nil
}
func syncDir(path string) error {
	f, err := os.Open(path)
	if err != nil {
		return err
	}
	defer f.Close()
	return f.Sync()
}

func readBounded(path string, max int64) ([]byte, error) {
	info, err := os.Lstat(path)
	if err != nil {
		return nil, err
	}
	if !info.Mode().IsRegular() || info.Size() > max {
		return nil, errors.New("invalid or oversized state file")
	}
	if info.Mode().Perm() != 0600 {
		if err = os.Chmod(path, 0600); err != nil {
			return nil, err
		}
	}
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	b, err := io.ReadAll(io.LimitReader(f, max+1))
	if err != nil {
		return nil, err
	}
	if int64(len(b)) > max {
		return nil, errors.New("oversized state file")
	}
	return b, nil
}
func statePath(opts Options, service string) string {
	return filepath.Join(opts.DataDir, service, "state.json")
}
func configPath(opts Options, service string, record *configRecord) string {
	return filepath.Join(opts.DataDir, service, record.File)
}

func loadState(opts Options, service string) (diskState, error) {
	var state diskState
	b, err := readBounded(statePath(opts, service), 16<<10)
	if os.IsNotExist(err) {
		return state, nil
	}
	if err != nil {
		return state, err
	}
	if err = json.Unmarshal(b, &state); err != nil {
		return diskState{}, errors.New("invalid runtime state")
	}
	// Older state only proves verification, not readiness. Never restore an
	// unproven legacy LastGood over a running service.
	if state.LastGood != nil && !state.LastGood.Ready {
		state.LastGood = nil
	}
	for _, r := range []*configRecord{state.Current, state.LastGood} {
		if r == nil {
			continue
		}
		prefix := "config-" + strconv.FormatUint(r.Generation, 10)
		validName := r.File == prefix+".json" || service == FRPC && r.File == prefix+".toml"
		if r.Generation == 0 || r.Generation > state.Generation || !validName {
			return diskState{}, errors.New("invalid config state reference")
		}
		hash, err := hex.DecodeString(r.SHA256)
		if err != nil || len(hash) != sha256.Size {
			return diskState{}, errors.New("invalid config digest")
		}
		data, err := readBounded(configPath(opts, service, r), storedConfigLimit(opts))
		if err != nil {
			return diskState{}, errors.New("accepted config is unavailable")
		}
		sum := sha256.Sum256(data)
		if hex.EncodeToString(sum[:]) != r.SHA256 {
			return diskState{}, errors.New("accepted config checksum mismatch")
		}
	}
	if state.Current == nil && state.LastGood != nil {
		return diskState{}, errors.New("invalid config state")
	}
	return state, nil
}
func saveState(opts Options, service string, state diskState) error {
	data, err := json.Marshal(state)
	if err != nil {
		return err
	}
	if len(data)+1 > 16<<10 {
		return errors.New("runtime metadata exceeds state limit")
	}
	release, err := admitWrite(opts, statePath(opts, service), int64(len(data)+1))
	if err != nil {
		return err
	}
	defer release()
	return atomicWritePrivate(opts, statePath(opts, service), append(data, '\n'), 0600, stateSync(opts))
}

// stageConfig gives the verifier a private candidate path. No API receives this
// file's contents and failures never modify the current state manifest.
func stageConfig(opts Options, service string, raw []byte) (path string, err error) {
	defer func() { err = storageWriteError(err) }()
	release, err := admitWrite(opts, filepath.Join(opts.DataDir, service), int64(len(raw)))
	if err != nil {
		return "", err
	}
	defer release()
	if err = writeContext(opts).Err(); err != nil {
		return "", err
	}
	f, err := os.CreateTemp(filepath.Join(opts.DataDir, service), ".candidate-*"+configExtension(service, raw))
	if err != nil {
		return "", err
	}
	name := f.Name()
	success := false
	defer func() {
		f.Close()
		if !success {
			os.Remove(name)
		}
	}()
	if err = f.Chmod(0600); err != nil {
		return "", err
	}
	if n, writeErr := writePrivate(opts, f, raw); writeErr != nil {
		return "", writeErr
	} else if n != len(raw) {
		return "", io.ErrShortWrite
	}
	if err = f.Sync(); err != nil {
		return "", err
	}
	if err = f.Close(); err != nil {
		return "", err
	}
	if err = writeContext(opts).Err(); err != nil {
		return "", err
	}
	success = true
	return name, nil
}

func commitConfig(opts Options, service string, state diskState, candidate string, raw []byte) (nextState diskState, err error) {
	defer func() { err = storageWriteError(err) }()
	checkedBytes, checkErr := readBounded(candidate, storedConfigLimit(opts))
	if checkErr != nil || sha256.Sum256(checkedBytes) != sha256.Sum256(raw) {
		return state, errors.New("verifier modified private candidate")
	}
	if state.Generation == ^uint64(0) {
		return state, errors.New("configuration generation exhausted")
	}
	gen := state.Generation + 1
	sum := sha256.Sum256(raw)
	record := &configRecord{Generation: gen, File: fmt.Sprintf("config-%d%s", gen, configExtension(service, raw)), SHA256: hex.EncodeToString(sum[:])}
	dest := configPath(opts, service, record)
	next := state
	next.Generation = gen
	if state.Current != nil && state.Current.Ready {
		next.LastGood = state.Current
	}
	next.Current = record
	data, err := json.Marshal(next)
	if err != nil {
		return state, err
	}
	release, err := admitWrite(opts, statePath(opts, service), int64(len(data)+1)+4096)
	if err != nil {
		return state, err
	}
	defer release()
	opts.storageAdmitted = true
	if err := writeContext(opts).Err(); err != nil {
		return state, err
	}
	if err := os.Rename(candidate, dest); err != nil {
		return state, err
	}
	if err := stateSync(opts)(filepath.Dir(dest)); err != nil {
		_ = os.Remove(dest)
		return state, err
	}
	if err := saveState(opts, service, next); err != nil {
		if errors.Is(err, ErrDurability) {
			return next, err
		}
		os.Remove(dest)
		return state, err
	}
	return next, nil
}
func pruneConfigs(opts Options, service string, state diskState) {
	dir := filepath.Join(opts.DataDir, service)
	entries, err := os.ReadDir(dir)
	if err != nil {
		return
	}
	keep := map[string]bool{"state.json": true}
	for _, r := range []*configRecord{state.Current, state.LastGood} {
		if r != nil {
			keep[r.File] = true
		}
	}
	for _, e := range entries {
		name := e.Name()
		if !keep[name] && !e.IsDir() && ((filepath.Ext(name) == ".json" || filepath.Ext(name) == ".toml") && len(name) > 7 && name[:7] == "config-" || len(name) > 11 && name[:11] == ".candidate-") {
			_ = os.Remove(filepath.Join(dir, name))
		}
	}
}

func configExtension(service string, raw []byte) string {
	if service == FRPC && !json.Valid(raw) {
		return ".toml"
	}
	return ".json"
}

func stateSync(opts Options) func(string) error {
	if opts.syncDirectory != nil {
		return opts.syncDirectory
	}
	return syncDir
}

// storedConfigLimit preserves reads and rollback of configurations accepted
// before installed write limits were lowered for small router flash.
func storedConfigLimit(opts Options) int64 {
	if opts.MaxConfigBytes > 4<<20 {
		return opts.MaxConfigBytes
	}
	return 4 << 20
}
func writeContext(opts Options) context.Context {
	if opts.storageContext != nil {
		return opts.storageContext
	}
	return context.Background()
}
func writePrivate(opts Options, f *os.File, data []byte) (int, error) {
	if err := writeContext(opts).Err(); err != nil {
		return 0, err
	}
	var n int
	var err error
	if opts.storageWrite != nil {
		n, err = opts.storageWrite(f, data)
	} else {
		n, err = f.Write(data)
	}
	if errors.Is(err, syscall.ENOSPC) {
		return n, storage.ErrInsufficientSpace
	}
	if err == nil {
		if cancelErr := writeContext(opts).Err(); cancelErr != nil {
			return n, cancelErr
		}
	}
	return n, err
}
func admitWrite(opts Options, path string, bytes int64) (func(), error) {
	if err := writeContext(opts).Err(); err != nil {
		return nil, err
	}
	if opts.StorageAdmission == nil || opts.storageAdmitted {
		return func() {}, nil
	}
	return opts.StorageAdmission(writeContext(opts), path, bytes, opts.storageRollback)
}

// admitConfig holds candidate/recovery scratch and metadata headroom through
// verification, commit, readiness, and possible recovery. Use the returned
// options for nested writes, and defer release through transaction cleanup.
func admitConfig(opts Options, ctx context.Context, service string, raw []byte, rollback bool) (release func(), operationOpts Options, err error) {
	opts.storageContext = ctx
	opts.storageRollback = rollback
	if ctx == nil {
		return nil, opts, errors.New("storage context is required")
	}
	if err = ctx.Err(); err != nil {
		return nil, opts, err
	}
	if opts.StorageAdmission == nil {
		return func() {}, opts, nil
	}
	bytes := 2*int64(len(raw)) + (48 << 10)
	if rollback {
		bytes = int64(len(raw)) + (32 << 10)
	}
	release, err = opts.StorageAdmission(ctx, filepath.Join(opts.DataDir, service), bytes, rollback)
	if err != nil {
		return nil, opts, err
	}
	opts.storageAdmitted = true
	return release, opts, nil
}
func stageRollbackConfig(opts Options, service string, raw []byte) (string, error) {
	opts.storageRollback = true
	return stageConfig(opts, service, raw)
}
func saveRollbackState(opts Options, service string, state diskState) error {
	opts.storageRollback = true
	return saveState(opts, service, state)
}

func storageWriteError(err error) error {
	if errors.Is(err, syscall.ENOSPC) {
		return storage.ErrInsufficientSpace
	}
	return err
}

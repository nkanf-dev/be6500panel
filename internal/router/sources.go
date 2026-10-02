package router

import (
	"context"
	"errors"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"
)

const (
	fileLimit      = 256 << 10
	procLimit      = 512 << 10
	commandLimit   = 512 << 10
	commandTimeout = 750 * time.Millisecond
)

var errTooLarge = errors.New("source exceeds size limit")
var errOutsideRoot = errors.New("source is outside fixture root")

func (a *Adapter) readFile(path string, limit int64) ([]byte, error) {
	full := filepath.Join(a.root, filepath.FromSlash(strings.TrimPrefix(path, "/")))
	// A fixture may contain normal in-root symlinks, but must not read the host.
	if !a.live {
		resolved, err := filepath.EvalSymlinks(full)
		if err != nil {
			return nil, err
		}
		relative, err := filepath.Rel(a.root, resolved)
		if err != nil || relative == ".." || strings.HasPrefix(relative, ".."+string(filepath.Separator)) {
			return nil, errOutsideRoot
		}
		full = resolved
	}
	f, err := os.Open(full)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	info, err := f.Stat()
	if err != nil {
		return nil, err
	}
	if !info.Mode().IsRegular() {
		return nil, errors.New("source is not a regular file")
	}
	if info.Size() > limit {
		return nil, errTooLarge
	}
	data, err := io.ReadAll(io.LimitReader(f, limit+1))
	if err != nil {
		return nil, err
	}
	if int64(len(data)) > limit {
		return nil, errTooLarge
	}
	return data, nil
}

func (a *Adapter) firstFile(paths []string, limit int64) ([]byte, error) {
	var missing error
	for _, path := range paths {
		data, err := a.readFile(path, limit)
		if err == nil {
			return data, nil
		}
		if !errors.Is(err, os.ErrNotExist) {
			return nil, err
		}
		missing = err
	}
	if missing == nil {
		missing = os.ErrNotExist
	}
	return nil, missing
}

type boundedOutput struct {
	data     []byte
	limit    int
	exceeded bool
}

func (b *boundedOutput) Write(p []byte) (int, error) {
	remaining := b.limit - len(b.data)
	if len(p) > remaining {
		b.data = append(b.data, p[:remaining]...)
		b.exceeded = true
		return remaining, errTooLarge
	}
	b.data = append(b.data, p...)
	return len(p), nil
}

func (a *Adapter) firewallSource(ctx context.Context, ipv6 bool) ([]byte, error) {
	if !a.live {
		path := "/var/run/be6500panel/iptables-save"
		if ipv6 {
			path = "/var/run/be6500panel/ip6tables-save"
		}
		return a.readFile(path, commandLimit)
	}
	name := "iptables-save"
	if ipv6 {
		name = "ip6tables-save"
	}
	// Both command names and argv are constants. No shell, user input, rule
	// writes, or raw stderr is used. A fixture root never reaches this branch.
	return readCommand(ctx, name)
}

// Only firewallSource calls this in production, with its fixed read-only argv.
func readCommand(ctx context.Context, name string, args ...string) ([]byte, error) {
	child, cancel := context.WithTimeout(ctx, commandTimeout)
	defer cancel()
	cmd := exec.CommandContext(child, name, args...)
	cmd.WaitDelay = 100 * time.Millisecond
	output := &boundedOutput{limit: commandLimit}
	cmd.Stdout = output
	cmd.Stderr = io.Discard
	err := cmd.Run()
	if output.exceeded {
		return nil, errTooLarge
	}
	if child.Err() != nil {
		return nil, child.Err()
	}
	if err != nil {
		return nil, err
	}
	return output.data, nil
}

func sourceCode(err error) string {
	switch {
	case errors.Is(err, errTooLarge):
		return "too_large"
	case errors.Is(err, context.DeadlineExceeded):
		return "timeout"
	case errors.Is(err, context.Canceled):
		return "canceled"
	case errors.Is(err, os.ErrNotExist), errors.Is(err, exec.ErrNotFound):
		return "unavailable"
	case errors.Is(err, os.ErrPermission), errors.Is(err, errOutsideRoot):
		return "permission_denied"
	default:
		return "read_failed"
	}
}

var errorMessages = map[string]string{
	"too_large":         "Observation exceeded its size limit.",
	"timeout":           "Observation command exceeded its time limit.",
	"canceled":          "Observation was canceled.",
	"unavailable":       "Observation source is unavailable.",
	"permission_denied": "Observation source is not accessible.",
	"read_failed":       "Observation source could not be read.",
	"invalid":           "Observation contains malformed or unsupported data; valid rows were kept.",
}

func moduleError(module, code string) ModuleError {
	return ModuleError{Module: module, Code: code, Message: errorMessages[code]}
}

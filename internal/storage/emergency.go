package storage

import (
	"context"
	"errors"
	"io"
	"os"
	"path/filepath"
)

type fileReserve struct{ path string }

func (r *fileReserve) Size() (int64, error) {
	info, err := os.Lstat(r.path)
	if os.IsNotExist(err) {
		return 0, nil
	}
	if err != nil {
		return 0, err
	}
	if !info.Mode().IsRegular() {
		return 0, errors.New("emergency reserve must be a regular file")
	}
	return info.Size(), nil
}
func (r *fileReserve) Allocate(ctx context.Context, size int64) error {
	dir := filepath.Dir(r.path)
	f, err := os.CreateTemp(dir, ".reserve-")
	if err != nil {
		return err
	}
	name := f.Name()
	defer os.Remove(name)
	defer f.Close()
	if err = f.Chmod(0600); err != nil {
		return err
	}
	zero := make([]byte, 32<<10)
	for size > 0 {
		if err := ctx.Err(); err != nil {
			return err
		}
		n := int64(len(zero))
		if n > size {
			n = size
		}
		wrote, writeErr := f.Write(zero[:int(n)])
		if writeErr != nil {
			return writeErr
		}
		if wrote != int(n) {
			return io.ErrShortWrite
		}
		size -= n
	}
	if err = f.Sync(); err != nil {
		return err
	}
	if err = f.Close(); err != nil {
		return err
	}
	if err = ctx.Err(); err != nil {
		return err
	}
	if err = os.Rename(name, r.path); err != nil {
		return err
	}
	return syncDirectory(dir)
}
func (r *fileReserve) Reclaim() error {
	if err := os.Remove(r.path); err != nil && !os.IsNotExist(err) {
		return err
	}
	return syncDirectory(filepath.Dir(r.path))
}
func syncDirectory(path string) error {
	f, err := os.Open(path)
	if err != nil {
		return err
	}
	defer f.Close()
	return f.Sync()
}

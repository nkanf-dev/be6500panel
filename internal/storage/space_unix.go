//go:build linux || darwin

package storage

import (
	"math"
	"os"
	"path/filepath"
	"syscall"
)

func measureSpace(path string) (Space, error) {
	// An atomic destination may not exist yet. Inspect its nearest existing
	// parent on the same filesystem rather than creating a probe file.
	for {
		info, err := os.Stat(path)
		if err == nil {
			var fs syscall.Statfs_t
			if err = syscall.Statfs(path, &fs); err != nil {
				return Space{}, err
			}
			free := uint64(fs.Bavail) * uint64(fs.Bsize)
			if uint64(fs.Bsize) != 0 && uint64(fs.Bavail) > math.MaxInt64/uint64(fs.Bsize) {
				free = math.MaxInt64
			}
			stat := info.Sys().(*syscall.Stat_t)
			return Space{Volume: uint64(stat.Dev), Available: int64(free)}, nil
		}
		if !os.IsNotExist(err) {
			return Space{}, err
		}
		parent := filepath.Dir(path)
		if parent == path {
			return Space{}, err
		}
		path = parent
	}
}

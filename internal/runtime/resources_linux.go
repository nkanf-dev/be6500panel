//go:build linux

package runtime

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"syscall"
)

func processRSS(pid int) (int64, bool) {
	// statm is tiny and contains no credentials. Linux RSS uses the native page size.
	f, err := os.Open(filepath.Join("/proc", strconv.Itoa(pid), "statm"))
	if err != nil {
		return 0, false
	}
	defer f.Close()
	var data [256]byte
	n, err := f.Read(data[:])
	if err != nil {
		return 0, false
	}
	fields := strings.Fields(string(data[:n]))
	if len(fields) < 2 {
		return 0, false
	}
	pages, err := strconv.ParseInt(fields[1], 10, 64)
	if err != nil || pages < 0 {
		return 0, false
	}
	return pages * int64(os.Getpagesize()), true
}
func checkRunSpace(opts Options) error {
	var stat syscall.Statfs_t
	if err := syscall.Statfs(opts.RunDir, &stat); err != nil {
		return fmt.Errorf("cannot inspect runtime capacity: %w", err)
	}
	available := uint64(stat.Bavail) * uint64(stat.Bsize)
	required := uint64(opts.MaxUncompressedBytes) + uint64(opts.MinFreeRunBytes)
	if available < required {
		return errors.New("insufficient runtime free space for bounded artifact")
	}
	return nil
}

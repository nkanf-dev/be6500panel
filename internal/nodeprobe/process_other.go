//go:build !linux && !darwin && !freebsd && !openbsd && !netbsd && !dragonfly

package nodeprobe

import (
	"errors"
	"os/exec"
)

func prepareProcessGroup(*exec.Cmd) error {
	return errors.New("isolated process groups are unavailable")
}
func terminateProcessGroup(*exec.Cmd, bool) {}

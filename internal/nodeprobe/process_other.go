//go:build !linux && !darwin

package nodeprobe

import (
	"errors"
	"os/exec"
)

func prepareProcessGroup(*exec.Cmd) error {
	return errors.New("isolated process groups are unavailable")
}
func terminateProcessGroup(*exec.Cmd, bool) {}

func retainProbeLeader(int) bool { return false }

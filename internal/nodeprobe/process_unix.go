//go:build linux || darwin || freebsd || openbsd || netbsd || dragonfly

package nodeprobe

import (
	"os/exec"
	"syscall"
)

func prepareProcessGroup(cmd *exec.Cmd) error {
	cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}
	return nil
}
func terminateProcessGroup(cmd *exec.Cmd, force bool) {
	if cmd.Process == nil {
		return
	}
	signal := syscall.SIGTERM
	if force {
		signal = syscall.SIGKILL
	}
	// Setpgid above makes this exact owned child the process group leader. Never
	// search names, use pkill, or signal the panel/active core process group.
	_ = syscall.Kill(-cmd.Process.Pid, signal)
	if force {
		_ = cmd.Process.Kill()
	}
}

//go:build linux

package nodeprobe

import (
	"os/exec"
	"syscall"
	"unsafe"
)

func prepareProcessGroup(cmd *exec.Cmd) error {
	cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true, Pdeathsig: syscall.SIGKILL}
	return nil
}
func retainProbeLeader(pid int) bool {
	var info [128]byte
	for {
		_, _, errno := syscall.Syscall6(syscall.SYS_WAITID, 1, uintptr(pid), uintptr(unsafe.Pointer(&info[0])), 4|0x01000000, 0, 0)
		if errno == syscall.EINTR {
			continue
		}
		return errno == 0
	}
}

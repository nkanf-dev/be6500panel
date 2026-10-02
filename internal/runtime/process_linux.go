//go:build linux

package runtime

import (
	"syscall"
	"unsafe"
)

// retainExitedLeader waits without reaping. The leader keeps its PID/PGID
// reserved until the process supervisor kills descendants then calls Cmd.Wait.
func retainExitedLeader(pid int) bool {
	var info [128]byte // Linux siginfo_t, deliberately no architecture field parsing.
	for {
		_, _, errno := syscall.Syscall6(syscall.SYS_WAITID, 1, uintptr(pid), uintptr(unsafe.Pointer(&info[0])), 4|0x01000000, 0, 0)
		if errno == syscall.EINTR {
			continue
		}
		return errno == 0
	}
}

func processAttributes() *syscall.SysProcAttr {
	return &syscall.SysProcAttr{Setpgid: true, Pdeathsig: syscall.SIGKILL}
}

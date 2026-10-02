//go:build darwin

package runtime

import (
	"syscall"
	"unsafe"
)

func retainExitedLeader(pid int) bool {
	var info [128]byte
	for {
		_, _, errno := syscall.Syscall6(syscall.SYS_WAITID, 1, uintptr(pid), uintptr(unsafe.Pointer(&info[0])), syscall.WEXITED|syscall.WNOWAIT, 0, 0)
		if errno == syscall.EINTR {
			continue
		}
		return errno == 0
	}
}

func processAttributes() *syscall.SysProcAttr { return &syscall.SysProcAttr{Setpgid: true} }

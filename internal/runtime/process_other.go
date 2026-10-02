//go:build !linux && !darwin

package runtime

import "syscall"

// The supported production ABI is Linux ARMv7; native fixture tests use Darwin.
func retainExitedLeader(pid int) bool { return false }

func processAttributes() *syscall.SysProcAttr { return &syscall.SysProcAttr{Setpgid: true} }

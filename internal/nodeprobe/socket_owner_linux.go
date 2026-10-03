//go:build linux

package nodeprobe

func newCoreSocketOwner(pid int, path, address string, done <-chan struct{}) (socketOwner, error) {
	return newProcSocketOwner("/proc", pid, path, address, done)
}

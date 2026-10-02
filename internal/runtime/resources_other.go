//go:build !linux

package runtime

func processRSS(pid int) (int64, bool) { return 0, false }
func checkRunSpace(opts Options) error { return nil }

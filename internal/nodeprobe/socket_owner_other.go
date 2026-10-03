//go:build !linux

package nodeprobe

// Without kernel socket ownership proof, never send private proxy credentials.
func newCoreSocketOwner(int, string, string, <-chan struct{}) (socketOwner, error) {
	return nil, ErrUnavailable
}

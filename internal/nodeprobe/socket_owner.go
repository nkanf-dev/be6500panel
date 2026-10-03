package nodeprobe

import (
	"context"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"net"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"
)

type socketOwner interface {
	VerifyListener() error
	VerifyConnection(net.Conn) error
}

var coreSocketOwnerFactory = newCoreSocketOwner

type procSocketOwner struct {
	procRoot   string
	pid        int
	address    string
	startTime  string
	executable os.FileInfo
	done       <-chan struct{}
}

func newProcSocketOwner(procRoot string, pid int, path, address string, done <-chan struct{}) (socketOwner, error) {
	executable, err := os.Stat(path)
	if err != nil || !executable.Mode().IsRegular() {
		return nil, ErrUnavailable
	}
	owner := &procSocketOwner{procRoot: procRoot, pid: pid, address: address, executable: executable, done: done}
	stat, err := owner.readStartTime()
	if err != nil {
		return nil, ErrUnavailable
	}
	owner.startTime = stat
	if owner.identity() != nil {
		return nil, ErrUnavailable
	}
	return owner, nil
}
func (o *procSocketOwner) path(parts ...string) string {
	return filepath.Join(append([]string{o.procRoot, strconv.Itoa(o.pid)}, parts...)...)
}
func (o *procSocketOwner) readStartTime() (string, error) {
	data, err := boundedProcRead(o.path("stat"), 4096)
	if err != nil {
		return "", ErrUnavailable
	}
	// comm can contain spaces and parentheses; fields after its final ')' begin
	// at state(field3). starttime(field22) is index19 in that suffix.
	end := strings.LastIndex(string(data), ")")
	if end < 0 {
		return "", ErrUnavailable
	}
	fields := strings.Fields(string(data)[end+1:])
	if len(fields) < 20 {
		return "", ErrUnavailable
	}
	return fields[19], nil
}
func (o *procSocketOwner) identity() error {
	select {
	case <-o.done:
		return ErrUnavailable
	default:
	}
	start, err := o.readStartTime()
	if err != nil || start != o.startTime {
		return ErrUnavailable
	}
	executable, err := os.Stat(o.path("exe"))
	if err != nil || !os.SameFile(executable, o.executable) {
		return ErrUnavailable
	}
	select {
	case <-o.done:
		return ErrUnavailable
	default:
	}
	return nil
}
func (o *procSocketOwner) verify(state, local, remote string) error {
	if o.identity() != nil {
		return ErrUnavailable
	}
	// Bound /proc fd traversal. The minimal ephemeral core should own very few
	// sockets; an unexpectedly large descriptor table is not probe evidence.
	directory, err := os.Open(o.path("fd"))
	if err != nil {
		return ErrUnavailable
	}
	entries, err := directory.ReadDir(1025)
	directory.Close()
	if err != nil && len(entries) == 0 {
		return ErrUnavailable
	}
	if len(entries) > 1024 {
		return ErrUnavailable
	}
	owned := map[string]bool{}
	for _, entry := range entries {
		value, readErr := os.Readlink(o.path("fd", entry.Name()))
		if readErr != nil {
			continue
		}
		if strings.HasPrefix(value, "socket:[") && strings.HasSuffix(value, "]") {
			owned[value[8:len(value)-1]] = true
		}
	}
	// Read this process's network namespace, not the panel's optional /proc/net.
	data, err := boundedProcRead(o.path("net", "tcp"), 1<<20)
	if err != nil || len(data) > 1<<20 {
		return ErrUnavailable
	}
	matched := false
	for _, line := range strings.Split(string(data), "\n") {
		fields := strings.Fields(line)
		if len(fields) < 10 || fields[3] != state || fields[1] != local || fields[2] != remote {
			continue
		}
		if owned[fields[9]] {
			matched = true
			break
		}
	}
	if !matched || o.identity() != nil {
		return ErrUnavailable
	}
	return nil
}
func procTCPAddress(address string) (string, error) {
	host, portText, err := net.SplitHostPort(address)
	if err != nil {
		return "", ErrUnavailable
	}
	ip := net.ParseIP(host).To4()
	port, err := strconv.ParseUint(portText, 10, 16)
	if ip == nil || err != nil || port == 0 {
		return "", ErrUnavailable
	}
	// Linux /proc/net/tcp reports IPv4 bytes in host little-endian order.
	return strings.ToUpper(hex.EncodeToString([]byte{ip[3], ip[2], ip[1], ip[0]})) + fmt.Sprintf(":%04X", port), nil
}
func (o *procSocketOwner) VerifyListener() error {
	local, err := procTCPAddress(o.address)
	if err != nil {
		return err
	}
	return o.verify("0A", local, "00000000:0000")
}
func (o *procSocketOwner) VerifyConnection(conn net.Conn) error {
	// For the child accepted socket, local is the proxy listener and remote is
	// this panel connection's ephemeral address. The client's own fd cannot prove
	// the server identity. No proxy authentication bytes have been written yet.
	local, err := procTCPAddress(conn.RemoteAddr().String())
	if err != nil {
		return err
	}
	remote, err := procTCPAddress(conn.LocalAddr().String())
	if err != nil {
		return err
	}
	return o.verify("01", local, remote)
}
func awaitOwnedConnection(ctx context.Context, owner socketOwner, conn net.Conn) error {
	ticker := time.NewTicker(5 * time.Millisecond)
	defer ticker.Stop()
	for {
		if owner.VerifyConnection(conn) == nil {
			return nil
		}
		select {
		case <-ctx.Done():
			return errors.New("owned probe socket unavailable")
		case <-ticker.C:
		}
	}
}

func boundedProcRead(path string, max int64) ([]byte, error) {
	file, err := os.Open(path)
	if err != nil {
		return nil, ErrUnavailable
	}
	defer file.Close()
	data, err := io.ReadAll(io.LimitReader(file, max+1))
	if err != nil || int64(len(data)) > max {
		return nil, ErrUnavailable
	}
	return data, nil
}

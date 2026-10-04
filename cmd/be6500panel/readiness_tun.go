package main

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/netip"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"time"

	managedruntime "be6500panel/internal/runtime"
)

var managedTUNName = regexp.MustCompile(`^b6p-[A-Za-z0-9_][A-Za-z0-9_-]{0,10}$`)

type tunReadinessTarget struct {
	name    string
	address netip.Prefix // Preserve the owned host, not just the masked network.
}

// This is the qualified 1.14.2 native shape, not legacy inet4_address or an
// auto-routing TUN. Reject unknown options instead of proving a different lane.
type tunReadinessInbound struct {
	Type          string   `json:"type"`
	Tag           string   `json:"tag"`
	InterfaceName string   `json:"interface_name"`
	Address       []string `json:"address"`
	MTU           int      `json:"mtu"`
	Stack         string   `json:"stack"`
	DNSMode       string   `json:"dns_mode"`
	AutoRoute     *bool    `json:"auto_route"`
	AutoRedirect  *bool    `json:"auto_redirect"`
	UDPTimeout    string   `json:"udp_timeout"`
	UDPNATMax     int      `json:"udp_nat_max"`
}

func nativeTUNReadinessTarget(raw []byte) (*tunReadinessTarget, error) {
	var config struct {
		Inbounds []json.RawMessage `json:"inbounds"`
		Route    struct {
			Rules []map[string]json.RawMessage `json:"rules"`
		} `json:"route"`
	}
	if err := json.Unmarshal(raw, &config); err != nil {
		return nil, errors.New("accepted listener configuration invalid")
	}
	var target *tunReadinessTarget
	tproxy := false
	for _, rawInbound := range config.Inbounds {
		var kind struct {
			Type string `json:"type"`
			Tag  string `json:"tag"`
		}
		if err := json.Unmarshal(rawInbound, &kind); err != nil {
			return nil, errors.New("accepted listener configuration invalid")
		}
		tproxy = tproxy || kind.Type == "tproxy"
		if kind.Type != "tun" && kind.Tag != "tun-in" {
			continue
		}
		var options map[string]json.RawMessage
		if err := json.Unmarshal(rawInbound, &options); err != nil || len(options) != 11 {
			return nil, errors.New("accepted managed TUN configuration invalid")
		}
		for _, key := range []string{"type", "tag", "interface_name", "address", "mtu", "stack", "dns_mode", "auto_route", "auto_redirect", "udp_timeout", "udp_nat_max"} {
			if _, exists := options[key]; !exists {
				return nil, errors.New("accepted managed TUN configuration invalid")
			}
		}
		var inbound tunReadinessInbound
		decoder := json.NewDecoder(bytes.NewReader(rawInbound))
		decoder.DisallowUnknownFields()
		if err := decoder.Decode(&inbound); err != nil || target != nil ||
			inbound.Type != "tun" || inbound.Tag != "tun-in" || !managedTUNName.MatchString(inbound.InterfaceName) ||
			inbound.MTU != 1500 || inbound.Stack != "system" || inbound.DNSMode != "disabled" ||
			inbound.AutoRoute == nil || *inbound.AutoRoute || inbound.AutoRedirect == nil || *inbound.AutoRedirect ||
			inbound.UDPTimeout != "2m" || inbound.UDPNATMax != 1024 || len(inbound.Address) != 1 {
			return nil, errors.New("accepted managed TUN configuration invalid")
		}
		prefix, err := netip.ParsePrefix(inbound.Address[0])
		if err != nil || !prefix.Addr().Is4() || !prefix.Addr().IsPrivate() || prefix.Bits() != 30 ||
			prefix.Addr() != prefix.Masked().Addr().Next() || !prefix.Contains(prefix.Addr().Next()) ||
			prefix.String() != inbound.Address[0] {
			return nil, errors.New("accepted managed TUN address invalid")
		}
		target = &tunReadinessTarget{name: inbound.InterfaceName, address: prefix}
	}
	if target == nil {
		return nil, nil
	}
	if tproxy {
		return nil, errors.New("accepted managed TUN conflicts with TPROXY")
	}
	// The initial backend has no qualified routed IPv6 path. Require the
	// compiler's unconditional direct-family rule; never infer it from IPv4.
	directIPv6 := false
	for _, rule := range config.Route.Rules {
		var version int
		if err := json.Unmarshal(rule["ip_version"], &version); err != nil || version != 6 {
			continue
		}
		var outbound string
		if err := json.Unmarshal(rule["outbound"], &outbound); err != nil || outbound != "direct" || len(rule) != 2 {
			return nil, errors.New("accepted managed TUN requires direct IPv6")
		}
		directIPv6 = true
	}
	if !directIPv6 {
		return nil, errors.New("accepted managed TUN requires direct IPv6")
	}
	return target, nil
}

type tunReadinessInterface struct {
	name      string
	up        bool
	mtu       int
	addresses []netip.Prefix
}

// Every native-host dependency is explicit so tests never need Linux, a TUN,
// a core process or sockets. Production uses read-only proc/net/interface I/O.
type tunReadinessIO struct {
	readFile       func(string) ([]byte, error)
	readDir        func(string) ([]os.DirEntry, error)
	readlink       func(string) (string, error)
	stat           func(string) (os.FileInfo, error)
	lstat          func(string) (os.FileInfo, error)
	interfaces     func() ([]tunReadinessInterface, error)
	ipv4Routes     func(context.Context) ([]byte, error)
	probeListeners func(context.Context, []byte) error
}

func nativeTUNReadinessIO() tunReadinessIO {
	return tunReadinessIO{
		readFile: readTUNReadinessFile, readDir: os.ReadDir, readlink: os.Readlink,
		stat: os.Stat, lstat: os.Lstat, interfaces: observeTUNReadinessInterfaces,
		ipv4Routes: observeTUNPreStartRoutes, probeListeners: checkNativeReadiness,
	}
}

func readTUNReadinessFile(path string) ([]byte, error) {
	file, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer file.Close()
	const limit = 4 << 20
	raw, err := io.ReadAll(io.LimitReader(file, limit+1))
	if err != nil || len(raw) > limit {
		return nil, errors.New("TUN observation unreadable or exceeds limit")
	}
	return raw, nil
}

func observeTUNReadinessInterfaces() ([]tunReadinessInterface, error) {
	interfaces, err := net.Interfaces()
	if err != nil {
		return nil, err
	}
	result := make([]tunReadinessInterface, 0, len(interfaces))
	for _, iface := range interfaces {
		addresses, err := iface.Addrs()
		if err != nil {
			return nil, err
		}
		observed := tunReadinessInterface{name: iface.Name, up: iface.Flags&net.FlagUp != 0, mtu: iface.MTU}
		for _, address := range addresses {
			prefix, err := netip.ParsePrefix(address.String())
			if err != nil {
				return nil, err
			}
			observed.addresses = append(observed.addresses, prefix)
		}
		result = append(result, observed)
	}
	return result, nil
}

type tunReadinessIdentity struct {
	pid        int
	generation uint64
	start      uint64
	exe        string
	exeInfo    os.FileInfo
	dirInfo    os.FileInfo
}

// checkOwnedNativeReadiness is the read-only gate used by startup and capture
// restoration. The caller binds status to the generation of raw. A TPROXY
// config retains the existing listener-only readiness behavior.
func checkOwnedNativeReadiness(ctx context.Context, raw []byte, status func() (managedruntime.Status, error)) error {
	return checkOwnedNativeReadinessWithIO(ctx, raw, status, nativeTUNReadinessIO())
}

func checkOwnedNativeReadinessWithIO(ctx context.Context, raw []byte, status func() (managedruntime.Status, error), observe tunReadinessIO) error {
	target, err := nativeTUNReadinessTarget(raw)
	if err != nil {
		return err
	}
	if target == nil {
		return observe.probeListeners(ctx, raw)
	}
	if err := ctx.Err(); err != nil {
		return fmt.Errorf("owned TUN readiness: %w", err)
	}
	initial, err := tunReadinessStatus(status)
	if err != nil {
		return err
	}
	identity, err := observeTUNReadinessIdentity(initial, observe)
	if err != nil {
		return err
	}
	ticker := time.NewTicker(readinessRetryInterval)
	defer ticker.Stop()
	var unavailable error
	for {
		if err := ctx.Err(); err != nil {
			return fmt.Errorf("owned TUN readiness: %w", errors.Join(err, unavailable))
		}
		if err := identity.unchanged(status, observe); err != nil {
			return err
		}
		unavailable = observeOwnedTUN(*target, identity.pid, observe)
		if unavailable == nil {
			if err := observe.probeListeners(ctx, raw); err != nil {
				return err
			}
			// Listener/DNS probes may wait. Re-observe the TUN and owner after
			// them; a disappeared interface or replaced PID cannot pass ready.
			unavailable = observeOwnedTUN(*target, identity.pid, observe)
			if err := identity.unchanged(status, observe); err != nil {
				return err
			}
			if unavailable == nil {
				return ctx.Err()
			}
		}
		select {
		case <-ctx.Done():
			return fmt.Errorf("owned TUN readiness: %w", errors.Join(ctx.Err(), unavailable))
		case <-ticker.C:
		}
	}
}

func tunReadinessStatus(get func() (managedruntime.Status, error)) (managedruntime.Status, error) {
	if get == nil {
		return managedruntime.Status{}, errors.New("managed TUN process unavailable")
	}
	status, err := get()
	if err != nil || status.Service != managedruntime.SingBox || status.PID <= 0 ||
		(status.State != managedruntime.Starting && status.State != managedruntime.Running) ||
		!status.Configured || !status.ArtifactAvailable {
		return managedruntime.Status{}, errors.New("managed TUN process unavailable")
	}
	// NeedsRecovery can be true while Starting: the restore hook has not yet
	// restored caller resources. Running-only gating would deadlock first start.
	return status, nil
}

func observeTUNReadinessIdentity(status managedruntime.Status, observe tunReadinessIO) (tunReadinessIdentity, error) {
	identity := tunReadinessIdentity{pid: status.PID, generation: status.Generation}
	base := filepath.Join("/proc", strconv.Itoa(status.PID))
	raw, err := observe.readFile(filepath.Join(base, "stat"))
	if err != nil {
		return identity, errors.New("managed TUN process identity unavailable")
	}
	identity.start, err = tunProcessStart(raw, status.PID)
	if err != nil {
		return identity, err
	}
	identity.exe, err = observe.readlink(filepath.Join(base, "exe"))
	if err != nil || !filepath.IsAbs(identity.exe) || filepath.Clean(identity.exe) != identity.exe ||
		!strings.HasPrefix(filepath.Base(identity.exe), ".artifact-") || strings.HasSuffix(identity.exe, " (deleted)") ||
		strings.ContainsAny(identity.exe, "\r\n\x00") {
		return identity, errors.New("managed TUN executable identity unavailable")
	}
	identity.exeInfo, err = observe.lstat(identity.exe)
	if err != nil || !identity.exeInfo.Mode().IsRegular() || identity.exeInfo.Size() <= 0 || identity.exeInfo.Mode().Perm() != 0700 {
		return identity, errors.New("managed TUN executable identity unavailable")
	}
	opened, err := observe.stat(filepath.Join(base, "exe"))
	if err != nil || !sameTUNReadinessFile(identity.exeInfo, opened) {
		return identity, errors.New("managed TUN executable identity unavailable")
	}
	identity.dirInfo, err = observe.lstat(filepath.Dir(identity.exe))
	if err != nil || !identity.dirInfo.IsDir() || identity.dirInfo.Mode().Perm() != 0700 {
		return identity, errors.New("managed TUN executable identity unavailable")
	}
	// No repeated full-artifact hash. Trust manager's retained process object,
	// then bind its private executable inode and starttime across observation.
	after, err := observe.readFile(filepath.Join(base, "stat"))
	start, parseErr := tunProcessStart(after, status.PID)
	if err != nil || parseErr != nil || start != identity.start {
		return identity, errors.New("managed TUN process identity changed")
	}
	return identity, nil
}

func sameTUNReadinessFile(before, after os.FileInfo) bool {
	return before != nil && after != nil && os.SameFile(before, after) && before.Mode() == after.Mode() &&
		before.Size() == after.Size() && before.ModTime().Equal(after.ModTime())
}

func (identity tunReadinessIdentity) unchanged(status func() (managedruntime.Status, error), observe tunReadinessIO) error {
	current, err := tunReadinessStatus(status)
	if err != nil || current.PID != identity.pid || current.Generation != identity.generation {
		return errors.New("managed TUN process identity changed")
	}
	after, err := observeTUNReadinessIdentity(current, observe)
	if err != nil || after.start != identity.start || after.exe != identity.exe ||
		!sameTUNReadinessFile(identity.exeInfo, after.exeInfo) || !os.SameFile(identity.dirInfo, after.dirInfo) {
		return errors.New("managed TUN process identity changed")
	}
	return nil
}

// /proc/PID/stat field 22 is starttime. comm can contain spaces and ')', so
// field splitting starts only after its final closing parenthesis.
func tunProcessStart(raw []byte, pid int) (uint64, error) {
	text := strings.TrimSpace(string(raw))
	open, close := strings.IndexByte(text, '('), strings.LastIndexByte(text, ')')
	if open < 1 || close <= open || strings.TrimSpace(text[:open]) != strconv.Itoa(pid) {
		return 0, errors.New("managed TUN process identity unavailable")
	}
	fields := strings.Fields(text[close+1:])
	if len(fields) < 20 || len(fields[0]) != 1 || fields[0] == "Z" || fields[0] == "X" || fields[0] == "x" {
		return 0, errors.New("managed TUN process identity unavailable")
	}
	start, err := strconv.ParseUint(fields[19], 10, 64)
	if err != nil || start == 0 {
		return 0, errors.New("managed TUN process identity unavailable")
	}
	return start, nil
}

func observeOwnedTUN(target tunReadinessTarget, pid int, observe tunReadinessIO) error {
	interfaces, err := observe.interfaces()
	if err != nil {
		return errors.New("managed TUN interface unavailable")
	}
	found := false
	for _, iface := range interfaces {
		if iface.name != target.name {
			continue
		}
		if found || !iface.up || iface.mtu != 1500 {
			return errors.New("managed TUN interface address or link state invalid")
		}
		ownedAddress := false
		for _, address := range iface.addresses {
			if address.Addr().Is6() && address.Addr().IsLinkLocalUnicast() {
				continue // Kernel link metadata is not routed IPv6 qualification.
			}
			if ownedAddress || address != target.address {
				return errors.New("managed TUN interface address or link state invalid")
			}
			ownedAddress = true
		}
		if !ownedAddress {
			return errors.New("managed TUN interface address or link state invalid")
		}
		found = true
	}
	if !found {
		return errors.New("managed TUN interface unavailable")
	}
	rpf, err := observe.readFile(filepath.Join("/proc/sys/net/ipv4/conf", target.name, "rp_filter"))
	if err != nil || strings.TrimSpace(string(rpf)) != "2" {
		return errors.New("managed TUN rp_filter is not loose")
	}
	owned, err := observeTUNProcessFDs(pid, target.name, observe)
	if err != nil {
		return err
	}
	tcp, err := observe.readFile("/proc/net/tcp")
	if err != nil {
		return errors.New("managed TUN private TCP listener unavailable")
	}
	inode, err := tunPrivateTCPListener(tcp, target.address.Addr())
	if err != nil {
		return err
	}
	if !owned[inode] {
		return errors.New("managed TUN private TCP listener is not owned")
	}
	return nil
}

func observeTUNProcessFDs(pid int, name string, observe tunReadinessIO) (map[uint64]bool, error) {
	base := filepath.Join("/proc", strconv.Itoa(pid))
	entries, err := observe.readDir(filepath.Join(base, "fd"))
	if err != nil || len(entries) > 8192 {
		return nil, errors.New("managed TUN file descriptors unavailable")
	}
	// Linux 5.4 tun fdinfo reports iff: <interface>. Firmware support is not
	// assumed from interface existence: missing/unreadable iff fails closed.
	ownedTUN := false
	sockets := make(map[uint64]bool)
	for _, entry := range entries {
		fd, err := strconv.ParseUint(entry.Name(), 10, 32)
		if err != nil || strconv.FormatUint(fd, 10) != entry.Name() {
			continue
		}
		link, err := observe.readlink(filepath.Join(base, "fd", entry.Name()))
		if os.IsNotExist(err) {
			continue // A closed unrelated fd is not ownership evidence.
		}
		if err != nil {
			return nil, errors.New("managed TUN file descriptors unavailable")
		}
		if strings.HasPrefix(link, "socket:[") && strings.HasSuffix(link, "]") {
			inode, err := strconv.ParseUint(link[len("socket:["):len(link)-1], 10, 64)
			if err == nil && inode > 0 {
				sockets[inode] = true
			}
		}
		info, err := observe.readFile(filepath.Join(base, "fdinfo", entry.Name()))
		if os.IsNotExist(err) {
			continue
		}
		if err != nil {
			return nil, errors.New("managed TUN fdinfo unavailable")
		}
		for _, line := range strings.Split(string(info), "\n") {
			key, value, ok := strings.Cut(line, ":")
			if ok && key == "iff" && strings.TrimSpace(value) == name {
				ownedTUN = true
			}
		}
	}
	if !ownedTUN {
		return nil, errors.New("managed TUN interface fd ownership unavailable")
	}
	return sockets, nil
}

func tunPrivateTCPListener(raw []byte, address netip.Addr) (uint64, error) {
	var inode uint64
	count := 0
	lines := strings.Split(strings.TrimSpace(string(raw)), "\n")
	if len(lines) == 0 || !strings.Contains(lines[0], "local_address") || !strings.Contains(lines[0], "inode") {
		return 0, errors.New("managed TUN private TCP observation invalid")
	}
	for _, line := range lines[1:] {
		fields := strings.Fields(line)
		if len(fields) < 10 {
			return 0, errors.New("managed TUN private TCP observation invalid")
		}
		if fields[3] != "0A" { // TCP_LISTEN, not an established tuple.
			continue
		}
		hexAddress, hexPort, ok := strings.Cut(fields[1], ":")
		ip, ipErr := strconv.ParseUint(hexAddress, 16, 32)
		port, portErr := strconv.ParseUint(hexPort, 16, 16)
		if !ok || len(hexAddress) != 8 || len(hexPort) != 4 || ipErr != nil || portErr != nil || port == 0 {
			return 0, errors.New("managed TUN private TCP observation invalid")
		}
		local := netip.AddrFrom4([4]byte{byte(ip), byte(ip >> 8), byte(ip >> 16), byte(ip >> 24)})
		if local != address || port == 53 {
			// Exact 1.14.2 creates a same-address DNS53 TCP listener even in
			// disabled DNS mode. It is not the system stack handoff socket.
			continue
		}
		inode, ipErr = strconv.ParseUint(fields[9], 10, 64)
		if ipErr != nil || inode == 0 {
			return 0, errors.New("managed TUN private TCP observation invalid")
		}
		count++
	}
	if count != 1 {
		return 0, errors.New("managed TUN private TCP listener is not unique")
	}
	return inode, nil
}

// checkNativeTUNPreStart refuses local prefix/name collisions before the core
// can create its connected route. It does not create an interface, change a
// route or repair a sysctl. Legacy native configurations do no host inspection.
func checkNativeTUNPreStart(ctx context.Context, raw []byte) error {
	return checkNativeTUNPreStartWithIO(ctx, raw, nativeTUNReadinessIO())
}

func checkNativeTUNPreStartWithIO(ctx context.Context, raw []byte, observe tunReadinessIO) error {
	target, err := nativeTUNReadinessTarget(raw)
	if err != nil || target == nil {
		return err
	}
	if err := ctx.Err(); err != nil {
		return fmt.Errorf("managed TUN pre-start: %w", err)
	}
	interfaces, err := observe.interfaces()
	if err != nil {
		return errors.New("managed TUN pre-start interfaces unavailable")
	}
	for _, iface := range interfaces {
		if iface.name == target.name {
			return errors.New("managed TUN interface already occupied")
		}
		for _, address := range iface.addresses {
			if !address.IsValid() || address.Addr().Is4In6() {
				return errors.New("managed TUN pre-start interface address invalid")
			}
			if address.Addr().Is4() && tunReadinessPrefixesOverlap(target.address, address) {
				return errors.New("managed TUN prefix conflicts with local interface")
			}
		}
	}
	rawRoutes, err := observe.ipv4Routes(ctx)
	if err != nil {
		return errors.New("managed TUN pre-start routes unavailable")
	}
	prefixes, err := tunPreStartRoutePrefixes(rawRoutes)
	if err != nil {
		return err
	}
	for _, prefix := range prefixes {
		if tunReadinessPrefixesOverlap(target.address, prefix) {
			return errors.New("managed TUN prefix conflicts with existing route")
		}
	}
	return ctx.Err()
}

func tunReadinessPrefixesOverlap(first, second netip.Prefix) bool {
	return first.Masked().Contains(second.Masked().Addr()) || second.Masked().Contains(first.Masked().Addr())
}

func tunPreStartRoutePrefixes(raw []byte) ([]netip.Prefix, error) {
	var prefixes []netip.Prefix
	for _, line := range strings.Split(string(raw), "\n") {
		fields := strings.Fields(line)
		if len(fields) == 0 {
			continue
		}
		index := 0
		switch fields[0] {
		case "unicast", "local", "broadcast", "multicast", "anycast", "throw", "unreachable", "prohibit", "blackhole", "nat":
			index++
		}
		if index == len(fields) {
			return nil, errors.New("managed TUN pre-start route observation invalid")
		}
		value := fields[index]
		if value == "default" {
			continue // The ordinary WAN default is not a private-prefix collision.
		}
		prefix, err := netip.ParsePrefix(value)
		if err != nil {
			address, addressErr := netip.ParseAddr(value)
			if addressErr != nil || !address.Is4() {
				return nil, errors.New("managed TUN pre-start route observation invalid")
			}
			prefix = netip.PrefixFrom(address, 32)
		}
		if !prefix.Addr().Is4() || prefix != prefix.Masked() {
			return nil, errors.New("managed TUN pre-start route observation invalid")
		}
		if prefix.Bits() != 0 {
			prefixes = append(prefixes, prefix)
		}
	}
	return prefixes, nil
}

// /proc/net/route covers only the main table. Query all IPv4 tables through
// fixed read-only ip arguments, including named policy tables and local routes.
// The output and execution lifetime are bounded; stderr is never exposed.
func observeTUNPreStartRoutes(ctx context.Context) ([]byte, error) {
	ctx, cancel := context.WithTimeout(ctx, 2*time.Second)
	defer cancel()
	cmd := exec.CommandContext(ctx, "ip", "-4", "route", "show", "table", "all")
	cmd.WaitDelay = listenerProbeTimeout
	output := &tunReadinessBoundedOutput{}
	cmd.Stdout = output
	if err := cmd.Run(); err != nil {
		return nil, errors.New("managed TUN pre-start routes unavailable")
	}
	return output.Bytes(), ctx.Err()
}

type tunReadinessBoundedOutput struct{ buffer bytes.Buffer }

func (output *tunReadinessBoundedOutput) Bytes() []byte { return output.buffer.Bytes() }

func (output *tunReadinessBoundedOutput) Write(raw []byte) (int, error) {
	if len(raw) > (4<<20)-output.buffer.Len() {
		return 0, errors.New("managed TUN route observation exceeds limit")
	}
	return output.buffer.Write(raw)
}

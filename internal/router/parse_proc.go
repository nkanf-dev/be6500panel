package router

import (
	"encoding/hex"
	"net/netip"
	"sort"
	"strconv"
	"strings"
)

func parseNetDev(data []byte) ([]Traffic, bool) {
	out := []Traffic{}
	lines := strings.Split(strings.TrimSpace(string(data)), "\n")
	if len(lines) < 2 || !strings.Contains(lines[0], "Inter-") || !strings.Contains(lines[1], "bytes") {
		return out, true
	}
	bad := false
	seen := map[string]bool{}
	for _, line := range lines[2:] {
		if strings.TrimSpace(line) == "" {
			continue
		}
		i := strings.LastIndexByte(line, ':')
		if i < 1 {
			bad = true
			continue
		}
		name := strings.TrimSpace(line[:i])
		f := strings.Fields(line[i+1:])
		if len(f) != 16 || !validInterface(name) || seen[name] {
			bad = true
			continue
		}
		var values [16]uint64
		ok := true
		for j := range f {
			var err error
			values[j], err = strconv.ParseUint(f[j], 10, 64)
			if err != nil {
				ok = false
			}
		}
		if !ok {
			bad = true
			continue
		}
		seen[name] = true
		out = append(out, Traffic{Interface: name, RXBytes: values[0], TXBytes: values[8]})
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Interface < out[j].Interface })
	return out, bad
}

func validInterface(s string) bool {
	if len(s) == 0 || len(s) > 64 {
		return false
	}
	for _, c := range s {
		if c <= 32 || c == 127 || c == '/' {
			return false
		}
	}
	return true
}

func ipv4Hex(s string) (netip.Addr, bool) {
	if len(s) != 8 {
		return netip.Addr{}, false
	}
	b, err := hex.DecodeString(s)
	if err != nil {
		return netip.Addr{}, false
	}
	// RN02 /proc/net/route uses native little-endian IPv4 words.
	return netip.AddrFrom4([4]byte{b[3], b[2], b[1], b[0]}), true
}

func parseIPv4Routes(data []byte) ([]Route, bool) {
	out := []Route{}
	lines := strings.Split(strings.TrimSpace(string(data)), "\n")
	if len(lines) == 0 {
		return out, true
	}
	header := strings.Fields(lines[0])
	if len(header) != 11 || header[0] != "Iface" || header[1] != "Destination" || header[7] != "Mask" {
		return out, true
	}
	bad := false
	for _, line := range lines[1:] {
		f := strings.Fields(line)
		if len(f) == 0 {
			continue
		}
		if len(f) != 11 || !validInterface(f[0]) {
			bad = true
			continue
		}
		dst, ok1 := ipv4Hex(f[1])
		gw, ok2 := ipv4Hex(f[2])
		mask, ok3 := ipv4Hex(f[7])
		flags, e1 := strconv.ParseUint(f[3], 16, 32)
		metric, e2 := strconv.ParseUint(f[6], 10, 64)
		validNumbers := true
		for _, index := range []int{4, 5, 8, 9, 10} {
			if _, err := strconv.ParseUint(f[index], 10, 64); err != nil {
				validNumbers = false
			}
		}
		if !ok1 || !ok2 || !ok3 || e1 != nil || e2 != nil || !validNumbers {
			bad = true
			continue
		}
		bits, ok := maskBits(mask.As4())
		if !ok {
			bad = true
			continue
		}
		if flags&1 == 0 || flags&0x200 != 0 {
			continue
		} // Down/reject routes are not forwarding routes.
		out = append(out, Route{Family: "ipv4", Destination: netip.PrefixFrom(dst, bits).Masked().String(), Gateway: gw.String(), Interface: f[0], Metric: metric})
	}
	return out, bad
}

func maskBits(mask [4]byte) (int, bool) {
	n := 0
	zero := false
	for _, b := range mask {
		for bit := 7; bit >= 0; bit-- {
			if b&(1<<bit) != 0 {
				if zero {
					return 0, false
				}
				n++
			} else {
				zero = true
			}
		}
	}
	return n, true
}

func ipv6Hex(s string) (netip.Addr, bool) {
	if len(s) != 32 {
		return netip.Addr{}, false
	}
	b, err := hex.DecodeString(s)
	if err != nil {
		return netip.Addr{}, false
	}
	var a [16]byte
	copy(a[:], b)
	return netip.AddrFrom16(a), true
}

func parseIPv6Routes(data []byte) ([]Route, bool) {
	out := []Route{}
	bad := false
	for _, line := range strings.Split(string(data), "\n") {
		f := strings.Fields(line)
		if len(f) == 0 {
			continue
		}
		if len(f) != 10 || !validInterface(f[9]) {
			bad = true
			continue
		}
		dst, ok1 := ipv6Hex(f[0])
		_, ok2 := ipv6Hex(f[2])
		gw, ok3 := ipv6Hex(f[4])
		bits, e1 := strconv.ParseUint(f[1], 16, 8)
		srcbits, e2 := strconv.ParseUint(f[3], 16, 8)
		metric, e3 := strconv.ParseUint(f[5], 16, 64)
		flags, e4 := strconv.ParseUint(f[8], 16, 32)
		_, e5 := strconv.ParseUint(f[6], 16, 64)
		_, e6 := strconv.ParseUint(f[7], 16, 64)
		if !ok1 || !ok2 || !ok3 || e1 != nil || e2 != nil || e3 != nil || e4 != nil || e5 != nil || e6 != nil || bits > 128 || srcbits > 128 {
			bad = true
			continue
		}
		if flags&1 == 0 || flags&0x200 != 0 {
			continue
		}
		out = append(out, Route{Family: "ipv6", Destination: netip.PrefixFrom(dst, int(bits)).Masked().String(), Gateway: gw.String(), Interface: f[9], Metric: metric})
	}
	return out, bad
}

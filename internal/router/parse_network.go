package router

import (
	"net"
	"net/netip"
	"sort"
	"strconv"
	"strings"
	"time"
)

func normalizedMAC(s string) (string, bool) {
	mac, err := net.ParseMAC(s)
	if err != nil || len(mac) != 6 || mac[0]&1 != 0 {
		return "", false
	}
	nonzero := false
	for _, b := range mac {
		if b != 0 {
			nonzero = true
		}
	}
	return mac.String(), nonzero
}

func parseLeases(data []byte, now time.Time) ([]Device, bool) {
	out := []Device{}
	bad := false
	seen := map[string]bool{}
	for _, line := range strings.Split(string(data), "\n") {
		f := strings.Fields(line)
		if len(f) == 0 {
			continue
		}
		if len(f) != 5 {
			bad = true
			continue
		}
		expiry, err := strconv.ParseInt(f[0], 10, 64)
		mac, ok := normalizedMAC(f[1])
		ip, e2 := netip.ParseAddr(f[2])
		if err != nil || expiry < 0 || !ok || e2 != nil || !ip.Is4() || ip.IsUnspecified() || ip.IsMulticast() || !safeString(f[3], 253) {
			bad = true
			continue
		}
		if expiry != 0 && expiry <= now.Unix() {
			continue
		}
		d := Device{IP: ip.String(), MAC: mac, Hostname: f[3], Lease: true}
		if d.Hostname == "*" {
			d.Hostname = ""
		}
		if expiry != 0 {
			t := time.Unix(expiry, 0).UTC()
			if t.Year() > 9999 {
				bad = true
				continue
			}
			d.ExpiresAt = &t
		}
		key := d.IP + "/" + d.MAC
		if seen[key] {
			bad = true
			continue
		}
		seen[key] = true
		out = append(out, d)
	}
	return out, bad
}

func parseARP(data []byte) ([]Device, bool) {
	out := []Device{}
	lines := strings.Split(strings.TrimSpace(string(data)), "\n")
	if len(lines) == 0 || !strings.HasPrefix(lines[0], "IP address") || !strings.Contains(lines[0], "HW address") {
		return out, true
	}
	bad := false
	for _, line := range lines[1:] {
		f := strings.Fields(line)
		if len(f) == 0 {
			continue
		}
		if len(f) != 6 {
			bad = true
			continue
		}
		ip, e1 := netip.ParseAddr(f[0])
		_, e2 := strconv.ParseUint(strings.TrimPrefix(f[1], "0x"), 16, 32)
		flags, e3 := strconv.ParseUint(strings.TrimPrefix(f[2], "0x"), 16, 32)
		if e1 != nil || !ip.Is4() || ip.IsUnspecified() || ip.IsMulticast() || e2 != nil || e3 != nil || !validInterface(f[5]) {
			bad = true
			continue
		}
		if flags&2 == 0 {
			continue
		}
		mac, ok := normalizedMAC(f[3])
		if !ok {
			bad = true
			continue
		}
		out = append(out, Device{IP: ip.String(), MAC: mac, Interface: f[5], Online: true})
	}
	return out, bad
}

func mergeDevices(leases, arp []Device) []Device {
	out := append([]Device{}, leases...)
	index := map[string]int{}
	leasedMAC := map[string]bool{}
	for i, d := range out {
		index[d.IP+"/"+d.MAC] = i
		leasedMAC[d.MAC] = true
	}
	for _, d := range arp {
		key := d.IP + "/" + d.MAC
		if i, ok := index[key]; ok {
			out[i].Online = true
			out[i].Interface = d.Interface
		} else if !leasedMAC[d.MAC] {
			index[key] = len(out)
			out = append(out, d)
		}
	}
	sort.Slice(out, func(i, j int) bool {
		a, _ := netip.ParseAddr(out[i].IP)
		b, _ := netip.ParseAddr(out[j].IP)
		if a == b {
			return out[i].MAC < out[j].MAC
		}
		return a.Less(b)
	})
	return out
}

func parseResolvers(data []byte) ([]string, bool) {
	out := []string{}
	bad := false
	seen := map[string]bool{}
	for _, line := range strings.Split(string(data), "\n") {
		line, _, _ = strings.Cut(line, "#")
		line, _, _ = strings.Cut(line, ";")
		f := strings.Fields(line)
		if len(f) == 0 || f[0] != "nameserver" {
			continue
		}
		if len(f) != 2 {
			bad = true
			continue
		}
		ip, err := netip.ParseAddr(f[1])
		if err != nil || ip.IsUnspecified() || ip.IsMulticast() {
			bad = true
			continue
		}
		value := ip.String()
		if !seen[value] {
			out = append(out, value)
			seen[value] = true
		}
	}
	return out, bad
}

func parseFirewall(data []byte) (FirewallFamily, bool) {
	out := FirewallFamily{}
	bad := false
	table := ""
	filter := false
	committed := false
	for _, line := range strings.Split(string(data), "\n") {
		line = strings.TrimSpace(line)
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		if strings.HasPrefix(line, "*") {
			if table != "" || len(line) < 2 || strings.ContainsAny(line, " \t") {
				bad = true
			}
			table = strings.TrimPrefix(line, "*")
			if table == "filter" {
				filter = true
			}
			continue
		}
		if line == "COMMIT" {
			if table == "" {
				bad = true
			}
			table = ""
			committed = true
			continue
		}
		if table == "" {
			bad = true
			continue
		}
		if strings.HasPrefix(line, ":") {
			f := strings.Fields(line)
			if len(f) != 3 || !strings.HasPrefix(f[2], "[") || !strings.HasSuffix(f[2], "]") {
				bad = true
				continue
			}
			if table != "filter" {
				continue
			}
			name := strings.TrimPrefix(f[0], ":")
			if name != "INPUT" && name != "FORWARD" && name != "OUTPUT" {
				continue
			}
			if f[1] != "ACCEPT" && f[1] != "DROP" {
				bad = true
				continue
			}
			switch name {
			case "INPUT":
				out.Input = f[1]
			case "FORWARD":
				out.Forward = f[1]
			case "OUTPUT":
				out.Output = f[1]
			}
		} else if strings.HasPrefix(line, "-A ") {
			if len(strings.Fields(line)) < 3 {
				bad = true
				continue
			}
			out.Rules++
		} else {
			bad = true
		}
	}
	if table != "" || !committed || !filter || out.Input == "" || out.Forward == "" || out.Output == "" {
		bad = true
	}
	return out, bad
}

package capture

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"net/netip"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strconv"
	"strings"

	"be6500panel/internal/proxy"
)

func readTableNames() (map[string]int, error) {
	names := map[string]int{"local": 255, "main": 254, "default": 253, "unspec": 0}
	paths := []string{"/etc/iproute2/rt_tables", "/usr/lib/iproute2/rt_tables"}
	for _, dir := range []string{"/etc/iproute2/rt_tables.d", "/usr/lib/iproute2/rt_tables.d"} {
		entries, err := os.ReadDir(dir)
		if err != nil && !os.IsNotExist(err) {
			return nil, err
		}
		for _, entry := range entries {
			if !entry.IsDir() && strings.HasSuffix(entry.Name(), ".conf") {
				paths = append(paths, filepath.Join(dir, entry.Name()))
			}
		}
	}
	for _, path := range paths {
		raw, err := os.ReadFile(path)
		if os.IsNotExist(err) {
			continue
		}
		if err != nil {
			return nil, err
		}
		for _, line := range strings.Split(string(raw), "\n") {
			line = strings.SplitN(line, "#", 2)[0]
			fields := strings.Fields(line)
			if len(fields) == 0 {
				continue
			}
			if len(fields) != 2 {
				return nil, fmt.Errorf("invalid route table alias in %s", path)
			}
			n, err := strconv.Atoi(fields[0])
			if err != nil || n < 0 {
				return nil, fmt.Errorf("invalid route table number in %s", path)
			}
			if old, ok := names[fields[1]]; ok && old != n {
				return nil, fmt.Errorf("ambiguous route table alias %q", fields[1])
			}
			names[fields[1]] = n
		}
	}
	return names, nil
}
func emptyFIB(out []byte, err error) bool {
	if err == nil || errors.Is(err, context.Canceled) || errors.Is(err, context.DeadlineExceeded) {
		return false
	}
	text := strings.TrimSpace(string(out))
	for _, family := range []string{"ipv4", "ipv6"} {
		diagnostic := "Error: " + family + ": FIB table does not exist."
		if text == diagnostic || text == diagnostic+"\nDump terminated" {
			return true
		}
	}
	return false
}

// showCaptureTable accepts the old ip-full's bare exit-2 dump diagnostic only
// after a successful all-table read proves the fixed owned table has no route.
// Other netlink/executable/context failures are never treated as empty tables.
func (c *Controller) showCaptureTable(ctx context.Context, family int, names map[string]int) ([]byte, error) {
	out, err := c.execute(ctx, routeShow(family))
	if err == nil {
		return out, nil
	}
	if emptyFIB(out, err) {
		return nil, nil
	}
	var exitErr *exec.ExitError
	if strings.TrimSpace(string(out)) != "Dump terminated" || errors.Is(err, context.Canceled) || errors.Is(err, context.DeadlineExceeded) || !errors.As(err, &exitErr) || exitErr.ExitCode() != 2 {
		return out, err
	}
	allArgs := []string{"ip", "-" + strconv.Itoa(family), "route", "show", "table", "all"}
	all, fallbackErr := c.execute(ctx, allArgs)
	if fallbackErr != nil {
		return out, errors.Join(err, fallbackErr)
	}
	if proofErr := captureTableAbsent(all, family, names); proofErr != nil {
		return out, errors.Join(err, proofErr)
	}
	return nil, nil
}
func captureTableAbsent(out []byte, family int, names map[string]int) error {
	// main-table routes omit `table`; non-main routes always print the numeric
	// ID or its rt_tables alias. Validate route starts so diagnostic text cannot
	// be mistaken for a successful empty dump.
	previousRoute := false
	for _, line := range strings.Split(string(out), "\n") {
		fields := strings.Fields(line)
		if len(fields) == 0 {
			continue
		}
		if fields[0] == "nexthop" && previousRoute {
			continue
		}
		destination := fields[0]
		switch destination {
		case "local", "broadcast", "unicast", "unreachable", "prohibit", "blackhole", "throw", "nat", "anycast", "multicast":
			if len(fields) < 2 {
				return errors.New("cannot verify complete route table dump")
			}
			destination = fields[1]
		}
		if destination != "default" {
			addr, e := netip.ParseAddr(destination)
			if e != nil {
				prefix, pe := netip.ParsePrefix(destination)
				if pe != nil {
					return errors.New("cannot verify complete route table dump")
				}
				addr = prefix.Addr()
			}
			if addr.Is4() != (family == 4) {
				return errors.New("unexpected family in route table dump")
			}
		}
		previousRoute = true
		for i, field := range fields {
			if field != "table" {
				continue
			}
			if i+1 >= len(fields) {
				return errors.New("cannot parse route table in all-table dump")
			}
			number, e := tableNumber(fields[i+1], names)
			if e != nil {
				return e
			}
			if number == proxy.CaptureTable {
				return errors.New("capture table occupied in all-table dump")
			}
		}
	}
	return nil
}
func (c *Controller) preflight(ctx context.Context, plan proxy.OwnedRulesPlan) error {
	names, err := c.tableNames()
	if err != nil {
		return fmt.Errorf("capture table alias preflight: %w", err)
	}
	for _, family := range plan.Ownership.RouteFamilies {
		out, err := c.showCaptureTable(ctx, family, names)
		if err != nil {
			return err
		}
		if len(bytes.TrimSpace(out)) != 0 {
			return errors.New("capture table occupied")
		}
		out, err = c.execute(ctx, ruleShow(family))
		if err != nil {
			return err
		}
		if err = ruleCollisions(out, names); err != nil {
			return err
		}
		out, err = c.execute(ctx, []string{ipTables(family), "-w", "5", "-t", "mangle", "-S"})
		if err != nil {
			return err
		}
		if err = markCollisions(out); err != nil {
			return err
		}
	}
	for _, chain := range plan.Ownership.Chains {
		args := []string{ipTables(chain.Family), "-w", "5", "-t", chain.Table, "-S", chain.Name}
		out, err := c.execute(ctx, args)
		if err == nil {
			return fmt.Errorf("capture chain occupied: %s", chain.Name)
		}
		if !resourceAbsent(args, out, err) {
			return err
		}
	}
	return nil
}
func ipTables(family int) string {
	if family == 6 {
		return "ip6tables"
	}
	return "iptables"
}
func routeShow(family int) []string {
	return []string{"ip", "-" + strconv.Itoa(family), "route", "show", "table", strconv.Itoa(proxy.CaptureTable)}
}
func ruleShow(family int) []string { return []string{"ip", "-" + strconv.Itoa(family), "rule", "show"} }
func tableNumber(raw string, names map[string]int) (int, error) {
	if n, err := strconv.Atoi(raw); err == nil && n >= 0 {
		return n, nil
	}
	if n, ok := names[raw]; ok {
		return n, nil
	}
	return 0, fmt.Errorf("unknown policy route table alias %q", raw)
}
func markMask(raw string) (value, mask uint32, err error) {
	pieces := strings.Split(raw, "/")
	if len(pieces) < 1 || len(pieces) > 2 {
		return 0, 0, errors.New("invalid mark/mask")
	}
	parse := func(s string) (uint32, error) {
		base := 10
		if strings.HasPrefix(s, "0x") || strings.HasPrefix(s, "0X") {
			base = 16
			s = s[2:]
		}
		n, e := strconv.ParseUint(s, base, 32)
		return uint32(n), e
	}
	value, err = parse(pieces[0])
	if err != nil {
		return 0, 0, err
	}
	mask = ^uint32(0)
	if len(pieces) == 2 {
		mask, err = parse(pieces[1])
	}
	return
}
func ruleCollisions(out []byte, names map[string]int) error {
	for _, line := range strings.Split(string(out), "\n") {
		fields := strings.Fields(line)
		if len(fields) == 0 {
			continue
		}
		priority, err := strconv.Atoi(strings.TrimSuffix(fields[0], ":"))
		if err != nil {
			return errors.New("cannot parse policy rule priority")
		}
		if priority == proxy.CapturePriority {
			return errors.New("capture rule priority occupied")
		}
		for i := 1; i < len(fields); i++ {
			switch fields[i] {
			case "lookup", "table":
				if i+1 >= len(fields) {
					return errors.New("cannot parse policy rule table")
				}
				table, err := tableNumber(fields[i+1], names)
				if err != nil {
					return err
				}
				if table == proxy.CaptureTable {
					return errors.New("capture table referenced by another policy rule")
				}
				i++
			case "fwmark":
				if i+1 >= len(fields) {
					return errors.New("cannot parse policy rule mark")
				}
				_, mask, err := markMask(fields[i+1])
				if err != nil {
					return fmt.Errorf("cannot parse policy rule mark: %w", err)
				}
				if mask&proxy.CaptureMask != 0 {
					return errors.New("capture mark overlaps another policy rule")
				}
				i++
			}
		}
	}
	return nil
}

// splitArgs reads iptables -S quoting. It never evaluates shell syntax.
func splitArgs(line string) ([]string, error) {
	var args []string
	var token strings.Builder
	quote := rune(0)
	escaped := false
	started := false
	for _, r := range line {
		if escaped {
			token.WriteRune(r)
			escaped = false
			started = true
			continue
		}
		if r == '\\' && quote != '\'' {
			escaped = true
			started = true
			continue
		}
		if quote != 0 {
			if r == quote {
				quote = 0
			} else {
				token.WriteRune(r)
			}
			started = true
			continue
		}
		if r == '\'' || r == '"' {
			quote = r
			started = true
			continue
		}
		if r == ' ' || r == '\t' {
			if started {
				args = append(args, token.String())
				token.Reset()
				started = false
			}
			continue
		}
		token.WriteRune(r)
		started = true
	}
	if escaped || quote != 0 {
		return nil, errors.New("invalid quoted iptables listing")
	}
	if started {
		args = append(args, token.String())
	}
	return args, nil
}
func option(args []string, key string) (string, bool, error) {
	for i, arg := range args {
		if arg == key {
			if i+1 >= len(args) {
				return "", false, fmt.Errorf("missing %s value", key)
			}
			return args[i+1], true, nil
		}
	}
	return "", false, nil
}
func markCollisions(out []byte) error {
	for _, line := range strings.Split(string(out), "\n") {
		args, err := splitArgs(line)
		if err != nil {
			return err
		}
		target, ok, err := option(args, "-j")
		if err != nil {
			return err
		}
		if !ok {
			target, ok, err = option(args, "-g")
			if err != nil {
				return err
			}
		}
		if !ok || (target != "MARK" && target != "CONNMARK" && target != "TPROXY") {
			continue
		}
		writes := uint32(0)
		recognized := false
		for _, key := range []string{"--set-xmark", "--set-mark", "--tproxy-mark", "--or-mark", "--xor-mark", "--and-mark"} {
			raw, ok, err := option(args, key)
			if err != nil {
				return err
			}
			if !ok {
				continue
			}
			value, mask, err := markMask(raw)
			if err != nil {
				return fmt.Errorf("cannot parse %s: %w", key, err)
			}
			recognized = true
			switch key {
			case "--or-mark", "--xor-mark":
				writes |= value
			case "--and-mark":
				writes |= ^value
			default:
				writes |= mask | value
			}
		}
		if target == "CONNMARK" && (slices.Contains(args, "--save-mark") || slices.Contains(args, "--restore-mark")) {
			recognized = true
			for _, key := range []string{"--nfmask", "--ctmask"} {
				raw, ok, err := option(args, key)
				if err != nil {
					return err
				}
				mask := ^uint32(0)
				if ok {
					var e error
					mask, _, e = markMask(raw)
					if e != nil {
						return e
					}
				}
				writes |= mask
			}
		}
		// A TPROXY with no --tproxy-mark does not change mark bits.
		if !recognized && target != "TPROXY" {
			return fmt.Errorf("unsupported %s mark operation in preflight", target)
		}
		if writes&proxy.CaptureMask != 0 {
			return fmt.Errorf("capture mark overlaps existing %s rule", target)
		}
	}
	return nil
}

// Reconcile checks the kernel, without creating or repairing any rule. A failed
// observation leaves the journal available for explicit Cleanup/recovery.
func (c *Controller) Reconcile(ctx context.Context) (Status, error) {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.plan == nil {
		return c.statusLocked(), nil
	}
	err := c.observeLocked(ctx)
	if err != nil {
		c.cleanupPending = true
		return c.statusLocked(), err
	}
	c.active = true
	c.cleanupPending = false
	return c.statusLocked(), nil
}
func (c *Controller) observeLocked(ctx context.Context) error {
	missing := false
	var pending error
	for _, chain := range c.plan.Ownership.Chains {
		args := []string{ipTables(chain.Family), "-w", "5", "-t", chain.Table, "-S", chain.Name}
		out, err := c.execute(ctx, args)
		if err != nil {
			if resourceAbsent(args, out, err) {
				missing = true
			} else {
				pending = errors.Join(pending, err)
			}
			continue
		}
		// Compare each exact generated rule with -C, allowing the kernel's -S
		// spelling to vary (for example IPv6 /128 and printed match modules).
		for _, cmd := range c.plan.Apply {
			if len(cmd) > 6 && cmd[0] == args[0] && cmd[4] == chain.Table && cmd[5] == "-A" && cmd[6] == chain.Name {
				check := slices.Clone(cmd)
				check[5] = "-C"
				out, err = c.execute(ctx, check)
				if err != nil {
					if resourceAbsent(check, out, err) {
						missing = true
					} else {
						pending = errors.Join(pending, err)
					}
				}
			}
		}
	}
	for _, cmd := range c.plan.Apply {
		if len(cmd) > 5 && cmd[5] == "-I" {
			check := slices.Clone(cmd)
			check[5] = "-C"
			check = append(check[:7], check[8:]...)
			out, err := c.execute(ctx, check)
			if err != nil {
				if resourceAbsent(check, out, err) {
					missing = true
				} else {
					pending = errors.Join(pending, err)
				}
			}
		}
	}
	names, err := c.tableNames()
	if err != nil {
		pending = errors.Join(pending, err)
	}
	for _, family := range c.plan.Ownership.RouteFamilies {
		out, err := c.showCaptureTable(ctx, family, names)
		if err != nil {
			pending = errors.Join(pending, err)
		} else if !hasLocalRoute(out, family) {
			missing = true
		}
		out, err = c.execute(ctx, ruleShow(family))
		if err != nil {
			pending = errors.Join(pending, err)
		} else if names != nil && !hasOwnedRule(out, c.plan.Ownership, family, names) {
			missing = true
		}
	}
	if missing {
		c.active = false
		pending = errors.Join(pending, errors.New("owned capture resources missing; cleanup and coordinated reapply required"))
	}
	return pending
}
func hasLocalRoute(out []byte, family int) bool {
	prefix := "0.0.0.0/0"
	if family == 6 {
		prefix = "::/0"
	}
	for _, line := range strings.Split(string(out), "\n") {
		f := strings.Fields(line)
		if len(f) >= 4 && f[0] == "local" && (f[1] == "default" || f[1] == prefix) && f[2] == "dev" && f[3] == "lo" {
			return true
		}
	}
	return false
}

// hasOwnedRule proves every exact client policy rule for a routed family.
// One observed source, duplicate lines or a broader prefix cannot stand in for
// another client. Unknown selectors cannot prove the compiled unrestricted rule.
func hasOwnedRule(out []byte, own proxy.RulesOwnership, family int, names map[string]int) bool {
	clients := own.ClientIPv4s
	singular := own.ClientIPv4
	if family == 6 {
		clients, singular = own.ClientIPv6s, own.ClientIPv6
	}
	remaining := make(map[string]bool, len(clients)+1)
	for _, client := range clients {
		remaining[client] = true
	}
	if singular != "" {
		remaining[singular] = true
	}
	if len(remaining) == 0 {
		return false
	}
	for _, line := range strings.Split(string(out), "\n") {
		f := strings.Fields(line)
		if len(f) != 9 || f[0] != strconv.Itoa(proxy.CapturePriority)+":" {
			continue
		}
		values := make(map[string]string, 4)
		valid := true
		for i := 1; i < len(f); i += 2 {
			key := f[i]
			if key == "table" {
				key = "lookup"
			}
			if key != "from" && key != "iif" && key != "fwmark" && key != "lookup" {
				valid = false
				break
			}
			if _, duplicate := values[key]; duplicate {
				valid = false
				break
			}
			values[key] = f[i+1]
		}
		if !valid || values["iif"] != own.LANInterface {
			continue
		}
		source := values["from"]
		addr, e := netip.ParseAddr(source)
		if e != nil {
			prefix, pe := netip.ParsePrefix(source)
			if pe != nil || prefix.Bits() != prefix.Addr().BitLen() {
				continue
			}
			addr = prefix.Addr()
		}
		if addr.Zone() != "" || addr.Is4In6() || addr.Is4() != (family == 4) || !remaining[addr.String()] {
			continue
		}
		value, mask, e := markMask(values["fwmark"])
		if e != nil || value != proxy.CaptureMark || mask != proxy.CaptureMask {
			continue
		}
		number, e := tableNumber(values["lookup"], names)
		if e == nil && number == proxy.CaptureTable {
			delete(remaining, addr.String())
		}
	}
	return len(remaining) == 0
}

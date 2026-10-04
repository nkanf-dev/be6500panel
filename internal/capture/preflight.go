package capture

import (
	"bytes"
	"context"
	"encoding/json"
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
	if plan.Ownership.Datapath == proxy.DatapathRoutedTUN {
		if err := c.preflightTUN(ctx, plan.Ownership); err != nil {
			return err
		}
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

// The application readiness lane separately proves the running main core's
// PID/executable and TUN fd ownership. Capture verifies address state, not PID
// identity, and must not demand absence of a core-created interface.
func (c *Controller) preflightTUN(ctx context.Context, own proxy.RulesOwnership) error {
	if !captureTUNInterface.MatchString(own.TUNInterface) {
		return errors.New("capture TUN interface invalid")
	}
	prefix, err := captureTUNPrefix(own.TUNAddress)
	if err != nil {
		return err
	}
	if err := c.observeTUNState(ctx, own); err != nil {
		return err
	}
	out, err := c.execute(ctx, []string{"ip", "-4", "route", "show", "table", "all"})
	if err != nil {
		return err
	}
	return tunRouteCollisions(out, prefix, own)
}

// Repeat fixed interface state reads during observation. Route/hooks remaining
// installed do not prove that the core-created TUN still has the right address,
// UP state or reverse-path setting. PID/fd owner proof remains application-owned.
func (c *Controller) observeTUNState(ctx context.Context, own proxy.RulesOwnership) error {
	out, err := c.execute(ctx, tunAddressShow(own.TUNInterface))
	if err != nil {
		return err
	}
	if err = verifyTUNAddress(out, own); err != nil {
		return err
	}
	out, err = c.execute(ctx, tunRPFilterShow(own.TUNInterface))
	if err != nil {
		return err
	}
	if strings.TrimSpace(string(out)) != "2" {
		return errors.New("capture TUN requires rp_filter 2")
	}
	return nil
}

func tunAddressShow(name string) []string {
	return []string{"ip", "-j", "-4", "address", "show", "dev", name}
}
func tunRPFilterShow(name string) []string {
	return []string{"sysctl", "-n", "net.ipv4.conf." + name + ".rp_filter"}
}
func verifyTUNAddress(out []byte, own proxy.RulesOwnership) error {
	var interfaces []struct {
		Name  string   `json:"ifname"`
		Flags []string `json:"flags"`
		MTU   int      `json:"mtu"`
		Addrs []struct {
			Family string `json:"family"`
			Local  string `json:"local"`
			Bits   int    `json:"prefixlen"`
		} `json:"addr_info"`
	}
	address, err := netip.ParsePrefix(own.TUNAddress)
	if err != nil || json.Unmarshal(out, &interfaces) != nil || len(interfaces) != 1 {
		return errors.New("capture TUN address unavailable")
	}
	iface := interfaces[0]
	if iface.Name != own.TUNInterface || !slices.Contains(iface.Flags, "UP") || iface.MTU != 1500 || len(iface.Addrs) != 1 ||
		iface.Addrs[0].Family != "inet" || iface.Addrs[0].Local != address.Addr().String() || iface.Addrs[0].Bits != 30 {
		return errors.New("capture TUN address mismatch")
	}
	return nil
}

// A normal factory default route overlaps every address but does not allocate
// an interface prefix. Every non-default allocation that overlaps the private
// /30 must be exactly the core-created connected/local/broadcast route, never a
// route on another interface or a broader private route.
func tunRouteCollisions(out []byte, prefix netip.Prefix, own proxy.RulesOwnership) error {
	address := netip.MustParsePrefix(own.TUNAddress).Addr()
	for _, line := range strings.Split(string(out), "\n") {
		fields := strings.Fields(line)
		if len(fields) == 0 {
			continue
		}
		if !diagnosticRouteRow(fields, 4) {
			return errors.New("cannot verify TUN prefix route collisions")
		}
		destination, kind := fields[0], "unicast"
		switch destination {
		case "local", "broadcast", "unicast", "unreachable", "prohibit", "blackhole", "throw", "nat", "anycast", "multicast":
			kind, destination = destination, fields[1]
		}
		if destination == "default" || destination == "0.0.0.0/0" {
			if dev, _, _ := option(fields, "dev"); dev == own.TUNInterface {
				return errors.New("capture TUN interface already has a default route")
			}
			continue
		}
		route, err := netip.ParsePrefix(destination)
		if err != nil {
			addr, parseErr := netip.ParseAddr(destination)
			if parseErr != nil || !addr.Is4() {
				return errors.New("cannot verify TUN prefix route collisions")
			}
			route = netip.PrefixFrom(addr, 32)
		}
		if !prefix.Overlaps(route) {
			continue
		}
		dev, hasDev, _ := option(fields, "dev")
		proto, hasProto, _ := option(fields, "proto")
		scope, hasScope, _ := option(fields, "scope")
		source, hasSource, _ := option(fields, "src")
		table, hasTable, _ := option(fields, "table")
		_, hasVia, _ := option(fields, "via")
		if !hasDev || dev != own.TUNInterface || !hasProto || proto != "kernel" || hasVia || (hasSource && source != address.String()) {
			return errors.New("capture TUN prefix route collision")
		}
		allowed := kind == "unicast" && route == prefix && hasScope && scope == "link" && (!hasTable || table == "main" || table == "254")
		allowed = allowed || kind == "local" && route.Bits() == 32 && route.Addr() == address && hasScope && scope == "host" && hasTable && (table == "local" || table == "255")
		allowed = allowed || kind == "broadcast" && route.Bits() == 32 && (route.Addr() == prefix.Addr() || route.Addr() == address.Next().Next()) && hasScope && scope == "link" && hasTable && (table == "local" || table == "255")
		if !allowed {
			return errors.New("capture TUN prefix route collision")
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
	return c.reconcileObservation(ctx, false)
}

// Resource proof and desired-scope drift are separate. A failed read is not
// clean inactivity; retain ownership and cleanup uncertainty, not Active proof.
func (c *Controller) observeStatusLocked(ctx context.Context) error {
	err := c.observeLocked(ctx)
	c.active = err == nil
	c.cleanupPending = err != nil || c.cleanupFailed
	return err
}

func (c *Controller) observeLocked(ctx context.Context) error {
	missing := false
	var pending error
	if c.plan.Ownership.Datapath == proxy.DatapathRoutedTUN {
		pending = errors.Join(pending, c.observeTUNState(ctx, c.plan.Ownership))
	}
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
		if c.plan.Ownership.Datapath == proxy.DatapathRoutedTUN {
			if err := tunChainShape(out, chain, c.plan.Apply); err != nil {
				pending = errors.Join(pending, err)
			}
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
		} else if !hasOwnedRoute(out, c.plan.Ownership, family) {
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

// Ordered semantic rows complement exact -C checks. Kernel spelling may add
// tcp/udp match modules or omit /32, but a same-length rule permutation must
// not prove the compiled TUN chain: bypass and final action order matters.
func tunChainShape(out []byte, chain proxy.OwnedChain, apply [][]string) error {
	expected := []string{}
	for _, cmd := range apply {
		if len(cmd) > 6 && cmd[0] == ipTables(chain.Family) && cmd[4] == chain.Table && cmd[5] == "-A" && cmd[6] == chain.Name {
			row, err := tunChainRow(cmd[5:], chain)
			if err != nil {
				return err
			}
			expected = append(expected, row)
		}
	}
	actual, declared := []string{}, false
	for _, line := range strings.Split(string(out), "\n") {
		args, err := splitArgs(line)
		if err != nil {
			return err
		}
		if len(args) == 0 {
			continue
		}
		if len(args) == 2 && args[0] == "-N" && args[1] == chain.Name && !declared && len(actual) == 0 {
			declared = true
			continue
		}
		row, err := tunChainRow(args, chain)
		if err != nil {
			return err
		}
		actual = append(actual, row)
	}
	if !declared || !slices.Equal(actual, expected) {
		return errors.New("owned TUN chain order or rules do not match compiled shape")
	}
	return nil
}

func tunChainRow(args []string, chain proxy.OwnedChain) (string, error) {
	bad := errors.New("owned TUN chain listing does not match compiled shape")
	if len(args) < 4 || args[0] != "-A" || args[1] != chain.Name {
		return "", bad
	}
	protocol, _, err := option(args, "-p")
	if err != nil {
		return "", bad
	}
	parts, seen := []string{}, map[string]bool{}
	for i := 2; i < len(args); i += 2 {
		if i+1 >= len(args) {
			return "", bad
		}
		key, value := args[i], args[i+1]
		if key == "-m" && (value == "tcp" || value == "udp") && value == protocol {
			continue // kernel's implied transport module carries no new match
		}
		if seen[key] {
			return "", bad
		}
		seen[key] = true
		switch key {
		case "-d":
			prefix, err := netip.ParsePrefix(value)
			if err != nil {
				addr, err := netip.ParseAddr(value)
				if err != nil || !addr.Is4() {
					return "", bad
				}
				prefix = netip.PrefixFrom(addr, 32)
			}
			if !prefix.Addr().Is4() {
				return "", bad
			}
			value = prefix.Masked().String()
		case "-p":
			if value != "tcp" && value != "udp" {
				return "", bad
			}
		case "-m":
			if value != "addrtype" {
				return "", bad
			}
		case "--dst-type":
			if value != "LOCAL" {
				return "", bad
			}
		case "--dport", "--to-ports":
			port, err := strconv.ParseUint(value, 10, 16)
			if err != nil || port == 0 {
				return "", bad
			}
			value = strconv.FormatUint(port, 10)
		case "--set-xmark":
			mark, mask, err := markMask(value)
			if err != nil {
				return "", bad
			}
			value = fmt.Sprintf("0x%x/0x%x", mark, mask)
		case "-j":
			if !slices.Contains([]string{"RETURN", "MARK", "REDIRECT", "ACCEPT"}, value) {
				return "", bad
			}
		default:
			return "", bad
		}
		parts = append(parts, key+"="+value)
	}
	slices.Sort(parts)
	return strings.Join(parts, " "), nil
}

func hasOwnedRoute(out []byte, own proxy.RulesOwnership, family int) bool {
	if own.Datapath != proxy.DatapathRoutedTUN {
		return hasLocalRoute(out, family)
	}
	if family != 4 || !captureTUNInterface.MatchString(own.TUNInterface) {
		return false
	}
	// The TUN table is owned exclusively. A local route, a via gateway or an
	// extra route cannot prove the compiled ordinary default-dev intent.
	lines := []string{}
	for _, line := range strings.Split(string(out), "\n") {
		if strings.TrimSpace(line) != "" {
			lines = append(lines, line)
		}
	}
	if len(lines) != 1 {
		return false
	}
	fields := strings.Fields(lines[0])
	if len(fields) < 3 || (fields[0] != "default" && fields[0] != "0.0.0.0/0") || fields[1] != "dev" || fields[2] != own.TUNInterface {
		return false
	}
	for i := 3; i < len(fields); i += 2 {
		if i+1 >= len(fields) {
			return false
		}
		switch fields[i] {
		case "scope":
			if fields[i+1] != "link" {
				return false
			}
		case "table":
			if fields[i+1] != strconv.Itoa(proxy.CaptureTable) {
				return false
			}
		case "proto":
			if fields[i+1] != "boot" && fields[i+1] != "static" {
				return false
			}
		default:
			return false
		}
	}
	return true
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
// Source MAC is enforced by the exact hooks checked above, not by ip rule (which
// has no MAC selector). The masked mark is set only after that hook matches.
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

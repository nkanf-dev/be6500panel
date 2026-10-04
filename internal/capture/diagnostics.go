package capture

import (
	"context"
	"errors"
	"net/netip"
	"os/exec"
	"reflect"
	"slices"
	"strconv"
	"strings"
	"time"

	"be6500panel/internal/proxy"
)

const (
	diagnosticTimeout        = 3 * time.Second
	diagnosticCommandTimeout = time.Second
	diagnosticMaxOutput      = 64 << 10
	diagnosticMaxLine        = 4096
	diagnosticMaxRows        = 512
)

// DatapathDiagnostics is one read-only observation of saved ownership. Counters
// are cumulative kernel rule counters, not interval rates or proof of delivery.
// Rules in a shared chain are not attributed to individual installed clients.
// NAT counters generally describe the first packet of a conntrack flow. This
// report neither repairs the datapath nor proves hooks, listeners or return paths.
// No command output, argv, destination, endpoint, or command error is projected.
// A scope-changed result deliberately discards all saved clients and evidence.
// State is no-plan, complete, partial, unavailable, scope-changed, or
// scope-unverified. Complete means only that the allowed reads completed, not
// that the transparent datapath is healthy.
type DatapathDiagnostics struct {
	Scope                    proxy.CaptureScope `json:"scope,omitempty"`
	InstalledLANIPv4Prefixes []string           `json:"installedLanIPv4Prefixes,omitempty"`
	MeasuredAt               time.Time          `json:"measuredAt"`
	DurationMS               int64              `json:"durationMs"`
	State                    string             `json:"state"`
	Error                    string             `json:"error,omitempty"`
	InstalledClients         []Client           `json:"installedClients,omitempty"`
	Chains                   []DiagnosticChain  `json:"chains,omitempty"`
	Routing                  []DiagnosticFamily `json:"routing,omitempty"`
}

// DiagnosticChain retains distinct rows, including identical rules. Counts are
// never summed across RETURN/TPROXY/REDIRECT stages (that could double count).
// Counters is nil when no counters were proved; a valid empty listing is [].
// State is ok, partial, missing, unavailable, or unsupported. Partial rows remain
// useful evidence, but must not be treated as a complete view of the chain.
type DiagnosticChain struct {
	Family   int               `json:"family"`
	Table    string            `json:"table"`
	Chain    string            `json:"chain"`
	Role     string            `json:"role,omitempty"` // TUN stage only; never proof of delivery.
	State    string            `json:"state"`
	Error    string            `json:"error,omitempty"`
	Counters []DiagnosticCount `json:"counters"`
}

type DiagnosticCount struct {
	Row             int     `json:"row"`
	Packets         uint64  `json:"packets"`
	Bytes           uint64  `json:"bytes"`
	Target          string  `json:"target"`
	Protocol        string  `json:"protocol"`
	DestinationPort *uint16 `json:"destinationPort,omitempty"`
	ListenerPort    *uint16 `json:"listenerPort,omitempty"`
}

type DiagnosticFamily struct {
	Family      int                `json:"family"`
	LocalRoute  DiagnosticEvidence `json:"localRoute"` // Legacy name; TUN proves an ordinary default dev route.
	PolicyRules DiagnosticEvidence `json:"policyRules"`
}

// Present is nil for unavailable or malformed evidence, never false by default.
// Present=false means a successful valid read did not prove the owned resource.
type DiagnosticEvidence struct {
	State   string `json:"state"`
	Present *bool  `json:"present"`
	Error   string `json:"error,omitempty"`
}

// Diagnostics copies the installed plan before any I/O. It never takes c.mu
// during command execution, calls the builder, mutates state, or runs a socket
// probe. In particular a slow read must not delay Disable's lock acquisition.
// The native Runner bounds output and honors context; injected Runners must do
// the same. Deadlines are 3s overall and 1s per read; native process pipe cleanup
// can add the existing run helper's 1s WaitDelay grace after cancellation.
// Every argv is separately checked against a fixed read-only contract,
// without c.execute/approvedCommand consulting mutable controller ownership.
func (c *Controller) Diagnostics(ctx context.Context) DatapathDiagnostics {
	started := time.Now()
	report := DatapathDiagnostics{MeasuredAt: started.UTC(), State: "no-plan"}
	// An unrelated lifecycle operation can hold mu while doing slow I/O. Do not
	// wait for it or extend this read's budget; do not infer absence from busy.
	if !c.mu.TryLock() {
		report.State, report.Error = "unavailable", "capture_diagnostic_busy"
		report.DurationMS = time.Since(started).Milliseconds()
		return report
	}
	identity, runner := c.plan, c.runner
	var saved proxy.OwnedRulesPlan
	if identity != nil {
		saved = clonePlan(*identity)
	}
	c.mu.Unlock()
	if identity == nil {
		report.DurationMS = time.Since(started).Milliseconds()
		return report
	}

	ctx, cancel := context.WithTimeout(ctx, diagnosticTimeout)
	defer cancel()
	report.State = "complete"
	report.Scope = saved.Ownership.Scope
	report.InstalledLANIPv4Prefixes = slices.Clone(saved.Ownership.LANIPv4Prefixes)
	report.InstalledClients = installedClients(saved.Ownership)
	for _, chain := range saved.Ownership.Chains {
		entry := DiagnosticChain{Family: chain.Family, Table: chain.Table, Chain: chain.Name, Role: diagnosticTUNRole(chain)}
		if !diagnosticOwnedChain(chain) {
			entry.State, entry.Error = "unsupported", "capture_diagnostic_chain_unsupported"
		} else {
			argv := diagnosticChainArgs(chain)
			out, code := diagnosticRead(ctx, runner, argv)
			if code == "capture_diagnostic_command_failed" && diagnosticChainMissing(argv[0], out) {
				entry.State, entry.Error = "missing", "capture_diagnostic_chain_missing"
			} else {
				counts, parsed, parseCode := diagnosticCounts(out, chain)
				entry.Counters = counts
				entry.Error = code
				if entry.Error == "" {
					entry.Error = parseCode
				}
				entry.State = "ok"
				if entry.Error != "" {
					entry.State = "unavailable"
					if parsed && len(counts) > 0 {
						entry.State = "partial"
					} else {
						entry.Counters = nil
					}
				}
			}
		}
		if entry.State != "ok" {
			report.State = "partial"
		}
		report.Chains = append(report.Chains, entry)
	}
	for _, family := range saved.Ownership.RouteFamilies {
		route, routeCode := diagnosticRead(ctx, runner, routeShow(family))
		rules, ruleCode := diagnosticRead(ctx, runner, ruleShow(family))
		entry := DiagnosticFamily{Family: family,
			LocalRoute:  diagnosticOwnedRoute(route, routeCode, saved.Ownership, family),
			PolicyRules: diagnosticRules(rules, ruleCode, saved.Ownership, family)}
		if entry.LocalRoute.State != "ok" || entry.PolicyRules.State != "ok" {
			report.State = "partial"
		}
		report.Routing = append(report.Routing, entry)
	}

	if !c.mu.TryLock() {
		// A concurrent lifecycle operation may be replacing ownership. Without
		// a final comparison, counts must not be attached to any saved scope.
		report.State, report.Error = "scope-unverified", "capture_diagnostic_scope_unverified"
		report.Scope, report.InstalledLANIPv4Prefixes = "", nil
		report.InstalledClients, report.Chains, report.Routing = nil, nil, nil
	} else {
		// Identity also detects cleanup followed by an identical new plan (ABA).
		changed := c.plan != identity || c.plan == nil || !reflect.DeepEqual(*c.plan, saved)
		c.mu.Unlock()
		if changed {
			report.State, report.Error = "scope-changed", "capture_diagnostic_scope_changed"
			report.Scope, report.InstalledLANIPv4Prefixes = "", nil
			report.InstalledClients, report.Chains, report.Routing = nil, nil, nil
		}
	}
	report.DurationMS = time.Since(started).Milliseconds()
	return report
}

func diagnosticOwnedChain(chain proxy.OwnedChain) bool {
	return (chain.Family == 4 || chain.Family == 6) &&
		(chain.Table == "mangle" || chain.Table == "nat" || diagnosticTUNRole(chain) != "") && ownedChain(chain.Family, chain.Table, chain.Name)
}

func diagnosticTUNRole(chain proxy.OwnedChain) string {
	if chain.Family != 4 || !ownedChain(chain.Family, chain.Table, chain.Name) {
		return ""
	}
	switch chain.Name {
	case "B6P_V4_TUN_MARK":
		return "original-ingress"
	case "B6P_V4_TUN_FORWARD":
		return "forward"
	case "B6P_V4_TUN_RETURN":
		return "return"
	case "B6P_V4_TUN_INPUT":
		return "private-input"
	case "B6P_V4_TUN_OUTPUT":
		return "private-output"
	}
	return ""
}

func diagnosticChainArgs(chain proxy.OwnedChain) []string {
	return []string{ipTables(chain.Family), "-w", "1", "-t", chain.Table, "-L", chain.Name, "-n", "-v", "-x"}
}

func diagnosticReadAllowed(argv []string) bool {
	for _, chain := range []proxy.OwnedChain{
		{Family: 4, Table: "mangle", Name: "B6P_V4_TUN_MARK"},
		{Family: 4, Table: "filter", Name: "B6P_V4_TUN_FORWARD"},
		{Family: 4, Table: "filter", Name: "B6P_V4_TUN_RETURN"},
		{Family: 4, Table: "filter", Name: "B6P_V4_TUN_INPUT"},
		{Family: 4, Table: "filter", Name: "B6P_V4_TUN_OUTPUT"},
	} {
		if slices.Equal(argv, diagnosticChainArgs(chain)) {
			return true
		}
	}
	for _, family := range []int{4, 6} {
		if slices.Equal(argv, routeShow(family)) || slices.Equal(argv, ruleShow(family)) {
			return true
		}
		for _, table := range []string{"mangle", "nat"} {
			name := "B6P_V" + strconv.Itoa(family) + "_CAPTURE"
			if table == "nat" {
				name = "B6P_V" + strconv.Itoa(family) + "_DNS"
			}
			if slices.Equal(argv, diagnosticChainArgs(proxy.OwnedChain{Family: family, Table: table, Name: name})) {
				return true
			}
		}
	}
	return false
}

func diagnosticRead(ctx context.Context, runner Runner, argv []string) ([]byte, string) {
	if !diagnosticReadAllowed(argv) {
		return nil, "capture_diagnostic_command_unapproved"
	}
	ctx, cancel := context.WithTimeout(ctx, diagnosticCommandTimeout)
	defer cancel()
	if ctx.Err() != nil {
		return nil, diagnosticError(ctx.Err())
	}
	out, err := runner(ctx, slices.Clone(argv))
	if ctx.Err() != nil {
		err = ctx.Err()
	}
	code := diagnosticError(err)
	if len(out) >= diagnosticMaxOutput {
		out = out[:diagnosticMaxOutput]
		if code == "" {
			code = "capture_diagnostic_output_limit"
		}
	}
	return out, code
}

func diagnosticError(err error) string {
	if err == nil {
		return ""
	}
	if errors.Is(err, context.DeadlineExceeded) {
		return "capture_diagnostic_timeout"
	}
	if errors.Is(err, context.Canceled) {
		return "capture_diagnostic_canceled"
	}
	var missingExecutable *exec.Error
	if errors.Is(err, exec.ErrNotFound) || errors.As(err, &missingExecutable) {
		return "capture_diagnostic_unavailable"
	}
	return "capture_diagnostic_command_failed"
}

func diagnosticChainMissing(tool string, out []byte) bool {
	text := strings.TrimSpace(string(out))
	return text == tool+": No chain/target/match by that name." || text == "No chain/target/match by that name."
}

func diagnosticCounts(out []byte, chain proxy.OwnedChain) ([]DiagnosticCount, bool, string) {
	var counts []DiagnosticCount
	chainHeader, columns, row := false, false, 0
	code := ""
	for _, line := range strings.Split(string(out), "\n") {
		if len(line) > diagnosticMaxLine {
			if columns && strings.TrimSpace(line) != "" {
				row++
				if row > diagnosticMaxRows {
					return counts, true, "capture_diagnostic_row_limit"
				}
			}
			code = "capture_diagnostic_malformed"
			continue
		}
		fields := strings.Fields(line)
		if len(fields) == 0 {
			continue
		}
		if fields[0] == "Chain" {
			if chainHeader || len(fields) != 4 || fields[1] != chain.Name || fields[3] != "references)" || !strings.HasPrefix(fields[2], "(") {
				return nil, false, "capture_diagnostic_malformed"
			}
			// Normal listing: Chain NAME (N references). No policy chains allowed.
			if _, ok := diagnosticUint(strings.TrimPrefix(fields[2], "("), 64); !ok {
				return nil, false, "capture_diagnostic_malformed"
			}
			chainHeader = true
			continue
		}
		if fields[0] == "pkts" {
			if !chainHeader || columns || !slices.Equal(fields, []string{"pkts", "bytes", "target", "prot", "opt", "in", "out", "source", "destination"}) {
				return nil, false, "capture_diagnostic_malformed"
			}
			columns = true
			counts = []DiagnosticCount{}
			continue
		}
		warning := strings.TrimSpace(strings.TrimPrefix(strings.TrimSpace(line), "#"))
		if strings.HasPrefix(warning, "Warning:") || strings.HasPrefix(warning, "WARNING:") {
			if code == "" {
				code = "capture_diagnostic_warning"
			}
			continue
		}
		if !columns {
			return nil, false, "capture_diagnostic_malformed"
		}
		row++
		if row > diagnosticMaxRows {
			return counts, true, "capture_diagnostic_row_limit"
		}
		count, ok := diagnosticCountForChain(fields, chain, row)
		if !ok {
			code = "capture_diagnostic_malformed"
			continue
		}
		counts = append(counts, count)
	}
	if !columns {
		return nil, false, "capture_diagnostic_malformed"
	}
	return counts, true, code
}

func diagnosticCount(fields []string, family, row int) (DiagnosticCount, bool) {
	return diagnosticCountForChain(fields, proxy.OwnedChain{Family: family}, row)
}

func diagnosticCountForChain(fields []string, chain proxy.OwnedChain, row int) (DiagnosticCount, bool) {
	family := chain.Family
	count := DiagnosticCount{Row: row}
	if len(fields) < 8 {
		return count, false
	}
	var ok bool
	count.Packets, ok = diagnosticUint(fields[0], 64)
	if !ok {
		return count, false
	}
	count.Bytes, ok = diagnosticUint(fields[1], 64)
	if !ok {
		return count, false
	}
	count.Target, count.Protocol = fields[2], fields[3]
	allowed := count.Target == "RETURN" || count.Target == "TPROXY" || count.Target == "REDIRECT"
	if role := diagnosticTUNRole(chain); role != "" {
		allowed = count.Target == "RETURN"
		if role == "original-ingress" {
			allowed = allowed || count.Target == "MARK"
		} else {
			allowed = allowed || count.Target == "ACCEPT" || count.Target == "DROP"
		}
	}
	if !allowed {
		return count, false
	}
	if count.Protocol != "all" && count.Protocol != "tcp" && count.Protocol != "udp" {
		return count, false
	}
	base := 8 // ip6tables leaves the opt column empty on some QSDK releases.
	if fields[4] == "--" {
		base = 9
	} else if family != 6 {
		return count, false
	}
	if len(fields) < base || !diagnosticAddress(fields[base-2], family) || !diagnosticAddress(fields[base-1], family) {
		return count, false
	}
	for i := base; i < len(fields); i++ {
		field := fields[i]
		if strings.HasPrefix(field, "dpt:") {
			port, ok := diagnosticPort(strings.TrimPrefix(field, "dpt:"))
			if !ok || count.DestinationPort != nil {
				return count, false
			}
			count.DestinationPort = &port
		}
		if count.Target == "REDIRECT" && field == "ports" {
			if i < base+1 || fields[i-1] != "redir" || i+1 >= len(fields) || count.ListenerPort != nil {
				return count, false
			}
			port, ok := diagnosticPort(fields[i+1])
			if !ok {
				return count, false
			}
			count.ListenerPort = &port
		}
		if count.Target == "TPROXY" && field == "redirect" {
			if i+1 >= len(fields) || count.ListenerPort != nil {
				return count, false
			}
			// QSDK prints an unbracketed IPv6 address, such as ::1:7893.
			value := fields[i+1]
			colon := strings.LastIndexByte(value, ':')
			if colon < 0 {
				return count, false
			}
			addr, err := netip.ParseAddr(strings.Trim(value[:colon], "[]"))
			port, ok := diagnosticPort(value[colon+1:])
			if err != nil || !addr.IsLoopback() || addr.Is4() != (family == 4) || !ok {
				return count, false
			}
			count.ListenerPort = &port
		}
	}
	return count, true
}

func diagnosticAddress(raw string, family int) bool {
	addr, err := netip.ParseAddr(raw)
	if err != nil {
		prefix, err := netip.ParsePrefix(raw)
		if err != nil {
			return false
		}
		addr = prefix.Addr()
	}
	return addr.Zone() == "" && !addr.Is4In6() && addr.Is4() == (family == 4)
}

func diagnosticUint(raw string, bits int) (uint64, bool) {
	if raw == "" {
		return 0, false
	}
	for _, ch := range raw {
		if ch < '0' || ch > '9' {
			return 0, false
		}
	}
	value, err := strconv.ParseUint(raw, 10, bits)
	return value, err == nil
}

func diagnosticPort(raw string) (uint16, bool) {
	port, ok := diagnosticUint(raw, 16)
	return uint16(port), ok && port != 0
}

func diagnosticBool(value bool) *bool { return &value }

func diagnosticProof(present bool) DiagnosticEvidence {
	if !present {
		return DiagnosticEvidence{State: "missing", Present: diagnosticBool(false)}
	}
	return DiagnosticEvidence{State: "ok", Present: diagnosticBool(true)}
}

func diagnosticRoute(out []byte, code string, family int) DiagnosticEvidence {
	return diagnosticOwnedRoute(out, code, proxy.RulesOwnership{}, family)
}

func diagnosticOwnedRoute(out []byte, code string, own proxy.RulesOwnership, family int) DiagnosticEvidence {
	if code != "" {
		if code == "capture_diagnostic_command_failed" && strings.HasPrefix(strings.TrimSpace(string(out)), "Error: ipv"+strconv.Itoa(family)+":") && emptyFIB(out, errors.New("read failed")) {
			return diagnosticProof(false)
		}
		return DiagnosticEvidence{State: "unavailable", Error: code}
	}
	// Be conservative about unfamiliar/truncated rows before a negative proof.
	for _, line := range strings.Split(string(out), "\n") {
		fields := strings.Fields(line)
		if len(fields) == 0 {
			continue
		}
		if len(line) > diagnosticMaxLine || !diagnosticRouteRow(fields, family) {
			return DiagnosticEvidence{State: "unavailable", Error: "capture_diagnostic_malformed"}
		}
	}
	return diagnosticProof(hasOwnedRoute(out, own, family))
}

func diagnosticRouteRow(fields []string, family int) bool {
	destination, start, terminal := fields[0], 1, false
	switch destination {
	case "unreachable", "prohibit", "blackhole", "throw":
		terminal = true
		fallthrough
	case "local", "broadcast", "unicast", "nat", "anycast", "multicast":
		if len(fields) < 2 {
			return false
		}
		destination, start = fields[1], 2
	}
	if destination != "default" && !diagnosticAddress(destination, family) {
		return false
	}
	path := terminal
	for i := start; i < len(fields); {
		if fields[i] == "linkdown" || fields[i] == "onlink" {
			i++
			continue
		}
		if i+1 >= len(fields) {
			return false
		}
		switch fields[i] {
		case "dev":
			path = true
		case "via":
			if !diagnosticAddress(fields[i+1], family) {
				return false
			}
			path = true
		case "src":
			if !diagnosticAddress(fields[i+1], family) {
				return false
			}
		case "metric", "mtu", "advmss", "hoplimit":
			if _, ok := diagnosticUint(fields[i+1], 32); !ok {
				return false
			}
		case "scope", "proto", "table", "pref":
		default:
			return false
		}
		i += 2
	}
	return path
}

func diagnosticRules(out []byte, code string, own proxy.RulesOwnership, family int) DiagnosticEvidence {
	if code != "" {
		return DiagnosticEvidence{State: "unavailable", Error: code}
	}
	names := map[string]int{"local": 255, "main": 254, "default": 253, "unspec": 0}
	owned := hasOwnedRule(out, own, family, names)
	for _, line := range strings.Split(string(out), "\n") {
		fields := strings.Fields(line)
		if len(fields) == 0 {
			continue
		}
		if len(line) > diagnosticMaxLine {
			return DiagnosticEvidence{State: "unavailable", Error: "capture_diagnostic_malformed"}
		}
		if rowCode := diagnosticRuleRow(fields, family, names, owned); rowCode != "" {
			return DiagnosticEvidence{State: "unavailable", Error: rowCode}
		}
	}
	return diagnosticProof(owned)
}

func diagnosticRuleRow(fields []string, family int, names map[string]int, owned bool) string {
	malformed := "capture_diagnostic_malformed"
	if len(fields) < 3 || !strings.HasSuffix(fields[0], ":") {
		return malformed
	}
	if _, ok := diagnosticUint(strings.TrimSuffix(fields[0], ":"), 32); !ok {
		return malformed
	}
	from, action := false, false
	seen := map[string]bool{}
	for i := 1; i < len(fields); i++ {
		key := fields[i]
		if key == "not" {
			// ip rule prints a leading inversion flag, never a trailing token.
			if i != 1 || i+1 >= len(fields) {
				return malformed
			}
			continue
		}
		if key == "blackhole" || key == "unreachable" || key == "prohibit" {
			action = true
			continue
		}
		if i+1 >= len(fields) || seen[key] {
			return malformed
		}
		seen[key] = true
		value := fields[i+1]
		i++
		switch key {
		case "from", "to":
			if value != "all" && !diagnosticAddress(value, family) {
				return malformed
			}
			from = from || key == "from"
		case "fwmark":
			if _, _, err := markMask(value); err != nil {
				return malformed
			}
		case "lookup", "table":
			action = true
			if _, err := tableNumber(value, names); err != nil && !owned && fields[0] == strconv.Itoa(proxy.CapturePriority)+":" {
				// Only unresolved owned candidates are uncertain. No alias I/O.
				return "capture_diagnostic_table_alias_unresolved"
			}
		case "goto":
			action = true
			if _, ok := diagnosticUint(value, 32); !ok {
				return malformed
			}
		case "suppress_prefixlength", "suppress_ifgroup", "pref", "priority":
			if _, ok := diagnosticUint(value, 32); !ok {
				return malformed
			}
		case "iif", "oif", "tos", "dsfield", "uidrange", "ipproto", "sport", "dport", "protocol", "l3mdev":
		default:
			return malformed
		}
	}
	if !from || !action {
		return malformed
	}
	return ""
}

// Package capture applies only internally compiled exact-client network intent.
package capture

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"maps"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"syscall"
	"time"

	"be6500panel/internal/proxy"
	"be6500panel/internal/storage"
)

type Runner func(context.Context, []string) ([]byte, error)
type Status struct {
	Desired        bool           `json:"desired"`
	Clients        []Client       `json:"clients"`
	IPv6           proxy.IPv6Mode `json:"ipv6,omitempty"`
	Error          string         `json:"error,omitempty"`
	Active         bool           `json:"active"`
	ClientIPv4     string         `json:"clientIPv4,omitempty"`
	ClientIPv6     string         `json:"clientIPv6,omitempty"`
	Commands       int            `json:"commands"`
	CleanupPending bool           `json:"cleanupPending"`
	State          string         `json:"state"`
}

// CommandError contains only internally compiled network argv and bounded kernel
// diagnostics. It preserves the original error for errors.Is/errors.As.
type CommandError struct {
	Argv   []string
	Output string
	Err    error
}

func (e *CommandError) Error() string {
	message := fmt.Sprintf("capture command %q failed: %v", e.Argv, e.Err)
	if e.Output != "" {
		message += fmt.Sprintf(" (output=%q)", e.Output)
	}
	return message
}
func (e *CommandError) Unwrap() error { return e.Err }

type Controller struct {
	mu               sync.Mutex
	path             string
	runner           Runner
	tableNames       func() (map[string]int, error)
	plan             *proxy.OwnedRulesPlan
	active           bool
	cleanupPending   bool
	desiredPath      string
	desired          Desired
	clients          []Client
	restoreError     string
	builder          Builder
	failedPlan       *proxy.OwnedRulesPlan
	storageAdmission storage.Admission
	storageReserved  bool
}

func New(dataDir string, runner Runner) (*Controller, error) {
	if runner == nil {
		runner = run
	}
	c := &Controller{path: filepath.Join(dataDir, "capture-journal.json"), desiredPath: filepath.Join(dataDir, "capture-desired.json"), runner: runner, tableNames: readTableNames}
	if err := c.loadDesired(); err != nil {
		return nil, err
	}
	raw, err := os.ReadFile(c.path)
	if err == nil {
		var stored journal
		if json.Unmarshal(raw, &stored) != nil {
			return nil, errors.New("capture journal invalid")
		}
		plan, err := recoveredPlan(stored)
		if err != nil {
			return nil, err
		}
		c.plan = &plan
		c.cleanupPending = true // Presence on disk does not prove live hooks.
	} else if !os.IsNotExist(err) {
		return nil, err
	}
	return c, nil
}
func (c *Controller) Status() Status { c.mu.Lock(); defer c.mu.Unlock(); return c.statusLocked() }
func (c *Controller) statusLocked() Status {
	state := Status{Active: c.active, CleanupPending: c.cleanupPending, State: "inactive", Desired: c.desired.Enabled, Clients: slices.Clone(c.clients), IPv6: c.desired.IPv6, Error: c.restoreError}
	if state.Clients == nil {
		state.Clients = []Client{}
	}
	if state.Desired {
		state.State = "suspended"
	}
	if c.plan != nil {
		state.ClientIPv4 = c.plan.Ownership.ClientIPv4
		state.ClientIPv6 = c.plan.Ownership.ClientIPv6
		if state.ClientIPv4 == "" && len(c.plan.Ownership.ClientIPv4s) > 0 {
			state.ClientIPv4 = c.plan.Ownership.ClientIPv4s[0]
		}
		if state.ClientIPv6 == "" && len(c.plan.Ownership.ClientIPv6s) > 0 {
			state.ClientIPv6 = c.plan.Ownership.ClientIPv6s[0]
		}
		state.Commands = len(c.plan.Apply)
		if c.cleanupPending {
			state.State = "cleanup-pending"
		} else if c.active {
			state.State = "active"
			if c.restoreError == "capture_devices_pending" {
				state.State = "partial"
			}
		} else {
			state.State = "staged"
		}
	}
	return state
}

// Apply accepts compiler input, never caller-supplied command arrays.
func (c *Controller) Apply(ctx context.Context, input proxy.RulesPlanInput) (Status, error) {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.applyLocked(ctx, input)
}
func (c *Controller) applyLocked(ctx context.Context, input proxy.RulesPlanInput) (Status, error) {
	if c.plan != nil {
		return c.statusLocked(), errors.New("capture already staged; stop first")
	}
	input = cloneInput(input)
	plan, err := proxy.PlanOwnedRules(input)
	if err != nil {
		return c.statusLocked(), err
	}
	plan = clonePlan(plan)
	raw, err := json.Marshal(journal{OwnedRulesPlan: plan, Input: &input})
	if err != nil {
		return c.statusLocked(), err
	}
	release, err := c.admitStorageLocked(ctx, temporaryStorageBytes(raw), false)
	if err != nil {
		return c.statusLocked(), err
	}
	defer release()
	if err = c.preflight(ctx, plan); err != nil {
		return c.statusLocked(), err
	}
	if err = atomicSave(c.path, raw); err != nil {
		return c.statusLocked(), err
	}
	c.plan = &plan
	c.cleanupPending = true
	for _, argv := range plan.Apply {
		if _, err = c.execute(ctx, argv); err != nil {
			rollbackCtx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
			cleanupErr := c.cleanupLocked(rollbackCtx)
			cancel()
			return c.statusLocked(), errors.Join(fmt.Errorf("capture application failed: %w", err), cleanupErr)
		}
	}
	c.active = true
	c.cleanupPending = false
	c.restoreError = ""
	return c.statusLocked(), nil
}
func (c *Controller) Cleanup(ctx context.Context) error {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.cleanupLocked(ctx)
}
func (c *Controller) cleanupLocked(ctx context.Context) error {
	if c.plan == nil {
		return nil
	}
	c.cleanupPending = true
	var pending error
	for _, argv := range c.plan.Cleanup {
		out, err := c.execute(ctx, argv)
		if err != nil && !resourceAbsent(argv, out, err) && !c.absentMACChain(ctx, argv, out, err) {
			pending = errors.Join(pending, err)
		}
	}
	if pending != nil {
		return pending
	} // Live hooks may remain. Never report clean inactivity.
	if err := os.Remove(c.path); err != nil && !os.IsNotExist(err) {
		return fmt.Errorf("capture journal removal failed: %w", err)
	}
	c.plan = nil
	c.active = false
	c.cleanupPending = false
	return nil
}
func resourceAbsent(a []string, out []byte, err error) bool {
	if err == nil || errors.Is(err, context.Canceled) || errors.Is(err, context.DeadlineExceeded) {
		return false
	}
	text := strings.TrimSpace(string(out))
	if len(a) > 3 && a[0] == "ip" && a[3] == "del" {
		if a[2] == "route" {
			return text == "RTNETLINK answers: No such process"
		}
		if a[2] == "rule" {
			return text == "RTNETLINK answers: No such file or directory" || text == "RTNETLINK answers: No such process"
		}
	}
	if len(a) <= 6 || (a[0] != "iptables" && a[0] != "ip6tables") {
		return false
	}
	op := a[5]
	if op != "-D" && op != "-F" && op != "-X" && op != "-S" && op != "-C" {
		return false
	}
	// These exact diagnostics differ from executable/table/match-load failures.
	noChain := text == a[0]+": No chain/target/match by that name." || text == "No chain/target/match by that name."
	if noChain {
		if (op == "-S" || op == "-F" || op == "-X") && len(a) == 7 {
			return true
		}
		// Source/interface-only legacy hooks use built-in matches. MAC hooks
		// and TPROXY/addrtype checks can fail because a match extension is
		// unavailable, so their ambiguous diagnostic is NOT absence.
		return (op == "-D" || op == "-C") && (a[6] == "PREROUTING" || a[6] == "FORWARD") && !slices.Contains(a, "--mac-source")
	}
	badRule := a[0] + ": Bad rule (does a matching rule exist in that chain?)."
	return (op == "-D" || op == "-C") && (text == badRule || text == "Bad rule (does a matching rule exist in that chain?).")
}
func (c *Controller) execute(ctx context.Context, argv []string) ([]byte, error) {
	if err := c.approvedCommand(argv); err != nil {
		return nil, err
	}
	ctx, cancel := context.WithTimeout(ctx, 8*time.Second)
	defer cancel()
	if err := ctx.Err(); err != nil {
		return nil, &CommandError{Argv: slices.Clone(argv), Err: err}
	}
	out, err := c.runner(ctx, slices.Clone(argv))
	if ctx.Err() != nil {
		err = errors.Join(err, ctx.Err())
	}
	if len(out) >= 64<<10 && isReadCommand(argv) {
		err = errors.Join(err, errors.New("capture inspection output reached limit"))
	}
	if len(out) > 64<<10 {
		out = out[:64<<10]
	}
	out = bytes.Clone(out)
	if err != nil {
		return out, &CommandError{Argv: slices.Clone(argv), Output: strings.TrimSpace(string(out)), Err: err}
	}
	return out, nil
}
func atomicSave(path string, raw []byte) error {
	if err := os.MkdirAll(filepath.Dir(path), 0700); err != nil {
		return err
	}
	f, err := os.CreateTemp(filepath.Dir(path), ".capture-")
	if err != nil {
		return err
	}
	defer os.Remove(f.Name())
	if err = f.Chmod(0600); err == nil {
		_, err = f.Write(raw)
	}
	if err == nil {
		err = f.Sync()
	}
	if closeErr := f.Close(); err == nil {
		err = closeErr
	}
	if err != nil {
		return err
	}
	if err := os.Rename(f.Name(), path); err != nil {
		return err
	}
	dir, err := os.Open(filepath.Dir(path))
	if err != nil {
		return err
	}
	defer dir.Close()
	return dir.Sync()
}

type boundedOutput struct{ bytes.Buffer }

func (b *boundedOutput) Write(p []byte) (int, error) {
	n := len(p)
	remaining := (64 << 10) - b.Len()
	if remaining > 0 {
		if len(p) > remaining {
			p = p[:remaining]
		}
		_, _ = b.Buffer.Write(p)
	}
	return n, nil
}
func run(ctx context.Context, argv []string) ([]byte, error) {
	// execute already matched argv against fixed inspection or compiled intent.
	if err := validateArgs(argv); err != nil {
		return nil, err
	}
	cmd := exec.CommandContext(ctx, argv[0], argv[1:]...)
	cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}
	cmd.Cancel = func() error {
		if cmd.Process == nil {
			return os.ErrProcessDone
		}
		return syscall.Kill(-cmd.Process.Pid, syscall.SIGKILL)
	}
	cmd.WaitDelay = time.Second
	var out boundedOutput
	cmd.Stdout = &out
	cmd.Stderr = &out
	err := cmd.Run()
	return out.Bytes(), err
}

// MAC-match load failure and a missing jump target can share one diagnostic.
// Only a separate approved target-chain listing can prove this hook absent.
func (c *Controller) absentMACChain(ctx context.Context, args []string, out []byte, err error) bool {
	if errors.Is(err, context.Canceled) || errors.Is(err, context.DeadlineExceeded) || len(args) < 7 || args[5] != "-D" || !slices.Contains(args, "--mac-source") {
		return false
	}
	text := strings.TrimSpace(string(out))
	if text != args[0]+": No chain/target/match by that name." && text != "No chain/target/match by that name." {
		return false
	}
	target, hasTarget, parseErr := option(args, "-j")
	if parseErr != nil || !hasTarget {
		return false
	}
	inspection := []string{args[0], "-w", "5", "-t", args[4], "-S", target}
	output, inspectionErr := c.execute(ctx, inspection)
	return resourceAbsent(inspection, output, inspectionErr)
}

func cloneInput(input proxy.RulesPlanInput) proxy.RulesPlanInput {
	input.ClientMACs = maps.Clone(input.ClientMACs)
	input.ClientIPv4s = slices.Clone(input.ClientIPv4s)
	input.ClientIPv6s = slices.Clone(input.ClientIPv6s)
	input.EndpointIPs = slices.Clone(input.EndpointIPs)
	input.ManagementIPs = slices.Clone(input.ManagementIPs)
	input.RouterDNSAddresses = slices.Clone(input.RouterDNSAddresses)
	return input
}

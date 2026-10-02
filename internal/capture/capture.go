// Package capture applies only internally compiled single-client network intent.
package capture

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"

	"be6500panel/internal/proxy"
)

type Runner func(context.Context, []string) ([]byte, error)
type Status struct {
	Active     bool   `json:"active"`
	ClientIPv4 string `json:"clientIPv4,omitempty"`
	ClientIPv6 string `json:"clientIPv6,omitempty"`
	Commands   int    `json:"commands"`
}
type Controller struct {
	mu     sync.Mutex
	path   string
	runner Runner
	plan   *proxy.OwnedRulesPlan
	active bool
}

func New(dataDir string, runner Runner) (*Controller, error) {
	if runner == nil {
		runner = run
	}
	c := &Controller{path: filepath.Join(dataDir, "capture-journal.json"), runner: runner}
	raw, err := os.ReadFile(c.path)
	if err == nil {
		var plan proxy.OwnedRulesPlan
		if json.Unmarshal(raw, &plan) != nil {
			return nil, errors.New("capture journal invalid")
		}
		for _, argv := range plan.Cleanup {
			if err = validate(argv); err != nil {
				return nil, err
			}
		}
		c.plan = &plan
	} else if !os.IsNotExist(err) {
		return nil, err
	}
	return c, nil
}
func (c *Controller) Status() Status { c.mu.Lock(); defer c.mu.Unlock(); return c.statusLocked() }
func (c *Controller) statusLocked() Status {
	state := Status{Active: c.active}
	if c.plan != nil {
		state.ClientIPv4 = c.plan.Ownership.ClientIPv4
		state.ClientIPv6 = c.plan.Ownership.ClientIPv6
		state.Commands = len(c.plan.Apply)
	}
	return state
}
func (c *Controller) Apply(ctx context.Context, plan proxy.OwnedRulesPlan) (Status, error) {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.plan != nil {
		return c.statusLocked(), errors.New("capture already staged; stop first")
	}
	for _, argv := range append(append([][]string{}, plan.Apply...), plan.Cleanup...) {
		if err := validate(argv); err != nil {
			return c.statusLocked(), err
		}
	}
	if err := c.preflight(ctx, plan); err != nil {
		return c.statusLocked(), err
	}
	raw, err := json.Marshal(plan)
	if err != nil {
		return c.statusLocked(), err
	}
	if err = atomicSave(c.path, raw); err != nil {
		return c.statusLocked(), err
	}
	c.plan = &plan
	for _, argv := range plan.Apply {
		if _, err = c.execute(ctx, argv); err != nil {
			rollbackCtx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
			cleanupErr := c.cleanupLocked(rollbackCtx)
			cancel()
			return c.statusLocked(), errors.Join(errors.New("capture application failed"), cleanupErr)
		}
	}
	c.active = true
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
	var pending error
	for _, argv := range c.plan.Cleanup {
		out, err := c.execute(ctx, argv)
		if err != nil && !absent(out) {
			pending = errors.Join(pending, errors.New("owned capture cleanup failed"))
		}
	}
	c.active = false
	if pending == nil {
		if err := os.Remove(c.path); err != nil && !os.IsNotExist(err) {
			return err
		}
		c.plan = nil
	}
	return pending
}
func absent(out []byte) bool {
	text := string(out)
	return strings.Contains(text, "No chain/target/match by that name") || strings.Contains(text, "Bad rule") || strings.Contains(text, "No such file") || strings.Contains(text, "does a matching rule exist") || strings.Contains(text, "Cannot find device")
}
func (c *Controller) execute(ctx context.Context, argv []string) ([]byte, error) {
	ctx, cancel := context.WithTimeout(ctx, 8*time.Second)
	defer cancel()
	return c.runner(ctx, argv)
}
func (c *Controller) preflight(ctx context.Context, plan proxy.OwnedRulesPlan) error {
	for _, family := range plan.Ownership.RouteFamilies {
		prefix := "-" + strconv.Itoa(family)
		for _, args := range [][]string{{"ip", prefix, "route", "show", "table", strconv.Itoa(proxy.CaptureTable)}, {"ip", prefix, "rule", "show"}} {
			out, err := c.execute(ctx, args)
			if err != nil {
				return errors.New("capture preflight failed")
			}
			if args[2] == "route" && len(bytes.TrimSpace(out)) != 0 {
				return errors.New("capture table occupied")
			}
			if args[2] == "rule" {
				for _, line := range strings.Split(string(out), "\n") {
					if strings.HasPrefix(strings.TrimSpace(line), strconv.Itoa(proxy.CapturePriority)+":") {
						return errors.New("capture rule priority occupied")
					}
				}
			}
		}
	}
	for _, chain := range plan.Ownership.Chains {
		tool := "iptables"
		if chain.Family == 6 {
			tool = "ip6tables"
		}
		_, err := c.execute(ctx, []string{tool, "-w", "5", "-t", chain.Table, "-S", chain.Name})
		if err == nil {
			return errors.New("capture chain occupied")
		}
	}
	return nil
}
func validate(argv []string) error {
	if len(argv) < 2 {
		return errors.New("invalid owned command")
	}
	if argv[0] != "ip" && argv[0] != "iptables" && argv[0] != "ip6tables" {
		return errors.New("unapproved capture executable")
	}
	for _, arg := range argv {
		if strings.ContainsAny(arg, "\x00\r\n") || len(arg) > 256 {
			return errors.New("invalid capture argument")
		}
	}
	// Stored cleanup must reference this module's fixed ownership, never global flush.
	if argv[0] != "ip" {
		for i, arg := range argv {
			if (arg == "-F" || arg == "-X" || arg == "-N") && (i+1 >= len(argv) || !strings.HasPrefix(argv[i+1], "B6P_")) {
				return errors.New("unowned chain")
			}
		}
	}
	return nil
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
	return os.Rename(f.Name(), path)
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
	if err := validate(argv); err != nil {
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

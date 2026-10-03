package router

import (
	"context"
	"errors"
	"io"
	"os"
	"os/exec"
	"time"
)

var ErrServiceAction = errors.New("service action refused or failed")

type ServiceActionRequest struct {
	Service       string `json:"service"`
	Action        string `json:"action"`
	ConfirmImpact bool   `json:"confirmImpact"`
}

// CommandAccepted only means the fixed init command returned zero. The actual
// process observation is returned separately; it is not a WAN/health assertion.
type ServiceActionResult struct {
	Service         string          `json:"service"`
	Action          string          `json:"action"`
	CommandAccepted bool            `json:"commandAccepted"`
	ErrorCode       string          `json:"errorCode,omitempty"`
	Snapshot        ServiceSnapshot `json:"snapshot"`
}

const dnsmasqImpact = "DNS/DHCP 服务将短暂中断，设备解析或续租可能受影响。"

var fixedServiceActions = map[string]struct {
	path    string
	actions []string
	impact  string
}{
	"ddns":    {"/etc/init.d/ddns", []string{"start", "stop", "restart", "reload"}, ""},
	"dnsmasq": {"/etc/init.d/dnsmasq", []string{"reload", "restart"}, dnsmasqImpact},
}

func containsServiceAction(actions []string, action string) bool {
	for _, a := range actions {
		if a == action {
			return true
		}
	}
	return false
}
func (o *ServiceObserver) installedAction(name string) bool {
	allowed, ok := fixedServiceActions[name]
	if !ok {
		return false
	}
	path, err := o.rootPath(allowed.path, true)
	if err != nil {
		return false
	}
	info, err := os.Stat(path)
	return err == nil && info.Mode().IsRegular() && info.Mode().Perm()&0111 != 0
}
func (o *ServiceObserver) attachActions(s *ServiceSnapshot) {
	for i := range s.Services {
		row := &s.Services[i]
		row.Actions = nil
		row.ActionImpact = ""
		if !o.adapter.live || s.Stale || row.Protected || row.Configured != "present" || !o.installedAction(row.Name) {
			continue
		}
		allowed, ok := fixedServiceActions[row.Name]
		if !ok {
			continue
		}
		row.ActionImpact = allowed.impact
		for _, action := range allowed.actions {
			if action == "stop" && row.ProcessState != "running" {
				continue
			}
			row.Actions = append(row.Actions, action)
		}
	}
}

// Action requires callers to hold the application's mutation coordinator and
// reject provisional/failed configuration recovery. HTTP routing/auth/origin
// remain the server's job. The collector also serializes its own action/readback.
func (o *ServiceObserver) Action(ctx context.Context, input ServiceActionRequest) (ServiceActionResult, error) {
	result := ServiceActionResult{Service: input.Service, Action: input.Action, Snapshot: emptyServices(o.now())}
	reject := func(code string) (ServiceActionResult, error) {
		result.ErrorCode = code
		return result, ErrServiceAction
	}
	select {
	case o.gate <- struct{}{}:
	case <-ctx.Done():
		return reject(sourceCode(ctx.Err()))
	}
	defer func() { <-o.gate }()
	if !o.cached.CheckedAt.IsZero() {
		result.Snapshot = cloneServices(o.cached)
	}
	allowed, ok := fixedServiceActions[input.Service]
	if !ok || !containsServiceAction(allowed.actions, input.Action) {
		return reject("service_action_not_allowed")
	}
	if allowed.impact != "" && !input.ConfirmImpact {
		return reject("service_impact_confirmation_required")
	}
	if !o.adapter.live {
		return reject("fixture_read_only")
	}
	if !o.installedAction(input.Service) {
		return reject("service_not_installed")
	}
	// A fresh source is necessary for any operation. In particular, never turn
	// an unknown process into a Stop button or stale PID target.
	snapshot, err := o.refresh(ctx)
	result.Snapshot = snapshot
	if err != nil || snapshot.Stale {
		return reject("service_observation_unavailable")
	}
	actionAvailable := false
	for _, row := range snapshot.Services {
		if row.Name == input.Service && containsServiceAction(row.Actions, input.Action) {
			actionAvailable = true
		}
	}
	if !actionAvailable {
		return reject("service_action_not_available")
	}
	err = o.runAction(ctx, allowed.path, input.Action)
	result.CommandAccepted = err == nil
	// Bypass the five-second browser cache after every attempted command. Keep
	// failures and stale readback explicit, even when the command was accepted.
	readback, readErr := o.refresh(ctx)
	result.Snapshot = readback
	if err != nil {
		return reject("service_command_" + sourceCode(err))
	}
	if readErr != nil || readback.Stale {
		return reject("service_readback_unavailable")
	}
	return result, nil
}
func runServiceCommand(ctx context.Context, path, action string) error {
	child, cancel := context.WithTimeout(ctx, 8*time.Second)
	defer cancel()
	cmd := exec.CommandContext(child, path, action)
	cmd.WaitDelay = 100 * time.Millisecond
	output := &boundedOutput{limit: 4096}
	cmd.Stdout = output
	cmd.Stderr = io.Discard
	err := cmd.Run()
	if output.exceeded {
		return errTooLarge
	}
	if child.Err() != nil {
		return child.Err()
	}
	return err
}

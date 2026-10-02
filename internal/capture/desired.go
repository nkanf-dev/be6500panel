package capture

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"net/netip"
	"os"
	"reflect"
	"slices"

	"be6500panel/internal/proxy"
)

// Desired is saved separately from live ownership. It contains identity and
// policy only: no commands, listener ports, endpoints or last-known device IPs.
type Desired struct {
	Enabled    bool              `json:"desired"`
	Devices    []DeviceSelection `json:"devices,omitempty"`
	ClientIPv4 string            `json:"clientIPv4,omitempty"`
	ClientIPv6 string            `json:"clientIPv6,omitempty"`
	IPv6       proxy.IPv6Mode    `json:"ipv6"`
}
type DeviceSelection struct {
	MAC string `json:"mac"`
}
type Client struct {
	MAC      string `json:"mac"`
	IP       string `json:"ip"`
	Hostname string `json:"hostname"`
}

// Builder resolves current device observations and accepted native configuration.
// It must return every selected MAC in clients, including unresolved identities.
type Builder func(context.Context, Desired) (proxy.RulesPlanInput, []Client, error)

func normalizeDesired(d Desired) (Desired, error) {
	if d.IPv6 == "" {
		d.IPv6 = proxy.IPv6Direct
	}
	if !d.Enabled {
		if len(d.Devices) == 0 {
			return Desired{IPv6: d.IPv6}, nil
		}
		d.Enabled = true
		normalized, err := normalizeDesired(d)
		normalized.Enabled = false
		return normalized, err
	}
	switch d.IPv6 {
	case proxy.IPv6Direct:
	case proxy.IPv6Follow, proxy.IPv6Block:
		addr, err := netip.ParseAddr(d.ClientIPv6)
		if err != nil || !addr.Is6() || addr.Is4In6() || addr.Zone() != "" || addr.IsUnspecified() || addr.IsLoopback() || addr.IsMulticast() {
			return Desired{}, errors.New("IPv6 policy requires an explicit exact client address")
		}
		d.ClientIPv6 = addr.String()
	default:
		return Desired{}, errors.New("invalid IPv6 policy")
	}
	if len(d.Devices) > 64 {
		return Desired{}, errors.New("at most 64 selected devices")
	}
	if len(d.Devices) > 0 {
		if d.ClientIPv4 != "" {
			return Desired{}, errors.New("use devices or clientIPv4, not both")
		}
		if d.IPv6 != proxy.IPv6Direct && len(d.Devices) != 1 {
			return Desired{}, errors.New("IPv6 follow/block requires one selected device and its explicit IPv6 address")
		}
		d.Devices = slices.Clone(d.Devices)
		for i := range d.Devices {
			mac, err := net.ParseMAC(d.Devices[i].MAC)
			if err != nil || len(mac) != 6 || mac[0]&1 != 0 || slices.Equal([]byte(mac), []byte{0, 0, 0, 0, 0, 0}) {
				return Desired{}, errors.New("invalid device MAC")
			}
			d.Devices[i].MAC = mac.String()
		}
		slices.SortFunc(d.Devices, func(a, b DeviceSelection) int {
			if a.MAC < b.MAC {
				return -1
			}
			if a.MAC > b.MAC {
				return 1
			}
			return 0
		})
		d.Devices = slices.Compact(d.Devices)
	} else {
		addr, err := netip.ParseAddr(d.ClientIPv4)
		if err != nil || !addr.Is4() || addr.IsUnspecified() || addr.IsLoopback() || addr.IsMulticast() || addr.String() == "255.255.255.255" {
			return Desired{}, errors.New("select devices or one exact IPv4 client address")
		}
		d.ClientIPv4 = addr.String()
	}
	if d.IPv6 == proxy.IPv6Direct {
		d.ClientIPv6 = ""
	}
	return d, nil
}
func desiredClients(d Desired) []Client {
	clients := make([]Client, 0, len(d.Devices))
	for _, device := range d.Devices {
		clients = append(clients, Client{MAC: device.MAC})
	}
	if len(d.Devices) == 0 && d.Enabled {
		clients = append(clients, Client{IP: d.ClientIPv4})
	}
	return clients
}
func (c *Controller) loadDesired() error {
	raw, err := os.ReadFile(c.desiredPath)
	if os.IsNotExist(err) {
		return nil
	}
	if err != nil {
		return err
	}
	if len(raw) > 32<<10 {
		return errors.New("desired capture exceeds limit")
	}
	var d Desired
	if json.Unmarshal(raw, &d) != nil {
		return errors.New("desired capture invalid")
	}
	d, err = normalizeDesired(d)
	if err != nil {
		return fmt.Errorf("desired capture invalid: %w", err)
	}
	c.desired = d
	c.clients = desiredClients(d)
	return nil
}
func (c *Controller) SetBuilder(builder Builder) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.builder = builder
}
func (c *Controller) Desired() Desired {
	c.mu.Lock()
	defer c.mu.Unlock()
	d := c.desired
	d.Devices = slices.Clone(d.Devices)
	return d
}
func (c *Controller) saveDesiredLocked(d Desired) error {
	raw, err := json.Marshal(d)
	if err != nil {
		return err
	}
	if err = atomicSave(c.desiredPath, raw); err != nil {
		// A directory sync may fail after rename committed. Treat the bytes
		// actually on disk as authoritative so an off switch stays off here too.
		if accepted, readErr := os.ReadFile(c.desiredPath); readErr == nil && slices.Equal(accepted, raw) {
			c.desired = d
			c.clients = desiredClients(d)
			c.restoreError = "capture_state_not_durable"
		}
		return err
	}
	c.desired = d
	c.clients = desiredClients(d)
	c.restoreError = ""
	return nil
}

// Select saves intent even when the device is unresolved or a listener is invalid.
// A failed restore always withdraws old resources and reports suspended intent.
func (c *Controller) Select(ctx context.Context, d Desired) (Status, error) {
	d.Enabled = true
	normalized, err := normalizeDesired(d)
	if err != nil {
		return c.Status(), err
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	c.failedPlan = nil
	if err = c.saveDesiredLocked(normalized); err != nil {
		return c.statusLocked(), errors.Join(err, c.cleanupLocked(ctx))
	}
	return c.restoreLocked(ctx)
}

// Disable persists the off switch before cleanup. Even failed cleanup cannot
// silently restore the selection at the next core start or panel boot.
func (c *Controller) Disable(ctx context.Context) error {
	c.mu.Lock()
	defer c.mu.Unlock()
	if err := c.saveDesiredLocked(Desired{IPv6: c.desired.IPv6}); err != nil {
		return errors.Join(err, c.cleanupLocked(ctx))
	}
	if err := c.cleanupLocked(ctx); err != nil {
		c.restoreError = "capture_cleanup_failed"
		return err
	}
	return nil
}

// Restore is invoked only after accepted core listeners pass readiness.
func (c *Controller) Restore(ctx context.Context) (Status, error) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.failedPlan = nil
	return c.restoreLocked(ctx)
}
func (c *Controller) restoreLocked(ctx context.Context) (Status, error) {
	if err := c.cleanupLocked(ctx); err != nil {
		c.restoreError = "capture_cleanup_failed"
		return c.statusLocked(), err
	}
	if !c.desired.Enabled {
		return c.statusLocked(), nil
	}
	if c.builder == nil {
		c.restoreError = "capture_configuration_unavailable"
		return c.statusLocked(), errors.New(c.restoreError)
	}
	d := c.desired
	d.Devices = slices.Clone(d.Devices)
	input, clients, err := c.builder(ctx, d)
	c.clients = slices.Clone(clients)
	var partial *PartialScopeError
	pendingDevices := errors.As(err, &partial)
	if err != nil && !pendingDevices {
		c.restoreError = err.Error()
		return c.statusLocked(), err
	}
	_, err = c.applyLocked(ctx, input)
	if err != nil {
		if expected, compileErr := proxy.PlanOwnedRules(input); compileErr == nil {
			failed := clonePlan(expected)
			c.failedPlan = &failed
		}
		c.restoreError = "capture_activation_failed"
		return c.statusLocked(), err
	}
	c.failedPlan = nil
	if pendingDevices {
		c.restoreError = partial.Error()
	}
	return c.statusLocked(), nil
}

// ReconcileDesired only observes. The server-owned readiness-gated refresh loop
// performs any rule changes; an HTTP GET never creates or deletes resources.
func (c *Controller) ReconcileDesired(ctx context.Context) (Status, error) {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.builder == nil || (!c.desired.Enabled && len(c.desired.Devices) == 0) {
		return c.statusLocked(), nil
	}
	d := c.desired
	d.Devices = slices.Clone(d.Devices)
	input, clients, err := c.builder(ctx, d)
	c.clients = slices.Clone(clients)
	if !c.desired.Enabled {
		return c.statusLocked(), nil
	}
	var partial *PartialScopeError
	pendingDevices := errors.As(err, &partial)
	if c.plan == nil {
		if err != nil {
			c.restoreError = err.Error()
		}
		return c.statusLocked(), err
	}
	if pendingDevices {
		err = nil
	}
	if err == nil {
		expected, compileErr := proxy.PlanOwnedRules(input)
		err = compileErr
		if err == nil && !plansEqual(*c.plan, expected) {
			err = errors.New("capture_scope_changed_apply_required")
		}
	}
	if err != nil {
		c.restoreError = err.Error()
		c.active = false
		c.cleanupPending = true
		return c.statusLocked(), err
	}
	if err = c.observeLocked(ctx); err != nil {
		c.active = false
		c.restoreError = "capture_observation_failed"
		c.cleanupPending = true
		return c.statusLocked(), err
	}
	c.active = true
	c.cleanupPending = false
	c.restoreError = ""
	if pendingDevices {
		c.restoreError = partial.Error()
	}
	return c.statusLocked(), nil
}

func plansEqual(a, b proxy.OwnedRulesPlan) bool {
	return reflect.DeepEqual(a.Apply, b.Apply) && reflect.DeepEqual(a.Ownership, b.Ownership)
}

// Suspend withdraws live rules while retaining selected identities and a safe
// reason code, for example after local DNS readiness fails.
func (c *Controller) Suspend(ctx context.Context, code string) error {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.restoreError = code
	return c.cleanupLocked(ctx)
}

// DisableRetainingSelection prevents native reload/rollback auto-restoration but
// retains the checkboxes for a later explicit Apply. DELETE uses Disable instead.
func (c *Controller) DisableRetainingSelection(ctx context.Context) error {
	c.mu.Lock()
	defer c.mu.Unlock()
	desired := c.desired
	desired.Enabled = false
	if err := c.saveDesiredLocked(desired); err != nil {
		return errors.Join(err, c.cleanupLocked(ctx))
	}
	if err := c.cleanupLocked(ctx); err != nil {
		c.restoreError = "capture_cleanup_failed"
		return err
	}
	return nil
}

// Refresh runs in the runtime mutation lane after current listener readiness.
// It rebuilds only changed or missing scope. An identical failed apply is not
// retried by the timer; explicit Apply/Start or changed accepted intent retries.
func (c *Controller) Refresh(ctx context.Context) (Status, error) {
	c.mu.Lock()
	defer c.mu.Unlock()
	if !c.desired.Enabled {
		return c.statusLocked(), nil
	}
	if c.builder == nil {
		c.restoreError = "capture_configuration_unavailable"
		return c.statusLocked(), errors.New(c.restoreError)
	}
	desired := c.desired
	desired.Devices = slices.Clone(desired.Devices)
	input, clients, buildErr := c.builder(ctx, desired)
	c.clients = slices.Clone(clients)
	var partial *PartialScopeError
	pending := errors.As(buildErr, &partial)
	if buildErr != nil && !pending {
		c.restoreError = buildErr.Error()
		return c.statusLocked(), errors.Join(buildErr, c.cleanupLocked(ctx))
	}
	expected, err := proxy.PlanOwnedRules(input)
	if err != nil {
		c.restoreError = "capture_native_scope_invalid"
		return c.statusLocked(), errors.Join(err, c.cleanupLocked(ctx))
	}
	if c.plan != nil && plansEqual(*c.plan, expected) && !c.cleanupPending {
		if err = c.observeLocked(ctx); err == nil {
			c.active = true
			c.restoreError = ""
			if pending {
				c.restoreError = partial.Error()
			}
			return c.statusLocked(), nil
		}
	}
	if err = c.cleanupLocked(ctx); err != nil {
		c.restoreError = "capture_cleanup_failed"
		return c.statusLocked(), err
	}
	if c.failedPlan != nil && plansEqual(*c.failedPlan, expected) {
		c.restoreError = "capture_activation_failed_apply_required"
		return c.statusLocked(), errors.New(c.restoreError)
	}
	if _, err = c.applyLocked(ctx, input); err != nil {
		failed := clonePlan(expected)
		c.failedPlan = &failed
		c.restoreError = "capture_activation_failed_apply_required"
		return c.statusLocked(), err
	}
	c.failedPlan = nil
	if pending {
		c.restoreError = partial.Error()
	}
	return c.statusLocked(), nil
}

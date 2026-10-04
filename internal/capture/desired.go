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
	"time"

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
// Datapath and TUN fields come only from that accepted config, never Desired or
// a recovery journal. GET reconciliation only compares the resulting intent.
// Builders must honor context cancellation and deadlines. GET invokes them
// outside the controller lock, without goroutines to hide blocked callbacks.
// GET and mutation builds may overlap; synchronize callback-owned state.
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
	c.observationEpoch++
	c.builder = builder
}
func (c *Controller) Desired() Desired {
	c.mu.Lock()
	defer c.mu.Unlock()
	d := c.desired
	d.Devices = slices.Clone(d.Devices)
	return d
}
func (c *Controller) saveDesiredLocked(ctx context.Context, d Desired, recovery bool) error {
	raw, err := json.Marshal(d)
	if err != nil {
		return err
	}
	release, err := c.admitStorageLocked(ctx, temporaryStorageBytes(raw), recovery)
	if err != nil {
		return err
	}
	defer release()
	if err = atomicSave(c.desiredPath, raw); err != nil {
		// A directory sync may fail after rename committed. Read back accepted
		// bytes, but never replace a failed off latch with saved enabled intent.
		// Only a successful explicit save can clear that latch and its warning.
		if accepted, readErr := os.ReadFile(c.desiredPath); !c.disableNotPersisted && readErr == nil && slices.Equal(accepted, raw) {
			c.desired = d
			c.clients = desiredClients(d)
			c.restoreError = "capture_state_not_durable"
		}
		return err
	}
	c.desired = d
	c.clients = desiredClients(d)
	c.restoreError = ""
	c.disableNotPersisted = false
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
	c.observationEpoch++
	return c.selectLocked(ctx, normalized)
}

// Disable latches the off switch in memory before trying to persist and clean.
// Failed persistence requires an explicit retry before process restart. A saved
// off switch stays off even when cleanup fails.
func (c *Controller) Disable(ctx context.Context) error {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.observationEpoch++
	return c.disableLocked(ctx, Desired{IPv6: c.desired.IPv6})
}

// Restore is invoked only after accepted core listeners pass readiness.
func (c *Controller) Restore(ctx context.Context) (Status, error) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.observationEpoch++
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
	return c.restoreBuiltLocked(ctx, input, clients, err)
}
func (c *Controller) restoreBuiltLocked(ctx context.Context, input proxy.RulesPlanInput, clients []Client, err error) (Status, error) {
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
	return c.reconcileObservation(ctx, true)
}

const (
	observationBudget         = 3 * time.Second
	observationBuilderBudget  = time.Second
	observationResourceBudget = 2 * time.Second
)

// observationSnapshot owns all mutable values used by an unlocked GET. The
// original plan pointer is an identity token only; reads use the cloned plan.
// The callbacks must honor their context. No abandoned goroutine is used to
// pretend that an uncooperative callback has a hard execution deadline.
type observationSnapshot struct {
	epoch               uint64
	desired             Desired
	builder             Builder
	planIdentity        *proxy.OwnedRulesPlan
	failedPlanIdentity  *proxy.OwnedRulesPlan
	inspector           *Controller
	status              Status
	cleanupFailed       bool
	disableNotPersisted bool
	restoreError        string
}

func (c *Controller) observationSnapshotLocked() observationSnapshot {
	d := c.desired
	d.Devices = slices.Clone(d.Devices)
	snapshot := observationSnapshot{
		epoch: c.observationEpoch, desired: d, builder: c.builder,
		planIdentity: c.plan, failedPlanIdentity: c.failedPlan,
		status: c.statusLocked(), cleanupFailed: c.cleanupFailed,
		disableNotPersisted: c.disableNotPersisted, restoreError: c.restoreError,
	}
	if c.plan != nil {
		plan := clonePlan(*c.plan)
		// This private inspector has no storage paths, desired intent or live
		// controller ownership. Only observeLocked may use it, never mutation.
		snapshot.inspector = &Controller{plan: &plan, runner: c.runner, tableNames: c.tableNames}
	}
	return snapshot
}

func (c *Controller) observationMatchesLocked(snapshot observationSnapshot) bool {
	return c.observationEpoch == snapshot.epoch && c.plan == snapshot.planIdentity &&
		c.failedPlan == snapshot.failedPlanIdentity && reflect.DeepEqual(c.desired, snapshot.desired) &&
		c.cleanupFailed == snapshot.cleanupFailed && c.disableNotPersisted == snapshot.disableNotPersisted &&
		c.restoreError == snapshot.restoreError && reflect.DeepEqual(c.statusLocked(), snapshot.status)
}

// A busy mutation lane does not prove either activity or clean absence. This
// response is temporary only: it never sets the actual-cleanup failure latch.
func busyObservation(state Status) (Status, error) {
	state.Active = false
	state.CleanupPending = true
	state.State = "cleanup-pending"
	if state.Error != "capture_disable_not_persisted" && state.Error != "capture_cleanup_failed" {
		state.Error = "capture_observation_busy"
	}
	if state.Clients == nil {
		state.Clients = []Client{}
	}
	return state, errors.New("capture_observation_busy")
}

// Both GET entry points use short, nonblocking lock sections. Scope resolution
// has a one-second context budget; installed proof gets a separate two-second
// context after a failed build, within the total three-second context budget.
func (c *Controller) reconcileObservation(ctx context.Context, desiredScope bool) (Status, error) {
	if ctx == nil {
		ctx = context.Background()
	}
	ctx, cancel := context.WithTimeout(ctx, observationBudget)
	defer cancel()
	if !c.mu.TryLock() {
		return busyObservation(Status{})
	}
	snapshot := c.observationSnapshotLocked()
	c.mu.Unlock()

	clients := slices.Clone(snapshot.status.Clients)
	scopeState := ""
	var scopeErr error
	var partial *PartialScopeError
	if desiredScope && (snapshot.desired.Enabled || len(snapshot.desired.Devices) > 0) {
		if snapshot.builder == nil {
			clients = desiredClients(snapshot.desired)
			if snapshot.desired.Enabled {
				scopeErr = errors.New("capture_configuration_unavailable")
			}
		} else {
			d := snapshot.desired
			d.Devices = slices.Clone(d.Devices)
			buildCtx, stopBuild := context.WithTimeout(ctx, observationBuilderBudget)
			input, builtClients, buildErr := snapshot.builder(buildCtx, d)
			// An expired callback is never fresh scope proof, even if it
			// accidentally returns nil. Retain previous selected observations.
			if contextErr := buildCtx.Err(); contextErr != nil {
				buildErr = errors.Join(buildErr, contextErr)
			} else {
				clients = slices.Clone(builtClients)
			}
			stopBuild()
			if snapshot.desired.Enabled {
				pendingDevices := errors.As(buildErr, &partial)
				if buildErr != nil && (!pendingDevices || errors.Is(buildErr, context.Canceled) || errors.Is(buildErr, context.DeadlineExceeded)) {
					scopeErr = buildErr
				} else if expected, err := proxy.PlanOwnedRules(input); err != nil {
					scopeErr = errors.New("capture_native_scope_invalid")
				} else if snapshot.inspector != nil {
					scopeState = "current"
					if !plansEqual(*snapshot.inspector.plan, expected) {
						scopeState = "changed"
						scopeErr = errors.New("capture_scope_changed_apply_required")
					}
				}
			}
		}
		if snapshot.desired.Enabled && scopeErr != nil && scopeState == "" {
			scopeState = "unresolved"
		}
	}

	// Scope failure cannot prove old resources gone. Always attempt saved
	// resource reads, with the budget reserved separately from the builder.
	var observationErr error
	if snapshot.inspector != nil {
		resourceCtx, stopReads := context.WithTimeout(ctx, observationResourceBudget)
		observationErr = snapshot.inspector.observeLocked(resourceCtx)
		observationErr = errors.Join(observationErr, resourceCtx.Err())
		stopReads()
	}

	if !c.mu.TryLock() {
		return busyObservation(snapshot.status)
	}
	defer c.mu.Unlock()
	if !c.observationMatchesLocked(snapshot) {
		// Disable, Select or another mutation/observation owns newer state.
		// Do not publish old clients, resource proof, warnings or uncertainty.
		return c.statusLocked(), nil
	}
	if desiredScope {
		c.clients = clients
		c.scopeState = scopeState
	}
	stickyCleanupError := c.cleanupFailed && c.restoreError == "capture_cleanup_failed"
	if snapshot.inspector == nil {
		if !stickyCleanupError {
			if scopeErr != nil {
				c.restoreError = scopeErr.Error()
			} else if partial != nil {
				c.restoreError = partial.Error()
			}
		}
		if scopeErr == nil && partial != nil {
			return c.statusLocked(), partial
		}
		return c.statusLocked(), scopeErr
	}
	c.active = observationErr == nil
	c.cleanupPending = observationErr != nil || c.cleanupFailed
	if !stickyCleanupError {
		switch {
		case scopeErr != nil:
			c.restoreError = scopeErr.Error()
		case observationErr != nil:
			c.restoreError = "capture_observation_failed"
		case desiredScope && c.desired.Enabled && !c.cleanupFailed:
			c.restoreError = ""
			if partial != nil {
				c.restoreError = partial.Error()
			}
		case c.restoreError == "capture_observation_failed":
			c.restoreError = ""
		}
	}
	return c.statusLocked(), errors.Join(scopeErr, observationErr)
}

func plansEqual(a, b proxy.OwnedRulesPlan) bool {
	return reflect.DeepEqual(a.Apply, b.Apply) && reflect.DeepEqual(a.Ownership, b.Ownership)
}

// Suspend withdraws live rules while retaining selected identities and a safe
// reason code, for example after local DNS readiness fails.
func (c *Controller) Suspend(ctx context.Context, code string) error {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.observationEpoch++
	c.restoreError = code
	return c.cleanupLocked(ctx)
}

// DisableRetainingSelection prevents native reload/rollback auto-restoration but
// retains the checkboxes for a later explicit Apply. DELETE uses Disable instead.
func (c *Controller) DisableRetainingSelection(ctx context.Context) error {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.observationEpoch++
	desired := c.desired
	desired.Enabled = false
	return c.disableLocked(ctx, desired)
}

// Refresh runs in the runtime mutation lane after current listener readiness.
// It rebuilds only changed or missing scope. An identical failed apply is not
// retried by the timer; explicit Apply/Start or changed accepted intent retries.
func (c *Controller) Refresh(ctx context.Context) (Status, error) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.observationEpoch++
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
			c.scopeState = "current"
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

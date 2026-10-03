package capture

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"path/filepath"
	"time"

	"be6500panel/internal/proxy"
	"be6500panel/internal/storage"
)

// SetStorageAdmission shares persistent headroom with other panel owners.
// New remains read-only and backward-compatible. Set this before mutations.
func (c *Controller) SetStorageAdmission(admission storage.Admission) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.storageAdmission = admission
}
func temporaryStorageBytes(raw []byte) int64 { return int64(len(raw)) + 4096 }
func (c *Controller) admitStorageLocked(ctx context.Context, size int64, recovery bool) (func(), error) {
	if ctx == nil {
		return nil, errors.New("capture storage context required")
	}
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	if c.storageAdmission == nil || c.storageReserved {
		return func() {}, nil
	}
	release, err := c.storageAdmission(ctx, filepath.Dir(c.path), size, recovery)
	if err != nil {
		if release != nil {
			release()
		}
		return nil, err
	}
	if release == nil {
		release = func() {}
	}
	if err := ctx.Err(); err != nil {
		release()
		return nil, err
	}
	return release, nil
}

// Selection admits every temporary file together before changing saved scope.
// The builder is evaluated once so sized bytes and applied bytes cannot diverge.
func (c *Controller) selectLocked(ctx context.Context, desired Desired) (Status, error) {
	desiredRaw, err := json.Marshal(desired)
	if err != nil {
		return c.statusLocked(), err
	}
	clients := desiredClients(desired)
	var input proxy.RulesPlanInput
	var buildErr error
	if c.builder == nil {
		buildErr = errors.New("capture_configuration_unavailable")
	} else {
		input, clients, buildErr = c.builder(ctx, desired)
	}
	input = cloneInput(input)
	total := temporaryStorageBytes(desiredRaw)
	var partial *PartialScopeError
	if buildErr == nil || errors.As(buildErr, &partial) {
		plan, compileErr := proxy.PlanOwnedRules(input)
		if compileErr != nil {
			buildErr = errors.New("capture_native_scope_invalid")
		} else {
			raw, marshalErr := json.Marshal(journal{OwnedRulesPlan: clonePlan(plan), Input: &input})
			if marshalErr != nil {
				return c.statusLocked(), marshalErr
			}
			total += temporaryStorageBytes(raw)
		}
	}
	release, err := c.admitStorageLocked(ctx, total, false)
	if err != nil {
		return c.statusLocked(), err
	}
	defer release()
	c.storageReserved = true
	defer func() { c.storageReserved = false }()
	c.failedPlan = nil
	if err = c.saveDesiredLocked(ctx, desired, false); err != nil {
		return c.statusLocked(), errors.Join(err, c.cleanupLocked(ctx))
	}
	if err = c.cleanupLocked(ctx); err != nil {
		c.restoreError = "capture_cleanup_failed"
		return c.statusLocked(), err
	}
	return c.restoreBuiltLocked(ctx, input, clients, buildErr)
}

func (c *Controller) disableLocked(ctx context.Context, desired Desired) error {
	// Latch off before any admission or write. The last saved intent may still
	// be enabled if storage fails, but no in-process refresh may restore it.
	// Keep the prior selection and observations until the write succeeds.
	c.desired.Enabled = false
	c.disableNotPersisted = true
	c.failedPlan = nil
	raw, persistErr := json.Marshal(desired)
	if persistErr == nil {
		var release func()
		release, persistErr = c.admitStorageLocked(ctx, temporaryStorageBytes(raw), true)
		if persistErr == nil {
			defer release()
			c.storageReserved = true
			defer func() { c.storageReserved = false }()
			persistErr = c.saveDesiredLocked(ctx, desired, true)
		}
	}
	// A canceled HTTP request must not abandon owned hooks. Like failed Apply
	// rollback, cleanup uses an independent, bounded context.
	cleanupCtx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	cleanupErr := c.cleanupLocked(cleanupCtx)
	if persistErr != nil {
		return errors.Join(fmt.Errorf("capture_disable_not_persisted: retry disable before process restart: %w", persistErr), cleanupErr)
	}
	if cleanupErr != nil {
		c.restoreError = "capture_cleanup_failed"
	}
	return cleanupErr
}

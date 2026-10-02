package main

import (
	"context"
	"log/slog"
	"time"

	"be6500panel/internal/capture"
	"be6500panel/internal/control"
	managedruntime "be6500panel/internal/runtime"
)

// refreshDesiredCapture follows already explicitly authorized MAC selection.
// Deployment must disable desired capture before restarting the panel if rule
// activation has not been authorized. Disabled/retained checkboxes never apply.
func refreshDesiredCapture(ctx context.Context, manager *managedruntime.Manager, controller *capture.Controller, configuration *control.Manager, logger *slog.Logger) {
	ticker := time.NewTicker(5 * time.Second)
	defer ticker.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
		}
		if !controller.Status().Desired {
			continue
		}
		state, err := manager.Status(managedruntime.SingBox)
		if err != nil || state.State != managedruntime.Running {
			continue
		}
		refreshCtx, cancel := context.WithTimeout(ctx, 30*time.Second)
		err = manager.ReadyOperation(refreshCtx, managedruntime.SingBox, func(ctx context.Context) error {
			if !controller.Status().Desired {
				return nil
			}
			if configuration != nil {
				status := configuration.Status()
				if !status.Enabled || status.ErrorCode != "" || status.PendingCommit != nil {
					return controller.Suspend(ctx, "capture_configuration_pending")
				}
			}
			_, err := controller.Refresh(ctx)
			return err
		})
		cancel()
		if err != nil {
			logger.Debug("Desired capture refresh suspended", "module", "proxy", "code", "capture_refresh_suspended")
		}
	}
}

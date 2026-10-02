package main

import (
	"context"
	"encoding/json"
	"log/slog"
	"os"
	"path/filepath"
	"time"

	managedruntime "be6500panel/internal/runtime"
)

func restoreDesiredRuntimes(ctx context.Context, manager *managedruntime.Manager, dataDir string, logger *slog.Logger) {
	raw, err := os.ReadFile(filepath.Join(dataDir, "desired-services.json"))
	if err != nil {
		return
	}
	var desired map[string]bool
	if json.Unmarshal(raw, &desired) != nil {
		return
	}
	for _, service := range []string{managedruntime.SingBox, managedruntime.FRPC} {
		if !desired[service] {
			continue
		}
		raw, err = os.ReadFile(filepath.Join(dataDir, "services", service, "state.json"))
		if err != nil {
			continue
		}
		var state struct {
			Artifact *managedruntime.Artifact `json:"artifact"`
		}
		if json.Unmarshal(raw, &state) != nil || state.Artifact == nil {
			continue
		}
		attempt := time.NewTimer(0)
		delay := 2 * time.Second
		for count := 0; count < 8; count++ {
			select {
			case <-ctx.Done():
				attempt.Stop()
				return
			case <-attempt.C:
			}
			stateResult, err := manager.Acquire(ctx, service, *state.Artifact)
			if err == nil && stateResult.Configured {
				_, err = manager.Start(ctx, service)
			}
			if err == nil {
				logger.Info("Runtime restored", "module", service, "code", "runtime_restored")
				break
			}
			logger.Warn("Runtime rebuilding", "module", service, "code", "runtime_rebuilding", "attempt", count+1)
			attempt.Reset(delay)
			if delay < time.Minute {
				delay *= 2
				if delay > time.Minute {
					delay = time.Minute
				}
			}
		}
		attempt.Stop()
	}
}

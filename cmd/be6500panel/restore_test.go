package main

import (
	"context"
	"io"
	"log/slog"
	"testing"
)

func TestNoDesiredServicesDoesNotAcquire(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	restoreDesiredRuntimes(ctx, nil, t.TempDir(), slog.New(slog.NewTextHandler(io.Discard, nil)))
}

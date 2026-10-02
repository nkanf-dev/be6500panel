// be6500panel runs one read-only HTTP control-plane process. It never launches
// shells, proxy/tunnel runtimes or router commands.
package main

import (
	"context"
	"errors"
	"flag"
	"fmt"
	"log/slog"
	"net"
	"net/http"
	"os"
	"os/signal"
	"strings"
	"syscall"
	"time"

	"be6500panel/internal/core"
	"be6500panel/internal/httpapi"
	"be6500panel/internal/modules"
)

func main() {
	if err := run(); err != nil {
		slog.Error("Server stopped with error", "code", "startup_failed", "module", "core", "error", err)
		os.Exit(1)
	}
}
func run() error {
	listen := flag.String("listen", "127.0.0.1:8787", "HTTP bind address; non-loopback requires BE6500PANEL_PASSWORD")
	demo := flag.Bool("demo", false, "Use explicitly labeled deterministic system samples (network still observes the host)")
	webDir := flag.String("web-dir", "web/dist", "Prebuilt browser assets; API works without them")
	flag.Parse()
	logs := &core.LogBuffer{}
	logger := slog.New(core.NewRingHandler(slog.NewJSONHandler(os.Stderr, nil), logs))
	slog.SetDefault(logger)
	password := os.Getenv("BE6500PANEL_PASSWORD")
	if err := validateListen(*listen, password); err != nil {
		return err
	}
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	observer := modules.NewSystem(*demo)
	// Log observation state transitions, not every periodic sample or raw metric.
	lastFailed := false
	observation := func(ctx context.Context) (core.SystemStatus, error) {
		status, err := observer.Observe(ctx)
		if err != nil && !lastFailed {
			logger.Warn("System observation unavailable", "code", "observation_unavailable", "module", "system")
			lastFailed = true
		}
		if err == nil && lastFailed {
			logger.Info("System observation restored", "code", "observation_restored", "module", "system")
			lastFailed = false
		}
		return status, err
	}
	sampler := core.NewSampler(observation, 2*time.Second)
	sampler.Start(ctx)
	defer sampler.Close()
	api, err := httpapi.New(httpapi.Config{System: observer, Network: modules.Network{}, Sampler: sampler, Password: password, WebDir: *webDir, Logger: logger, Logs: logs})
	if err != nil {
		return err
	}
	defer api.Close()
	registry, err := modules.Builtins(observer, modules.Network{})
	if err != nil {
		return err
	}
	for _, module := range registry.Modules() {
		logger.Info("Module registered: "+module.State, "code", "module_registered", "module", module.ID, "state", module.State)
	}
	listener, err := net.Listen("tcp", *listen)
	if err != nil {
		return fmt.Errorf("HTTP bind failed: %w", err)
	}
	server := &http.Server{Handler: api, ReadHeaderTimeout: 5 * time.Second, ReadTimeout: 10 * time.Second, IdleTimeout: 60 * time.Second, MaxHeaderBytes: 16 * 1024, BaseContext: func(net.Listener) context.Context { return ctx }}
	stopped := make(chan error, 1)
	go func() { stopped <- server.Serve(listener) }()
	logger.Info(fmt.Sprintf("HTTP server started: %s, read-only, authRequired=%t", observer.Mode(), password != ""), "code", "server_started", "module", "core", "listen", *listen, "mode", observer.Mode(), "readOnly", true, "authRequired", password != "")
	select {
	case err := <-stopped:
		if !errors.Is(err, http.ErrServerClosed) {
			return err
		}
		return nil
	case <-ctx.Done():
		logger.Info("HTTP server stopping", "code", "server_stopping", "module", "core")
		api.Close() // Explicitly cancel long-lived SSE before graceful HTTP shutdown.
		shutdownCtx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		if err := server.Shutdown(shutdownCtx); err != nil {
			_ = server.Close()
			return err
		}
		err := <-stopped
		if !errors.Is(err, http.ErrServerClosed) {
			return err
		}
		return nil
	}
}
func validateListen(address, password string) error {
	host, port, err := net.SplitHostPort(address)
	if err != nil || port == "" {
		return fmt.Errorf("--listen must be host:port")
	}
	loopback := strings.EqualFold(host, "localhost")
	if ip := net.ParseIP(host); ip != nil {
		loopback = ip.IsLoopback()
	}
	// Only explicit numeric loopback addresses or localhost are trusted. No DNS
	// resolution is used to bypass the non-loopback authentication requirement.
	if !loopback && password == "" {
		return fmt.Errorf("non-loopback --listen requires BE6500PANEL_PASSWORD")
	}
	return nil
}

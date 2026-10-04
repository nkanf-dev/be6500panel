// be6500panel runs a modular router control plane with explicit commit and runtime operations.
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

	"be6500panel/internal/capture"
	"be6500panel/internal/control"
	"be6500panel/internal/core"
	"be6500panel/internal/deviceannotations"
	"be6500panel/internal/devicetelemetry"
	"be6500panel/internal/httpapi"
	"be6500panel/internal/maintenance"
	"be6500panel/internal/modules"
	"be6500panel/internal/proxy"
	"be6500panel/internal/requesttrace"
	"be6500panel/internal/router"
	managedruntime "be6500panel/internal/runtime"
	"be6500panel/internal/storage"
	"be6500panel/internal/telemetry"
	"be6500panel/internal/traffic"
	"be6500panel/internal/transport"
	"path/filepath"
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
	dataDir := flag.String("data-dir", "", "Persistent private settings directory; enables runtime management")
	runDir := flag.String("run-dir", "/tmp/be6500panel/runtime", "Volatile runtime artifact directory")
	controlEnabled := flag.Bool("enable-control", false, "Enable staged UCI configuration commits")
	adapterRoot := flag.String("router-root", "/", "Router observation/configuration root")
	localArtifacts := flag.String("local-artifacts", "", "Trusted local artifact source directory")
	artifactTransport := flag.String("artifact-transport", "native", "Artifact HTTPS transport: native or curl")
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
	routerAdapter := router.New(*adapterRoot)
	var runtimeManager *managedruntime.Manager
	var controlManager *control.Manager
	var captureManager *capture.Controller
	var err error
	var artifactClient *http.Client
	var flashBudget *storage.Budget
	if *dataDir != "" {
		if err := os.MkdirAll(*dataDir, 0700); err != nil {
			return err
		}
		flashBudget, err = storage.New(storage.Options{EmergencyPath: filepath.Join(*dataDir, ".rollback-reserve")})
		if err != nil {
			return err
		}
		if err = flashBudget.Replenish(ctx); err != nil {
			logger.Warn("Rollback space reserve unavailable", "code", "storage_reserve_unavailable", "module", "core")
		}
	}
	if *artifactTransport == "curl" {
		artifactClient = &http.Client{Transport: transport.CurlTransport{}}
	} else if *artifactTransport != "native" {
		return fmt.Errorf("unknown artifact transport")
	}
	if *dataDir != "" {
		if password == "" {
			return fmt.Errorf("runtime control requires BE6500PANEL_PASSWORD")
		}
		captureManager, err = capture.New(*dataDir, nil)
		if err != nil {
			return err
		}
		captureManager.SetStorageAdmission(flashBudget.Admit)
		runtimeManager, err = managedruntime.New(managedruntime.Options{DataDir: filepath.Join(*dataDir, "services"), RunDir: *runDir, Logger: logger, LocalSourceRoot: *localArtifacts, HTTPClient: artifactClient, StorageAdmission: flashBudget.Admit, MaxConfigBytes: 512 << 10, MaxCompressedBytes: 20 << 20, DownloadTimeout: 6 * time.Minute, MaxUncompressedBytes: 40 << 20, PreStartHook: func(ctx context.Context, id string, raw []byte) error {
			if id != managedruntime.SingBox {
				return nil
			}
			return checkNativeTUNPreStart(ctx, raw)
		}, ReadyHook: func(ctx context.Context, id string) error {
			err := runtimeReadiness(func() *managedruntime.Manager { return runtimeManager })(ctx, id)
			if err != nil && id == managedruntime.SingBox {
				withdrawCtx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
				defer cancel()
				return errors.Join(err, captureManager.Suspend(withdrawCtx, "capture_readiness_failed"))
			}
			return err
		}, ReadyTimeout: 15 * time.Second, RestoreHook: func(ctx context.Context, id string) error {
			if id != managedruntime.SingBox {
				return nil
			}
			if controlManager != nil {
				status := controlManager.Status()
				if !status.Enabled || status.ErrorCode != "" || status.PendingCommit != nil {
					return captureManager.Suspend(ctx, "capture_configuration_pending")
				}
			}
			_, err := captureManager.Restore(ctx)
			return err
		}, CleanupHook: func(ctx context.Context, id string) error {
			if id == managedruntime.SingBox {
				return captureManager.Cleanup(ctx)
			}
			return nil
		}})
		if err != nil {
			return err
		}
		defer runtimeManager.Close()
		// Exclusive runtime store admission must precede boot network mutations.
		// A second panel that cannot own .manager.lock never withdraws live hooks.
		withdrawCtx, cancelWithdraw := context.WithTimeout(ctx, 30*time.Second)
		withdrawErr := captureManager.Cleanup(withdrawCtx)
		cancelWithdraw()
		if withdrawErr != nil {
			logger.Warn("Boot capture cleanup pending", "code", "capture_cleanup_failed", "module", "proxy")
		}
		captureManager.SetBuilder(func(ctx context.Context, desired capture.Desired) (proxy.RulesPlanInput, []capture.Client, error) {
			clients := make([]capture.Client, 0, len(desired.Devices))
			for _, device := range desired.Devices {
				clients = append(clients, capture.Client{MAC: device.MAC})
			}
			raw, generation, err := runtimeManager.Config(managedruntime.SingBox)
			if err != nil {
				return proxy.RulesPlanInput{}, clients, errors.New("capture_configuration_unavailable")
			}
			if err = checkOwnedNativeReadiness(ctx, raw, func() (managedruntime.Status, error) {
				state, err := runtimeManager.Status(managedruntime.SingBox)
				if err != nil || state.Generation != generation {
					return state, errors.New("capture accepted generation changed")
				}
				return state, nil
			}); err != nil {
				return proxy.RulesPlanInput{}, clients, errors.New("capture_backend_not_ready")
			}
			observation, err := routerAdapter.CaptureObservation(ctx)
			if err != nil {
				return proxy.RulesPlanInput{}, clients, errors.New("capture_devices_unavailable")
			}
			return capture.BuildFromAccepted(ctx, desired, raw, observation, capture.ResolveEndpoints)
		})
	}
	if *controlEnabled {
		if *dataDir == "" || password == "" {
			return fmt.Errorf("configuration control requires data-dir and password")
		}
		controlManager, err = control.New(control.Options{Root: *adapterRoot, DataDir: filepath.Join(*dataDir, "configuration"), Logger: logger, ConfirmationTimeout: 120 * time.Second, StorageAdmission: flashBudget.Admit, PreserveLANManagement: true})
		if err != nil {
			return err
		}
		defer controlManager.Close()
	}
	var history *traffic.Collector
	historyError := ""
	if *dataDir != "" {
		history, err = traffic.New(traffic.Options{DataDir: filepath.Join(*dataDir, "traffic"), Source: router.NewWANSource(routerAdapter), StorageAdmission: flashBudget.Admit})
		if err != nil {
			historyError = "历史存储未就绪；请检查持久存储空间和历史文件，实时状态仍可使用。"
			logger.Warn("Persistent traffic history unavailable", "code", "history_storage_unavailable", "module", "network")
		} else {
			history.Start(ctx)
			defer func() {
				if err := history.Close(); err != nil {
					logger.Warn("Traffic history final sync failed", "code", "history_sync_failed", "module", "network")
				}
			}()
		}
	}
	var metrics *telemetry.Collector
	if runtimeManager != nil {
		metrics, err = telemetry.New(telemetry.Options{Config: func(ctx context.Context) (telemetry.CoreConfig, error) {
			if err := ctx.Err(); err != nil {
				return telemetry.CoreConfig{}, err
			}
			state, err := runtimeManager.Status(managedruntime.SingBox)
			if err != nil || state.State != managedruntime.Running || state.NeedsRecovery || state.ErrorCode != "" {
				return telemetry.CoreConfig{}, telemetry.ErrUnavailable
			}
			raw, generation, err := runtimeManager.Config(managedruntime.SingBox)
			if err != nil || generation != state.Generation {
				return telemetry.CoreConfig{}, telemetry.ErrUnavailable
			}
			after, err := runtimeManager.Status(managedruntime.SingBox)
			if err != nil || after.State != managedruntime.Running || after.NeedsRecovery || after.ErrorCode != "" || after.Generation != generation || after.PID != state.PID {
				return telemetry.CoreConfig{}, telemetry.ErrUnavailable
			}
			config, err := telemetry.FromNativeConfig(raw)
			config.Epoch = fmt.Sprintf("%d:%d", generation, state.PID)
			return config, err
		}})
		if err != nil {
			return err
		}
		metrics.Start(ctx)
		defer metrics.Close()
	}
	deviceActivity, err := devicetelemetry.New(router.NewDeviceSource(routerAdapter))
	if err != nil {
		return err
	}
	deviceActivity.Start(ctx)
	defer deviceActivity.Close()
	var diagnosticProxy requesttrace.ProxyProvider
	if runtimeManager != nil {
		diagnosticProxy = acceptedDiagnosticProxy(runtimeManager, routerAdapter)
	}
	requestTraces := requesttrace.New(requesttrace.Config{ProxyProvider: diagnosticProxy})
	serviceObserver := router.NewServiceObserver(*adapterRoot)
	var deviceNames *deviceannotations.Store
	if *dataDir != "" {
		deviceNames, err = deviceannotations.New(deviceannotations.Options{DataDir: *dataDir, Context: ctx, StorageAdmission: flashBudget.Admit})
		if err != nil {
			logger.Warn("Device annotation storage unavailable; existing state retained", "code", "annotations_storage_unavailable", "module", "devices")
		}
	}
	var backupService *maintenance.Service
	if controlManager != nil {
		backupService = maintenance.New(maintenance.Options{
			Native: controlManager,
			Metadata: func(ctx context.Context) (maintenance.Metadata, error) {
				snapshot, err := routerAdapter.Snapshot(ctx)
				if err != nil {
					return maintenance.Metadata{}, err
				}
				if snapshot.Platform.Model == "" || snapshot.Platform.Firmware == "" {
					return maintenance.Metadata{}, errors.New("platform metadata unavailable")
				}
				return maintenance.Metadata{Model: snapshot.Platform.Model, Build: snapshot.Platform.Firmware}, nil
			},
			Runtime: func(ctx context.Context, service string) (maintenance.RuntimeDocument, error) {
				if err := ctx.Err(); err != nil {
					return maintenance.RuntimeDocument{}, err
				}
				if runtimeManager == nil {
					return maintenance.RuntimeDocument{}, errors.New("runtime unavailable")
				}
				raw, generation, err := runtimeManager.Config(service)
				return maintenance.RuntimeDocument{Content: string(raw), Generation: generation}, err
			},
		})
	}
	api, err := httpapi.New(httpapi.Config{System: observer, Network: modules.Network{}, Sampler: sampler, Password: password, WebDir: *webDir, Logger: logger, Logs: logs, Router: routerAdapter, Runtime: runtimeManager, Control: controlManager, DataDir: *dataDir, Capture: captureManager, Traffic: history, TrafficError: historyError, Telemetry: metrics, DeviceTelemetry: deviceActivity, RequestTraces: requestTraces, Services: serviceObserver, DeviceAnnotations: deviceNames, Maintenance: backupService, StorageAdmission: flashBudget.Admit})
	if err != nil {
		return err
	}
	defer api.Close()
	if runtimeManager != nil {
		go restoreDesiredRuntimes(ctx, runtimeManager, *dataDir, logger)
		go refreshDesiredCapture(ctx, runtimeManager, captureManager, controlManager, logger)
	}
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
	logger.Info(fmt.Sprintf("HTTP server started: %s, control=%t, authRequired=%t", observer.Mode(), controlManager != nil, password != ""), "code", "server_started", "module", "core", "listen", *listen, "mode", observer.Mode(), "readOnly", controlManager == nil, "authRequired", password != "")
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

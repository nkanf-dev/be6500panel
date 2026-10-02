// Package httpapi exposes typed JSON, bounded SSE and optional prebuilt assets.
package httpapi

import (
	"context"
	"fmt"
	"log/slog"
	"net/http"
	"net/url"
	"sync"
	"time"

	"be6500panel/internal/capture"
	"be6500panel/internal/control"
	"be6500panel/internal/core"
	"be6500panel/internal/modules"
	"be6500panel/internal/router"
	managedruntime "be6500panel/internal/runtime"
	"be6500panel/internal/telemetry"
	"be6500panel/internal/traffic"
)

type Config struct {
	System       *modules.System
	Network      modules.Network
	Sampler      *core.Sampler
	Password     string
	WebDir       string
	Heartbeat    time.Duration
	Logger       *slog.Logger
	Logs         *core.LogBuffer
	Router       *router.Adapter
	Runtime      *managedruntime.Manager
	Control      *control.Manager
	DataDir      string
	Capture      *capture.Controller
	Traffic      *traffic.Collector
	TrafficError string
	Telemetry    *telemetry.Collector
}
type Server struct {
	system       *modules.System
	network      modules.Network
	sampler      *core.Sampler
	registry     *core.Registry
	coordinator  *core.Coordinator
	auth         *auth
	heartbeat    time.Duration
	logger       *slog.Logger
	logs         *core.LogBuffer
	static       http.Handler
	ctx          context.Context
	cancel       context.CancelFunc
	closeOnce    sync.Once
	router       *router.Adapter
	runtime      *managedruntime.Manager
	control      *control.Manager
	dataDir      string
	proxyState   *proxyState
	capture      *capture.Controller
	traffic      *traffic.Collector
	trafficError string
	telemetry    *telemetry.Collector
	desiredMu    sync.Mutex
}

func New(cfg Config) (*Server, error) {
	if cfg.System == nil || cfg.Sampler == nil {
		return nil, fmt.Errorf("system observer and sampler are required")
	}
	registry, err := modules.Builtins(cfg.System, cfg.Network)
	if err != nil {
		return nil, err
	}
	if cfg.Heartbeat <= 0 {
		cfg.Heartbeat = 15 * time.Second
	}
	if cfg.Logs == nil {
		cfg.Logs = &core.LogBuffer{}
	}
	if cfg.Logger == nil {
		cfg.Logger = slog.New(core.NewRingHandler(slog.Default().Handler(), cfg.Logs))
	}
	ctx, cancel := context.WithCancel(context.Background())
	return &Server{system: cfg.System, network: cfg.Network, sampler: cfg.Sampler, registry: registry, coordinator: core.NewCoordinator(), auth: newAuth(cfg.Password), heartbeat: cfg.Heartbeat, logger: cfg.Logger, logs: cfg.Logs, router: cfg.Router, runtime: cfg.Runtime, control: cfg.Control, dataDir: cfg.DataDir, proxyState: newProxyState(cfg.DataDir), capture: cfg.Capture, traffic: cfg.Traffic, trafficError: cfg.TrafficError, telemetry: cfg.Telemetry, static: staticHandler(cfg.WebDir), ctx: ctx, cancel: cancel}, nil
}
func (s *Server) Close() { s.closeOnce.Do(func() { s.cancel(); s.sampler.Close() }) }
func (s *Server) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("X-Content-Type-Options", "nosniff")
	w.Header().Set("Referrer-Policy", "same-origin")
	if r.URL.Path != "/api" && (len(r.URL.Path) < 5 || r.URL.Path[:5] != "/api/") {
		s.static.ServeHTTP(w, r)
		return
	}
	w.Header().Set("Cache-Control", "no-store")
	if !safeMethod(r.Method) && !sameOrigin(r) {
		s.logger.Warn("Unsafe origin rejected", "code", "origin_rejected", "module", "core")
		fail(w, 403, "origin_rejected", "Unsafe requests must be same-origin.")
		return
	}
	method, exists := routes[r.URL.Path]
	if !exists {
		fail(w, 404, "not_found", "Unknown API path.")
		return
	}
	allowedMethod := method == r.Method || (r.URL.Path == "/api/configuration/drafts" && r.Method == "DELETE") || (r.URL.Path == "/api/proxy/capture" && (r.Method == "POST" || r.Method == "DELETE"))
	if !allowedMethod {
		w.Header().Set("Allow", method)
		fail(w, 405, "method_not_allowed", "Method is not allowed for this endpoint.")
		return
	}
	public := r.URL.Path == "/api/session" || r.URL.Path == "/api/session/login" || r.URL.Path == "/api/session/logout"
	if !public && !s.auth.authenticated(r) {
		fail(w, 401, "unauthenticated", "Authentication required.")
		return
	}
	switch r.URL.Path {
	case "/api/router":
		s.routerSnapshot(w, r)
	case "/api/runtime":
		s.runtimes(w, r)
	case "/api/runtime/acquire":
		s.runtimeAcquire(w, r)
	case "/api/runtime/configure":
		s.runtimeConfigure(w, r)
	case "/api/runtime/config":
		s.runtimeConfig(w, r)
	case "/api/runtime/start":
		s.runtimeAction(w, r, true)
	case "/api/runtime/stop":
		s.runtimeAction(w, r, false)
	case "/api/runtime/restore":
		s.runtimeRestore(w, r)
	case "/api/proxy/nodes":
		s.proxyNodes(w, r)
	case "/api/proxy/import":
		s.proxyImport(w, r)
	case "/api/proxy/select":
		s.proxySelect(w, r)
	case "/api/proxy/capture":
		s.proxyCapture(w, r)
	case "/api/proxy/metrics":
		s.proxyMetrics(w, r)
	case "/api/proxy/probe":
		s.proxyProbe(w, r)
	case "/api/traffic/history":
		s.trafficHistory(w, r)
	case "/api/configuration":
		s.configDocuments(w, r)
	case "/api/configuration/stage":
		s.configStage(w, r)
	case "/api/configuration/drafts":
		s.configDrafts(w, r)
	case "/api/configuration/commit":
		s.configCommit(w, r)
	case "/api/configuration/confirm":
		s.configConfirm(w, r, false)
	case "/api/configuration/rollback":
		s.configConfirm(w, r, true)
	case "/api/configuration/status":
		s.configStatus(w, r)
	case "/api/logs":
		s.logsResponse(w, r)
	case "/api/health":
		writeJSON(w, 200, struct {
			Status   string `json:"status"`
			Mode     string `json:"mode"`
			ReadOnly bool   `json:"readOnly"`
		}{"ok", s.system.Mode(), s.control == nil})
	case "/api/modules":
		writeJSON(w, 200, struct {
			Modules []core.Module `json:"modules"`
		}{s.ModuleList()})
	case "/api/system":
		system, err := s.sampler.Latest()
		if err != nil {
			fail(w, 503, "observation_unavailable", err.Error())
			return
		}
		writeJSON(w, 200, system)
	case "/api/network":
		network, err := s.network.Observe(r.Context())
		if err != nil {
			fail(w, 503, "observation_unavailable", err.Error())
			return
		}
		writeJSON(w, 200, network)
	case "/api/devices":
		writeJSON(w, 200, struct {
			Devices   []any  `json:"devices"`
			Supported bool   `json:"supported"`
			Reason    string `json:"reason"`
		}{[]any{}, false, modules.DeviceReason})
	case "/api/frpc":
		writeJSON(w, 200, struct {
			Supported bool   `json:"supported"`
			Running   bool   `json:"running"`
			Reason    string `json:"reason"`
			Proxies   []any  `json:"proxies"`
		}{false, false, modules.FRPCReason, []any{}})
	case "/api/proxy/plan":
		s.proxyPlan(w, r)
	case "/api/frpc/plan":
		s.frpcPlan(w, r)
	case "/api/operations/apply":
		fail(w, 501, "not_implemented", "Applying plans is not implemented; no system changes were made.")
	case "/api/session":
		s.session(w, r)
	case "/api/session/login":
		s.login(w, r)
	case "/api/session/logout":
		s.auth.logout(w, r)
		s.session(w, r)
	case "/api/events":
		s.events(w, r)
	}
}

var routes = map[string]string{
	"/api/proxy/metrics": "GET", "/api/proxy/probe": "POST", "/api/traffic/history": "GET",

	"/api/router": "GET", "/api/runtime": "GET", "/api/runtime/acquire": "POST", "/api/runtime/configure": "POST", "/api/runtime/config": "GET", "/api/runtime/start": "POST", "/api/runtime/stop": "POST", "/api/runtime/restore": "POST",
	"/api/proxy/nodes": "GET", "/api/proxy/import": "POST", "/api/proxy/select": "POST", "/api/proxy/capture": "GET",
	"/api/configuration": "GET", "/api/configuration/stage": "POST", "/api/configuration/drafts": "GET", "/api/configuration/commit": "POST", "/api/configuration/confirm": "POST", "/api/configuration/rollback": "POST", "/api/configuration/status": "GET",
	"/api/logs": "GET", "/api/health": "GET", "/api/modules": "GET", "/api/system": "GET", "/api/network": "GET", "/api/devices": "GET", "/api/frpc": "GET", "/api/events": "GET", "/api/proxy/plan": "POST", "/api/frpc/plan": "POST", "/api/operations/apply": "POST", "/api/session": "GET", "/api/session/login": "POST", "/api/session/logout": "POST",
}

func safeMethod(method string) bool {
	return method == "GET" || method == "HEAD" || method == "OPTIONS"
}
func sameOrigin(r *http.Request) bool {
	if r.Header.Get("Sec-Fetch-Site") == "cross-site" {
		return false
	}
	origins := r.Header.Values("Origin")
	if len(origins) == 0 {
		return true
	}
	if len(origins) != 1 {
		return false
	}
	origin, err := url.Parse(origins[0])
	if err != nil {
		return false
	}
	scheme := "http"
	if r.TLS != nil {
		scheme = "https"
	}
	return origin.Scheme == scheme && origin.Host == r.Host && origin.User == nil && origin.Path == "" && origin.RawQuery == "" && origin.Fragment == ""
}
func (s *Server) session(w http.ResponseWriter, r *http.Request) {
	writeJSON(w, 200, struct {
		Authenticated bool `json:"authenticated"`
		AuthRequired  bool `json:"authRequired"`
	}{s.auth.authenticated(r), s.auth.required})
}
func (s *Server) login(w http.ResponseWriter, r *http.Request) {
	if s.auth.required && !s.auth.allowAttempt(r) {
		s.logger.Warn("Login rate limited", "code", "rate_limited", "module", "core")
		w.Header().Set("Retry-After", "60")
		fail(w, 429, "rate_limited", "Too many login attempts; try again later.")
		return
	}
	var input struct {
		Password string `json:"password"`
	}
	if !decodeJSON(w, r, &input, "password") {
		return
	}
	if !s.auth.required {
		s.session(w, r)
		return
	}
	if !s.auth.passwordMatches(input.Password) {
		s.logger.Warn("Login rejected", "code", "invalid_password", "module", "core")
		fail(w, 401, "invalid_password", "Invalid password.")
		return
	}
	if err := s.auth.createSession(w, r); err != nil {
		fail(w, 500, "session_unavailable", "Cannot create session.")
		return
	}
	s.auth.resetAttempts(r)
	writeJSON(w, 200, struct {
		Authenticated bool `json:"authenticated"`
		AuthRequired  bool `json:"authRequired"`
	}{true, true})
}
func (s *Server) proxyPlan(w http.ResponseWriter, r *http.Request) {
	var input modules.ProxyInput
	if !decodeJSON(w, r, &input, "mode", "dnsStrategy", "ipv6Policy", "failurePolicy", "nodeCount") {
		s.logger.Warn("Plan JSON rejected", "code", "invalid_input", "module", "proxy")
		return
	}
	plan, err := (modules.Proxy{}).Plan(input, s.coordinator)
	s.planResult(w, "proxy", plan, err)
}
func (s *Server) frpcPlan(w http.ResponseWriter, r *http.Request) {
	var input modules.FRPCInput
	if !decodeJSON(w, r, &input, "serverAddress", "serverPort", "tls", "transport", "proxies") {
		s.logger.Warn("Plan JSON rejected", "code", "invalid_input", "module", "frpc")
		return
	}
	plan, err := (modules.FRPC{}).Plan(input, s.coordinator)
	s.planResult(w, "frpc", plan, err)
}
func (s *Server) planResult(w http.ResponseWriter, module string, plan core.OperationPlan, err error) {
	if err != nil {
		s.logger.Warn("Plan validation rejected", "code", "invalid_input", "module", module)
		fail(w, 400, "invalid_input", err.Error())
		return
	}
	s.logger.Info("Read-only plan validated", "code", "plan_validated", "module", module)
	writeJSON(w, 200, plan)
}

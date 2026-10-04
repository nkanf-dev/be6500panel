package httpapi

// This temporary front owns only the policy draft and selection view. The old
// panel remains the sole core owner. Construction and Close never touch it.
import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"log/slog"
	"mime"
	"net"
	"net/http"
	"net/http/httputil"
	"net/url"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"be6500panel/internal/localrules"
	"be6500panel/internal/proxy"
	managedruntime "be6500panel/internal/runtime"
	"be6500panel/internal/storage"
)

// The original live owner listens on this router management address, not on
// loopback. Keep the endpoint fixed; browser input never selects an upstream.
const ruleManagementUpstream = "http://192.168.31.1:8787"

type RuleManagementFrontConfig struct {
	Context          context.Context
	DataDir, WebDir  string
	Logger           *slog.Logger
	StorageAdmission storage.Admission
}
type RuleManagementFront struct {
	server                    *Server
	upstream                  *url.URL
	proxy                     *httputil.ReverseProxy
	reads, writes             *http.Client
	transport, writeTransport *http.Transport
	static                    http.Handler
}

type ruleRuntimeCallbacks struct {
	Status    func(string) (managedruntime.Status, error)
	Config    func(string) ([]byte, uint64, error)
	Configure func(context.Context, string, []byte, uint64) (managedruntime.Status, error)
}

func (s *Server) hasRuleRuntime() bool { return s.runtime != nil || s.ruleRuntime != nil }
func (s *Server) ruleRuntimeEnabled(w http.ResponseWriter) bool {
	if s.ruleRuntime != nil {
		return true
	}
	return s.runtimeEnabled(w)
}
func (s *Server) ruleStatus(service string) (managedruntime.Status, error) {
	if s.ruleRuntime != nil {
		return s.ruleRuntime.Status(service)
	}
	return s.runtime.Status(service)
}
func (s *Server) ruleConfig(service string) ([]byte, uint64, error) {
	if s.ruleRuntime != nil {
		return s.ruleRuntime.Config(service)
	}
	return s.runtime.Config(service)
}
func (s *Server) ruleConfigure(ctx context.Context, service string, raw []byte, generation uint64) (managedruntime.Status, error) {
	if s.ruleRuntime != nil {
		return s.ruleRuntime.Configure(ctx, service, raw, generation)
	}
	return s.runtime.ConfigureGuarded(ctx, service, raw, generation, s.runtimeMutationGuard)
}
func (s *Server) ruleRecordedNodeMatchesAccepted(id string, nodes []proxy.Node) bool {
	if s.ruleRuntime == nil {
		return s.recordedNodeMatchesAccepted(id, nodes)
	}
	if id == "" {
		return false
	}
	raw, _, err := s.ruleConfig(managedruntime.SingBox)
	if err != nil {
		return false
	}
	for _, node := range nodes {
		if node.ID == id {
			return acceptedSelectedNodeMatches(raw, node)
		}
	}
	return false
}

type ruleForwardError struct {
	statusCode int
	apiError   apiError
	status     managedruntime.Status
}

func (e *ruleForwardError) Error() string { return e.apiError.Code }
func ruleForwardFailure(w http.ResponseWriter, err error) bool {
	var e *ruleForwardError
	if !errors.As(err, &e) {
		return false
	}
	if e.status.Service == "" {
		fail(w, e.statusCode, e.apiError.Code, e.apiError.Message)
	} else {
		writeJSON(w, e.statusCode, struct {
			Error  apiError              `json:"error"`
			Status managedruntime.Status `json:"status"`
		}{e.apiError, e.status})
	}
	return true
}
func (s *Server) ruleRuntimeError(w http.ResponseWriter, err error) {
	if !ruleForwardFailure(w, err) {
		s.runtimeError(w, err)
	}
}
func (s *Server) ruleRuntimeResult(w http.ResponseWriter, state managedruntime.Status, err error) {
	if s.ruleRuntime == nil {
		s.runtimeResult(w, managedruntime.SingBox, "config_committed", state, err)
		return
	}
	if ruleForwardFailure(w, err) {
		return
	}
	if err != nil {
		s.ruleRuntimeError(w, err)
		return
	}
	writeJSON(w, 200, state) // The original owner already persisted desired state.
}

func NewRuleManagementFront(cfg RuleManagementFrontConfig) (*RuleManagementFront, error) {
	upstream, _ := url.Parse(ruleManagementUpstream)
	return newRuleManagementFront(cfg, upstream)
}

// The alternate upstream is private to package tests; production cannot choose
// a remote URL or obtain a second runtime manager.
func newRuleManagementFront(cfg RuleManagementFrontConfig, upstream *url.URL) (*RuleManagementFront, error) {
	if cfg.DataDir == "" {
		return nil, errors.New("private data directory is required")
	}
	if cfg.Context == nil {
		cfg.Context = context.Background()
	}
	if cfg.Logger == nil {
		cfg.Logger = slog.Default()
	}
	ctx, cancel := context.WithCancel(cfg.Context)
	s := &Server{logger: cfg.Logger, ctx: ctx, cancel: cancel, dataDir: cfg.DataDir,
		storageAdmission: cfg.StorageAdmission, proxyState: newProxyState(cfg.DataDir)}
	s.localRules, s.localRulesError = localrules.New(localrules.Options{DataDir: cfg.DataDir, Context: ctx, StorageAdmission: cfg.StorageAdmission})
	transport := &http.Transport{Proxy: nil, DialContext: (&net.Dialer{Timeout: 3 * time.Second}).DialContext,
		ResponseHeaderTimeout: 3 * time.Second, IdleConnTimeout: 30 * time.Second, MaxIdleConns: 16, MaxIdleConnsPerHost: 8}
	writeTransport := transport.Clone()
	writeTransport.ResponseHeaderTimeout = 90 * time.Second
	noRedirect := func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }
	f := &RuleManagementFront{server: s, upstream: upstream, transport: transport, writeTransport: writeTransport, static: staticHandler(cfg.WebDir),
		reads:  &http.Client{Transport: transport, Timeout: 3 * time.Second, CheckRedirect: noRedirect},
		writes: &http.Client{Transport: writeTransport, Timeout: 90 * time.Second, CheckRedirect: noRedirect}}
	f.proxy = &httputil.ReverseProxy{Transport: ruleFrontTransport{transport, writeTransport}, FlushInterval: -1,
		Rewrite: func(r *httputil.ProxyRequest) {
			r.SetURL(upstream)
			r.Out.Host = upstream.Host
			if r.In.Header.Get("Origin") != "" {
				r.Out.Header.Set("Origin", upstream.Scheme+"://"+upstream.Host)
			}
		}, ModifyResponse: func(res *http.Response) error {
			// The unchanged owner supports eight WAN windows. Advertise actual
			// capabilities without inventing or rewriting any historical samples.
			if res.StatusCode != http.StatusOK || res.Request.URL.Path != "/api/traffic/history" {
				return nil
			}
			raw, err := io.ReadAll(io.LimitReader(res.Body, (1<<20)+1))
			res.Body.Close()
			if err != nil || len(raw) > 1<<20 {
				return errors.New("history response unavailable")
			}
			var doc map[string]json.RawMessage
			if json.Unmarshal(raw, &doc) != nil {
				return errors.New("history response invalid")
			}
			doc["supportedRanges"] = json.RawMessage(`["30m","3h","6h","1d","7d","30d","180d","1y"]`)
			raw, err = json.Marshal(doc)
			if err != nil {
				return err
			}
			res.Body = io.NopCloser(bytes.NewReader(raw))
			res.ContentLength = int64(len(raw))
			res.Header.Set("Content-Length", strconv.Itoa(len(raw)))
			return nil
		}, ErrorHandler: func(w http.ResponseWriter, r *http.Request, err error) {
			fail(w, 502, "upstream_unavailable", "原面板暂时不可用，请重新读取运行状态")
		}, ErrorLog: slog.NewLogLogger(cfg.Logger.Handler(), slog.LevelError)}
	return f, nil
}

type ruleFrontTransport struct{ reads, writes *http.Transport }

func (t ruleFrontTransport) RoundTrip(r *http.Request) (*http.Response, error) {
	if safeMethod(r.Method) {
		return t.reads.RoundTrip(r)
	}
	return t.writes.RoundTrip(r)
}
func (f *RuleManagementFront) Close() {
	f.server.cancel()
	f.transport.CloseIdleConnections()
	f.writeTransport.CloseIdleConnections()
}

// The cookie lives only in this request's callbacks. Shared fields hold no
// browser credentials, and mutexes are never copied with the Server value.
func (f *RuleManagementFront) requestServer(r *http.Request) *Server {
	base := f.server
	cookie := r.Header.Get("Cookie")
	s := &Server{logger: base.logger, ctx: base.ctx, dataDir: base.dataDir,
		storageAdmission: base.storageAdmission, localRules: base.localRules,
		localRulesError: base.localRulesError, proxyState: base.proxyState}
	s.ruleRuntime = &ruleRuntimeCallbacks{
		Status: func(service string) (managedruntime.Status, error) {
			var result struct {
				Services []managedruntime.Status `json:"services"`
			}
			_, err := f.upstreamJSON(r.Context(), cookie, "GET", "/api/runtime", nil, &result)
			if err != nil {
				return managedruntime.Status{}, err
			}
			for _, state := range result.Services {
				if state.Service == service {
					return state, nil
				}
			}
			return managedruntime.Status{}, unavailableRuleUpstream()
		},
		Config: func(service string) ([]byte, uint64, error) {
			var result struct {
				Service    string `json:"service"`
				Config     string `json:"config"`
				Generation uint64 `json:"generation"`
			}
			_, err := f.upstreamJSON(r.Context(), cookie, "GET", "/api/runtime/config?service="+url.QueryEscape(service), nil, &result)
			if err == nil && result.Service != service {
				err = unavailableRuleUpstream()
			}
			return []byte(result.Config), result.Generation, err
		},
		Configure: func(ctx context.Context, service string, raw []byte, generation uint64) (managedruntime.Status, error) {
			body, _ := json.Marshal(struct {
				Service    string `json:"service"`
				Config     string `json:"config"`
				Generation uint64 `json:"generation"`
			}{service, string(raw), generation})
			var state managedruntime.Status
			_, err := f.upstreamJSON(ctx, cookie, "POST", "/api/runtime/configure", body, &state)
			if err != nil {
				var forwarded *ruleForwardError
				if errors.As(err, &forwarded) {
					state = forwarded.status
				}
			} else if state.Service != service {
				err = unavailableRuleUpstream()
			}
			return state, err
		},
	}
	return s
}
func unavailableRuleUpstream() *ruleForwardError {
	return &ruleForwardError{statusCode: 502, apiError: apiError{Code: "upstream_unavailable", Message: "原面板暂时不可用，请重新读取运行状态"}}
}

// Preserve fixed upstream API codes, HTTP status and recovery state. Never echo
// upstream messages: a transport or verifier error could contain private input.
var ruleUpstreamMessages = map[string]string{
	"unauthenticated": "Authentication required.", "generation_conflict": "配置已更新，请重新读取",
	"operation_busy": "另一操作正在执行", "invalid_service": "服务类型无效", "not_configured": "尚未配置",
	"artifact_unavailable": "运行文件尚未就绪", "artifact_compressed_limit": "下载文件超过大小限制",
	"artifact_uncompressed_limit": "解压文件超过大小限制", "readiness_failed": "本地监听未就绪",
	"config_check_failed": "配置校验失败，请查看运行诊断", "operation_timeout": "操作超时",
	"operation_cancelled": "操作已取消", "runtime_operation_failed": "运行操作失败",
	"configuration_pending": "请先完成或恢复当前网络配置，再修改服务运行状态",
	"storage_failed":        "运行配置保存未确认，请重新读取运行状态", "storage_insufficient": "存储空间不足",
	"subscription_invalid": "订阅格式或节点参数无效", "subscription_fetch_failed": "订阅下载失败",
	"no_compatible_nodes": "没有兼容的 VLESS 节点", "invalid_input": "请求参数无效",
	"body_too_large": "请求内容过大", "proxy_mutation_pending": "订阅或节点配置正在保存，请稍后重试",
	"configuration_unavailable": "配置存储未启用", "runtime_unavailable": "运行管理未启用",
	"invalid_json":           "JSON fields or types do not match the request contract.",
	"unsupported_media_type": "Content-Type must be application/json.",
}

func (f *RuleManagementFront) upstreamJSON(ctx context.Context, cookie, method, path string, body []byte, result any) (int, error) {
	req, err := http.NewRequestWithContext(ctx, method, f.upstream.String()+path, bytes.NewReader(body))
	if err != nil {
		return 0, unavailableRuleUpstream()
	}
	req.Header.Set("Cookie", cookie)
	if method != "GET" {
		req.Header.Set("Content-Type", "application/json")
		req.Header.Set("Origin", f.upstream.String())
	}
	client := f.reads
	if method != "GET" {
		client = f.writes
	}
	res, err := client.Do(req)
	if err != nil {
		if ctx.Err() != nil {
			return 0, ctx.Err()
		}
		var timeout net.Error
		if errors.As(err, &timeout) && timeout.Timeout() {
			return 0, context.DeadlineExceeded
		}
		return 0, unavailableRuleUpstream()
	}
	defer res.Body.Close()
	raw, err := io.ReadAll(io.LimitReader(res.Body, (8<<20)+1))
	if err != nil || len(raw) > 8<<20 {
		return res.StatusCode, unavailableRuleUpstream()
	}
	if res.StatusCode < 200 || res.StatusCode >= 300 {
		var envelope struct {
			Error  apiError              `json:"error"`
			Status managedruntime.Status `json:"status"`
		}
		_ = json.Unmarshal(raw, &envelope)
		message, ok := ruleUpstreamMessages[envelope.Error.Code]
		if !ok {
			envelope.Error.Code, message = "runtime_operation_failed", "运行操作失败"
		}
		if envelope.Status.Restored {
			message += "；已恢复上一可运行配置"
		}
		if envelope.Status.NeedsRecovery {
			message += "；恢复未完成，请检查运行状态"
		}
		status := res.StatusCode
		if status < 400 || status > 599 {
			status = 502
		}
		return status, &ruleForwardError{status, apiError{envelope.Error.Code, message}, envelope.Status}
	}
	if json.Unmarshal(raw, result) != nil {
		return res.StatusCode, unavailableRuleUpstream()
	}
	return res.StatusCode, nil
}

func (f *RuleManagementFront) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("X-Content-Type-Options", "nosniff")
	w.Header().Set("Referrer-Policy", "same-origin")
	if r.URL.Path != "/api" && (len(r.URL.Path) < 5 || r.URL.Path[:5] != "/api/") {
		f.static.ServeHTTP(w, r)
		return
	}
	w.Header().Set("Cache-Control", "no-store")
	if !safeMethod(r.Method) && !sameOrigin(r) {
		fail(w, 403, "origin_rejected", "Unsafe requests must be same-origin.")
		return
	}
	path := r.URL.Path
	intercepted := path == LocalRulesPath || path == LocalRulesPreviewPath || path == LocalRulesApplyPath || path == "/api/proxy/select" || path == "/api/proxy/import" || path == "/api/proxy/nodes"
	if !intercepted {
		// SSE stays streaming; all other forwarded calls have an absolute deadline.
		if path != "/api/events" || r.Method != "GET" {
			timeout := 3 * time.Second
			if !safeMethod(r.Method) {
				timeout = 90 * time.Second
			}
			ctx, cancel := context.WithTimeout(r.Context(), timeout)
			defer cancel()
			r = r.WithContext(ctx)
		}
		f.proxy.ServeHTTP(w, r)
		return
	}
	allowed := routes[path]
	if r.Method != allowed && !(path == LocalRulesPath && r.Method == "POST") {
		w.Header().Set("Allow", allowed)
		fail(w, 405, "method_not_allowed", "Method is not allowed for this endpoint.")
		return
	}
	var session struct {
		Authenticated bool `json:"authenticated"`
	}
	_, err := f.upstreamJSON(r.Context(), r.Header.Get("Cookie"), "GET", "/api/session", nil, &session)
	if err != nil {
		f.server.ruleRuntimeError(w, err)
		return
	}
	if !session.Authenticated {
		fail(w, 401, "unauthenticated", "Authentication required.")
		return
	}
	s := f.requestServer(r)
	switch path {
	case LocalRulesPath:
		s.proxyLocalRules(w, r)
	case LocalRulesPreviewPath:
		s.proxyLocalRulesPreview(w, r)
	case LocalRulesApplyPath:
		s.proxyLocalRulesApply(w, r)
	case "/api/proxy/select":
		s.proxySelect(w, r)
	case "/api/proxy/nodes":
		p := s.proxyState
		if !p.mutationMu.TryLock() {
			fail(w, 409, "proxy_mutation_pending", "订阅或节点配置正在保存，请稍后重试")
			return
		}
		defer p.mutationMu.Unlock()
		var nodes struct {
			Revision string `json:"revision"`
		}
		_, err := f.upstreamJSON(r.Context(), r.Header.Get("Cookie"), "GET", path, nil, &nodes)
		if err != nil {
			s.ruleRuntimeError(w, err)
			return
		}
		p.mu.Lock()
		p.revision = nodes.Revision
		p.mu.Unlock()
		s.proxyNodes(w, r)
	case "/api/proxy/import":
		f.importSubscription(w, r, s)
	}
}

func (f *RuleManagementFront) importSubscription(w http.ResponseWriter, r *http.Request, s *Server) {
	p := s.proxyState
	if !p.mutationMu.TryLock() {
		fail(w, 409, "proxy_mutation_pending", "订阅或节点配置正在保存，请稍后重试")
		return
	}
	defer p.mutationMu.Unlock()
	media, _, err := mime.ParseMediaType(r.Header.Get("Content-Type"))
	if err != nil || media != "application/json" {
		fail(w, 415, "unsupported_media_type", "Content-Type must be application/json.")
		return
	}
	body, err := io.ReadAll(http.MaxBytesReader(w, r.Body, 3<<20))
	if err != nil {
		fail(w, 413, "body_too_large", "请求内容过大")
		return
	}
	var nodes struct {
		Revision string `json:"revision"`
	}
	_, err = f.upstreamJSON(r.Context(), r.Header.Get("Cookie"), "POST", "/api/proxy/import", body, &nodes)
	if err != nil {
		s.ruleRuntimeError(w, err)
		return
	}
	// Reload only the subscription; the original import does not clear its
	// selection file. Never resurrect that stale file marker after import.
	file, loadErr := os.Open(filepath.Join(s.dataDir, "subscription.yaml"))
	var sub proxy.Subscription
	if loadErr == nil {
		raw, readErr := io.ReadAll(io.LimitReader(file, proxy.MaxSubscriptionBytes+1))
		_ = file.Close()
		loadErr = readErr
		if loadErr == nil && len(raw) > proxy.MaxSubscriptionBytes {
			loadErr = errors.New("subscription exceeds limit")
		}
		if loadErr == nil {
			sub, loadErr = proxy.ParseClashYAML(strings.NewReader(string(raw)))
		}
	}
	p.mu.Lock()
	if loadErr == nil {
		p.subscription = sub
	}
	p.loadFailed, p.selected, p.revision = loadErr != nil, "", nodes.Revision
	p.mu.Unlock()
	if loadErr != nil {
		fail(w, 503, "rules_unavailable", "订阅保存后读取未确认，请重新读取订阅状态")
		return
	}
	s.proxyNodes(w, r)
}

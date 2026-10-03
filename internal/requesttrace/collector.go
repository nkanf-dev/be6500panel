package requesttrace

import (
	"context"
	"crypto/tls"
	"crypto/x509"
	"errors"
	"io"
	"net"
	"net/http"
	"net/http/httptrace"
	"net/url"
	"strconv"
	"sync"
	"time"
)

type Collector struct {
	mu       sync.Mutex
	traces   []Trace
	running  bool
	sequence uint64
	provider ProxyProvider
	targets  []Target
	// Private test seams cannot be selected through the API or caller Config.
	rootCAs     *x509.CertPool
	timeout     time.Duration
	dialContext func(context.Context, string, string) (net.Conn, error)
}

func New(cfg Config) *Collector {
	return &Collector{provider: cfg.ProxyProvider, targets: append([]Target{}, presets...), timeout: Timeout}
}
func (c *Collector) Snapshot() Snapshot {
	c.mu.Lock()
	defer c.mu.Unlock()
	traces := make([]Trace, len(c.traces))
	for i, trace := range c.traces {
		traces[i] = cloneTrace(trace)
	}
	return Snapshot{Traces: traces, Targets: append([]Target{}, c.targets...), Limits: Limits{TimeoutMS: Timeout.Milliseconds(), BodyBytes: MaxBodyBytes, Concurrency: 1, Capacity: Capacity}, Running: c.running}
}

// Run performs exactly one diagnostic; network failures are completed trace
// results, not lost API errors. Input/busy/unavailable errors make no request.
func (c *Collector) Run(ctx context.Context, input Input) (Trace, error) {
	var target Target
	for _, candidate := range c.targets {
		if candidate.ID == input.TargetID {
			target = candidate
			break
		}
	}
	if target.ID == "" || (input.Route != "direct" && input.Route != "proxy") {
		return Trace{}, ErrInput
	}
	c.mu.Lock()
	if c.running {
		c.mu.Unlock()
		return Trace{}, ErrBusy
	}
	c.running = true
	c.sequence++
	id := strconv.FormatUint(c.sequence, 10)
	c.mu.Unlock()
	defer func() { c.mu.Lock(); c.running = false; c.mu.Unlock() }()
	ctx, cancel := context.WithTimeout(ctx, c.timeout)
	defer cancel()
	transport := &http.Transport{
		Proxy: nil, DisableKeepAlives: true, DisableCompression: true,
		TLSClientConfig:     &tls.Config{RootCAs: c.rootCAs, MinVersion: tls.VersionTLS12},
		TLSHandshakeTimeout: Timeout, ResponseHeaderTimeout: Timeout, MaxResponseHeaderBytes: 16 << 10,
		ForceAttemptHTTP2: false,
	}
	dialer := &net.Dialer{Timeout: Timeout}
	transport.DialContext = dialer.DialContext
	if c.dialContext != nil {
		transport.DialContext = c.dialContext
	}
	if input.Route == "proxy" {
		if c.provider == nil {
			return Trace{}, ErrUnavailable
		}
		endpoint, err := c.provider(ctx)
		if err != nil {
			return Trace{}, ErrUnavailable
		}
		proxyURL, err := endpoint.proxyURL()
		if err != nil {
			return Trace{}, ErrUnavailable
		}
		transport.Proxy = http.ProxyURL(proxyURL)
	}
	defer transport.CloseIdleConnections()
	recorder := newRecorder(time.Now(), input.Route)
	transport.OnProxyConnectResponse = func(_ context.Context, _ *url.URL, _ *http.Request, response *http.Response) error {
		// A rejected CONNECT response identifies the failed stage, but the
		// transport exposes no request-start event. Do not invent its duration.
		if response.StatusCode != http.StatusOK {
			recorder.failure("connect")
		}
		return nil
	}
	trace := Trace{ID: id, TargetID: target.ID, TargetLabel: target.Label, URL: target.URL, Route: input.Route, StartedAt: recorder.started.UTC(), PeerScope: "origin"}
	if input.Route == "proxy" {
		trace.PeerScope = "proxy"
	}
	req, err := http.NewRequestWithContext(httptrace.WithClientTrace(ctx, recorder.hooks()), http.MethodGet, target.URL, nil)
	if err != nil {
		return Trace{}, ErrInput
	}
	req.Header.Set("User-Agent", "be6500panel-network-diagnostic")
	req.Header.Set("Accept", "*/*")
	req.Header.Set("Accept-Encoding", "identity")
	client := &http.Client{Transport: transport, CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }}
	response, runErr := client.Do(req)
	if response != nil {
		trace.StatusCode = &response.StatusCode
		if runErr == nil {
			// The download interval covers body consumption after response headers;
			// Waiting ends at first byte, so a small header interval can remain a gap.
			recorder.begin("transfer")
			trace.BytesRead, runErr = io.Copy(io.Discard, io.LimitReader(response.Body, MaxBodyBytes))
			recorder.end("transfer", runErr)
			trace.BodyLimitReached = trace.BytesRead == MaxBodyBytes && (response.ContentLength < 0 || response.ContentLength > MaxBodyBytes)
		}
		_ = response.Body.Close()
	}
	trace.Phases, trace.PeerAddress, trace.FailurePhase = recorder.result(runErr != nil)
	trace.FinishedAt = recorder.finished.UTC()
	trace.TotalMS = recorder.finished.Sub(recorder.started).Seconds() * 1000
	switch {
	case runErr == nil && trace.StatusCode != nil && *trace.StatusCode >= 200 && *trace.StatusCode < 300:
		trace.Outcome = "success"
	case runErr == nil:
		trace.Outcome = "http_error"
		trace.ErrorCode = "http_status"
		phase := "response"
		trace.FailurePhase = &phase
	case errors.Is(ctx.Err(), context.Canceled):
		trace.Outcome = "cancelled"
		trace.ErrorCode = "cancelled"
	case errors.Is(ctx.Err(), context.DeadlineExceeded) || isTimeout(runErr):
		trace.Outcome = "timeout"
		trace.ErrorCode = "timeout"
	default:
		trace.Outcome = "failed"
		trace.ErrorCode = classifyError(runErr)
		if trace.FailurePhase != nil && *trace.FailurePhase == "connect" {
			trace.ErrorCode = "proxy_connect_failed"
		}
	}
	c.mu.Lock()
	c.traces = append([]Trace{cloneTrace(trace)}, c.traces...)
	if len(c.traces) > Capacity {
		c.traces = c.traces[:Capacity]
	}
	c.mu.Unlock()
	return trace, nil
}
func isTimeout(err error) bool {
	var network net.Error
	return errors.Is(err, context.DeadlineExceeded) || (errors.As(err, &network) && network.Timeout())
}
func classifyError(err error) string {
	var invalid x509.CertificateInvalidError
	var authority x509.UnknownAuthorityError
	var hostname x509.HostnameError
	var verification *tls.CertificateVerificationError
	if errors.As(err, &invalid) || errors.As(err, &authority) || errors.As(err, &hostname) || errors.As(err, &verification) {
		return "tls_verification_failed"
	}
	var dns *net.DNSError
	if errors.As(err, &dns) {
		return "dns_failed"
	}
	return "request_failed"
}
func cloneTrace(trace Trace) Trace {
	trace.Phases = append([]Phase{}, trace.Phases...)
	cloneFloat := func(p *float64) *float64 {
		if p == nil {
			return nil
		}
		v := *p
		return &v
	}
	for i := range trace.Phases {
		phase := &trace.Phases[i]
		phase.StartMS, phase.EndMS, phase.DurationMS = cloneFloat(phase.StartMS), cloneFloat(phase.EndMS), cloneFloat(phase.DurationMS)
	}
	if trace.StatusCode != nil {
		v := *trace.StatusCode
		trace.StatusCode = &v
	}
	if trace.PeerAddress != nil {
		v := *trace.PeerAddress
		trace.PeerAddress = &v
	}
	if trace.FailurePhase != nil {
		v := *trace.FailurePhase
		trace.FailurePhase = &v
	}
	return trace
}

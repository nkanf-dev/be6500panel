package telemetry

import (
	"bytes"
	"context"
	"crypto/hmac"
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/netip"
	"net/url"
	"regexp"
	"sort"
	"strconv"
	"sync"
	"time"
	"unicode"
)

// Options reads private accepted core config through Config. The caller must
// include a core epoch (for example config generation + PID) to detect restart.
type Options struct {
	Config     func(context.Context) (CoreConfig, error)
	Interval   time.Duration
	StaleAfter time.Duration
}
type Collector struct {
	opts        Options
	client      *http.Client
	mu          sync.RWMutex
	collectMu   sync.Mutex
	snap        Snapshot
	previous    *Totals
	previousAt  time.Time
	epoch       string
	key         [32]byte
	probing     bool
	lastProbe   time.Time
	lifecycleMu sync.Mutex
	cancel      context.CancelFunc
	done        chan struct{}
	closed      bool
}

func New(opts Options) (*Collector, error) {
	if opts.Config == nil {
		return nil, fmt.Errorf("telemetry config provider is required")
	}
	if opts.Interval == 0 {
		opts.Interval = 2 * time.Second
	}
	if opts.Interval < time.Second || opts.Interval > time.Minute {
		return nil, fmt.Errorf("telemetry interval out of bounds")
	}
	if opts.StaleAfter == 0 {
		opts.StaleAfter = 3 * opts.Interval
	}
	if opts.StaleAfter < opts.Interval {
		return nil, fmt.Errorf("telemetry stale interval invalid")
	}
	c := &Collector{opts: opts, snap: Unavailable("本机核心遥测未就绪")}
	if _, err := rand.Read(c.key[:]); err != nil {
		return nil, err
	}
	c.client = &http.Client{Timeout: 7 * time.Second, Transport: &http.Transport{Proxy: nil, DialContext: (&net.Dialer{Timeout: time.Second}).DialContext, DisableCompression: true, MaxIdleConns: 1, MaxIdleConnsPerHost: 1, IdleConnTimeout: 30 * time.Second}, CheckRedirect: func(*http.Request, []*http.Request) error { return ErrUnavailable }}
	return c, nil
}

func Unavailable(reason string) Snapshot {
	reason = boundedText(reason, 256)
	cap := Capability{false, reason}
	return Snapshot{State: "unavailable", Reason: reason, Source: Source, Capabilities: Capabilities{Connections: cap, Traffic: cap, Routing: cap, Latency: cap, RequestPhases: Capability{false, "HTTPS 内容不透明；核心不提供 DNS/TCP/TLS/TTFB 请求阶段"}}, Connections: []Connection{}, Traffic: []TrafficSample{}, Probes: []Probe{}}
}

// Start samples independently of open browser tabs; Close cancels ongoing reads.
func (c *Collector) Start(ctx context.Context) {
	c.lifecycleMu.Lock()
	defer c.lifecycleMu.Unlock()
	if c.cancel != nil || c.closed {
		return
	}
	ctx, c.cancel = context.WithCancel(ctx)
	c.done = make(chan struct{})
	go func() {
		defer close(c.done)
		_ = c.Collect(ctx)
		ticker := time.NewTicker(c.opts.Interval)
		defer ticker.Stop()
		for {
			select {
			case <-ctx.Done():
				return
			case <-ticker.C:
				_ = c.Collect(ctx)
			}
		}
	}()
}
func (c *Collector) Close() {
	c.lifecycleMu.Lock()
	if c.closed {
		c.lifecycleMu.Unlock()
		return
	}
	c.closed = true
	cancel, done := c.cancel, c.done
	c.lifecycleMu.Unlock()
	if cancel != nil {
		cancel()
		<-done
	}
	c.client.CloseIdleConnections()
}

func (c *Collector) Snapshot() Snapshot {
	c.mu.RLock()
	defer c.mu.RUnlock()
	s := c.snap
	s.Connections = append([]Connection{}, s.Connections...)
	s.Traffic = append([]TrafficSample{}, s.Traffic...)
	s.Probes = append([]Probe{}, s.Probes...)
	if s.SampledAt != nil {
		t := *s.SampledAt
		s.SampledAt = &t
		if time.Since(t) > c.opts.StaleAfter && s.State == "ready" {
			s.State = "stale"
			s.Reason = "核心样本已过期；下方为最后一次观测"
		}
	}
	return s
}

// Collect requests only the finite /connections snapshot, never the streaming
// /traffic endpoint or raw /configs and /proxies responses.
func (c *Collector) Collect(ctx context.Context) error {
	c.collectMu.Lock()
	defer c.collectMu.Unlock()
	cfg, err := c.opts.Config(ctx)
	if err != nil || !validCoreConfig(cfg) {
		c.unavailable("当前核心未运行或未配置 localhost Clash API")
		return ErrUnavailable
	}
	var raw coreSnapshot
	if err = c.get(ctx, cfg, "/connections", &raw); err != nil {
		c.unavailable("本机核心遥测读取失败；检查 with_clash_api 构建及本机监听")
		return ErrUnavailable
	}
	current, currentErr := c.opts.Config(ctx)
	if currentErr != nil || current.Epoch != cfg.Epoch || current.Address != cfg.Address || current.Secret != cfg.Secret {
		c.unavailable("核心配置已变化；等待下一次采样")
		return ErrUnavailable
	}
	now := time.Now().UTC()
	if raw.UploadTotal == nil || raw.DownloadTotal == nil || len(raw.Connections) > 4096 {
		c.unavailable("核心连接响应无效或超过上限")
		return ErrUnavailable
	}
	total := Totals{*raw.UploadTotal, *raw.DownloadTotal}
	connections := make([]Connection, 0, min(len(raw.Connections), MaxConnections))
	routingAvailable := false
	// Keep deterministic newest connections; discard all raw metadata after this call.
	sort.Slice(raw.Connections, func(i, j int) bool { return raw.Connections[i].Start.After(raw.Connections[j].Start) })
	for _, r := range raw.Connections {
		if len(connections) >= MaxConnections {
			break
		}
		if r.ID == "" || r.Start.IsZero() || r.Start.After(now) {
			continue
		}
		network := "unknown"
		if r.Metadata.Network == "tcp" || r.Metadata.Network == "udp" {
			network = r.Metadata.Network
		}
		outbound := "unavailable"
		if len(r.Chains) > 0 && (r.Chains[0] == "proxy" || r.Chains[0] == "direct") {
			outbound = r.Chains[0]
		}
		descriptor := safeRule(r.Rule)
		if r.Rule != "" && outbound != "unavailable" {
			routingAvailable = true
		}
		connections = append(connections, Connection{ID: c.id(r.ID), StartedAt: r.Start.UTC(), AgeMS: now.Sub(r.Start).Milliseconds(), Network: network, SourceIP: safeIP(r.Metadata.SourceIP), SourcePort: safePort(r.Metadata.SourcePort), DestinationIP: safeIP(r.Metadata.DestinationIP), DestinationPort: safePort(r.Metadata.DestinationPort), Host: safeHost(r.Metadata.Host), UploadBytes: r.Upload, DownloadBytes: r.Download, Outbound: outbound, RuleID: c.id(r.Rule), Rule: descriptor})
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	epoch := cfg.Epoch + ":" + c.id(cfg.Address+"\x00"+cfg.Secret)
	reset := c.previous == nil || epoch != c.epoch || now.Sub(c.previousAt) > c.opts.StaleAfter || total.UploadBytes < c.previous.UploadBytes || total.DownloadBytes < c.previous.DownloadBytes
	sample := TrafficSample{Time: now, Reset: reset}
	if !reset {
		seconds := now.Sub(c.previousAt).Seconds()
		if seconds > 0 {
			sample.UploadRate = float64(total.UploadBytes-c.previous.UploadBytes) / seconds
			sample.DownloadRate = float64(total.DownloadBytes-c.previous.DownloadBytes) / seconds
		}
	}
	if c.epoch != "" && epoch != c.epoch {
		c.snap.Probes = []Probe{}
	}
	c.snap.State = "ready"
	c.snap.Reason = ""
	c.snap.SampledAt = &now
	c.snap.Totals = total
	c.snap.ActiveConnections = len(raw.Connections)
	c.snap.Truncated = len(raw.Connections) > MaxConnections
	c.snap.Connections = connections
	c.snap.Capabilities.Connections = Capability{true, "仅活跃连接；短于采样间隔的连接可能未被观测，不提供结束时间"}
	c.snap.Capabilities.Traffic = Capability{true, "核心总流量（含直连）；内存保留最近 900 点，重启后清空，非 WAN 历史"}
	c.snap.Capabilities.Routing = Capability{routingAvailable || len(raw.Connections) == 0, "仅采样时仍活跃的实际匹配规则与出站；不是全量规则命中计数，拒绝连接未覆盖"}
	c.snap.Capabilities.Latency = Capability{cfg.CanProbe, "仅手动测量当前 proxy 出站到固定 HTTPS 204 目标的请求延迟；不是连接 RTT"}
	if !cfg.CanProbe {
		c.snap.Capabilities.Latency.Reason = "当前配置没有可供探测的选中 proxy 出站"
	}
	c.snap.Traffic = append(c.snap.Traffic, sample)
	if len(c.snap.Traffic) > MaxTrafficSamples {
		c.snap.Traffic = append([]TrafficSample{}, c.snap.Traffic[len(c.snap.Traffic)-MaxTrafficSamples:]...)
	}
	c.previous = &total
	c.previousAt = now
	c.epoch = epoch
	return nil
}

func (c *Collector) unavailable(reason string) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.previous = nil
	if c.snap.SampledAt == nil {
		c.snap.State = "unavailable"
	} else {
		c.snap.State = "stale"
	}
	c.snap.Reason = reason
	cap := Capability{false, reason}
	c.snap.Capabilities.Connections = cap
	c.snap.Capabilities.Traffic = cap
	c.snap.Capabilities.Routing = cap
	c.snap.Capabilities.Latency = cap
}

type coreSnapshot struct {
	UploadTotal   *uint64
	DownloadTotal *uint64
	Connections   []coreConnection
}

// Admit connection count before allocating a large typed slice. The byte cap
// alone does not prevent a small-object array from amplifying memory usage.
func (s *coreSnapshot) UnmarshalJSON(body []byte) error {
	var fields map[string]json.RawMessage
	if json.Unmarshal(body, &fields) != nil || fields == nil {
		return ErrUnavailable
	}
	if json.Unmarshal(fields["uploadTotal"], &s.UploadTotal) != nil || json.Unmarshal(fields["downloadTotal"], &s.DownloadTotal) != nil {
		return ErrUnavailable
	}
	decoder := json.NewDecoder(bytes.NewReader(fields["connections"]))
	token, err := decoder.Token()
	if err != nil || token != json.Delim('[') {
		return ErrUnavailable
	}
	s.Connections = []coreConnection{}
	for decoder.More() {
		if len(s.Connections) >= 4096 {
			return ErrUnavailable
		}
		var conn coreConnection
		if decoder.Decode(&conn) != nil {
			return ErrUnavailable
		}
		s.Connections = append(s.Connections, conn)
	}
	token, err = decoder.Token()
	if err != nil || token != json.Delim(']') {
		return ErrUnavailable
	}
	return nil
}

type coreConnection struct {
	ID       string    `json:"id"`
	Start    time.Time `json:"start"`
	Upload   uint64    `json:"upload"`
	Download uint64    `json:"download"`
	Chains   []string  `json:"chains"`
	Rule     string    `json:"rule"`
	Metadata struct {
		Network         string `json:"network"`
		SourceIP        string `json:"sourceIP"`
		SourcePort      string `json:"sourcePort"`
		DestinationIP   string `json:"destinationIP"`
		DestinationPort string `json:"destinationPort"`
		Host            string `json:"host"`
	} `json:"metadata"`
}

func (c *Collector) get(ctx context.Context, cfg CoreConfig, path string, out any) error {
	ctx, cancel := context.WithTimeout(ctx, 6*time.Second)
	defer cancel()
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, "http://"+cfg.Address+path, nil)
	if err != nil {
		return ErrUnavailable
	}
	if cfg.Secret != "" {
		req.Header.Set("Authorization", "Bearer "+cfg.Secret)
	}
	req.Header.Set("Accept", "application/json")
	res, err := c.client.Do(req)
	if err != nil {
		return ErrUnavailable
	}
	defer res.Body.Close()
	if res.StatusCode != http.StatusOK {
		return ErrUnavailable
	}
	body, err := io.ReadAll(io.LimitReader(res.Body, MaxResponseBytes+1))
	if err != nil || len(body) > MaxResponseBytes {
		return ErrUnavailable
	}
	decoder := json.NewDecoder(bytes.NewReader(body))
	if decoder.Decode(out) != nil {
		return ErrUnavailable
	}
	if decoder.Decode(new(any)) != io.EOF {
		return ErrUnavailable
	}
	return nil
}

// Probe sends one explicit bounded request through the currently configured
// fixed "proxy" outbound. The panel never accepts arbitrary URLs or node names.
func (c *Collector) Probe(ctx context.Context) error {
	c.mu.Lock()
	if c.probing {
		c.mu.Unlock()
		return ErrBusy
	}
	if !c.lastProbe.IsZero() && time.Since(c.lastProbe) < 10*time.Second {
		c.mu.Unlock()
		return ErrCooldown
	}
	c.probing = true
	c.lastProbe = time.Now()
	c.mu.Unlock()
	defer func() { c.mu.Lock(); c.probing = false; c.mu.Unlock() }()
	cfg, err := c.opts.Config(ctx)
	if err != nil || !validCoreConfig(cfg) || !cfg.CanProbe {
		return ErrUnavailable
	}
	var result struct {
		Delay uint16 `json:"delay"`
	}
	err = c.get(ctx, cfg, "/proxies/proxy/delay?timeout=5000&url="+url.QueryEscape(ProbeURL), &result)
	current, currentErr := c.opts.Config(ctx)
	if currentErr != nil || current.Epoch != cfg.Epoch || current.Address != cfg.Address || current.Secret != cfg.Secret {
		return ErrUnavailable
	}
	probe := Probe{Time: time.Now().UTC(), DelayMS: result.Delay, Status: "ok"}
	if err != nil || result.Delay == 0 {
		probe.DelayMS = 0
		probe.Status = "failed"
	}
	c.mu.Lock()
	c.snap.Probes = append(c.snap.Probes, probe)
	if len(c.snap.Probes) > MaxProbes {
		c.snap.Probes = append([]Probe{}, c.snap.Probes[len(c.snap.Probes)-MaxProbes:]...)
	}
	c.mu.Unlock()
	if err != nil || result.Delay == 0 {
		return ErrProbeFailed
	}
	return nil
}

func (c *Collector) id(raw string) string {
	hash := hmac.New(sha256.New, c.key[:])
	_, _ = hash.Write([]byte(raw))
	return hex.EncodeToString(hash.Sum(nil)[:8])
}

// Select public routing metadata only; never return process/auth-user matches
// or private raw outbound configuration. Ordinary target domains/IPs are useful
// to the authenticated owner and are not hidden or replaced with fake labels.
var ruleKey = regexp.MustCompile(`(?:^|[ !(])([a-z_]+)=`)

func safeRule(raw string) string {
	if raw == "" {
		return "核心未提供匹配规则"
	}
	if raw == "final" {
		return raw
	}
	if len(raw) > 4096 {
		return "实际匹配规则（描述超过上限）"
	}
	allowed := map[string]bool{"domain": true, "domain_suffix": true, "domain_keyword": true, "domain_regex": true, "ip_cidr": true, "source_ip_cidr": true, "ip_version": true, "inbound": true, "network": true, "port": true, "source_port": true, "rule_set": true}
	matches := ruleKey.FindAllStringSubmatch(raw, -1)
	if len(matches) == 0 {
		return "实际匹配规则（未提供公开条件）"
	}
	for _, match := range matches {
		if !allowed[match[1]] {
			return "实际匹配规则（非公开条件）"
		}
	}
	return boundedText(raw, 160)
}
func boundedText(raw string, max int) string {
	out := make([]rune, 0, min(len(raw), max))
	for _, r := range raw {
		if unicode.IsControl(r) {
			continue
		}
		out = append(out, r)
		if len(out) == max {
			break
		}
	}
	return string(out)
}
func safeIP(raw string) string {
	a, err := netip.ParseAddr(raw)
	if err != nil || a.Zone() != "" {
		return ""
	}
	return a.String()
}
func safePort(raw string) uint16 {
	n, err := strconv.ParseUint(raw, 10, 16)
	if err != nil {
		return 0
	}
	return uint16(n)
}
func safeHost(raw string) string {
	if len(raw) == 0 || len(raw) > 253 {
		return ""
	}
	for _, r := range raw {
		if !((r >= 'a' && r <= 'z') || (r >= 'A' && r <= 'Z') || (r >= '0' && r <= '9') || r == '.' || r == '-' || r == '_' || r == ':') {
			return ""
		}
	}
	return raw
}

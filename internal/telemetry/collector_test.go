package telemetry

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

func fakeCollector(t *testing.T, handler http.HandlerFunc) (*Collector, *httptest.Server) {
	t.Helper()
	server := httptest.NewServer(handler)
	c, err := New(Options{Config: func(context.Context) (CoreConfig, error) {
		return CoreConfig{Address: strings.TrimPrefix(server.URL, "http://"), Secret: "private-secret", Epoch: "1", CanProbe: true}, nil
	}})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { c.Close(); server.Close() })
	return c, server
}
func coreFixture(total uint64, connections any) string {
	data, _ := json.Marshal(map[string]any{"uploadTotal": total, "downloadTotal": total * 2, "connections": connections})
	return string(data)
}
func connectionFixture(id, rule string) map[string]any {
	return map[string]any{"id": id, "start": time.Now().Add(-time.Minute).UTC(), "upload": uint64(40), "download": uint64(90), "chains": []string{"proxy"}, "rule": rule, "metadata": map[string]any{"network": "tcp", "host": "observed.example", "processPath": "private-process", "sourceIP": "192.168.31.200", "sourcePort": "51000", "destinationIP": "198.51.100.1", "destinationPort": "443"}, "uuid": "private-node-uuid"}
}

func TestFromNativeConfigLoopbackOnly(t *testing.T) {
	for _, address := range []string{"0.0.0.0:9090", "192.168.31.1:9090", "localhost:9090", "127.0.0.1:0", "127.0.0.1:9090/path", "127.0.0.1:99999"} {
		body, _ := json.Marshal(map[string]any{"experimental": map[string]any{"clash_api": map[string]any{"external_controller": address}}})
		if _, err := FromNativeConfig(body); !errors.Is(err, ErrUnavailable) {
			t.Fatalf("accepted %s", address)
		}
	}
	for _, address := range []string{"127.0.0.1:9090", "[::1]:9090"} {
		body, _ := json.Marshal(map[string]any{"experimental": map[string]any{"clash_api": map[string]any{"external_controller": address, "secret": "private"}}, "outbounds": []map[string]any{{"tag": "proxy", "type": "vless"}}})
		cfg, err := FromNativeConfig(body)
		if err != nil || cfg.Address != address || cfg.Secret != "private" || !cfg.CanProbe {
			t.Fatalf("parse %s: %+v %v", address, cfg, err)
		}
	}
	for _, body := range [][]byte{nil, []byte("{}"), []byte("{invalid"), make([]byte, (2<<20)+1)} {
		if _, err := FromNativeConfig(body); !errors.Is(err, ErrUnavailable) {
			t.Fatal("invalid config accepted")
		}
	}
	if _, err := ClashOptions("short"); err == nil {
		t.Fatal("short secret accepted")
	}
	options, err := ClashOptions(strings.Repeat("x", 32))
	if err != nil || options["external_controller"] != "127.0.0.1:9090" {
		t.Fatal(options, err)
	}
}

func TestCollectSanitizesAndMeasuresActualCounters(t *testing.T) {
	var total atomic.Uint64
	total.Store(100)
	c, _ := fakeCollector(t, func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/connections" || r.Header.Get("Authorization") != "Bearer private-secret" {
			t.Error("unexpected core request")
		}
		fmt.Fprint(w, coreFixture(total.Load(), []any{connectionFixture("real-core-uuid", "domain=observed.example => route(proxy)")}))
	})
	if err := c.Collect(context.Background()); err != nil {
		t.Fatal(err)
	}
	first := c.Snapshot()
	if first.State != "ready" || first.ActiveConnections != 1 || first.Totals.UploadBytes != 100 || !first.Traffic[0].Reset {
		t.Fatalf("%+v", first)
	}
	if first.Capabilities.RequestPhases.Available {
		t.Fatal("invented request phases")
	}
	if first.Connections[0].Outbound != "proxy" || first.Connections[0].Network != "tcp" || first.Connections[0].AgeMS < 59000 || first.Connections[0].Rule != "domain=observed.example => route(proxy)" || first.Connections[0].SourceIP != "192.168.31.200" || first.Connections[0].SourcePort != 51000 || first.Connections[0].DestinationIP != "198.51.100.1" || first.Connections[0].DestinationPort != 443 || first.Connections[0].Host != "observed.example" {
		t.Fatal(first.Connections)
	}
	encoded, _ := json.Marshal(first)
	for _, secret := range []string{"private-node-uuid", "private-process", "real-core-uuid", "private-secret"} {
		if strings.Contains(string(encoded), secret) {
			t.Fatalf("leaked %s", secret)
		}
	}
	// Control previous sample time without waiting for a wall-clock interval.
	c.mu.Lock()
	c.previousAt = time.Now().Add(-2 * time.Second)
	c.mu.Unlock()
	total.Store(200)
	if err := c.Collect(context.Background()); err != nil {
		t.Fatal(err)
	}
	second := c.Snapshot()
	sample := second.Traffic[1]
	if sample.Reset || sample.UploadRate < 40 || sample.UploadRate > 60 || sample.DownloadRate < 80 {
		t.Fatalf("wrong counter rate %+v", sample)
	}
	second.Connections[0].Rule = "changed"
	second.Traffic[0].UploadRate = 99
	if c.Snapshot().Connections[0].Rule == "changed" || c.Snapshot().Traffic[0].UploadRate == 99 {
		t.Fatal("snapshot alias")
	}
}

func TestCollectRestartAndStaleDoNotProduceTrafficSpikes(t *testing.T) {
	var total atomic.Uint64
	total.Store(1000)
	var fail atomic.Bool
	c, _ := fakeCollector(t, func(w http.ResponseWriter, r *http.Request) {
		if fail.Load() {
			w.WriteHeader(500)
			return
		}
		fmt.Fprint(w, coreFixture(total.Load(), []any{}))
	})
	_ = c.Collect(context.Background())
	total.Store(10)
	_ = c.Collect(context.Background())
	reset := c.Snapshot()
	if !reset.Traffic[1].Reset || reset.Traffic[1].DownloadRate != 0 {
		t.Fatal("counter reset counted")
	}
	fail.Store(true)
	if err := c.Collect(context.Background()); !errors.Is(err, ErrUnavailable) {
		t.Fatal(err)
	}
	stale := c.Snapshot()
	if stale.State != "stale" || stale.Capabilities.Traffic.Available || stale.Totals.UploadBytes != 10 {
		t.Fatal(stale)
	}
	fail.Store(false)
	total.Store(1000)
	_ = c.Collect(context.Background())
	if !c.Snapshot().Traffic[2].Reset {
		t.Fatal("outage bridged with false rate")
	}
	c.mu.Lock()
	old := time.Now().Add(-time.Minute)
	c.snap.SampledAt = &old
	c.mu.Unlock()
	if c.Snapshot().State != "stale" {
		t.Fatal("old sample shown live")
	}
}

func TestCollectionBudgetsAndMalformedResponses(t *testing.T) {
	for _, body := range []string{"{}", `{"uploadTotal":-1,"downloadTotal":0,"connections":[]}`, `{"uploadTotal":0,"downloadTotal":0,"connections":null}`, `{"uploadTotal":0,"downloadTotal":0,"connections":[]}{}`, strings.Repeat("x", MaxResponseBytes+1), coreFixture(0, make([]struct{}, 4097))} {
		t.Run(fmt.Sprint(len(body)), func(t *testing.T) {
			c, _ := fakeCollector(t, func(w http.ResponseWriter, r *http.Request) { fmt.Fprint(w, body) })
			if err := c.Collect(context.Background()); !errors.Is(err, ErrUnavailable) {
				t.Fatal("accepted invalid body", err)
			}
			if c.Snapshot().State != "unavailable" {
				t.Fatal(c.Snapshot())
			}
		})
	}
	conns := []any{}
	for i := 0; i < 200; i++ {
		conns = append(conns, connectionFixture(fmt.Sprint(i), "rule_set=cn-domain => route(direct)"))
	}
	c, _ := fakeCollector(t, func(w http.ResponseWriter, r *http.Request) { fmt.Fprint(w, coreFixture(0, conns)) })
	_ = c.Collect(context.Background())
	s := c.Snapshot()
	if len(s.Connections) != MaxConnections || s.ActiveConnections != 200 || !s.Truncated {
		t.Fatal(s)
	}
	// Ring size is bounded even with repeated HTTP collection.
	c.mu.Lock()
	c.snap.Traffic = make([]TrafficSample, MaxTrafficSamples)
	c.mu.Unlock()
	_ = c.Collect(context.Background())
	if len(c.Snapshot().Traffic) != MaxTrafficSamples {
		t.Fatal("unbounded traffic ring")
	}
}

func TestCollectionHonorsCancellationAndRejectsRedirects(t *testing.T) {
	started := make(chan struct{})
	c, _ := fakeCollector(t, func(w http.ResponseWriter, r *http.Request) { close(started); <-r.Context().Done() })
	ctx, cancel := context.WithCancel(context.Background())
	result := make(chan error, 1)
	go func() { result <- c.Collect(ctx) }()
	<-started
	cancel()
	select {
	case err := <-result:
		if !errors.Is(err, ErrUnavailable) {
			t.Fatal(err)
		}
	case <-time.After(time.Second):
		t.Fatal("collection ignored cancellation")
	}
	redirected, _ := fakeCollector(t, func(w http.ResponseWriter, r *http.Request) {
		http.Redirect(w, r, "http://192.0.2.1:9090/connections", 302)
	})
	if err := redirected.Collect(context.Background()); !errors.Is(err, ErrUnavailable) {
		t.Fatal("redirect followed")
	}
}

func TestExplicitProbeFixedTargetCooldownAndBudget(t *testing.T) {
	var requests atomic.Int64
	c, _ := fakeCollector(t, func(w http.ResponseWriter, r *http.Request) {
		requests.Add(1)
		if r.URL.Path != "/proxies/proxy/delay" || r.URL.Query().Get("url") != ProbeURL || r.URL.Query().Get("timeout") != "5000" {
			t.Error(r.URL)
		}
		fmt.Fprint(w, `{"delay":83}`)
	})
	if err := c.Probe(context.Background()); err != nil {
		t.Fatal(err)
	}
	if err := c.Probe(context.Background()); !errors.Is(err, ErrCooldown) {
		t.Fatal("cooldown bypass", err)
	}
	s := c.Snapshot()
	if len(s.Probes) != 1 || s.Probes[0].DelayMS != 83 || s.Probes[0].Status != "ok" || requests.Load() != 1 {
		t.Fatal(s)
	}
	c.mu.Lock()
	c.lastProbe = time.Time{}
	c.snap.Probes = make([]Probe, MaxProbes)
	c.mu.Unlock()
	_ = c.Probe(context.Background())
	if len(c.Snapshot().Probes) != MaxProbes {
		t.Fatal("unbounded probes")
	}
}

func TestProbeFailureBusyAndCancellation(t *testing.T) {
	c, _ := fakeCollector(t, func(w http.ResponseWriter, r *http.Request) {
		fmt.Fprint(w, `{"delay":0,"message":"private endpoint"}`)
	})
	if err := c.Probe(context.Background()); !errors.Is(err, ErrProbeFailed) {
		t.Fatal(err)
	}
	if p := c.Snapshot().Probes[0]; p.Status != "failed" || p.DelayMS != 0 {
		t.Fatal(p)
	}
	c.mu.Lock()
	c.probing = true
	c.mu.Unlock()
	if err := c.Probe(context.Background()); !errors.Is(err, ErrBusy) {
		t.Fatal(err)
	}
}

func TestNoLANRequestAndLifecycleClose(t *testing.T) {
	c, err := New(Options{Config: func(context.Context) (CoreConfig, error) { return CoreConfig{Address: "192.168.31.1:9090"}, nil }})
	if err != nil {
		t.Fatal(err)
	}
	defer c.Close()
	if err := c.Collect(context.Background()); !errors.Is(err, ErrUnavailable) {
		t.Fatal("non-local dial attempted")
	}
	started := make(chan struct{})
	running, _ := fakeCollector(t, func(w http.ResponseWriter, r *http.Request) { close(started); <-r.Context().Done() })
	running.Start(context.Background())
	<-started
	done := make(chan struct{})
	go func() { running.Close(); close(done) }()
	select {
	case <-done:
	case <-time.After(time.Second):
		t.Fatal("Close ignored in-flight cancellation")
	}
	running.Close()
	running.Start(context.Background())
}

func TestEpochChangeClearsNodeProbes(t *testing.T) {
	c, server := fakeCollector(t, func(w http.ResponseWriter, r *http.Request) {
		if strings.Contains(r.URL.Path, "delay") {
			fmt.Fprint(w, `{"delay":3}`)
		} else {
			fmt.Fprint(w, coreFixture(0, []any{}))
		}
	})
	var epoch atomic.Int64
	epoch.Store(1)
	c.opts.Config = func(context.Context) (CoreConfig, error) {
		return CoreConfig{Address: strings.TrimPrefix(server.URL, "http://"), Epoch: fmt.Sprint(epoch.Load()), CanProbe: true}, nil
	}
	_ = c.Collect(context.Background())
	_ = c.Probe(context.Background())
	epoch.Store(2)
	_ = c.Collect(context.Background())
	if len(c.Snapshot().Probes) != 0 || !c.Snapshot().Traffic[1].Reset {
		t.Fatal("node epoch retained old probes")
	}
}

func TestPublicConnectionFieldsAreBoundedAndTyped(t *testing.T) {
	r := connectionFixture("id", strings.Repeat("domain=example.test ", 100)+" => route(proxy)")
	r["metadata"] = map[string]any{"network": "tcp", "host": "https://user:password@example.test/private", "sourceIP": "192.168.31.2\nprivate-secret", "sourcePort": "65536", "destinationIP": "not-an-IP", "destinationPort": "-1"}
	c, _ := fakeCollector(t, func(w http.ResponseWriter, rq *http.Request) { fmt.Fprint(w, coreFixture(1, []any{r})) })
	if err := c.Collect(context.Background()); err != nil {
		t.Fatal(err)
	}
	s := c.Snapshot().Connections[0]
	if s.Host != "" || s.SourceIP != "" || s.DestinationIP != "" || s.SourcePort != 0 || s.DestinationPort != 0 || len([]rune(s.Rule)) > 160 {
		t.Fatal(s)
	}
	if safeRule("auth_user=private-password domain=example.test => route(proxy)") != "实际匹配规则（非公开条件）" {
		t.Fatal("private user rule exposed")
	}
	if safeHost(strings.Repeat("a", 254)) != "" {
		t.Fatal("oversize host")
	}
	if safeIP("::ffff:192.0.2.1") != "::ffff:192.0.2.1" || safePort("443") != 443 {
		t.Fatal("valid endpoint unavailable")
	}
}

func TestProbeCancelledAndEpochChangedAreNotSuccess(t *testing.T) {
	started := make(chan struct{})
	c, _ := fakeCollector(t, func(w http.ResponseWriter, r *http.Request) { close(started); <-r.Context().Done() })
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan error, 1)
	go func() { done <- c.Probe(ctx) }()
	<-started
	cancel()
	select {
	case err := <-done:
		if err == nil {
			t.Fatal("cancelled probe succeeded")
		}
	case <-time.After(time.Second):
		t.Fatal("probe ignored cancellation")
	}
	if c.Snapshot().Probes[0].Status != "failed" {
		t.Fatal("cancelled probe counted as success")
	}
	var cfgCalls atomic.Int64
	changed, server := fakeCollector(t, func(w http.ResponseWriter, r *http.Request) { fmt.Fprint(w, `{"delay":20}`) })
	changed.opts.Config = func(context.Context) (CoreConfig, error) {
		return CoreConfig{Address: strings.TrimPrefix(server.URL, "http://"), Epoch: fmt.Sprint(cfgCalls.Add(1)), CanProbe: true}, nil
	}
	if err := changed.Probe(context.Background()); !errors.Is(err, ErrUnavailable) {
		t.Fatal("changed node accepted", err)
	}
	if len(changed.Snapshot().Probes) != 0 {
		t.Fatal("probe attributed to new node")
	}
}

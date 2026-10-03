package requesttrace

import (
	"context"
	"crypto/x509"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/http/httptest"
	"net/http/httptrace"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

func fixtureCollector(serverURL string) *Collector {
	c := New(Config{})
	c.targets = []Target{{ID: "google204", Label: "fixture", URL: serverURL}}
	return c
}
func runFixture(t *testing.T, c *Collector, route string) Trace {
	t.Helper()
	trace, err := c.Run(context.Background(), Input{TargetID: "google204", Route: route})
	if err != nil {
		t.Fatal(err)
	}
	return trace
}
func phase(t *testing.T, trace Trace, id string) Phase {
	t.Helper()
	for _, p := range trace.Phases {
		if p.ID == id {
			return p
		}
	}
	t.Fatal("missing phase", id)
	return Phase{}
}
func complete(t *testing.T, p Phase) {
	t.Helper()
	if !p.Observed || p.StartMS == nil || p.EndMS == nil || p.DurationMS == nil || *p.EndMS < *p.StartMS || *p.DurationMS < 0 {
		t.Fatalf("not a completed observation: %+v", p)
	}
}
func unknown(t *testing.T, p Phase) {
	t.Helper()
	if p.Observed || p.StartMS != nil || p.EndMS != nil || p.DurationMS != nil {
		t.Fatalf("fabricated unknown phase: %+v", p)
	}
}
func TestNoProbeOnConstructionOrSnapshotAndFixedInput(t *testing.T) {
	var requests atomic.Int32
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { requests.Add(1); w.WriteHeader(204) }))
	defer server.Close()
	c := fixtureCollector(server.URL)
	snap := c.Snapshot()
	if len(snap.Traces) != 0 || snap.Running || requests.Load() != 0 || snap.Limits.TimeoutMS != 10000 || snap.Limits.BodyBytes != 65536 || snap.Limits.Concurrency != 1 || snap.Limits.Capacity != 64 {
		t.Fatalf("wrong idle snapshot: %+v", snap)
	}
	for _, input := range []Input{{"https://example.com", "direct"}, {"google204", ""}, {"google204", "DIRECT"}, {"google204", "http://proxy:2080"}} {
		if _, err := c.Run(context.Background(), input); !errors.Is(err, ErrInput) {
			t.Fatal("accepted unrestricted input", input, err)
		}
	}
	if requests.Load() != 0 {
		t.Fatal("GET/input validation probed")
	}
	production := New(Config{}).Snapshot()
	if len(production.Targets) != 2 || production.Targets[0].ID != "google204" || production.Targets[1].ID != "cloudflare" {
		t.Fatal("unexpected presets", production.Targets)
	}
}
func TestDirectHTTPAndDelayedFirstByte(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		time.Sleep(40 * time.Millisecond)
		_, _ = io.WriteString(w, "ok")
	}))
	defer server.Close()
	trace := runFixture(t, fixtureCollector(server.URL), "direct")
	if trace.Outcome != "success" || trace.StatusCode == nil || *trace.StatusCode != 200 || trace.BytesRead != 2 || trace.PeerAddress == nil || trace.PeerScope != "origin" {
		t.Fatalf("wrong trace: %+v", trace)
	}
	complete(t, phase(t, trace, "tcp"))
	complete(t, phase(t, trace, "ttfb"))
	complete(t, phase(t, trace, "transfer"))
	if *phase(t, trace, "ttfb").DurationMS < 25 {
		t.Fatal("first byte delay not measured")
	}
	unknown(t, phase(t, trace, "dns"))
	unknown(t, phase(t, trace, "tls"))
	unknown(t, phase(t, trace, "connect"))
	if *phase(t, trace, "ttfb").StartMS < *phase(t, trace, "tcp").EndMS {
		t.Fatal("phase offset missing")
	}
}
func trusted(t *testing.T, server *httptest.Server) *x509.CertPool {
	t.Helper()
	roots := x509.NewCertPool()
	roots.AddCert(server.Certificate())
	return roots
}
func TestTLSVerificationTrustedAndUntrusted(t *testing.T) {
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { w.WriteHeader(204) }))
	defer server.Close()
	c := fixtureCollector(server.URL)
	c.rootCAs = trusted(t, server)
	good := runFixture(t, c, "direct")
	if good.Outcome != "success" {
		t.Fatalf("trusted TLS failed: %+v", good)
	}
	complete(t, phase(t, good, "tls"))
	bad := runFixture(t, fixtureCollector(server.URL), "direct")
	if bad.Outcome != "failed" || bad.ErrorCode != "tls_verification_failed" || bad.FailurePhase == nil || *bad.FailurePhase != "tls" {
		t.Fatalf("untrusted TLS accepted or misreported: %+v", bad)
	}
	complete(t, phase(t, bad, "tcp"))
	complete(t, phase(t, bad, "tls"))
	unknown(t, phase(t, bad, "ttfb"))
	unknown(t, phase(t, bad, "transfer"))
	if len(c.Snapshot().Traces) != 1 {
		t.Fatal("history missing")
	}
}
func TestConnectFailureRetainsMeasuredPhase(t *testing.T) {
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	address := listener.Addr().String()
	_ = listener.Close()
	trace := runFixture(t, fixtureCollector("https://"+address), "direct")
	if trace.Outcome != "failed" || trace.FailurePhase == nil || *trace.FailurePhase != "tcp" {
		t.Fatalf("wrong connect failure: %+v", trace)
	}
	complete(t, phase(t, trace, "tcp"))
	unknown(t, phase(t, trace, "tls"))
	unknown(t, phase(t, trace, "ttfb"))
}
func TestCancellationTimeoutAndSingleConcurrency(t *testing.T) {
	for _, mode := range []string{"cancel", "timeout"} {
		t.Run(mode, func(t *testing.T) {
			entered := make(chan struct{}, 1)
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { entered <- struct{}{}; <-r.Context().Done() }))
			defer server.Close()
			c := fixtureCollector(server.URL)
			if mode == "timeout" {
				c.timeout = 80 * time.Millisecond
			}
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			done := make(chan Trace, 1)
			go func() {
				trace, err := c.Run(ctx, Input{"google204", "direct"})
				if err != nil {
					panic(err)
				}
				done <- trace
			}()
			<-entered
			if !c.Snapshot().Running {
				t.Fatal("active state missing")
			}
			if _, err := c.Run(context.Background(), Input{"google204", "direct"}); !errors.Is(err, ErrBusy) {
				t.Fatal("concurrent diagnostic admitted", err)
			}
			if mode == "cancel" {
				cancel()
			}
			trace := <-done
			wanted := "cancelled"
			if mode == "timeout" {
				wanted = "timeout"
			}
			if trace.Outcome != wanted || trace.FailurePhase == nil || *trace.FailurePhase != "ttfb" {
				t.Fatalf("wrong interrupted trace: %+v", trace)
			}
			p := phase(t, trace, "ttfb")
			if !p.Observed || p.StartMS == nil || p.EndMS != nil || p.DurationMS != nil {
				t.Fatal("unfinished wait fabricated", p)
			}
			complete(t, phase(t, trace, "tcp"))
			unknown(t, phase(t, trace, "transfer"))
			if c.Snapshot().Running || len(c.Snapshot().Traces) != 1 {
				t.Fatal("interrupted trace not retained")
			}
		})
	}
}
func TestBodyCapRedirectAndBoundedRing(t *testing.T) {
	var redirected atomic.Int32
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/redirect" {
			w.Header().Set("Location", "/destination")
			w.WriteHeader(302)
			return
		}
		if r.URL.Path == "/destination" {
			redirected.Add(1)
		}
		_, _ = io.WriteString(w, strings.Repeat("x", int(MaxBodyBytes)+100))
	}))
	defer server.Close()
	c := fixtureCollector(server.URL)
	trace := runFixture(t, c, "direct")
	if trace.BytesRead != MaxBodyBytes || !trace.BodyLimitReached {
		t.Fatalf("body cap wrong: %+v", trace)
	}
	redirect := runFixture(t, fixtureCollector(server.URL+"/redirect"), "direct")
	if redirect.Outcome != "http_error" || redirect.StatusCode == nil || *redirect.StatusCode != 302 || redirected.Load() != 0 {
		t.Fatalf("redirect followed: %+v", redirect)
	}
	for i := 0; i < Capacity+2; i++ {
		runFixture(t, c, "direct")
	}
	snap := c.Snapshot()
	if len(snap.Traces) != Capacity {
		t.Fatal("unbounded ring", len(snap.Traces))
	}
	snap.Traces[0].Phases[1].ID = "changed"
	*snap.Traces[0].StatusCode = 999
	if c.Snapshot().Traces[0].Phases[1].ID == "changed" || *c.Snapshot().Traces[0].StatusCode == 999 {
		t.Fatal("history shares mutable state")
	}
}
func TestObservedDNSFailureRemainsMeasured(t *testing.T) {
	c := fixtureCollector("https://fixture.invalid")
	c.dialContext = func(ctx context.Context, _, _ string) (net.Conn, error) {
		hooks := httptrace.ContextClientTrace(ctx)
		hooks.DNSStart(httptrace.DNSStartInfo{Host: "fixture.invalid"})
		dnsErr := &net.DNSError{Name: "fixture.invalid", Err: "fixture resolver failure", IsNotFound: true}
		hooks.DNSDone(httptrace.DNSDoneInfo{Err: dnsErr})
		return nil, dnsErr
	}
	trace := runFixture(t, c, "direct")
	if trace.Outcome != "failed" || trace.ErrorCode != "dns_failed" || trace.FailurePhase == nil || *trace.FailurePhase != "dns" {
		t.Fatalf("wrong DNS failure: %+v", trace)
	}
	complete(t, phase(t, trace, "dns"))
	unknown(t, phase(t, trace, "tcp"))
}
func TestHTTPProxyRemoteDNSAndTCPPeer(t *testing.T) {
	var seen atomic.Int32
	proxy := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Host != "unresolved-origin.invalid" {
			t.Errorf("origin no longer absolute host: %s", r.URL)
		}
		seen.Add(1)
		w.WriteHeader(204)
	}))
	defer proxy.Close()
	c := fixtureCollector("http://unresolved-origin.invalid/generate_204")
	c.provider = func(context.Context) (ProxyEndpoint, error) {
		return ProxyEndpoint{address: strings.TrimPrefix(proxy.URL, "http://")}, nil
	}
	trace := runFixture(t, c, "proxy")
	if trace.Outcome != "success" || seen.Load() != 1 || trace.PeerScope != "proxy" || trace.PeerAddress == nil || *trace.PeerAddress != strings.TrimPrefix(proxy.URL, "http://") {
		t.Fatalf("wrong proxy peer: %+v", trace)
	}
	unknown(t, phase(t, trace, "dns"))
	unknown(t, phase(t, trace, "connect"))
	complete(t, phase(t, trace, "tcp"))
	if phase(t, trace, "dns").Reason != "proxy_origin_dns_not_observable" {
		t.Fatal("proxy DNS truth lost")
	}
}
func TestTLSViaHTTPConnectRemainsVerified(t *testing.T) {
	origin := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { w.WriteHeader(204) }))
	defer origin.Close()
	var connects atomic.Int32
	proxy := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != "CONNECT" {
			t.Error("unexpected proxy method", r.Method)
			w.WriteHeader(400)
			return
		}
		if r.Host != strings.TrimPrefix(origin.URL, "https://") {
			t.Error("unexpected CONNECT host", r.Host)
			w.WriteHeader(400)
			return
		}
		upstream, err := net.Dial("tcp", r.Host)
		if err != nil {
			w.WriteHeader(502)
			return
		}
		defer upstream.Close()
		downstream, rw, err := w.(http.Hijacker).Hijack()
		if err != nil {
			t.Error(err)
			return
		}
		defer downstream.Close()
		_, _ = io.WriteString(rw, "HTTP/1.1 200 Connection established\r\n\r\n")
		_ = rw.Flush()
		connects.Add(1)
		done := make(chan struct{})
		go func() { _, _ = io.Copy(upstream, rw); _ = upstream.(*net.TCPConn).CloseWrite(); close(done) }()
		_, _ = io.Copy(downstream, upstream)
		_ = downstream.(*net.TCPConn).CloseWrite()
		<-done
	}))
	defer proxy.Close()
	c := fixtureCollector(origin.URL)
	c.rootCAs = trusted(t, origin)
	c.provider = func(context.Context) (ProxyEndpoint, error) {
		return ProxyEndpoint{address: strings.TrimPrefix(proxy.URL, "http://")}, nil
	}
	good := runFixture(t, c, "proxy")
	if good.Outcome != "success" || connects.Load() != 1 {
		t.Fatalf("CONNECT failed: %+v", good)
	}
	complete(t, phase(t, good, "tcp"))
	complete(t, phase(t, good, "tls"))
	unknown(t, phase(t, good, "dns"))
	unknown(t, phase(t, good, "connect"))
	c.rootCAs = nil
	bad := runFixture(t, c, "proxy")
	if bad.ErrorCode != "tls_verification_failed" || bad.Outcome != "failed" {
		t.Fatalf("proxy disabled origin verification: %+v", bad)
	}
}
func TestMixedProxyAcceptedScopeOnly(t *testing.T) {
	for _, tc := range []struct {
		listen string
		lan    []string
		ok     bool
	}{
		{"127.0.0.1", nil, true}, {"::1", nil, true}, {"192.168.31.1", []string{"192.168.31.1"}, true},
		{"192.168.31.1", nil, false}, {"0.0.0.0", nil, false}, {"::", nil, false}, {"proxy.example", nil, false}, {"203.0.113.4", nil, false}, {"fe80::1%br-lan", nil, false},
	} {
		t.Run(tc.listen, func(t *testing.T) {
			native := []byte(fmt.Sprintf(`{"inbounds":[{"type":"mixed","listen":%q,"listen_port":2080}]}`, tc.listen))
			endpoint, err := MixedProxyFromNative(native, tc.lan)
			if (err == nil) != tc.ok {
				t.Fatal("wrong listener admission", endpoint, err)
			}
			public, _ := json.Marshal(endpoint)
			if string(public) != "{}" {
				t.Fatal("private endpoint leaked")
			}
		})
	}
	for _, native := range []string{
		`{}`, `{"inbounds":[{"type":"mixed","listen":"127.0.0.1","listen_port":0}]}`,
		`{"inbounds":[{"type":"mixed","listen":"127.0.0.1","listen_port":2080,"users":[{"username":"secret","password":"private"}]}]}`,
		`{"inbounds":[{"type":"mixed","listen":"127.0.0.1","listen_port":2080,"tls":{"enabled":true}}]}`,
		`{"inbounds":[{"type":"mixed","listen":"127.0.0.1","listen_port":2080},{"type":"mixed","listen":"127.0.0.1","listen_port":2081}]}`,
	} {
		if _, err := MixedProxyFromNative([]byte(native), nil); err == nil {
			t.Fatal("unsafe mixed listener admitted", native)
		}
	}
	c := New(Config{})
	if _, err := c.Run(context.Background(), Input{"google204", "proxy"}); !errors.Is(err, ErrUnavailable) {
		t.Fatal("missing provider admitted", err)
	}
	if c.Snapshot().Running {
		t.Fatal("unavailable provider leaked running lock")
	}
}

func TestNativeResolverFailureIsMeasured(t *testing.T) {
	socket, err := net.ListenPacket("udp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer socket.Close()
	done := make(chan struct{})
	go func() {
		defer close(done)
		buffer := make([]byte, 512)
		for {
			n, peer, err := socket.ReadFrom(buffer)
			if err != nil {
				return
			}
			if n < 12 {
				continue
			}
			response := append([]byte{}, buffer[:n]...)
			response[2] = 0x81
			response[3] = 0x83 // standard authoritative NXDOMAIN
			response[6] = 0
			response[7] = 0
			response[8] = 0
			response[9] = 0
			response[10] = 0
			response[11] = 0
			_, _ = socket.WriteTo(response, peer)
		}
	}()
	resolver := &net.Resolver{PreferGo: true, Dial: func(ctx context.Context, _, _ string) (net.Conn, error) {
		return (&net.Dialer{}).DialContext(ctx, "udp", socket.LocalAddr().String())
	}}
	c := fixtureCollector("https://native-dns-fixture.invalid")
	c.dialContext = (&net.Dialer{Resolver: resolver, Timeout: time.Second}).DialContext
	trace := runFixture(t, c, "direct")
	if trace.Outcome != "failed" || trace.ErrorCode != "dns_failed" || trace.FailurePhase == nil || *trace.FailurePhase != "dns" {
		t.Fatalf("native DNS failure absent: %+v", trace)
	}
	complete(t, phase(t, trace, "dns"))
	unknown(t, phase(t, trace, "tcp"))
	_ = socket.Close()
	<-done
}

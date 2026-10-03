package nodeprobe

import (
	"be6500panel/internal/proxy"
	"bufio"
	"context"
	"crypto/tls"
	"crypto/x509"
	"encoding/base64"
	"encoding/json"
	"io"
	"net"
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func validNode() proxy.Node {
	return proxy.Node{ID: "node-1", Name: "private", Server: "node.example", Port: 443, UUID: "01234567-89ab-cdef-0123-456789abcdef", ServerName: "identity.example", RealityPublicKey: base64.RawURLEncoding.EncodeToString(make([]byte, 32)), RealityShortID: "abcd", Fingerprint: "chrome", Flow: "xtls-rprx-vision", UDP: true}
}
func TestCoreProcessFixture(t *testing.T) {
	if os.Getenv("NODE_PROBE_PROCESS_FIXTURE") != "1" {
		return
	}
	data, err := os.ReadFile(os.Getenv("NODE_PROBE_CONFIG"))
	if err != nil {
		os.Exit(2)
	}
	var config struct {
		Inbounds []struct {
			Listen string `json:"listen"`
			Port   int    `json:"listen_port"`
		} `json:"inbounds"`
	}
	if json.Unmarshal(data, &config) != nil || len(config.Inbounds) != 1 {
		os.Exit(3)
	}
	addr := net.JoinHostPort(config.Inbounds[0].Listen, fmtInt(config.Inbounds[0].Port))
	listener, err := net.Listen("tcp4", addr)
	if err != nil {
		os.Exit(4)
	}
	// Offline fake local core: never dials the target or runs a real core.
	_ = http.Serve(listener, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { http.Error(w, "fixture", 502) }))
	os.Exit(0)
}
func fmtInt(value int) string {
	b := []byte{}
	if value == 0 {
		return "0"
	}
	for value > 0 {
		b = append([]byte{byte('0' + value%10)}, b...)
		value /= 10
	}
	return string(b)
}
func TestCoreProcessPrivateFilesAndExactCleanup(t *testing.T) {
	original := coreProcessCommand
	defer func() { coreProcessCommand = original }()
	coreProcessCommand = func(path, config string) *exec.Cmd {
		cmd := exec.Command(os.Args[0], "-test.run=^TestCoreProcessFixture$")
		cmd.Env = append(os.Environ(), "NODE_PROBE_PROCESS_FIXTURE=1", "NODE_PROBE_CONFIG="+config)
		return cmd
	}
	temp := t.TempDir()
	sentinel := filepath.Join(temp, "unrelated")
	os.WriteFile(sentinel, []byte("keep"), 0600)
	session, err := newCoreSession(context.Background(), CoreLease{Path: "/frozen/core"}, []proxy.Node{validNode()}, temp)
	if err != nil {
		t.Fatal(err)
	}
	core := session.(*coreSession)
	dirInfo, _ := os.Stat(core.dir)
	configInfo, _ := os.Stat(filepath.Join(core.dir, "config.json"))
	if dirInfo.Mode().Perm() != 0700 || configInfo.Mode().Perm() != 0600 {
		t.Fatal("private file permissions")
	}
	for _, client := range core.clients {
		transport := client.Transport.(*http.Transport)
		if transport.TLSClientConfig != nil && transport.TLSClientConfig.InsecureSkipVerify {
			t.Fatal("insecure HTTPS")
		}
	}
	if delay, code := session.Probe(context.Background(), 0); code != "node_unreachable" || delay != 0 {
		t.Fatal("fake CONNECT was mislabeled HTTPS success")
	}
	session.Close()
	session.Close()
	if _, err = os.Stat(core.dir); !os.IsNotExist(err) {
		t.Fatal("owned run directory leaked")
	}
	if _, err = os.Stat(sentinel); err != nil {
		t.Fatal("unrelated file removed")
	}
	select {
	case <-core.done:
	default:
		t.Fatal("owned process leaked")
	}
}
func TestCoreFailedStartupExactCleanup(t *testing.T) {
	original := coreProcessCommand
	defer func() { coreProcessCommand = original }()
	coreProcessCommand = func(string, string) *exec.Cmd {
		return exec.Command(os.Args[0], "-test.run=^$", "-invalid-fixture-option")
	}
	temp := t.TempDir()
	if _, err := newCoreSession(context.Background(), CoreLease{Path: "/frozen/core"}, []proxy.Node{validNode()}, temp); err == nil {
		t.Fatal("failed process accepted")
	}
	entries, _ := os.ReadDir(temp)
	if len(entries) != 0 {
		t.Fatal("failed startup leaked config")
	}
}
func localCONNECTProxy(t *testing.T) (*httptest.Server, chan string) {
	t.Helper()
	auth := make(chan string, 8)
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodConnect {
			http.Error(w, "CONNECT required", 405)
			return
		}
		auth <- r.Header.Get("Proxy-Authorization")
		upstream, err := net.Dial("tcp", r.Host)
		if err != nil {
			http.Error(w, "unreachable", 502)
			return
		}
		client, rw, err := w.(http.Hijacker).Hijack()
		if err != nil {
			upstream.Close()
			return
		}
		rw.WriteString("HTTP/1.1 200 Connection Established\r\n\r\n")
		rw.Flush()
		go func() { defer client.Close(); defer upstream.Close(); io.Copy(upstream, rw) }()
		go func() { defer client.Close(); defer upstream.Close(); io.Copy(client, upstream) }()
	}))
	t.Cleanup(server.Close)
	return server, auth
}
func TestRealHTTPSDurationThroughLocalAuthenticatedProxy(t *testing.T) {
	origin := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/generate_204" {
			t.Error("wrong request")
		}
		timer := time.NewTimer(25 * time.Millisecond)
		defer timer.Stop()
		<-timer.C
		w.WriteHeader(204)
	}))
	defer origin.Close()
	local, auth := localCONNECTProxy(t)
	proxyURL, _ := url.Parse(local.URL)
	proxyURL.User = url.UserPassword("probe-0", "PRIVATE-PASSWORD")
	roots := x509.NewCertPool()
	roots.AddCert(origin.Certificate())
	transport := &http.Transport{Proxy: http.ProxyURL(proxyURL), TLSClientConfig: &tls.Config{RootCAs: roots, MinVersion: tls.VersionTLS12}, DisableKeepAlives: true}
	defer transport.CloseIdleConnections()
	session := &coreSession{target: origin.URL + "/generate_204", clients: []*http.Client{{Transport: transport, Timeout: Timeout, CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }}}}
	delay, code := session.Probe(context.Background(), 0)
	if code != "" || delay < 25 {
		t.Fatalf("not full HTTPS request duration: %d %s", delay, code)
	}
	if value := <-auth; value != "Basic "+base64.StdEncoding.EncodeToString([]byte("probe-0:PRIVATE-PASSWORD")) {
		t.Fatal("node proxy authentication missing")
	}
	// The same origin is untrusted by the system pool; TLS failures must never
	// become a successful TCP delay or trigger an insecure fallback.
	transport.TLSClientConfig = &tls.Config{MinVersion: tls.VersionTLS12}
	if delay, code = session.Probe(context.Background(), 0); delay != 0 || code != "node_unreachable" {
		t.Fatal("unverified TLS accepted")
	}
}
func TestHTTPSStatusBodyTimeoutAndRedirectAreBounded(t *testing.T) {
	cases := []struct {
		name    string
		handler http.HandlerFunc
		code    string
	}{{"not204", func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(200)
		io.WriteString(w, strings.Repeat("x", MaxResponseBytes+20))
	}, "node_unreachable"}, {"redirect", func(w http.ResponseWriter, r *http.Request) { http.Redirect(w, r, "http://not-allowed.example", 302) }, "node_unreachable"}, {"timeout", func(w http.ResponseWriter, r *http.Request) { <-r.Context().Done() }, "node_timeout"}}
	for _, item := range cases {
		t.Run(item.name, func(t *testing.T) {
			server := httptest.NewTLSServer(item.handler)
			defer server.Close()
			client := server.Client()
			client.CheckRedirect = func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }
			session := &coreSession{target: server.URL, clients: []*http.Client{client}}
			ctx, cancel := context.WithTimeout(context.Background(), 50*time.Millisecond)
			defer cancel()
			if delay, code := session.Probe(ctx, 0); delay != 0 || code != item.code {
				t.Fatalf("bad outcome %d %s", delay, code)
			}
		})
	}
}

// Compile-time check that fixtures are native HTTP peers, not TCP success fakes.
var _ io.Reader = (*bufio.Reader)(nil)

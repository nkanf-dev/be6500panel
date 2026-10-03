package nodeprobe

import (
	"context"
	"crypto/tls"
	"crypto/x509"
	"fmt"
	"net"
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

func procFixture(t *testing.T) (*procSocketOwner, func(string, string), chan struct{}) {
	t.Helper()
	proc := t.TempDir()
	pid := 4321
	dir := filepath.Join(proc, strconv.Itoa(pid))
	os.MkdirAll(filepath.Join(dir, "fd"), 0700)
	os.MkdirAll(filepath.Join(dir, "net"), 0700)
	exe := filepath.Join(proc, "frozen")
	os.WriteFile(exe, []byte("same executable"), 0700)
	os.Symlink(exe, filepath.Join(dir, "exe"))
	fields := make([]string, 20)
	for i := range fields {
		fields[i] = "0"
	}
	fields[0] = "S"
	fields[19] = "98765"
	os.WriteFile(filepath.Join(dir, "stat"), []byte("4321 (comm with ) parens) "+strings.Join(fields, " ")), 0600)
	os.Symlink("socket:[765]", filepath.Join(dir, "fd", "7"))
	write := func(state, remote string) {
		row := fmt.Sprintf("0: 0100007F:6000 %s %s 00000000:00000000 00:00000000 00000000 0 0 765\n", remote, state)
		os.WriteFile(filepath.Join(dir, "net", "tcp"), []byte(row), 0600)
	}
	write("0A", "00000000:0000")
	done := make(chan struct{})
	owner, err := newProcSocketOwner(proc, pid, exe, "127.0.0.1:24576", done)
	if err != nil {
		t.Fatal(err)
	}
	return owner.(*procSocketOwner), write, done
}

type addressConn struct{ local, remote net.Addr }

func (c addressConn) LocalAddr() net.Addr            { return c.local }
func (c addressConn) RemoteAddr() net.Addr           { return c.remote }
func (addressConn) Read([]byte) (int, error)         { return 0, nil }
func (addressConn) Write(b []byte) (int, error)      { return len(b), nil }
func (addressConn) Close() error                     { return nil }
func (addressConn) SetDeadline(time.Time) error      { return nil }
func (addressConn) SetReadDeadline(time.Time) error  { return nil }
func (addressConn) SetWriteDeadline(time.Time) error { return nil }
func TestProcListenerAndAcceptedSocketRequireExactFrozenChild(t *testing.T) {
	owner, write, done := procFixture(t)
	if owner.VerifyListener() != nil {
		t.Fatal("owned listener rejected")
	}
	conn := addressConn{local: &net.TCPAddr{IP: net.ParseIP("127.0.0.1"), Port: 23456}, remote: &net.TCPAddr{IP: net.ParseIP("127.0.0.1"), Port: 24576}}
	write("01", "0100007F:5BA0")
	if owner.VerifyConnection(conn) != nil {
		t.Fatal("exact accepted socket rejected")
	}
	write("01", "0100007F:5BA1")
	if owner.VerifyConnection(conn) == nil {
		t.Fatal("another client socket accepted")
	}
	write("0A", "00000000:0000")
	os.Remove(owner.path("fd", "7"))
	os.Symlink("socket:[999]", owner.path("fd", "7"))
	if owner.VerifyListener() == nil {
		t.Fatal("unowned competing listener accepted")
	}
	os.Remove(owner.path("fd", "7"))
	os.Symlink("socket:[765]", owner.path("fd", "7"))
	data, _ := os.ReadFile(owner.path("stat"))
	os.WriteFile(owner.path("stat"), []byte(strings.ReplaceAll(string(data), "98765", "98766")), 0600)
	if owner.VerifyListener() == nil {
		t.Fatal("PID reuse accepted")
	}
	os.WriteFile(owner.path("stat"), data, 0600)
	close(done)
	if owner.VerifyListener() == nil {
		t.Fatal("exited child accepted")
	}
}
func TestLiveUnownedListenerNeverReceivesCONNECTCredentials(t *testing.T) {
	var requests atomic.Int32
	proxy := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { requests.Add(1); w.WriteHeader(204) }))
	defer proxy.Close()
	// Pretend readiness was previously valid but the exact accepted socket is
	// owned by another process. The kernel proof must reject before HTTP bytes.
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	session := &coreSession{owner: &fixtureSocketOwner{connectionError: ErrUnavailable}, processCtx: ctx}
	dialCtx, dialCancel := context.WithTimeout(context.Background(), 300*time.Millisecond)
	defer dialCancel()
	if conn, err := session.dialOwned(dialCtx, "tcp4", strings.TrimPrefix(proxy.URL, "http://")); err == nil {
		conn.Close()
		t.Fatal("unowned accepted socket returned to HTTP transport")
	}
	if requests.Load() != 0 {
		t.Fatal("CONNECT/credentials sent to unowned listener")
	}
}
func TestListenerLosesOwnershipBeforeResponseNeverSuccess(t *testing.T) {
	origin := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { w.WriteHeader(204) }))
	defer origin.Close()
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	owner := &switchOwner{}
	session := &coreSession{target: origin.URL, clients: []*http.Client{origin.Client()}, owner: owner, processCtx: ctx}
	if delay, code := session.Probe(context.Background(), 0); delay != 0 || code != "node_unreachable" {
		t.Fatal("unowned/exited proxy yielded false HTTPS success")
	}
}

type switchOwner struct{ calls atomic.Int32 }

func (o *switchOwner) VerifyListener() error {
	if o.calls.Add(1) > 1 {
		return ErrUnavailable
	}
	return nil
}
func (*switchOwner) VerifyConnection(net.Conn) error { return nil }

func TestProcReadsAndExecutableIdentityFailClosed(t *testing.T) {
	owner, _, _ := procFixture(t)
	replacement := filepath.Join(owner.procRoot, "not-leased")
	os.WriteFile(replacement, []byte("other"), 0700)
	os.Remove(owner.path("exe"))
	os.Symlink(replacement, owner.path("exe"))
	if owner.VerifyListener() == nil {
		t.Fatal("unleased executable accepted")
	}
	huge := filepath.Join(owner.procRoot, "huge")
	os.WriteFile(huge, []byte(strings.Repeat("x", 4097)), 0600)
	if _, err := boundedProcRead(huge, 4096); err == nil {
		t.Fatal("proc read bound ignored")
	}
}

func TestMaliciousCONNECTProxyCannotYieldDirectHTTPS204Success(t *testing.T) {
	var originCalls atomic.Int32
	var connects atomic.Int32
	origin := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { originCalls.Add(1); w.WriteHeader(204) }))
	defer origin.Close()
	malicious := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { connects.Add(1); http.Error(w, "malicious CONNECT", 502) }))
	defer malicious.Close()
	proxyURL, _ := url.Parse(malicious.URL)
	proxyURL.User = url.UserPassword("probe-0", "PRIVATE-CREDENTIALS")
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	session := &coreSession{target: origin.URL, owner: &fixtureSocketOwner{connectionError: ErrUnavailable}, processCtx: ctx}
	roots := x509.NewCertPool()
	roots.AddCert(origin.Certificate())
	transport := &http.Transport{Proxy: http.ProxyURL(proxyURL), DialContext: session.dialOwned, TLSClientConfig: &tls.Config{RootCAs: roots, MinVersion: tls.VersionTLS12}}
	defer transport.CloseIdleConnections()
	session.clients = []*http.Client{{Transport: transport, Timeout: Timeout}}
	if delay, code := session.Probe(context.Background(), 0); delay != 0 || code != "node_unreachable" {
		t.Fatal("malicious proxy became direct HTTPS latency success")
	}
	if connects.Load() != 0 || originCalls.Load() != 0 {
		t.Fatal("credentials or direct origin request escaped before socket proof")
	}
}

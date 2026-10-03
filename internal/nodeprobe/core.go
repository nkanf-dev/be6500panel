package nodeprobe

import (
	"be6500panel/internal/proxy"
	"context"
	"crypto/rand"
	"encoding/hex"
	"errors"
	"io"
	"net"
	"net/http"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"sync"
	"time"
)

const MaxResponseBytes = 1024
const startupTimeout = 5 * time.Second

type coreSession struct {
	dir        string
	cmd        *exec.Cmd
	done       chan struct{}
	process    *ownedProcess
	owner      socketOwner
	processCtx context.Context
	closeOnce  sync.Once
	clients    []*http.Client
	transports []*http.Transport
	target     string
}

// coreProcessCommand is replaced only by offline process fixtures in tests.
var coreProcessCommand = func(path, config string) *exec.Cmd { return exec.Command(path, "run", "--config", config) }

func newCoreSession(ctx context.Context, lease CoreLease, nodes []proxy.Node, tempRoot string) (probeSession, error) {
	if !filepath.IsAbs(lease.Path) {
		return nil, ErrUnavailable
	}
	dir, err := os.MkdirTemp(tempRoot, "be6500panel-node-probe-")
	if err != nil {
		return nil, ErrUnavailable
	}
	session := &coreSession{dir: dir, target: Target}
	success := false
	defer func() {
		if !success {
			session.Close()
		}
	}()
	if os.Chmod(dir, 0700) != nil {
		return nil, ErrUnavailable
	}
	listener, err := net.Listen("tcp4", "127.0.0.1:0")
	if err != nil {
		return nil, ErrUnavailable
	}
	address := listener.Addr().String()
	token := make([]byte, 32)
	if _, err = rand.Read(token); err != nil {
		listener.Close()
		return nil, ErrUnavailable
	}
	password := hex.EncodeToString(token)
	config, err := proxy.CompileProbe(proxy.ProbeCompileInput{Nodes: nodes, ListenAddress: address, Password: password})
	if err != nil {
		listener.Close()
		return nil, ErrUnavailable
	}
	configPath := filepath.Join(dir, "config.json")
	if err = os.WriteFile(configPath, config, 0600); err != nil {
		listener.Close()
		return nil, ErrUnavailable
	}
	// Port reservation alone is not listener authentication. Before any proxy
	// credentials are written, prove the accepted socket belongs to the exact
	// frozen executable process; a competing loopback listener fails closed.
	listener.Close()
	cmd := coreProcessCommand(lease.Path, configPath)
	cmd.Dir = dir
	// This is one temporary Go core, not one core per node. Keep its GC target
	// small for the router; node/outbound count and in-flight requests are fixed.
	cmd.Env = append(cmd.Environ(), "GOMEMLIMIT=48MiB", "GOGC=50")
	cmd.Stdout = io.Discard
	cmd.Stderr = io.Discard
	cmd.WaitDelay = 250 * time.Millisecond
	process, err := startOwnedProcess(cmd)
	if err != nil {
		return nil, ErrUnavailable
	}
	session.cmd = cmd
	session.process = process
	session.done = process.done
	session.processCtx = process.ctx
	owner, err := coreSocketOwnerFactory(cmd.Process.Pid, lease.Path, address, process.exited)
	if err != nil {
		return nil, ErrUnavailable
	}
	session.owner = owner
	startupCtx, cancel := context.WithTimeout(ctx, startupTimeout)
	defer cancel()
	stopProcessCancel := context.AfterFunc(process.ctx, cancel)
	defer stopProcessCancel()
	ticker := time.NewTicker(30 * time.Millisecond)
	defer ticker.Stop()
	for {
		if startupCtx.Err() != nil || process.ctx.Err() != nil {
			return nil, ErrUnavailable
		}
		if owner.VerifyListener() == nil {
			break
		}
		select {
		case <-ticker.C:
		case <-process.exited:
			return nil, ErrUnavailable
		case <-startupCtx.Done():
			return nil, ErrUnavailable
		}
	}
	// Each request uses its own local authenticated proxy user. TLS to the fixed
	// public target is verified by Go's default trust store, with no insecure mode.
	for i := range nodes {
		proxyURL := &url.URL{Scheme: "http", Host: address, User: url.UserPassword(proxy.ProbeTag(i), password)}
		transport := &http.Transport{Proxy: http.ProxyURL(proxyURL), DialContext: session.dialOwned, TLSHandshakeTimeout: Timeout, ResponseHeaderTimeout: Timeout, DisableKeepAlives: true, MaxConnsPerHost: 1, ForceAttemptHTTP2: false}
		session.transports = append(session.transports, transport)
		session.clients = append(session.clients, &http.Client{Transport: transport, Timeout: Timeout, CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }})
	}
	success = true
	return session, nil
}
func (s *coreSession) Probe(ctx context.Context, index int) (int64, string) {
	if index < 0 || index >= len(s.clients) {
		return 0, "node_unreachable"
	}
	requestCtx, cancel := context.WithCancel(ctx)
	defer cancel()
	stopProcessCancel := func() bool { return true }
	if s.processCtx != nil {
		if s.processCtx.Err() != nil || s.owner == nil || s.owner.VerifyListener() != nil {
			return 0, "node_unreachable"
		}
		stopProcessCancel = context.AfterFunc(s.processCtx, cancel)
	}
	defer stopProcessCancel()
	request, err := http.NewRequestWithContext(requestCtx, http.MethodGet, s.target, nil)
	if err != nil {
		return 0, "node_unreachable"
	}
	request.Header.Set("User-Agent", "be6500panel-node-probe/1")
	began := time.Now()
	response, err := s.clients[index].Do(request)
	if err != nil {
		if errors.Is(err, context.DeadlineExceeded) || ctx.Err() == context.DeadlineExceeded {
			return 0, "node_timeout"
		}
		var netErr net.Error
		if errors.As(err, &netErr) && netErr.Timeout() {
			return 0, "node_timeout"
		}
		return 0, "node_unreachable"
	}
	defer response.Body.Close()
	read, err := io.Copy(io.Discard, io.LimitReader(response.Body, MaxResponseBytes+1))
	if errors.Is(err, context.DeadlineExceeded) || ctx.Err() == context.DeadlineExceeded {
		return 0, "node_timeout"
	}
	if err != nil || read > MaxResponseBytes || response.StatusCode != http.StatusNoContent {
		return 0, "node_unreachable"
	}
	if ctx.Err() != nil {
		if ctx.Err() == context.DeadlineExceeded {
			return 0, "node_timeout"
		}
		return 0, "node_cancelled"
	}
	if s.processCtx != nil && (s.processCtx.Err() != nil || s.owner.VerifyListener() != nil) {
		return 0, "node_unreachable"
	}
	// Full verified HTTPS request duration, not TCP connect latency. Genuine
	// sub-millisecond observations are allowed to round to zero; unknown is nil.
	return time.Since(began).Milliseconds(), ""
}
func (s *coreSession) Close() {
	s.closeOnce.Do(func() {
		for _, transport := range s.transports {
			transport.CloseIdleConnections()
		}
		if s.process != nil {
			s.process.Close()
		}
		if s.dir != "" {
			_ = os.RemoveAll(s.dir)
		}
	})
}

func (s *coreSession) dialOwned(ctx context.Context, network, address string) (net.Conn, error) {
	if s.owner == nil || s.processCtx == nil || s.processCtx.Err() != nil || s.owner.VerifyListener() != nil {
		return nil, ErrUnavailable
	}
	dialCtx, cancel := context.WithCancel(ctx)
	defer cancel()
	stopCancel := context.AfterFunc(s.processCtx, cancel)
	defer stopCancel()
	conn, err := (&net.Dialer{Timeout: Timeout}).DialContext(dialCtx, network, address)
	if err != nil {
		return nil, ErrUnavailable
	}
	// TCP connect sends no proxy credentials. Only the exact child-owned accepted
	// socket permits this connection to be used by HTTP CONNECT/Proxy-Authorization.
	verifyCtx, verifyCancel := context.WithTimeout(dialCtx, 250*time.Millisecond)
	defer verifyCancel()
	if awaitOwnedConnection(verifyCtx, s.owner, conn) != nil || s.processCtx.Err() != nil {
		conn.Close()
		return nil, ErrUnavailable
	}
	// Close the actual connection on child exit, even while CONNECT/TLS is running.
	stopClose := context.AfterFunc(s.processCtx, func() { conn.Close() })
	return &ownedConn{Conn: conn, stop: stopClose}, nil
}

type ownedConn struct {
	net.Conn
	stop func() bool
}

func (c *ownedConn) Close() error { c.stop(); return c.Conn.Close() }

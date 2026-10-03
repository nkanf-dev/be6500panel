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
	// Reserve the port until immediately before core startup. A private random
	// proxy password prevents an unrelated local listener returning fake success.
	listener.Close()
	cmd := coreProcessCommand(lease.Path, configPath)
	cmd.Dir = dir
	// This is one temporary Go core, not one core per node. Keep its GC target
	// small for the router; node/outbound count and in-flight requests are fixed.
	cmd.Env = append(cmd.Environ(), "GOMEMLIMIT=48MiB", "GOGC=50")
	cmd.Stdout = io.Discard
	cmd.Stderr = io.Discard
	if err = prepareProcessGroup(cmd); err != nil {
		return nil, ErrUnavailable
	}
	if err = cmd.Start(); err != nil {
		return nil, ErrUnavailable
	}
	session.cmd = cmd
	session.done = make(chan struct{})
	go func() { _ = cmd.Wait(); close(session.done) }()
	startupCtx, cancel := context.WithTimeout(ctx, startupTimeout)
	defer cancel()
	ticker := time.NewTicker(30 * time.Millisecond)
	defer ticker.Stop()
	for {
		select {
		case <-session.done:
			return nil, ErrUnavailable
		case <-startupCtx.Done():
			return nil, ErrUnavailable
		default:
		}
		conn, dialErr := (&net.Dialer{Timeout: 50 * time.Millisecond}).DialContext(startupCtx, "tcp4", address)
		if dialErr == nil {
			conn.Close()
			break
		}
		select {
		case <-ticker.C:
		case <-session.done:
			return nil, ErrUnavailable
		case <-startupCtx.Done():
			return nil, ErrUnavailable
		}
	}
	// Each request uses its own local authenticated proxy user. TLS to the fixed
	// public target is verified by Go's default trust store, with no insecure mode.
	for i := range nodes {
		proxyURL := &url.URL{Scheme: "http", Host: address, User: url.UserPassword(proxy.ProbeTag(i), password)}
		transport := &http.Transport{Proxy: http.ProxyURL(proxyURL), DialContext: (&net.Dialer{Timeout: Timeout}).DialContext, TLSHandshakeTimeout: Timeout, ResponseHeaderTimeout: Timeout, DisableKeepAlives: true, MaxConnsPerHost: 1, ForceAttemptHTTP2: false}
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
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, s.target, nil)
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
	// Full verified HTTPS request duration, not TCP connect latency. Genuine
	// sub-millisecond observations are allowed to round to zero; unknown is nil.
	return time.Since(began).Milliseconds(), ""
}
func (s *coreSession) Close() {
	s.closeOnce.Do(func() {
		for _, transport := range s.transports {
			transport.CloseIdleConnections()
		}
		if s.cmd != nil {
			// Signal this owned group even if the leader exited: helpers must
			// not survive a failed/short-lived core process.
			terminateProcessGroup(s.cmd, false)
			timer := time.NewTimer(250 * time.Millisecond)
			select {
			case <-s.done:
				timer.Stop()
				terminateProcessGroup(s.cmd, true)
			case <-timer.C:
				terminateProcessGroup(s.cmd, true)
				<-s.done
			}
		}
		if s.dir != "" {
			_ = os.RemoveAll(s.dir)
		}
	})
}

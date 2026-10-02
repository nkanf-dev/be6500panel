// Package transport provides deployment-specific verified artifact transfer adapters.
package transport

import (
	"context"
	"errors"
	"io"
	"net/http"
	"os/exec"
	"strconv"
	"sync"
	"syscall"
	"time"
)

// CurlTransport uses the router's TLS stack for bootstrap downloads. It does not
// disable certificate validation; the runtime manager still validates every digest.
// It is deliberately limited to credential-free HTTPS GET artifact bodies.
type CurlTransport struct {
	Executable string
	Timeout    time.Duration
}

func (t CurlTransport) RoundTrip(request *http.Request) (*http.Response, error) {
	if request.Method != http.MethodGet || request.URL.Scheme != "https" || request.URL.User != nil || request.URL.Hostname() == "" {
		return nil, errors.New("artifact transport requires HTTPS GET")
	}
	executable := t.Executable
	if executable == "" {
		executable = "/usr/bin/curl"
	}
	timeout := t.Timeout
	if timeout == 0 {
		timeout = 6 * time.Minute
	}
	ctx, cancel := context.WithCancel(request.Context())
	cmd := exec.CommandContext(ctx, executable, "--fail", "--silent", "--show-error", "--location", "--proto", "=https", "--proto-redir", "=https", "--ipv4", "--connect-timeout", "20", "--max-time", strconv.Itoa(int(timeout.Seconds())), "--header", "Accept-Encoding: identity", "--header", "Accept: application/octet-stream", "--url", request.URL.String())
	cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}
	cmd.Cancel = func() error {
		if cmd.Process == nil {
			return nil
		}
		return syscall.Kill(-cmd.Process.Pid, syscall.SIGKILL)
	}
	cmd.WaitDelay = time.Second
	reader, writer := io.Pipe()
	cmd.Stdout = writer
	cmd.Stderr = io.Discard
	if err := cmd.Start(); err != nil {
		cancel()
		reader.Close()
		writer.Close()
		return nil, errors.New("artifact TLS transport unavailable")
	}
	done := make(chan error, 1)
	go func() {
		err := cmd.Wait()
		if err != nil {
			err = errors.New("artifact TLS transfer failed")
		}
		writer.CloseWithError(err)
		done <- err
		close(done)
	}()
	body := &curlBody{reader: reader, cancel: cancel, done: done}
	return &http.Response{StatusCode: http.StatusOK, Status: "200 OK", Header: http.Header{}, Body: body, ContentLength: -1, Request: request}, nil
}

type curlBody struct {
	reader   *io.PipeReader
	cancel   context.CancelFunc
	done     <-chan error
	once     sync.Once
	closeErr error
}

func (b *curlBody) Read(p []byte) (int, error) { return b.reader.Read(p) }
func (b *curlBody) Close() error {
	b.once.Do(func() { b.cancel(); b.reader.Close(); b.closeErr = <-b.done })
	return b.closeErr
}

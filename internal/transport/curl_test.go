package transport

import (
	"context"
	"io"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func fakeCurl(t *testing.T, body string) string {
	t.Helper()
	p := filepath.Join(t.TempDir(), "curl")
	if err := os.WriteFile(p, []byte("#!/bin/sh\n"+body+"\n"), 0700); err != nil {
		t.Fatal(err)
	}
	return p
}
func TestCurlTransferAndFailure(t *testing.T) {
	for _, f := range []struct {
		body    string
		want    string
		failure bool
	}{{"printf verified", "verified", false}, {"exit 22", "", true}} {
		transport := CurlTransport{Executable: fakeCurl(t, f.body)}
		req, _ := http.NewRequest("GET", "https://example.com/artifact", nil)
		res, err := transport.RoundTrip(req)
		if err != nil {
			t.Fatal(err)
		}
		raw, err := io.ReadAll(res.Body)
		_ = res.Body.Close()
		if f.failure {
			if err == nil {
				t.Fatal("exit failure accepted")
			}
		} else if err != nil || string(raw) != f.want {
			t.Fatal(string(raw), err)
		}
	}
}
func TestCurlCancellation(t *testing.T) {
	transport := CurlTransport{Executable: fakeCurl(t, "sleep 30")}
	ctx, cancel := context.WithCancel(context.Background())
	req, _ := http.NewRequestWithContext(ctx, "GET", "https://example.com/artifact", nil)
	res, err := transport.RoundTrip(req)
	if err != nil {
		t.Fatal(err)
	}
	cancel()
	start := time.Now()
	res.Body.Close()
	if time.Since(start) > 2*time.Second {
		t.Fatal("cancel not bounded")
	}
}
func TestCurlRejectsNonArtifactMethods(t *testing.T) {
	for _, raw := range []string{"http://example.com/a", "https://user:secret@example.com/a"} {
		u, _ := url.Parse(raw)
		_, err := (CurlTransport{}).RoundTrip(&http.Request{Method: "GET", URL: u})
		if err == nil {
			t.Fatal("accepted", raw)
		}
	}
	req, _ := http.NewRequest("POST", "https://example.com/a", strings.NewReader("body"))
	if _, err := (CurlTransport{}).RoundTrip(req); err == nil {
		t.Fatal("POST")
	}
}

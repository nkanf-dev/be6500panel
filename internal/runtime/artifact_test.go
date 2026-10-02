package runtime

import (
	"bytes"
	"compress/gzip"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func artifactTestDigest(data []byte) string {
	sum := sha256.Sum256(data)
	return hex.EncodeToString(sum[:])
}

func artifactTestGzip(t *testing.T, data []byte) []byte {
	t.Helper()
	var compressed bytes.Buffer
	writer := gzip.NewWriter(&compressed)
	if _, err := writer.Write(data); err != nil {
		t.Fatal(err)
	}
	if err := writer.Close(); err != nil {
		t.Fatal(err)
	}
	return compressed.Bytes()
}

func artifactTestOptions(t *testing.T) Options {
	t.Helper()
	return Options{
		RunDir:               t.TempDir(),
		AllowLoopbackHTTP:    true,
		MaxCompressedBytes:   16 << 20,
		MaxUncompressedBytes: 40 << 20,
	}
}

func artifactTestCleanFailure(t *testing.T, opts Options, artifact Artifact, want string) {
	t.Helper()
	path, err := acquireArtifact(context.Background(), opts, "test", artifact)
	if err == nil || !strings.Contains(err.Error(), want) {
		t.Fatalf("acquireArtifact() = %q, %v; want error containing %q", path, err, want)
	}
	if path != "" {
		t.Fatalf("failed acquisition returned path %q", path)
	}
	entries, err := os.ReadDir(opts.RunDir)
	if err != nil && !errors.Is(err, os.ErrNotExist) {
		t.Fatal(err)
	}
	if len(entries) != 0 {
		t.Fatalf("failed acquisition left staging files: %v", entries)
	}
}

func TestAcquireArtifactHTTPAndGzip(t *testing.T) {
	payload := []byte("#!/bin/sh\nprintf 'artifact test\\n'\n")
	for _, compression := range []string{"none", "gzip"} {
		t.Run(compression, func(t *testing.T) {
			fetched := payload
			if compression == "gzip" {
				fetched = artifactTestGzip(t, payload)
			}
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				if got := r.Header.Get("Accept-Encoding"); got != "identity" {
					t.Errorf("Accept-Encoding = %q, want identity", got)
				}
				_, _ = w.Write(fetched)
			}))
			defer server.Close()
			opts := artifactTestOptions(t)
			opts.MaxCompressedBytes = int64(len(fetched))
			opts.MaxUncompressedBytes = int64(len(payload))
			existing := filepath.Join(opts.RunDir, "test")
			if err := os.WriteFile(existing, []byte("live executable"), 0700); err != nil {
				t.Fatal(err)
			}
			artifact := Artifact{URL: server.URL, SHA256: strings.ToUpper(artifactTestDigest(fetched)), Compression: compression}
			path, err := acquireArtifact(context.Background(), opts, "test", artifact)
			if err != nil {
				t.Fatal(err)
			}
			if path == existing || filepath.Dir(path) != opts.RunDir {
				t.Fatalf("invalid staging path %q", path)
			}
			got, err := os.ReadFile(path)
			if err != nil || !bytes.Equal(got, payload) {
				t.Fatalf("staged payload = %q, %v; want %q", got, err, payload)
			}
			info, err := os.Stat(path)
			if err != nil || info.Mode().Perm() != 0700 {
				t.Fatalf("staged permissions = %v, %v; want 0700", info, err)
			}
			live, err := os.ReadFile(existing)
			if err != nil || string(live) != "live executable" {
				t.Fatalf("existing executable changed: %q, %v", live, err)
			}
			second, err := acquireArtifact(context.Background(), opts, "test", artifact)
			if err != nil || second == path {
				t.Fatalf("second staging path = %q, %v; want a new file", second, err)
			}
		})
	}
}

func TestAcquireArtifactHTTPSDefault(t *testing.T) {
	payload := []byte("executable")
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_, _ = w.Write(payload)
	}))
	defer server.Close()
	opts := artifactTestOptions(t)
	opts.AllowLoopbackHTTP = false
	opts.HTTPClient = server.Client()
	path, err := acquireArtifact(context.Background(), opts, "test", Artifact{
		URL: server.URL, SHA256: artifactTestDigest(payload), Compression: "none",
	})
	if err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(path); err != nil {
		t.Fatal(err)
	}
}

func TestAcquireArtifactRejectsInvalidMetadataAndURLs(t *testing.T) {
	validDigest := artifactTestDigest([]byte("test"))
	cases := []struct {
		name      string
		artifact  Artifact
		allowHTTP bool
		want      string
	}{
		{"short digest", Artifact{URL: "https://example.com/tool", SHA256: "abc", Compression: "none"}, false, "SHA256"},
		{"nonhex digest", Artifact{URL: "https://example.com/tool", SHA256: strings.Repeat("z", 64), Compression: "none"}, false, "SHA256"},
		{"compression", Artifact{URL: "https://example.com/tool", SHA256: validDigest, Compression: "zip"}, false, "compression"},
		{"missing compression", Artifact{URL: "https://example.com/tool", SHA256: validDigest}, false, "compression"},
		{"http disabled", Artifact{URL: "http://127.0.0.1/tool", SHA256: validDigest, Compression: "none"}, false, "HTTP"},
		{"nonloopback", Artifact{URL: "http://192.0.2.1/tool", SHA256: validDigest, Compression: "none"}, true, "loopback"},
		{"hostname loopback", Artifact{URL: "http://localhost/tool", SHA256: validDigest, Compression: "none"}, true, "loopback"},
		{"credentials", Artifact{URL: "https://user:pass@example.com/tool", SHA256: validDigest, Compression: "none"}, false, "credentials"},
		{"fragment", Artifact{URL: "https://example.com/tool#fragment", SHA256: validDigest, Compression: "none"}, false, "fragment"},
		{"empty fragment", Artifact{URL: "https://example.com/tool#", SHA256: validDigest, Compression: "none"}, false, "fragment"},
		{"ftp", Artifact{URL: "ftp://example.com/tool", SHA256: validDigest, Compression: "none"}, false, "scheme"},
		{"relative", Artifact{URL: "/tool", SHA256: validDigest, Compression: "none"}, false, "absolute"},
		{"opaque", Artifact{URL: "https:example.com/tool", SHA256: validDigest, Compression: "none"}, false, "absolute"},
		{"untrusted file", Artifact{URL: "file:///tmp/tool", SHA256: validDigest, Compression: "none"}, false, "LocalSourceRoot"},
	}
	for _, test := range cases {
		t.Run(test.name, func(t *testing.T) {
			opts := artifactTestOptions(t)
			opts.AllowLoopbackHTTP = test.allowHTTP
			artifactTestCleanFailure(t, opts, test.artifact, test.want)
		})
	}
}

func TestAcquireArtifactLimitsDigestAndCorruption(t *testing.T) {
	payload := bytes.Repeat([]byte("a"), 1024)
	compressed := artifactTestGzip(t, payload)
	corrupt := bytes.Clone(compressed)
	corrupt[len(corrupt)-8] ^= 0xff
	cases := []struct {
		name              string
		body              []byte
		compression       string
		compressedLimit   int64
		uncompressedLimit int64
		chunked           bool
		digest            string
		want              string
	}{
		{"content length", payload, "none", 100, 2048, false, "", "compressed"},
		{"streamed compressed", payload, "none", 100, 2048, true, "", "compressed"},
		{"raw extracted", payload, "none", 2048, 100, false, "", "uncompressed"},
		{"gzip bomb", compressed, "gzip", 2048, 100, false, "", "uncompressed"},
		{"gzip compressed", compressed, "gzip", int64(len(compressed) - 1), 2048, true, "", "compressed"},
		{"digest mismatch", payload, "none", 2048, 2048, false, artifactTestDigest([]byte("different")), "SHA256"},
		{"gzip hashes compressed bytes", compressed, "gzip", 2048, 2048, false, artifactTestDigest(payload), "SHA256"},
		{"gzip trailing bytes", append(bytes.Clone(compressed), []byte("garbage")...), "gzip", 2048, 2048, false, "", "EOF"},
		{"corrupt checksum", corrupt, "gzip", 2048, 2048, false, "", "checksum"},
		{"truncated gzip", compressed[:len(compressed)-4], "gzip", 2048, 2048, false, "", "EOF"},
		{"invalid gzip", []byte("not a gzip"), "gzip", 2048, 2048, false, "", "gzip"},
	}
	for _, test := range cases {
		t.Run(test.name, func(t *testing.T) {
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				if test.chunked {
					w.(http.Flusher).Flush()
				} else {
					w.Header().Set("Content-Length", fmt.Sprint(len(test.body)))
				}
				_, _ = w.Write(test.body)
			}))
			defer server.Close()
			opts := artifactTestOptions(t)
			opts.MaxCompressedBytes = test.compressedLimit
			opts.MaxUncompressedBytes = test.uncompressedLimit
			digest := test.digest
			if digest == "" {
				digest = artifactTestDigest(test.body)
			}
			artifactTestCleanFailure(t, opts, Artifact{URL: server.URL, SHA256: digest, Compression: test.compression}, test.want)
		})
	}
}

func TestAcquireArtifactRedirects(t *testing.T) {
	payload := []byte("executable")
	targetHits := 0
	target := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		targetHits++
		_, _ = w.Write(payload)
	}))
	defer target.Close()
	for _, test := range []struct{ name, target, want string }{
		{"allowed", target.URL, ""},
		{"hostname", strings.Replace(target.URL, "127.0.0.1", "localhost", 1), "loopback"},
		{"fragment", target.URL + "#fragment", "fragment"},
		{"empty fragment", target.URL + "#", "fragment"},
		{"credentials", strings.Replace(target.URL, "http://", "http://user:pass@", 1), "credentials"},
		{"file", "file:///tmp/tool", "scheme"},
	} {
		t.Run(test.name, func(t *testing.T) {
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				http.Redirect(w, r, test.target, http.StatusFound)
			}))
			defer server.Close()
			opts := artifactTestOptions(t)
			artifact := Artifact{URL: server.URL, SHA256: artifactTestDigest(payload), Compression: "none"}
			before := targetHits
			if test.want != "" {
				artifactTestCleanFailure(t, opts, artifact, test.want)
				if targetHits != before {
					t.Fatal("unsafe redirect reached the target")
				}
			} else if _, err := acquireArtifact(context.Background(), opts, "test", artifact); err != nil {
				t.Fatal(err)
			}
		})
	}
}

func TestAcquireArtifactTrustedFiles(t *testing.T) {
	root := t.TempDir()
	payload := []byte("local executable")
	inside := filepath.Join(root, "tool")
	if err := os.WriteFile(inside, payload, 0600); err != nil {
		t.Fatal(err)
	}
	outside := filepath.Join(t.TempDir(), "tool")
	if err := os.WriteFile(outside, payload, 0600); err != nil {
		t.Fatal(err)
	}
	link := filepath.Join(root, "escape")
	if err := os.Symlink(outside, link); err != nil {
		t.Fatal(err)
	}
	for _, test := range []struct{ name, path, want string }{
		{"inside", inside, ""},
		{"outside", outside, "outside"},
		{"symlink", link, "outside"},
		{"directory", root, "regular file"},
	} {
		t.Run(test.name, func(t *testing.T) {
			opts := artifactTestOptions(t)
			opts.LocalSourceRoot = root
			artifact := Artifact{URL: (&url.URL{Scheme: "file", Path: test.path}).String(), SHA256: artifactTestDigest(payload), Compression: "none"}
			if test.want != "" {
				artifactTestCleanFailure(t, opts, artifact, test.want)
			} else {
				path, err := acquireArtifact(context.Background(), opts, "test", artifact)
				if err != nil {
					t.Fatal(err)
				}
				got, err := os.ReadFile(path)
				if err != nil || !bytes.Equal(got, payload) {
					t.Fatalf("local staging = %q, %v", got, err)
				}
			}
		})
	}
}

func TestAcquireArtifactCancellation(t *testing.T) {
	started := make(chan struct{})
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
		w.(http.Flusher).Flush()
		close(started)
		<-r.Context().Done()
	}))
	defer server.Close()
	opts := artifactTestOptions(t)
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	go func() {
		select {
		case <-started:
			cancel()
		case <-ctx.Done():
		}
	}()
	path, err := acquireArtifact(ctx, opts, "test", Artifact{
		URL: server.URL, SHA256: artifactTestDigest(nil), Compression: "none",
	})
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("canceled acquisition = %q, %v; want context.Canceled", path, err)
	}
	entries, readErr := os.ReadDir(opts.RunDir)
	if readErr != nil || len(entries) != 0 {
		t.Fatalf("cancellation left staging files: %v, %v", entries, readErr)
	}
}

func TestAcquireArtifactRespectsHTTPClientRedirectPolicy(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		http.Redirect(w, r, "/again", http.StatusFound)
	}))
	defer server.Close()
	opts := artifactTestOptions(t)
	denied := errors.New("caller blocked redirect")
	client := &http.Client{CheckRedirect: func(r *http.Request, via []*http.Request) error { return denied }}
	opts.HTTPClient = client
	_, err := acquireArtifact(context.Background(), opts, "test", Artifact{
		URL: server.URL, SHA256: artifactTestDigest(nil), Compression: "none",
	})
	if !errors.Is(err, denied) {
		t.Fatalf("client redirect policy not retained: %v", err)
	}
	if err := client.CheckRedirect(nil, nil); !errors.Is(err, denied) {
		t.Fatalf("shared client's policy was mutated: %v", err)
	}
}

func TestAcquireArtifactResponseErrors(t *testing.T) {
	for _, status := range []int{http.StatusNotFound, http.StatusNoContent} {
		t.Run(fmt.Sprint(status), func(t *testing.T) {
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				w.WriteHeader(status)
			}))
			defer server.Close()
			opts := artifactTestOptions(t)
			artifactTestCleanFailure(t, opts, Artifact{URL: server.URL, SHA256: artifactTestDigest(nil), Compression: "none"}, "status")
		})
	}
}

// This transport exposes close failures without requiring a network failure.
type artifactTestRoundTripper func(*http.Request) (*http.Response, error)

func (transport artifactTestRoundTripper) RoundTrip(r *http.Request) (*http.Response, error) {
	return transport(r)
}

type artifactTestReadCloser struct {
	io.Reader
	closeErr error
}

func (body artifactTestReadCloser) Close() error { return body.closeErr }

func TestAcquireArtifactCloseErrorRemovesStage(t *testing.T) {
	payload := []byte("executable")
	closeErr := errors.New("failed source close")
	opts := artifactTestOptions(t)
	opts.HTTPClient = &http.Client{Transport: artifactTestRoundTripper(func(r *http.Request) (*http.Response, error) {
		return &http.Response{
			StatusCode:    http.StatusOK,
			Header:        make(http.Header),
			Body:          artifactTestReadCloser{Reader: bytes.NewReader(payload), closeErr: closeErr},
			ContentLength: int64(len(payload)),
			Request:       r,
		}, nil
	})}
	artifactTestCleanFailure(t, opts, Artifact{
		URL: "https://example.com/tool", SHA256: artifactTestDigest(payload), Compression: "none",
	}, closeErr.Error())
}

func TestAcquireArtifactGzipMembersAndDefaultLimits(t *testing.T) {
	first := []byte("first")
	second := []byte("second")
	compressed := append(artifactTestGzip(t, first), artifactTestGzip(t, second)...)
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_, _ = w.Write(compressed)
	}))
	defer server.Close()
	opts := artifactTestOptions(t)
	opts.MaxCompressedBytes = 0
	opts.MaxUncompressedBytes = 0
	path, err := acquireArtifact(context.Background(), opts, "test", Artifact{
		URL: server.URL, SHA256: artifactTestDigest(compressed), Compression: "gzip",
	})
	if err != nil {
		t.Fatal(err)
	}
	got, err := os.ReadFile(path)
	if err != nil || !bytes.Equal(got, append(first, second...)) {
		t.Fatalf("gzip members = %q, %v; want firstsecond", got, err)
	}
}

func TestAcquireArtifactRedirectPolicyCannotChangeTrust(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		http.Redirect(w, r, "/next", http.StatusFound)
	}))
	defer server.Close()
	opts := artifactTestOptions(t)
	opts.HTTPClient = &http.Client{CheckRedirect: func(r *http.Request, via []*http.Request) error {
		r.URL.Host = "192.0.2.1"
		return nil
	}}
	artifactTestCleanFailure(t, opts, Artifact{
		URL: server.URL, SHA256: artifactTestDigest(nil), Compression: "none",
	}, "loopback")
}

func TestAcquireArtifactRejectsHTTPSRedirectDowngrade(t *testing.T) {
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		http.Redirect(w, r, "http://127.0.0.1:1/tool", http.StatusFound)
	}))
	defer server.Close()
	opts := artifactTestOptions(t)
	opts.AllowLoopbackHTTP = false
	opts.HTTPClient = server.Client()
	artifactTestCleanFailure(t, opts, Artifact{
		URL: server.URL, SHA256: artifactTestDigest(nil), Compression: "none",
	}, "HTTP")
}

func TestAcquireArtifactFilePathTraversal(t *testing.T) {
	parent := t.TempDir()
	root := filepath.Join(parent, "trusted")
	if err := os.Mkdir(root, 0700); err != nil {
		t.Fatal(err)
	}
	outside := filepath.Join(parent, "tool")
	payload := []byte("executable")
	if err := os.WriteFile(outside, payload, 0600); err != nil {
		t.Fatal(err)
	}
	opts := artifactTestOptions(t)
	opts.LocalSourceRoot = root
	artifactTestCleanFailure(t, opts, Artifact{
		URL:    (&url.URL{Scheme: "file", Path: root + "/../tool"}).String(),
		SHA256: artifactTestDigest(payload), Compression: "none",
	}, "outside")
}

func TestAcquireArtifactInvalidOptionsAndCanceledContext(t *testing.T) {
	artifact := Artifact{URL: "https://example.com/tool", SHA256: artifactTestDigest(nil), Compression: "none"}
	for _, field := range []string{"compressed", "uncompressed"} {
		t.Run(field, func(t *testing.T) {
			opts := artifactTestOptions(t)
			if field == "compressed" {
				opts.MaxCompressedBytes = -1
			} else {
				opts.MaxUncompressedBytes = -1
			}
			artifactTestCleanFailure(t, opts, artifact, "positive")
		})
	}
	opts := artifactTestOptions(t)
	if _, err := acquireArtifact(nil, opts, "test", artifact); err == nil {
		t.Fatal("nil context accepted")
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if path, err := acquireArtifact(ctx, opts, "test", artifact); path != "" || !errors.Is(err, context.Canceled) {
		t.Fatalf("pre-canceled acquisition = %q, %v", path, err)
	}
}

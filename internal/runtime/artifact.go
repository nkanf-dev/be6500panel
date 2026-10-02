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
	"net"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strings"
)

var (
	errArtifactCompressedLimit   = errors.New("artifact exceeds compressed byte limit")
	errArtifactUncompressedLimit = errors.New("artifact exceeds uncompressed byte limit")
)

// acquireArtifact returns a new, checksum-verified executable in RunDir. The
// caller owns the returned file and must either activate it or remove it. No
// current executable or persistent metadata is changed here. SHA256 identifies
// the fetched bytes, before gzip decompression.
func acquireArtifact(ctx context.Context, opts Options, service string, artifact Artifact) (path string, err error) {
	if ctx == nil {
		return "", errors.New("artifact context is required")
	}
	if err := ctx.Err(); err != nil {
		return "", err
	}
	if len(artifact.SHA256) != sha256.Size*2 {
		return "", errors.New("artifact SHA256 must contain exactly 64 hexadecimal characters")
	}
	wantDigest, err := hex.DecodeString(artifact.SHA256)
	if err != nil {
		return "", errors.New("artifact SHA256 must contain exactly 64 hexadecimal characters")
	}
	if artifact.Compression != "none" && artifact.Compression != "gzip" {
		return "", errors.New("unsupported artifact compression: use none or gzip")
	}
	if opts.MaxCompressedBytes == 0 {
		opts.MaxCompressedBytes = 16 << 20
	}
	if opts.MaxUncompressedBytes == 0 {
		opts.MaxUncompressedBytes = 40 << 20
	}
	if opts.MaxCompressedBytes < 0 || opts.MaxUncompressedBytes < 0 {
		return "", errors.New("artifact byte limits must be positive")
	}
	if opts.RunDir == "" {
		return "", errors.New("artifact RunDir is required")
	}
	if strings.Contains(artifact.URL, "#") {
		return "", errors.New("artifact URL must not contain a fragment")
	}
	sourceURL, err := url.Parse(artifact.URL)
	if err != nil {
		return "", fmt.Errorf("parse artifact URL: %w", err)
	}
	if err := validateArtifactURL(sourceURL, opts, true); err != nil {
		return "", err
	}
	source, err := openArtifactSource(ctx, opts, sourceURL)
	if err != nil {
		return "", err
	}
	var staged *os.File
	var stagedPath string
	defer func() {
		if closeErr := source.Close(); err == nil && closeErr != nil {
			err = fmt.Errorf("close artifact source: %w", closeErr)
		}
		if staged != nil {
			if closeErr := staged.Close(); err == nil && closeErr != nil {
				err = fmt.Errorf("close artifact staging file: %w", closeErr)
			}
		}
		if err != nil {
			if stagedPath != "" {
				_ = os.Remove(stagedPath)
			}
			path = ""
		}
	}()

	// The manager creates this private directory. Creating it here also permits
	// acquisition into a new RunDir without touching DataDir or the live binary.
	if err := os.MkdirAll(opts.RunDir, 0700); err != nil {
		return "", fmt.Errorf("create artifact staging directory: %w", err)
	}
	staged, err = os.CreateTemp(opts.RunDir, ".artifact-*")
	if err != nil {
		return "", fmt.Errorf("create artifact staging file: %w", err)
	}
	stagedPath = staged.Name()

	digest := sha256.New()
	limited := &artifactReadLimiter{ctx: ctx, reader: source, remaining: opts.MaxCompressedBytes}
	fetched := io.TeeReader(limited, digest)
	var extracted io.Reader = fetched
	if artifact.Compression == "gzip" {
		reader, err := gzip.NewReader(fetched)
		if err != nil {
			return "", fmt.Errorf("open artifact gzip stream: %w", err)
		}
		defer reader.Close()
		// The default multistream mode consumes every gzip member and rejects
		// truncated trailers, bad checksums, and non-gzip trailing bytes.
		extracted = reader
	}
	output := &artifactWriteLimiter{writer: staged, remaining: opts.MaxUncompressedBytes}
	if _, err := io.Copy(output, extracted); err != nil {
		return "", fmt.Errorf("stream artifact: %w", err)
	}
	if err := ctx.Err(); err != nil {
		return "", err
	}
	if !bytes.Equal(digest.Sum(nil), wantDigest) {
		return "", errors.New("artifact SHA256 mismatch")
	}
	if err := staged.Chmod(0700); err != nil {
		return "", fmt.Errorf("set artifact executable permissions: %w", err)
	}
	if err := staged.Sync(); err != nil {
		return "", fmt.Errorf("sync artifact staging file: %w", err)
	}
	if err := ctx.Err(); err != nil {
		return "", err
	}
	return stagedPath, nil
}

func validateArtifactURL(u *url.URL, opts Options, allowFile bool) error {
	if u.User != nil {
		return errors.New("artifact URL credentials are not allowed")
	}
	if u.Fragment != "" || u.RawFragment != "" {
		return errors.New("artifact URL must not contain a fragment")
	}
	if !u.IsAbs() || u.Opaque != "" {
		return errors.New("artifact URL must be absolute and hierarchical")
	}
	switch u.Scheme {
	case "https", "http":
		if u.Host == "" || u.Hostname() == "" {
			return errors.New("artifact URL must have a host")
		}
		if u.Scheme == "http" {
			if !opts.AllowLoopbackHTTP {
				return errors.New("artifact HTTP requires explicit AllowLoopbackHTTP")
			}
			ip := net.ParseIP(u.Hostname())
			if ip == nil || !ip.IsLoopback() {
				return errors.New("artifact HTTP requires a numeric loopback host")
			}
		}
		return nil
	case "file":
		if !allowFile {
			return errors.New("artifact redirect scheme must be HTTPS or allowed loopback HTTP")
		}
		if opts.LocalSourceRoot == "" {
			return errors.New("artifact file URL requires explicit LocalSourceRoot")
		}
		if u.Host != "" || u.RawQuery != "" || u.ForceQuery || !filepath.IsAbs(u.Path) {
			return errors.New("artifact file URL requires an absolute local path without host or query")
		}
		return nil
	default:
		return errors.New("unsupported artifact URL scheme")
	}
}

func openArtifactSource(ctx context.Context, opts Options, u *url.URL) (io.ReadCloser, error) {
	if u.Scheme == "file" {
		return openLocalArtifact(opts, u)
	}
	baseClient := opts.HTTPClient
	if baseClient == nil {
		baseClient = http.DefaultClient
	}
	client := *baseClient // Do not mutate a caller's shared HTTP client.
	previousRedirectPolicy := client.CheckRedirect
	client.CheckRedirect = func(request *http.Request, via []*http.Request) error {
		if err := validateArtifactURL(request.URL, opts, false); err != nil {
			return err
		}
		// url.Parse discards an empty fragment marker. Inspect Location too.
		if request.Response != nil && strings.Contains(request.Response.Header.Get("Location"), "#") {
			return errors.New("artifact redirect must not contain a fragment")
		}
		if len(via) >= 10 {
			return errors.New("artifact stopped after 10 redirects")
		}
		if previousRedirectPolicy != nil {
			if err := previousRedirectPolicy(request, via); err != nil {
				return err
			}
			// A caller policy can edit the next request. Validate its final URL.
			return validateArtifactURL(request.URL, opts, false)
		}
		return nil
	}
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, u.String(), nil)
	if err != nil {
		return nil, fmt.Errorf("create artifact request: %w", err)
	}
	// Disable transparent HTTP decompression. The digest must cover exactly
	// the bytes fetched, not a transport's decoded view of those bytes.
	request.Header.Set("Accept-Encoding", "identity")
	response, err := client.Do(request)
	if err != nil {
		return nil, fmt.Errorf("fetch artifact: %w", err)
	}
	if response.StatusCode != http.StatusOK {
		response.Body.Close()
		return nil, fmt.Errorf("artifact HTTP status %d", response.StatusCode)
	}
	if response.Uncompressed {
		response.Body.Close()
		return nil, errors.New("artifact HTTP transport must not decompress response bytes")
	}
	if response.ContentLength > opts.MaxCompressedBytes {
		response.Body.Close()
		return nil, errArtifactCompressedLimit
	}
	return response.Body, nil
}

func openLocalArtifact(opts Options, u *url.URL) (io.ReadCloser, error) {
	root, err := filepath.Abs(filepath.Clean(opts.LocalSourceRoot))
	if err != nil {
		return nil, fmt.Errorf("resolve artifact LocalSourceRoot: %w", err)
	}
	root, err = filepath.EvalSymlinks(root)
	if err != nil {
		return nil, fmt.Errorf("resolve artifact LocalSourceRoot: %w", err)
	}
	rootInfo, err := os.Stat(root)
	if err != nil || !rootInfo.IsDir() {
		return nil, errors.New("artifact LocalSourceRoot must be a directory")
	}
	path, err := filepath.EvalSymlinks(filepath.Clean(filepath.FromSlash(u.Path)))
	if err != nil {
		return nil, fmt.Errorf("resolve local artifact path: %w", err)
	}
	rel, err := filepath.Rel(root, path)
	if err != nil || rel == ".." || strings.HasPrefix(rel, ".."+string(filepath.Separator)) || filepath.IsAbs(rel) {
		return nil, errors.New("artifact file is outside LocalSourceRoot")
	}
	info, err := os.Stat(path)
	if err != nil {
		return nil, fmt.Errorf("stat local artifact: %w", err)
	}
	if !info.Mode().IsRegular() {
		return nil, errors.New("artifact source must be a regular file")
	}
	if info.Size() > opts.MaxCompressedBytes {
		return nil, errArtifactCompressedLimit
	}
	file, err := os.Open(path)
	if err != nil {
		return nil, fmt.Errorf("open local artifact: %w", err)
	}
	openedInfo, err := file.Stat()
	if err != nil || !os.SameFile(info, openedInfo) {
		file.Close()
		return nil, errors.New("artifact local source changed while opening")
	}
	return file, nil
}

// artifactReadLimiter probes at most one byte beyond the compressed limit. A
// stream exactly at the limit still reaches EOF and verifies successfully.
// Context checks also bound local file work, which has no HTTP request context.
type artifactReadLimiter struct {
	ctx       context.Context
	reader    io.Reader
	remaining int64
}

func (r *artifactReadLimiter) Read(p []byte) (int, error) {
	if err := r.ctx.Err(); err != nil {
		return 0, err
	}
	if len(p) == 0 {
		return 0, nil
	}
	if r.remaining == 0 {
		var probe [1]byte
		n, err := r.reader.Read(probe[:])
		if ctxErr := r.ctx.Err(); ctxErr != nil {
			return 0, ctxErr
		}
		if n != 0 {
			return 0, errArtifactCompressedLimit
		}
		return 0, err
	}
	if int64(len(p)) > r.remaining {
		p = p[:int(r.remaining)]
	}
	n, err := r.reader.Read(p)
	r.remaining -= int64(n)
	if ctxErr := r.ctx.Err(); ctxErr != nil {
		return n, ctxErr
	}
	return n, err
}

type artifactWriteLimiter struct {
	writer    io.Writer
	remaining int64
}

func (w *artifactWriteLimiter) Write(p []byte) (int, error) {
	if int64(len(p)) > w.remaining {
		return 0, errArtifactUncompressedLimit
	}
	n, err := w.writer.Write(p)
	w.remaining -= int64(n)
	return n, err
}

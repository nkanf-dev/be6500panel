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
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
)

func probeRunEntries(t *testing.T, opts Options) []string {
	t.Helper()
	entries, err := os.ReadDir(opts.RunDir)
	if err != nil {
		t.Fatal(err)
	}
	out := make([]string, 0, len(entries))
	for _, entry := range entries {
		out = append(out, entry.Name())
	}
	return out
}

func probeDataSnapshot(t *testing.T, root string) map[string][sha256.Size]byte {
	t.Helper()
	out := make(map[string][sha256.Size]byte)
	err := filepath.WalkDir(root, func(path string, entry os.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if !entry.IsDir() {
			body, err := os.ReadFile(path)
			if err != nil {
				return err
			}
			out[path] = sha256.Sum256(body)
		}
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	return out
}

func TestProbeLeasePreservesLiveRuntimeAndPrivateBytes(t *testing.T) {
	var hooks atomic.Int32
	m, opts := testManager(t, func(o *Options) {
		o.CleanupHook = func(context.Context, string) error { hooks.Add(1); return nil }
		o.ReadyHook = func(context.Context, string) error { hooks.Add(1); return nil }
		o.RestoreHook = func(context.Context, string) error { hooks.Add(1); return nil }
		o.StorageAdmission = func(context.Context, string, int64, bool) (func(), error) {
			hooks.Add(1)
			return func() {}, nil
		}
	})
	acquireFixture(t, m, opts, SingBox, fixture)
	accepted(t, m, SingBox, "good-private-config", 0)
	before, err := m.Start(context.Background(), SingBox)
	if err != nil {
		t.Fatal(err)
	}
	m.mu.Lock()
	service := m.services[SingBox]
	proc, disk, epoch, original := service.proc, service.disk, service.epoch, service.binary
	m.mu.Unlock()
	data := probeDataSnapshot(t, opts.DataDir)
	calls := hooks.Load()
	lease, err := m.AcquireProbeLease(context.Background(), func(context.Context) error {
		status, err := m.Status(SingBox)
		if err != nil || status.PID != before.PID {
			t.Fatal("guard could not read active status", status, err)
		}
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	defer lease.Release()
	if !filepath.IsAbs(lease.Path()) || lease.Path() == original || filepath.Dir(filepath.Dir(lease.Path())) != m.opts.RunDir {
		t.Fatal("lease is not a separate private runtime copy")
	}
	body, err := os.ReadFile(lease.Path())
	if err != nil || string(body) != fixture {
		t.Fatal("leased bytes changed", err)
	}
	for _, path := range []string{lease.Path(), filepath.Dir(lease.Path())} {
		info, err := os.Lstat(path)
		if err != nil || info.Mode().Perm() != 0700 {
			t.Fatal("lease permissions are not private", err)
		}
	}
	for _, text := range []string{fmt.Sprint(lease), fmt.Sprintf("%+v", lease), fmt.Sprintf("%#v", lease)} {
		if strings.Contains(text, opts.RunDir) || strings.Contains(text, lease.Path()) || strings.Contains(text, "good-private") {
			t.Fatal("lease formatting exposed private material")
		}
	}
	after, _ := m.Status(SingBox)
	if before.PID != after.PID || before.State != after.State || before.Generation != after.Generation || before.Desired != after.Desired {
		t.Fatalf("lease changed runtime status: %+v -> %+v", before, after)
	}
	m.mu.Lock()
	unchanged := service.proc == proc && reflect.DeepEqual(service.disk, disk) && service.epoch == epoch && service.binary == original
	m.mu.Unlock()
	if !unchanged || hooks.Load() != calls || !reflect.DeepEqual(data, probeDataSnapshot(t, opts.DataDir)) {
		t.Fatal("lease changed process, private state, flash, or resource hooks")
	}
	// The probe job does not hold the shared lane after the bounded copy.
	_, done, err := m.begin(context.Background(), FRPC)
	if err != nil {
		t.Fatal("lease kept the mutation lane", err)
	}
	done()
}

func TestProbeLeaseGzipUsesVerifiedUncompressedProvenance(t *testing.T) {
	m, opts := testManager(t, nil)
	body := []byte(fixture)
	var compressed bytes.Buffer
	gz := gzip.NewWriter(&compressed)
	if _, err := gz.Write(body); err != nil {
		t.Fatal(err)
	}
	if err := gz.Close(); err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(opts.LocalSourceRoot, "fixture.gz")
	if err := os.WriteFile(path, compressed.Bytes(), 0600); err != nil {
		t.Fatal(err)
	}
	compressedSHA := sha256.Sum256(compressed.Bytes())
	binarySHA := sha256.Sum256(body)
	artifact := Artifact{URL: "file://" + path, SHA256: hex.EncodeToString(compressedSHA[:]), Compression: "gzip"}
	if _, err := m.Acquire(context.Background(), SingBox, artifact); err != nil {
		t.Fatal(err)
	}
	m.mu.Lock()
	recorded := m.services[SingBox].binarySHA256
	m.mu.Unlock()
	if recorded != binarySHA || recorded == compressedSHA {
		t.Fatal("uncompressed checksum provenance was not retained")
	}
	lease, err := m.AcquireProbeLease(context.Background(), nil)
	if err != nil {
		t.Fatal(err)
	}
	defer lease.Release()
	leased, err := os.ReadFile(lease.Path())
	if err != nil || !bytes.Equal(leased, body) {
		t.Fatal("gzip lease has wrong bytes", err)
	}
	artifact.SHA256 = strings.Repeat("0", 64)
	if _, err := m.Acquire(context.Background(), SingBox, artifact); err == nil {
		t.Fatal("unverified compressed download was accepted")
	}
	m.mu.Lock()
	unchanged := m.services[SingBox].binarySHA256 == binarySHA
	m.mu.Unlock()
	if !unchanged {
		t.Fatal("failed download changed original provenance")
	}
}

func TestProbeLeaseRejectsUntrustedAndUnsafeOriginals(t *testing.T) {
	cases := []struct {
		name string
		edit func(*testing.T, *Manager, Options, string)
		want error
	}{
		{"missing-provenance", func(t *testing.T, m *Manager, _ Options, _ string) {
			m.mu.Lock()
			m.services[SingBox].binarySHA256 = [sha256.Size]byte{}
			m.mu.Unlock()
		}, ErrNoArtifact},
		{"mutated", func(t *testing.T, _ *Manager, _ Options, path string) {
			body := []byte(fixture)
			body[len(body)-2] ^= 1
			if err := os.WriteFile(path, body, 0700); err != nil {
				t.Fatal(err)
			}
		}, ErrProbeLease},
		{"symlink", func(t *testing.T, _ *Manager, opts Options, path string) {
			source := filepath.Join(opts.LocalSourceRoot, "symlink-target")
			if err := os.WriteFile(source, []byte(fixture), 0700); err != nil {
				t.Fatal(err)
			}
			if err := os.Remove(path); err != nil {
				t.Fatal(err)
			}
			if err := os.Symlink(source, path); err != nil {
				t.Fatal(err)
			}
		}, ErrProbeLease},
		{"nonregular", func(t *testing.T, _ *Manager, _ Options, path string) {
			if err := os.Remove(path); err != nil {
				t.Fatal(err)
			}
			if err := os.Mkdir(path, 0700); err != nil {
				t.Fatal(err)
			}
		}, ErrProbeLease},
		{"not-executable", func(t *testing.T, _ *Manager, _ Options, path string) {
			if err := os.Chmod(path, 0600); err != nil {
				t.Fatal(err)
			}
		}, ErrProbeLease},
		{"oversized", func(t *testing.T, m *Manager, _ Options, path string) {
			if err := os.Truncate(path, m.opts.MaxUncompressedBytes+1); err != nil {
				t.Fatal(err)
			}
		}, ErrArtifactUncompressedLimit},
		{"path-escape", func(t *testing.T, m *Manager, opts Options, _ string) {
			path := filepath.Join(opts.LocalSourceRoot, "outside")
			if err := os.WriteFile(path, []byte(fixture), 0700); err != nil {
				t.Fatal(err)
			}
			m.mu.Lock()
			m.services[SingBox].binary = path
			m.mu.Unlock()
		}, ErrProbeLease},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			var hooks atomic.Int32
			m, opts := testManager(t, func(o *Options) {
				o.MaxUncompressedBytes = int64(len(fixture)) + 1
				o.CleanupHook = func(context.Context, string) error { hooks.Add(1); return nil }
				o.ReadyHook = func(context.Context, string) error { hooks.Add(1); return nil }
				o.RestoreHook = func(context.Context, string) error { hooks.Add(1); return nil }
			})
			acquireFixture(t, m, opts, SingBox, fixture)
			m.mu.Lock()
			original := m.services[SingBox].binary
			m.mu.Unlock()
			tc.edit(t, m, opts, original)
			entries := probeRunEntries(t, opts)
			data := probeDataSnapshot(t, opts.DataDir)
			before, _ := m.Status(SingBox)
			lease, err := m.AcquireProbeLease(context.Background(), nil)
			if !errors.Is(err, tc.want) || lease.Path() != "" {
				lease.Release()
				t.Fatalf("unsafe original accepted: %v", err)
			}
			if strings.Contains(err.Error(), opts.RunDir) || strings.Contains(err.Error(), opts.LocalSourceRoot) {
				t.Fatal("private path leaked in public lease error")
			}
			after, _ := m.Status(SingBox)
			if !reflect.DeepEqual(entries, probeRunEntries(t, opts)) || !reflect.DeepEqual(data, probeDataSnapshot(t, opts.DataDir)) || !reflect.DeepEqual(before, after) || hooks.Load() != 0 {
				t.Fatal("rejected lease wrote files/state or ran hooks")
			}
		})
	}
}

func TestProbeLeaseAdmissionRejectsWithoutWork(t *testing.T) {
	m, opts := testManager(t, nil)
	acquireFixture(t, m, opts, SingBox, fixture)
	entries := probeRunEntries(t, opts)
	guardErr := errors.New("fixed caller guard refusal")
	var guards atomic.Int32
	guard := func(context.Context) error { guards.Add(1); return guardErr }
	if _, err := m.AcquireProbeLease(context.Background(), guard); !errors.Is(err, guardErr) {
		t.Fatal("guard refusal ignored", err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := m.AcquireProbeLease(ctx, guard); !errors.Is(err, context.Canceled) || guards.Load() != 1 {
		t.Fatal("pre-canceled lease ran guard", err)
	}
	ctx, cancel = context.WithCancel(context.Background())
	if _, err := m.AcquireProbeLease(ctx, func(context.Context) error { cancel(); return nil }); !errors.Is(err, context.Canceled) {
		t.Fatal("successful guard cancellation ignored", err)
	}
	_, done, err := m.begin(context.Background(), FRPC)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := m.AcquireProbeLease(context.Background(), guard); !errors.Is(err, ErrBusy) || guards.Load() != 1 {
		t.Fatal("busy lane ran guard", err)
	}
	done()
	if !reflect.DeepEqual(entries, probeRunEntries(t, opts)) {
		t.Fatal("admission refusal created lease files")
	}
	// No sing-box provenance may be inferred from an acquired frpc binary.
	other, otherOpts := testManager(t, nil)
	acquireFixture(t, other, otherOpts, FRPC, fixture)
	if _, err := other.AcquireProbeLease(context.Background(), nil); !errors.Is(err, ErrNoArtifact) {
		t.Fatal("lease was not fixed to sing-box", err)
	}
}

func TestProbeLeaseFrozenAcrossReplacementAndReleaseCap(t *testing.T) {
	m, opts := testManager(t, nil)
	acquireFixture(t, m, opts, SingBox, fixture)
	m.mu.Lock()
	original := m.services[SingBox].binary
	m.mu.Unlock()
	lease, err := m.AcquireProbeLease(context.Background(), nil)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(lease.Release)
	copyOfLease := lease
	entries := probeRunEntries(t, opts)
	if _, err := m.AcquireProbeLease(context.Background(), nil); !errors.Is(err, ErrBusy) {
		t.Fatal("second lease exceeded RAM cap", err)
	}
	if !reflect.DeepEqual(entries, probeRunEntries(t, opts)) {
		t.Fatal("capped lease created files")
	}
	newBody := fixture + "\n# replacement\n"
	acquireFixture(t, m, opts, SingBox, newBody)
	if _, err := os.Lstat(original); !os.IsNotExist(err) {
		t.Fatal("old active artifact was retained", err)
	}
	leased, err := os.ReadFile(lease.Path())
	if err != nil || string(leased) != fixture {
		t.Fatal("artifact replacement changed frozen lease", err)
	}
	m.mu.Lock()
	current := m.services[SingBox].binary
	m.mu.Unlock()
	sentinel := filepath.Join(opts.RunDir, "unrelated-private-file")
	if err := os.WriteFile(sentinel, []byte("keep"), 0600); err != nil {
		t.Fatal(err)
	}
	var wg sync.WaitGroup
	for i := 0; i < 20; i++ {
		wg.Add(1)
		go func() { defer wg.Done(); copyOfLease.Release() }()
	}
	wg.Wait()
	for _, path := range []string{lease.Path(), filepath.Dir(lease.Path())} {
		if _, err := os.Lstat(path); !os.IsNotExist(err) {
			t.Fatal("release did not remove own lease", err)
		}
	}
	for _, path := range []string{current, sentinel} {
		if _, err := os.Lstat(path); err != nil {
			t.Fatal("release removed another owner's file", err)
		}
	}
	next, err := m.AcquireProbeLease(context.Background(), nil)
	if err != nil {
		t.Fatal("release did not free lease cap", err)
	}
	defer next.Release()
	copyOfLease.Release() // must not release or remove the new owner's lease
	leased, err = os.ReadFile(next.Path())
	if err != nil || string(leased) != newBody {
		t.Fatal("new lease was not stable", err)
	}
	if _, err := m.AcquireProbeLease(context.Background(), nil); !errors.Is(err, ErrBusy) {
		t.Fatal("old Release cleared new lease cap", err)
	}
}

type cancelProbeReader struct {
	cancel context.CancelFunc
	reader io.Reader
}

func (r *cancelProbeReader) Read(p []byte) (int, error) {
	n, err := r.reader.Read(p)
	r.cancel()
	return n, err
}

func TestProbeCopyCancellationAndLimitDuringStreaming(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	var output bytes.Buffer
	_, err := copyProbeBytes(ctx, &output, &cancelProbeReader{cancel: cancel, reader: strings.NewReader(fixture)}, int64(len(fixture)))
	if !errors.Is(err, context.Canceled) || output.Len() != 0 {
		t.Fatal("copy ignored cancellation after read", err)
	}
	output.Reset()
	if _, err := copyProbeBytes(context.Background(), &output, strings.NewReader(fixture), int64(len(fixture)-1)); !errors.Is(err, ErrArtifactUncompressedLimit) {
		t.Fatal("stream grew past bounded byte limit", err)
	}
	output.Reset()
	if n, err := copyProbeBytes(context.Background(), &output, strings.NewReader(fixture), int64(len(fixture))); err != nil || n != int64(len(fixture)) || output.String() != fixture {
		t.Fatal("exact-limit copy rejected", n, err)
	}
}

func TestProbeLeaseRecoveryRestoresDigestAndCloseClearsProvenance(t *testing.T) {
	var m *Manager
	m, opts := testManager(t, func(o *Options) {
		o.ReadyHook = func(_ context.Context, id string) error {
			m.mu.Lock()
			binary := m.services[id].binary
			m.mu.Unlock()
			body, err := os.ReadFile(binary)
			if err != nil {
				return err
			}
			if strings.Contains(string(body), "# failed-upgrade") {
				return ErrReadiness
			}
			return nil
		}
	})
	acquireFixture(t, m, opts, SingBox, fixture)
	accepted(t, m, SingBox, "ready-private", 0)
	if _, err := m.Start(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(opts.LocalSourceRoot, "failed-upgrade")
	body := []byte(fixture + "\n# failed-upgrade\n")
	if err := os.WriteFile(path, body, 0700); err != nil {
		t.Fatal(err)
	}
	digest := sha256.Sum256(body)
	if _, err := m.Acquire(context.Background(), SingBox, Artifact{URL: "file://" + path, SHA256: hex.EncodeToString(digest[:]), Compression: "none"}); !errors.Is(err, ErrReadiness) || errors.Is(err, ErrRecovery) {
		t.Fatal("fixture recovery failed", err)
	}
	lease, err := m.AcquireProbeLease(context.Background(), nil)
	if err != nil {
		t.Fatal("rollback lost original digest", err)
	}
	defer lease.Release()
	leased, err := os.ReadFile(lease.Path())
	if err != nil || string(leased) != fixture {
		t.Fatal("rollback lease has candidate bytes", err)
	}
	if err := m.Close(); err != nil {
		t.Fatal(err)
	}
	m.mu.Lock()
	cleared := m.services[SingBox].binary == "" && m.services[SingBox].binarySHA256 == [sha256.Size]byte{}
	m.mu.Unlock()
	if !cleared {
		t.Fatal("closed binary retained provenance")
	}
	// Caller owns release; runtime Close must not delete a still-owned copy.
	if _, err := os.Lstat(lease.Path()); err != nil {
		t.Fatal("runtime closed caller-owned lease", err)
	}
	if _, err := m.AcquireProbeLease(context.Background(), nil); !errors.Is(err, ErrClosed) {
		t.Fatal(err)
	}
	reopened, err := New(opts)
	if err != nil {
		t.Fatal(err)
	}
	defer reopened.Close()
	if _, err := reopened.AcquireProbeLease(context.Background(), nil); !errors.Is(err, ErrNoArtifact) {
		t.Fatal("boot trusted leftover runtime bytes", err)
	}
}

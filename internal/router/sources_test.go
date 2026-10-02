package router

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestSourceBoundsAndRootIsolation(t *testing.T) {
	root := t.TempDir()
	a := New(root)
	fixtureFile(t, root, "/bounded", "12345")
	if _, err := a.readFile("/bounded", 4); !errors.Is(err, errTooLarge) {
		t.Fatal("unbounded file", err)
	}
	data, err := a.readFile("/bounded", 5)
	if err != nil || string(data) != "12345" {
		t.Fatalf("data=%q err=%v", data, err)
	}
	fixtureFile(t, root, "/target", "safe")
	if err := os.Symlink("target", filepath.Join(root, "inside")); err != nil {
		t.Fatal(err)
	}
	if data, err = a.readFile("/inside", 5); err != nil || string(data) != "safe" {
		t.Fatalf("in-root symlink %q %v", data, err)
	}
	outside := filepath.Join(t.TempDir(), "private")
	if err := os.WriteFile(outside, []byte("HOST-MUST-NOT-BE-READ"), 0600); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(outside, filepath.Join(root, "outside")); err != nil {
		t.Fatal(err)
	}
	if _, err = a.readFile("/outside", fileLimit); !errors.Is(err, errOutsideRoot) {
		t.Fatalf("root escape: %v", err)
	}
	if _, err = a.firewallSource(context.Background(), false); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("fixture attempted host firewall: %v", err)
	}
	if _, err = a.readFile("/", fileLimit); err == nil {
		t.Fatal("accepted directory as file")
	}
}

func TestBoundedCommandOutput(t *testing.T) {
	out := &boundedOutput{limit: 5}
	if n, err := out.Write([]byte("123")); n != 3 || err != nil {
		t.Fatalf("%d %v", n, err)
	}
	if n, err := out.Write([]byte("456")); n != 2 || !errors.Is(err, errTooLarge) || !out.exceeded || string(out.data) != "12345" {
		t.Fatalf("%d %v %+v", n, err, out)
	}
	if n, err := out.Write([]byte("7")); n != 0 || !errors.Is(err, errTooLarge) || len(out.data) != 5 {
		t.Fatalf("%d %v %+v", n, err, out)
	}
}

func TestFixtureAliasNeverEnablesLiveCommands(t *testing.T) {
	root := t.TempDir()
	alias := filepath.Join(root, "root-alias")
	if err := os.Symlink("/", alias); err != nil {
		t.Fatal(err)
	}
	if New(alias).live {
		t.Fatal("a fixture root alias enabled live commands")
	}
	if !New("").live || !New("/").live {
		t.Fatal("explicit live roots did not enable observations")
	}
}

// This helper is the project's own test executable, not a downloaded binary or
// shell. Test argv are fixed and no router command is invoked.
func TestReadCommandHelper(t *testing.T) {
	if len(os.Args) < 2 {
		return
	}
	mode := os.Args[len(os.Args)-1]
	if !strings.HasPrefix(mode, "router-synthetic-") {
		return
	}
	_, _ = os.Stderr.WriteString("SYNTHETIC-STDERR-NOT-RETURNED")
	switch mode {
	case "router-synthetic-success":
		_, _ = os.Stdout.WriteString("synthetic-output")
		os.Exit(0)
	case "router-synthetic-large":
		_, _ = os.Stdout.WriteString(strings.Repeat("x", commandLimit+1))
		os.Exit(0)
	case "router-synthetic-timeout":
		<-time.After(3 * time.Second)
		os.Exit(0)
	case "router-synthetic-exit":
		os.Exit(1)
	}
}

func TestReadCommandTimeoutBoundsAndSafeErrors(t *testing.T) {
	// The race runtime's default one-second exit sleep exceeds the adapter's
	// production timeout. Disable only that delay in synthetic child tests.
	t.Setenv("GORACE", os.Getenv("GORACE")+" atexit_sleep_ms=0")
	executable, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	args := []string{"-test.run=^TestReadCommandHelper$", "--", "router-synthetic-success"}
	data, err := readCommand(context.Background(), executable, args...)
	if err != nil || string(data) != "synthetic-output" {
		t.Fatalf("output=%q err=%v", data, err)
	}
	args[2] = "router-synthetic-large"
	if data, err = readCommand(context.Background(), executable, args...); !errors.Is(err, errTooLarge) || data != nil {
		t.Fatalf("size bound: len=%d err=%v", len(data), err)
	}
	args[2] = "router-synthetic-timeout"
	start := time.Now()
	if _, err = readCommand(context.Background(), executable, args...); !errors.Is(err, context.DeadlineExceeded) || time.Since(start) > 2*time.Second {
		t.Fatalf("timeout: elapsed=%v err=%v", time.Since(start), err)
	}
	args[2] = "router-synthetic-exit"
	if data, err = readCommand(context.Background(), executable, args...); err == nil || data != nil || sourceCode(err) != "read_failed" {
		t.Fatalf("failed command: %q %v", data, err)
	}
	canceled, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err = readCommand(canceled, executable, args...); !errors.Is(err, context.Canceled) {
		t.Fatalf("canceled: %v", err)
	}
}

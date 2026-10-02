package modules

import (
	"context"
	"encoding/json"
	"math"
	"net"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"

	"be6500panel/internal/core"
)

func TestProcParsing(t *testing.T) {
	memory, err := parseMemory("MemTotal: 1024 kB\nMemAvailable: 512 kB\nSwapTotal: 0 kB\n")
	if err != nil || memory.TotalBytes != 1024*1024 || memory.AvailableBytes != 512*1024 {
		t.Fatal(memory, err)
	}
	for _, text := range []string{"", "MemTotal: 1024 kB", "MemTotal: 1 MB\nMemAvailable: 0 kB", "MemTotal: 1 kB\nMemAvailable: 2 kB", "MemTotal: 18446744073709551615 kB\nMemAvailable: 0 kB", "MemTotal: -1 kB\nMemAvailable: 0 kB"} {
		if _, err := parseMemory(text); err == nil {
			t.Fatal(text)
		}
	}
	if v, err := parseLoad("0.1 0.2 0.3 1/30 123"); err != nil || v[2] != .3 {
		t.Fatal(v, err)
	}
	for _, text := range []string{"", "0 0", "NaN 0 0", "Inf 0 0", "-1 0 0"} {
		if _, err := parseLoad(text); err == nil {
			t.Fatal(text)
		}
	}
}
func TestSystemHonest(t *testing.T) {
	observer := NewSystem(true)
	first, err := observer.Observe(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	second, _ := observer.Observe(context.Background())
	if first.Mode != "demo" || first.Hostname != "be6500panel-demo" || first.Memory != second.Memory || first.Load != second.Load {
		t.Fatal(first, second)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := observer.Observe(ctx); err == nil {
		t.Fatal("context ignored")
	}
	host := NewSystem(false)
	status, err := host.Observe(context.Background())
	if runtime.GOOS == "linux" {
		if err != nil {
			t.Fatal(err)
		}
		if status.Mode != "host" || status.Memory.TotalBytes == 0 || status.CPUCount < 1 || status.SampledAt.IsZero() || math.IsNaN(status.UptimeSeconds) {
			t.Fatal(status)
		}
	} else {
		if err == nil {
			t.Fatal("fabricated host metrics")
		}
	}
}
func TestNetworkAlwaysHost(t *testing.T) {
	got, err := (Network{}).Observe(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	actual, err := net.Interfaces()
	if err != nil {
		t.Fatal(err)
	}
	if len(got.Interfaces) != len(actual) || got.Routes == nil || got.RouteObservationSupported {
		t.Fatal(got)
	}
	for _, iface := range actual {
		found := false
		for _, item := range got.Interfaces {
			if item.Name == iface.Name {
				found = true
				if item.MTU != iface.MTU || item.Addresses == nil {
					t.Fatal(item)
				}
			}
		}
		if !found {
			t.Fatal(iface.Name)
		}
	}
}
func TestFRPCEdgeCases(t *testing.T) {
	const body = `{"serverAddress":"2001:db8::1","serverPort":7000,"tls":true,"transport":"quic","proxies":[{"name":"web","type":"https","localAddress":"localhost","localPort":443,"domains":["test.example.invalid"]}]}`
	var input FRPCInput
	if err := json.Unmarshal([]byte(body), &input); err != nil {
		t.Fatal(err)
	}
	if _, err := (FRPC{}).Plan(input, core.NewCoordinator()); err != nil {
		t.Fatal(err)
	}
	bad := input
	bad.Proxies = append(append([]FRPCProxy{}, input.Proxies...), input.Proxies...)
	if _, err := (FRPC{}).Plan(bad, core.NewCoordinator()); err == nil {
		t.Fatal("duplicate names")
	}
	bad = input
	bad.Proxies = make([]FRPCProxy, 65)
	if _, err := (FRPC{}).Plan(bad, core.NewCoordinator()); err == nil {
		t.Fatal("too many")
	}
	for _, host := range []string{"", " user.invalid", "user@host.invalid", "https://example.invalid", "host.invalid:123", "host/route", "-host.invalid", "host..invalid", strings.Repeat("a", 64) + ".invalid", "例子.invalid"} {
		if validHost(host) {
			t.Fatal(host)
		}
	}
	for _, host := range []string{"127.0.0.1", "::1", "relay.example.invalid", "localhost", "relay.example.invalid."} {
		if !validHost(host) {
			t.Fatal(host)
		}
	}
}
func TestProxyEnums(t *testing.T) {
	input := ProxyInput{Mode: "direct", DNSStrategy: "direct", IPv6Policy: "direct", FailurePolicy: "block-proxy", NodeCount: 0}
	if _, err := (Proxy{}).Plan(input, core.NewCoordinator()); err != nil {
		t.Fatal(err)
	}
	input.DNSStrategy = "invalid"
	if _, err := (Proxy{}).Plan(input, core.NewCoordinator()); err == nil {
		t.Fatal("bad DNS")
	}
	input.DNSStrategy = "split"
	input.IPv6Policy = "invalid"
	if _, err := (Proxy{}).Plan(input, core.NewCoordinator()); err == nil {
		t.Fatal("bad IPv6")
	}
	input.IPv6Policy = "follow"
	input.FailurePolicy = "invalid"
	if _, err := (Proxy{}).Plan(input, core.NewCoordinator()); err == nil {
		t.Fatal("bad failure")
	}
}

func TestSyntheticProcAdapter(t *testing.T) {
	root := t.TempDir()
	if err := os.MkdirAll(filepath.Join(root, "sys/kernel"), 0700); err != nil {
		t.Fatal(err)
	}
	fixture := map[string]string{"sys/kernel/osrelease": "synthetic-test-kernel\n", "uptime": "120.25 500.00\n", "loadavg": "0.1 0.2 0.3 1/30 200\n", "meminfo": "MemTotal: 1024 kB\nMemAvailable: 512 kB\n"}
	for name, value := range fixture {
		if err := os.WriteFile(filepath.Join(root, name), []byte(value), 0600); err != nil {
			t.Fatal(err)
		}
	}
	system := &System{procRoot: root}
	now := time.Now().UTC()
	status, err := system.observeProc(context.Background(), now)
	if err != nil || status.Kernel != "synthetic-test-kernel" || status.UptimeSeconds != 120.25 || status.SampledAt != now || status.Memory.AvailableBytes != 512*1024 || status.Mode != "host" {
		t.Fatal(status, err)
	}
	if err := os.WriteFile(filepath.Join(root, "uptime"), []byte("NaN"), 0600); err != nil {
		t.Fatal(err)
	}
	if _, err := system.observeProc(context.Background(), now); err == nil {
		t.Fatal("invalid uptime fabricated")
	}
	if err := os.Remove(filepath.Join(root, "meminfo")); err != nil {
		t.Fatal(err)
	}
	if _, err := readProc(root, "meminfo"); err == nil {
		t.Fatal("missing proc accepted")
	}
	if err := os.WriteFile(filepath.Join(root, "oversize"), []byte(strings.Repeat("x", 65537)), 0600); err != nil {
		t.Fatal(err)
	}
	if _, err := readProc(root, "oversize"); err == nil {
		t.Fatal("unbounded proc read")
	}
}

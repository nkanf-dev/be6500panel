package main

import (
	"context"
	"encoding/binary"
	"encoding/json"
	"errors"
	"io"
	"net"
	"reflect"
	"strings"
	"testing"
	"time"

	managedruntime "be6500panel/internal/runtime"
)

func TestFRPCReadinessDoesNotInventConnection(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	if err := runtimeReadiness(func() *managedruntime.Manager { return nil })(ctx, managedruntime.FRPC); err != nil {
		t.Fatal(err)
	}
}

func TestNativeReadinessTargets(t *testing.T) {
	tests := []struct {
		name string
		raw  string
		want []readinessTarget
		fail bool
	}{
		{"mixed-only", `{"inbounds":[{"type":"mixed","listen":"127.0.0.1","listen_port":2080}]}`, []readinessTarget{{network: "tcp", address: "127.0.0.1:2080"}}, false},
		{"mixed-with-outbound-dns", `{"inbounds":[{"type":"mixed","listen_port":2080}],"dns":{"servers":[{"tag":"dns-direct","server":"223.5.5.5"}]}}`, []readinessTarget{{network: "tcp", address: "127.0.0.1:2080"}}, false},
		{"dns-both", `{"inbounds":[{"type":"direct","tag":"dns-in","listen":"0.0.0.0","listen_port":1053}],"route":{"rules":[{"inbound":["dns-in"],"action":"hijack-dns"}]},"dns":{"rules":[{"domain":["endpoint.test"],"server":"dns-direct"}]}}`, []readinessTarget{{network: "udp", address: "127.0.0.1:1053", domain: "endpoint.test"}, {network: "tcp", address: "127.0.0.1:1053", domain: "endpoint.test"}}, false},
		{"dns-udp-only", `{"inbounds":[{"type":"direct","tag":"resolver","listen":"::","listen_port":1053,"network":"udp"}],"route":{"rules":[{"inbound":"resolver","action":"hijack-dns"}]}}`, []readinessTarget{{network: "udp", address: "[::1]:1053", domain: "dns.alidns.com"}}, false},
		{"dns-tcp-only", `{"inbounds":[{"type":"direct","tag":"dns-in","listen_port":1053,"network":"tcp"}],"route":{"rules":[{"inbound":["dns-in"],"action":"hijack-dns"}]}}`, []readinessTarget{{network: "tcp", address: "127.0.0.1:1053", domain: "dns.alidns.com"}}, false},
		{"dns-port-rule", `{"inbounds":[{"type":"direct","listen_port":53}],"route":{"rules":[{"port":[53],"action":"hijack-dns"}]}}`, []readinessTarget{{network: "udp", address: "127.0.0.1:53", domain: "dns.alidns.com"}, {network: "tcp", address: "127.0.0.1:53", domain: "dns.alidns.com"}}, false},
		{"ordinary-direct", `{"inbounds":[{"type":"direct","tag":"echo","listen_port":1053}]}`, []readinessTarget{{network: "tcp", address: "127.0.0.1:1053"}}, false},
		{"dns-missing-route", `{"inbounds":[{"type":"mixed","listen_port":2080},{"type":"direct","tag":"dns-in","listen_port":1053}]}`, nil, true},
		{"dns-port-missing-route", `{"inbounds":[{"type":"direct","listen_port":53}]}`, nil, true},
		{"wrong-hijack-inbound", `{"inbounds":[{"type":"direct","tag":"dns-in","listen_port":1053}],"route":{"rules":[{"inbound":["another"],"action":"hijack-dns"}]}}`, nil, true},
		{"invalid-address", `{"inbounds":[{"type":"mixed","listen":"not-an-ip","listen_port":2080}]}`, nil, true},
		{"zero-port", `{"inbounds":[{"type":"mixed","listen":"127.0.0.1"}]}`, nil, true},
		{"invalid-network", `{"inbounds":[{"type":"direct","tag":"dns-in","listen_port":1053,"network":"sctp"}],"route":{"rules":[{"inbound":["dns-in"],"action":"hijack-dns"}]}}`, nil, true},
		{"no-listener", `{"inbounds":[]}`, nil, true},
		{"invalid-json", `{`, nil, true},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			got, err := nativeReadinessTargets([]byte(test.raw))
			if (err != nil) != test.fail {
				t.Fatalf("targets error = %v, want failure %v", err, test.fail)
			}
			if !test.fail && !reflect.DeepEqual(got, test.want) {
				t.Fatalf("targets = %#v, want %#v", got, test.want)
			}
		})
	}
}

func TestDNSReadinessDomainSelection(t *testing.T) {
	tests := []struct{ name, dns, want string }{
		{"direct-bootstrap", `{"rules":[{"domain":["proxy.test"],"server":"dns-proxy"},{"domain":["node.test"],"server":"dns-direct"}]}`, "node.test"},
		{"direct-identity-preferred", `{"servers":[{"tag":"dns-direct","server":"223.5.5.5","tls":{"server_name":"dns.alidns.com"}}],"rules":[{"domain":["a-node.test","dns.alidns.com"],"server":"dns-direct"}]}`, "dns.alidns.com"},
		{"direct-identity-fallback", `{"servers":[{"tag":"dns-direct","server":"223.5.5.5","tls":{"server_name":"resolver.test"}}]}`, "resolver.test"},
		{"direct-host-fallback", `{"servers":[{"tag":"dns-direct","server":"resolver.test"}]}`, "resolver.test"},
		{"normalization", `{"rules":[{"domain":"Node.TEST.","server":"dns-direct"}]}`, "node.test"},
		{"invalid-domain-skipped", `{"rules":[{"domain":["192.0.2.1","-invalid.test","good.test"],"server":"dns-direct"}]}`, "good.test"},
		{"standard-bootstrap", `{}`, "dns.alidns.com"},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			raw := `{"inbounds":[{"type":"direct","tag":"dns-in","listen_port":1053}],"route":{"rules":[{"inbound":["dns-in"],"action":"hijack-dns"}]},"dns":` + test.dns + `}`
			targets, err := nativeReadinessTargets([]byte(raw))
			if err != nil {
				t.Fatal(err)
			}
			for _, target := range targets {
				if target.domain != test.want {
					t.Fatalf("domain = %q, want %q", target.domain, test.want)
				}
			}
		})
	}
}

func TestDNSReadinessWireValidation(t *testing.T) {
	query, err := dnsReadinessQuery("bootstrap.test", 0x1234)
	if err != nil {
		t.Fatal(err)
	}
	valid := dnsTestAnswer(query)
	tests := []struct {
		name   string
		change func([]byte) []byte
	}{
		{"short-header", func(b []byte) []byte { return b[:11] }},
		{"transaction-mismatch", func(b []byte) []byte { b[1]++; return b }},
		{"not-response", func(b []byte) []byte { b[2] &= 0x7f; return b }},
		{"wrong-opcode", func(b []byte) []byte { b[2] |= 0x08; return b }},
		{"truncated", func(b []byte) []byte { b[2] |= 0x02; return b }},
		{"servfail", func(b []byte) []byte { b[3] |= 2; return b }},
		{"nxdomain", func(b []byte) []byte { b[3] |= 3; return b }},
		{"no-question", func(b []byte) []byte { b[5] = 0; return b }},
		{"wrong-question", func(b []byte) []byte { b[13] = 'z'; return b }},
		{"wrong-question-type", func(b []byte) []byte { b[len(query)-3] = 28; return b }},
		{"no-answer", func(b []byte) []byte { b[7] = 0; return b[:len(query)] }},
		{"only-additional-answer", func(b []byte) []byte { b[7] = 0; b[11] = 1; return b }},
		{"short-answer", func(b []byte) []byte { return b[:len(b)-1] }},
		{"bad-a-length", func(b []byte) []byte { b[len(query)+11] = 3; return b[:len(b)-1] }},
		{"wrong-answer-type", func(b []byte) []byte { b[len(query)+3] = 28; return b }},
		{"wrong-answer-class", func(b []byte) []byte { b[len(query)+5] = 3; return b }},
		{"unrelated-answer", func(b []byte) []byte { b[len(query)+1] = 22; return b }},
		{"compression-cycle", func(b []byte) []byte { b[len(query)+1] = byte(len(query)); return b }},
		{"compression-outside", func(b []byte) []byte { b[len(query)] = 0xff; b[len(query)+1] = 0xff; return b }},
		{"extra-garbage", func(b []byte) []byte { return append(b, 0) }},
	}
	if err = validateDNSReadinessResponse(valid, query); err != nil {
		t.Fatalf("valid response: %v", err)
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			if err := validateDNSReadinessResponse(test.change(append([]byte(nil), valid...)), query); err == nil {
				t.Fatal("invalid response passed readiness")
			}
		})
	}
	aliasQuery, _ := dnsReadinessQuery("alias.test", 0x1234)
	aliasAnswer := append([]byte(nil), valid[:len(query)]...)
	aliasAnswer[7] = 2
	aliasAnswer = append(aliasAnswer, 0xc0, 0x0c, 0, 5, 0, 1, 0, 0, 0, 60, 0, byte(len(aliasQuery)-16))
	aliasAnswer = append(aliasAnswer, aliasQuery[12:len(aliasQuery)-4]...)
	aliasOffset := len(aliasAnswer)
	aliasAnswer = append(aliasAnswer, aliasQuery[12:len(aliasQuery)-4]...)
	aliasAnswer = append(aliasAnswer, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 192, 0, 2, 1)
	if err = validateDNSReadinessResponse(aliasAnswer, query); err != nil {
		t.Fatalf("valid CNAME+A response: %v", err)
	}
	cnameOnly := append([]byte(nil), aliasAnswer[:aliasOffset]...)
	cnameOnly[7] = 1
	if err = validateDNSReadinessResponse(cnameOnly, query); err == nil {
		t.Fatal("CNAME without resolved A passed")
	}
}

func TestDNSReadinessQueryBounds(t *testing.T) {
	for _, domain := range []string{"", "192.0.2.1", "bad..test", "-bad.test", strings.Repeat("x", 64) + ".test", strings.Repeat("x.", 128)} {
		if _, err := dnsReadinessQuery(domain, 1); err == nil {
			t.Fatalf("invalid query domain %q accepted", domain)
		}
	}
}

func TestDNSReadinessProtocols(t *testing.T) {
	for _, network := range []string{"udp", "tcp"} {
		t.Run(network, func(t *testing.T) {
			address := startDNSReadinessServer(t, network, dnsTestAnswer)
			ctx, cancel := context.WithTimeout(context.Background(), time.Second)
			defer cancel()
			if err := probeDNSReadiness(ctx, network, address, "bootstrap.test", time.Second); err != nil {
				t.Fatal(err)
			}
		})
	}
}

func TestDNSReadinessInvalidProtocols(t *testing.T) {
	for _, network := range []string{"udp", "tcp"} {
		for _, kind := range []string{"no-answer", "malformed", "transaction-mismatch", "no-records", "rcode"} {
			t.Run(network+"/"+kind, func(t *testing.T) {
				address := startDNSReadinessServer(t, network, func(query []byte) []byte {
					answer := dnsTestAnswer(query)
					switch kind {
					case "no-answer":
						return nil
					case "malformed":
						return answer[:11]
					case "transaction-mismatch":
						answer[1]++
					case "no-records":
						answer[7] = 0
						answer = answer[:len(query)]
					case "rcode":
						answer[3] |= 2
					}
					return answer
				})
				ctx, cancel := context.WithTimeout(context.Background(), time.Second)
				defer cancel()
				if err := probeDNSReadiness(ctx, network, address, "bootstrap.test", 30*time.Millisecond); err == nil {
					t.Fatal("invalid DNS endpoint passed")
				}
			})
		}
	}
}

func TestNativeReadinessRequiresUDPAndTCPDNS(t *testing.T) {
	address := startDNSReadinessServer(t, "tcp", dnsTestAnswer)
	raw := dnsReadinessConfig(t, address, "")
	ctx, cancel := context.WithTimeout(context.Background(), 100*time.Millisecond)
	defer cancel()
	if err := checkNativeReadiness(ctx, raw); !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("TCP-only DNS should fail UDP readiness: %v", err)
	}
}

func TestNativeReadinessUDPOnlyDNS(t *testing.T) {
	address := startDNSReadinessServer(t, "udp", dnsTestAnswer)
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	if err := checkNativeReadiness(ctx, dnsReadinessConfig(t, address, "udp")); err != nil {
		t.Fatal(err)
	}
}

func TestNativeReadinessTCPAcceptWithoutDNS(t *testing.T) {
	address := startDNSReadinessServer(t, "tcp", func([]byte) []byte { return nil })
	ctx, cancel := context.WithTimeout(context.Background(), 100*time.Millisecond)
	defer cancel()
	if err := checkNativeReadiness(ctx, dnsReadinessConfig(t, address, "tcp")); !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("TCP accept without DNS should fail: %v", err)
	}
}

func TestNativeReadinessMixedOnly(t *testing.T) {
	address := startDNSReadinessServer(t, "tcp", func([]byte) []byte { return nil })
	host, port, _ := net.SplitHostPort(address)
	raw := []byte(`{"inbounds":[{"type":"mixed","listen":"` + host + `","listen_port":` + port + `}]}`)
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	if err := checkNativeReadiness(ctx, raw); err != nil {
		t.Fatal(err)
	}
}

func TestNativeReadinessDualTransportDNS(t *testing.T) {
	address := startDNSReadinessServer(t, "tcp", dnsTestAnswer)
	startDNSReadinessServer(t, "udp", dnsTestAnswer, address)
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	if err := checkNativeReadiness(ctx, dnsReadinessConfig(t, address, "")); err != nil {
		t.Fatal(err)
	}
}

func TestNativeReadinessRequiresTCPDNS(t *testing.T) {
	address := startDNSReadinessServer(t, "udp", dnsTestAnswer)
	ctx, cancel := context.WithTimeout(context.Background(), 100*time.Millisecond)
	defer cancel()
	if err := checkNativeReadiness(ctx, dnsReadinessConfig(t, address, "")); !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("UDP-only DNS should fail when config also enables TCP: %v", err)
	}
}

func TestDNSReadinessContextCancelsActiveRead(t *testing.T) {
	for _, network := range []string{"udp", "tcp"} {
		t.Run(network, func(t *testing.T) {
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			address := startDNSReadinessServer(t, network, func([]byte) []byte { cancel(); return nil })
			done := make(chan error, 1)
			go func() { done <- probeDNSReadiness(ctx, network, address, "bootstrap.test", time.Minute) }()
			select {
			case err := <-done:
				if err == nil {
					t.Fatal("canceled probe passed")
				}
			case <-time.After(time.Second):
				t.Fatal("context cancellation did not stop active DNS read")
			}
		})
	}
}

func TestDNSReadinessExtendedRcode(t *testing.T) {
	query, err := dnsReadinessQuery("bootstrap.test", 1)
	if err != nil {
		t.Fatal(err)
	}
	answer := dnsTestAnswer(query)
	answer[11] = 1
	answer = append(answer, 0, 0, 41, 0x10, 0, 1, 0, 0, 0, 0, 0)
	if err := validateDNSReadinessResponse(answer, query); err == nil {
		t.Fatal("extended error rcode passed readiness")
	}
	answer[len(answer)-6] = 0
	if err := validateDNSReadinessResponse(answer, query); err != nil {
		t.Fatalf("valid answer with OPT: %v", err)
	}
}

func TestDNSReadinessOversizedResponse(t *testing.T) {
	for _, network := range []string{"udp", "tcp"} {
		t.Run(network, func(t *testing.T) {
			address := startDNSReadinessServer(t, network, func([]byte) []byte { return make([]byte, dnsReadinessMaxPacket+1) })
			ctx, cancel := context.WithTimeout(context.Background(), time.Second)
			defer cancel()
			if err := probeDNSReadiness(ctx, network, address, "bootstrap.test", time.Second); err == nil {
				t.Fatal("oversized DNS response passed readiness")
			}
		})
	}
}

func FuzzDNSReadinessResponse(f *testing.F) {
	query, err := dnsReadinessQuery("bootstrap.test", 1)
	if err != nil {
		f.Fatal(err)
	}
	f.Add(dnsTestAnswer(query))
	f.Add(query)
	f.Add([]byte{})
	f.Fuzz(func(t *testing.T, response []byte) { _ = validateDNSReadinessResponse(response, query) })
}

func TestNativeReadinessCanceled(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if err := checkNativeReadiness(ctx, []byte(`{"inbounds":[{"type":"mixed","listen_port":2080}]}`)); !errors.Is(err, context.Canceled) {
		t.Fatalf("canceled readiness = %v", err)
	}
}

func dnsReadinessConfig(t *testing.T, address, network string) []byte {
	t.Helper()
	host, port, err := net.SplitHostPort(address)
	if err != nil {
		t.Fatal(err)
	}
	raw := `{"inbounds":[{"type":"direct","tag":"dns-in","listen":"` + host + `","listen_port":` + port + `,"network":"` + network + `"}],"route":{"rules":[{"inbound":["dns-in"],"action":"hijack-dns"}]},"dns":{"rules":[{"domain":["bootstrap.test"],"server":"dns-direct"}]}}`
	if !json.Valid([]byte(raw)) {
		t.Fatal("invalid fixture config")
	}
	return []byte(raw)
}

func dnsTestAnswer(query []byte) []byte {
	answer := append([]byte(nil), query...)
	answer[2] = 0x81
	answer[3] = 0x80
	answer[7] = 1
	return append(answer, 0xc0, 0x0c, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 192, 0, 2, 1)
}

func startDNSReadinessServer(t *testing.T, network string, respond func([]byte) []byte, bindAddress ...string) string {
	t.Helper()
	address := "127.0.0.1:0"
	if len(bindAddress) > 0 {
		address = bindAddress[0]
	}
	stop := make(chan struct{})
	if network == "udp" {
		listener, err := net.ListenPacket("udp", address)
		if err != nil {
			t.Fatal(err)
		}
		t.Cleanup(func() { close(stop); listener.Close() })
		go func() {
			buffer := make([]byte, 4096)
			for {
				n, peer, err := listener.ReadFrom(buffer)
				if err != nil {
					return
				}
				if reply := respond(append([]byte(nil), buffer[:n]...)); reply != nil {
					listener.WriteTo(reply, peer)
				}
			}
		}()
		return listener.LocalAddr().String()
	}
	listener, err := net.Listen("tcp", address)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { close(stop); listener.Close() })
	go func() {
		for {
			conn, err := listener.Accept()
			if err != nil {
				return
			}
			go func() {
				defer conn.Close()
				conn.SetDeadline(time.Now().Add(time.Second))
				var prefix [2]byte
				if _, err := io.ReadFull(conn, prefix[:]); err != nil {
					return
				}
				query := make([]byte, int(binary.BigEndian.Uint16(prefix[:])))
				if _, err := io.ReadFull(conn, query); err != nil {
					return
				}
				answer := respond(query)
				if answer == nil {
					<-stop
					return
				}
				binary.BigEndian.PutUint16(prefix[:], uint16(len(answer)))
				// Write separate chunks to exercise DNS-over-TCP stream framing.
				conn.Write(prefix[:1])
				conn.Write(prefix[1:])
				if len(answer) > 0 {
					conn.Write(answer[:1])
					conn.Write(answer[1:])
				}
			}()
		}
	}()
	return listener.Addr().String()
}

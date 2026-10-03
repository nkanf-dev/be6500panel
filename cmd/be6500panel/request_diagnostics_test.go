package main

import (
	"be6500panel/internal/requesttrace"
	"be6500panel/internal/router"
	managedruntime "be6500panel/internal/runtime"
	"context"
	"errors"
	"testing"
)

type diagnosticRuntimeFixture struct {
	states     []managedruntime.Status
	raw        []byte
	generation uint64
	err        error
	reads      int
}

func (f *diagnosticRuntimeFixture) Status(string) (managedruntime.Status, error) {
	if f.err != nil {
		return managedruntime.Status{}, f.err
	}
	n := f.reads
	f.reads++
	if n >= len(f.states) {
		n = len(f.states) - 1
	}
	return f.states[n], nil
}
func (f *diagnosticRuntimeFixture) Config(string) ([]byte, uint64, error) {
	return f.raw, f.generation, f.err
}

type diagnosticRouterFixture struct {
	addresses []string
	calls     int
	err       error
}

func (f *diagnosticRouterFixture) CaptureObservation(context.Context) (router.CaptureObservation, error) {
	f.calls++
	return router.CaptureObservation{LANAddresses: f.addresses}, f.err
}
func fixtureDiagnosticRuntime(bind string) *diagnosticRuntimeFixture {
	return &diagnosticRuntimeFixture{
		states: []managedruntime.Status{{State: managedruntime.Running, PID: 123, Generation: 7}},
		raw:    []byte(`{"inbounds":[{"type":"mixed","listen":"` + bind + `","listen_port":2091}]}`), generation: 7,
	}
}
func TestAcceptedDiagnosticProxy(t *testing.T) {
	t.Run("loopback does not depend on DHCP source", func(t *testing.T) {
		source := &diagnosticRouterFixture{err: errors.New("no lease file")}
		_, err := acceptedDiagnosticProxy(fixtureDiagnosticRuntime("127.0.0.1"), source)(context.Background())
		if err != nil || source.calls != 0 {
			t.Fatalf("err=%v calls=%d", err, source.calls)
		}
	})
	t.Run("LAN listener is verified only from adapter", func(t *testing.T) {
		source := &diagnosticRouterFixture{addresses: []string{"192.0.2.1"}}
		_, err := acceptedDiagnosticProxy(fixtureDiagnosticRuntime("192.0.2.1"), source)(context.Background())
		if err != nil || source.calls != 1 {
			t.Fatalf("err=%v calls=%d", err, source.calls)
		}
	})
	for _, test := range []struct {
		name   string
		change func(*diagnosticRuntimeFixture, *diagnosticRouterFixture)
	}{
		{"stopped", func(r *diagnosticRuntimeFixture, s *diagnosticRouterFixture) {
			r.states[0].State = managedruntime.Stopped
		}},
		{"recovery", func(r *diagnosticRuntimeFixture, s *diagnosticRouterFixture) { r.states[0].NeedsRecovery = true }},
		{"unverified LAN", func(r *diagnosticRuntimeFixture, s *diagnosticRouterFixture) { s.addresses = nil }},
		{"invalid interface source", func(r *diagnosticRuntimeFixture, s *diagnosticRouterFixture) { s.err = errors.New("invalid") }},
		{"stale accepted generation", func(r *diagnosticRuntimeFixture, s *diagnosticRouterFixture) { r.generation = 6 }},
		{"replaced process", func(r *diagnosticRuntimeFixture, s *diagnosticRouterFixture) {
			r.states = append(r.states, managedruntime.Status{State: managedruntime.Running, PID: 124, Generation: 7})
		}},
		{"changed generation", func(r *diagnosticRuntimeFixture, s *diagnosticRouterFixture) {
			r.states = append(r.states, managedruntime.Status{State: managedruntime.Running, PID: 123, Generation: 8})
		}},
		{"wildcard", func(r *diagnosticRuntimeFixture, s *diagnosticRouterFixture) {
			r.raw = []byte(`{"inbounds":[{"type":"mixed","listen":"0.0.0.0","listen_port":2091}]}`)
		}},
		{"no accepted config", func(r *diagnosticRuntimeFixture, s *diagnosticRouterFixture) {
			r.err = errors.New("private unavailable")
		}},
	} {
		t.Run(test.name, func(t *testing.T) {
			r := fixtureDiagnosticRuntime("192.0.2.1")
			s := &diagnosticRouterFixture{addresses: []string{"192.0.2.1"}}
			test.change(r, s)
			if _, err := acceptedDiagnosticProxy(r, s)(context.Background()); !errors.Is(err, requesttrace.ErrUnavailable) {
				t.Fatal(err)
			}
		})
	}
	t.Run("no runtime", func(t *testing.T) {
		if _, err := acceptedDiagnosticProxy(nil, nil)(context.Background()); !errors.Is(err, requesttrace.ErrUnavailable) {
			t.Fatal(err)
		}
	})
	t.Run("cancelled", func(t *testing.T) {
		ctx, cancel := context.WithCancel(context.Background())
		cancel()
		if _, err := acceptedDiagnosticProxy(fixtureDiagnosticRuntime("127.0.0.1"), nil)(ctx); !errors.Is(err, context.Canceled) {
			t.Fatal(err)
		}
	})
}

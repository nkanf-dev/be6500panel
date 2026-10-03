package router

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"testing"
)

func actionObserver(t *testing.T) *ServiceObserver {
	t.Helper()
	root := serviceRoot(t)
	fixtureFile(t, root, "/etc/init.d/ddns", "#!/bin/sh\n")
	for _, name := range []string{"ddns", "dnsmasq"} {
		if err := os.Chmod(filepath.Join(root, "etc/init.d", name), 0700); err != nil {
			t.Fatal(err)
		}
	}
	o := NewServiceObserver(root)
	// Simulate a live-capable source entirely inside this synthetic tree. Source
	// and runner are injected; this test cannot call ubus or native init scripts.
	o.source = func(context.Context) ([]byte, error) { return o.adapter.readFile(serviceFixture, commandLimit) }
	o.adapter.live = true
	return o
}
func TestServiceActionsFixedAllowlistAndFixtureCannotRun(t *testing.T) {
	o := NewServiceObserver(serviceRoot(t))
	calls := 0
	o.runAction = func(context.Context, string, string) error { calls++; return nil }
	for _, input := range []ServiceActionRequest{
		{Service: "../ddns", Action: "start"}, {Service: "/etc/init.d/ddns", Action: "start"}, {Service: "ddns;reboot", Action: "start"},
		{Service: "ddns", Action: "start;reboot"}, {Service: "ddns", Action: "enable"}, {Service: "dnsmasq", Action: "stop", ConfirmImpact: true},
		{Service: "be6500-rescue", Action: "stop"}, {Service: "rescue", Action: "stop"}, {Service: "be6500panel", Action: "stop"}, {Service: "dropbear", Action: "restart"},
		{Service: "network", Action: "restart"}, {Service: "wifi", Action: "restart"}, {Service: "firewall", Action: "restart"},
	} {
		r, err := o.Action(context.Background(), input)
		if !errors.Is(err, ErrServiceAction) || r.ErrorCode != "service_action_not_allowed" {
			t.Fatalf("allowed %+v %+v %v", input, r, err)
		}
	}
	r, err := o.Action(context.Background(), ServiceActionRequest{Service: "ddns", Action: "start"})
	if err == nil || r.ErrorCode != "fixture_read_only" {
		t.Fatalf("fixture action %+v %v", r, err)
	}
	if calls != 0 {
		t.Fatal("refused action ran command")
	}
}
func TestServiceActionInstalledAndImpactRequired(t *testing.T) {
	o := actionObserver(t)
	calls := 0
	o.runAction = func(context.Context, string, string) error { calls++; return nil }
	result, err := o.Action(context.Background(), ServiceActionRequest{Service: "dnsmasq", Action: "reload"})
	if err == nil || result.ErrorCode != "service_impact_confirmation_required" || calls != 0 {
		t.Fatalf("unconfirmed %+v %v", result, err)
	}
	os.Remove(filepath.Join(o.adapter.root, "etc/init.d/ddns"))
	result, err = o.Action(context.Background(), ServiceActionRequest{Service: "ddns", Action: "start"})
	if err == nil || result.ErrorCode != "service_not_installed" || calls != 0 {
		t.Fatalf("missing installed %+v %v", result, err)
	}
}
func TestServiceActionFixedCommandReadbackAndNoHealthClaim(t *testing.T) {
	for _, action := range []string{"reload", "restart"} {
		t.Run(action, func(t *testing.T) {
			o := actionObserver(t)
			var commands [][]string
			o.runAction = func(ctx context.Context, path, verb string) error {
				commands = append(commands, []string{path, verb})
				fixtureFile(t, o.adapter.root, serviceFixture, `{"dnsmasq":{"instances":{"main":{"running":false,"exit_code":2}}}}`)
				return nil
			}
			result, err := o.Action(context.Background(), ServiceActionRequest{Service: "dnsmasq", Action: action, ConfirmImpact: true})
			if err != nil || !result.CommandAccepted || result.Snapshot.Stale {
				t.Fatalf("action %+v %v", result, err)
			}
			if !reflect.DeepEqual(commands, [][]string{{"/etc/init.d/dnsmasq", action}}) {
				t.Fatalf("argv %v", commands)
			}
			if r := serviceRow(t, result.Snapshot, "dnsmasq"); r.ProcessState != "failed" {
				t.Fatalf("accepted != health %+v", r)
			}
			cached, _ := o.Snapshot(context.Background())
			if r := serviceRow(t, cached, "dnsmasq"); r.ProcessState != "failed" {
				t.Fatal("readback did not invalidate cache")
			}
		})
	}
}
func TestServiceUnknownCannotStopOrStaleCannotAct(t *testing.T) {
	o := actionObserver(t)
	commands := 0
	o.runAction = func(context.Context, string, string) error { commands++; return nil }
	result, err := o.Action(context.Background(), ServiceActionRequest{Service: "ddns", Action: "stop"})
	if err == nil || result.ErrorCode != "service_action_not_available" || commands != 0 {
		t.Fatalf("unknown stop %+v %v", result, err)
	}
	o.source = func(context.Context) ([]byte, error) { return nil, context.DeadlineExceeded }
	result, err = o.Action(context.Background(), ServiceActionRequest{Service: "ddns", Action: "start"})
	if err == nil || result.ErrorCode != "service_observation_unavailable" || !result.Snapshot.Stale || commands != 0 {
		t.Fatalf("stale action %+v %v", result, err)
	}
	for _, r := range result.Snapshot.Services {
		if len(r.Actions) > 0 {
			t.Fatal("stale actions exposed")
		}
	}
}
func TestServiceCommandFailureAndReadbackFailureVisible(t *testing.T) {
	o := actionObserver(t)
	o.runAction = func(context.Context, string, string) error { return context.DeadlineExceeded }
	result, err := o.Action(context.Background(), ServiceActionRequest{Service: "ddns", Action: "start"})
	if err == nil || result.CommandAccepted || result.ErrorCode != "service_command_timeout" || result.Snapshot.SampledAt == nil {
		t.Fatalf("command failure %+v %v", result, err)
	}
	o.runAction = func(context.Context, string, string) error {
		o.source = func(context.Context) ([]byte, error) { return nil, context.DeadlineExceeded }
		return nil
	}
	result, err = o.Action(context.Background(), ServiceActionRequest{Service: "ddns", Action: "start"})
	if err == nil || !result.CommandAccepted || result.ErrorCode != "service_readback_unavailable" || !result.Snapshot.Stale {
		t.Fatalf("readback failure %+v %v", result, err)
	}
}

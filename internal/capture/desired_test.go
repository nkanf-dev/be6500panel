package capture

import (
	"context"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	"be6500panel/internal/proxy"
	"be6500panel/internal/router"
)

const acceptedNative = `{"inbounds":[{"type":"mixed","listen":"192.0.2.1","listen_port":2081},{"type":"tproxy","listen":"127.0.0.1","listen_port":7894},{"type":"direct","tag":"dns-in","listen":"192.0.2.1","listen_port":1054}],"outbounds":[{"type":"vless","server":"node.example"}],"dns":{"servers":[{"type":"tls","server":"203.0.113.53","detour":"direct"}]},"route":{"rules":[{"inbound":["dns-in"],"action":"hijack-dns"},{"ip_version":6,"outbound":"direct"}]}}`

func deviceObservation() router.CaptureObservation {
	return router.CaptureObservation{LANPrefixes: []string{"192.0.2.0/24"}, LANAddresses: []string{"192.0.2.1"}, ManagementIPs: []string{"192.0.2.1", "127.0.0.1", "198.51.100.1"}, Devices: []router.Device{{MAC: "02:00:00:00:00:10", IP: "192.0.2.10", Hostname: "mac", Eligible: true}, {MAC: "02:00:00:00:00:11", IP: "192.0.2.11", Hostname: "phone", Eligible: true}}}
}
func desiredDevices() Desired {
	return Desired{Enabled: true, Devices: []DeviceSelection{{MAC: "02:00:00:00:00:10"}, {MAC: "02:00:00:00:00:11"}}, IPv6: proxy.IPv6Direct}
}
func fakeResolve(ctx context.Context, host string) ([]string, error) {
	if host != "node.example" {
		return nil, errors.New("unexpected resolver hostname")
	}
	return []string{"203.0.113.4"}, nil
}
func TestAcceptedNativeConfigAndCurrentDevicesOwnRestore(t *testing.T) {
	input, clients, err := BuildFromAccepted(context.Background(), desiredDevices(), []byte(acceptedNative), deviceObservation(), fakeResolve)
	if err != nil {
		t.Fatal(err)
	}
	if input.Ports != (proxy.Ports{Mixed: 2081, TProxy: 7894, DNS: 1054}) || !reflect.DeepEqual(input.ClientIPv4s, []string{"192.0.2.10", "192.0.2.11"}) || !reflect.DeepEqual(input.EndpointIPs, []string{"203.0.113.4", "203.0.113.53"}) || !reflect.DeepEqual(input.RouterDNSAddresses, []string{"192.0.2.1"}) {
		t.Fatalf("stale/unbounded config: %+v", input)
	}
	if !reflect.DeepEqual(input.ClientMACs, map[string]string{"192.0.2.10": "02:00:00:00:00:10", "192.0.2.11": "02:00:00:00:00:11"}) {
		t.Fatalf("missing MAC guards: %v", input.ClientMACs)
	}
	if clients[0].Hostname != "mac" || clients[1].IP != "192.0.2.11" {
		t.Fatal(clients)
	}
	plan, err := proxy.PlanOwnedRules(input)
	if err != nil {
		t.Fatal(err)
	}
	for _, command := range plan.Apply {
		for _, arg := range command {
			if arg == "192.0.2.0/24" {
				t.Fatal("LAN scope widened")
			}
		}
	}
}
func TestRestoreResolvesDHCPChangeAndUnresolvedNeverUsesStaleIP(t *testing.T) {
	c := testController(t, idleRunner)
	observation := deviceObservation()
	c.SetBuilder(func(ctx context.Context, d Desired) (proxy.RulesPlanInput, []Client, error) {
		return BuildFromAccepted(ctx, d, []byte(acceptedNative), observation, fakeResolve)
	})
	if _, err := c.Select(context.Background(), desiredDevices()); err != nil {
		t.Fatal(err)
	}
	observation.Devices[0].IP = "192.0.2.20"
	c.runner = savedPlanRunner(t, *c.plan)
	state, err := c.ReconcileDesired(context.Background())
	// GET proves the old installed scope. Only the mutation lane may replace it.
	if err == nil || !state.Active || state.CleanupPending || !state.Desired || state.State != "scope-changed" || state.Error != "capture_scope_changed_apply_required" {
		t.Fatalf("read-only drift observation hid installed rules: %+v %v", state, err)
	}
	c.runner = idleRunner
	state, err = c.Refresh(context.Background())
	if err != nil || !state.Active || state.Clients[0].IP != "192.0.2.20" {
		t.Fatalf("DHCP refresh failed: %+v %v", state, err)
	}
	observation.Devices[0] = router.Device{MAC: "02:00:00:00:00:99", IP: "192.0.2.20", Eligible: true}
	state, err = c.Refresh(context.Background())
	if err != nil || !state.Active || !state.Desired || state.State != "partial" || state.Error != "capture_devices_pending" || state.Clients[0].MAC != "02:00:00:00:00:10" || state.Clients[0].IP != "" || state.Clients[1].IP != "192.0.2.11" {
		t.Fatalf("reused/unresolved IP retained: %+v %v", state, err)
	}
	for _, command := range c.plan.Apply {
		for _, arg := range command {
			if arg == "192.0.2.20/32" {
				t.Fatal("reused IP captured in partial group")
			}
		}
	}
	observation.Devices = nil
	state, err = c.Restore(context.Background())
	if err == nil || state.Active || !state.Desired || state.State != "suspended" {
		t.Fatalf("fully offline group did not suspend: %+v %v", state, err)
	}
	raw, err := os.ReadFile(c.desiredPath)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(raw), "192.0.2") || strings.Contains(string(raw), "listen") || strings.Contains(string(raw), "ports") {
		t.Fatalf("saved desired contains stale live metadata: %s", raw)
	}
}
func TestAcceptedNativeRefusesListenerPolicyAndLANMismatches(t *testing.T) {
	for _, tc := range []struct{ name, old, new string }{
		{"loopback-dns", "\"listen\":\"192.0.2.1\",\"listen_port\":1054", "\"listen\":\"127.0.0.1\",\"listen_port\":1054"},
		{"wan-dns", "\"listen\":\"192.0.2.1\",\"listen_port\":1054", "\"listen\":\"198.51.100.1\",\"listen_port\":1054"},
		{"wrong-tproxy", "\"listen\":\"127.0.0.1\"", "\"listen\":\"192.0.2.1\""},
		{"missing-dns-route", "hijack-dns", "route"},
		{"wrong-policy", "\"ip_version\":6,\"outbound\":\"direct\"", "\"ip_version\":6,\"action\":\"reject\""},
		{"udp-only-dns", "\"listen_port\":1054", "\"listen_port\":1054,\"network\":\"udp\""},
		{"colliding-port", "\"listen_port\":1054", "\"listen_port\":7894"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			raw := strings.Replace(acceptedNative, tc.old, tc.new, 1)
			if raw == acceptedNative {
				t.Fatal("fixture edit missed")
			}
			if _, _, err := BuildFromAccepted(context.Background(), desiredDevices(), []byte(raw), deviceObservation(), fakeResolve); err == nil {
				t.Fatal("unsafe native accepted")
			}
		})
	}
	observation := deviceObservation()
	observation.Devices[0].IP = "198.51.100.10"
	if _, _, err := BuildFromAccepted(context.Background(), desiredDevices(), []byte(acceptedNative), observation, fakeResolve); err == nil {
		t.Fatal("upstream client accepted")
	}
}
func TestDisableSavedBeforeFailedCleanupAndFreshManagerCannotRestore(t *testing.T) {
	fail := false
	dir := t.TempDir()
	c, err := New(dir, func(ctx context.Context, a []string) ([]byte, error) {
		if fail && len(a) > 5 && a[5] == "-D" {
			return nil, errors.New("busy")
		}
		return idleRunner(ctx, a)
	})
	if err != nil {
		t.Fatal(err)
	}
	c.SetBuilder(func(ctx context.Context, d Desired) (proxy.RulesPlanInput, []Client, error) {
		return BuildFromAccepted(ctx, d, []byte(acceptedNative), deviceObservation(), fakeResolve)
	})
	if _, err = c.Select(context.Background(), desiredDevices()); err != nil {
		t.Fatal(err)
	}
	fail = true
	if err = c.Disable(context.Background()); err == nil {
		t.Fatal("cleanup failure hidden")
	}
	if c.Status().Desired || !c.Status().CleanupPending {
		t.Fatal(c.Status())
	}
	restored, err := New(dir, idleRunner)
	if err != nil {
		t.Fatal(err)
	}
	called := false
	restored.SetBuilder(func(context.Context, Desired) (proxy.RulesPlanInput, []Client, error) {
		called = true
		return proxy.RulesPlanInput{}, nil, nil
	})
	if _, err = restored.Restore(context.Background()); err != nil {
		t.Fatal(err)
	}
	if called || restored.Status().Desired || restored.Status().Active {
		t.Fatal("disabled state reactivated")
	}
}
func TestLegacyJournalDoesNotEnableDesiredScope(t *testing.T) {
	dir := t.TempDir()
	raw, _ := json.Marshal(testPlan(t))
	if err := os.WriteFile(filepath.Join(dir, "capture-journal.json"), raw, 0600); err != nil {
		t.Fatal(err)
	}
	c, err := New(dir, idleRunner)
	if err != nil {
		t.Fatal(err)
	}
	called := false
	c.SetBuilder(func(context.Context, Desired) (proxy.RulesPlanInput, []Client, error) {
		called = true
		return testInput(), nil, nil
	})
	if _, err = c.Restore(context.Background()); err != nil {
		t.Fatal(err)
	}
	if called || c.Status().Active || c.Status().Desired {
		t.Fatal("journal activated desired scope")
	}
}

func TestThreeDeviceGroupReplacementRemovesOnlyDeselectedClient(t *testing.T) {
	controller := testController(t, idleRunner)
	observation := deviceObservation()
	observation.Devices = append(observation.Devices, router.Device{MAC: "02:00:00:00:00:12", IP: "192.0.2.12", Eligible: true})
	desired := desiredDevices()
	desired.Devices = append(desired.Devices, DeviceSelection{MAC: "02:00:00:00:00:12"})
	controller.SetBuilder(func(ctx context.Context, d Desired) (proxy.RulesPlanInput, []Client, error) {
		return BuildFromAccepted(ctx, d, []byte(acceptedNative), observation, fakeResolve)
	})
	state, err := controller.Select(context.Background(), desired)
	if err != nil || !state.Active || len(state.Clients) != 3 {
		t.Fatalf("group not active: %+v %v", state, err)
	}
	desired.Devices = []DeviceSelection{desired.Devices[0], desired.Devices[2]}
	state, err = controller.Select(context.Background(), desired)
	if err != nil || !state.Active || len(state.Clients) != 2 || state.Clients[0].IP != "192.0.2.10" || state.Clients[1].IP != "192.0.2.12" {
		t.Fatalf("replacement lost remaining group: %+v %v", state, err)
	}
	for _, command := range controller.plan.Apply {
		for _, arg := range command {
			if arg == "192.0.2.11/32" || arg == "192.0.2.0/24" {
				t.Fatalf("deselected or broad scope restored: %v", command)
			}
		}
	}
	desiredRead := controller.Desired()
	if len(desiredRead.Devices) != 2 {
		t.Fatal("subset not saved")
	}
	desiredRead.Devices[0].MAC = "02:00:00:00:00:99"
	if controller.Desired().Devices[0].MAC != "02:00:00:00:00:10" {
		t.Fatal("desired read alias")
	}
}

func TestReadOnlyReconcileAndBoundedBackgroundFailureRetry(t *testing.T) {
	calls := 0
	fail := false
	c := testController(t, func(ctx context.Context, args []string) ([]byte, error) {
		if !isReadCommand(args) {
			calls++
		}
		if fail && len(args) > 5 && args[5] == "-N" {
			return nil, errors.New("kernel failure")
		}
		return idleRunner(ctx, args)
	})
	observation := deviceObservation()
	c.SetBuilder(func(ctx context.Context, d Desired) (proxy.RulesPlanInput, []Client, error) {
		return BuildFromAccepted(ctx, d, []byte(acceptedNative), observation, fakeResolve)
	})
	if _, err := c.Select(context.Background(), desiredDevices()); err != nil {
		t.Fatal(err)
	}
	baseline := calls
	observation.Devices[0].IP = "192.0.2.20"
	mutationRunner := c.runner
	c.runner = savedPlanRunner(t, *c.plan)
	state, err := c.ReconcileDesired(context.Background())
	if err == nil || !state.Active || state.CleanupPending || state.State != "scope-changed" {
		t.Fatal("drift observation did not prove saved resources", state, err)
	}
	c.runner = mutationRunner
	if calls != baseline {
		t.Fatal("GET observation mutated rules")
	}
	fail = true
	if _, err := c.Refresh(context.Background()); err == nil {
		t.Fatal("failed apply hidden")
	}
	baseline = calls
	if _, err := c.Refresh(context.Background()); err == nil {
		t.Fatal("failed same input must require explicit retry")
	}
	if calls != baseline {
		t.Fatal("timer blindly reapplied failed commands")
	}
	fail = false
	if _, err := c.Restore(context.Background()); err != nil {
		t.Fatal("explicit retry failed", err)
	}
	if !c.Status().Active {
		t.Fatal("explicit retry did not restore")
	}
	if err := c.DisableRetainingSelection(context.Background()); err != nil {
		t.Fatal(err)
	}
	baseline = calls
	if _, err := c.Refresh(context.Background()); err != nil {
		t.Fatal(err)
	}
	if calls != baseline || c.Status().Active || c.Status().Desired {
		t.Fatal("disabled retained selection applied")
	}
}

func TestExplicitFailedApplyIsNotTimerRetriedUntilExplicitStart(t *testing.T) {
	mutations := 0
	c := testController(t, func(ctx context.Context, args []string) ([]byte, error) {
		if !isReadCommand(args) {
			mutations++
		}
		if len(args) > 5 && args[5] == "-N" {
			return nil, errors.New("no TPROXY")
		}
		return idleRunner(ctx, args)
	})
	c.SetBuilder(func(ctx context.Context, d Desired) (proxy.RulesPlanInput, []Client, error) {
		return BuildFromAccepted(ctx, d, []byte(acceptedNative), deviceObservation(), fakeResolve)
	})
	if _, err := c.Select(context.Background(), desiredDevices()); err == nil {
		t.Fatal("failed explicit apply hidden")
	}
	baseline := mutations
	if _, err := c.Refresh(context.Background()); err == nil {
		t.Fatal("failed timer refresh hidden")
	}
	if mutations != baseline {
		t.Fatal("background blindly retried explicit failed apply")
	}
}

func TestMACMatchLoadFailureIsNotTreatedAsAbsentHook(t *testing.T) {
	command := []string{"iptables", "-w", "5", "-t", "mangle", "-D", "PREROUTING", "-i", "br-lan", "-s", "192.0.2.10/32", "-m", "mac", "--mac-source", "02:00:00:00:00:10", "-j", "B6P_V4_CAPTURE"}
	if resourceAbsent(command, []byte("iptables: No chain/target/match by that name."), errors.New("failed match load")) {
		t.Fatal("ambiguous MAC module failure lost cleanup ownership")
	}
	if !resourceAbsent(command, []byte("iptables: Bad rule (does a matching rule exist in that chain?)."), errors.New("absent")) {
		t.Fatal("explicit matching-rule absence not recognized")
	}
}

func TestAmbiguousMissingMACHookNeedsTargetChainAbsenceProof(t *testing.T) {
	controller := testController(t, idleRunner)
	controller.SetBuilder(func(ctx context.Context, d Desired) (proxy.RulesPlanInput, []Client, error) {
		return BuildFromAccepted(ctx, d, []byte(acceptedNative), deviceObservation(), fakeResolve)
	})
	if _, err := controller.Select(context.Background(), desiredDevices()); err != nil {
		t.Fatal(err)
	}
	proved := 0
	controller.runner = func(ctx context.Context, args []string) ([]byte, error) {
		if len(args) > 5 && args[5] == "-D" {
			return []byte("iptables: No chain/target/match by that name."), errors.New("absent")
		}
		if len(args) == 7 && args[5] == "-S" {
			proved++
			return absentChain()
		}
		return nil, nil
	}
	if err := controller.Cleanup(context.Background()); err != nil {
		t.Fatal("separately proved missing hooks should clean", err)
	}
	if proved != 4 || controller.Status().CleanupPending {
		t.Fatal("missing target proof count", proved)
	}
}

func TestExplicitIPv6ScopeSharesAuthorizedMACGuard(t *testing.T) {
	desired := desiredDevices()
	desired.Devices = desired.Devices[:1]
	desired.IPv6 = proxy.IPv6Follow
	desired.ClientIPv6 = "2001:db8::10"
	native := strings.Replace(acceptedNative, `"listen":"127.0.0.1"`, `"listen":"::"`, 1)
	native = strings.Replace(native, `"listen":"192.0.2.1","listen_port":1054`, `"listen":"::","listen_port":1054`, 1)
	native = strings.Replace(native, `,{"ip_version":6,"outbound":"direct"}`, ``, 1)
	input, _, err := BuildFromAccepted(context.Background(), desired, []byte(native), deviceObservation(), fakeResolve)
	if err != nil {
		t.Fatal(err)
	}
	if input.ClientMACs["192.0.2.10"] != "02:00:00:00:00:10" || input.ClientMACs["2001:db8::10"] != "02:00:00:00:00:10" {
		t.Fatal("IPv6 exactidentity guard missing", input.ClientMACs)
	}
}

package capture

import (
	"context"
	"encoding/json"
	"errors"
	"os"
	"reflect"
	"slices"
	"strings"
	"sync"
	"testing"
	"time"

	"be6500panel/internal/proxy"
	"be6500panel/internal/router"
)

// Gateway fixtures are synthetic declarations, accepted native bytes and fake
// observations. No native command, network or running core is used here.
func gatewayDesired(prefixes ...string) Desired {
	if len(prefixes) == 0 {
		prefixes = []string{"192.168.31.0/24"}
	}
	return Desired{Scope: proxy.CaptureScopeGateway, LANIPv4Prefixes: prefixes, Enabled: true, IPv6: proxy.IPv6Direct}
}

func gatewayObservation() router.CaptureObservation {
	return router.CaptureObservation{
		LANPrefixes: []string{"192.168.31.0/24"}, LANAddresses: []string{"192.168.31.1"},
		ManagementIPs: []string{"192.168.31.1", "127.0.0.1", "198.51.100.1"},
	}
}

func gatewayNative() []byte {
	return []byte(strings.ReplaceAll(acceptedRoutedTUN, "192.0.2.1", "192.168.31.1"))
}

func gatewayInput(t *testing.T, d Desired) proxy.RulesPlanInput {
	t.Helper()
	input, clients, err := BuildFromAccepted(context.Background(), d, gatewayNative(), gatewayObservation(), fakeResolve)
	if err != nil || len(clients) != 0 {
		t.Fatalf("gateway accepted build: %+v %v %v", input, clients, err)
	}
	return input
}

func gatewayPlan(t *testing.T, d Desired) proxy.OwnedRulesPlan {
	t.Helper()
	plan, err := proxy.PlanOwnedRules(gatewayInput(t, d))
	if err != nil {
		t.Fatal(err)
	}
	return plan
}

func gatewayRules(prefixes []string) string {
	lines := ""
	for _, prefix := range prefixes {
		lines += "16500: from " + prefix + " iif br-lan fwmark 0x4000/0x4000 lookup 16500\n"
	}
	return lines
}

func gatewayObservedRunner(t *testing.T, plan proxy.OwnedRulesPlan, route, rules string) Runner {
	t.Helper()
	base := tunObservedRunner(t, plan, route, "")
	return func(ctx context.Context, argv []string) ([]byte, error) {
		if slices.Equal(argv, ruleShow(4)) {
			return []byte(rules), nil
		}
		return base(ctx, argv)
	}
}

func TestGatewayDesiredCanonicalDeclarationAndOffRetention(t *testing.T) {
	d := gatewayDesired("192.168.31.0/24", "10.1.0.0/16")
	got, err := normalizeDesired(d)
	if err != nil || got.Scope != proxy.CaptureScopeGateway || !reflect.DeepEqual(got.LANIPv4Prefixes, []string{"10.1.0.0/16", "192.168.31.0/24"}) || len(desiredClients(got)) != 0 {
		t.Fatalf("declaration normalization: %+v %v", got, err)
	}
	d.LANIPv4Prefixes[0] = "192.168.99.0/24"
	if slices.Contains(got.LANIPv4Prefixes, d.LANIPv4Prefixes[0]) {
		t.Fatal("normalized declaration aliases caller")
	}
	got.Enabled = false
	off, err := normalizeDesired(got)
	if err != nil || off.Enabled || !reflect.DeepEqual(off, got) {
		t.Fatalf("off discarded approved scope: %+v %v", off, err)
	}
	for _, tc := range []struct {
		name string
		edit func(*Desired)
	}{
		{"empty", func(d *Desired) { d.LANIPv4Prefixes = nil }},
		{"default", func(d *Desired) { d.LANIPv4Prefixes = []string{"0.0.0.0/0"} }},
		{"public", func(d *Desired) { d.LANIPv4Prefixes = []string{"192.0.2.0/24"} }},
		{"noncanonical", func(d *Desired) { d.LANIPv4Prefixes = []string{"192.168.31.7/24"} }},
		{"overlap", func(d *Desired) { d.LANIPv4Prefixes = []string{"192.168.31.0/24", "192.168.31.2/32"} }},
		{"duplicate", func(d *Desired) { d.LANIPv4Prefixes = []string{"192.168.31.0/24", "192.168.31.0/24"} }},
		{"too-many", func(d *Desired) {
			d.LANIPv4Prefixes = []string{"10.1.0.1/32", "10.1.0.2/32", "10.1.0.3/32", "10.1.0.4/32", "10.1.0.5/32", "10.1.0.6/32", "10.1.0.7/32", "10.1.0.8/32", "10.1.0.9/32"}
		}},
		{"devices", func(d *Desired) { d.Devices = desiredDevices().Devices }},
		{"client-v4", func(d *Desired) { d.ClientIPv4 = "192.168.31.10" }},
		{"client-v6", func(d *Desired) { d.ClientIPv6 = "fd00::10" }},
		{"ipv6-follow", func(d *Desired) { d.IPv6 = proxy.IPv6Follow }},
		{"unknown-scope", func(d *Desired) { d.Scope = "other" }},
		{"devices-with-prefix", func(d *Desired) { d.Scope = proxy.CaptureScopeDevices }},
		{"legacy-with-prefix", func(d *Desired) { d.Scope = "" }},
	} {
		t.Run(tc.name, func(t *testing.T) {
			for _, enabled := range []bool{false, true} {
				d := gatewayDesired()
				d.Enabled = enabled
				tc.edit(&d)
				if _, err := normalizeDesired(d); err == nil {
					t.Fatalf("unsafe declaration accepted (enabled=%v): %+v", enabled, d)
				}
			}
		})
	}
}

func TestGatewaySelectUsesFreshLANWithoutDeviceInventoryAndSavesOff(t *testing.T) {
	dir := t.TempDir()
	c, err := New(dir, tunIdleRunner)
	if err != nil {
		t.Fatal(err)
	}
	c.SetBuilder(func(ctx context.Context, d Desired) (proxy.RulesPlanInput, []Client, error) {
		return BuildFromAccepted(ctx, d, gatewayNative(), gatewayObservation(), fakeResolve)
	})
	d := gatewayDesired()
	state, err := c.Select(context.Background(), d)
	if err != nil || !state.Active || !state.Desired || state.Scope != proxy.CaptureScopeGateway || len(state.Clients) != 0 || len(state.InstalledClients) != 0 || !reflect.DeepEqual(state.LANIPv4Prefixes, d.LANIPv4Prefixes) || !reflect.DeepEqual(state.InstalledLANIPv4Prefixes, d.LANIPv4Prefixes) {
		t.Fatalf("prefix activation: %+v %v", state, err)
	}
	d.LANIPv4Prefixes[0] = "10.0.0.0/8"
	if !reflect.DeepEqual(c.Desired().LANIPv4Prefixes, []string{"192.168.31.0/24"}) {
		t.Fatal("saved desired aliases explicit selection")
	}
	if err := c.Disable(context.Background()); err != nil {
		t.Fatal(err)
	}
	state = c.Status()
	if state.Desired || state.Active || state.CleanupPending || state.Scope != proxy.CaptureScopeGateway || len(state.InstalledLANIPv4Prefixes) != 0 || !reflect.DeepEqual(state.LANIPv4Prefixes, []string{"192.168.31.0/24"}) || len(state.Clients) != 0 {
		t.Fatalf("off declaration: %+v", state)
	}
	fresh, err := New(dir, func(context.Context, []string) ([]byte, error) {
		t.Fatal("loading disabled intent must not run commands")
		return nil, nil
	})
	if err != nil || fresh.Desired().Enabled || !reflect.DeepEqual(fresh.Desired(), c.Desired()) {
		t.Fatalf("off restart lost declaration: %v %+v", err, fresh)
	}
	fresh.SetBuilder(func(context.Context, Desired) (proxy.RulesPlanInput, []Client, error) {
		t.Fatal("restore evaluated disabled gateway")
		return proxy.RulesPlanInput{}, nil, nil
	})
	if state, err := fresh.Restore(context.Background()); err != nil || state.Active || state.Desired {
		t.Fatal(state, err)
	}
}

func TestGatewayAcceptedUsesOnlyDeclaredObservedPrefixIncludingNarrowCanary(t *testing.T) {
	for _, declared := range []string{"192.168.31.0/24", "192.168.31.128/25", "192.168.31.250/32"} {
		t.Run(declared, func(t *testing.T) {
			d := gatewayDesired(declared)
			observed := gatewayObservation()
			observed.Devices = []router.Device{{MAC: "02:00:00:00:00:99", IP: "192.168.31.99", Eligible: false}}
			input, clients, err := BuildFromAccepted(context.Background(), d, gatewayNative(), observed, fakeResolve)
			if err != nil || input.Scope != proxy.CaptureScopeGateway || input.Datapath != proxy.DatapathRoutedTUN || input.LANInterface != "br-lan" || input.ClientIPv4 != "" || input.ClientIPv6 != "" || len(input.ClientIPv4s) != 0 || len(input.ClientIPv6s) != 0 || input.ClientMACs != nil || len(clients) != 0 || !reflect.DeepEqual(input.LANIPv4Prefixes, []string{declared}) {
				t.Fatalf("builder used inventory or broadened declaration: %+v %v %v", input, clients, err)
			}
			d.LANIPv4Prefixes[0] = "10.0.0.0/8"
			observed.LANPrefixes[0] = "10.0.0.0/8"
			if !reflect.DeepEqual(input.LANIPv4Prefixes, []string{declared}) {
				t.Fatal("input aliases desired or observed prefixes")
			}
		})
	}
	for _, declaration := range []string{"192.168.32.250/32", "192.168.30.0/23", "10.0.0.0/8"} {
		if _, _, err := BuildFromAccepted(context.Background(), gatewayDesired(declaration), gatewayNative(), gatewayObservation(), fakeResolve); err == nil || err.Error() != "capture_gateway_scope_missing" {
			t.Fatalf("outside or broader declaration accepted: %s %v", declaration, err)
		}
	}
	for _, prefixes := range [][]string{nil, {"0.0.0.0/0"}, {"192.168.31.1/24"}, {"192.0.2.0/24"}, {"192.168.31.0/24", "broken"}, {"192.168.31.0/24", "192.168.31.128/25"}} {
		observed := gatewayObservation()
		observed.LANPrefixes = prefixes
		if _, _, err := BuildFromAccepted(context.Background(), gatewayDesired(), gatewayNative(), observed, fakeResolve); err == nil || err.Error() != "capture_lan_unavailable" {
			t.Fatalf("malformed observed prefixes accepted: %v %v", prefixes, err)
		}
	}
}

func TestGatewayAcceptedRequiresActualMainTUNAndRetainsManagementProvenance(t *testing.T) {
	d := gatewayDesired()
	legacy := []byte(strings.ReplaceAll(acceptedNative, "192.0.2.1", "192.168.31.1"))
	if _, _, err := BuildFromAccepted(context.Background(), d, legacy, gatewayObservation(), fakeResolve); err == nil || err.Error() != "capture_gateway_tun_required" {
		t.Fatalf("gateway used legacy listener: %v", err)
	}
	observed := gatewayObservation()
	observed.ManagementIPs = append(observed.ManagementIPs, "172.31.255.253")
	observed.InterfaceAddresses = []router.CaptureInterfaceAddress{{Interface: "b6p-tun", Address: "172.31.255.253/30"}}
	input, _, err := BuildFromAccepted(context.Background(), d, gatewayNative(), observed, fakeResolve)
	if err != nil || slices.Contains(input.ManagementIPs, "172.31.255.253") {
		t.Fatalf("own TUN local address became management collision: %+v %v", input, err)
	}
	observed.InterfaceAddresses[0].Interface = "b6p-foreign"
	if _, _, err := BuildFromAccepted(context.Background(), d, gatewayNative(), observed, fakeResolve); err == nil || err.Error() != "capture_tun_prefix_collision" {
		t.Fatal("foreign TUN address was exempted", err)
	}
	observed = gatewayObservation()
	observed.ManagementIPs = append(observed.ManagementIPs, "172.31.255.253")
	if _, _, err := BuildFromAccepted(context.Background(), d, gatewayNative(), observed, fakeResolve); err == nil || err.Error() != "capture_tun_prefix_collision" {
		t.Fatal("bare management IP exempted", err)
	}
	observed = gatewayObservation()
	observed.LANPrefixes = []string{"172.16.0.0/12"}
	if _, _, err := BuildFromAccepted(context.Background(), gatewayDesired("172.20.0.0/16"), gatewayNative(), observed, fakeResolve); err == nil || err.Error() != "capture_tun_prefix_collision" {
		t.Fatal("whole observed prefix overlap exempted", err)
	}
}

func TestGatewayMissingSavedLANWithdrawsInsteadOfBroadening(t *testing.T) {
	c := testController(t, tunIdleRunner)
	observed := gatewayObservation()
	c.SetBuilder(func(ctx context.Context, d Desired) (proxy.RulesPlanInput, []Client, error) {
		return BuildFromAccepted(ctx, d, gatewayNative(), observed, fakeResolve)
	})
	d := gatewayDesired("192.168.31.250/32")
	if _, err := c.Select(context.Background(), d); err != nil {
		t.Fatal(err)
	}
	observed.LANPrefixes = []string{"192.168.32.0/24"}
	state, err := c.Refresh(context.Background())
	if err == nil || state.Active || !state.Desired || state.ScopeState != "unresolved" || state.State != "suspended" || len(state.InstalledLANIPv4Prefixes) != 0 || !reflect.DeepEqual(state.LANIPv4Prefixes, d.LANIPv4Prefixes) || len(state.Clients) != 0 {
		t.Fatalf("missing declaration did not suspend: %+v %v", state, err)
	}
	if _, err := os.Stat(c.path); !os.IsNotExist(err) {
		t.Fatal("withdrawal retained cleaned journal", err)
	}
	observed = gatewayObservation()
	state, err = c.Restore(context.Background())
	if err != nil || !state.Active || !reflect.DeepEqual(state.InstalledLANIPv4Prefixes, d.LANIPv4Prefixes) {
		t.Fatalf("restore broadened canary: %+v %v", state, err)
	}
}

func TestGatewayJournalRecompilesPrefixesNeverStoredApplyAndKeepsOldCleanup(t *testing.T) {
	input := gatewayInput(t, gatewayDesired("192.168.31.250/32"))
	plan, err := proxy.PlanOwnedRules(input)
	if err != nil {
		t.Fatal(err)
	}
	for _, includeInput := range []bool{true, false} {
		stored := journal{OwnedRulesPlan: clonePlan(plan)}
		stored.Apply = [][]string{{"must-never-run", "foreign"}}
		if includeInput {
			stored.Input = &input
		}
		recovered, err := recoveredPlan(stored)
		if err != nil || !reflect.DeepEqual(recovered.Cleanup, plan.Cleanup) || !reflect.DeepEqual(recovered.Ownership, plan.Ownership) || len(recovered.Apply) == 1 && recovered.Apply[0][0] == "must-never-run" {
			t.Fatalf("gateway journal recovery (input=%v): %+v %v", includeInput, recovered, err)
		}
		for _, mutate := range []func(*journal){
			func(j *journal) { j.Ownership.LANIPv4Prefixes[0] = "192.168.31.0/24" },
			func(j *journal) { j.Cleanup[0] = append(j.Cleanup[0], "foreign") },
			func(j *journal) { j.Ownership.Scope = "" },
		} {
			bad := journal{OwnedRulesPlan: clonePlan(plan)}
			if includeInput {
				copy := cloneInput(input)
				bad.Input = &copy
			}
			mutate(&bad)
			if _, err := recoveredPlan(bad); err == nil {
				t.Fatalf("foreign gateway scope/cleanup accepted (input=%v): %+v", includeInput, bad)
			}
		}
	}
	// Default empty scope remains exact-device old-journal meaning.
	old := testPlan(t)
	old.Apply = [][]string{{"must-never-run", "old"}}
	recovered, err := recoveredPlan(journal{OwnedRulesPlan: old})
	if err != nil || recovered.Ownership.Scope != "" || len(recovered.Ownership.LANIPv4Prefixes) != 0 || !reflect.DeepEqual(recovered.Cleanup, old.Cleanup) {
		t.Fatalf("old device journal cleanup migrated: %+v %v", recovered, err)
	}
	raw, err := json.Marshal(journal{OwnedRulesPlan: old})
	if err != nil || strings.Contains(string(raw), "LANIPv4Prefixes") || strings.Contains(string(raw), `"Scope"`) {
		t.Fatalf("old metadata defaults not omitted: %s %v", raw, err)
	}
}

func TestGatewayPolicyProofRequiresEveryExactPrefixAndRejectsForeignScope(t *testing.T) {
	input := gatewayInput(t, gatewayDesired())
	input.LANIPv4Prefixes = []string{"192.168.31.0/25", "192.168.31.128/25"}
	plan, err := proxy.PlanOwnedRules(input)
	if err != nil {
		t.Fatal(err)
	}
	good := gatewayRules(plan.Ownership.LANIPv4Prefixes)
	for _, tc := range []struct {
		name, rules string
		want        bool
	}{
		{"all", good, true},
		{"alias", strings.ReplaceAll(good, "lookup 16500", "table capture"), true},
		{"factory-unrelated", "0: from all lookup local\n" + good + "32766: from all lookup main\n", true},
		{"missing", gatewayRules(plan.Ownership.LANIPv4Prefixes[:1]), false},
		{"broader", gatewayRules([]string{"192.168.31.0/24"}), false},
		{"extra", good + gatewayRules([]string{"192.168.32.0/24"}), false},
		{"duplicate", good + gatewayRules(plan.Ownership.LANIPv4Prefixes[:1]), false},
		{"noncanonical", strings.Replace(good, "192.168.31.0/25", "192.168.31.1/25", 1), false},
		{"wrong-iif", strings.Replace(good, "iif br-lan", "iif br-guest", 1), false},
		{"wrong-mask", strings.Replace(good, "0x4000/0x4000", "0x4000", 1), false},
		{"wrong-table", strings.Replace(good, "lookup 16500", "lookup main", 1), false},
		{"selector", strings.Replace(good, " iif ", " to 1.1.1.1 iif ", 1), false},
		{"foreign-reserved-table", good + "16501: from all lookup 16500\n", false},
		{"foreign-reserved-mark", good + "16501: from all fwmark 0x4000/0x4000 lookup main\n", false},
		{"foreign-priority", good + "16500: from all lookup main\n", false},
	} {
		t.Run(tc.name, func(t *testing.T) {
			if got := hasOwnedRule([]byte(tc.rules), plan.Ownership, 4, map[string]int{"capture": proxy.CaptureTable, "local": 255, "main": 254}); got != tc.want {
				t.Fatalf("gateway policy proof=%v want=%v: %q", got, tc.want, tc.rules)
			}
		})
	}
	canary := gatewayPlan(t, gatewayDesired("192.168.31.250/32"))
	if !hasOwnedRule([]byte(gatewayRules([]string{"192.168.31.250"})), canary.Ownership, 4, nil) || hasOwnedRule([]byte(good), plan.Ownership, 6, nil) {
		t.Fatal("host spelling or initial gateway family proof incorrect")
	}
}

func TestGatewayObservedStatusKeepsDeclaredAndInstalledPrefixTruth(t *testing.T) {
	d := gatewayDesired()
	plan := gatewayPlan(t, d)
	for _, tc := range []struct {
		name, route, rules string
		wantActive         bool
	}{
		{"ordinary-tun", "default dev b6p-tun", gatewayRules(d.LANIPv4Prefixes), true},
		{"plain-local-not-tun", "local default dev lo scope host", gatewayRules(d.LANIPv4Prefixes), false},
		{"foreign-prefix", "default dev b6p-tun", gatewayRules([]string{"192.168.32.0/24"}), false},
		{"broader-prefix", "default dev b6p-tun", gatewayRules([]string{"192.168.0.0/16"}), false},
	} {
		t.Run(tc.name, func(t *testing.T) {
			c := testController(t, gatewayObservedRunner(t, plan, tc.route, tc.rules))
			c.desired, c.plan = cloneDesired(d), &plan
			c.SetBuilder(func(ctx context.Context, d Desired) (proxy.RulesPlanInput, []Client, error) {
				return BuildFromAccepted(ctx, d, gatewayNative(), gatewayObservation(), fakeResolve)
			})
			state, err := c.ReconcileDesired(context.Background())
			if state.Active != tc.wantActive || (err == nil) != tc.wantActive || state.CleanupPending == tc.wantActive || state.Scope != proxy.CaptureScopeGateway || state.ScopeState != "current" || !reflect.DeepEqual(state.LANIPv4Prefixes, d.LANIPv4Prefixes) || !reflect.DeepEqual(state.InstalledLANIPv4Prefixes, d.LANIPv4Prefixes) || len(state.Clients) != 0 || len(state.InstalledClients) != 0 {
				t.Fatalf("status scope truth: %+v %v", state, err)
			}
		})
	}
	c := testController(t, gatewayObservedRunner(t, plan, "default dev b6p-tun", gatewayRules(d.LANIPv4Prefixes)))
	c.desired, c.plan = gatewayDesired("192.168.31.250/32"), &plan
	c.SetBuilder(func(ctx context.Context, d Desired) (proxy.RulesPlanInput, []Client, error) {
		return BuildFromAccepted(ctx, d, gatewayNative(), gatewayObservation(), fakeResolve)
	})
	state, err := c.ReconcileDesired(context.Background())
	if err == nil || !state.Active || state.ScopeState != "changed" || state.State != "scope-changed" || !reflect.DeepEqual(state.LANIPv4Prefixes, []string{"192.168.31.250/32"}) || !reflect.DeepEqual(state.InstalledLANIPv4Prefixes, d.LANIPv4Prefixes) {
		t.Fatalf("desired/installed drift collapsed: %+v %v", state, err)
	}
	c.SetBuilder(func(ctx context.Context, d Desired) (proxy.RulesPlanInput, []Client, error) {
		observed := gatewayObservation()
		observed.LANPrefixes = nil
		return BuildFromAccepted(ctx, d, gatewayNative(), observed, fakeResolve)
	})
	state, err = c.ReconcileDesired(context.Background())
	if err == nil || !state.Active || state.ScopeState != "unresolved" || !reflect.DeepEqual(state.InstalledLANIPv4Prefixes, d.LANIPv4Prefixes) {
		t.Fatalf("missing fresh topology erased installed truth: %+v %v", state, err)
	}
}

func TestGatewayCloneDesiredInputPlanStatusAndSnapshotOwnPrefixArrays(t *testing.T) {
	d := gatewayDesired()
	input := gatewayInput(t, d)
	plan := gatewayPlan(t, d)
	dcopy, icopy, pcopy := cloneDesired(d), cloneInput(input), clonePlan(plan)
	dcopy.LANIPv4Prefixes[0], icopy.LANIPv4Prefixes[0], pcopy.Ownership.LANIPv4Prefixes[0] = "10.0.0.0/8", "10.0.0.0/8", "10.0.0.0/8"
	if d.LANIPv4Prefixes[0] != "192.168.31.0/24" || input.LANIPv4Prefixes[0] != "192.168.31.0/24" || plan.Ownership.LANIPv4Prefixes[0] != "192.168.31.0/24" {
		t.Fatal("clone aliases approved declaration")
	}
	c := testController(t, tunIdleRunner)
	c.desired, c.plan = cloneDesired(d), &plan
	state, saved := c.Status(), c.Desired()
	state.LANIPv4Prefixes[0], state.InstalledLANIPv4Prefixes[0], saved.LANIPv4Prefixes[0] = "10.0.0.0/8", "10.0.0.0/8", "10.0.0.0/8"
	c.mu.Lock()
	snapshot := c.observationSnapshotLocked()
	if !c.observationMatchesLocked(snapshot) {
		t.Fatal("fresh snapshot mismatch")
	}
	snapshot.desired.LANIPv4Prefixes[0] = "10.0.0.0/8"
	if c.observationMatchesLocked(snapshot) {
		t.Fatal("prefix mutation escaped snapshot identity")
	}
	snapshot.inspector.plan.Ownership.LANIPv4Prefixes[0] = "10.0.0.0/8"
	c.mu.Unlock()
	if !reflect.DeepEqual(c.Desired(), d) || c.Status().InstalledLANIPv4Prefixes[0] != "192.168.31.0/24" {
		t.Fatal("status, desired or snapshot aliases controller")
	}
}

func TestGatewayBuilderPrefixMutationCannotChangeSavedSelection(t *testing.T) {
	c := testController(t, tunIdleRunner)
	c.SetBuilder(func(_ context.Context, d Desired) (proxy.RulesPlanInput, []Client, error) {
		d.LANIPv4Prefixes[0] = "10.0.0.0/8"
		return proxy.RulesPlanInput{}, []Client{{IP: "invented-device"}}, errors.New("capture_lan_unavailable")
	})
	d := gatewayDesired()
	state, err := c.Select(context.Background(), d)
	if err == nil || !reflect.DeepEqual(c.Desired(), d) || len(state.Clients) != 0 || !reflect.DeepEqual(state.LANIPv4Prefixes, d.LANIPv4Prefixes) {
		t.Fatalf("callback changed explicit declaration or invented clients: %+v %v", state, err)
	}
}

func TestGatewayStaleGETCannotPublishPrefixesAfterDisable(t *testing.T) {
	c := testController(t, tunIdleRunner)
	c.SetBuilder(func(ctx context.Context, d Desired) (proxy.RulesPlanInput, []Client, error) {
		return BuildFromAccepted(ctx, d, gatewayNative(), gatewayObservation(), fakeResolve)
	})
	if _, err := c.Select(context.Background(), gatewayDesired()); err != nil {
		t.Fatal(err)
	}
	plan := clonePlan(*c.plan)
	healthy := gatewayObservedRunner(t, plan, "default dev b6p-tun", gatewayRules(plan.Ownership.LANIPv4Prefixes))
	c.runner = func(ctx context.Context, args []string) ([]byte, error) {
		if ctx.Value(desiredObservationContextKey{}) != nil {
			return healthy(ctx, args)
		}
		return tunIdleRunner(ctx, args)
	}
	entered, release := make(chan struct{}), make(chan struct{})
	var once sync.Once
	defer once.Do(func() { close(release) })
	c.SetBuilder(func(ctx context.Context, d Desired) (proxy.RulesPlanInput, []Client, error) {
		close(entered)
		select {
		case <-release:
		case <-ctx.Done():
			return proxy.RulesPlanInput{}, nil, ctx.Err()
		}
		return BuildFromAccepted(ctx, d, gatewayNative(), gatewayObservation(), fakeResolve)
	})
	result := startDesiredObservation(c, true)
	waitDesiredObservation(t, entered)
	disabled := make(chan error, 1)
	go func() { disabled <- c.Disable(context.Background()) }()
	select {
	case err := <-disabled:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(500 * time.Millisecond):
		t.Fatal("Disable waited for gateway GET builder")
	}
	want := c.Status()
	once.Do(func() { close(release) })
	got := receiveDesiredObservation(t, result)
	if got.err != nil || !reflect.DeepEqual(got.state, want) || got.state.Active || got.state.Desired || len(got.state.InstalledLANIPv4Prefixes) != 0 || !reflect.DeepEqual(got.state.LANIPv4Prefixes, []string{"192.168.31.0/24"}) {
		t.Fatalf("stale GET changed off gateway: %+v %v", got.state, got.err)
	}
}

func TestGatewayDiagnosticsReportsPrefixScopeAndDropsChangedEvidence(t *testing.T) {
	plan := gatewayPlan(t, gatewayDesired())
	for _, changed := range []bool{false, true} {
		t.Run(map[bool]string{false: "prefix-proof", true: "scope-changed"}[changed], func(t *testing.T) {
			var c *Controller
			calls := 0
			c = testController(t, func(_ context.Context, argv []string) ([]byte, error) {
				calls++
				if !diagnosticReadAllowed(argv) {
					t.Fatalf("diagnostics wrote: %v", argv)
				}
				if changed && calls == 1 {
					c.mu.Lock()
					copy := clonePlan(*c.plan)
					copy.Ownership.LANIPv4Prefixes = []string{"192.168.31.250/32"}
					c.plan = &copy
					c.mu.Unlock()
				}
				if slices.Equal(argv, routeShow(4)) {
					return []byte("default dev b6p-tun"), nil
				}
				if slices.Equal(argv, ruleShow(4)) {
					return []byte(gatewayRules(plan.Ownership.LANIPv4Prefixes)), nil
				}
				return diagnosticFixture(argv[6]), nil
			})
			copy := clonePlan(plan)
			c.plan = &copy
			report := c.Diagnostics(context.Background())
			if changed {
				if report.State != "scope-changed" || report.Scope != "" || len(report.InstalledLANIPv4Prefixes) != 0 || len(report.InstalledClients) != 0 || len(report.Chains) != 0 || len(report.Routing) != 0 {
					t.Fatalf("stale prefix evidence retained: %+v", report)
				}
				return
			}
			if report.State != "complete" || report.Scope != proxy.CaptureScopeGateway || !reflect.DeepEqual(report.InstalledLANIPv4Prefixes, plan.Ownership.LANIPv4Prefixes) || len(report.InstalledClients) != 0 || len(report.Routing) != 1 || report.Routing[0].PolicyRules.Present == nil || !*report.Routing[0].PolicyRules.Present {
				t.Fatalf("gateway diagnostics missing prefix scope: %+v", report)
			}
			report.InstalledLANIPv4Prefixes[0] = "10.0.0.0/8"
			if c.Status().InstalledLANIPv4Prefixes[0] != "192.168.31.0/24" {
				t.Fatal("diagnostic output aliases installed plan")
			}
		})
	}
}

func TestGatewayPreflightUsesExistingMainTUNAndExactOwnedResources(t *testing.T) {
	plan := gatewayPlan(t, gatewayDesired("192.168.31.250/32"))
	for _, tc := range []struct{ name, route, rules string }{
		{name: "clean"},
		{name: "occupied-table", route: "local default dev lo scope host"},
		{name: "foreign-prefix-rule", rules: gatewayRules([]string{"192.168.32.0/24"})},
	} {
		t.Run(tc.name, func(t *testing.T) {
			calls := [][]string{}
			c := testController(t, func(ctx context.Context, argv []string) ([]byte, error) {
				if !isReadCommand(argv) {
					t.Fatalf("preflight mutated kernel: %v", argv)
				}
				calls = append(calls, slices.Clone(argv))
				if slices.Equal(argv, routeShow(4)) {
					return []byte(tc.route), nil
				}
				if slices.Equal(argv, ruleShow(4)) && tc.rules != "" {
					return []byte(tc.rules), nil
				}
				return tunIdleRunner(ctx, argv)
			})
			err := c.preflight(context.Background(), plan)
			if (err == nil) != (tc.name == "clean") {
				t.Fatalf("preflight %s: %v", tc.name, err)
			}
			if !slices.Equal(calls[0], tunAddressShow("b6p-tun")) || !slices.Equal(calls[1], tunRPFilterShow("b6p-tun")) {
				t.Fatal("gateway preflight omitted actual main TUN reads", calls)
			}
		})
	}
}

func TestGatewayObservationChecksEveryPrefixHookAndOrderedSharedChain(t *testing.T) {
	plan := gatewayPlan(t, gatewayDesired("192.168.31.250/32"))
	for _, missing := range []string{"", "B6P_V4_TUN_MARK", "B6P_V4_DNS", "B6P_V4_TUN_RETURN"} {
		t.Run(map[string]string{"": "complete", "B6P_V4_TUN_MARK": "mark", "B6P_V4_DNS": "dns", "B6P_V4_TUN_RETURN": "return"}[missing], func(t *testing.T) {
			base := gatewayObservedRunner(t, plan, "default dev b6p-tun", gatewayRules(plan.Ownership.LANIPv4Prefixes))
			checked := map[string]bool{}
			c := testController(t, func(ctx context.Context, argv []string) ([]byte, error) {
				if len(argv) > 6 && argv[5] == "-C" {
					checked[strings.Join(argv, " ")] = true
					if argv[6] == "PREROUTING" && slices.Contains(argv, missing) || argv[6] == "FORWARD" && slices.Contains(argv, missing) {
						return []byte("iptables: Bad rule (does a matching rule exist in that chain?)."), errors.New("missing fake exact hook")
					}
				}
				return base(ctx, argv)
			})
			copy := clonePlan(plan)
			c.plan = &copy
			state, err := c.Reconcile(context.Background())
			if (err == nil) != (missing == "") || state.Active != (missing == "") || state.CleanupPending != (missing != "") {
				t.Fatalf("missing prefix hook overlooked: %+v %v", state, err)
			}
			for _, command := range plan.Apply {
				if len(command) <= 6 || command[5] != "-A" && command[5] != "-I" {
					continue
				}
				check := slices.Clone(command)
				check[5] = "-C"
				if command[5] == "-I" {
					check = append(check[:7], check[8:]...)
				}
				if !checked[strings.Join(check, " ")] {
					t.Fatalf("gateway proof omitted exact prefix rule/hook: %v", check)
				}
			}
		})
	}
}

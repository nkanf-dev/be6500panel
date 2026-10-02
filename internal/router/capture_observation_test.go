package router

import (
	"context"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"time"
)

const captureARPHeader = "IP address HW type Flags HW address Mask Device\n"
const captureRouteHeader = "Iface Destination Gateway Flags RefCnt Use Metric Mask MTU Window IRTT\n"

func captureFixture(t *testing.T, leases, arp string) *Adapter {
	t.Helper()
	root := t.TempDir()
	fixtureFile(t, root, "/tmp/dhcp.leases", leases)
	fixtureFile(t, root, "/proc/net/arp", captureARPHeader+arp)
	fixtureFile(t, root, "/proc/net/route", captureRouteHeader+"br-lan 000200C0 00000000 0001 0 0 0 00FFFFFF 0 0 0\neth0.2 00000000 016433C6 0003 0 0 3 00000000 0 0 0\n")
	fixtureFile(t, root, "/etc/config/network", "config interface 'lan'\n option device 'br-lan'\n option ipaddr '192.0.2.1'\n option netmask '255.255.255.0'\nconfig interface 'wan'\n option device 'eth0.2'\n option ipaddr '198.51.100.2'\n option netmask '255.255.255.0'\n")
	a := New(root)
	a.now = func() time.Time { return time.Unix(2000000000, 0) }
	return a
}

func TestCaptureObservationLANOnlyAndManagement(t *testing.T) {
	a := captureFixture(t, "0 02:00:00:00:00:10 192.0.2.10 test-pc *\n", "192.0.2.10 0x1 0x2 02:00:00:00:00:10 * br-lan\n192.0.2.20 0x1 0x2 02:00:00:00:00:20 * br-lan\n198.51.100.1 0x1 0x2 02:00:00:00:00:01 * eth0.2\n192.0.2.30 0x1 0x2 02:00:00:00:00:30 * eth0.2\n203.0.113.40 0x1 0x2 02:00:00:00:00:40 * br-lan\n192.0.2.1 0x1 0x2 02:00:00:00:00:11 * br-lan\n")
	got, err := a.CaptureObservation(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(got.LANPrefixes, []string{"192.0.2.0/24"}) || !reflect.DeepEqual(got.ManagementIPs, []string{"192.0.2.1", "198.51.100.2"}) {
		t.Fatalf("unexpected scope: %+v", got)
	}
	eligible := []string{}
	for _, d := range got.Devices {
		if d.Eligible {
			eligible = append(eligible, d.IP)
		}
		if d.IP == "198.51.100.1" && (d.Eligible || d.Interface != "eth0.2") {
			t.Fatalf("upstream ARP lost interface or became eligible: %+v", d)
		}
		if d.IP == "192.0.2.10" && (!d.Lease || d.ExpiresAt != nil || d.Interface != "br-lan") {
			t.Fatalf("infinite lease provenance/interface lost: %+v", d)
		}
	}
	if !reflect.DeepEqual(eligible, []string{"192.0.2.10", "192.0.2.20"}) {
		t.Fatalf("non-LAN or router identity accepted: %+v", got.Devices)
	}
	data, err := json.Marshal(got)
	if err != nil || strings.Contains(string(data), `"Lease"`) || strings.Contains(string(data), `"lease"`) || !strings.Contains(string(data), `"eligible":true`) {
		t.Fatalf("wrong public identity fields: %s err=%v", data, err)
	}
}

func TestCaptureObservationLeaseOverridesStaleARP(t *testing.T) {
	a := captureFixture(t, "2000000600 02:00:00:00:00:10 192.0.2.10 current *\n", "192.0.2.9 0x1 0x2 02:00:00:00:00:10 * br-lan\n192.0.2.10 0x1 0x2 02:00:00:00:00:10 * br-lan\n")
	got, err := a.CaptureObservation(context.Background())
	if err != nil || len(got.Devices) != 1 || got.Devices[0].IP != "192.0.2.10" || !got.Devices[0].Eligible || !got.Devices[0].Lease || !got.Devices[0].Online {
		t.Fatalf("current lease did not beat stale ARP: %+v err=%v", got, err)
	}
}

func TestCaptureObservationConflictingIdentities(t *testing.T) {
	for _, test := range []struct {
		name, leases, arp string
	}{
		{"lease-lease-owner", "0 02:00:00:00:00:10 192.0.2.10 one *\n0 02:00:00:00:00:20 192.0.2.10 two *\n", ""},
		{"lease-arp-owner", "0 02:00:00:00:00:10 192.0.2.10 one *\n", "192.0.2.10 0x1 0x2 02:00:00:00:00:20 * br-lan\n"},
		{"multiple-current-leases", "0 02:00:00:00:00:10 192.0.2.10 one *\n0 02:00:00:00:00:10 192.0.2.20 two *\n", ""},
		{"multiple-arp-addresses", "", "192.0.2.10 0x1 0x2 02:00:00:00:00:10 * br-lan\n192.0.2.20 0x1 0x2 02:00:00:00:00:10 * br-lan\n"},
		{"arp-owner", "", "192.0.2.10 0x1 0x2 02:00:00:00:00:10 * br-lan\n192.0.2.10 0x1 0x2 02:00:00:00:00:20 * br-lan\n"},
	} {
		t.Run(test.name, func(t *testing.T) {
			a := captureFixture(t, test.leases, test.arp)
			got, err := a.CaptureObservation(context.Background())
			if err != nil || len(got.Devices) == 0 {
				t.Fatalf("could not observe conflict: %+v err=%v", got, err)
			}
			for _, d := range got.Devices {
				if d.Eligible {
					t.Fatalf("ambiguous identity accepted: %+v", got.Devices)
				}
			}
		})
	}
}

func TestCaptureObservationMissingScopeNeverWidens(t *testing.T) {
	a := captureFixture(t, "0 02:00:00:00:00:10 192.0.2.10 test-pc *\n", "")
	fixtureFile(t, a.root, "/proc/net/route", captureRouteHeader+"br-lan 00000000 00000000 0001 0 0 0 00000000 0 0 0\n")
	fixtureFile(t, a.root, "/etc/config/network", "config interface 'wan'\n option device 'eth0.2'\n option ipaddr '198.51.100.2'\n option netmask '255.255.255.0'\n")
	got, err := a.CaptureObservation(context.Background())
	if err == nil || len(got.LANPrefixes) != 0 {
		t.Fatalf("missing br-lan scope accepted: %+v err=%v", got, err)
	}
	for _, d := range got.Devices {
		if d.Eligible {
			t.Fatalf("missing scope widened to device address: %+v", d)
		}
	}
}

func TestCaptureObservationRejectsMissingOrInvalidSources(t *testing.T) {
	for _, test := range []struct {
		name, path, contents string
		remove               bool
	}{
		{"missing-leases", "/tmp/dhcp.leases", "", true},
		{"invalid-leases", "/tmp/dhcp.leases", "bad PRIVATE-DATA\n", false},
		{"missing-arp", "/proc/net/arp", "", true},
		{"invalid-arp", "/proc/net/arp", "bad PRIVATE-DATA\n", false},
		{"missing-routes", "/proc/net/route", "", true},
		{"invalid-routes", "/proc/net/route", "bad PRIVATE-DATA\n", false},
		{"missing-interface-addresses", "/etc/config/network", "", true},
		{"invalid-interface-addresses", "/etc/config/network", "config interface 'lan'\n option ipaddr 'PRIVATE-DATA'\n option netmask '255.255.255.0'\n", false},
	} {
		t.Run(test.name, func(t *testing.T) {
			a := captureFixture(t, "0 02:00:00:00:00:10 192.0.2.10 test-pc *\n", "192.0.2.20 0x1 0x2 02:00:00:00:00:20 * br-lan\n")
			if test.remove {
				if err := os.Remove(filepath.Join(a.root, strings.TrimPrefix(test.path, "/"))); err != nil {
					t.Fatal(err)
				}
			} else {
				fixtureFile(t, a.root, test.path, test.contents)
			}
			got, err := a.CaptureObservation(context.Background())
			if err == nil || strings.Contains(err.Error(), "PRIVATE-DATA") {
				t.Fatalf("invalid identity source not rejected safely: %+v err=%v", got, err)
			}
			for _, d := range got.Devices {
				if d.Eligible {
					t.Fatalf("partial identity source accepted for capture: %+v", d)
				}
			}
		})
	}
}

func TestCaptureObservationUncachedAndIndependentOfSnapshotSources(t *testing.T) {
	a := captureFixture(t, "0 02:00:00:00:00:10 192.0.2.10 test-pc *\n", "")
	// Snapshot sees missing platform/WiFi/firewall/traffic data. Capture must not.
	_, _ = a.Snapshot(context.Background())
	first, err := a.CaptureObservation(context.Background())
	if err != nil || len(first.Devices) != 1 || !first.Devices[0].Eligible {
		t.Fatalf("%+v err=%v", first, err)
	}
	fixtureFile(t, a.root, "/tmp/dhcp.leases", "0 02:00:00:00:00:10 192.0.2.11 changed *\n")
	second, err := a.CaptureObservation(context.Background())
	if err != nil || len(second.Devices) != 1 || second.Devices[0].IP != "192.0.2.11" || !second.Devices[0].Eligible {
		t.Fatalf("capture used cached devices: %+v err=%v", second, err)
	}
	first.Devices[0].IP = "tampered"
	first.LANPrefixes[0] = "0.0.0.0/0"
	third, err := a.CaptureObservation(context.Background())
	if err != nil || third.Devices[0].IP != "192.0.2.11" || third.LANPrefixes[0] != "192.0.2.0/24" {
		t.Fatalf("capture copy leaked: %+v err=%v", third, err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	got, err := a.CaptureObservation(ctx)
	if !errors.Is(err, context.Canceled) || got.Devices == nil || got.LANPrefixes == nil || got.ManagementIPs == nil {
		t.Fatalf("cancel: %+v err=%v", got, err)
	}
}

func TestSnapshotAnnotatesCaptureEligibility(t *testing.T) {
	a := captureFixture(t, "0 02:00:00:00:00:10 192.0.2.10 test-pc *\n", "198.51.100.1 0x1 0x2 02:00:00:00:00:01 * eth0.2\n")
	got, err := a.Snapshot(context.Background())
	if err != nil || len(got.Devices) != 2 {
		t.Fatalf("%+v err=%v", got, err)
	}
	for _, d := range got.Devices {
		if d.Eligible != (d.IP == "192.0.2.10") {
			t.Fatalf("snapshot eligibility differs: %+v", got.Devices)
		}
	}
}

func TestCaptureObservationLANAddressesIncludeIPv6(t *testing.T) {
	a := captureFixture(t, "0 02:00:00:00:00:10 192.0.2.10 test-pc *\n", "")
	fixtureFile(t, a.root, "/etc/config/network", "config interface 'lan'\n option device 'br-lan'\n list ipaddr '192.0.2.1/24'\n list ipaddr '203.0.113.1/24'\n list ip6addr '2001:db8:1::1/64'\n list ip6addr 'fe80::1/64'\nconfig interface 'wan'\n option device 'eth0.2'\n option ipaddr '198.51.100.2'\n option netmask '255.255.255.0'\n option ip6addr '2001:db8:2::1/64'\nconfig interface 'loopback'\n option ifname 'lo'\n option ipaddr '127.0.0.1'\n option netmask '255.0.0.0'\n")
	got, err := a.CaptureObservation(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(got.LANPrefixes, []string{"192.0.2.0/24", "203.0.113.0/24"}) {
		t.Fatalf("missing br-lan prefix: %+v", got)
	}
	if !reflect.DeepEqual(got.LANAddresses, []string{"192.0.2.1", "2001:db8:1::1", "203.0.113.1", "fe80::1"}) {
		t.Fatalf("LAN address subset includes upstream/loopback or loses IPv6: %+v", got)
	}
	if !reflect.DeepEqual(got.ManagementIPs, []string{"127.0.0.1", "192.0.2.1", "198.51.100.2", "2001:db8:1::1", "2001:db8:2::1", "203.0.113.1", "fe80::1"}) {
		t.Fatalf("management interface identities incomplete: %+v", got)
	}
}

func TestCaptureObservationConnectedRouteProvidesScopeWithoutMaskGuess(t *testing.T) {
	a := captureFixture(t, "0 02:00:00:00:00:10 192.0.2.10 test-pc *\n", "")
	fixtureFile(t, a.root, "/etc/config/network", "config interface 'lan'\n option ifname 'eth0.1'\n option ipaddr '192.0.2.1'\n")
	got, err := a.CaptureObservation(context.Background())
	if err != nil || !reflect.DeepEqual(got.LANPrefixes, []string{"192.0.2.0/24"}) || !got.Devices[0].Eligible {
		t.Fatalf("direct br-lan route not used: %+v err=%v", got, err)
	}
	fixtureFile(t, a.root, "/proc/net/route", captureRouteHeader+"eth0.2 000200C0 00000000 0001 0 0 0 00FFFFFF 0 0 0\nbr-lan 000200C0 010200C0 0003 0 0 0 00FFFFFF 0 0 0\n")
	got, err = a.CaptureObservation(context.Background())
	if err == nil || len(got.LANPrefixes) != 0 || got.Devices[0].Eligible {
		t.Fatalf("guessed mask, WAN route, or gateway br-lan route accepted: %+v err=%v", got, err)
	}
}

func TestCaptureObservationLeaseExpiryAndInfiniteProvenance(t *testing.T) {
	a := captureFixture(t, "2000000001 02:00:00:00:00:10 192.0.2.10 current *\n0 02:00:00:00:00:20 192.0.2.20 infinite *\n", "192.0.2.9 0x1 0x2 02:00:00:00:00:10 * br-lan\n")
	got, err := a.CaptureObservation(context.Background())
	if err != nil || len(got.Devices) != 2 || got.Devices[0].IP != "192.0.2.10" || !got.Devices[0].Eligible || !got.Devices[1].Lease || got.Devices[1].ExpiresAt != nil {
		t.Fatalf("%+v err=%v", got, err)
	}
	a.now = func() time.Time { return time.Unix(2000000001, 0) }
	got, err = a.CaptureObservation(context.Background())
	if err != nil || len(got.Devices) != 2 || got.Devices[0].IP != "192.0.2.9" || got.Devices[0].Lease || !got.Devices[0].Eligible || !got.Devices[1].Lease {
		t.Fatalf("expired lease still authoritative or infinite lease lost: %+v err=%v", got, err)
	}
}

func TestCaptureObservationRejectsNetworkBroadcastAndLeaseOnWAN(t *testing.T) {
	a := captureFixture(t, "0 02:00:00:00:00:10 192.0.2.10 wan-lease *\n0 02:00:00:00:00:20 192.0.2.0 network *\n0 02:00:00:00:00:30 192.0.2.255 broadcast *\n", "192.0.2.10 0x1 0x2 02:00:00:00:00:10 * eth0.2\n")
	got, err := a.CaptureObservation(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	for _, d := range got.Devices {
		if d.Eligible {
			t.Fatalf("ineligible source accepted: %+v", d)
		}
	}
}

func TestCaptureObservationConflictingInterfaces(t *testing.T) {
	a := captureFixture(t, "0 02:00:00:00:00:10 192.0.2.10 test-pc *\n", "192.0.2.10 0x1 0x2 02:00:00:00:00:10 * br-lan\n192.0.2.10 0x1 0x2 02:00:00:00:00:10 * eth0.2\n")
	got, err := a.CaptureObservation(context.Background())
	if err != nil || len(got.Devices) != 1 || got.Devices[0].Eligible {
		t.Fatalf("conflicting interface identity accepted: %+v err=%v", got, err)
	}
}

func TestCaptureObservationExplicitNonLANFixtureDeviceStaysNonLAN(t *testing.T) {
	a := captureFixture(t, "0 02:00:00:00:00:10 192.0.2.10 test-pc *\n", "")
	fixtureFile(t, a.root, "/etc/config/network", "config interface 'lan'\n option device 'br-guest'\n option ipaddr '192.0.2.1'\n option netmask '255.255.255.0'\n")
	fixtureFile(t, a.root, "/proc/net/route", captureRouteHeader)
	got, err := a.CaptureObservation(context.Background())
	if err == nil || len(got.LANPrefixes) != 0 || len(got.LANAddresses) != 0 || got.Devices[0].Eligible {
		t.Fatalf("explicit non-br-lan fixture scope widened: %+v err=%v", got, err)
	}
}

func TestCaptureObservationConcurrentSnapshotCopies(t *testing.T) {
	a := captureFixture(t, "0 02:00:00:00:00:10 192.0.2.10 test-pc *\n", "192.0.2.10 0x1 0x2 02:00:00:00:00:10 * br-lan\n")
	results := make(chan error, 20)
	for i := 0; i < 20; i++ {
		go func() {
			got, err := a.CaptureObservation(context.Background())
			if err == nil {
				if len(got.Devices) != 1 || !got.Devices[0].Eligible || !got.Devices[0].Lease || len(got.LANAddresses) != 1 {
					err = errors.New("concurrent observation lost identity")
				} else {
					got.Devices[0].Eligible = false
					got.LANAddresses[0] = "tampered"
				}
			}
			_, _ = a.Snapshot(context.Background())
			results <- err
		}()
	}
	for i := 0; i < 20; i++ {
		if err := <-results; err != nil {
			t.Fatal(err)
		}
	}
	got, err := a.CaptureObservation(context.Background())
	if err != nil || !got.Devices[0].Eligible || got.LANAddresses[0] != "192.0.2.1" {
		t.Fatalf("%+v err=%v", got, err)
	}
}

package capture

import (
	"context"
	"reflect"
	"strings"
	"testing"

	"be6500panel/internal/proxy"
)

func TestAcceptedDualStackRouterAddressesDoNotInvalidateIPv4Capture(t *testing.T) {
	observed := deviceObservation()
	observed.LANAddresses = []string{"192.0.2.1", "fe80::1", "fd00:6969:6969::1", "2001:db8:31::1"}
	observed.ManagementIPs = append(observed.ManagementIPs, observed.LANAddresses[1:]...)
	input, _, err := BuildFromAccepted(context.Background(), desiredDevices(), []byte(acceptedNative), observed, fakeResolve)
	if err != nil {
		t.Fatalf("normal dual-stack LAN prevented IPv4 capture: %v", err)
	}
	if !reflect.DeepEqual(input.RouterDNSAddresses, []string{"192.0.2.1"}) {
		t.Fatalf("IPv4 capture contains non-IPv4 DNS exceptions: %v", input.RouterDNSAddresses)
	}
	if _, err = proxy.PlanOwnedRules(input); err != nil {
		t.Fatal(err)
	}
}

func TestIPv6FollowCapturesRoutableRouterDNSButNotLinkLocal(t *testing.T) {
	observed := deviceObservation()
	observed.LANAddresses = []string{"192.0.2.1", "fe80::1", "fd00:6969:6969::1", "2001:db8:31::1"}
	observed.ManagementIPs = append(observed.ManagementIPs, observed.LANAddresses[1:]...)
	desired := desiredDevices()
	desired.Devices = desired.Devices[:1]
	desired.IPv6 = proxy.IPv6Follow
	desired.ClientIPv6 = "2001:db8:31::10"
	native := strings.Replace(acceptedNative, `"listen":"127.0.0.1"`, `"listen":"::"`, 1)
	native = strings.Replace(native, `"listen":"192.0.2.1","listen_port":1054`, `"listen":"::","listen_port":1054`, 1)
	native = strings.Replace(native, `,{"ip_version":6,"outbound":"direct"}`, ``, 1)
	input, _, err := BuildFromAccepted(context.Background(), desired, []byte(native), observed, fakeResolve)
	if err != nil {
		t.Fatal(err)
	}
	want := []string{"192.0.2.1", "fd00:6969:6969::1", "2001:db8:31::1"}
	if !reflect.DeepEqual(input.RouterDNSAddresses, want) {
		t.Fatalf("unexpected DNS exceptions: %v", input.RouterDNSAddresses)
	}
	if _, err = proxy.PlanOwnedRules(input); err != nil {
		t.Fatal(err)
	}
}

func TestRouterDNSExceptionFilteringMatchesFamilyAndPreservesBypasses(t *testing.T) {
	observed := deviceObservation()
	observed.LANAddresses = []string{"192.168.31.1", "10.0.0.1", "fe80::1", "fe80::1%br-lan", "::1", "::", "ff02::1", "::ffff:192.0.2.1", "fd00::1", "2001:db8::1"}
	if got := captureRouterDNSAddresses(observed.LANAddresses, proxy.IPv6Direct); !reflect.DeepEqual(got, []string{"192.168.31.1", "10.0.0.1"}) {
		t.Fatalf("direct: %v", got)
	}
	if got := captureRouterDNSAddresses(observed.LANAddresses, proxy.IPv6Block); !reflect.DeepEqual(got, []string{"192.168.31.1", "10.0.0.1"}) {
		t.Fatalf("block: %v", got)
	}
	if got := captureRouterDNSAddresses(observed.LANAddresses, proxy.IPv6Follow); !reflect.DeepEqual(got, []string{"192.168.31.1", "10.0.0.1", "fd00::1", "2001:db8::1"}) {
		t.Fatalf("follow: %v", got)
	}
	observed = deviceObservation()
	observed.LANAddresses = append(observed.LANAddresses, "fe80::1")
	observed.ManagementIPs = append(observed.ManagementIPs, "fe80::1")
	original := append([]string{}, observed.ManagementIPs...)
	input, _, err := BuildFromAccepted(context.Background(), desiredDevices(), []byte(acceptedNative), observed, fakeResolve)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(input.ManagementIPs, original) || !reflect.DeepEqual(observed.ManagementIPs, original) {
		t.Fatal("management bypasses changed")
	}
}

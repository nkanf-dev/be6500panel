package httpapi

import (
	"be6500panel/internal/proxy"
	"be6500panel/internal/router"
	"reflect"
	"testing"
)

func TestGatewayDeclarationUsesFreshLANNotDeviceInventory(t *testing.T) {
	observed := router.CaptureObservation{LANPrefixes: []string{"192.168.31.0/24"}, LANAddresses: []string{"192.168.31.1"}, ManagementIPs: []string{"192.168.31.1"}}
	desired, err := gatewayDesiredFromObservation(observed)
	if err != nil || desired.Scope != proxy.CaptureScopeGateway || !desired.Enabled || len(desired.Devices) != 0 || !reflect.DeepEqual(desired.LANIPv4Prefixes, []string{"192.168.31.0/24"}) {
		t.Fatal(desired, err)
	}
	desired.LANIPv4Prefixes[0] = "10.0.0.0/24"
	if observed.LANPrefixes[0] != "192.168.31.0/24" {
		t.Fatal("observed range aliased")
	}
	for _, bad := range []router.CaptureObservation{{}, {LANPrefixes: []string{"0.0.0.0/0"}, LANAddresses: observed.LANAddresses, ManagementIPs: observed.ManagementIPs}, {LANPrefixes: []string{"192.168.31.1/24"}, LANAddresses: observed.LANAddresses, ManagementIPs: observed.ManagementIPs}} {
		if _, err := gatewayDesiredFromObservation(bad); err == nil {
			t.Fatal("unsafe fresh declaration", bad)
		}
	}
}

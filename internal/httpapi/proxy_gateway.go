package httpapi

import (
	"be6500panel/internal/capture"
	"be6500panel/internal/proxy"
	"be6500panel/internal/router"
	"errors"
)

// The browser requests a gateway action, never source ranges or device inventory.
// Fresh main-br-lan prefixes declare normal new-device routing coverage.
func gatewayDesiredFromObservation(observed router.CaptureObservation) (capture.Desired, error) {
	if len(observed.LANAddresses) == 0 || len(observed.ManagementIPs) == 0 {
		return capture.Desired{}, errors.New("capture_lan_unavailable")
	}
	prefixes, err := proxy.CanonicalGatewayPrefixes(observed.LANPrefixes)
	if err != nil {
		return capture.Desired{}, errors.New("capture_lan_unavailable")
	}
	return capture.Desired{Scope: proxy.CaptureScopeGateway, Enabled: true, LANIPv4Prefixes: prefixes, IPv6: proxy.IPv6Direct}, nil
}

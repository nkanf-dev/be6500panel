package capture_test

import (
	"context"
	"encoding/json"
	"reflect"
	"strings"
	"testing"

	"be6500panel/internal/capture"
	"be6500panel/internal/proxy"
	"be6500panel/internal/router"
)

func TestRoutedTUNCompilerAcceptedCapturePlannerIntegration(t *testing.T) {
	node := proxy.Node{ID: "synthetic-integration", Server: "203.0.113.44", Port: 443, UUID: "00000000-1111-4222-8333-444444444444", ServerName: "www.example.com", RealityPublicKey: "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE", RealityShortID: "aabbccdd", Fingerprint: "chrome", Flow: "xtls-rprx-vision", UDP: true}
	compiled, err := proxy.CompileNative(proxy.CompileInput{Node: node, Datapath: proxy.DatapathRoutedTUN, RoutedTUN: &proxy.RoutedTUNConfig{InterfaceName: "b6p-tun", Address: "172.31.255.253/30"}, IPv6: proxy.IPv6Direct, MixedListenAddress: "192.168.31.1", DNSListenAddress: "192.168.31.1", Ports: proxy.Ports{Mixed: 2080, TProxy: 7893, DNS: 6450}, ManagementIPs: []string{"192.168.31.1"}})
	if err != nil {
		t.Fatal(err)
	}
	desired := capture.Desired{Enabled: true, IPv6: proxy.IPv6Direct, Devices: []capture.DeviceSelection{{MAC: "02:be:65:00:00:fa"}}}
	observation := router.CaptureObservation{Devices: []router.Device{{IP: "192.168.31.250", MAC: "02:be:65:00:00:fa", Eligible: true}}, LANPrefixes: []string{"192.168.31.0/24"}, LANAddresses: []string{"192.168.31.1"}, ManagementIPs: []string{"192.168.31.1"}}
	input, clients, err := capture.BuildFromAccepted(context.Background(), desired, compiled.Config, observation, nil)
	if err != nil {
		t.Fatal(err)
	}
	if input.Datapath != proxy.DatapathRoutedTUN || input.TUNInterface != "b6p-tun" || input.TUNAddress != "172.31.255.253/30" || len(clients) != 1 || input.ClientMACs["192.168.31.250"] != "02:be:65:00:00:fa" {
		t.Fatal("accepted backend/scope lost", input, clients)
	}
	plan, err := proxy.PlanOwnedRules(input)
	if err != nil {
		t.Fatal(err)
	}
	wantRoute := []string{"ip", "-4", "route", "add", "default", "dev", "b6p-tun", "table", "16500"}
	foundRoute := false
	for _, row := range plan.Apply {
		if reflect.DeepEqual(row, wantRoute) {
			foundRoute = true
		}
		text := strings.Join(row, " ")
		if strings.Contains(text, "TPROXY") || strings.Contains(text, "route add local") {
			t.Fatal("socket-handoff path leaked into TUN", row)
		}
	}
	if !foundRoute || plan.Ownership.Datapath != proxy.DatapathRoutedTUN || len(plan.Ownership.Chains) != 6 {
		t.Fatal(plan.Ownership)
	}
	var config map[string]any
	if json.Unmarshal(compiled.Config, &config) != nil {
		t.Fatal("compiler invalid JSON")
	}
	outs := config["outbounds"].([]any)
	for _, value := range outs {
		if value.(map[string]any)["type"] == "socks" {
			t.Fatal("diagnostic identity-hiding SOCKS made production")
		}
	}
}

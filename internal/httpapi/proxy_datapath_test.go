package httpapi

import (
	"be6500panel/internal/proxy"
	"fmt"
	"testing"
)

func TestDatapathSelectionPreservesAcceptedBackend(t *testing.T) {
	tunRaw := []byte(`{"inbounds":[{"type":"mixed"},{"type":"tun","tag":"tun-in","interface_name":"b6p-tun","address":["172.31.255.253/30"],"stack":"system","dns_mode":"disabled","mtu":1500,"auto_route":false,"auto_redirect":false,"udp_timeout":"2m","udp_nat_max":1024}]}`)
	mode, cfg, err := proxyDatapathSelection(nil, nil, tunRaw)
	if err != nil || mode != proxy.DatapathRoutedTUN || cfg == nil || cfg.InterfaceName != "b6p-tun" {
		t.Fatal(mode, cfg, err)
	}
	legacy := []byte(`{"inbounds":[{"type":"tproxy","tag":"tproxy-in"}]}`)
	if mode, cfg, err = proxyDatapathSelection(nil, nil, legacy); err != nil || mode != proxy.DatapathRoutedTUN || cfg == nil || cfg.InterfaceName != "b6p-tun" {
		t.Fatal(mode, cfg, err)
	}
	target := proxy.DatapathTPROXY
	if _, _, err = proxyDatapathSelection(&target, nil, tunRaw); err == nil {
		t.Fatal("retired TPROXY product path regenerated")
	}
	target = proxy.DatapathRoutedTUN
	input := &proxy.RoutedTUNConfig{InterfaceName: "b6p-tun", Address: "172.31.255.253/30"}
	mode, cfg, err = proxyDatapathSelection(&target, input, legacy)
	if err != nil || mode != target || cfg == input {
		t.Fatal("request copy/default mutation", mode, cfg, err)
	}
}
func TestDatapathSelectionRejectsAmbiguity(t *testing.T) {
	for _, raw := range []string{`bad`, `{"inbounds":[{"type":"tun","tag":"foreign","interface_name":"b6p-tun","address":["172.31.255.253/30"],"stack":"system","dns_mode":"disabled","mtu":1500,"auto_route":false,"auto_redirect":false,"udp_timeout":"2m","udp_nat_max":1024}]}`, `{"inbounds":[{"type":"tproxy"},{"type":"tun","tag":"tun-in","interface_name":"b6p-tun","address":["172.31.255.253/30"],"stack":"system","dns_mode":"disabled","mtu":1500,"auto_route":false,"auto_redirect":false,"udp_timeout":"2m","udp_nat_max":1024}]}`} {
		if _, _, err := proxyDatapathSelection(nil, nil, []byte(raw)); err == nil {
			t.Fatal("unsafe accepted inference", raw)
		}
	}
	empty := proxy.DatapathMode("")
	if _, _, err := proxyDatapathSelection(&empty, nil, nil); err == nil {
		t.Fatal("explicit unknown backend")
	}
	mode := proxy.DatapathRoutedTUN
	if selected, cfg, err := proxyDatapathSelection(&mode, nil, nil); err != nil || selected != proxy.DatapathRoutedTUN || cfg == nil || cfg.Address != "172.31.255.253/30" {
		t.Fatal("ready-to-use default missing", selected, cfg, err)
	}
	if _, _, err := proxyDatapathSelection(nil, &proxy.RoutedTUNConfig{}, nil); err == nil {
		t.Fatal("TUN intent without explicit selector")
	}
}

func TestImplicitDatapathDoesNotRewriteUnsafeTUN(t *testing.T) {
	for _, unsafe := range []string{`{"inbounds":[{"type":"tun","tag":"tun-in","interface_name":"b6p-tun","address":["172.31.255.253/30"],"stack":"system","dns_mode":"disabled","mtu":1500,"auto_route":true,"udp_timeout":"2m","udp_nat_max":1024}]}`, `{"inbounds":[{"type":"tun","tag":"tun-in","interface_name":"b6p-tun","address":["172.31.255.253/30"],"stack":"gvisor","dns_mode":"disabled","mtu":1500,"udp_timeout":"2m","udp_nat_max":1024}]}`} {
		if _, _, err := proxyDatapathSelection(nil, nil, []byte(unsafe)); err == nil {
			t.Fatal("unsafe accepted native normalized silently")
		}
	}
}

func TestImplicitDatapathRefusesUnpreservedNativeFields(t *testing.T) {
	base := `{"inbounds":[{"type":"tun","tag":"tun-in","interface_name":"b6p-tun","address":["172.31.255.253/30"],"stack":"system","dns_mode":"disabled","mtu":1500,"auto_route":false,"auto_redirect":false,"udp_timeout":"2m","udp_nat_max":1024%s}]}`
	for _, extra := range []string{`,"netns":"isolated"`, `,"netns":""`, `,"include_uid":[0]`, `,"route_address":["0.0.0.0/0"]`, `,"udp_mapping":"endpoint-independent"`} {
		if _, _, err := proxyDatapathSelection(nil, nil, []byte(fmt.Sprintf(base, extra))); err == nil {
			t.Fatal("native semantics silently dropped", extra)
		}
	}
}

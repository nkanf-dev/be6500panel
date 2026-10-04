package httpapi

import (
	"be6500panel/internal/proxy"
	"testing"
)

func TestDatapathSelectionPreservesAcceptedBackend(t *testing.T) {
	tunRaw := []byte(`{"inbounds":[{"type":"mixed"},{"type":"tun","tag":"tun-in","interface_name":"b6p-tun","address":["172.31.255.253/30"],"stack":"system","dns_mode":"disabled","mtu":1500,"auto_route":false,"auto_redirect":false,"udp_timeout":"2m","udp_nat_max":1024}]}`)
	mode, cfg, err := proxyDatapathSelection(nil, nil, tunRaw)
	if err != nil || mode != proxy.DatapathRoutedTUN || cfg == nil || cfg.InterfaceName != "b6p-tun" {
		t.Fatal(mode, cfg, err)
	}
	legacy := []byte(`{"inbounds":[{"type":"tproxy","tag":"tproxy-in"}]}`)
	if mode, cfg, err = proxyDatapathSelection(nil, nil, legacy); err != nil || mode != "" || cfg != nil {
		t.Fatal(mode, cfg, err)
	}
	target := proxy.DatapathTPROXY
	if mode, cfg, err = proxyDatapathSelection(&target, nil, tunRaw); err != nil || mode != target || cfg != nil {
		t.Fatal(mode, cfg, err)
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
	if _, _, err := proxyDatapathSelection(&mode, nil, nil); err == nil {
		t.Fatal("missing explicit TUN intent")
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

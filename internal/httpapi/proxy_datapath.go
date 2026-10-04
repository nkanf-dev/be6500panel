package httpapi

import (
	"be6500panel/internal/proxy"
	"encoding/json"
	"errors"
)

// Missing request fields preserve the actual accepted backend. No selection
// record or GET may silently migrate TPROXY or activate a routed-TUN backend.
func proxyDatapathSelection(requested *proxy.DatapathMode, tun *proxy.RoutedTUNConfig, accepted []byte) (proxy.DatapathMode, *proxy.RoutedTUNConfig, error) {
	bad := func() (proxy.DatapathMode, *proxy.RoutedTUNConfig, error) {
		return "", nil, errors.New("proxy_datapath_invalid")
	}
	if requested != nil {
		switch *requested {
		case proxy.DatapathTPROXY:
			if tun != nil {
				return bad()
			}
			return proxy.DatapathTPROXY, nil, nil
		case proxy.DatapathRoutedTUN:
			if tun == nil {
				return bad()
			}
			copy := *tun
			return proxy.DatapathRoutedTUN, &copy, nil
		default:
			return bad()
		}
	}
	if tun != nil {
		return bad()
	}
	if len(accepted) == 0 {
		return "", nil, nil
	}
	var cfg struct {
		Inbounds []struct {
			Type         string   `json:"type"`
			Tag          string   `json:"tag"`
			Interface    string   `json:"interface_name"`
			Address      []string `json:"address"`
			Stack        string   `json:"stack"`
			DNSMode      string   `json:"dns_mode"`
			MTU          uint32   `json:"mtu"`
			AutoRoute    bool     `json:"auto_route"`
			AutoRedirect bool     `json:"auto_redirect"`
			UDPTimeout   string   `json:"udp_timeout"`
			UDPNATMax    uint32   `json:"udp_nat_max"`
		} `json:"inbounds"`
	}
	if json.Unmarshal(accepted, &cfg) != nil {
		return bad()
	}
	var rawConfig struct {
		Inbounds []json.RawMessage `json:"inbounds"`
	}
	if json.Unmarshal(accepted, &rawConfig) != nil || len(rawConfig.Inbounds) != len(cfg.Inbounds) {
		return bad()
	}
	var found *proxy.RoutedTUNConfig
	legacy := false
	for index, in := range cfg.Inbounds {
		if in.Type == "tproxy" {
			if legacy {
				return bad()
			}
			legacy = true
		}
		if in.Type == "tun" {
			var fields map[string]json.RawMessage
			if json.Unmarshal(rawConfig.Inbounds[index], &fields) != nil {
				return bad()
			}
			allowed := map[string]bool{"type": true, "tag": true, "interface_name": true, "address": true, "stack": true, "dns_mode": true, "mtu": true, "auto_route": true, "auto_redirect": true, "udp_timeout": true, "udp_nat_max": true}
			for key := range fields {
				if !allowed[key] {
					return bad()
				}
			}
			if found != nil || in.Tag != "tun-in" || in.Interface == "" || len(in.Address) != 1 || in.Stack != "system" || in.DNSMode != "disabled" || in.MTU != 1500 || in.AutoRoute || in.AutoRedirect || in.UDPTimeout != "2m" || in.UDPNATMax != 1024 {
				return bad()
			}
			found = &proxy.RoutedTUNConfig{InterfaceName: in.Interface, Address: in.Address[0]}
		}
	}
	if found != nil {
		if legacy {
			return bad()
		}
		return proxy.DatapathRoutedTUN, found, nil
	}
	return "", nil, nil
}

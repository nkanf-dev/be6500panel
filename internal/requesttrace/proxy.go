package requesttrace

import (
	"encoding/json"
	"net"
	"net/netip"
	"net/url"
	"strconv"
)

// ProxyEndpoint has no public listener URL or credentials. Only the server-side
// accepted-config parser below can construct one outside this package.
type ProxyEndpoint struct{ address string }

// MixedProxyFromNative validates a single accepted HTTP-capable mixed inbound.
// Wildcard binds cannot establish a safe destination. A non-loopback literal
// must exactly match a LAN address supplied by the authoritative router adapter.
// Authenticated/TLS mixed inbounds are not guessed or downgraded to plain HTTP.
func MixedProxyFromNative(native []byte, verifiedLAN []string) (ProxyEndpoint, error) {
	if len(native) == 0 || len(native) > 1<<20 || len(verifiedLAN) > 32 {
		return ProxyEndpoint{}, ErrUnavailable
	}
	var doc struct {
		Inbounds []struct {
			Type   string          `json:"type"`
			Listen string          `json:"listen"`
			Port   int             `json:"listen_port"`
			Users  json.RawMessage `json:"users"`
			TLS    json.RawMessage `json:"tls"`
		} `json:"inbounds"`
	}
	if json.Unmarshal(native, &doc) != nil || len(doc.Inbounds) > 64 {
		return ProxyEndpoint{}, ErrUnavailable
	}
	var endpoint ProxyEndpoint
	count := 0
	for _, in := range doc.Inbounds {
		if in.Type != "mixed" {
			continue
		}
		count++
		addr, err := netip.ParseAddr(in.Listen)
		if err != nil || addr.Zone() != "" || addr.Is4In6() || addr.IsUnspecified() || addr.IsMulticast() || addr.IsLinkLocalUnicast() || in.Port < 1 || in.Port > 65535 {
			return ProxyEndpoint{}, ErrUnavailable
		}
		allowed := addr.IsLoopback()
		for _, literal := range verifiedLAN {
			lan, err := netip.ParseAddr(literal)
			if err == nil && lan.Zone() == "" && !lan.IsUnspecified() && lan == addr {
				allowed = true
			}
		}
		if !allowed {
			return ProxyEndpoint{}, ErrUnavailable
		}
		if len(in.Users) != 0 && string(in.Users) != "null" {
			var users []json.RawMessage
			if json.Unmarshal(in.Users, &users) != nil || len(users) != 0 {
				return ProxyEndpoint{}, ErrUnavailable
			}
		}
		if len(in.TLS) != 0 && string(in.TLS) != "null" {
			var tls struct {
				Enabled bool `json:"enabled"`
			}
			if json.Unmarshal(in.TLS, &tls) != nil || tls.Enabled {
				return ProxyEndpoint{}, ErrUnavailable
			}
		}
		endpoint.address = net.JoinHostPort(addr.String(), strconv.Itoa(in.Port))
	}
	if count != 1 {
		return ProxyEndpoint{}, ErrUnavailable
	}
	return endpoint, nil
}
func (p ProxyEndpoint) proxyURL() (*url.URL, error) {
	if p.address == "" {
		return nil, ErrUnavailable
	}
	return &url.URL{Scheme: "http", Host: p.address}, nil
}

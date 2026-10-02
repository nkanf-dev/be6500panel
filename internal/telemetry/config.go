package telemetry

import (
	"encoding/json"
	"errors"
	"net"
	"strconv"
	"strings"
)

var (
	ErrUnavailable = errors.New("core telemetry is unavailable")
	ErrBusy        = errors.New("a latency probe is already running")
	ErrCooldown    = errors.New("wait before another latency probe")
	ErrProbeFailed = errors.New("selected outbound latency probe failed")
)

// CoreConfig is private. Never encode it into a panel response or log.
type CoreConfig struct {
	Address  string
	Secret   string
	Epoch    string
	CanProbe bool
}

// FromNativeConfig only accepts a literal loopback listener from accepted native
// configuration. No destination, raw config, credentials or errors are reflected.
func FromNativeConfig(raw []byte) (CoreConfig, error) {
	if len(raw) == 0 || len(raw) > 2<<20 {
		return CoreConfig{}, ErrUnavailable
	}
	var cfg struct {
		Experimental struct {
			ClashAPI *struct {
				Address string `json:"external_controller"`
				Secret  string `json:"secret"`
			} `json:"clash_api"`
		} `json:"experimental"`
		Outbounds []struct {
			Tag  string `json:"tag"`
			Type string `json:"type"`
		} `json:"outbounds"`
	}
	if json.Unmarshal(raw, &cfg) != nil || cfg.Experimental.ClashAPI == nil {
		return CoreConfig{}, ErrUnavailable
	}
	api := cfg.Experimental.ClashAPI
	out := CoreConfig{Address: api.Address, Secret: api.Secret}
	if !validCoreConfig(out) {
		return CoreConfig{}, ErrUnavailable
	}
	for _, outbound := range cfg.Outbounds {
		if outbound.Tag == "proxy" && outbound.Type != "direct" && outbound.Type != "block" {
			out.CanProbe = true
		}
	}
	return out, nil
}

func validCoreConfig(cfg CoreConfig) bool {
	host, port, err := net.SplitHostPort(cfg.Address)
	if err != nil {
		return false
	}
	ip := net.ParseIP(host)
	p, err := strconv.ParseUint(port, 10, 16)
	return err == nil && p > 0 && ip != nil && ip.IsLoopback() && len(cfg.Secret) <= 256 && !strings.ContainsAny(cfg.Secret, "\r\n")
}

// ClashOptions is for the owner's native compiler, not a public endpoint.
// The owner must generate/persist secret privately and re-check the full config.
func ClashOptions(secret string) (map[string]any, error) {
	if len(secret) < 32 || len(secret) > 256 || strings.ContainsAny(secret, "\r\n") {
		return nil, ErrUnavailable
	}
	return map[string]any{"external_controller": "127.0.0.1:9090", "secret": secret, "access_control_allow_origin": []string{"http://127.0.0.1"}}, nil
}

package httpapi

import (
	"encoding/json"
	"errors"
	"net"
	"strconv"
)

// preserveLocalTelemetry carries only an accepted loopback-only observability
// listener into a newly compiled node config. Older minimal cores without a
// telemetry listener remain supported. Private authentication stays private.
func preserveLocalTelemetry(candidate, accepted []byte) ([]byte, error) {
	var current struct {
		Experimental struct {
			ClashAPI json.RawMessage `json:"clash_api"`
		} `json:"experimental"`
	}
	if len(accepted) == 0 {
		return candidate, nil
	}
	if err := json.Unmarshal(accepted, &current); err != nil {
		return nil, errors.New("accepted telemetry configuration invalid")
	}
	api := current.Experimental.ClashAPI
	if len(api) == 0 || string(api) == "null" {
		return candidate, nil
	}
	var listener struct {
		Controller string `json:"external_controller"`
	}
	if json.Unmarshal(api, &listener) != nil {
		return nil, errors.New("accepted telemetry listener invalid")
	}
	if listener.Controller == "" {
		return candidate, nil
	}
	host, port, err := net.SplitHostPort(listener.Controller)
	ip := net.ParseIP(host)
	number, portErr := strconv.Atoi(port)
	if err != nil || ip == nil || !ip.IsLoopback() || portErr != nil || number < 1 || number > 65535 {
		return nil, errors.New("telemetry listener must remain loopback-only")
	}
	var next map[string]json.RawMessage
	if json.Unmarshal(candidate, &next) != nil || next == nil {
		return nil, errors.New("compiled proxy configuration invalid")
	}
	experimental := map[string]json.RawMessage{}
	if raw := next["experimental"]; len(raw) > 0 && string(raw) != "null" {
		if json.Unmarshal(raw, &experimental) != nil || experimental == nil {
			return nil, errors.New("compiled experimental configuration invalid")
		}
	}
	experimental["clash_api"] = append(json.RawMessage(nil), api...)
	raw, err := json.Marshal(experimental)
	if err != nil {
		return nil, err
	}
	next["experimental"] = raw
	return json.Marshal(next)
}

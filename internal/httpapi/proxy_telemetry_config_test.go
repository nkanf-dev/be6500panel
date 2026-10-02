package httpapi

import (
	"encoding/json"
	"strings"
	"testing"
)

func TestPreserveLocalTelemetryAcrossNodeCommit(t *testing.T) {
	accepted := []byte(`{"experimental":{"clash_api":{"external_controller":"127.0.0.1:9090","secret":"private-token"},"cache_file":{"enabled":true}}}`)
	candidate := []byte(`{"outbounds":[{"type":"direct","tag":"direct"}],"experimental":{"debug":{"listen":"127.0.0.1:6060"}}}`)
	output, err := preserveLocalTelemetry(candidate, accepted)
	if err != nil {
		t.Fatal(err)
	}
	var next map[string]json.RawMessage
	if err = json.Unmarshal(output, &next); err != nil {
		t.Fatal(err)
	}
	var experimental map[string]json.RawMessage
	if err = json.Unmarshal(next["experimental"], &experimental); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(experimental["clash_api"]), "private-token") || experimental["debug"] == nil {
		t.Fatal("private accepted telemetry and compiled fields must both remain")
	}
	if experimental["cache_file"] != nil {
		t.Fatal("do not carry unrelated cache behavior")
	}
}

func TestPreserveLocalTelemetryOldCoreHasNoNewRequirement(t *testing.T) {
	candidate := []byte(`{"inbounds":[{"type":"mixed"}]}`)
	for _, accepted := range [][]byte{nil, []byte(`{}`), []byte(`{"experimental":{"clash_api":null}}`), []byte(`{"experimental":{"clash_api":{}}}`)} {
		output, err := preserveLocalTelemetry(candidate, accepted)
		if err != nil || string(output) != string(candidate) {
			t.Fatalf("unexpected mutation or error %v", err)
		}
	}
}

func TestPreserveLocalTelemetryRejectsExposedOrInvalidController(t *testing.T) {
	for _, address := range []string{"0.0.0.0:9090", "192.168.31.1:9090", "localhost:9090", "127.0.0.1:0", "[::]:9090", "127.0.0.1:99999"} {
		raw, _ := json.Marshal(map[string]any{"experimental": map[string]any{"clash_api": map[string]string{"external_controller": address}}})
		if _, err := preserveLocalTelemetry([]byte(`{}`), raw); err == nil {
			t.Fatalf("accepted unsafe controller %s", address)
		}
	}
	if _, err := preserveLocalTelemetry([]byte(`{}`), []byte(`{broken`)); err == nil {
		t.Fatal("invalid accepted config")
	}
	if _, err := preserveLocalTelemetry([]byte(`{broken`), []byte(`{"experimental":{"clash_api":{"external_controller":"[::1]:9090"}}}`)); err == nil {
		t.Fatal("invalid candidate config")
	}
}

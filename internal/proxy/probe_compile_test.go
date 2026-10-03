package proxy

import (
	"encoding/json"
	"strings"
	"testing"
)

func TestCompileProbeIsolatedCredentialPreserving(t *testing.T) {
	n := testNode(t)
	n.ID = "node-one"
	other := n
	other.ID = "node-two"
	other.Server = "other.example"
	data, err := CompileProbe(ProbeCompileInput{Nodes: []Node{n, other}, ListenAddress: "127.0.0.1:24567", Password: strings.Repeat("a", 64)})
	if err != nil {
		t.Fatal(err)
	}
	var config map[string]any
	if json.Unmarshal(data, &config) != nil {
		t.Fatal("invalid config")
	}
	if config["experimental"] != nil || config["route"].(map[string]any)["rule_set"] != nil {
		t.Fatal("active control/policy copied")
	}
	ins := maps(config["inbounds"])
	if len(ins) != 1 || ins[0]["type"] != "mixed" || ins[0]["listen"] != "127.0.0.1" || len(ins[0]["users"].([]any)) != 2 {
		t.Fatal("not isolated inbound")
	}
	outs := maps(config["outbounds"])
	for i, want := range []Node{n, other} {
		out := outs[i]
		tls := out["tls"].(map[string]any)
		if out["tag"] != ProbeTag(i) || out["uuid"] != want.UUID || out["server"] != want.Server || out["flow"] != want.Flow || out["packet_encoding"] != "xudp" || tls["server_name"] != want.ServerName || tls["insecure"] != nil || tls["reality"].(map[string]any)["public_key"] != want.RealityPublicKey || tls["reality"].(map[string]any)["short_id"] != want.RealityShortID || tls["utls"].(map[string]any)["fingerprint"] != want.Fingerprint {
			t.Fatal("supported credentials/options not preserved")
		}
	}
	rules := maps(config["route"].(map[string]any)["rules"])
	if len(rules) != 3 || rules[2]["action"] != "reject" {
		t.Fatal("unknown user fallback not rejected")
	}
	for i := 0; i < 2; i++ {
		if rules[i]["auth_user"].([]any)[0] != ProbeTag(i) || rules[i]["outbound"] != ProbeTag(i) {
			t.Fatal("node identity route mismatch")
		}
	}
	for _, forbidden := range []string{"tproxy", "dns-in", "cache_file", "selector", "insecure"} {
		if strings.Contains(string(data), forbidden) {
			t.Fatalf("forbidden active feature: %s", forbidden)
		}
	}
}
func TestCompileProbeLimitsAndNoPrivateError(t *testing.T) {
	n := testNode(t)
	n.ID = "id"
	good := ProbeCompileInput{Nodes: []Node{n}, ListenAddress: "127.0.0.1:24567", Password: strings.Repeat("b", 64)}
	cases := []ProbeCompileInput{good, good, good, good, good}
	cases[0].ListenAddress = "0.0.0.0:1"
	cases[1].Password = "tiny"
	cases[2].Nodes = []Node{n, n}
	cases[3].Nodes = make([]Node, MaxProbeNodes+1)
	cases[4].Nodes[0].UUID = "PRIVATE-INVALID-UUID"
	for _, in := range cases {
		_, err := CompileProbe(in)
		if err == nil || strings.Contains(err.Error(), "PRIVATE") {
			t.Fatal("invalid input accepted or echoed")
		}
	}
}

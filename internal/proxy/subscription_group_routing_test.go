package proxy

import (
	"encoding/json"
	"strings"
	"testing"
)

func TestSingleDirectSelectorDoesNotBecomeProxy(t *testing.T) {
	raw := strings.Replace(syntheticYAML, "dns:\n", `  - name: domestic
    type: select
    proxies: [DIRECT]
  - name: nested-domestic
    type: select
    proxies: [domestic]
dns:
`, 1)
	raw = strings.Replace(raw, "  - GEOIP,CN,DIRECT", "  - GEOIP,CN,domestic\n  - GEOSITE,CN,nested-domestic", 1)
	sub, err := ParseClashYAML(strings.NewReader(raw))
	if err != nil {
		t.Fatal(err)
	}
	count := 0
	for _, rule := range sub.Rules {
		if rule.Kind == RuleSet {
			count++
			if rule.Target != TargetDirect {
				t.Fatal("explicit DIRECT group changed to proxy", rule)
			}
		}
	}
	if count != 2 {
		t.Fatal("CN rule fixture missing", count)
	}
	compiled, err := CompileNative(CompileInput{Node: sub.Nodes[0], Rules: sub.Rules, RuleSets: stagedRefs()})
	if err != nil {
		t.Fatal(err)
	}
	var native struct {
		Route struct {
			Rules []struct {
				Sets     []string `json:"rule_set"`
				Outbound string   `json:"outbound"`
			} `json:"rules"`
		} `json:"route"`
	}
	if json.Unmarshal(compiled.Config, &native) != nil {
		t.Fatal("native config")
	}
	for _, rule := range native.Route.Rules {
		if len(rule.Sets) > 0 && rule.Outbound == "proxy" && (rule.Sets[0] == "cn-domain" || rule.Sets[0] == "cn-ip") {
			t.Fatal("CN direct selector still proxies", rule)
		}
	}
}
func TestUniformGroupDoesNotGuessMixedOrCyclicChoice(t *testing.T) {
	groups := map[string][]string{"direct": {"DIRECT"}, "nested": {"direct", "DIRECT"}, "blocked": {"REJECT"}, "mixed": {"DIRECT", "node"}, "cycle": {"cycle"}, "unknown": {"missing"}}
	for _, name := range []string{"direct", "nested"} {
		target, okay := uniformGroupTarget(name, groups, map[string]bool{})
		if !okay || target != TargetDirect {
			t.Fatal(name, target, okay)
		}
	}
	if target, okay := uniformGroupTarget("blocked", groups, map[string]bool{}); !okay || target != TargetBlock {
		t.Fatal(target, okay)
	}
	for _, name := range []string{"mixed", "cycle", "unknown"} {
		if _, okay := uniformGroupTarget(name, groups, map[string]bool{}); okay {
			t.Fatal("guessed selector intent", name)
		}
	}
}

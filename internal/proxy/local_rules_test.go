package proxy

import (
	"encoding/json"
	"errors"
	"fmt"
	"reflect"
	"strings"
	"testing"
)

func overlayRule(id string, enabled bool, kind RuleKind, value string, target Target) LocalRule {
	return LocalRule{ID: id, Enabled: enabled, Label: id, Rule: Rule{Kind: kind, Value: value, Target: target}}
}

func hasOverlayDiagnostic(out EffectivePolicy, code string) bool {
	for _, d := range out.Diagnostics {
		if d.Code == code {
			return true
		}
	}
	return false
}

func TestLocalPolicyPrependOrderAndClearPreservesSubscription(t *testing.T) {
	subscription := []Rule{
		{Kind: RuleDomain, Value: "gpt.kanglives.top", Target: TargetProxy, Index: 19},
		{Kind: RuleDomainSuffix, Value: "example.com", Target: TargetDirect, Index: 45},
		{Kind: RuleMatch, Target: TargetProxy, Index: 81},
	}
	original := append([]Rule{}, subscription...)
	policy := Policy{Rules: []LocalRule{
		overlayRule("gpt-direct", true, RuleDomain, "gpt.kanglives.top", TargetDirect),
		overlayRule("block-first", true, RuleDomain, "blocked.example.com", TargetBlock),
		overlayRule("disabled", false, RuleMatch, "", TargetBlock),
	}}
	out, err := MergeEffectivePolicy(subscription, policy)
	if err != nil {
		t.Fatal(err)
	}
	if len(out.Rules) != 5 || out.Rules[0].Target != TargetDirect || out.Rules[1].Target != TargetBlock || !reflect.DeepEqual(out.Rules[2:], original) {
		t.Fatal("local priority or source order changed", out.Rules)
	}
	if out.Provenance[0].StableID != "gpt-direct" || out.Provenance[0].Layer != "local" || out.Provenance[0].SourceIndex != -1 || out.Provenance[2].SourceIndex != 19 || out.Provenance[2].SourceOrdinal != 0 || out.Provenance[2].EffectiveIndex != 2 {
		t.Fatal("provenance mismatch", out.Provenance)
	}
	if !hasOverlayDiagnostic(out, "disabled-rule") || !reflect.DeepEqual(subscription, original) {
		t.Fatal("disabled rule compiled or caller modified")
	}
	out.Rules[2].Value = "changed.example"
	if !reflect.DeepEqual(subscription, original) {
		t.Fatal("output aliases source")
	}
	cleared, err := MergeEffectivePolicy(subscription, Policy{})
	if err != nil || !reflect.DeepEqual(cleared.Rules, original) {
		t.Fatal("clear failed to restore exact subscription", err, cleared.Rules)
	}
}

func TestLocalBlockBeforeSubscriptionDirectAndCompilerPrelude(t *testing.T) {
	domain := "blocked.example.com"
	sub := []Rule{{Kind: RuleDomain, Value: domain, Target: TargetDirect, Index: 12}}
	policy := Policy{Rules: []LocalRule{
		overlayRule("block-domain", true, RuleDomain, domain, TargetBlock),
		overlayRule("block-management", true, RuleDomain, "router.example.com", TargetBlock),
	}}
	out, err := MergeEffectivePolicy(sub, policy)
	if err != nil {
		t.Fatal(err)
	}
	compiled, err := CompileNative(CompileInput{Node: testNode(t), Rules: out.Rules, BootstrapDomains: []string{"router.example.com"}, ManagementIPs: []string{"192.168.31.1"}})
	if err != nil {
		t.Fatal(err)
	}
	rules := maps(decodeConfig(t, compiled)["route"].(map[string]any)["rules"])
	var blocked, direct, bootstrap, management int = -1, -1, -1, -1
	for i, rule := range rules {
		encoded, _ := json.Marshal(rule)
		if strings.Contains(string(encoded), "192.168.31.1/32") && rule["outbound"] == "direct" {
			management = i
		}
		if domains, ok := rule["domain"].([]any); ok {
			for _, value := range domains {
				if value == domain && rule["action"] == "reject" {
					blocked = i
				}
				if value == domain && rule["outbound"] == "direct" {
					direct = i
				}
				if value == "router.example.com" && rule["outbound"] == "direct" {
					bootstrap = i
				}
			}
		}
	}
	if blocked < 0 || direct <= blocked || management < 0 || management >= blocked || bootstrap < 0 || bootstrap >= blocked {
		t.Fatal("user policy bypassed compiler management/bootstrap prelude", blocked, direct, bootstrap, management)
	}
}

func TestSubscriptionEditExactReferenceRefreshAndDuplicateOccurrence(t *testing.T) {
	sub := []Rule{
		{Kind: RuleDomainSuffix, Value: "EXAMPLE.com", Target: TargetProxy, Index: 5},
		{Kind: RuleDomainSuffix, Value: "example.com", Target: TargetProxy, Index: 8},
		{Kind: RuleIPCIDR, Value: "192.0.2.129/24", Target: TargetDirect, Index: 20},
	}
	refs, err := SubscriptionFingerprints(sub)
	if err != nil || refs[0] == refs[1] || !strings.HasSuffix(refs[0], ":1") || !strings.HasSuffix(refs[1], ":2") || refs[0][:64] != refs[1][:64] {
		t.Fatal("duplicate identity failed", refs, err)
	}
	normalized := append([]Rule{}, sub...)
	normalized[0].Index = 900
	normalized[0].Value = "example.com"
	normalized[2].Value = "192.0.2.0/24"
	newRefs, _ := SubscriptionFingerprints(normalized)
	if !reflect.DeepEqual(refs, newRefs) {
		t.Fatal("fingerprint includes index/case/host prefix", refs, newRefs)
	}
	replacement := Rule{Kind: RuleDomain, Value: "replacement.example", Target: TargetBlock, Index: 999}
	policy := Policy{SubscriptionEdits: []SubscriptionEdit{
		{ID: "rewrite-second", SourceFingerprint: refs[1], Replacement: &replacement, Label: "rewritten"},
		{ID: "disable-cidr", SourceFingerprint: refs[2], Disabled: true},
	}}
	out, err := MergeEffectivePolicy(sub, policy)
	if err != nil || len(out.Rules) != 2 || out.Rules[0] != sub[0] || out.Rules[1].Value != replacement.Value || out.Rules[1].Index != sub[1].Index {
		t.Fatal("exact rewrite/disable failed", out, err)
	}
	if out.Provenance[1].StableID != "rewrite-second" || out.Provenance[1].SourceFingerprint != refs[1] || out.Provenance[1].SourceIndex != 8 || out.Provenance[1].Label != "rewritten" || replacement.Index != 999 {
		t.Fatal("rewritten provenance or input changed", out.Provenance)
	}
	refresh := append([]Rule{{Kind: RuleDomain, Value: "new.example", Target: TargetDirect, Index: 0}}, sub...)
	refreshed, err := MergeEffectivePolicy(refresh, policy)
	if err != nil || len(refreshed.Rules) != 3 || refreshed.Rules[0] != refresh[0] || refreshed.Rules[2].Value != replacement.Value {
		t.Fatal("nonsemantic subscription insertion broke references", err, refreshed)
	}
	// Changed target and removed duplicate cannot match either old reference.
	newSub := []Rule{{Kind: RuleDomainSuffix, Value: "example.com", Target: TargetDirect, Index: 8}}
	orphaned, err := MergeEffectivePolicy(newSub, policy)
	if err != nil || !reflect.DeepEqual(orphaned.Rules, newSub) || !hasOverlayDiagnostic(orphaned, "orphaned-edit") {
		t.Fatal("orphan edit rewrote unrelated refreshed rule", orphaned, err)
	}
	if policy.SubscriptionEdits[0].Replacement != &replacement {
		t.Fatal("merger modified overlay")
	}
}

func TestLocalTerminalPreviewDisabledDoesNotMakeRulesUnreachable(t *testing.T) {
	sub := []Rule{{Kind: RuleDomain, Value: "example.com", Target: TargetDirect, Index: 4}}
	policy := Policy{Rules: []LocalRule{
		overlayRule("inactive-match", false, RuleMatch, "", TargetBlock),
		overlayRule("active-domain", true, RuleDomain, "first.example", TargetProxy),
	}}
	out, err := MergeEffectivePolicy(sub, policy)
	if err != nil || hasOverlayDiagnostic(out, "unreachable-rule") {
		t.Fatal("inactive terminal blocks later rule", err, out)
	}
	policy.Rules[0].Enabled = true
	out, err = MergeEffectivePolicy(sub, policy)
	if err != nil || len(out.Rules) != 3 || !hasOverlayDiagnostic(out, "unreachable-rule") {
		t.Fatal("terminal preview missing", out, err)
	}
	for _, d := range out.Diagnostics {
		if d.Code == "unreachable-rule" && (d.Index < 1 || d.Scope != "effective-rule") {
			t.Fatal("bad preview identity", d)
		}
	}
}

func TestPolicyValidationFixedReasonsAndBounds(t *testing.T) {
	valid := Policy{Rules: []LocalRule{overlayRule("safe-id", true, RuleDomain, "example.com", TargetDirect)}}
	ref, _ := SubscriptionFingerprints([]Rule{valid.Rules[0].Rule})
	tests := []struct {
		name string
		edit func(*Policy)
	}{
		{"empty-id", func(p *Policy) { p.Rules[0].ID = "" }},
		{"unicode-id", func(p *Policy) { p.Rules[0].ID = "身份" }},
		{"private-id", func(p *Policy) { p.Rules[0].ID = "secret/path" }},
		{"id-limit", func(p *Policy) { p.Rules[0].ID = strings.Repeat("a", 65) }},
		{"label-limit", func(p *Policy) { p.Rules[0].Label = strings.Repeat("界", 65) }},
		{"note-limit", func(p *Policy) { p.Rules[0].Note = strings.Repeat("n", 257) }},
		{"control-text", func(p *Policy) { p.Rules[0].Note = "secret\x00" }},
		{"unknown-matcher", func(p *Policy) { p.Rules[0].Rule.Kind = "process-path"; p.Rules[0].Rule.Value = "/secret/process" }},
		{"unknown-ruleset", func(p *Policy) { p.Rules[0].Rule.Kind = RuleSet; p.Rules[0].Rule.Value = "secret-url" }},
		{"invalid-domain", func(p *Policy) { p.Rules[0].Rule.Value = "https://secret.example" }},
		{"keyword-control", func(p *Policy) { p.Rules[0].Rule.Kind = RuleDomainKeyword; p.Rules[0].Rule.Value = "secret\x01" }},
		{"keyword-space", func(p *Policy) { p.Rules[0].Rule.Kind = RuleDomainKeyword; p.Rules[0].Rule.Value = "secret value" }},
		{"invalid-action", func(p *Policy) { p.Rules[0].Rule.Target = "secret-action" }},
		{"no-resolve-domain", func(p *Policy) { p.Rules[0].Rule.NoResolve = true }},
		{"invalid-disabled", func(p *Policy) { p.Rules[0].Enabled = false; p.Rules[0].Rule.Kind = "process-path" }},
		{"duplicate-id", func(p *Policy) { p.Rules = append(p.Rules, p.Rules[0]) }},
		{"ref-index", func(p *Policy) {
			p.SubscriptionEdits = []SubscriptionEdit{{ID: "e", SourceFingerprint: "0", Disabled: true}}
		}},
		{"ref-noncanonical", func(p *Policy) {
			p.SubscriptionEdits = []SubscriptionEdit{{ID: "e", SourceFingerprint: ref[0][:65] + "01", Disabled: true}}
		}},
		{"ref-duplicate", func(p *Policy) {
			p.SubscriptionEdits = []SubscriptionEdit{{ID: "e", SourceFingerprint: ref[0], Disabled: true}, {ID: "e2", SourceFingerprint: ref[0], Disabled: true}}
		}},
		{"edit-no-operation", func(p *Policy) { p.SubscriptionEdits = []SubscriptionEdit{{ID: "e", SourceFingerprint: ref[0]}} }},
		{"edit-both-operations", func(p *Policy) {
			r := p.Rules[0].Rule
			p.SubscriptionEdits = []SubscriptionEdit{{ID: "e", SourceFingerprint: ref[0], Disabled: true, Replacement: &r}}
		}},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			policy := ClonePolicy(valid)
			test.edit(&policy)
			if err := ValidatePolicy(policy); !errors.Is(err, ErrInvalidPolicy) || strings.Contains(err.Error(), "secret") {
				t.Fatal("unsafe validation error", err)
			}
			out, err := MergeEffectivePolicy(nil, policy)
			raw, _ := json.Marshal(out.Diagnostics)
			if !errors.Is(err, ErrInvalidPolicy) || len(out.Rules) != 0 || len(out.Diagnostics) == 0 || strings.Contains(string(raw), "secret") {
				t.Fatal("unsafe/partial preview", out, err)
			}
		})
	}
	max := Policy{}
	for i := 0; i < MaxLocalRules; i++ {
		rule := overlayRule(fmt.Sprintf("r%d", i), false, RuleDomainKeyword, "example", TargetBlock)
		rule.Label, rule.Note = strings.Repeat("界", MaxRuleLabelRunes), strings.Repeat("界", MaxRuleNoteRunes)
		max.Rules = append(max.Rules, rule)
	}
	for i := 0; i < MaxSubscriptionEdits; i++ {
		max.SubscriptionEdits = append(max.SubscriptionEdits, SubscriptionEdit{ID: fmt.Sprintf("e%d", i), SourceFingerprint: fmt.Sprintf("%064x:1", i), Disabled: true})
	}
	if err := ValidatePolicy(max); err != nil {
		t.Fatal("valid maximum rejected", err)
	}
	tooManyRules := ClonePolicy(max)
	tooManyRules.Rules = append(tooManyRules.Rules, overlayRule("overflow", true, RuleMatch, "", TargetDirect))
	if ValidatePolicy(tooManyRules) == nil {
		t.Fatal("local rule cap absent")
	}
	tooManyEdits := ClonePolicy(max)
	tooManyEdits.SubscriptionEdits = append(tooManyEdits.SubscriptionEdits, SubscriptionEdit{ID: "overflow", SourceFingerprint: fmt.Sprintf("%064x:1", 99999), Disabled: true})
	if ValidatePolicy(tooManyEdits) == nil {
		t.Fatal("edit cap absent")
	}
}

func TestPolicyRevisionStableSemanticAndOrderedEnabledEdits(t *testing.T) {
	policy := Policy{Rules: []LocalRule{
		overlayRule("first", true, RuleDomain, "EXAMPLE.com", TargetDirect),
		overlayRule("second", false, RuleIPCIDR, "192.0.2.129/24", TargetBlock),
	}}
	revision, _ := PolicyRevision(policy)
	semantic := ClonePolicy(policy)
	semantic.Rules[0].Rule.Value = "example.com"
	semantic.Rules[0].Rule.Index = 500
	semantic.Rules[1].Rule.Value = "192.0.2.0/24"
	if actual, _ := PolicyRevision(semantic); actual != revision {
		t.Fatal("revision includes source index or nonsemantic spelling")
	}
	for _, change := range []func(*Policy){
		func(p *Policy) { p.Rules[0].Enabled = false },
		func(p *Policy) { p.Rules[0], p.Rules[1] = p.Rules[1], p.Rules[0] },
		func(p *Policy) { p.Rules[0].Rule.Target = TargetBlock },
		func(p *Policy) { p.Rules[0].Label = "edited" },
		func(p *Policy) { p.Rules[0].Note = "edited" },
		func(p *Policy) {
			p.SubscriptionEdits = []SubscriptionEdit{{ID: "edit", SourceFingerprint: strings.Repeat("0", 64) + ":1", Disabled: true}}
		},
	} {
		changed := ClonePolicy(policy)
		change(&changed)
		if actual, err := PolicyRevision(changed); err != nil || actual == revision {
			t.Fatal("policy change absent from identity", err)
		}
	}
	if nilRevision, _ := PolicyRevision(Policy{}); nilRevision == "" {
		t.Fatal("empty identity missing")
	} else if emptyRevision, _ := PolicyRevision(ClonePolicy(Policy{})); emptyRevision != nilRevision {
		t.Fatal("nil/empty arrays have different identity")
	}
}

func TestEffectivePolicyKeepsImportedDirectOnlyGroup(t *testing.T) {
	raw := strings.Replace(syntheticYAML, "dns:\n", "  - name: direct-only\n    type: select\n    proxies: [DIRECT]\ndns:\n", 1)
	raw = strings.Replace(raw, "  - GEOIP,CN,DIRECT", "  - GEOIP,CN,direct-only", 1)
	subscription, err := ParseClashYAML(strings.NewReader(raw))
	if err != nil {
		t.Fatal(err)
	}
	policy := Policy{Rules: []LocalRule{overlayRule("my-domain", true, RuleDomain, "gpt.kanglives.top", TargetDirect)}}
	out, err := MergeEffectivePolicy(subscription.Rules, policy)
	if err != nil || !reflect.DeepEqual(out.Rules[1:], subscription.Rules) {
		t.Fatal("import changed while merging", err, out)
	}
	found := false
	for _, r := range out.Rules {
		if r.Kind == RuleSet && r.Value == "cn-ip" {
			found = true
			if r.Target != TargetDirect {
				t.Fatal("direct-only group became proxy")
			}
		}
	}
	if !found {
		t.Fatal("direct-only fixture missing")
	}
}

func TestMergeEffectivePolicyPureDeterministicWithoutStagedIO(t *testing.T) {
	// Rule sets need no disk path or network staging for pure policy preview.
	// No compiler/config/store call is required, and output is independent.
	subscription := []Rule{{Kind: RuleSet, Value: "cn-ip", Target: TargetDirect, Index: 18}}
	policy := Policy{Rules: []LocalRule{overlayRule("local-set", true, RuleSet, "proxy-domain", TargetProxy)}}
	first, err := MergeEffectivePolicy(subscription, policy)
	second, againErr := MergeEffectivePolicy(subscription, policy)
	if err != nil || againErr != nil || !reflect.DeepEqual(first, second) {
		t.Fatal("pure preview depends on external staged I/O", err, againErr)
	}
	first.Provenance[0].Label = "mutated"
	first.Rules[0].Value = "mutated"
	if second.Provenance[0].Label != policy.Rules[0].Label || second.Rules[0] != policy.Rules[0].Rule {
		t.Fatal("pure output aliases another preview")
	}
	invalid, err := MergeEffectivePolicy([]Rule{{Kind: "process-path", Value: "secret", Target: TargetDirect}}, Policy{})
	if !errors.Is(err, ErrInvalidPolicy) || !hasOverlayDiagnostic(invalid, "unsupported-matcher") {
		t.Fatal("unsafe subscription accepted", invalid, err)
	}
}

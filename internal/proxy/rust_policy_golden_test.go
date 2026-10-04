package proxy

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

// These fixtures contain public synthetic policy data only. This generator
// calls the Go reference's pure policy functions, not the native compiler,
// router, store, server, API, or any network interface. EffectiveIndex belongs
// to the user policy list and must never be treated as a native route index.
//
// Ordinary tests never write files. Explicit generation and read-only checks:
//
//	BE6500_RUST_POLICY_GOLDEN_OUTPUT=/absolute/output.json go test -p 1 ./internal/proxy -run '^TestRustPolicyGoldenFixtures$' -count=1
//	BE6500_RUST_POLICY_GOLDEN_COMPARE=/absolute/existing.json go test -p 1 ./internal/proxy -run '^TestRustPolicyGoldenFixtures$' -count=1
const (
	rustPolicyGoldenOutputEnv  = "BE6500_RUST_POLICY_GOLDEN_OUTPUT"
	rustPolicyGoldenCompareEnv = "BE6500_RUST_POLICY_GOLDEN_COMPARE"
)

type rustPolicyGoldenInput struct {
	name         string
	subscription []Rule
	policy       Policy
	valid        bool
}

type rustPolicyGoldenCase struct {
	Name            string           `json:"name"`
	Subscription    []Rule           `json:"subscription"`
	Policy          Policy           `json:"policy"`
	Valid           bool             `json:"valid"`
	Revision        string           `json:"revision,omitempty"`
	Fingerprints    *[]string        `json:"fingerprints,omitempty"`
	Effective       *EffectivePolicy `json:"effective,omitempty"`
	ValidationError string           `json:"validationError,omitempty"`
}

type rustPolicyGoldenDocument struct {
	Version int                    `json:"version"`
	Cases   []rustPolicyGoldenCase `json:"cases"`
}

func rustPolicyGoldenLocal(id string, enabled bool, rule Rule) LocalRule {
	return LocalRule{ID: id, Enabled: enabled, Label: id, Rule: rule}
}

func rustPolicyGoldenRefs(t *testing.T, subscription []Rule) []string {
	t.Helper()
	refs, err := SubscriptionFingerprints(subscription)
	if err != nil {
		t.Fatal("synthetic reference subscription is invalid", err)
	}
	return append([]string{}, refs...)
}

func rustPolicyGoldenInputs(t *testing.T) []rustPolicyGoldenInput {
	t.Helper()
	inputs := []rustPolicyGoldenInput{}
	add := func(name string, subscription []Rule, policy Policy, valid bool) {
		inputs = append(inputs, rustPolicyGoldenInput{
			name: name, subscription: append([]Rule{}, subscription...), policy: ClonePolicy(policy), valid: valid,
		})
	}
	domain := func(value string, target Target, index int) Rule {
		return Rule{Kind: RuleDomain, Value: value, Target: target, Index: index}
	}
	local := func(id string, rule Rule) LocalRule { return rustPolicyGoldenLocal(id, true, rule) }

	add("empty", nil, Policy{}, true)
	add("local-gpt-exact-direct-before-proxy", []Rule{
		domain("gpt.example.com", TargetProxy, 19),
		{Kind: RuleMatch, Target: TargetProxy, Index: 81},
	}, Policy{Rules: []LocalRule{
		local("gpt-direct", domain("gpt.example.com", TargetDirect, 700)),
	}}, true)
	add("enabled-order-disabled-terminal", []Rule{domain("source.example", TargetProxy, 45)}, Policy{Rules: []LocalRule{
		local("first", domain("first.example", TargetDirect, 501)),
		rustPolicyGoldenLocal("inactive-match", false, Rule{Kind: RuleMatch, Target: TargetBlock, Index: 502}),
		local("second", Rule{Kind: RuleDomainSuffix, Value: "second.example", Target: TargetProxy, Index: 503}),
	}}, true)
	add("local-block-before-subscription-direct", []Rule{domain("blocked.example", TargetDirect, 12)}, Policy{Rules: []LocalRule{
		local("block-domain", domain("blocked.example", TargetBlock, 900)),
	}}, true)

	editSource := []Rule{
		{Kind: RuleDomainSuffix, Value: "EXAMPLE.com", Target: TargetProxy, Index: 5},
		{Kind: RuleDomainSuffix, Value: "example.com", Target: TargetProxy, Index: 8},
		{Kind: RuleIPCIDR, Value: "192.0.2.129/24", Target: TargetDirect, Index: 20},
	}
	editRefs := rustPolicyGoldenRefs(t, editSource)
	replacement := domain("replacement.example", TargetBlock, 999)
	editPolicy := Policy{SubscriptionEdits: []SubscriptionEdit{
		{ID: "rewrite-second", SourceFingerprint: editRefs[1], Replacement: &replacement, Label: "rewritten", Note: "second semantic occurrence"},
		{ID: "disable-cidr", SourceFingerprint: editRefs[2], Disabled: true, Label: "disabled CIDR"},
	}}
	add("subscription-disable-replace-exact-refs", editSource, editPolicy, true)
	refresh := append([]Rule{domain("inserted.example", TargetDirect, 30)}, editSource...)
	refresh[1].Index, refresh[1].Value = 900, "example.com"
	refresh[2].Index, refresh[2].Value = 901, "EXAMPLE.COM"
	refresh[3].Index, refresh[3].Value = 902, "192.0.2.0/24"
	add("subscription-refresh-index-spelling-stable-refs", refresh, editPolicy, true)
	add("orphan-after-refresh", []Rule{
		{Kind: RuleDomainSuffix, Value: "example.com", Target: TargetDirect, Index: 8},
	}, editPolicy, true)
	add("terminal-match-unreachable-preview", []Rule{
		{Kind: RuleDomainSuffix, Value: "source.example", Target: TargetProxy, Index: 77},
		{Kind: RuleMatch, Target: TargetProxy, Index: 88},
	}, Policy{Rules: []LocalRule{
		local("terminal", Rule{Kind: RuleMatch, Target: TargetBlock, Index: 100}),
		local("after-terminal", domain("later.example", TargetDirect, 101)),
		rustPolicyGoldenLocal("disabled-after-terminal", false, domain("inactive.example", TargetBlock, 102)),
	}}, true)

	normalizationPolicy := Policy{Rules: []LocalRule{
		local("exact", domain("UPPER.Example", TargetDirect, 701)),
		local("suffix", Rule{Kind: RuleDomainSuffix, Value: "SUFFIX.Example", Target: TargetProxy, Index: 702}),
		local("keyword", Rule{Kind: RuleDomainKeyword, Value: "MiXeD-Key", Target: TargetBlock, Index: 703}),
	}}
	normalizationSource := []Rule{
		domain("UPPER.Example", TargetDirect, 41),
		{Kind: RuleDomainSuffix, Value: "SUFFIX.Example", Target: TargetProxy, Index: 42},
		{Kind: RuleDomainKeyword, Value: "MiXeD-Key", Target: TargetBlock, Index: 43},
	}
	add("uppercase-domain-keyword-normalization", normalizationSource, normalizationPolicy, true)
	lowerPolicy := ClonePolicy(normalizationPolicy)
	lowerSource := append([]Rule{}, normalizationSource...)
	for i := range lowerPolicy.Rules {
		lowerPolicy.Rules[i].Rule.Value = strings.ToLower(lowerPolicy.Rules[i].Rule.Value)
		lowerPolicy.Rules[i].Rule.Index = 801 + i
		lowerSource[i].Value = strings.ToLower(lowerSource[i].Value)
		lowerSource[i].Index = 141 + i
	}
	add("lowercase-domain-keyword-same-identity", lowerSource, lowerPolicy, true)

	ipv4Policy := Policy{Rules: []LocalRule{
		local("cidr-no-resolve", Rule{Kind: RuleIPCIDR, Value: "192.0.2.129/24", Target: TargetDirect, NoResolve: true, Index: 333}),
		local("cidr-resolve", Rule{Kind: RuleIPCIDR, Value: "198.51.100.129/25", Target: TargetBlock, Index: 334}),
	}}
	ipv4Source := []Rule{ipv4Policy.Rules[0].Rule, ipv4Policy.Rules[1].Rule}
	add("ipv4-host-bits-and-no-resolve", ipv4Source, ipv4Policy, true)
	ipv4Canonical := ClonePolicy(ipv4Policy)
	ipv4Canonical.Rules[0].Rule.Value, ipv4Canonical.Rules[1].Rule.Value = "192.0.2.0/24", "198.51.100.128/25"
	ipv4Canonical.Rules[0].Rule.Index, ipv4Canonical.Rules[1].Rule.Index = 444, 445
	add("ipv4-masked-same-identity", []Rule{ipv4Canonical.Rules[0].Rule, ipv4Canonical.Rules[1].Rule}, ipv4Canonical, true)

	ipv6Policy := Policy{Rules: []LocalRule{
		local("ipv6", Rule{Kind: RuleIPCIDR, Value: "2001:0DB8:0000:0000:0000:0000:0000:00A1/64", Target: TargetProxy, NoResolve: true, Index: 616}),
	}}
	add("ipv6-expanded-host-bits", []Rule{ipv6Policy.Rules[0].Rule}, ipv6Policy, true)
	ipv6Canonical := ClonePolicy(ipv6Policy)
	ipv6Canonical.Rules[0].Rule.Value, ipv6Canonical.Rules[0].Rule.Index = "2001:db8::/64", 717
	add("ipv6-canonical-same-identity", []Rule{ipv6Canonical.Rules[0].Rule}, ipv6Canonical, true)

	controlled := []Rule{
		{Kind: RuleSet, Value: "cn-ip", Target: TargetDirect, NoResolve: true, Index: 11},
		{Kind: RuleSet, Value: "cn-ip", Target: TargetDirect, Index: 13},
		{Kind: RuleSet, Value: "cn-domain", Target: TargetDirect, Index: 17},
		{Kind: RuleSet, Value: "proxy-domain", Target: TargetProxy, Index: 23},
	}
	controlledPolicy := Policy{}
	for i, rule := range controlled {
		controlledPolicy.Rules = append(controlledPolicy.Rules, local(fmt.Sprintf("set-%d", i), rule))
	}
	add("controlled-sets-cn-ip-no-resolve-distinct", controlled, controlledPolicy, true)

	unicodeRule := local("unicode-public", domain("unicode.example", TargetDirect, 121))
	unicodeRule.Label = "公开标签 <>& \u2028\u2029 中文"
	unicodeRule.Note = "public <>&\u2028\u2029\nsecond line\t注释"
	add("unicode-html-line-separators-multiline-metadata", nil, Policy{Rules: []LocalRule{unicodeRule}}, true)
	duplicates := []Rule{
		domain("DUPLICATE.example", TargetProxy, 31),
		domain("duplicate.example", TargetProxy, 37),
		domain("Duplicate.Example", TargetProxy, 41),
	}
	duplicateRefs := rustPolicyGoldenRefs(t, duplicates)
	add("duplicate-semantic-occurrences", duplicates, Policy{SubscriptionEdits: []SubscriptionEdit{
		{ID: "only-second", SourceFingerprint: duplicateRefs[1], Disabled: true},
	}}, true)

	baseSource := []Rule{domain("public.example", TargetDirect, 7)}
	baseRef := rustPolicyGoldenRefs(t, baseSource)[0]
	badIDs := Policy{Rules: []LocalRule{
		local("", domain("empty-id.example", TargetDirect, 0)),
		local("身份", domain("unicode-id.example", TargetDirect, 0)),
		local("fake/id", domain("slash-id.example", TargetDirect, 0)),
		local(strings.Repeat("a", MaxPolicyIDBytes+1), domain("long-id.example", TargetDirect, 0)),
		local("duplicate", domain("duplicate-id.example", TargetDirect, 0)),
		local("duplicate", domain("duplicate-id-again.example", TargetBlock, 0)),
	}, SubscriptionEdits: []SubscriptionEdit{
		{ID: "duplicate", SourceFingerprint: baseRef, Disabled: true},
	}}
	for i := range badIDs.Rules {
		badIDs.Rules[i].Label = "public"
	}
	add("invalid-ids-and-cross-layer-duplicates", baseSource, badIDs, false)

	badMetadata := Policy{}
	for i, metadata := range [][2]string{
		{strings.Repeat("界", MaxRuleLabelRunes+1), ""},
		{"", strings.Repeat("n", MaxRuleNoteRunes+1)},
		{"", "public\x00note"},
		{"label\nline", ""},
		{"", "public\rnote"},
	} {
		rule := local(fmt.Sprintf("metadata-%d", i), domain("metadata.example", TargetDirect, i))
		rule.Label, rule.Note = metadata[0], metadata[1]
		badMetadata.Rules = append(badMetadata.Rules, rule)
	}
	add("invalid-metadata-rune-bounds-controls", nil, badMetadata, false)

	badMatchers := []Rule{
		{Kind: "process-path", Value: "/public/fake-process", Target: TargetDirect},
		domain("https://invalid.example", TargetDirect, 0),
		{Kind: RuleDomainSuffix, Value: "bad..example", Target: TargetDirect},
		{Kind: RuleDomainKeyword, Value: "public keyword", Target: TargetDirect},
		{Kind: RuleDomainKeyword, Value: "public\x01keyword", Target: TargetDirect},
		{Kind: RuleDomainKeyword, Value: "公开", Target: TargetDirect},
		{Kind: RuleDomainKeyword, Value: strings.Repeat("k", MaxRuleValueBytes+1), Target: TargetDirect},
		domain("invalid-action.example", "unsupported", 0),
		{Kind: RuleMatch, Value: "not-empty", Target: TargetBlock},
	}
	badMatcherPolicy := Policy{}
	for i, rule := range badMatchers {
		// Inactive rules are validated too, including the process matcher.
		badMatcherPolicy.Rules = append(badMatcherPolicy.Rules, rustPolicyGoldenLocal(fmt.Sprintf("matcher-%d", i), i != 0, rule))
	}
	add("invalid-matchers-domain-keyword-action-value-bound", nil, badMatcherPolicy, false)

	badOptions := []Rule{
		{Kind: RuleIPCIDR, Value: "192.0.2.0/33", Target: TargetDirect},
		{Kind: RuleIPCIDR, Value: "2001:db8::/129", Target: TargetDirect},
		{Kind: RuleIPCIDR, Value: "::ffff:192.0.2.1/128", Target: TargetDirect},
		{Kind: RuleDomain, Value: "option.example", Target: TargetDirect, NoResolve: true},
		{Kind: RuleMatch, Target: TargetBlock, NoResolve: true},
		{Kind: RuleSet, Value: "cn-domain", Target: TargetDirect, NoResolve: true},
		{Kind: RuleSet, Value: "proxy-domain", Target: TargetProxy, NoResolve: true},
		{Kind: RuleSet, Value: "unknown-public-set", Target: TargetDirect},
		{Kind: RuleSet, Value: "CN-IP", Target: TargetDirect},
	}
	badOptionPolicy := Policy{}
	for i, rule := range badOptions {
		badOptionPolicy.Rules = append(badOptionPolicy.Rules, local(fmt.Sprintf("option-%d", i), rule))
	}
	add("invalid-cidr-mapped-ipv6-no-resolve-controlled-set", nil, badOptionPolicy, false)

	operationSource := []Rule{
		domain("operation-0.example", TargetDirect, 10),
		domain("operation-1.example", TargetDirect, 11),
		domain("operation-2.example", TargetDirect, 12),
		domain("operation-3.example", TargetDirect, 13),
	}
	operationRefs := rustPolicyGoldenRefs(t, operationSource)
	badReplacement := Rule{Kind: "process-name", Value: "public-fake-process", Target: TargetBlock}
	badCIDRReplacement := Rule{Kind: RuleIPCIDR, Value: "192.0.2.1/99", Target: TargetDirect}
	add("invalid-edit-both-absent-and-replacements", operationSource, Policy{SubscriptionEdits: []SubscriptionEdit{
		{ID: "both", SourceFingerprint: operationRefs[0], Disabled: true, Replacement: &replacement},
		{ID: "absent", SourceFingerprint: operationRefs[1]},
		{ID: "bad-replacement", SourceFingerprint: operationRefs[2], Replacement: &badReplacement},
		{ID: "bad-cidr-replacement", SourceFingerprint: operationRefs[3], Replacement: &badCIDRReplacement},
	}}, false)

	badReferences := []string{
		"0", strings.ToUpper(baseRef), baseRef[:65] + "0", baseRef[:65] + "01",
		baseRef[:65] + fmt.Sprint(MaxRules+1), baseRef[:65] + "+1", baseRef[:65] + "-1",
		baseRef, baseRef,
	}
	badReferencePolicy := Policy{}
	for i, ref := range badReferences {
		badReferencePolicy.SubscriptionEdits = append(badReferencePolicy.SubscriptionEdits, SubscriptionEdit{
			ID: fmt.Sprintf("reference-%d", i), SourceFingerprint: ref, Disabled: true,
		})
	}
	add("invalid-fingerprints-noncanonical-bound-duplicates", baseSource, badReferencePolicy, false)
	return inputs
}

func rustPolicyGoldenBuild(t *testing.T) rustPolicyGoldenDocument {
	t.Helper()
	document := rustPolicyGoldenDocument{Version: 1, Cases: []rustPolicyGoldenCase{}}
	names := map[string]bool{}
	for _, input := range rustPolicyGoldenInputs(t) {
		if names[input.name] {
			t.Fatal("duplicate golden case", input.name)
		}
		names[input.name] = true
		policy := ClonePolicy(input.policy)
		subscription := append([]Rule{}, input.subscription...)
		originalPolicy, originalSubscription := ClonePolicy(policy), append([]Rule{}, subscription...)
		validationErr := ValidatePolicy(policy)
		if (validationErr == nil) != input.valid {
			t.Fatalf("%s: unexpected validation result: %v", input.name, validationErr)
		}
		revision, revisionErr := PolicyRevision(policy)
		fingerprints, fingerprintErr := SubscriptionFingerprints(subscription)
		if fingerprintErr != nil {
			t.Fatalf("%s: public synthetic subscription must be valid: %v", input.name, fingerprintErr)
		}
		fingerprints = append([]string{}, fingerprints...)
		effective, mergeErr := MergeEffectivePolicy(subscription, policy)
		fixture := rustPolicyGoldenCase{
			Name: input.name, Subscription: subscription, Policy: policy, Valid: validationErr == nil,
			Fingerprints: &fingerprints, Effective: &effective,
		}
		if validationErr == nil {
			if revisionErr != nil || mergeErr != nil || revision == "" {
				t.Fatalf("%s: valid policy failed revision or merge: %v %v", input.name, revisionErr, mergeErr)
			}
			fixture.Revision = revision
		} else {
			if !errors.Is(validationErr, ErrInvalidPolicy) || !errors.Is(revisionErr, ErrInvalidPolicy) || !errors.Is(mergeErr, ErrInvalidPolicy) {
				t.Fatalf("%s: unexpected invalid-policy errors: %v %v %v", input.name, validationErr, revisionErr, mergeErr)
			}
			if len(effective.Rules) != 0 || len(effective.Provenance) != 0 || len(effective.Diagnostics) == 0 {
				t.Fatalf("%s: invalid policy returned partial rules or lost safe diagnostics", input.name)
			}
			fixture.ValidationError = validationErr.Error()
		}
		if !reflect.DeepEqual(policy, originalPolicy) || !reflect.DeepEqual(subscription, originalSubscription) {
			t.Fatal("reference functions changed caller-owned inputs", input.name)
		}
		if fixture.Policy.Rules == nil || fixture.Policy.SubscriptionEdits == nil || fixture.Subscription == nil ||
			*fixture.Fingerprints == nil || effective.Rules == nil || effective.Provenance == nil || effective.Diagnostics == nil {
			t.Fatal("golden JSON arrays must be explicit []", input.name)
		}
		document.Cases = append(document.Cases, fixture)
	}
	return document
}

func rustPolicyGoldenBytes(t *testing.T) []byte {
	t.Helper()
	// Preserve Go encoding/json escaping and typed field order. Revision and
	// fingerprints are generated by Go's actual policy identity implementation.
	raw, err := json.MarshalIndent(rustPolicyGoldenBuild(t), "", "  ")
	if err != nil {
		t.Fatal(err)
	}
	return append(raw, '\n')
}

func TestRustPolicyGoldenSourceCases(t *testing.T) {
	document := rustPolicyGoldenBuild(t)
	cases := map[string]rustPolicyGoldenCase{}
	for _, fixture := range document.Cases {
		cases[fixture.Name] = fixture
	}
	for _, pair := range [][2]string{
		{"uppercase-domain-keyword-normalization", "lowercase-domain-keyword-same-identity"},
		{"ipv4-host-bits-and-no-resolve", "ipv4-masked-same-identity"},
		{"ipv6-expanded-host-bits", "ipv6-canonical-same-identity"},
	} {
		left, right := cases[pair[0]], cases[pair[1]]
		if left.Revision != right.Revision || !reflect.DeepEqual(*left.Fingerprints, *right.Fingerprints) {
			t.Fatal("semantic normalization or source-index exclusion changed", pair)
		}
	}
	before := cases["subscription-disable-replace-exact-refs"]
	after := cases["subscription-refresh-index-spelling-stable-refs"]
	if before.Revision != after.Revision || !reflect.DeepEqual(*before.Fingerprints, (*after.Fingerprints)[1:]) {
		t.Fatal("unrelated insertion or source-index change broke exact references")
	}
	if before.Effective.Rules[1].Index != 8 || before.Effective.Provenance[1].EffectiveIndex != 1 ||
		before.Effective.Provenance[1].SourceIndex != 8 || before.Policy.SubscriptionEdits[0].Replacement.Index != 999 {
		t.Fatal("replacement source provenance or caller-owned replacement changed")
	}
	duplicateRefs := *cases["duplicate-semantic-occurrences"].Fingerprints
	for i, ref := range duplicateRefs {
		if ref[:64] != duplicateRefs[0][:64] || !strings.HasSuffix(ref, fmt.Sprintf(":%d", i+1)) {
			t.Fatal("duplicate semantic occurrence identities changed")
		}
	}
	controlledRefs := *cases["controlled-sets-cn-ip-no-resolve-distinct"].Fingerprints
	if controlledRefs[0] == controlledRefs[1] {
		t.Fatal("no-resolve flag missing from semantic identity")
	}
	raw := rustPolicyGoldenBytes(t)
	if !bytes.Equal(raw, rustPolicyGoldenBytes(t)) {
		t.Fatal("fixture encoding is not deterministic")
	}
	for _, escaped := range []string{`\u003c`, `\u003e`, `\u0026`, `\u2028`, `\u2029`} {
		if !bytes.Contains(raw, []byte(escaped)) {
			t.Fatal("Go JSON escaping absent from Unicode metadata", escaped)
		}
	}
}

// Collection boundary payloads are generated in source tests, not stored as
// thousands of repetitive fixture rows. Rust differential tests can generate
// the same bounded values locally. No expected identity hash is hardcoded.
func TestRustPolicyGoldenCollectionBounds(t *testing.T) {
	subscription := make([]Rule, MaxSubscriptionEdits+1)
	for i := range subscription {
		subscription[i] = Rule{Kind: RuleDomainKeyword, Value: fmt.Sprintf("public-%d", i), Target: TargetDirect, Index: 3*i + 1}
	}
	refs := rustPolicyGoldenRefs(t, subscription)
	policy := ClonePolicy(Policy{})
	for i := 0; i < MaxLocalRules; i++ {
		policy.Rules = append(policy.Rules, rustPolicyGoldenLocal(fmt.Sprintf("local-%d", i), false, Rule{
			Kind: RuleDomainKeyword, Value: "public", Target: TargetBlock,
		}))
	}
	for i := 0; i < MaxSubscriptionEdits; i++ {
		policy.SubscriptionEdits = append(policy.SubscriptionEdits, SubscriptionEdit{
			ID: fmt.Sprintf("edit-%d", i), SourceFingerprint: refs[i], Disabled: true,
		})
	}
	if err := ValidatePolicy(policy); err != nil {
		t.Fatal("exact 512 local / 1024 edit policy bounds rejected", err)
	}
	if _, err := PolicyRevision(policy); err != nil {
		t.Fatal("exact-bound policy revision failed", err)
	}
	if _, err := MergeEffectivePolicy(subscription, policy); err != nil {
		t.Fatal("exact-bound policy merge failed", err)
	}
	for _, dimension := range []string{"local-rules", "subscription-edits"} {
		t.Run(dimension, func(t *testing.T) {
			overflow := ClonePolicy(policy)
			if dimension == "local-rules" {
				overflow.Rules = append(overflow.Rules, rustPolicyGoldenLocal("overflow-local", true, Rule{Kind: RuleMatch, Target: TargetDirect}))
			} else {
				overflow.SubscriptionEdits = append(overflow.SubscriptionEdits, SubscriptionEdit{ID: "overflow-edit", SourceFingerprint: refs[MaxSubscriptionEdits], Disabled: true})
			}
			if !errors.Is(ValidatePolicy(overflow), ErrInvalidPolicy) {
				t.Fatal("collection bound overflow accepted")
			}
			if _, err := PolicyRevision(overflow); !errors.Is(err, ErrInvalidPolicy) {
				t.Fatal("collection bound overflow revision accepted")
			}
			out, err := MergeEffectivePolicy(subscription, overflow)
			if !errors.Is(err, ErrInvalidPolicy) || len(out.Rules) != 0 || len(out.Diagnostics) != 1 || out.Diagnostics[0].Code != "policy-limit" {
				t.Fatal("collection bound overflow lost exact Go diagnostic", out, err)
			}
		})
	}
}

func TestRustPolicyGoldenFixtures(t *testing.T) {
	outputPath, comparePath := os.Getenv(rustPolicyGoldenOutputEnv), os.Getenv(rustPolicyGoldenCompareEnv)
	if outputPath == "" && comparePath == "" {
		t.Skip("explicit golden output or compare path is required; ordinary tests do not write fixtures")
	}
	if outputPath != "" && comparePath != "" {
		t.Fatal("choose either explicit output or read-only compare, not both")
	}
	selectedPath := outputPath
	if selectedPath == "" {
		selectedPath = comparePath
	}
	if !filepath.IsAbs(selectedPath) {
		t.Fatal("golden output and compare paths must be absolute")
	}
	raw := rustPolicyGoldenBytes(t)
	if outputPath != "" {
		if err := os.WriteFile(outputPath, raw, 0o644); err != nil {
			t.Fatal(err)
		}
		return
	}
	existing, err := os.ReadFile(comparePath)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(existing, raw) {
		t.Fatal("Go golden differs; regenerate explicitly and review the fixture change")
	}
}

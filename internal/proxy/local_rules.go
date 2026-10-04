package proxy

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"net/netip"
	"strconv"
	"strings"
	"unicode"
	"unicode/utf8"
)

const (
	MaxLocalRules        = 512
	MaxSubscriptionEdits = 1024
	MaxPolicyIDBytes     = 64
	MaxRuleLabelRunes    = 64
	MaxRuleNoteRunes     = 256
	MaxRuleValueBytes    = 253
)

var ErrInvalidPolicy = errors.New("invalid local proxy policy")

// Policy is an independent draft overlay, not native configuration. Slice order
// is the local rule order. Disabled rules remain here but never enter Rules.
// There is no process matcher: forwarded clients have no process identity.
type Policy struct {
	Rules             []LocalRule        `json:"rules"`
	SubscriptionEdits []SubscriptionEdit `json:"subscriptionEdits"`
}

type LocalRule struct {
	ID      string `json:"id"`
	Enabled bool   `json:"enabled"`
	Label   string `json:"label"`
	Note    string `json:"note"`
	Rule    Rule   `json:"rule"`
}

// SubscriptionEdit references a semantic SHA256 plus its duplicate occurrence,
// never a positional index. Exactly one of Disabled and Replacement is set.
// A missing reference remains an inactive orphan when a subscription refreshes.
type SubscriptionEdit struct {
	ID                string `json:"id"`
	SourceFingerprint string `json:"sourceFingerprint"`
	Disabled          bool   `json:"disabled"`
	Replacement       *Rule  `json:"replacement,omitempty"`
	Label             string `json:"label"`
	Note              string `json:"note"`
}

// EffectiveRuleIdentity is user policy provenance only. EffectiveIndex indexes
// EffectivePolicy.Rules, NOT a native route rule. Native indexes must be joined
// after compiling the actual prelude, resolve actions and fallback rules.
type EffectiveRuleIdentity struct {
	EffectiveIndex    int      `json:"effectiveIndex"`
	Layer             string   `json:"layer"`
	StableID          string   `json:"stableId"`
	Label             string   `json:"label"`
	SourceFingerprint string   `json:"sourceFingerprint,omitempty"`
	SourceIndex       int      `json:"sourceIndex"`
	SourceOrdinal     int      `json:"sourceOrdinal"`
	Kind              RuleKind `json:"kind"`
	Value             string   `json:"value,omitempty"`
	Target            Target   `json:"target"`
}

type EffectivePolicy struct {
	Rules       []Rule                  `json:"rules"`
	Provenance  []EffectiveRuleIdentity `json:"provenance"`
	Diagnostics []Diagnostic            `json:"diagnostics"`
}

func policyDiagnostic(scope string, index int, code, message string) Diagnostic {
	return Diagnostic{Scope: scope, Index: index, Code: code, Message: message}
}

func validPolicyID(id string) bool {
	if len(id) == 0 || len(id) > MaxPolicyIDBytes {
		return false
	}
	for i := range id {
		c := id[i]
		if !(c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' || c == '-' || c == '_' || c == '.') {
			return false
		}
	}
	return true
}

func validPolicyText(text string, limit int, multiline bool) bool {
	if len(text) > limit*utf8.UTFMax || !utf8.ValidString(text) || utf8.RuneCountInString(text) > limit {
		return false
	}
	for _, r := range text {
		if unicode.IsControl(r) && !(multiline && (r == '\n' || r == '\t')) {
			return false
		}
	}
	return true
}

// ruleProblem applies the existing private validator, then narrows fields that
// it does not constrain (keyword controls and inappropriate no-resolve).
// All reasons are fixed text and never include the rejected value.
func ruleProblem(r Rule) (string, string) {
	switch r.Kind {
	case RuleDomain, RuleDomainSuffix, RuleDomainKeyword, RuleIPCIDR, RuleSet, RuleMatch:
	default:
		return "unsupported-matcher", "rule matcher is not supported for forwarded clients"
	}
	if r.Kind == RuleSet && r.Value != "cn-domain" && r.Value != "cn-ip" && r.Value != "proxy-domain" {
		return "unsupported-rule-set", "rule set is not a controlled local set"
	}
	if validateRule(r) != nil {
		return "invalid-rule", "rule matcher value or action is invalid"
	}
	if len(r.Value) > MaxRuleValueBytes || r.NoResolve && r.Kind != RuleIPCIDR && !(r.Kind == RuleSet && r.Value == "cn-ip") {
		return "invalid-rule", "rule value or option exceeds supported limits"
	}
	if r.Kind == RuleDomainKeyword {
		for i := range r.Value {
			if r.Value[i] < 0x21 || r.Value[i] > 0x7e {
				return "invalid-rule", "rule keyword requires printable ASCII without spaces"
			}
		}
	}
	return "", ""
}

func validSourceFingerprint(ref string) bool {
	// Full normalized semantic SHA256, followed by a 1-based occurrence.
	if len(ref) < 66 || len(ref) > 69 || ref[64] != ':' {
		return false
	}
	for _, c := range ref[:64] {
		if !(c >= '0' && c <= '9' || c >= 'a' && c <= 'f') {
			return false
		}
	}
	occurrence, err := strconv.Atoi(ref[65:])
	return err == nil && occurrence >= 1 && occurrence <= MaxRules && strconv.Itoa(occurrence) == ref[65:]
}

func policyProblems(p Policy) []Diagnostic {
	problems := []Diagnostic{}
	if len(p.Rules) > MaxLocalRules || len(p.SubscriptionEdits) > MaxSubscriptionEdits {
		return append(problems, policyDiagnostic("policy", -1, "policy-limit", "local rule or subscription edit limit exceeded"))
	}
	ids := make(map[string]bool, len(p.Rules)+len(p.SubscriptionEdits))
	metadata := func(scope string, index int, id, label, note string) {
		if !validPolicyID(id) || ids[id] {
			problems = append(problems, policyDiagnostic(scope, index, "invalid-id", "rule identity must be unique bounded ASCII"))
		}
		ids[id] = true
		if !validPolicyText(label, MaxRuleLabelRunes, false) || !validPolicyText(note, MaxRuleNoteRunes, true) {
			problems = append(problems, policyDiagnostic(scope, index, "invalid-metadata", "rule label or note exceeds supported limits"))
		}
	}
	for i, local := range p.Rules {
		metadata("local-rule", i, local.ID, local.Label, local.Note)
		if code, reason := ruleProblem(local.Rule); code != "" {
			problems = append(problems, policyDiagnostic("local-rule", i, code, reason))
		}
	}
	refs := make(map[string]bool, len(p.SubscriptionEdits))
	for i, edit := range p.SubscriptionEdits {
		metadata("subscription-edit", i, edit.ID, edit.Label, edit.Note)
		if !validSourceFingerprint(edit.SourceFingerprint) || refs[edit.SourceFingerprint] {
			problems = append(problems, policyDiagnostic("subscription-edit", i, "invalid-reference", "subscription reference must be exact and unique"))
		}
		refs[edit.SourceFingerprint] = true
		if edit.Disabled == (edit.Replacement != nil) {
			problems = append(problems, policyDiagnostic("subscription-edit", i, "invalid-edit", "subscription edit must disable or replace one rule"))
		}
		if edit.Replacement != nil {
			if code, reason := ruleProblem(*edit.Replacement); code != "" {
				problems = append(problems, policyDiagnostic("subscription-edit", i, code, reason))
			}
		}
	}
	return problems
}

// ValidatePolicy validates inactive rules and orphan references too. Merge
// returns the same safe reasons for preview; neither function performs I/O.
func ValidatePolicy(p Policy) error {
	if len(policyProblems(p)) != 0 {
		return ErrInvalidPolicy
	}
	return nil
}

// ClonePolicy gives the store and API independent ownership, including edit
// replacement pointers. Empty arrays are canonical [] rather than JSON null.
func ClonePolicy(p Policy) Policy {
	out := Policy{Rules: append([]LocalRule{}, p.Rules...), SubscriptionEdits: append([]SubscriptionEdit{}, p.SubscriptionEdits...)}
	for i, edit := range out.SubscriptionEdits {
		if edit.Replacement != nil {
			rule := *edit.Replacement
			out.SubscriptionEdits[i].Replacement = &rule
		}
	}
	return out
}

type semanticRule struct {
	Kind      RuleKind `json:"kind"`
	Value     string   `json:"value"`
	Target    Target   `json:"target"`
	NoResolve bool     `json:"noResolve"`
}

func normalizedRule(r Rule) semanticRule {
	value := r.Value
	switch r.Kind {
	case RuleDomain, RuleDomainSuffix, RuleDomainKeyword:
		value = strings.ToLower(value)
	case RuleIPCIDR:
		prefix, _ := netip.ParsePrefix(value) // called only after validation
		value = prefix.Masked().String()
	}
	return semanticRule{r.Kind, value, r.Target, r.NoResolve}
}

func hashPolicyValue(value any) string {
	raw, _ := json.Marshal(value) // only validated concrete structs and slices
	hash := sha256.Sum256(raw)
	return hex.EncodeToString(hash[:])
}

// PolicyRevision hashes typed ordered policy content, without source Index.
// It is a dirty/readback identity, not a schema version or concurrency protocol.
func PolicyRevision(p Policy) (string, error) {
	if err := ValidatePolicy(p); err != nil {
		return "", err
	}
	type localIdentity struct {
		ID      string       `json:"id"`
		Enabled bool         `json:"enabled"`
		Label   string       `json:"label"`
		Note    string       `json:"note"`
		Rule    semanticRule `json:"rule"`
	}
	type editIdentity struct {
		ID                string        `json:"id"`
		SourceFingerprint string        `json:"sourceFingerprint"`
		Disabled          bool          `json:"disabled"`
		Label             string        `json:"label"`
		Note              string        `json:"note"`
		Replacement       *semanticRule `json:"replacement"`
	}
	identity := struct {
		Rules []localIdentity `json:"rules"`
		Edits []editIdentity  `json:"subscriptionEdits"`
	}{Rules: []localIdentity{}, Edits: []editIdentity{}}
	for _, r := range p.Rules {
		identity.Rules = append(identity.Rules, localIdentity{r.ID, r.Enabled, r.Label, r.Note, normalizedRule(r.Rule)})
	}
	for _, e := range p.SubscriptionEdits {
		entry := editIdentity{ID: e.ID, SourceFingerprint: e.SourceFingerprint, Disabled: e.Disabled, Label: e.Label, Note: e.Note}
		if e.Replacement != nil {
			rule := normalizedRule(*e.Replacement)
			entry.Replacement = &rule
		}
		identity.Edits = append(identity.Edits, entry)
	}
	return hashPolicyValue(identity), nil
}

// SubscriptionFingerprints excludes Rule.Index. Equal normalized semantics
// get distinct references by occurrence in source order, not global position.
func SubscriptionFingerprints(subscription []Rule) ([]string, error) {
	if len(subscription) > MaxRules {
		return nil, ErrInvalidPolicy
	}
	refs := make([]string, len(subscription))
	occurrences := make(map[string]int, len(subscription))
	for i, r := range subscription {
		if code, _ := ruleProblem(r); code != "" {
			return nil, ErrInvalidPolicy
		}
		hash := hashPolicyValue(normalizedRule(r))
		occurrences[hash]++
		refs[i] = hash + ":" + strconv.Itoa(occurrences[hash])
	}
	return refs, nil
}

// MergeEffectivePolicy is pure and owns its output. It prepends enabled local
// rules, then preserves subscription order and exact Rules unless explicitly
// edited. Management/bootstrap bypass is a compiler prelude OUTSIDE this list;
// callers must keep CompileNative's mandatory prelude ahead of this user policy.
// Rules after a terminal MATCH remain in preview but compile as unreachable.
func MergeEffectivePolicy(subscription []Rule, policy Policy) (EffectivePolicy, error) {
	out := EffectivePolicy{Rules: []Rule{}, Provenance: []EffectiveRuleIdentity{}, Diagnostics: policyProblems(policy)}
	if len(out.Diagnostics) != 0 {
		return out, ErrInvalidPolicy
	}
	if len(subscription) > MaxRules {
		out.Diagnostics = append(out.Diagnostics, policyDiagnostic("subscription", -1, "subscription-limit", "subscription rule limit exceeded"))
		return out, ErrInvalidPolicy
	}
	for i, r := range subscription {
		if code, reason := ruleProblem(r); code != "" {
			out.Diagnostics = append(out.Diagnostics, policyDiagnostic("subscription", i, code, reason))
		}
	}
	if len(out.Diagnostics) != 0 {
		return out, ErrInvalidPolicy
	}
	refs, _ := SubscriptionFingerprints(subscription)
	edits := make(map[string]int, len(policy.SubscriptionEdits))
	for i, e := range policy.SubscriptionEdits {
		edits[e.SourceFingerprint] = i
	}
	used := make([]bool, len(policy.SubscriptionEdits))
	terminal := false
	appendRule := func(r Rule, identity EffectiveRuleIdentity) {
		identity.EffectiveIndex = len(out.Rules)
		identity.Kind, identity.Value, identity.Target = r.Kind, r.Value, r.Target
		if terminal {
			out.Diagnostics = append(out.Diagnostics, policyDiagnostic("effective-rule", identity.EffectiveIndex, "unreachable-rule", "rule follows terminal MATCH and is unreachable"))
		}
		out.Rules = append(out.Rules, r)
		out.Provenance = append(out.Provenance, identity)
		if r.Kind == RuleMatch {
			terminal = true
		}
	}
	for i, local := range policy.Rules {
		if !local.Enabled {
			out.Diagnostics = append(out.Diagnostics, policyDiagnostic("local-rule", i, "disabled-rule", "disabled local rule is not compiled"))
			continue
		}
		appendRule(local.Rule, EffectiveRuleIdentity{Layer: "local", StableID: local.ID, Label: local.Label, SourceIndex: -1, SourceOrdinal: i})
	}
	for i, original := range subscription {
		rule := original
		identity := EffectiveRuleIdentity{Layer: "subscription", StableID: refs[i], SourceFingerprint: refs[i], SourceIndex: original.Index, SourceOrdinal: i}
		if editIndex, exists := edits[refs[i]]; exists {
			used[editIndex] = true
			edit := policy.SubscriptionEdits[editIndex]
			if edit.Disabled {
				out.Diagnostics = append(out.Diagnostics, policyDiagnostic("subscription-edit", editIndex, "disabled-rule", "explicitly disabled subscription rule is not compiled"))
				continue
			}
			rule = *edit.Replacement
			rule.Index = original.Index
			identity.StableID, identity.Label = edit.ID, edit.Label
		}
		appendRule(rule, identity)
	}
	for i, matched := range used {
		if !matched {
			out.Diagnostics = append(out.Diagnostics, policyDiagnostic("subscription-edit", i, "orphaned-edit", "subscription edit reference is absent and remains inactive"))
		}
	}
	return out, nil
}

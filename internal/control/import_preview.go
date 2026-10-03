package control

import (
	"sort"
	"strings"
)

// PreviewDocuments validates a selected native candidate set without creating
// drafts, temporary files, running UCI or changing the live configuration.
// Stage and Commit still own native execution checks and authoritative CAS.
// Unknown vendor options and comments remain untouched in the original text.
func PreviewDocuments(current, candidates []Document) []Draft {
	live := map[string]string{}
	combined := map[string]string{}
	for _, document := range current {
		live[document.Module] = document.Content
		combined[document.Module] = document.Content
	}
	for _, document := range candidates {
		combined[document.Module] = document.Content
	}
	out := make([]Draft, 0, len(candidates))
	seen := map[string]bool{}
	for _, document := range candidates {
		issues := []Issue{}
		dependencies := []Issue{}
		switch {
		case !allowed(document.Module):
			issues = append(issues, issue("module_not_allowed", "This native configuration module is not editable."))
		case seen[document.Module]:
			issues = append(issues, issue("duplicate_module", "Select only one document for each native module."))
		case len(document.Content) > MaxDocumentBytes:
			issues = append(issues, issue("document_too_large", "Native configuration exceeds the document limit."))
		default:
			_, issues = validate(document.Module, document.Content)
			if len(issues) == 0 {
				issues = validateExecutionChanges(document.Module, live[document.Module], document.Content)
			}
			if len(issues) == 0 {
				dependencies = validateReferences(map[string]string{document.Module: document.Content}, combined)
				if document.Module == "network" {
					dependencies = append(dependencies, importReverseReferences(live, combined, candidates)...)
				}
			}
		}
		seen[document.Module] = true
		importSortIssues(issues)
		importSortIssues(dependencies)
		risks := risk(document.Module, live[document.Module], document.Content)
		importSortIssues(risks)
		out = append(out, Draft{Module: document.Module, Diff: diff(document.Module, live[document.Module], document.Content), Risks: risks, Valid: len(issues) == 0, Errors: append([]Issue{}, issues...), Dependencies: append([]Issue{}, dependencies...)})
	}
	return out
}

// A selected network edit must not break an unchanged document that points to
// an interface it removes. Do not reject pre-existing unknown vendor references
// in untouched documents, but detect each reference that this bundle breaks.
func importReverseReferences(live, combined map[string]string, candidates []Document) []Issue {
	selected := map[string]bool{}
	for _, document := range candidates {
		selected[document.Module] = true
	}
	interfaces := func(text string) map[string]bool {
		result := map[string]bool{}
		sections, _ := parse(text)
		for _, section := range sections {
			if section.kind == "interface" && section.name != "" {
				result[section.name] = true
			}
		}
		return result
	}
	before, after := interfaces(live["network"]), interfaces(combined["network"])
	issues := []Issue{}
	for _, module := range []string{"wireless", "dhcp", "firewall"} {
		if selected[module] {
			continue
		}
		sections, _ := parse(live[module])
		broken := false
		for _, section := range sections {
			key := ""
			switch {
			case module == "wireless" && section.kind == "wifi-iface":
				key = "network"
			case module == "dhcp" && section.kind == "dhcp":
				key = "interface"
			case module == "firewall" && section.kind == "zone":
				key = "network"
			}
			for _, value := range section.values[key] {
				for _, name := range strings.Fields(value) {
					if before[name] && !after[name] {
						broken = true
					}
				}
			}
		}
		if broken {
			issues = append(issues, issue("invalid_reference", "Network changes would break current "+module+" references; select its matching configuration document."))
		}
	}
	return issues
}

func importSortIssues(issues []Issue) {
	sort.SliceStable(issues, func(i, j int) bool {
		if issues[i].Code != issues[j].Code {
			return issues[i].Code < issues[j].Code
		}
		return issues[i].Message < issues[j].Message
	})
}

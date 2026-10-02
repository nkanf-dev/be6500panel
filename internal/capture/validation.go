package capture

import (
	"errors"
	"fmt"
	"reflect"
	"slices"
	"strconv"
	"strings"

	"be6500panel/internal/proxy"
)

// Input is persisted with new journals. Embedded fields keep older journals
// readable; stored Apply is never executed after a restart.
type journal struct {
	proxy.OwnedRulesPlan
	Input *proxy.RulesPlanInput `json:"input,omitempty"`
}

func recoveredPlan(stored journal) (proxy.OwnedRulesPlan, error) {
	ownership := stored.Ownership
	input := proxy.RulesPlanInput{ClientIPv4: ownership.ClientIPv4, ClientIPv6: ownership.ClientIPv6, LANInterface: ownership.LANInterface, IPv6: proxy.IPv6Direct}
	if stored.Input != nil {
		input = *stored.Input
	} else {
		for _, chain := range ownership.Chains {
			if chain.Family == 6 {
				if chain.Name == "B6P_V6_BLOCK" {
					input.IPv6 = proxy.IPv6Block
				} else {
					input.IPv6 = proxy.IPv6Follow
				}
			}
		}
	}
	plan, err := proxy.PlanOwnedRules(input)
	if err != nil {
		return proxy.OwnedRulesPlan{}, fmt.Errorf("capture journal input invalid: %w", err)
	}
	if !reflect.DeepEqual(ownership, plan.Ownership) || !reflect.DeepEqual(stored.Cleanup, plan.Cleanup) {
		return proxy.OwnedRulesPlan{}, errors.New("capture journal does not match fixed compiled ownership and cleanup")
	}
	return plan, nil
}

func clonePlan(plan proxy.OwnedRulesPlan) proxy.OwnedRulesPlan {
	cloneCommands := func(commands [][]string) [][]string {
		result := make([][]string, len(commands))
		for i, argv := range commands {
			result[i] = slices.Clone(argv)
		}
		return result
	}
	plan.Apply = cloneCommands(plan.Apply)
	plan.Cleanup = cloneCommands(plan.Cleanup)
	plan.OnFailure = cloneCommands(plan.OnFailure)
	plan.Warnings = slices.Clone(plan.Warnings)
	plan.Ownership.RouteFamilies = slices.Clone(plan.Ownership.RouteFamilies)
	plan.Ownership.Chains = slices.Clone(plan.Ownership.Chains)
	return plan
}

// validate checks the fixed read-only inspection commands. Mutations are only
// accepted by approvedCommand when they exactly match the internally compiled
// plan; there is no second hand-written firewall rule compiler here.
func validate(a []string) error {
	if err := validateArgs(a); err != nil {
		return err
	}
	if a[0] == "ip" && len(a) >= 2 && (a[1] == "-4" || a[1] == "-6") {
		if slices.Equal(a[2:], []string{"route", "show", "table", strconv.Itoa(proxy.CaptureTable)}) || slices.Equal(a[2:], []string{"route", "show", "table", "all"}) || slices.Equal(a[2:], []string{"rule", "show"}) {
			return nil
		}
	}
	family := 4
	if a[0] == "ip6tables" {
		family = 6
	}
	if (a[0] == "iptables" || a[0] == "ip6tables") && len(a) >= 6 && slices.Equal(a[1:4], []string{"-w", "5", "-t"}) && a[5] == "-S" {
		if len(a) == 6 && a[4] == "mangle" {
			return nil
		}
		if len(a) == 7 && ownedChain(family, a[4], a[6]) {
			return nil
		}
	}
	return errors.New("command is not fixed read-only capture inspection")
}
func validateArgs(a []string) error {
	if len(a) < 2 || (a[0] != "ip" && a[0] != "iptables" && a[0] != "ip6tables") {
		return errors.New("unapproved capture executable")
	}
	for _, arg := range a {
		if strings.ContainsAny(arg, "\x00\r\n") || len(arg) > 256 {
			return errors.New("invalid capture argument")
		}
	}
	return nil
}
func (c *Controller) approvedCommand(a []string) error {
	if err := validateArgs(a); err != nil {
		return err
	}
	if validate(a) == nil {
		return nil
	}
	if c.plan != nil {
		for _, commands := range [][][]string{c.plan.Apply, c.plan.Cleanup} {
			for _, cmd := range commands {
				if slices.Equal(cmd, a) {
					return nil
				}
			}
		}
		for _, cmd := range c.plan.Apply {
			if len(cmd) <= 6 || (cmd[5] != "-A" && cmd[5] != "-I") {
				continue
			}
			check := slices.Clone(cmd)
			check[5] = "-C"
			if cmd[5] == "-I" {
				check = append(check[:7], check[8:]...)
			}
			if slices.Equal(check, a) {
				return nil
			}
		}
	}
	return errors.New("command does not match internally compiled capture intent")
}
func ownedChain(family int, table, name string) bool {
	if table == "mangle" {
		return name == "B6P_V"+strconv.Itoa(family)+"_CAPTURE"
	}
	if table == "nat" {
		return name == "B6P_V"+strconv.Itoa(family)+"_DNS"
	}
	return family == 6 && table == "filter" && name == "B6P_V6_BLOCK"
}
func isReadCommand(a []string) bool {
	if len(a) > 3 && a[0] == "ip" {
		return a[3] == "show"
	}
	return len(a) > 5 && (a[5] == "-S" || a[5] == "-C")
}

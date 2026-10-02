package httpapi

import (
	"be6500panel/internal/core"
	managedruntime "be6500panel/internal/runtime"
	"strings"
	"testing"
)

func TestModuleListReflectsRuntimeDeploymentWithoutObsoleteReasons(t *testing.T) {
	for _, enabled := range []bool{false, true} {
		s, ts := testServer(t, "")
		if enabled {
			s.runtime = &managedruntime.Manager{}
		}
		for _, module := range s.ModuleList() {
			if module.ID != "proxy" && module.ID != "frpc" {
				continue
			}
			for _, cap := range module.Capabilities {
				if cap.ID == "apply" || cap.ID == "observe" {
					if cap.Supported != enabled {
						t.Errorf("%s/%s support=%v want %v", module.ID, cap.ID, cap.Supported, enabled)
					}
					if enabled && strings.Contains(cap.Reason, "not integrated") {
						t.Errorf("obsolete capability: %+v", cap)
					}
					if enabled && cap.Reason != "" {
						t.Errorf("supported capability still has failure reason: %+v", cap)
					}
				}
			}
		}
		s.Close()
		ts.Close()
	}
}

func TestModuleListDoesNotMutateRegisteredCapabilities(t *testing.T) {
	s, ts := testServer(t, "")
	defer ts.Close()
	defer s.Close()
	before := s.registry.Modules()
	s.runtime = &managedruntime.Manager{}
	s.ModuleList()
	after := s.registry.Modules()
	find := func(items []core.Module) core.Module {
		for _, m := range items {
			if m.ID == "frpc" {
				return m
			}
		}
		return core.Module{}
	}
	a, b := find(before), find(after)
	for i := range a.Capabilities {
		if a.Capabilities[i] != b.Capabilities[i] {
			t.Fatal("ModuleList modified registry")
		}
	}
}

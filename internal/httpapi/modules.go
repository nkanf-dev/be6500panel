package httpapi

import (
	"be6500panel/internal/core"
)

// ModuleList reports the active deployment's capability seams without replacing domain ownership.
func (s *Server) ModuleList() []core.Module {
	modules := s.registry.Modules()
	for i := range modules {
		m := &modules[i]
		switch m.ID {
		case "devices", "wifi", "dns", "firewall":
			if s.router != nil {
				m.State = "ready"
				m.Capabilities = []core.Capability{{ID: "observe", Title: "设备观察", Supported: true}}
				if m.ID != "devices" {
					m.Capabilities = append(m.Capabilities, core.Capability{ID: "configure", Title: "配置 Commit", Supported: s.control != nil})
				}
			}
		case "network":
			if s.control != nil {
				m.Capabilities = append(m.Capabilities, core.Capability{ID: "configure", Title: "配置 Commit", Supported: true})
			}
		case "system":
			if s.control != nil {
				m.Capabilities = append(m.Capabilities, core.Capability{ID: "configure", Title: "配置 Commit", Supported: true})
			}
		case "proxy", "frpc":
			if s.runtime != nil {
				m.Capabilities = append(m.Capabilities, core.Capability{ID: "runtime", Title: "运行控制", Supported: true})
			}
		}
	}
	return modules
}

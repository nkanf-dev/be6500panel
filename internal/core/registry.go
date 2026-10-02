package core

import "fmt"

// Registry is configured once before serving. Descriptors are returned as copies.
type Registry struct{ modules []Module }

func (r *Registry) Register(provider ModuleProvider) error {
	m := provider.Descriptor()
	if m.ID == "" || m.Title == "" || (m.State != "ready" && m.State != "unavailable") {
		return fmt.Errorf("invalid module descriptor")
	}
	for _, existing := range r.modules {
		if existing.ID == m.ID {
			return fmt.Errorf("duplicate module %q", m.ID)
		}
	}
	ids := map[string]bool{}
	for _, c := range m.Capabilities {
		if c.ID == "" || c.Title == "" || ids[c.ID] || (!c.Supported && c.Reason == "") {
			return fmt.Errorf("invalid capability in %q", m.ID)
		}
		ids[c.ID] = true
	}
	m.Capabilities = append([]Capability{}, m.Capabilities...)
	r.modules = append(r.modules, m)
	return nil
}
func (r *Registry) Modules() []Module {
	out := append([]Module{}, r.modules...)
	for i := range out {
		out[i].Capabilities = append([]Capability{}, out[i].Capabilities...)
	}
	return out
}

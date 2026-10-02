package httpapi

import (
	"be6500panel/internal/proxy"
	managedruntime "be6500panel/internal/runtime"
	"encoding/json"
)

func (s *Server) recordedNodeMatchesAccepted(nodeID string, nodes []proxy.Node) bool {
	if nodeID == "" || s.runtime == nil {
		return false
	}
	raw, _, err := s.runtime.Config(managedruntime.SingBox)
	if err != nil {
		return false
	}
	var config struct {
		Outbounds []struct {
			Tag    string `json:"tag"`
			Type   string `json:"type"`
			Server string `json:"server"`
			Port   uint16 `json:"server_port"`
			UUID   string `json:"uuid"`
		} `json:"outbounds"`
	}
	if json.Unmarshal(raw, &config) != nil {
		return false
	}
	for _, node := range nodes {
		if node.ID != nodeID {
			continue
		}
		for _, outbound := range config.Outbounds {
			if outbound.Tag == "proxy" && outbound.Type == "vless" && outbound.Server == node.Server && outbound.Port == node.Port && outbound.UUID == node.UUID {
				return true
			}
		}
	}
	return false
}

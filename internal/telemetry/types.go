// Package telemetry reads a bounded, sanitized view of a localhost-only core API.
// It does not change proxy configuration or expose the core API to the browser.
package telemetry

import "time"

const (
	Source            = "sing-box Clash API · localhost"
	MaxConnections    = 128
	MaxTrafficSamples = 900
	MaxProbes         = 32
	MaxResponseBytes  = 1 << 20
	ProbeURL          = "https://www.gstatic.com/generate_204"
)

type Capability struct {
	Available bool   `json:"available"`
	Reason    string `json:"reason"`
}
type Capabilities struct {
	Connections   Capability `json:"connections"`
	Traffic       Capability `json:"traffic"`
	Routing       Capability `json:"routing"`
	Latency       Capability `json:"latency"`
	RequestPhases Capability `json:"requestPhases"`
}
type Totals struct {
	UploadBytes   uint64 `json:"uploadBytes"`
	DownloadBytes uint64 `json:"downloadBytes"`
}
type Connection struct {
	ID              string    `json:"id"`
	StartedAt       time.Time `json:"startedAt"`
	AgeMS           int64     `json:"ageMs"`
	Network         string    `json:"network"`
	SourceIP        string    `json:"sourceIP"`
	SourcePort      uint16    `json:"sourcePort"`
	DestinationIP   string    `json:"destinationIP"`
	DestinationPort uint16    `json:"destinationPort"`
	Host            string    `json:"host"`
	UploadBytes     uint64    `json:"uploadBytes"`
	DownloadBytes   uint64    `json:"downloadBytes"`
	Outbound        string    `json:"outbound"`
	RuleID          string    `json:"ruleId"`
	Rule            string    `json:"rule"`
}
type TrafficSample struct {
	Time         time.Time `json:"time"`
	UploadRate   float64   `json:"uploadRate"`
	DownloadRate float64   `json:"downloadRate"`
	Reset        bool      `json:"reset"`
}
type Probe struct {
	Time    time.Time `json:"time"`
	DelayMS uint16    `json:"delayMs"`
	Status  string    `json:"status"`
}
type Snapshot struct {
	State             string          `json:"state"`
	Reason            string          `json:"reason"`
	Source            string          `json:"source"`
	SampledAt         *time.Time      `json:"sampledAt,omitempty"`
	Capabilities      Capabilities    `json:"capabilities"`
	Totals            Totals          `json:"totals"`
	ActiveConnections int             `json:"activeConnections"`
	Truncated         bool            `json:"truncated"`
	Connections       []Connection    `json:"connections"`
	Traffic           []TrafficSample `json:"traffic"`
	Probes            []Probe         `json:"probes"`
}

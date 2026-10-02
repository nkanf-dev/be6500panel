// Package core owns shared contracts, module registration and observation transport.
package core

import "time"

type Capability struct {
	ID        string `json:"id"`
	Title     string `json:"title"`
	Supported bool   `json:"supported"`
	Reason    string `json:"reason,omitempty"`
}
type Module struct {
	ID           string       `json:"id"`
	Title        string       `json:"title"`
	Description  string       `json:"description"`
	State        string       `json:"state"`
	Capabilities []Capability `json:"capabilities"`
}
type ModuleProvider interface{ Descriptor() Module }

type Memory struct {
	TotalBytes     uint64 `json:"totalBytes"`
	AvailableBytes uint64 `json:"availableBytes"`
}
type SystemStatus struct {
	Mode          string     `json:"mode"`
	Hostname      string     `json:"hostname"`
	OS            string     `json:"os"`
	Arch          string     `json:"arch"`
	Kernel        string     `json:"kernel"`
	UptimeSeconds float64    `json:"uptimeSeconds"`
	CPUCount      int        `json:"cpuCount"`
	Memory        Memory     `json:"memory"`
	Load          [3]float64 `json:"load"`
	SampledAt     time.Time  `json:"sampledAt"`
}
type Interface struct {
	Name      string   `json:"name"`
	Addresses []string `json:"addresses"`
	Up        bool     `json:"up"`
	MTU       int      `json:"mtu"`
}
type NetworkStatus struct {
	Interfaces                []Interface `json:"interfaces"`
	Routes                    []any       `json:"routes"`
	RouteObservationSupported bool        `json:"routeObservationSupported"`
}
type PlanStep struct {
	Module string `json:"module"`
	Action string `json:"action"`
	Detail string `json:"detail"`
}
type OperationPlan struct {
	ID         string     `json:"id"`
	Generation uint64     `json:"generation"`
	ReadOnly   bool       `json:"readOnly"`
	Summary    string     `json:"summary"`
	Steps      []PlanStep `json:"steps"`
	Warnings   []string   `json:"warnings"`
	CanApply   bool       `json:"canApply"`
}
type Snapshot struct {
	System    SystemStatus `json:"system"`
	SampledAt time.Time    `json:"sampledAt"`
}

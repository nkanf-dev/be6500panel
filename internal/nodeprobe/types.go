// Package nodeprobe owns isolated, explicit, bounded HTTPS node probes. It
// never changes the accepted configuration, active core, router or capture.
package nodeprobe

import (
	"be6500panel/internal/proxy"
	"context"
	"errors"
	"time"
)

const (
	Target      = "https://www.gstatic.com/generate_204"
	MaxNodes    = proxy.MaxProbeNodes
	Concurrency = 1
	Timeout     = 3 * time.Second
	JobTimeout  = 15 * time.Minute
)

var (
	ErrBusy        = errors.New("probe_busy")
	ErrRevision    = errors.New("revision_mismatch")
	ErrNodes       = errors.New("invalid_nodes")
	ErrUnavailable = errors.New("artifact_unavailable")
	ErrClosed      = errors.New("probe_closed")
)

type NodeSet struct {
	Revision string
	Nodes    []proxy.Node
}

// CoreLease must name a checksum-proven frozen binary, privately leased for the
// entire job. AcquireLease must not start/stop or modify the active core. Release
// is called exactly once after owned process cleanup, including failed startup.
type CoreLease struct {
	Path    string `json:"-"`
	Release func() `json:"-"`
}

func (CoreLease) String() string     { return "private frozen probe core lease" }
func (l CoreLease) GoString() string { return l.String() }

type Config struct {
	Nodes        func() (NodeSet, error)
	AcquireLease func(context.Context) (CoreLease, error)
	// Availability is read-only; empty means available. It never leases or probes.
	Availability func() string
	TempDir      string
}
type StartInput struct {
	All      bool     `json:"all"`
	NodeIDs  []string `json:"nodeIds"`
	Revision string   `json:"revision"`
}
type Result struct {
	NodeID     string     `json:"nodeId"`
	Status     string     `json:"status"`
	DelayMS    *int64     `json:"delayMs,omitempty"`
	MeasuredAt *time.Time `json:"measuredAt,omitempty"`
	Target     string     `json:"target"`
	ErrorCode  string     `json:"errorCode,omitempty"`
}
type Job struct {
	ID         string     `json:"id"`
	Status     string     `json:"status"`
	Total      int        `json:"total"`
	Completed  int        `json:"completed"`
	StartedAt  time.Time  `json:"startedAt"`
	FinishedAt *time.Time `json:"finishedAt,omitempty"`
	ErrorCode  string     `json:"errorCode,omitempty"`
}
type Limits struct {
	MaxNodes    int   `json:"maxNodes"`
	Concurrency int   `json:"concurrency"`
	TimeoutMS   int64 `json:"timeoutMs"`
}
type Snapshot struct {
	Revision        string   `json:"revision"`
	Available       bool     `json:"available"`
	UnavailableCode string   `json:"unavailableCode,omitempty"`
	Target          string   `json:"target"`
	Running         bool     `json:"running"`
	Job             *Job     `json:"job,omitempty"`
	Results         []Result `json:"results"`
	Limits          Limits   `json:"limits"`
}

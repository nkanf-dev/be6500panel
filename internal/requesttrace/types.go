// Package requesttrace records opt-in, fixed-target HTTP diagnostics. It never
// captures client traffic, decrypts user HTTPS, changes routing, or probes on its own.
package requesttrace

import (
	"context"
	"errors"
	"time"
)

const (
	Timeout            = 10 * time.Second
	MaxBodyBytes int64 = 64 << 10
	Capacity           = 64
)

var (
	ErrBusy        = errors.New("request diagnostic already running")
	ErrInput       = errors.New("invalid diagnostic target or route")
	ErrUnavailable = errors.New("accepted proxy listener unavailable")
)

type Input struct {
	TargetID string `json:"targetId"`
	Route    string `json:"route"`
}
type Target struct {
	ID    string `json:"id"`
	Label string `json:"label"`
	URL   string `json:"url"`
}
type Phase struct {
	ID         string   `json:"id"`
	Observed   bool     `json:"observed"`
	StartMS    *float64 `json:"startMs"`
	EndMS      *float64 `json:"endMs"`
	DurationMS *float64 `json:"durationMs"`
	Reason     string   `json:"reason,omitempty"`
}
type Trace struct {
	ID               string    `json:"id"`
	TargetID         string    `json:"targetId"`
	TargetLabel      string    `json:"targetLabel"`
	URL              string    `json:"url"`
	Route            string    `json:"route"`
	StartedAt        time.Time `json:"startedAt"`
	FinishedAt       time.Time `json:"finishedAt"`
	TotalMS          float64   `json:"totalMs"`
	Outcome          string    `json:"outcome"`
	StatusCode       *int      `json:"statusCode"`
	BytesRead        int64     `json:"bytesRead"`
	BodyLimitReached bool      `json:"bodyLimitReached"`
	PeerAddress      *string   `json:"peerAddress"`
	PeerScope        string    `json:"peerScope"`
	FailurePhase     *string   `json:"failurePhase"`
	ErrorCode        string    `json:"errorCode,omitempty"`
	Phases           []Phase   `json:"phases"`
}
type Limits struct {
	TimeoutMS   int64 `json:"timeoutMs"`
	BodyBytes   int64 `json:"bodyBytes"`
	Concurrency int   `json:"concurrency"`
	Capacity    int   `json:"capacity"`
}
type Snapshot struct {
	Traces  []Trace  `json:"traces"`
	Targets []Target `json:"targets"`
	Limits  Limits   `json:"limits"`
	Running bool     `json:"running"`
}

// ProxyProvider must read the accepted, currently running native configuration
// privately. It must not accept browser input or use a configured default port
// without checking that accepted document. Call MixedProxyFromNative with LAN
// addresses verified from the router, never from the HTTP request.
type ProxyProvider func(context.Context) (ProxyEndpoint, error)
type Config struct{ ProxyProvider ProxyProvider }

var presets = []Target{
	{ID: "google204", Label: "Google 204", URL: "https://www.gstatic.com/generate_204"},
	{ID: "cloudflare", Label: "Cloudflare", URL: "https://www.cloudflare.com/cdn-cgi/trace"},
}

// Package devicetelemetry collects bounded recent per-device counter history.
// It uses memory only: no flash allocation and no claim of persistent history.
package devicetelemetry

import (
	"context"
	"errors"
	"time"
)

const (
	MaxDevices            = 128
	MaxAddresses          = 16
	MaxSourceRows         = 512
	MaxSourceBytes        = 512 << 10
	MaxPoints             = 288
	MaxQueryDevices       = 64
	RetentionDays         = 7
	SampleInterval        = 15 * time.Second
	StaleAfter            = 60 * time.Second
	fineSeconds     int64 = 300
	coarseSeconds   int64 = 3600
	fineCapacity          = 289 // 24 hours plus current bucket
	coarseCapacity        = 169 // 7 days plus current bucket
	// BucketMemoryBytes is the maximum fixed ring payload, excluding small
	// device metadata and bounded source/query allocations. No files are used.
	BucketMemoryBytes = MaxDevices * (fineCapacity + coarseCapacity) * 32
)

var ErrQuery = errors.New("use range 30m, 24h or 7d, maxPoints 1..288, limit 1..64 and search up to 64 characters")
var ErrClosed = errors.New("device telemetry is closed")
var ErrSource = errors.New("trafficd contains invalid or oversized device counters")

type Counter struct {
	Address string
	RX, TX  uint64
}
type CurrentCounter struct {
	Address string `json:"address"`
	RXBytes uint64 `json:"rxBytes"`
	TXBytes uint64 `json:"txBytes"`
}
type WirelessLink struct {
	Interface     string  `json:"interface"`
	Protocol      string  `json:"protocol,omitempty"`
	MLD           *bool   `json:"mld,omitempty"`
	SignalDBM     *int    `json:"signalDBM,omitempty"`
	NoiseDBM      *int    `json:"noiseDBM,omitempty"`
	NegotiatedRX  string  `json:"negotiatedRX,omitempty"`
	NegotiatedTX  string  `json:"negotiatedTX,omitempty"`
	AgeingSeconds *uint64 `json:"ageingSeconds,omitempty"`
}
type Observation struct {
	ID, Name, Interface string
	Associated          bool
	OnlineSeconds       *uint64
	AgeingSeconds       *uint64
	Links               []WirelessLink
	Counters            []Counter
}
type Snapshot struct {
	SampledAt time.Time
	Devices   []Observation
	Truncated bool
	Error     string
}
type Source interface {
	Snapshot(context.Context) (Snapshot, error)
}

type Point struct {
	Time            time.Time `json:"time"`
	RXBytes         *uint64   `json:"rxBytes"`
	TXBytes         *uint64   `json:"txBytes"`
	CoverageSeconds float64   `json:"coverageSeconds"`
}
type Device struct {
	ID               string           `json:"id"`
	Name             string           `json:"name"`
	Addresses        []string         `json:"addresses"`
	Interface        string           `json:"interface"`
	Associated       bool             `json:"associated"`
	LastSeen         time.Time        `json:"lastSeen"`
	Stale            bool             `json:"stale"`
	RXBytes          uint64           `json:"rxBytes"`
	TXBytes          uint64           `json:"txBytes"`
	CoverageSeconds  float64          `json:"coverageSeconds"`
	RXBytesPerSecond *float64         `json:"rxBytesPerSecond,omitempty"`
	TXBytesPerSecond *float64         `json:"txBytesPerSecond,omitempty"`
	RawRXBytes       *uint64          `json:"rawRXBytes,omitempty"`
	RawTXBytes       *uint64          `json:"rawTXBytes,omitempty"`
	OnlineSeconds    *uint64          `json:"onlineSeconds,omitempty"`
	AgeingSeconds    *uint64          `json:"ageingSeconds,omitempty"`
	Counters         []CurrentCounter `json:"counters"`
	Links            []WirelessLink   `json:"links"`
	AddressConflicts []string         `json:"addressConflicts"`
	Samples          []Point          `json:"samples"`
}
type Group struct {
	CoverageSeconds  float64  `json:"coverageSeconds"`
	Name             string   `json:"name"`
	DeviceCount      int      `json:"deviceCount"`
	RXBytes          uint64   `json:"rxBytes"`
	TXBytes          uint64   `json:"txBytes"`
	RXBytesPerSecond *float64 `json:"rxBytesPerSecond,omitempty"`
	TXBytesPerSecond *float64 `json:"txBytesPerSecond,omitempty"`
}
type History struct {
	Enabled           bool       `json:"enabled"`
	Persistent        bool       `json:"persistent"`
	RetentionDays     int        `json:"retentionDays"`
	Source            string     `json:"source"`
	Direction         string     `json:"direction"`
	Range             string     `json:"range"`
	ResolutionSeconds int64      `json:"resolutionSeconds"`
	State             string     `json:"state"`
	SampledAt         *time.Time `json:"sampledAt,omitempty"`
	OldestAt          *time.Time `json:"oldestAt,omitempty"`
	Error             string     `json:"error,omitempty"`
	DeviceCount       int        `json:"deviceCount"`
	MatchedCount      int        `json:"matchedCount"`
	Truncated         bool       `json:"truncated"`
	Devices           []Device   `json:"devices"`
	Groups            []Group    `json:"groups"`
}

func Disabled(name string) History {
	return History{Range: name, Source: "trafficd", Direction: "vendor-rx-tx", RetentionDays: RetentionDays, State: "unavailable", Devices: []Device{}, Groups: []Group{}, Error: "Device traffic collection is not enabled."}
}

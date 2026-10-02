// Package traffic stores a single real default-route WAN counter series in
// bounded, checksummed binary rings. Collection is independent of HTTP clients.
package traffic

import (
	"context"
	"errors"
	"time"

	"be6500panel/internal/router"
)

const (
	RetentionDays    = 400
	DefaultMaxPoints = 1500
	MaxPoints        = 2000
	SampleInterval   = 2 * time.Second
	FlushInterval    = time.Minute
	// DiskBytes is the exact fixed file budget, excluding filesystem metadata.
	DiskBytes = (5760+8640+9600)*128 + 3*64
)

var ErrRange = errors.New("unsupported traffic history range")
var ErrMaxPoints = errors.New("maxPoints must be between 1 and 2000")
var ErrClosed = errors.New("traffic history is closed")

var ranges = map[string]time.Duration{
	"30m": 30 * time.Minute, "3h": 3 * time.Hour, "6h": 6 * time.Hour,
	"1d": 24 * time.Hour, "7d": 7 * 24 * time.Hour, "30d": 30 * 24 * time.Hour,
	"180d": 180 * 24 * time.Hour, "1y": 365 * 24 * time.Hour,
}

// ValidateQuery rejects arbitrary time spans and bounds both memory and output.
func ValidateQuery(name string, maxPoints int) error {
	if _, ok := ranges[name]; !ok {
		return ErrRange
	}
	if maxPoints < 1 || maxPoints > MaxPoints {
		return ErrMaxPoints
	}
	return nil
}

type Source interface {
	Snapshot(context.Context) (router.Snapshot, error)
}

type Options struct {
	// DataDir is required. Failure never silently switches to volatile storage.
	DataDir string
	Source  Source
}

type Sample struct {
	Time            time.Time `json:"time"`
	RX              float64   `json:"rx"`
	TX              float64   `json:"tx"`
	RXPeak          float64   `json:"rxPeak"`
	TXPeak          float64   `json:"txPeak"`
	RXBytes         uint64    `json:"rxBytes"`
	TXBytes         uint64    `json:"txBytes"`
	CoverageSeconds float64   `json:"coverageSeconds"`
}

type Summary struct {
	RXBytes         uint64  `json:"rxBytes"`
	TXBytes         uint64  `json:"txBytes"`
	CoverageSeconds float64 `json:"coverageSeconds"`
}

type History struct {
	Enabled           bool       `json:"enabled"`
	Persistent        bool       `json:"persistent"`
	RetentionDays     int        `json:"retentionDays"`
	Source            string     `json:"source"`
	Range             string     `json:"range"`
	ResolutionSeconds int64      `json:"resolutionSeconds"`
	Samples           []Sample   `json:"samples"`
	Summary           Summary    `json:"summary"`
	OldestAt          *time.Time `json:"oldestAt,omitempty"`
	Error             string     `json:"error,omitempty"`
}

// Disabled returns the same contract when collection was not configured.
func Disabled(name string) History {
	return History{Range: name, Samples: []Sample{}, Error: "Persistent traffic history is not enabled; configure a data directory."}
}

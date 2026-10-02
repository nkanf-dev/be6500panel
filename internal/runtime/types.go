// Package runtime manages fixed sing-box and frpc artifacts, private configs and processes.
// It never accepts shell commands or arbitrary executable arguments.
package runtime

import (
	"context"
	"errors"
	"log/slog"
	"net/http"
	"time"
)

const (
	SingBox = "sing-box"
	FRPC    = "frpc"
)

var (
	ErrService                   = errors.New("unsupported managed service")
	ErrBusy                      = errors.New("another runtime operation is in progress")
	ErrClosed                    = errors.New("runtime manager is closed")
	ErrGeneration                = errors.New("configuration generation conflict")
	ErrNotConfigured             = errors.New("service is not configured")
	ErrNoArtifact                = errors.New("service artifact is unavailable")
	ErrCheck                     = errors.New("candidate verification failed")
	ErrReadiness                 = errors.New("managed process readiness failed")
	ErrArtifactCompressedLimit   = errors.New("artifact exceeds compressed byte limit")
	ErrArtifactUncompressedLimit = errors.New("artifact exceeds uncompressed byte limit")
	// ErrDurability means the atomic rename committed, but directory fsync failed.
	// The returned generation is authoritative; previous snapshots are retained.
	ErrDurability = errors.New("runtime state committed without confirmed directory durability")
)

// Artifact describes the fetched bytes. SHA256 hashes the compressed bytes when
// Compression is gzip. Only none and gzip are supported (never tar archives).
type Artifact struct {
	URL         string `json:"url"`
	SHA256      string `json:"sha256"`
	Compression string `json:"compression"`
	Version     string `json:"version"`
}

// Options sets storage and finite resource limits. Zero limits select defaults.
// DataDir must be persistent; RunDir must be a separate volatile filesystem.
// LocalSourceRoot and AllowLoopbackHTTP are explicit experimental source opt-ins.
// CleanupHook removes caller-owned network resources; no firewall changes occur here.
type Options struct {
	DataDir              string
	RunDir               string
	Logger               *slog.Logger
	AllowLoopbackHTTP    bool
	LocalSourceRoot      string
	HTTPClient           *http.Client
	MaxCompressedBytes   int64
	MaxUncompressedBytes int64
	MinFreeRunBytes      int64
	MaxConfigBytes       int64
	TailBytes            int
	DownloadTimeout      time.Duration
	CheckTimeout         time.Duration
	ReadyTimeout         time.Duration
	// ResourceTimeout bounds owned network cleanup/restore independently of process TERM and listener readiness.
	ResourceTimeout time.Duration
	TermGrace       time.Duration
	BackoffInitial  time.Duration
	BackoffMax      time.Duration
	StableAfter     time.Duration
	MaxRestarts     int
	CleanupHook     func(context.Context, string) error
	// ReadyHook checks fixed local listeners, not remote connectivity. It must
	// honor ctx and must not call manager mutations; Config/Status reads are safe.
	ReadyHook func(context.Context, string) error
	// RestoreHook rebuilds caller-owned resources after readiness. A failure
	// keeps the core running but reports resources suspended through their owner.
	// It runs in the mutation lane and may call Config/Status, not mutations.
	RestoreHook      func(context.Context, string) error
	syncDirectory    func(string) error // test seam; production always uses syncDir
	waitExitedLeader func(int) bool     // test seam; production always uses waitid(WNOWAIT)
}

type State string

const (
	NotConfigured State = "notconfigured"
	Downloading   State = "downloading"
	Rebuilding    State = "rebuilding"
	Checking      State = "checking"
	Starting      State = "starting"
	Running       State = "running"
	Backoff       State = "backoff"
	Stopped       State = "stopped"
	Error         State = "error"
)

// Status contains no config body, artifact URL, or subprocess output. Errors are
// fixed diagnostic codes; a core may emit credentials into its own output.
type Status struct {
	Service           string    `json:"service"`
	State             State     `json:"state"`
	Generation        uint64    `json:"generation"`
	Configured        bool      `json:"configured"`
	ArtifactAvailable bool      `json:"artifactAvailable"`
	Version           string    `json:"version,omitempty"`
	PID               int       `json:"pid,omitempty"`
	RSSBytes          int64     `json:"rssBytes"`
	RSSAvailable      bool      `json:"rssAvailable"`
	Desired           bool      `json:"desired"`
	Restarts          int       `json:"restarts"`
	RetryAt           time.Time `json:"retryAt,omitempty"`
	ErrorCode         string    `json:"errorCode,omitempty"`
	RecoveryPlan      []string  `json:"recoveryPlan,omitempty"`
}

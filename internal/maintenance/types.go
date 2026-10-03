// Package maintenance implements scoped private configuration downloads and
// memory-only import previews. It never applies configuration or starts services.
package maintenance

import (
	"be6500panel/internal/control"
	"context"
	"time"
)

const (
	MaxBackupBytes          = 2 << 20
	MaxRuntimeDocumentBytes = 512 << 10
	MaxPreviews             = 4
	MaxPreviewContentBytes  = 4 << 20
	PreviewLifetime         = 10 * time.Minute
)

var NativeScopes = []string{"network", "wireless", "dhcp", "firewall", "system", "dropbear"}

// NativeControl is deliberately narrower than Manager: maintenance has no Apply,
// Confirm, Rollback, runtime Start or arbitrary file/path operation.
type NativeControl interface {
	Documents(context.Context) (control.DocumentSet, error)
	Stage(context.Context, control.StageRequest) (control.Draft, error)
	// DeleteDraft must check cancellation before mutation, including after any
	// blocked lock acquisition. A deletion already persisting may still finish;
	// cleanup reports that uncertainty explicitly rather than claiming removal.
	DeleteDraft(context.Context, string) error
}

type Metadata struct {
	Model string
	Build string
}
type RuntimeDocument struct {
	Content    string
	Generation uint64
}
type Options struct {
	Native   NativeControl
	Metadata func(context.Context) (Metadata, error)
	// Runtime reads only the accepted private configuration for frpc or sing-box.
	Runtime func(context.Context, string) (RuntimeDocument, error)
	Now     func() time.Time
}

type Document struct {
	Module     string `json:"module"`
	Content    string `json:"content"`
	Digest     string `json:"digest"`
	Generation uint64 `json:"generation,omitempty"`
}

type Envelope struct {
	Model      string     `json:"model"`
	Build      string     `json:"build"`
	CreatedAt  time.Time  `json:"createdAt"`
	Generation uint64     `json:"generation"`
	Scopes     []string   `json:"scopes"`
	Documents  []Document `json:"documents"`
}
type Issue = control.Issue
type Summary struct {
	Added      int `json:"added"`
	Modified   int `json:"modified"`
	Deleted    int `json:"deleted"`
	Unchanged  int `json:"unchanged"`
	Uncompared int `json:"uncompared"`
}
type Change struct {
	Module       string  `json:"module"`
	Kind         string  `json:"kind"`
	BeforeBytes  int     `json:"beforeBytes"`
	AfterBytes   int     `json:"afterBytes"`
	BeforeDigest string  `json:"beforeDigest"`
	AfterDigest  string  `json:"afterDigest"`
	Diff         string  `json:"diff"`
	Stageable    bool    `json:"stageable"`
	Valid        bool    `json:"valid"`
	Errors       []Issue `json:"errors"`
	Dependencies []Issue `json:"dependencies"`
	Risks        []Issue `json:"risks"`
}
type Preview struct {
	ID            string    `json:"id"`
	Generation    uint64    `json:"generation"`
	SourceModel   string    `json:"sourceModel"`
	CurrentModel  string    `json:"currentModel"`
	ModelMismatch bool      `json:"modelMismatch"`
	ExpiresAt     time.Time `json:"expiresAt"`
	Summary       Summary   `json:"summary"`
	Changes       []Change  `json:"changes"`
	Warnings      []Issue   `json:"warnings"`
}
type StageRequest struct {
	PreviewID                string   `json:"previewId"`
	Generation               uint64   `json:"generation"`
	Modules                  []string `json:"modules"`
	AcknowledgeModelMismatch bool     `json:"acknowledgeModelMismatch"`
}
type StageResult struct {
	Generation uint64          `json:"generation"`
	Drafts     []control.Draft `json:"drafts"`
	Warnings   []Issue         `json:"warnings"`
}

// Error carries only safe diagnostics. RetainedDraftIDs makes a cleanup failure
// actionable instead of hiding orphaned private drafts after a partial Stage.
type Error struct {
	Code             string   `json:"code"`
	Message          string   `json:"message"`
	CauseCode        string   `json:"causeCode,omitempty"`
	CleanupPending   bool     `json:"cleanupPending,omitempty"`
	RetainedDraftIDs []string `json:"retainedDraftIds,omitempty"`
}

func (e *Error) Error() string           { return e.Code + ": " + e.Message }
func failure(code, message string) error { return &Error{Code: code, Message: message} }

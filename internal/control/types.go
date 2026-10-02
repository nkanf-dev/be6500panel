// Package control owns private UCI drafts and durable configuration transactions.
// No method stages into the live UCI namespace. Commit is the only apply boundary.
package control

import (
	"context"
	"log/slog"
	"time"
)

const (
	MaxDocumentBytes = 128 << 10
	MaxDrafts        = 12
)

// Runner executes one fixed executable with argv, never a shell command.
// Injected runners must honor context cancellation and must not log private output.
type Runner func(context.Context, string, ...string) ([]byte, error)

type Options struct {
	Root                string
	DataDir             string
	Logger              *slog.Logger
	Runner              Runner
	Verify              func(context.Context, []string) error
	Reload              func(context.Context, string) error
	ConfirmationTimeout time.Duration
	// StorageAdmission reserves full temporary allocations on the target
	// filesystem. recovery permits the shared owner to use its rollback reserve.
	// The returned release is held until writes and temporary cleanup finish.
	// Nil keeps the standalone control manager's existing bounded-file behavior.
	StorageAdmission func(context.Context, string, int64, bool) (func(), error)
}

type Document struct {
	Module  string `json:"module"`
	Content string `json:"content"`
}
type PendingCommit struct {
	ID       string    `json:"id"`
	Deadline time.Time `json:"deadline"`
}
type DocumentSet struct {
	Generation    uint64         `json:"generation"`
	Documents     []Document     `json:"documents"`
	PendingCommit *PendingCommit `json:"pendingCommit,omitempty"`
}
type Issue struct {
	Code    string `json:"code"`
	Message string `json:"message"`
}
type Draft struct {
	ID         string    `json:"id"`
	Module     string    `json:"module"`
	Generation uint64    `json:"generation"`
	Diff       string    `json:"diff"`
	Risks      []Issue   `json:"risks"`
	Valid      bool      `json:"valid"`
	Errors     []Issue   `json:"errors"`
	CreatedAt  time.Time `json:"createdAt"`
}
type StageRequest struct {
	Module     string `json:"module"`
	Content    string `json:"content"`
	Generation uint64 `json:"generation"`
}
type CommitRequest struct {
	DraftIDs         []string `json:"draftIds"`
	Generation       uint64   `json:"generation"`
	AcknowledgeRisks bool     `json:"acknowledgeRisks"`
}
type Operation struct {
	ID             string     `json:"id"`
	State          string     `json:"state"`
	Generation     uint64     `json:"generation"`
	Deadline       *time.Time `json:"deadline,omitempty"`
	ChangedModules []string   `json:"changedModules"`
}
type Status struct {
	Enabled       bool           `json:"enabled"`
	Generation    uint64         `json:"generation"`
	PendingCommit *PendingCommit `json:"pendingCommit,omitempty"`
	ErrorCode     string         `json:"errorCode,omitempty"`
}

// Error is safe for an authenticated API response. It never contains config or command output.
type Error struct {
	Code    string `json:"code"`
	Message string `json:"message"`
}

func (e *Error) Error() string           { return e.Code + ": " + e.Message }
func failure(code, message string) error { return &Error{Code: code, Message: message} }

var modules = []string{"network", "wireless", "dhcp", "firewall", "system", "dropbear"}

func allowed(module string) bool {
	for _, m := range modules {
		if module == m {
			return true
		}
	}
	return false
}

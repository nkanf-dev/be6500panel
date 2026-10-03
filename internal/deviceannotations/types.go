// Package deviceannotations persists bounded LAN administrator labels, notes,
// and tags. MAC addresses, never IP addresses, are the annotation identity.
// Notes are plaintext administrator content; this package does not scan them.
package deviceannotations

import (
	"context"
	"errors"
	"net"
	"strings"
	"unicode/utf8"

	"be6500panel/internal/storage"
)

const (
	DefaultDataDir = "/data/be6500panel"
	FileName       = "device-names.json"
	MaxDevices     = 256
	MaxLabelRunes  = 80
	MaxNoteRunes   = 1000
	MaxTags        = 8
	MaxTagRunes    = 32
	// JSON escapes can use six bytes per rune. This bound holds all 256
	// devices at their worst-case valid limits, including JSON overhead.
	MaxFileBytes = 3 << 20
	// Revisions remain exact in JavaScript JSON consumers.
	MaxRevision uint64 = 1<<53 - 1
)

var (
	ErrInvalidInput      = errors.New("invalid device annotation")
	ErrLimit             = errors.New("device annotation limit reached")
	ErrConflict          = errors.New("device annotations changed; refresh before saving")
	ErrRevisionExhausted = errors.New("device annotation revision limit reached")
	ErrStorage           = errors.New("device annotation storage unavailable")
)

type Annotation struct {
	Label string   `json:"label"`
	Note  string   `json:"note"`
	Tags  []string `json:"tags"`
}

type Snapshot struct {
	Revision uint64                `json:"revision"`
	Devices  map[string]Annotation `json:"devices"`
}

type UpdateRequest struct {
	MAC              string   `json:"mac"`
	Label            string   `json:"label"`
	Note             string   `json:"note"`
	Tags             []string `json:"tags"`
	ExpectedRevision uint64   `json:"expectedRevision"`
}

type Options struct {
	// DataDir defaults to /data/be6500panel. Keep one Store per directory.
	DataDir string
	// Use the same admission owner as all other persistent volume writers.
	// Annotation saves are normal writes (recovery=false), including deletes.
	StorageAdmission storage.Admission
	// Context optionally bounds startup loading; request contexts bound saves.
	Context context.Context
}

// CanonicalMAC accepts standard six-byte hardware-address notation and returns
// uppercase colon-separated keys. It rejects EUI-64, IPs, and surrounding space.
func CanonicalMAC(value string) (string, error) {
	address, err := net.ParseMAC(value)
	if err != nil || len(address) != 6 {
		return "", ErrInvalidInput
	}
	return strings.ToUpper(address.String()), nil
}

func validText(value string, limit int) bool {
	return len(value) <= limit*utf8.UTFMax && utf8.ValidString(value) && utf8.RuneCountInString(value) <= limit
}
func validateAnnotation(a Annotation) error {
	if !validText(a.Label, MaxLabelRunes) || !validText(a.Note, MaxNoteRunes) || len(a.Tags) > MaxTags {
		return ErrInvalidInput
	}
	for _, tag := range a.Tags {
		if !validText(tag, MaxTagRunes) {
			return ErrInvalidInput
		}
	}
	return nil
}
func cloneAnnotation(a Annotation) Annotation {
	a.Tags = append([]string{}, a.Tags...)
	return a
}
func cloneSnapshot(s Snapshot) Snapshot {
	out := Snapshot{Revision: s.Revision, Devices: make(map[string]Annotation, len(s.Devices))}
	for mac, a := range s.Devices {
		out.Devices[mac] = cloneAnnotation(a)
	}
	return out
}

package core

import (
	"context"
	"log/slog"
	"sync"
	"time"
)

const LogCapacity = 500

type LogEntry struct {
	Sequence uint64    `json:"sequence"`
	Time     time.Time `json:"time"`
	Level    string    `json:"level"`
	Code     string    `json:"code"`
	Module   string    `json:"module"`
	Message  string    `json:"message"`
}

// LogBuffer has no persistence. Only the fixed, non-secret event fields enter
// the ring. Callers must use fixed messages/codes, never request contents.
type LogBuffer struct {
	mu       sync.Mutex
	entries  [LogCapacity]LogEntry
	next     int
	count    int
	sequence uint64
}

func (b *LogBuffer) add(entry LogEntry) {
	b.mu.Lock()
	defer b.mu.Unlock()
	b.sequence++
	entry.Sequence = b.sequence
	b.entries[b.next] = entry
	b.next = (b.next + 1) % LogCapacity
	if b.count < LogCapacity {
		b.count++
	}
}
func (b *LogBuffer) Entries(limit int) []LogEntry {
	b.mu.Lock()
	defer b.mu.Unlock()
	if limit < 0 {
		limit = 0
	}
	if limit > b.count {
		limit = b.count
	}
	out := make([]LogEntry, 0, limit)
	start := (b.next - limit + LogCapacity) % LogCapacity
	for i := 0; i < limit; i++ {
		out = append(out, b.entries[(start+i)%LogCapacity])
	}
	return out
}

// RingHandler forwards records to the process handler and stores only the small
// public event projection. Arbitrary attributes never enter the public API.
type RingHandler struct {
	sink    slog.Handler
	buffer  *LogBuffer
	attrs   []slog.Attr
	grouped bool
}

func NewRingHandler(sink slog.Handler, buffer *LogBuffer) *RingHandler {
	return &RingHandler{sink: sink, buffer: buffer}
}
func (h *RingHandler) Enabled(ctx context.Context, level slog.Level) bool {
	return h.sink.Enabled(ctx, level)
}
func (h *RingHandler) Handle(ctx context.Context, record slog.Record) error {
	entry := LogEntry{Time: record.Time.UTC(), Level: record.Level.String(), Message: record.Message}
	apply := func(attr slog.Attr) {
		attr.Value = attr.Value.Resolve()
		if attr.Key == "code" {
			entry.Code = attr.Value.String()
		}
		if attr.Key == "module" {
			entry.Module = attr.Value.String()
		}
	}
	if !h.grouped {
		for _, attr := range h.attrs {
			apply(attr)
		}
		record.Attrs(func(attr slog.Attr) bool { apply(attr); return true })
	}
	h.buffer.add(entry)
	return h.sink.Handle(ctx, record)
}
func (h *RingHandler) WithAttrs(attrs []slog.Attr) slog.Handler {
	out := *h
	out.sink = h.sink.WithAttrs(attrs)
	out.attrs = append(append([]slog.Attr{}, h.attrs...), attrs...)
	return &out
}
func (h *RingHandler) WithGroup(name string) slog.Handler {
	if name == "" {
		return h
	}
	out := *h
	out.sink = h.sink.WithGroup(name)
	out.grouped = true
	return &out
}

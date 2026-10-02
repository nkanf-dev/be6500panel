package core

import (
	"crypto/rand"
	"encoding/hex"
	"sync/atomic"
)

// Coordinator is the seam for future serialized configuration changes. This
// foundation only produces plans. No apply, persistence or system-write path exists.
type Coordinator struct{ generation atomic.Uint64 }

func NewCoordinator() *Coordinator { c := &Coordinator{}; c.generation.Store(1); return c }
func (c *Coordinator) Plan(summary string, steps []PlanStep, warnings []string) (OperationPlan, error) {
	var id [16]byte
	if _, err := rand.Read(id[:]); err != nil {
		return OperationPlan{}, err
	}
	return OperationPlan{ID: hex.EncodeToString(id[:]), Generation: c.generation.Load(), ReadOnly: true, Summary: summary, Steps: steps, Warnings: warnings, CanApply: false}, nil
}

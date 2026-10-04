package capture

import "context"

// Backend is the capture control surface used by the panel. A production
// remote guardian backend must never fall back to a local network writer.
// Builder evaluation belongs to the panel before typed guardian requests.
type Backend interface {
	Status() Status
	Desired() Desired
	SetBuilder(Builder)
	Select(context.Context, Desired) (Status, error)
	Disable(context.Context) error
	Restore(context.Context) (Status, error)
	Suspend(context.Context, string) error
	Cleanup(context.Context) error
	DisableRetainingSelection(context.Context) error
	Refresh(context.Context) (Status, error)
	Reconcile(context.Context) (Status, error)
	ReconcileDesired(context.Context) (Status, error)
	Diagnostics(context.Context) DatapathDiagnostics
}

var _ Backend = (*Controller)(nil)

package main

import (
	"be6500panel/internal/requesttrace"
	"be6500panel/internal/router"
	managedruntime "be6500panel/internal/runtime"
	"context"
)

type diagnosticRuntime interface {
	Status(string) (managedruntime.Status, error)
	Config(string) ([]byte, uint64, error)
}
type diagnosticRouter interface {
	CaptureObservation(context.Context) (router.CaptureObservation, error)
}

// acceptedDiagnosticProxy reads private accepted state only. It never mutates
// runtime/capture and does not guess a listener from a browser supplied address.
func acceptedDiagnosticProxy(runtime diagnosticRuntime, source diagnosticRouter) requesttrace.ProxyProvider {
	return func(ctx context.Context) (requesttrace.ProxyEndpoint, error) {
		unavailable := requesttrace.ProxyEndpoint{}
		if ctx.Err() != nil {
			return unavailable, ctx.Err()
		}
		if runtime == nil {
			return unavailable, requesttrace.ErrUnavailable
		}
		before, err := runtime.Status(managedruntime.SingBox)
		if err != nil || before.State != managedruntime.Running || before.NeedsRecovery {
			return unavailable, requesttrace.ErrUnavailable
		}
		raw, generation, err := runtime.Config(managedruntime.SingBox)
		if err != nil || generation != before.Generation {
			return unavailable, requesttrace.ErrUnavailable
		}
		endpoint, err := requesttrace.MixedProxyFromNative(raw, nil)
		if err != nil {
			if source == nil {
				return unavailable, requesttrace.ErrUnavailable
			}
			observed, sourceErr := source.CaptureObservation(ctx)
			if sourceErr != nil {
				return unavailable, requesttrace.ErrUnavailable
			}
			endpoint, err = requesttrace.MixedProxyFromNative(raw, observed.LANAddresses)
			if err != nil {
				return unavailable, requesttrace.ErrUnavailable
			}
		}
		after, err := runtime.Status(managedruntime.SingBox)
		if err != nil || after.State != managedruntime.Running || after.NeedsRecovery || after.Generation != generation || after.PID != before.PID {
			return unavailable, requesttrace.ErrUnavailable
		}
		if ctx.Err() != nil {
			return unavailable, ctx.Err()
		}
		return endpoint, nil
	}
}

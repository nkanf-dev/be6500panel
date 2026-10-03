package httpapi

import (
	"be6500panel/internal/nodeprobe"
	"be6500panel/internal/proxy"
	managedruntime "be6500panel/internal/runtime"
	"context"
	"net/http"
	"os"
)

func (s *Server) probeNodeSnapshot() (nodeprobe.NodeSet, error) {
	p := s.proxyState
	p.mu.Lock()
	defer p.mu.Unlock()
	if p.loadFailed || p.revision == "" {
		return nodeprobe.NodeSet{}, nodeprobe.ErrRevision
	}
	return nodeprobe.NodeSet{Revision: p.revision, Nodes: append([]proxy.Node{}, p.subscription.Nodes...)}, nil
}
func (s *Server) newNodeProbeManager() (*nodeprobe.Manager, error) {
	return nodeprobe.New(nodeprobe.Config{
		Nodes: s.probeNodeSnapshot,
		Availability: func() string {
			if s.runtime == nil {
				return "artifact_unavailable"
			}
			status, err := s.runtime.Status(managedruntime.SingBox)
			if err != nil || !status.ArtifactAvailable || status.ErrorCode == "state_not_durable" {
				return "artifact_unavailable"
			}
			return ""
		},
		AcquireLease: func(ctx context.Context) (nodeprobe.CoreLease, error) {
			if s.runtime == nil {
				return nodeprobe.CoreLease{}, nodeprobe.ErrUnavailable
			}
			lease, err := s.runtime.AcquireProbeLease(ctx, s.runtimeMutationGuard)
			if err != nil {
				return nodeprobe.CoreLease{}, err
			}
			return nodeprobe.CoreLease{Path: lease.Path(), Release: lease.Release}, nil
		},
		TempDir: os.TempDir(),
	})
}
func (s *Server) nodeProbeRequest(w http.ResponseWriter, r *http.Request) {
	if r.Method == http.MethodPost {
		if !s.runtimeMutationAllowed(w) {
			return
		}
		// Subscription replacement cannot publish between node snapshot and job
		// admission. The provider reads p.mu only; it never recurses this gate.
		if !s.proxyState.mutationMu.TryLock() {
			fail(w, 409, "proxy_mutation_pending", "订阅正在保存，请稍后重试测速")
			return
		}
		defer s.proxyState.mutationMu.Unlock()
	}
	if s.nodeProbes == nil {
		HandleNodeProbes(w, r, nil)
		return
	}
	HandleNodeProbes(w, r, s.nodeProbes)
}

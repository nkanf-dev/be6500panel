package requesttrace

import (
	"crypto/tls"
	"net/http/httptrace"
	"sync"
	"time"
)

// Each phase is the span between actual callbacks, relative to one monotonic
// request start. Multiple DNS/connect attempts may overlap; these are spans,
// not an additive decomposition. Unknown and unfinished phases remain null.
type recorder struct {
	mu          sync.Mutex
	started     time.Time
	finished    time.Time
	closed      bool
	phases      []Phase
	last        string
	failed      string
	peer        *string
	tcpAttempts int
}

func newRecorder(start time.Time, route string) *recorder {
	r := &recorder{started: start, last: "request"}
	for _, id := range []string{"dns", "tcp", "connect", "tls", "ttfb", "transfer"} {
		reason := "not_observed"
		if id == "connect" {
			reason = "not_used_by_direct_route"
		}
		if route == "proxy" && id == "dns" {
			reason = "proxy_origin_dns_not_observable"
		}
		if route == "proxy" && id == "connect" {
			reason = "connect_timing_not_exposed_by_httptrace"
		}
		r.phases = append(r.phases, Phase{ID: id, Reason: reason})
	}
	return r
}
func (r *recorder) update(id string, finish bool, err error) {
	r.mu.Lock()
	defer r.mu.Unlock()
	if r.closed {
		return
	}
	now := time.Since(r.started).Seconds() * 1000
	for i := range r.phases {
		p := &r.phases[i]
		if p.ID != id {
			continue
		}
		if !finish {
			p.Observed = true
			p.Reason = ""
			if p.StartMS == nil {
				p.StartMS = &now
			}
			if id == "tcp" {
				r.tcpAttempts++
				if r.tcpAttempts > 1 {
					p.Reason = "multiple_connection_attempt_span"
				}
			}
		} else if p.StartMS != nil {
			p.EndMS = &now
			duration := now - *p.StartMS
			p.DurationMS = &duration
		}
		r.last = id
		if err != nil {
			r.failed = id
		} else if r.failed == id {
			// A later successful attempt supersedes a failed address attempt.
			r.failed = ""
		}
		return
	}
}
func (r *recorder) failure(id string) {
	r.mu.Lock()
	defer r.mu.Unlock()
	if !r.closed {
		r.failed = id
	}
}
func (r *recorder) begin(id string)          { r.update(id, false, nil) }
func (r *recorder) end(id string, err error) { r.update(id, true, err) }
func (r *recorder) hooks() *httptrace.ClientTrace {
	return &httptrace.ClientTrace{
		DNSStart:     func(httptrace.DNSStartInfo) { r.begin("dns") },
		DNSDone:      func(info httptrace.DNSDoneInfo) { r.end("dns", info.Err) },
		ConnectStart: func(string, string) { r.begin("tcp") },
		ConnectDone: func(_, address string, err error) {
			r.end("tcp", err)
			if err == nil {
				r.mu.Lock()
				if !r.closed {
					r.peer = &address
				}
				r.mu.Unlock()
			}
		},
		TLSHandshakeStart: func() { r.begin("tls") },
		TLSHandshakeDone:  func(_ tls.ConnectionState, err error) { r.end("tls", err) },
		GotConn: func(info httptrace.GotConnInfo) {
			if info.Conn != nil {
				peer := info.Conn.RemoteAddr().String()
				r.mu.Lock()
				if !r.closed {
					r.peer = &peer
				}
				r.mu.Unlock()
			}
		},
		WroteRequest: func(info httptrace.WroteRequestInfo) {
			if info.Err == nil {
				// Earlier failed parallel dials are not this request's failure.
				r.mu.Lock()
				r.failed = ""
				r.mu.Unlock()
				r.begin("ttfb")
			} else {
				r.mu.Lock()
				r.failed = "request"
				r.mu.Unlock()
			}
		},
		GotFirstResponseByte: func() { r.end("ttfb", nil) },
	}
}
func (r *recorder) result(failed bool) ([]Phase, *string, *string) {
	r.mu.Lock()
	defer r.mu.Unlock()
	if !r.closed {
		r.finished = time.Now()
		r.closed = true
	}
	phases := cloneTrace(Trace{Phases: r.phases}).Phases
	var failure *string
	if failed {
		// Choose the furthest stage actually reached. A failed parallel TCP
		// address attempt must not hide a later TLS/wait/download failure.
		name := "request"
		for _, p := range phases {
			if p.Observed {
				name = p.ID
			}
		}
		if r.failed == "request" || r.failed == "connect" {
			name = r.failed
		}
		failure = &name
	}
	var peer *string
	if r.peer != nil {
		v := *r.peer
		peer = &v
	}
	return phases, peer, failure
}

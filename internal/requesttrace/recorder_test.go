package requesttrace

import (
	"errors"
	"testing"
	"time"
)

func TestRecorderParallelAttemptDoesNotMoveFailureBack(t *testing.T) {
	r := newRecorder(time.Now(), "direct")
	r.begin("tcp")
	r.begin("tcp")
	r.end("tcp", nil)
	r.begin("tls")
	r.end("tls", nil)
	r.begin("ttfb")
	// A second address attempt loses after the request has reached first-byte wait.
	r.end("tcp", errors.New("losing address"))
	phases, _, failure := r.result(true)
	if failure == nil || *failure != "ttfb" {
		t.Fatal("parallel dial masked wait failure", failure)
	}
	complete(t, phases[1])
	if phases[1].Reason != "multiple_connection_attempt_span" {
		t.Fatal("parallel span unlabeled")
	}
	if phases[4].EndMS != nil {
		t.Fatal("unfinished wait has fabricated end")
	}
}

func TestRecorderLateCallbacksCannotMutateFinishedIntervals(t *testing.T) {
	r := newRecorder(time.Now(), "direct")
	r.begin("tcp")
	r.end("tcp", nil)
	phases, _, _ := r.result(false)
	finished := r.finished
	r.end("tcp", errors.New("late lost dial"))
	r.begin("dns")
	after, _, _ := r.result(false)
	if r.finished != finished || *after[1].EndMS != *phases[1].EndMS || after[0].Observed {
		t.Fatal("late callback changed completed trace")
	}
}

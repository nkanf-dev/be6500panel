package runtime

import (
	"context"
	"errors"
	"io"
	"os/exec"
	goruntime "runtime"
	"sync"
	"syscall"
	"time"
)

// tailWriter retains a bounded suffix, never forwards core output to Logger/API.
type tailWriter struct {
	mu    sync.Mutex
	data  []byte
	limit int
}

func (w *tailWriter) Write(b []byte) (int, error) {
	n := len(b)
	w.mu.Lock()
	defer w.mu.Unlock()
	if w.limit <= 0 {
		return n, nil
	}
	if n >= w.limit {
		w.data = append(w.data[:0], b[n-w.limit:]...)
		return n, nil
	}
	excess := len(w.data) + n - w.limit
	if excess > 0 {
		copy(w.data, w.data[excess:])
		w.data = w.data[:len(w.data)-excess]
	}
	w.data = append(w.data, b...)
	return n, nil
}

var _ io.Writer = (*tailWriter)(nil)

type managedProcess struct {
	cmd      *exec.Cmd
	done     chan struct{}
	signalMu sync.Mutex
	exited   bool
	err      error
	started  time.Time
	exitedAt time.Time
	tail     *tailWriter
}

func launch(binary string, args []string, opts Options) (*managedProcess, error) {
	cmd := exec.Command(binary, args...)
	cmd.SysProcAttr = processAttributes()
	// Bound inherited descendant pipes, even on development platforms without waitid.
	cmd.WaitDelay = opts.TermGrace
	tail := &tailWriter{limit: opts.TailBytes}
	cmd.Stdout = tail
	cmd.Stderr = tail
	// Do not inherit panel credentials or proxy environment into managed cores.
	cmd.Env = []string{"PATH=/usr/sbin:/usr/bin:/sbin:/bin", "HOME=" + opts.RunDir, "TMPDIR=" + opts.RunDir}
	cmd.Dir = opts.RunDir
	p := &managedProcess{cmd: cmd, done: make(chan struct{}), tail: tail}
	started := make(chan error, 1)
	go func() {
		// Linux Pdeathsig belongs to the creator OS thread. Own that thread for
		// the whole child lifetime, not the transient API handler goroutine.
		goruntime.LockOSThread()
		defer goruntime.UnlockOSThread()
		if err := cmd.Start(); err != nil {
			started <- err
			return
		}
		p.started = time.Now()
		started <- nil
		waitLeader := retainExitedLeader
		if opts.waitExitedLeader != nil {
			waitLeader = opts.waitExitedLeader
		}
		retained := waitLeader(cmd.Process.Pid)
		p.signalMu.Lock()
		// Cmd.Wait has not run, so the leader is either running or unreaped.
		// A waitid failure is fail-closed: kill and reap, never mark a live core
		// unsignalable or leave terminate blocked forever.
		_ = syscall.Kill(-cmd.Process.Pid, syscall.SIGKILL)
		p.exited = true
		p.exitedAt = time.Now()
		p.signalMu.Unlock()
		err := cmd.Wait()
		if !retained {
			err = errors.New("cannot retain managed process identity")
		}
		p.signalMu.Lock()
		p.err = err
		p.signalMu.Unlock()
		close(p.done)
	}()
	if err := <-started; err != nil {
		return nil, err
	}
	return p, nil
}

func (p *managedProcess) signal(sig syscall.Signal) {
	p.signalMu.Lock()
	defer p.signalMu.Unlock()
	if !p.exited {
		_ = syscall.Kill(-p.cmd.Process.Pid, sig)
	}
}

// terminate never signals a PID read from Status or persistent state. It only
// signals the unreaped process group held by this operation's process object.
func (p *managedProcess) terminate(grace time.Duration) {
	if p == nil {
		return
	}
	p.signal(syscall.SIGTERM)
	t := time.NewTimer(grace)
	defer t.Stop()
	select {
	case <-p.done:
		return
	case <-t.C:
	}
	p.signal(syscall.SIGKILL)
	<-p.done // Wait reaps the managed leader; this never delegates reaping to init.
}

func verify(ctx context.Context, binary, service, path string, opts Options) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	args := []string{"check", "-c", path}
	if service == FRPC {
		args = []string{"verify", "-c", path}
	}
	p, err := launch(binary, args, opts)
	if err != nil {
		return errors.New("cannot launch candidate verifier")
	}
	timer := time.NewTimer(opts.CheckTimeout)
	defer timer.Stop()
	select {
	case <-p.done:
		if p.err != nil {
			return ErrCheck
		}
		return nil
	case <-ctx.Done():
		p.terminate(opts.TermGrace)
		return ctx.Err()
	case <-timer.C:
		p.terminate(opts.TermGrace)
		return errors.New("candidate verification timed out")
	}
}

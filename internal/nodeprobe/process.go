package nodeprobe

import (
	"context"
	"os/exec"
	"runtime"
	"sync"
	"time"
)

// ownedProcess retains the leader's PID/PGID until group cleanup is complete.
// exited means WNOWAIT observed an exit, not that Cmd.Wait reaped the leader.
type ownedProcess struct {
	cmd       *exec.Cmd
	exited    chan struct{}
	done      chan struct{}
	allowReap chan struct{}
	ctx       context.Context
	cancel    context.CancelFunc
	signalMu  sync.Mutex
	reaping   bool
	closeOnce sync.Once
}

func startOwnedProcess(cmd *exec.Cmd) (*ownedProcess, error) {
	if err := prepareProcessGroup(cmd); err != nil {
		return nil, err
	}
	ctx, cancel := context.WithCancel(context.Background())
	p := &ownedProcess{cmd: cmd, exited: make(chan struct{}), done: make(chan struct{}), allowReap: make(chan struct{}), ctx: ctx, cancel: cancel}
	started := make(chan error, 1)
	go func() {
		// Linux Pdeathsig belongs to this creator thread; keep it alive throughout.
		runtime.LockOSThread()
		defer runtime.UnlockOSThread()
		if err := cmd.Start(); err != nil {
			cancel()
			started <- err
			return
		}
		started <- nil
		_ = retainProbeLeader(cmd.Process.Pid)
		cancel()
		close(p.exited)
		// No Wait/reap before Close has terminated this exact owned process group.
		<-p.allowReap
		p.signalMu.Lock()
		p.reaping = true
		p.signalMu.Unlock()
		_ = cmd.Wait()
		close(p.done)
	}()
	if err := <-started; err != nil {
		return nil, err
	}
	return p, nil
}
func (p *ownedProcess) signal(force bool) {
	p.signalMu.Lock()
	defer p.signalMu.Unlock()
	if !p.reaping {
		terminateProcessGroup(p.cmd, force)
	}
}
func (p *ownedProcess) Close() {
	p.closeOnce.Do(func() {
		p.cancel()
		p.signal(false)
		timer := time.NewTimer(250 * time.Millisecond)
		select {
		case <-p.exited:
			timer.Stop()
		case <-timer.C:
			p.signal(true)
			<-p.exited
		}
		// The leader is still unreaped, including on early exit, so the numeric group
		// cannot have been reused for the active core or another unrelated process.
		p.signal(true)
		close(p.allowReap)
		<-p.done
	})
}

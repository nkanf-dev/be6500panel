package nodeprobe

import (
	"os"
	"os/exec"
	"testing"
	"time"
)

func TestExitedLeaderIsRetainedUntilGroupCleanupThenWait(t *testing.T) {
	cmd := exec.Command(os.Args[0], "-test.run=^$")
	process, err := startOwnedProcess(cmd)
	if err != nil {
		t.Fatal(err)
	}
	select {
	case <-process.exited:
	case <-time.After(2 * time.Second):
		t.Fatal("exit not observed")
	}
	select {
	case <-process.done:
		t.Fatal("leader reaped before process group cleanup")
	default:
	}
	if cmd.ProcessState != nil {
		t.Fatal("Cmd.Wait ran before cleanup")
	}
	if process.ctx.Err() == nil {
		t.Fatal("exit did not cancel owned requests")
	}
	process.Close()
	process.Close()
	select {
	case <-process.done:
	default:
		t.Fatal("leader not reaped after cleanup")
	}
	if cmd.ProcessState == nil {
		t.Fatal("owned leader Wait missing")
	}
	process.signal(true) // reaping flag suppresses all post-reap numeric signals
}

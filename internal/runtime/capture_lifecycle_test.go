package runtime

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"sync/atomic"
	"syscall"
	"testing"

	"be6500panel/internal/capture"
	"be6500panel/internal/proxy"
)

func captureRunner(_ context.Context, a []string) ([]byte, error) {
	if len(a) == 7 && a[5] == "-S" {
		return []byte(a[0] + ": No chain/target/match by that name."), errors.New("absent")
	}
	return nil, nil
}
func TestDesiredCaptureLifecycleConfigureStopFailureRetryAndBoot(t *testing.T) {
	captureDir := t.TempDir()
	controller, err := capture.New(captureDir, captureRunner)
	if err != nil {
		t.Fatal(err)
	}
	var manager *Manager
	var ready atomic.Bool
	var fail atomic.Bool
	changes := []string{}
	controller.SetBuilder(func(ctx context.Context, d capture.Desired) (proxy.RulesPlanInput, []capture.Client, error) {
		if !ready.Load() {
			t.Fatal("capture restored before readiness")
		}
		raw, _, err := manager.Config(SingBox)
		if err != nil {
			return proxy.RulesPlanInput{}, nil, err
		}
		changes = append(changes, string(raw))
		clients := []capture.Client{{MAC: d.Devices[0].MAC, IP: "192.0.2.10"}}
		if fail.Load() {
			return proxy.RulesPlanInput{}, clients, errors.New("capture_device_unresolved")
		}
		return proxy.RulesPlanInput{ClientIPv4: "192.0.2.10", LANInterface: "br-lan", IPv6: proxy.IPv6Direct}, clients, nil
	})
	manager, opts := testManager(t, func(o *Options) {
		o.ReadyHook = func(context.Context, string) error { ready.Store(true); return nil }
		o.CleanupHook = func(ctx context.Context, id string) error {
			if id == SingBox {
				ready.Store(false)
				return controller.Cleanup(ctx)
			}
			return nil
		}
		o.RestoreHook = func(ctx context.Context, id string) error {
			if id == SingBox {
				_, err := controller.Restore(ctx)
				return err
			}
			return nil
		}
	})
	acquireFixture(t, manager, opts, SingBox, fixture)
	accepted(t, manager, SingBox, "first-node", 0)
	if _, err = manager.Start(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
	if _, err = controller.Select(context.Background(), capture.Desired{Devices: []capture.DeviceSelection{{MAC: "02:00:00:00:00:10"}}, IPv6: proxy.IPv6Direct}); err != nil {
		t.Fatal(err)
	}
	next := accepted(t, manager, SingBox, "second-node", 1)
	if next.State != Running || !controller.Status().Active || !controller.Status().Desired || changes[len(changes)-1] != "second-node" {
		t.Fatalf("node change lost scope: %+v %+v %v", next, controller.Status(), changes)
	}
	if _, err = manager.Stop(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
	if state := controller.Status(); state.Active || !state.Desired || state.State != "suspended" {
		t.Fatalf("stop erased desired scope: %+v", state)
	}
	if _, err = os.Stat(filepath.Join(captureDir, "capture-journal.json")); !os.IsNotExist(err) {
		t.Fatal("stop kept live journal", err)
	}
	fail.Store(true)
	if state, err := manager.Start(context.Background(), SingBox); err != nil || state.State != Running {
		t.Fatalf("capture failure must keep explicit core usable: %+v %v", state, err)
	}
	if state := controller.Status(); state.Active || !state.Desired || state.Error != "capture_device_unresolved" {
		t.Fatalf("restore failure hidden: %+v", state)
	}
	fail.Store(false)
	if _, err = manager.Start(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
	if !controller.Status().Active {
		t.Fatal("explicit start did not retry suspended intent")
	}
	manager.mu.Lock()
	process := manager.services[SingBox].proc
	manager.mu.Unlock()
	process.signal(syscall.SIGKILL)
	waitStatus(t, manager, SingBox, func(s Status) bool {
		return s.State == Running && s.PID != process.cmd.Process.Pid && controller.Status().Active
	})
	if err = manager.Close(); err != nil {
		t.Fatal(err)
	}
	if controller.Status().Active || !controller.Status().Desired {
		t.Fatal("panel close lost saved intent")
	}
	controller, err = capture.New(captureDir, captureRunner)
	if err != nil {
		t.Fatal(err)
	}
	if controller.Status().Active || !controller.Status().Desired {
		t.Fatal("fresh capture manager did not load desired-only state")
	}
	reopened, err := New(opts)
	if err != nil {
		t.Fatal(err)
	}
	manager = reopened
	defer reopened.Close()
	controller.SetBuilder(func(ctx context.Context, d capture.Desired) (proxy.RulesPlanInput, []capture.Client, error) {
		if !ready.Load() {
			t.Fatal("boot restored capture before readiness")
		}
		return proxy.RulesPlanInput{ClientIPv4: "192.0.2.11", LANInterface: "br-lan", IPv6: proxy.IPv6Direct}, []capture.Client{{MAC: d.Devices[0].MAC, IP: "192.0.2.11"}}, nil
	})
	acquireFixture(t, reopened, opts, SingBox, fixture)
	if _, err = reopened.Start(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
	if state := controller.Status(); !state.Active || !state.Desired || state.Clients[0].IP != "192.0.2.11" {
		t.Fatalf("boot did not resolve fresh scope: %+v", state)
	}
	if err = controller.Disable(context.Background()); err != nil {
		t.Fatal(err)
	}
	accepted(t, reopened, SingBox, "third-node", 2)
	if controller.Status().Active || controller.Status().Desired {
		t.Fatal("DELETE-disabled selection restored on node change")
	}
}

func TestRestoreHookNotCalledBeforeReadyAndReadyOperationSerialized(t *testing.T) {
	var calls atomic.Int32
	var block, failRestore atomic.Bool
	failRestore.Store(true)
	m, opts := testManager(t, func(o *Options) {
		o.ReadyHook = func(context.Context, string) error {
			if block.Load() {
				return errors.New("not ready")
			}
			return nil
		}
		o.RestoreHook = func(context.Context, string) error {
			calls.Add(1)
			if failRestore.Load() {
				return errors.New("owned resources failed")
			}
			return nil
		}
	})
	acquireFixture(t, m, opts, SingBox, fixture)
	accepted(t, m, SingBox, "good", 0)
	if err := m.ReadyOperation(context.Background(), SingBox, func(context.Context) error { t.Fatal("operation ran while stopped"); return nil }); !errors.Is(err, ErrReadiness) {
		t.Fatal(err)
	}
	block.Store(true)
	if _, err := m.Start(context.Background(), SingBox); !errors.Is(err, ErrReadiness) {
		t.Fatal(err)
	}
	if calls.Load() != 0 {
		t.Fatal("failed readiness ran restore hook")
	}
	block.Store(false)
	state, err := m.Start(context.Background(), SingBox)
	if err != nil || state.State != Running || calls.Load() != 1 {
		t.Fatalf("resource failure hid healthy core: %+v %v %d", state, err, calls.Load())
	}
	if err := m.ReadyOperation(context.Background(), SingBox, func(context.Context) error { t.Fatal("operation ran before owned-resource recovery"); return nil }); !errors.Is(err, ErrReadiness) {
		t.Fatal(err)
	}
	failRestore.Store(false)
	if state, err = m.Start(context.Background(), SingBox); err != nil || state.NeedsRecovery {
		t.Fatal(state, err)
	}
	err = m.ReadyOperation(context.Background(), SingBox, func(context.Context) error {
		if _, err := m.Stop(context.Background(), SingBox); !errors.Is(err, ErrBusy) {
			t.Fatal("scope mutation did not reserve runtime lane", err)
		}
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
}

func TestResourceLaneRejectsConcurrentStopConfigureAndCapture(t *testing.T) {
	manager, opts := testManager(t, nil)
	acquireFixture(t, manager, opts, SingBox, fixture)
	accepted(t, manager, SingBox, "good", 0)
	if _, err := manager.Start(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
	entered := make(chan struct{})
	release := make(chan struct{})
	finished := make(chan error, 1)
	go func() {
		finished <- manager.ResourceOperation(context.Background(), SingBox, func(context.Context) error { close(entered); <-release; return nil })
	}()
	<-entered
	if _, err := manager.Stop(context.Background(), SingBox); !errors.Is(err, ErrBusy) {
		t.Fatal("native lane raced Stop", err)
	}
	if _, err := manager.Configure(context.Background(), SingBox, []byte("new"), 1); !errors.Is(err, ErrBusy) {
		t.Fatal("native lane raced Configure", err)
	}
	if err := manager.ReadyOperation(context.Background(), SingBox, func(context.Context) error { t.Fatal("capture raced native operation"); return nil }); !errors.Is(err, ErrBusy) {
		t.Fatal(err)
	}
	close(release)
	if err := <-finished; err != nil {
		t.Fatal(err)
	}
	if _, err := manager.Stop(context.Background(), SingBox); err != nil {
		t.Fatal(err)
	}
}

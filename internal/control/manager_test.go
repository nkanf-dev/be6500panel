package control

import (
	"context"
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"strings"
	"sync"
	"syscall"
	"testing"
	"time"
)

const testNetwork = "config interface 'lan'\n option proto 'static'\n option ipaddr '192.168.31.1'\n option netmask '255.255.255.0'\n"
const testDHCP = "config dnsmasq\n option domain 'lan'\n"
const testWireless = "config wifi-device 'radio0'\n option disabled '0'\nconfig wifi-iface\n option device 'radio0'\n option ifname 'wl0'\n option ssid 'test-only'\n option encryption 'psk2'\n option key 'synthetic-pass'\n"

type fixture struct {
	root, data string
	mu         sync.Mutex
	calls      [][]string
	reloads    []string
	runErr     error
	reloadFn   func(context.Context, string) error
}

func newFixture(t *testing.T) *fixture {
	t.Helper()
	f := &fixture{root: t.TempDir(), data: t.TempDir()}
	if err := os.MkdirAll(filepath.Join(f.root, "etc", "config"), 0700); err != nil {
		t.Fatal(err)
	}
	for _, d := range []Document{{"network", testNetwork}, {"wireless", testWireless}, {"dhcp", testDHCP}, {"firewall", "config defaults\n option input 'ACCEPT'\n option forward 'REJECT'\n option output 'ACCEPT'\n"}, {"system", "config system\n option hostname 'fixture-router'\n"}, {"dropbear", "config dropbear\n option Port '22'\n option PasswordAuth 'on'\n"}} {
		if err := os.WriteFile(filepath.Join(f.root, "etc", "config", d.Module), []byte(d.Content), 0644); err != nil {
			t.Fatal(err)
		}
	}
	return f
}
func (f *fixture) run(ctx context.Context, path string, args ...string) ([]byte, error) {
	f.mu.Lock()
	f.calls = append(f.calls, append([]string{path}, args...))
	err := f.runErr
	f.mu.Unlock()
	if ctx.Err() != nil {
		return nil, ctx.Err()
	}
	if err != nil {
		return nil, err
	}
	if path != "/sbin/uci" {
		return nil, errors.New("unexpected executable")
	}
	if reflect.DeepEqual(args, []string{"-q", "get", "xiaoqiang.common.INITTED"}) {
		return []byte("YES\n"), nil
	}
	if len(args) != 7 || args[0] != "-s" || args[1] != "-c" || args[3] != "-P" || args[2] != args[4] || args[5] != "show" || !allowed(args[6]) {
		return nil, errors.New("unexpected UCI argv")
	}
	if strings.HasPrefix(args[2], filepath.Join(f.root, "etc", "config")) {
		return nil, errors.New("live namespace used")
	}
	data, e := os.ReadFile(filepath.Join(args[2], args[6]))
	if e != nil {
		return nil, e
	}
	_, e = parse(string(data))
	return nil, e
}
func (f *fixture) reload(ctx context.Context, module string) error {
	f.mu.Lock()
	f.reloads = append(f.reloads, module)
	fn := f.reloadFn
	f.mu.Unlock()
	if ctx.Err() != nil {
		return ctx.Err()
	}
	if fn != nil {
		return fn(ctx, module)
	}
	return nil
}
func (f *fixture) options() Options {
	return Options{Root: f.root, DataDir: f.data, Runner: f.run, Reload: f.reload}
}
func openFixture(t *testing.T, f *fixture) *Manager {
	t.Helper()
	m, e := New(f.options())
	if e != nil {
		t.Fatal(e)
	}
	t.Cleanup(func() { m.Close() })
	return m
}
func readFixture(t *testing.T, f *fixture, module string) string {
	t.Helper()
	b, e := os.ReadFile(filepath.Join(f.root, "etc", "config", module))
	if e != nil {
		t.Fatal(e)
	}
	return string(b)
}
func stage(t *testing.T, m *Manager, module, text string) Draft {
	t.Helper()
	d, e := m.Stage(context.Background(), StageRequest{module, text, m.Status().Generation})
	if e != nil {
		t.Fatal(e)
	}
	return d
}
func commit(t *testing.T, m *Manager, d Draft, ack bool) (Operation, error) {
	t.Helper()
	return m.Commit(context.Background(), CommitRequest{[]string{d.ID}, d.Generation, ack})
}
func errorCode(t *testing.T, e error, want string) {
	t.Helper()
	var ce *Error
	if !errors.As(e, &ce) || ce.Code != want {
		t.Fatalf("got %v, want code %s", e, want)
	}
}

func TestStageIsPrivateAndNeverWritesLive(t *testing.T) {
	f := newFixture(t)
	m := openFixture(t, f)
	before := readFixture(t, f, "dhcp")
	d := stage(t, m, "dhcp", testDHCP+" option local '/lan/'\n")
	if !d.Valid || d.Diff == "" || len(d.Risks) != 0 {
		t.Fatalf("bad draft: %#v", d)
	}
	if readFixture(t, f, "dhcp") != before || len(f.reloads) != 0 {
		t.Fatal("stage modified live configuration")
	}
	for _, name := range []string{"state.json"} {
		info, e := os.Stat(filepath.Join(f.data, name))
		if e != nil || info.Mode().Perm() != 0600 {
			t.Fatalf("private mode: %v %v", info, e)
		}
	}
	entries, e := os.ReadDir(f.data)
	if e != nil {
		t.Fatal(e)
	}
	for _, entry := range entries {
		if strings.HasPrefix(entry.Name(), "candidate-") {
			t.Fatal("candidate directory leaked")
		}
	}
	docs, e := m.Documents(context.Background())
	if e != nil || docs.Documents[2].Content != before {
		t.Fatalf("documents returned draft: %v", e)
	}
}
func TestInvalidCandidatesRemainDraftsAndCannotCommit(t *testing.T) {
	for _, text := range []string{"config interface 'lan'\n option ipaddr 'not-an-ip'\n", "config dropbear\n option Port '70000'\n", "config x\n option macaddr 'not-a-mac'\n", "config x\n option thing 'unterminated\n", "config x\n option thing 'ok'\nexec 'touch /tmp/oops'\n"} {
		t.Run(text, func(t *testing.T) {
			f := newFixture(t)
			m := openFixture(t, f)
			before := readFixture(t, f, "network")
			d := stage(t, m, "network", text)
			if d.Valid || len(d.Errors) == 0 {
				t.Fatal("invalid candidate accepted")
			}
			_, e := commit(t, m, d, true)
			errorCode(t, e, "invalid_candidate")
			if readFixture(t, f, "network") != before || len(f.reloads) != 0 {
				t.Fatal("invalid candidate applied")
			}
		})
	}
}
func TestGenerationConflictAndExternalLiveEdits(t *testing.T) {
	f := newFixture(t)
	m := openFixture(t, f)
	d := stage(t, m, "dhcp", testDHCP+" option local '/lan/'\n")
	_, e := m.Commit(context.Background(), CommitRequest{[]string{d.ID}, d.Generation + 1, false})
	errorCode(t, e, "generation_conflict")
	if e = os.WriteFile(filepath.Join(f.root, "etc", "config", "dhcp"), []byte(testDHCP+" option authoritative '1'\n"), 0600); e != nil {
		t.Fatal(e)
	}
	_, e = commit(t, m, d, false)
	errorCode(t, e, "generation_conflict")
	if m.Status().Generation != d.Generation+1 {
		t.Fatal("external edit generation not advanced")
	}
}
func TestSameModuleDraftsPreservedAndCommitAmbiguityRejected(t *testing.T) {
	f := newFixture(t)
	m := openFixture(t, f)
	a := stage(t, m, "dhcp", testDHCP+" option local '/one/'\n")
	b := stage(t, m, "dhcp", testDHCP+" option local '/two/'\n")
	list, e := m.Drafts(context.Background())
	if e != nil || len(list) != 2 {
		t.Fatal("drafts overwritten")
	}
	_, e = m.Commit(context.Background(), CommitRequest{[]string{a.ID, b.ID}, a.Generation, false})
	errorCode(t, e, "duplicate_module")
	if e = m.DeleteDraft(context.Background(), a.ID); e != nil {
		t.Fatal(e)
	}
	op, e := commit(t, m, b, false)
	if e != nil || op.State != "committed" {
		t.Fatalf("commit failed: %#v %v", op, e)
	}
	if readFixture(t, f, "dhcp") != testDHCP+" option local '/two/'\n" {
		t.Fatal("wrong draft applied")
	}
}
func TestConnectivityRiskRequiresAckAndExplicitConfirm(t *testing.T) {
	f := newFixture(t)
	m := openFixture(t, f)
	newText := strings.ReplaceAll(testNetwork, "192.168.31.1", "192.168.32.1")
	d := stage(t, m, "network", newText)
	if len(d.Risks) == 0 {
		t.Fatal("risk missing")
	}
	_, e := commit(t, m, d, false)
	errorCode(t, e, "risk_acknowledgement_required")
	if readFixture(t, f, "network") != testNetwork {
		t.Fatal("ack refusal modified live")
	}
	op, e := commit(t, m, d, true)
	if e != nil || op.State != "pending_confirmation" || op.Deadline == nil {
		t.Fatalf("expected provisional: %#v %v", op, e)
	}
	if m.Status().PendingCommit == nil || readFixture(t, f, "network") != newText {
		t.Fatal("provisional state wrong")
	}
	_, e = m.Stage(context.Background(), StageRequest{"dhcp", testDHCP, op.Generation})
	errorCode(t, e, "confirmation_pending")
	confirmed, e := m.Confirm(context.Background(), op.ID)
	if e != nil || confirmed.State != "committed" || confirmed.Deadline != nil || m.Status().PendingCommit != nil {
		t.Fatalf("confirm failed: %#v %v", confirmed, e)
	}
}
func TestWiFiCredentialChangesAreProvisional(t *testing.T) {
	f := newFixture(t)
	m := openFixture(t, f)
	d := stage(t, m, "wireless", strings.ReplaceAll(testWireless, "test-only", "test-new"))
	if len(d.Risks) == 0 {
		t.Fatal("Wi-Fi connectivity risk not detected")
	}
	_, e := commit(t, m, d, false)
	errorCode(t, e, "risk_acknowledgement_required")
}
func TestFailedReloadRestoresAllDocuments(t *testing.T) {
	f := newFixture(t)
	m := openFixture(t, f)
	first := true
	f.reloadFn = func(_ context.Context, _ string) error {
		if first {
			first = false
			return errors.New("synthetic failure")
		}
		return nil
	}
	a := stage(t, m, "dhcp", testDHCP+" option local '/one/'\n")
	b := stage(t, m, "system", "config system\n option hostname 'new-fixture'\n")
	op, e := m.Commit(context.Background(), CommitRequest{[]string{a.ID, b.ID}, a.Generation, false})
	errorCode(t, e, "reload_failed")
	if op.State != "rolled_back" || readFixture(t, f, "dhcp") != testDHCP || readFixture(t, f, "system") != "config system\n option hostname 'fixture-router'\n" {
		t.Fatalf("rollback failed: %#v", op)
	}
	if !reflect.DeepEqual(f.reloads, []string{"dhcp", "system", "dhcp", "system"}) {
		t.Fatalf("reload all: %v", f.reloads)
	}
}
func TestVerificationFailureRestoresAcceptedState(t *testing.T) {
	f := newFixture(t)
	o := f.options()
	o.Verify = func(context.Context, []string) error { return errors.New("synthetic verification failure") }
	m, e := New(o)
	if e != nil {
		t.Fatal(e)
	}
	defer m.Close()
	d := stage(t, m, "dhcp", testDHCP+" option local '/one/'\n")
	op, e := commit(t, m, d, false)
	errorCode(t, e, "verification_failed")
	if op.State != "rolled_back" || readFixture(t, f, "dhcp") != testDHCP {
		t.Fatal("verification did not roll back")
	}
}
func TestTimeoutAutomaticallyRollsBack(t *testing.T) {
	f := newFixture(t)
	o := f.options()
	o.ConfirmationTimeout = 20 * time.Millisecond
	done := make(chan struct{}, 1)
	count := 0
	o.Reload = func(context.Context, string) error {
		count++
		if count == 2 {
			done <- struct{}{}
		}
		return nil
	}
	m, e := New(o)
	if e != nil {
		t.Fatal(e)
	}
	defer m.Close()
	d := stage(t, m, "network", strings.ReplaceAll(testNetwork, "192.168.31.1", "192.168.32.1"))
	op, e := commit(t, m, d, true)
	if e != nil {
		t.Fatal(e)
	}
	select {
	case <-done:
	case <-time.After(time.Second):
		t.Fatal("deadline rollback did not run")
	}
	s := m.Status()
	if s.PendingCommit != nil || readFixture(t, f, "network") != testNetwork {
		t.Fatal("timeout failed to restore prior state")
	}
	rolled, e := m.Rollback(context.Background(), op.ID)
	if e != nil || rolled.State != "rolled_back" {
		t.Fatalf("idempotent rollback failed: %#v %v", rolled, e)
	}
}
func TestRestartRestoresPendingWithoutWaitingForDeadline(t *testing.T) {
	f := newFixture(t)
	m, e := New(f.options())
	if e != nil {
		t.Fatal(e)
	}
	d := stage(t, m, "network", strings.ReplaceAll(testNetwork, "192.168.31.1", "192.168.32.1"))
	op, e := commit(t, m, d, true)
	if e != nil {
		t.Fatal(e)
	}
	if e = m.Close(); e != nil {
		t.Fatal(e)
	}
	recovered, e := New(f.options())
	if e != nil {
		t.Fatal(e)
	}
	defer recovered.Close()
	if recovered.Status().PendingCommit != nil || readFixture(t, f, "network") != testNetwork {
		t.Fatal("restart did not recover")
	}
	rolled, e := recovered.Rollback(context.Background(), op.ID)
	if e != nil || rolled.State != "rolled_back" {
		t.Fatal("recovery operation missing")
	}
}
func TestRestartRecoversInterruptedAtomicReplacements(t *testing.T) {
	f := newFixture(t)
	m, e := New(f.options())
	if e != nil {
		t.Fatal(e)
	}
	m.mu.Lock()
	live, e := m.syncGeneration()
	if e != nil {
		t.Fatal(e)
	}
	id, e := randomID()
	if e != nil {
		t.Fatal(e)
	}
	m.journal = &journal{Operation: Operation{ID: id, State: "committed", Generation: m.disk.Generation + 1, ChangedModules: []string{"dhcp", "system"}}, Phase: "applying", Before: map[string]snapshot{"dhcp": live["dhcp"], "system": live["system"]}, BaseGeneration: m.disk.Generation}
	if e = m.saveJournal(); e != nil {
		t.Fatal(e)
	}
	if e = atomicWrite(m.livePath("dhcp"), []byte("config dnsmasq\n option domain 'changed'\n"), 0600); e != nil {
		t.Fatal(e)
	}
	m.mu.Unlock()
	m.Close()
	recovered, e := New(f.options())
	if e != nil {
		t.Fatal(e)
	}
	defer recovered.Close()
	if readFixture(t, f, "dhcp") != testDHCP {
		t.Fatal("interrupted replacement not restored")
	}
}
func TestCloseCancelsInjectedRunner(t *testing.T) {
	f := newFixture(t)
	o := f.options()
	entered := make(chan struct{})
	o.Runner = func(ctx context.Context, _ string, _ ...string) ([]byte, error) {
		close(entered)
		<-ctx.Done()
		return nil, ctx.Err()
	}
	m, e := New(o)
	if e != nil {
		t.Fatal(e)
	}
	stageDone := make(chan struct{})
	go func() {
		m.Stage(context.Background(), StageRequest{"dhcp", testDHCP + " option local '/x/'\n", m.Status().Generation})
		close(stageDone)
	}()
	select {
	case <-entered:
	case <-time.After(time.Second):
		t.Fatal("validation did not start")
	}
	closeDone := make(chan struct{})
	go func() { m.Close(); close(closeDone) }()
	select {
	case <-closeDone:
	case <-time.After(time.Second):
		t.Fatal("Close blocked on runner")
	}
	select {
	case <-stageDone:
	case <-time.After(time.Second):
		t.Fatal("stage did not cancel")
	}
	_, e = m.Documents(context.Background())
	errorCode(t, e, "closed")
}
func TestNativeParserQuoteConcatenationAndCommandRejection(t *testing.T) {
	p, e := parse("config wifi-iface\n option ssid 'a'\\''b' # comment\n option key \"space \\\" quote\"\n list network 'lan'\n")
	if e != nil || one(p[0], "ssid") != "a'b" {
		t.Fatalf("quote parsing failed: %#v %v", p, e)
	}
	for _, bad := range []string{"package foo\n", "uci set network.lan.ipaddr=x\n", "config x; reboot\n", "config x\noption x 'a'\n$(reboot)\n"} {
		if _, e = parse(bad); e == nil {
			t.Errorf("command accepted: %s", bad)
		}
	}
}
func TestBoundsAndWhitelist(t *testing.T) {
	f := newFixture(t)
	m := openFixture(t, f)
	_, e := m.Stage(context.Background(), StageRequest{"../passwd", "x", m.Status().Generation})
	errorCode(t, e, "module_not_allowed")
	_, e = m.Stage(context.Background(), StageRequest{"dhcp", strings.Repeat("x", MaxDocumentBytes+1), m.Status().Generation})
	errorCode(t, e, "document_too_large")
	for i := 0; i < MaxDrafts; i++ {
		stage(t, m, "dhcp", testDHCP)
	}
	_, e = m.Stage(context.Background(), StageRequest{"dhcp", testDHCP, m.Status().Generation})
	errorCode(t, e, "draft_limit")
}
func TestFixedReloadCommandsAndSystemGuard(t *testing.T) {
	f := newFixture(t)
	o := f.options()
	o.Reload = nil
	var commands [][]string
	o.Runner = func(ctx context.Context, path string, args ...string) ([]byte, error) {
		if path == "/sbin/uci" {
			if reflect.DeepEqual(args, []string{"-q", "get", "xiaoqiang.common.INITTED"}) {
				return []byte("NO"), nil
			}
			return f.run(ctx, path, args...)
		}
		commands = append(commands, append([]string{path}, args...))
		return nil, nil
	}
	m, e := New(o)
	if e != nil {
		t.Fatal(e)
	}
	defer m.Close()
	d := stage(t, m, "system", "config system\n option hostname 'new-fixture'\n")
	_, e = commit(t, m, d, false)
	errorCode(t, e, "system_reload_unsafe")
	if len(commands) != 0 || readFixture(t, f, "system") != "config system\n option hostname 'fixture-router'\n" {
		t.Fatal("unsafe system reload modified live")
	}
	d = stage(t, m, "dhcp", testDHCP+" option local '/one/'\n")
	_, e = commit(t, m, d, false)
	if e != nil {
		t.Fatal(e)
	}
	if !reflect.DeepEqual(commands, [][]string{{"/etc/init.d/dnsmasq", "reload"}}) {
		t.Fatalf("unsafe command argv: %v", commands)
	}
}

func TestNativeValidationFailureCannotChangeLive(t *testing.T) {
	f := newFixture(t)
	m := openFixture(t, f)
	f.runErr = errors.New("synthetic UCI parser rejection")
	d := stage(t, m, "dhcp", testDHCP+" option local '/x/'\n")
	if d.Valid || len(d.Errors) != 1 || d.Errors[0].Code != "uci_validation_failed" {
		t.Fatalf("native validation not checked: %#v", d)
	}
	_, e := commit(t, m, d, false)
	errorCode(t, e, "invalid_candidate")
	if readFixture(t, f, "dhcp") != testDHCP || len(f.reloads) != 0 {
		t.Fatal("failed native validation touched live")
	}
}
func TestStoreCannotBeOwnedByTwoManagers(t *testing.T) {
	f := newFixture(t)
	m, e := New(f.options())
	if e != nil {
		t.Fatal(e)
	}
	_, e = New(f.options())
	errorCode(t, e, "manager_busy")
	m.Close()
	next, e := New(f.options())
	if e != nil {
		t.Fatal(e)
	}
	next.Close()
}
func TestRollbackReloadFailureDisablesMutationsAndCanRetry(t *testing.T) {
	f := newFixture(t)
	m := openFixture(t, f)
	f.reloadFn = func(context.Context, string) error { return errors.New("synthetic reload failure") }
	d := stage(t, m, "dhcp", testDHCP+" option local '/x/'\n")
	op, e := commit(t, m, d, false)
	errorCode(t, e, "rollback_failed")
	if m.Status().Enabled || m.Status().ErrorCode != "rollback_failed" || readFixture(t, f, "dhcp") != testDHCP {
		t.Fatal("rollback failure state incorrect")
	}
	_, e = m.Stage(context.Background(), StageRequest{"dhcp", testDHCP, m.Status().Generation})
	errorCode(t, e, "rollback_failed")
	f.mu.Lock()
	f.reloadFn = nil
	f.mu.Unlock()
	recovered, e := m.Rollback(context.Background(), op.ID)
	if e != nil || recovered.State != "rolled_back" || !m.Status().Enabled {
		t.Fatalf("retry rollback failed: %#v %v", recovered, e)
	}
}
func TestReloadCannotSilentlyAlterCandidate(t *testing.T) {
	f := newFixture(t)
	m := openFixture(t, f)
	first := true
	f.reloadFn = func(context.Context, string) error {
		if first {
			first = false
			return os.WriteFile(filepath.Join(f.root, "etc", "config", "dhcp"), []byte(testDHCP+" option domain 'changed-by-reload'\n"), 0600)
		}
		return nil
	}
	d := stage(t, m, "dhcp", testDHCP+" option local '/x/'\n")
	op, e := commit(t, m, d, false)
	errorCode(t, e, "verification_failed")
	if op.State != "rolled_back" || readFixture(t, f, "dhcp") != testDHCP {
		t.Fatal("candidate drift accepted")
	}
}
func TestCrossDocumentReferencesUseCandidateSet(t *testing.T) {
	f := newFixture(t)
	m := openFixture(t, f)
	d := stage(t, m, "wireless", testWireless+" option network 'missing-interface'\n")
	if d.Valid || len(d.Errors) == 0 || d.Errors[0].Code != "invalid_reference" {
		t.Fatal("unknown interface reference accepted")
	}
	d = stage(t, m, "wireless", testWireless+" option network 'lan'\n")
	if !d.Valid {
		t.Fatalf("known reference rejected: %#v", d)
	}
}
func TestSecretsAreNotIncludedInErrorsOrLogs(t *testing.T) {
	f := newFixture(t)
	m := openFixture(t, f)
	secret := "not-a-real-secret-fixture"
	d := stage(t, m, "wireless", "config wifi-iface\n option key '"+secret+"\n")
	if d.Valid {
		t.Fatal("invalid syntax accepted")
	}
	for _, e := range d.Errors {
		if strings.Contains(e.Message, secret) {
			t.Fatal("configuration text leaked in diagnostic")
		}
	}
}

func TestExpertDocumentsCannotAddArbitraryExecutionHooks(t *testing.T) {
	f := newFixture(t)
	m := openFixture(t, f)
	d := stage(t, m, "firewall", "config include\n option path '/tmp/user-script'\n option type 'script'\n")
	if d.Valid || d.Errors[0].Code != "execution_hook_not_allowed" {
		t.Fatal("arbitrary script include accepted")
	}
	vendor := "config include\n option path '/etc/vendor-firewall'\n option type 'script'\n"
	if e := os.WriteFile(filepath.Join(f.root, "etc", "config", "firewall"), []byte(vendor), 0600); e != nil {
		t.Fatal(e)
	}
	if _, e := m.Documents(context.Background()); e != nil {
		t.Fatal(e)
	}
	d = stage(t, m, "firewall", vendor+"config defaults\n option input 'ACCEPT'\n")
	if !d.Valid {
		t.Fatalf("preserved vendor include rejected: %#v", d)
	}
	d = stage(t, m, "network", "config interface 'lan'\n option proto 'custom_user_script'\n")
	if d.Valid || d.Errors[0].Code != "protocol_not_registered" {
		t.Fatal("unregistered protocol accepted")
	}
}

func TestRunnerReturnsWhenReloadBackgroundChildHoldsDescriptors(t *testing.T) {
	dir := t.TempDir()
	script := filepath.Join(dir, "reload-fixture")
	// A synthetic reload imitates a factory init script spawning an independent
	// helper that inherits stdout/stderr. It is fixture code, never user input.
	content := "#!/bin/sh\nsleep 3 &\nexit 0\n"
	if err := os.WriteFile(script, []byte(content), 0700); err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	out, err := runCommand(ctx, script, "reload")
	if err != nil {
		t.Fatalf("successful reload treated as failure: %v", err)
	}
	if len(out) != 0 {
		t.Fatal("reload output should be discarded")
	}
}
func TestSafeDiagnosticCauses(t *testing.T) {
	for _, tc := range []struct {
		err  error
		want string
	}{{exec.ErrWaitDelay, "exec_wait_delay"}, {context.Canceled, "cancelled"}, {&os.PathError{Op: "rename", Path: "private-test-secret", Err: syscall.EXDEV}, "errno_exdev"}, {errors.New("private-test-secret"), "operation_error"}} {
		if got := diagnosticCause(tc.err); got != tc.want || strings.Contains(got, "private-test-secret") {
			t.Fatalf("diagnostic %q want %q", got, tc.want)
		}
	}
}

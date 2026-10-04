package captureguardian

import (
	"errors"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

type testClock struct{ nanos atomic.Int64 }

func (c *testClock) now() time.Time          { return time.Unix(100, c.nanos.Load()) }
func (c *testClock) advance(d time.Duration) { c.nanos.Add(int64(d)) }
func fixture(t *testing.T) (*LeaseBook, *testClock, IdentitySnapshot) {
	t.Helper()
	c := &testClock{}
	b, err := NewLeaseBook(c.now, 3*time.Second, strings.Repeat("a", 32))
	must(t, err)
	i := IdentitySnapshot{BootID: "boot-one", Panel: ProcessIdentity{101, 1001, 8, 10}, Core: ProcessIdentity{102, 1002, 8, 11}, ConfigGeneration: 8, ConfigSHA256: strings.Repeat("b", 64)}
	must(t, b.RegisterOwner(ownerOf(i)))
	return b, c, i
}
func must(t *testing.T, err error) {
	t.Helper()
	if err != nil {
		t.Fatal(err)
	}
}
func want(t *testing.T, err, expected error) {
	t.Helper()
	if !errors.Is(err, expected) {
		t.Fatalf("error = %v, want %v", err, expected)
	}
}
func armed(t *testing.T, b *LeaseBook, i IdentitySnapshot) Token {
	t.Helper()
	token, err := b.Prepare(i, strings.Repeat("c", 64))
	must(t, err)
	return token
}
func closed(t *testing.T, p *Permit) {
	t.Helper()
	select {
	case <-p.Done():
	default:
		t.Fatal("permission not canceled before cleanup")
	}
}

func TestActivationRenewAndStickyWithdrawal(t *testing.T) {
	b, c, i := fixture(t)
	token := armed(t, b, i)
	if s := b.State(); s.Phase != PhaseArmed || s.DesiredOff || s.Token != token {
		t.Fatal(s)
	}
	p, err := b.BeginActivation(token)
	must(t, err)
	if p.Token() != token {
		t.Fatal("permit token")
	}
	_, err = b.BeginActivation(token)
	want(t, err, ErrAlreadyCommitted)
	must(t, b.Activate(token, i))
	must(t, p.Check())
	c.advance(time.Second)
	must(t, b.Renew(token, 1, i))
	want(t, b.Renew(token, 1, i), ErrRenewSequence)
	want(t, b.Renew(token, 0, i), ErrRenewSequence)
	c.advance(2 * time.Second)
	if b.Expired() {
		t.Fatal("renew did not extend receipt deadline")
	}
	must(t, b.Withdraw(token))
	closed(t, p)
	if s := b.State(); !s.DesiredOff || s.Phase != PhaseCleanupPending {
		t.Fatal(s)
	}
	want(t, b.Renew(token, 2, i), ErrRevoked)
	want(t, b.Activate(token, i), ErrRevoked)
	_, err = b.BeginActivation(token)
	want(t, err, ErrRevoked)
	_, err = b.Prepare(i, token.PlanSHA256)
	want(t, err, ErrBusy)
	must(t, b.CompleteWithdrawal(token))
	if s := b.State(); s.Phase != PhaseOff || !s.DesiredOff || s.HasToken {
		t.Fatal(s)
	}
	want(t, b.Renew(token, 2, i), ErrStaleToken)
	newToken := armed(t, b, i)
	if newToken.ActivationSequence != token.ActivationSequence+1 {
		t.Fatal("activation sequence did not advance")
	}
}

func TestPreparedAndActiveExpiry(t *testing.T) {
	for _, activate := range []bool{false, true} {
		t.Run(map[bool]string{false: "armed", true: "active"}[activate], func(t *testing.T) {
			b, c, i := fixture(t)
			token := armed(t, b, i)
			var p *Permit
			if activate {
				var err error
				p, err = b.BeginActivation(token)
				must(t, err)
				must(t, b.Activate(token, i))
			}
			c.advance(3 * time.Second)
			if !b.Expired() {
				t.Fatal("deadline equality must expire")
			}
			if p != nil {
				closed(t, p)
				want(t, p.Check(), ErrExpired)
			}
			_, err := b.BeginActivation(token)
			want(t, err, ErrExpired)
			want(t, b.Renew(token, 1, i), ErrExpired)
			if b.State().Phase != PhaseCleanupPending {
				t.Fatal("expired lease lost recovery barrier")
			}
			must(t, b.CompleteWithdrawal(token))
			if !b.Expired() || b.State().Phase != PhaseOff {
				t.Fatal("expiry should remain sticky, with no pending work")
			}
			armed(t, b, i)
			if b.Expired() {
				t.Fatal("explicit prepare did not rearm")
			}
		})
	}
	b, c, i := fixture(t)
	token := armed(t, b, i)
	p, err := b.BeginActivation(token)
	must(t, err)
	c.advance(3 * time.Second)
	want(t, b.Activate(token, i), ErrExpired)
	closed(t, p)
}

func TestIdentityChangesRevokeBeforePublication(t *testing.T) {
	changes := map[string]func(*IdentitySnapshot){
		"boot":                func(i *IdentitySnapshot) { i.BootID = "boot-two" },
		"panel-pid":           func(i *IdentitySnapshot) { i.Panel.PID++ },
		"reused-panel-pid":    func(i *IdentitySnapshot) { i.Panel.StartTime++ },
		"panel-executable":    func(i *IdentitySnapshot) { i.Panel.ExecutableIno++ },
		"core-pid":            func(i *IdentitySnapshot) { i.Core.PID++ },
		"reused-core-pid":     func(i *IdentitySnapshot) { i.Core.StartTime++ },
		"core-device":         func(i *IdentitySnapshot) { i.Core.ExecutableDev++ },
		"generation":          func(i *IdentitySnapshot) { i.ConfigGeneration++ },
		"config-hash":         func(i *IdentitySnapshot) { i.ConfigSHA256 = strings.Repeat("d", 64) },
		"missing-observation": func(i *IdentitySnapshot) { *i = IdentitySnapshot{} },
	}
	for name, change := range changes {
		t.Run(name, func(t *testing.T) {
			b, _, i := fixture(t)
			token := armed(t, b, i)
			p, err := b.BeginActivation(token)
			must(t, err)
			changed := i
			change(&changed)
			want(t, b.Activate(token, changed), ErrIdentityMismatch)
			closed(t, p)
			want(t, b.Activate(token, i), ErrRevoked)
			want(t, b.Renew(token, 1, i), ErrRevoked)
			if s := b.State(); s.Phase != PhaseCleanupPending || !s.DesiredOff {
				t.Fatal(s)
			}
		})
	}
	b, _, i := fixture(t)
	token := armed(t, b, i)
	p, err := b.BeginActivation(token)
	must(t, err)
	changed := i
	changed.Core.StartTime++
	want(t, b.ObserveIdentity(changed), ErrIdentityMismatch)
	closed(t, p)
	b, _, i = fixture(t)
	token = armed(t, b, i)
	p, err = b.BeginActivation(token)
	must(t, err)
	changed = i
	changed.ConfigGeneration++
	want(t, b.Renew(token, 1, changed), ErrIdentityMismatch)
	closed(t, p)
}

func TestSamePlanABAAndRestartRefuseStaleTokens(t *testing.T) {
	b, _, i := fixture(t)
	old := armed(t, b, i)
	must(t, b.Withdraw(old))
	must(t, b.CompleteWithdrawal(old))
	current := armed(t, b, i)
	forged := current
	forged.PlanSHA256 = strings.Repeat("d", 64)
	for _, stale := range []Token{old, forged} {
		want(t, b.Withdraw(stale), ErrStaleToken)
		want(t, b.CompleteWithdrawal(stale), ErrStaleToken)
		want(t, b.Renew(stale, 1, i), ErrStaleToken)
		_, err := b.BeginActivation(stale)
		want(t, err, ErrStaleToken)
		if s := b.State(); s.Token != current || s.Phase != PhaseArmed {
			t.Fatal("stale call disturbed current activation")
		}
	}
	c := &testClock{}
	restarted, err := NewLeaseBook(c.now, 3*time.Second, strings.Repeat("e", 32))
	must(t, err)
	must(t, restarted.RegisterOwner(ownerOf(i)))
	fresh := armed(t, restarted, i)
	if fresh.ActivationSequence != 1 || fresh.GuardianInstance == old.GuardianInstance {
		t.Fatal("bad restart identity")
	}
	_, err = restarted.BeginActivation(old)
	want(t, err, ErrStaleToken)
}

func TestOwnerTakeoverNeedsExplicitRevocationAndProof(t *testing.T) {
	b, _, i := fixture(t)
	token := armed(t, b, i)
	p, err := b.BeginActivation(token)
	must(t, err)
	newOwner := ownerOf(i)
	newOwner.Panel.StartTime++
	want(t, b.RegisterOwner(newOwner), ErrOwnerConflict)
	if err := p.Check(); err != nil {
		t.Fatal("failed registration altered permission")
	}
	must(t, b.Revoke(ReasonOwnerChanged))
	closed(t, p)
	want(t, b.RegisterOwner(newOwner), ErrOwnerConflict)
	must(t, b.CompleteWithdrawal(token))
	must(t, b.RegisterOwner(newOwner))
	nextOwner := newOwner
	nextOwner.Panel.StartTime++
	want(t, b.RegisterOwner(nextOwner), ErrOwnerConflict)
	if !b.State().DesiredOff {
		t.Fatal("registration automatically rearmed")
	}
	_, err = b.Prepare(i, token.PlanSHA256)
	want(t, err, ErrIdentityMismatch)
	i.Panel = newOwner.Panel
	armed(t, b, i)
}

func TestValidationAndFixedErrors(t *testing.T) {
	for _, instance := range []string{"", strings.Repeat("a", 31), strings.Repeat("a", 129), strings.Repeat("/", 32)} {
		_, err := NewLeaseBook(time.Now, time.Second, instance)
		want(t, err, ErrInvalid)
	}
	_, err := NewLeaseBook(nil, time.Second, strings.Repeat("a", 32))
	want(t, err, ErrInvalid)
	_, err = NewLeaseBook(time.Now, 0, strings.Repeat("a", 32))
	want(t, err, ErrInvalid)
	b, _, i := fixture(t)
	bad := ownerOf(i)
	bad.Panel.PID = 0
	want(t, b.RegisterOwner(bad), ErrInvalid)
	bad = ownerOf(i)
	bad.BootID = "private/path"
	want(t, b.RegisterOwner(bad), ErrInvalid)
	for _, hash := range []string{"", strings.Repeat("g", 64), strings.Repeat("a", 65)} {
		_, err = b.Prepare(i, hash)
		want(t, err, ErrInvalid)
	}
	badI := i
	badI.Core.PID = i.Panel.PID
	_, err = b.Prepare(badI, strings.Repeat("c", 64))
	want(t, err, ErrInvalid)
	want(t, b.Revoke(RevokeReason("private caller text")), ErrInvalid)
	if strings.Contains(ErrInvalid.Error(), "private") {
		t.Fatal("error leaked metadata")
	}
	fresh, err := NewLeaseBook(time.Now, time.Second, strings.Repeat("f", 32))
	must(t, err)
	_, err = fresh.Prepare(i, strings.Repeat("c", 64))
	want(t, err, ErrOwnerNotRegistered)
	if fresh.Expired() {
		t.Fatal("fresh off book reports expired")
	}
	b.sequence = ^uint64(0)
	_, err = b.Prepare(i, strings.Repeat("c", 64))
	want(t, err, ErrSequenceExhausted)
}

func TestClockRegressionCancelsAndRenewReceiptDuration(t *testing.T) {
	b, c, i := fixture(t)
	token := armed(t, b, i)
	p, err := b.BeginActivation(token)
	must(t, err)
	c.advance(time.Second)
	must(t, b.Renew(token, 1, i))
	if !b.State().Deadline.Equal(c.now().Add(3 * time.Second)) {
		t.Fatal("deadline not based on guardian receipt")
	}
	c.advance(-time.Second)
	want(t, p.Check(), ErrRevoked)
	closed(t, p)
	if b.State().Reason != ReasonClockRegression {
		t.Fatal("clock regression not latched")
	}
}

func TestConcurrentCommitAndRevoke(t *testing.T) {
	b, _, i := fixture(t)
	token := armed(t, b, i)
	var winners atomic.Int64
	var wg sync.WaitGroup
	for n := 0; n < 32; n++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			_, err := b.BeginActivation(token)
			if err == nil {
				winners.Add(1)
			} else if err != ErrAlreadyCommitted {
				t.Error(err)
			}
		}()
	}
	wg.Wait()
	if winners.Load() != 1 {
		t.Fatal("CAS permitted multiple activations")
	}
	p := b.permit
	for n := 0; n < 32; n++ {
		wg.Add(1)
		go func() { defer wg.Done(); must(t, b.Revoke(ReasonExplicit)); b.State(); _ = p.Check() }()
	}
	wg.Wait()
	closed(t, p)
	if s := b.State(); s.Phase != PhaseCleanupPending || !s.DesiredOff {
		t.Fatal(s)
	}
}

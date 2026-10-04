// Package captureguardian provides an in-memory capture permission contract.
// It does not observe processes, persist state, execute commands, or prove cleanup.
// The caller must supply verified identities and drive expiry with its own monitor.
package captureguardian

import (
	"errors"
	"sync"
	"time"
)

var (
	ErrInvalid            = errors.New("guardian: invalid metadata")
	ErrOwnerNotRegistered = errors.New("guardian: owner not registered")
	ErrOwnerConflict      = errors.New("guardian: owner conflict")
	ErrBusy               = errors.New("guardian: withdrawal required")
	ErrStaleToken         = errors.New("guardian: stale token")
	ErrRevoked            = errors.New("guardian: permission revoked")
	ErrExpired            = errors.New("guardian: lease expired")
	ErrIdentityMismatch   = errors.New("guardian: identity mismatch")
	ErrRenewSequence      = errors.New("guardian: stale renewal sequence")
	ErrAlreadyCommitted   = errors.New("guardian: activation already begun")
	ErrSequenceExhausted  = errors.New("guardian: activation sequence exhausted")
)

// ProcessIdentity contains only observed identity metadata, never an executable path.
type ProcessIdentity struct {
	PID           int
	StartTime     uint64
	ExecutableDev uint64
	ExecutableIno uint64
}

type OwnerIdentity struct {
	BootID string
	Panel  ProcessIdentity
}

type IdentitySnapshot struct {
	BootID           string
	Panel, Core      ProcessIdentity
	ConfigGeneration uint64
	ConfigSHA256     string
}

// Token is comparable. PlanSHA256 hashes the exact persisted journal bytes,
// not just the configuration. It is not a command or a firewall comment.
type Token struct {
	GuardianInstance   string
	ActivationSequence uint64
	Identity           IdentitySnapshot
	PlanSHA256         string
}

type Phase string

const (
	PhaseOff            Phase = "off"
	PhaseArmed          Phase = "armed"
	PhaseActivating     Phase = "activating"
	PhaseActive         Phase = "active"
	PhaseCleanupPending Phase = "cleanup-pending"
)

type RevokeReason string

const (
	ReasonNone            RevokeReason = ""
	ReasonExplicit        RevokeReason = "explicit"
	ReasonOwnerChanged    RevokeReason = "owner-changed"
	ReasonIdentityChanged RevokeReason = "identity-changed"
	ReasonExpired         RevokeReason = "expired"
	ReasonClockRegression RevokeReason = "clock-regression"
)

type Status struct {
	Phase             Phase
	Owner             OwnerIdentity
	OwnerRegistered   bool
	Token             Token
	HasToken          bool
	DesiredOff        bool
	Deadline          time.Time
	LastRenewSequence uint64
	Reason            RevokeReason
}

// LeaseBook serializes transitions, not external mutation. Never hold its mutex
// while doing I/O. The injected clock must be monotonic (time.Now is suitable).
// A fresh instance ID must be supplied on each guardian restart; no lease survives.
type LeaseBook struct {
	mu                  sync.Mutex
	now                 func() time.Time
	lifetime            time.Duration
	instance            string
	sequence            uint64
	lastNow             time.Time
	haveNow             bool
	state               Status
	permit              *Permit
	ownerReplaceAllowed bool
}

func NewLeaseBook(now func() time.Time, lifetime time.Duration, guardianInstance string) (*LeaseBook, error) {
	if now == nil || lifetime <= 0 || len(guardianInstance) < 32 || len(guardianInstance) > 128 || !hex(guardianInstance) {
		return nil, ErrInvalid
	}
	return &LeaseBook{now: now, lifetime: lifetime, instance: guardianInstance, state: Status{Phase: PhaseOff, DesiredOff: true}}, nil
}

func (b *LeaseBook) RegisterOwner(owner OwnerIdentity) error {
	b.mu.Lock()
	defer b.mu.Unlock()
	if !validOwner(owner) {
		return ErrInvalid
	}
	b.tickLocked()
	if b.state.HasToken && owner != b.state.Owner {
		return ErrOwnerConflict
	}
	if b.state.Phase == PhaseCleanupPending {
		return ErrBusy
	}
	if b.state.OwnerRegistered && b.state.Owner != owner && !b.ownerReplaceAllowed {
		return ErrOwnerConflict
	}
	if b.state.Owner != owner || !b.state.OwnerRegistered {
		b.ownerReplaceAllowed = false
	}
	b.state.Owner, b.state.OwnerRegistered = owner, true
	return nil
}

// Prepare is an explicit rearm. Call only after persistence/recompilation proof.
// The returned token is Armed immediately, before any capture-producing hooks.
func (b *LeaseBook) Prepare(identity IdentitySnapshot, planSHA256 string) (Token, error) {
	b.mu.Lock()
	defer b.mu.Unlock()
	if !validIdentity(identity) || len(planSHA256) != 64 || !hex(planSHA256) {
		return Token{}, ErrInvalid
	}
	if !b.state.OwnerRegistered {
		return Token{}, ErrOwnerNotRegistered
	}
	if ownerOf(identity) != b.state.Owner {
		return Token{}, ErrIdentityMismatch
	}
	now, clockOK := b.tickLocked()
	if b.state.HasToken {
		return Token{}, ErrBusy
	}
	if !clockOK {
		return Token{}, ErrRevoked
	}
	if b.sequence == ^uint64(0) {
		return Token{}, ErrSequenceExhausted
	}
	b.sequence++
	token := Token{b.instance, b.sequence, identity, planSHA256}
	b.state = Status{Phase: PhaseArmed, Owner: b.state.Owner, OwnerRegistered: true, Token: token, HasToken: true, Deadline: now.Add(b.lifetime)}
	b.permit = &Permit{book: b, token: token, done: make(chan struct{})}
	return token, nil
}

// BeginActivation is a one-shot CAS. Duplicate Commit must query status instead
// of replaying mutation. Permission does not replace the caller's identity checks.
func (b *LeaseBook) BeginActivation(token Token) (*Permit, error) {
	b.mu.Lock()
	defer b.mu.Unlock()
	if err := b.currentLocked(token); err != nil {
		return nil, err
	}
	if b.state.Phase != PhaseArmed {
		return nil, ErrAlreadyCommitted
	}
	b.state.Phase = PhaseActivating
	return b.permit, nil
}

// Activate records success only after the caller independently verifies resources.
func (b *LeaseBook) Activate(token Token, observed IdentitySnapshot) error {
	b.mu.Lock()
	defer b.mu.Unlock()
	if err := b.currentLocked(token); err != nil {
		return err
	}
	if err := b.identityLocked(observed); err != nil {
		return err
	}
	if b.state.Phase != PhaseActivating {
		return ErrAlreadyCommitted
	}
	b.state.Phase = PhaseActive
	return nil
}

// Renew uses receipt time and a strictly increasing, nonzero renewal sequence.
// Identity/lease validity is required in every live phase, including Armed.
func (b *LeaseBook) Renew(token Token, renewSequence uint64, observed IdentitySnapshot) error {
	b.mu.Lock()
	defer b.mu.Unlock()
	if err := b.currentLocked(token); err != nil {
		return err
	}
	if err := b.identityLocked(observed); err != nil {
		return err
	}
	if renewSequence == 0 || renewSequence <= b.state.LastRenewSequence {
		return ErrRenewSequence
	}
	b.state.LastRenewSequence = renewSequence
	b.state.Deadline = b.lastNow.Add(b.lifetime)
	return nil
}

// ObserveIdentity is for the independent monitor. Invalid or missing observation
// must revoke too: inability to prove identity is not a valid lease.
func (b *LeaseBook) ObserveIdentity(observed IdentitySnapshot) error {
	b.mu.Lock()
	defer b.mu.Unlock()
	b.tickLocked()
	if b.state.HasToken {
		if b.state.Phase == PhaseCleanupPending {
			if b.state.Reason == ReasonExpired {
				return ErrExpired
			}
			return ErrRevoked
		}
		return b.identityLocked(observed)
	}
	if !validIdentity(observed) || !b.state.OwnerRegistered || ownerOf(observed) != b.state.Owner {
		b.revokeLocked(ReasonOwnerChanged)
		return ErrIdentityMismatch
	}
	return nil
}

// Withdraw invalidates before any external off write or cleanup. An old token
// never withdraws a newer activation, even with the same generation/journal.
func (b *LeaseBook) Withdraw(token Token) error {
	b.mu.Lock()
	defer b.mu.Unlock()
	if !b.state.HasToken || token != b.state.Token {
		return ErrStaleToken
	}
	b.revokeLocked(ReasonExplicit)
	return nil
}

func (b *LeaseBook) Revoke(reason RevokeReason) error {
	b.mu.Lock()
	defer b.mu.Unlock()
	switch reason {
	case ReasonExplicit, ReasonOwnerChanged, ReasonIdentityChanged, ReasonExpired, ReasonClockRegression:
	default:
		return ErrInvalid
	}
	b.revokeLocked(reason)
	return nil
}

// CompleteWithdrawal acknowledges externally proved durable off AND absence.
// The book cannot prove these facts. Failures must leave cleanup-pending intact.
func (b *LeaseBook) CompleteWithdrawal(token Token) error {
	b.mu.Lock()
	defer b.mu.Unlock()
	if !b.state.HasToken || token != b.state.Token {
		return ErrStaleToken
	}
	if b.state.Phase != PhaseCleanupPending {
		return ErrBusy
	}
	b.state.Phase, b.state.HasToken, b.state.Token = PhaseOff, false, Token{}
	b.state.LastRenewSequence = 0
	b.permit = nil
	return nil
}

// Expired includes Armed. Once recognized, expiry is sticky until explicit rearm.
// There is no internal timer: the external monitor must drive this method/State.
func (b *LeaseBook) Expired() bool {
	b.mu.Lock()
	defer b.mu.Unlock()
	b.tickLocked()
	return b.state.Reason == ReasonExpired
}

func (b *LeaseBook) State() Status {
	b.mu.Lock()
	defer b.mu.Unlock()
	b.tickLocked()
	return b.state
}
func (b *LeaseBook) Status() Status { return b.State() }

// Permit.Done closes synchronously when revocation is recognized. Call Check
// before each bounded external command; the caller also couples Done to context
// cancellation. This cannot bound an uninterruptible external call or kernel work.
type Permit struct {
	book  *LeaseBook
	token Token
	done  chan struct{}
}

func (p *Permit) Token() Token          { return p.token }
func (p *Permit) Done() <-chan struct{} { return p.done }
func (p *Permit) Check() error {
	p.book.mu.Lock()
	defer p.book.mu.Unlock()
	return p.book.currentLocked(p.token)
}

func (b *LeaseBook) currentLocked(token Token) error {
	if !b.state.HasToken || token != b.state.Token {
		return ErrStaleToken
	}
	b.tickLocked()
	if b.state.Phase == PhaseCleanupPending {
		if b.state.Reason == ReasonExpired {
			return ErrExpired
		}
		return ErrRevoked
	}
	return nil
}
func (b *LeaseBook) identityLocked(observed IdentitySnapshot) error {
	if !validIdentity(observed) || observed != b.state.Token.Identity {
		reason := ReasonIdentityChanged
		if !validOwner(ownerOf(observed)) || ownerOf(observed) != b.state.Owner {
			reason = ReasonOwnerChanged
		}
		b.revokeLocked(reason)
		return ErrIdentityMismatch
	}
	return nil
}
func (b *LeaseBook) revokeLocked(reason RevokeReason) {
	b.ownerReplaceAllowed = true
	if reason == ReasonOwnerChanged {
		b.state.OwnerRegistered = false
	}
	b.state.DesiredOff, b.state.Deadline = true, time.Time{}
	if b.state.Phase != PhaseCleanupPending {
		b.state.Reason = reason
	}
	if b.state.HasToken {
		if b.state.Phase != PhaseCleanupPending {
			close(b.permit.done)
		}
		b.state.Phase = PhaseCleanupPending
	}
}
func (b *LeaseBook) tickLocked() (time.Time, bool) {
	now := b.now()
	if b.haveNow && now.Before(b.lastNow) {
		b.revokeLocked(ReasonClockRegression)
		return b.lastNow, false
	}
	b.lastNow, b.haveNow = now, true
	if b.state.HasToken && b.state.Phase != PhaseCleanupPending && !now.Before(b.state.Deadline) {
		b.revokeLocked(ReasonExpired)
	}
	return now, true
}
func ownerOf(i IdentitySnapshot) OwnerIdentity { return OwnerIdentity{i.BootID, i.Panel} }
func validIdentity(i IdentitySnapshot) bool {
	return validOwner(ownerOf(i)) && validProcess(i.Core) && i.Core.PID != i.Panel.PID && i.ConfigGeneration > 0 && len(i.ConfigSHA256) == 64 && hex(i.ConfigSHA256)
}
func validProcess(p ProcessIdentity) bool {
	return p.PID > 0 && p.StartTime > 0 && p.ExecutableDev > 0 && p.ExecutableIno > 0
}
func validOwner(o OwnerIdentity) bool {
	if !validProcess(o.Panel) || len(o.BootID) == 0 || len(o.BootID) > 128 {
		return false
	}
	for _, c := range o.BootID {
		if !(c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' || c == '-' || c == '_') {
			return false
		}
	}
	return true
}
func hex(s string) bool {
	for _, c := range s {
		if !(c >= 'a' && c <= 'f' || c >= 'A' && c <= 'F' || c >= '0' && c <= '9') {
			return false
		}
	}
	return true
}

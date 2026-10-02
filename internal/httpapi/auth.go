package httpapi

import (
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/hex"
	"net"
	"net/http"
	"sync"
	"time"
)

const sessionCookie = "be6500panel_session"
const maxSessions = 64
const maxAttemptKeys = 1024
const sessionTTL = 8 * time.Hour
const attemptWindow = time.Minute
const maxAttempts = 5

type attempts struct {
	count   int
	expires time.Time
}
type auth struct {
	required bool
	password [32]byte
	mu       sync.Mutex
	sessions map[[32]byte]time.Time
	failures map[string]attempts
	now      func() time.Time
}

func newAuth(password string) *auth {
	return &auth{required: password != "", password: sha256.Sum256([]byte(password)), sessions: map[[32]byte]time.Time{}, failures: map[string]attempts{}, now: time.Now}
}
func (a *auth) authenticated(r *http.Request) bool {
	if !a.required {
		return true
	}
	cookie, err := r.Cookie(sessionCookie)
	if err != nil || len(cookie.Value) != 64 {
		return false
	}
	key := sha256.Sum256([]byte(cookie.Value))
	a.mu.Lock()
	defer a.mu.Unlock()
	expiry, ok := a.sessions[key]
	if ok && !a.now().Before(expiry) {
		delete(a.sessions, key)
		return false
	}
	return ok
}
func (a *auth) allowAttempt(r *http.Request) bool {
	ip, _, err := net.SplitHostPort(r.RemoteAddr)
	if err != nil {
		ip = r.RemoteAddr
	}
	a.mu.Lock()
	defer a.mu.Unlock()
	now := a.now()
	for key, item := range a.failures {
		if !now.Before(item.expires) {
			delete(a.failures, key)
		}
	}
	item := a.failures[ip]
	if item.count >= maxAttempts {
		return false
	}
	if item.count == 0 && len(a.failures) >= maxAttemptKeys {
		return false
	}
	if item.count == 0 {
		item.expires = now.Add(attemptWindow)
	}
	item.count++
	a.failures[ip] = item
	return true
}
func (a *auth) resetAttempts(r *http.Request) {
	ip, _, err := net.SplitHostPort(r.RemoteAddr)
	if err != nil {
		ip = r.RemoteAddr
	}
	a.mu.Lock()
	defer a.mu.Unlock()
	delete(a.failures, ip)
}
func (a *auth) passwordMatches(password string) bool {
	given := sha256.Sum256([]byte(password))
	return subtle.ConstantTimeCompare(given[:], a.password[:]) == 1
}
func (a *auth) createSession(w http.ResponseWriter, r *http.Request) error {
	var token [32]byte
	if _, err := rand.Read(token[:]); err != nil {
		return err
	}
	value := hex.EncodeToString(token[:])
	key := sha256.Sum256([]byte(value))
	now := a.now()
	a.mu.Lock()
	for key, expiry := range a.sessions {
		if !now.Before(expiry) {
			delete(a.sessions, key)
		}
	}
	if len(a.sessions) >= maxSessions { // Evict the oldest session, with a fixed memory bound.
		var oldestKey [32]byte
		var oldest time.Time
		for key, expiry := range a.sessions {
			if oldest.IsZero() || expiry.Before(oldest) {
				oldest = expiry
				oldestKey = key
			}
		}
		delete(a.sessions, oldestKey)
	}
	a.sessions[key] = now.Add(sessionTTL)
	a.mu.Unlock()
	http.SetCookie(w, &http.Cookie{Name: sessionCookie, Value: value, Path: "/", HttpOnly: true, SameSite: http.SameSiteStrictMode, Secure: r.TLS != nil, MaxAge: int(sessionTTL.Seconds())})
	return nil
}
func (a *auth) logout(w http.ResponseWriter, r *http.Request) {
	if cookie, err := r.Cookie(sessionCookie); err == nil {
		key := sha256.Sum256([]byte(cookie.Value))
		a.mu.Lock()
		delete(a.sessions, key)
		a.mu.Unlock()
	}
	http.SetCookie(w, &http.Cookie{Name: sessionCookie, Value: "", Path: "/", HttpOnly: true, SameSite: http.SameSiteStrictMode, Secure: r.TLS != nil, MaxAge: -1})
}

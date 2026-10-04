//! Bounded single-user sessions. Only SHA256 token hashes stay in the session map.
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt;
use std::net::IpAddr;
use std::time::{Duration, Instant};
use subtle::ConstantTimeEq;

pub const SESSION_COOKIE: &str = "be6500panel_session";
pub const MAX_SESSIONS: usize = 64;
pub const MAX_ATTEMPT_KEYS: usize = 1024;
pub const SESSION_TTL: Duration = Duration::from_secs(8 * 60 * 60);
pub const ATTEMPT_WINDOW: Duration = Duration::from_secs(60);
pub const MAX_ATTEMPTS: u8 = 5;

type TokenHash = [u8; 32];
type Entropy = dyn FnMut(&mut [u8; 32]) -> Result<(), AuthError> + Send;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthError {
    EntropyUnavailable,
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("cannot create session")
    }
}
impl std::error::Error for AuthError {}

struct Attempts {
    count: u8,
    expires: Duration,
}

pub struct Auth {
    required: bool,
    password: [u8; 32],
    sessions: HashMap<TokenHash, Duration>,
    attempts: HashMap<IpAddr, Attempts>,
    now: Box<dyn Fn() -> Duration + Send>,
    entropy: Box<Entropy>,
}

impl fmt::Debug for Auth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Auth")
            .field("required", &self.required)
            .field("session_count", &self.sessions.len())
            .field("attempt_key_count", &self.attempts.len())
            .finish_non_exhaustive()
    }
}

/// This response-only value contains a credential; Debug deliberately redacts it.
pub struct SessionCookie(String);
impl SessionCookie {
    pub fn header_value(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for SessionCookie {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionCookie([redacted])")
    }
}

fn hash(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

impl Auth {
    pub fn new(password: &str) -> Self {
        let started = Instant::now();
        Self::with_sources(
            password,
            move || started.elapsed(),
            |bytes| getrandom::fill(bytes).map_err(|_| AuthError::EntropyUnavailable),
        )
    }

    /// Narrow deterministic clock/entropy seams. Production uses `new` and OS entropy.
    /// Time is monotonic elapsed time, not user-selected wall-clock timestamps.
    pub fn with_sources(
        password: &str,
        now: impl Fn() -> Duration + Send + 'static,
        entropy: impl FnMut(&mut [u8; 32]) -> Result<(), AuthError> + Send + 'static,
    ) -> Self {
        Self {
            required: !password.is_empty(),
            password: hash(password.as_bytes()),
            sessions: HashMap::new(),
            attempts: HashMap::new(),
            now: Box::new(now),
            entropy: Box::new(entropy),
        }
    }

    pub fn required(&self) -> bool {
        self.required
    }

    pub fn password_matches(&self, password: &str) -> bool {
        let given = hash(password.as_bytes());
        bool::from(given.ct_eq(&self.password))
    }

    pub fn authenticated(&mut self, cookie: Option<&str>) -> bool {
        if !self.required {
            return true;
        }
        let Some(key) = cookie.and_then(session_key) else {
            return false;
        };
        match self.sessions.get(&key) {
            Some(expiry) if (self.now)() < *expiry => true,
            Some(_) => {
                self.sessions.remove(&key);
                false
            }
            None => false,
        }
    }

    pub fn allow_attempt(&mut self, peer: IpAddr) -> bool {
        let now = (self.now)();
        self.attempts.retain(|_, item| now < item.expires);
        if let Some(item) = self.attempts.get_mut(&peer) {
            if item.count >= MAX_ATTEMPTS {
                return false;
            }
            item.count += 1;
            return true;
        }
        if self.attempts.len() >= MAX_ATTEMPT_KEYS {
            return false;
        }
        self.attempts.insert(
            peer,
            Attempts {
                count: 1,
                expires: now.saturating_add(ATTEMPT_WINDOW),
            },
        );
        true
    }

    pub fn reset_attempts(&mut self, peer: IpAddr) {
        self.attempts.remove(&peer);
    }

    pub fn create_session(&mut self) -> Result<SessionCookie, AuthError> {
        // Entropy failure must not evict an existing session.
        let mut random = [0_u8; 32];
        (self.entropy)(&mut random)?;
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut value = String::with_capacity(64);
        for byte in random {
            value.push(HEX[(byte >> 4) as usize] as char);
            value.push(HEX[(byte & 15) as usize] as char);
        }
        let key = hash(value.as_bytes());
        let now = (self.now)();
        self.sessions.retain(|_, expiry| now < *expiry);
        if self.sessions.len() >= MAX_SESSIONS
            && let Some(oldest) = self
                .sessions
                .iter()
                .min_by_key(|(_, expiry)| *expiry)
                .map(|(key, _)| *key)
        {
            self.sessions.remove(&oldest);
        }
        self.sessions.insert(key, now.saturating_add(SESSION_TTL));
        // This engineering service accepts plain TCP only: no forwarded header
        // or browser-selected scheme can grant a trusted TLS fact or Secure flag.
        Ok(SessionCookie(format!(
            "{SESSION_COOKIE}={value}; Path=/; Max-Age={}; HttpOnly; SameSite=Strict",
            SESSION_TTL.as_secs()
        )))
    }

    pub fn logout(&mut self, cookie: Option<&str>) {
        if let Some(key) = cookie.and_then(session_key) {
            self.sessions.remove(&key);
        }
    }

    pub fn clear_cookie() -> &'static str {
        "be6500panel_session=; Path=/; Max-Age=0; HttpOnly; SameSite=Strict"
    }

    /// Non-secret cardinalities for bound tests and memory sizing.
    pub fn storage_counts(&self) -> (usize, usize) {
        (self.sessions.len(), self.attempts.len())
    }
}

fn session_key(cookie: &str) -> Option<TokenHash> {
    let mut found = None;
    for pair in cookie.split(';') {
        let Some((name, value)) = pair.trim().split_once('=') else {
            continue;
        };
        if name != SESSION_COOKIE {
            continue;
        }
        if found.is_some() {
            return None;
        }
        // net/http accepts quoted cookie values. Only issued lowercase hex64
        // values can authenticate; arbitrary or ambiguous cookies never do.
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .unwrap_or(value);
        if value.len() != 64
            || !value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return None;
        }
        found = Some(hash(value.as_bytes()));
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_tokens_are_only_constant_size_sha256_hashes() {
        let mut auth = Auth::new("test-password");
        let cookie = auth.create_session().unwrap();
        let value = cookie
            .header_value()
            .split(';')
            .next()
            .unwrap()
            .split_once('=')
            .unwrap()
            .1;
        assert_eq!(std::mem::size_of::<TokenHash>(), 32);
        assert!(auth.sessions.contains_key(&hash(value.as_bytes())));
        assert_eq!(auth.sessions.len(), 1);
        assert!(!format!("{auth:?}").contains(value));
    }
}

use be6500_panel::auth::{
    Auth, AuthError, MAX_ATTEMPT_KEYS, MAX_ATTEMPTS, MAX_SESSIONS, SESSION_TTL,
};
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;

fn peer(n: u32) -> IpAddr {
    IpAddr::V4(Ipv4Addr::from(n))
}
fn controlled() -> (Auth, Arc<AtomicU64>) {
    let time = Arc::new(AtomicU64::new(0));
    let clock = time.clone();
    let mut counter = 0_u64;
    let auth = Auth::with_sources(
        "test-password",
        move || Duration::from_secs(clock.load(Ordering::Relaxed)),
        move |bytes| {
            counter += 1;
            bytes.fill(0);
            bytes[..8].copy_from_slice(&counter.to_be_bytes());
            Ok(())
        },
    );
    (auth, time)
}

#[test]
fn password_comparison_no_password_and_secret_safe_debug() {
    let auth = Auth::new("a-private-password");
    assert!(auth.required());
    assert!(auth.password_matches("a-private-password"));
    for invalid in [
        "",
        "a-private-passwore",
        "a-private-password\0",
        "A-private-password",
    ] {
        assert!(!auth.password_matches(invalid));
    }
    assert!(!format!("{auth:?}").contains("a-private-password"));
    let mut empty = Auth::new("");
    assert!(!empty.required());
    assert!(empty.authenticated(None));
}

#[test]
fn session_ttl_invalid_cookie_and_logout() {
    let (mut auth, time) = controlled();
    let cookie = auth.create_session().unwrap();
    assert!(auth.authenticated(Some(cookie.header_value())));
    for invalid in [
        "",
        "other=value",
        "be6500panel_session=short",
        "be6500panel_session=zzzz",
        "be6500panel_session=\"bad\"",
        "be6500panel_session=a; be6500panel_session=b",
    ] {
        assert!(!auth.authenticated(Some(invalid)));
    }
    assert!(!format!("{cookie:?}").contains(cookie.header_value()));
    time.store(SESSION_TTL.as_secs() - 1, Ordering::Relaxed);
    assert!(auth.authenticated(Some(cookie.header_value())));
    time.store(SESSION_TTL.as_secs(), Ordering::Relaxed);
    assert!(!auth.authenticated(Some(cookie.header_value())));
    let next = auth.create_session().unwrap();
    auth.logout(Some(next.header_value()));
    assert!(!auth.authenticated(Some(next.header_value())));
}

#[test]
fn oldest_session_eviction_and_expired_cleanup() {
    let (mut auth, time) = controlled();
    let first = auth.create_session().unwrap();
    let mut last = auth.create_session().unwrap();
    for i in 1..=MAX_SESSIONS {
        time.store(i as u64, Ordering::Relaxed);
        last = auth.create_session().unwrap();
    }
    assert!(!auth.authenticated(Some(first.header_value())));
    assert!(auth.authenticated(Some(last.header_value())));
    assert_eq!(auth.storage_counts(), (MAX_SESSIONS, 0));
    time.store(
        SESSION_TTL.as_secs() + MAX_SESSIONS as u64,
        Ordering::Relaxed,
    );
    auth.create_session().unwrap();
    assert_eq!(auth.storage_counts(), (1, 0));
}

#[test]
fn attempt_limits_peer_ip_window_reset_and_fixed_key_bound() {
    let (mut auth, time) = controlled();
    for _ in 0..MAX_ATTEMPTS {
        assert!(auth.allow_attempt(peer(1)));
    }
    assert!(!auth.allow_attempt(peer(1)));
    assert!(auth.allow_attempt(peer(2)));
    auth.reset_attempts(peer(1));
    assert!(auth.allow_attempt(peer(1)));
    for i in 3..=MAX_ATTEMPT_KEYS as u32 {
        assert!(auth.allow_attempt(peer(i)));
    }
    assert_eq!(auth.storage_counts().1, MAX_ATTEMPT_KEYS);
    assert!(!auth.allow_attempt(peer(MAX_ATTEMPT_KEYS as u32 + 1)));
    time.store(60, Ordering::Relaxed);
    assert!(auth.allow_attempt(peer(MAX_ATTEMPT_KEYS as u32 + 1)));
    assert_eq!(auth.storage_counts().1, 1);
}

#[test]
fn entropy_failure_cannot_create_or_evict_sessions() {
    let mut auth = Auth::with_sources(
        "test-password",
        || Duration::ZERO,
        |_| Err(AuthError::EntropyUnavailable),
    );
    assert!(matches!(
        auth.create_session(),
        Err(AuthError::EntropyUnavailable)
    ));
    assert_eq!(auth.storage_counts(), (0, 0));
    assert_eq!(
        format!("{:?}", AuthError::EntropyUnavailable),
        "EntropyUnavailable"
    );
}

#[test]
fn production_entropy_creates_hex64_cookie_with_plain_listener_attributes() {
    let mut auth = Auth::new("test-password");
    let first = auth.create_session().unwrap();
    let second = auth.create_session().unwrap();
    let value = first
        .header_value()
        .split(';')
        .next()
        .unwrap()
        .split_once('=')
        .unwrap()
        .1;
    assert_eq!(value.len(), 64);
    assert!(
        value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    );
    assert!(
        first
            .header_value()
            .contains("Path=/; Max-Age=28800; HttpOnly; SameSite=Strict")
    );
    assert!(!first.header_value().contains("Secure"));
    assert!(first.header_value() != second.header_value());
}

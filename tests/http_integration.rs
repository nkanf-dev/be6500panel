use be6500_panel::server::{HEALTH_BODY, Service};
use std::fs;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "be6500-http-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn round_trip(service: Service, request: &[u8]) -> Vec<u8> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        service.handle(stream).unwrap();
    });
    let mut client = TcpStream::connect(addr).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    client.write_all(request).unwrap();
    client.shutdown(Shutdown::Write).unwrap();
    let mut response = Vec::new();
    client.read_to_end(&mut response).unwrap();
    server.join().unwrap();
    response
}

fn parts(response: &[u8]) -> (&str, &[u8]) {
    let end = response.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    (
        std::str::from_utf8(&response[..end]).unwrap(),
        &response[end..],
    )
}

fn assert_response(response: &[u8], status: u16, expected: &[u8], head: bool) {
    let (headers, body) = parts(response);
    assert!(
        headers.starts_with(&format!("HTTP/1.1 {status} ")),
        "{headers}"
    );
    assert!(headers.contains(&format!("Content-Length: {}\r\n", expected.len())));
    assert!(headers.contains("Connection: close\r\n"));
    assert!(headers.contains("X-Content-Type-Options: nosniff\r\n"));
    assert!(headers.contains("Cache-Control: no-store\r\n"));
    assert_eq!(body, if head { &[] } else { expected });
}

#[test]
fn http_loopback_exact_health_memory_head_and_missing_contract() {
    let fixture = Fixture::new();
    fs::write(
        fixture.0.join("meminfo"),
        "MemTotal: 100 kB\nMemAvailable: 25 kB\n",
    )
    .unwrap();
    for method in ["GET", "HEAD"] {
        let head = method == "HEAD";
        for (path, body, status) in [
            ("/api/health", HEALTH_BODY, 200),
            (
                "/api/system/memory",
                &b"{\"source\":\"procfs\",\"totalBytes\":102400,\"availableBytes\":25600}"[..],
                200,
            ),
            ("/api/system", &b"{\"error\":\"not found\"}"[..], 404),
            ("/api/config", &b"{\"error\":\"not found\"}"[..], 404),
        ] {
            let request = format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\n\r\n");
            let response = round_trip(Service::new(fixture.0.clone()), request.as_bytes());
            assert_response(&response, status, body, head);
        }
    }
    fs::write(
        fixture.0.join("meminfo"),
        "MemTotal: 100 kB\nMemFree: 25 kB\n",
    )
    .unwrap();
    let response = round_trip(
        Service::new(fixture.0.clone()),
        b"GET /api/system/memory HTTP/1.1\r\nHost: localhost\r\n\r\n",
    );
    assert_response(&response, 503, b"{\"error\":\"memory unavailable\"}", false);
    let response = round_trip(
        Service::new(fixture.0.join("missing")),
        b"HEAD /api/system/memory HTTP/1.1\r\nHost: localhost\r\n\r\n",
    );
    assert_response(&response, 503, b"{\"error\":\"memory unavailable\"}", true);
}

#[test]
fn http_loopback_errors_are_fixed_and_truncated_head_has_no_body() {
    let fixture = Fixture::new();
    for (request, status, body, head) in [
        (
            &b"GET /private-secret HTTP/1.1\r\n\r\n"[..],
            400,
            &b"{\"error\":\"bad request\"}"[..],
            false,
        ),
        (
            b"POST /api/health HTTP/1.1\r\nHost: localhost\r\n\r\n",
            405,
            b"{\"error\":\"method not allowed\"}",
            false,
        ),
        (
            b"GET / HTTP/1.1\r\nHost: localhost\r\n",
            400,
            b"{\"error\":\"bad request\"}",
            false,
        ),
        (
            b"HEAD / HTTP/1.1\r\nHost: localhost\r\n",
            400,
            b"{\"error\":\"bad request\"}",
            true,
        ),
        (
            b"HEAD / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\n\r\n",
            400,
            b"{\"error\":\"bad request\"}",
            true,
        ),
        (
            b"GET / HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n",
            400,
            b"{\"error\":\"bad request\"}",
            false,
        ),
    ] {
        let response = round_trip(Service::new(fixture.0.clone()), request);
        assert_response(&response, status, body, head);
        assert!(
            !response
                .windows(b"private-secret".len())
                .any(|w| w == b"private-secret")
        );
    }
}

#[test]
fn http_slow_trickle_has_one_absolute_request_deadline() {
    let fixture = Fixture::new();
    let service = Service::new(fixture.0.clone());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let started = Instant::now();
        service
            .handle_with_deadlines(stream, Duration::from_millis(180), Duration::from_secs(1))
            .unwrap();
        started.elapsed()
    });
    let mut client = TcpStream::connect(addr).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    client.write_all(b"GET / HTTP/1.1\r\n").unwrap();
    let mut sender = client.try_clone().unwrap();
    let trickle = thread::spawn(move || {
        // In a fixture thread, not in the agent control loop or production service.
        for _ in 0..8 {
            thread::sleep(Duration::from_millis(60));
            if sender.write_all(b"X").is_err() {
                break;
            }
        }
    });
    let mut response = Vec::new();
    client.read_to_end(&mut response).unwrap();
    let elapsed = server.join().unwrap();
    trickle.join().unwrap();
    assert_response(&response, 408, b"{\"error\":\"request timeout\"}", false);
    assert!(
        elapsed < Duration::from_millis(450),
        "per-read timeout used instead: {elapsed:?}"
    );
}

#[test]
fn http_static_large_body_head_and_reserved_api() {
    use be6500_panel::static_files::{StaticFiles, TRANSFER_BUFFER_BYTES};
    let fixture = Fixture::new();
    fs::write(fixture.0.join("index.html"), b"<html>fixture</html>").unwrap();
    let big = vec![b'z'; TRANSFER_BUFFER_BYTES * 20 + 7];
    fs::write(fixture.0.join("big.js"), &big).unwrap();
    fs::create_dir(fixture.0.join("api")).unwrap();
    fs::write(fixture.0.join("api/private"), b"not an API").unwrap();
    for (path, status, body) in [
        ("/", 200, &b"<html>fixture</html>"[..]),
        ("/big.js", 200, &big[..]),
        ("/api/private", 404, &b"{\"error\":\"not found\"}"[..]),
        ("/%2e%2e/private", 404, &b"{\"error\":\"not found\"}"[..]),
        ("/missing", 404, &b"{\"error\":\"not found\"}"[..]),
    ] {
        for method in ["GET", "HEAD"] {
            let service = Service::new(fixture.0.clone())
                .with_static_files(StaticFiles::new(&fixture.0).unwrap());
            let response = round_trip(
                service,
                format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").as_bytes(),
            );
            assert_response(&response, status, body, method == "HEAD");
            if path == "/big.js" {
                assert!(
                    parts(&response)
                        .0
                        .contains("Content-Type: text/javascript; charset=utf-8\r\n")
                );
            }
        }
    }
}

#[test]
fn http_static_write_has_one_absolute_deadline_for_nonreading_client() {
    use be6500_panel::static_files::StaticFiles;
    let fixture = Fixture::new();
    fs::File::create(fixture.0.join("large.bin"))
        .unwrap()
        .set_len(32 * 1024 * 1024)
        .unwrap();
    let service =
        Service::new(fixture.0.clone()).with_static_files(StaticFiles::new(&fixture.0).unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let started = Instant::now();
        let result = service.handle_with_deadlines(
            stream,
            Duration::from_secs(1),
            Duration::from_millis(160),
        );
        (result, started.elapsed())
    });
    let mut client = TcpStream::connect(addr).unwrap();
    client
        .write_all(b"GET /large.bin HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .unwrap();
    // Keep the receiver open but never consume the body.
    let (result, elapsed) = server.join().unwrap();
    assert!(result.is_err());
    assert!(
        elapsed < Duration::from_millis(700),
        "write deadline was renewed: {elapsed:?}"
    );
    drop(client);
}

fn shared_round_trip(service: &Service, request: &[u8]) -> Vec<u8> {
    thread::scope(|scope| {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let client = scope.spawn(move || {
            let mut client = TcpStream::connect(addr).unwrap();
            client
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            client.write_all(request).unwrap();
            client.shutdown(Shutdown::Write).unwrap();
            let mut response = Vec::new();
            client.read_to_end(&mut response).unwrap();
            response
        });
        let (stream, _) = listener.accept().unwrap();
        service.handle(stream).unwrap();
        client.join().unwrap()
    })
}
fn post(path: &str, body: &[u8], extra: &str) -> Vec<u8> {
    let mut request=format!("POST {path} HTTP/1.1\r\nHost: panel.example:18890\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{extra}\r\n",body.len()).into_bytes();
    request.extend_from_slice(body);
    request
}
fn api_error(code: &str, message: &str) -> Vec<u8> {
    format!("{{\"error\":{{\"code\":\"{code}\",\"message\":\"{message}\"}}}}\n").into_bytes()
}

#[test]
fn http_session_browser_contract_protection_public_assets_and_logout() {
    use be6500_panel::{auth::Auth, static_files::StaticFiles};
    let fixture = Fixture::new();
    fs::write(fixture.0.join("index.html"), b"login asset").unwrap();
    let service = Service::new(fixture.0.clone())
        .with_auth(Auth::new("test-password"))
        .with_static_files(StaticFiles::new(&fixture.0).unwrap());
    assert_response(
        &shared_round_trip(
            &service,
            b"GET /api/session HTTP/1.1\r\nHost: localhost\r\n\r\n",
        ),
        200,
        b"{\"authenticated\":false,\"authRequired\":true}\n",
        false,
    );
    for path in ["/api/health", "/api/system/memory", "/api/unknown"] {
        assert_response(
            &shared_round_trip(
                &service,
                format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").as_bytes(),
            ),
            401,
            &api_error("unauthenticated", "Authentication required."),
            false,
        );
    }
    assert_response(
        &shared_round_trip(&service, b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n"),
        200,
        b"login asset",
        false,
    );
    let response = shared_round_trip(
        &service,
        &post(
            "/api/session/login",
            br#"{"password":"test-password"}"#,
            "Origin: http://panel.example:18890\r\n",
        ),
    );
    assert_response(
        &response,
        200,
        b"{\"authenticated\":true,\"authRequired\":true}\n",
        false,
    );
    let headers = parts(&response).0;
    let set_cookie = headers
        .lines()
        .find_map(|l| l.strip_prefix("Set-Cookie: "))
        .unwrap();
    assert!(set_cookie.contains("HttpOnly"));
    assert!(set_cookie.contains("SameSite=Strict"));
    assert!(!set_cookie.contains("Secure"));
    let cookie = set_cookie.split(';').next().unwrap();
    let authenticated = shared_round_trip(
        &service,
        format!("GET /api/session HTTP/1.1\r\nHost: localhost\r\nCookie: {cookie}\r\n\r\n")
            .as_bytes(),
    );
    assert_response(
        &authenticated,
        200,
        b"{\"authenticated\":true,\"authRequired\":true}\n",
        false,
    );
    let health = shared_round_trip(
        &service,
        format!("GET /api/health HTTP/1.1\r\nHost: localhost\r\nCookie: {cookie}\r\n\r\n")
            .as_bytes(),
    );
    assert_response(&health, 200, HEALTH_BODY, false);
    let logout = shared_round_trip(
        &service,
        &post("/api/session/logout", b"", &format!("Cookie: {cookie}\r\n")),
    );
    // Go reports the request cookie after removing its stored hash: false when required.
    assert_response(
        &logout,
        200,
        b"{\"authenticated\":false,\"authRequired\":true}\n",
        false,
    );
    assert!(parts(&logout).0.contains(
        "Set-Cookie: be6500panel_session=; Path=/; Max-Age=0; HttpOnly; SameSite=Strict\r\n"
    ));
    assert_response(
        &shared_round_trip(
            &service,
            format!("GET /api/session HTTP/1.1\r\nHost: localhost\r\nCookie: {cookie}\r\n\r\n")
                .as_bytes(),
        ),
        200,
        b"{\"authenticated\":false,\"authRequired\":true}\n",
        false,
    );
}

#[test]
fn http_login_fixed_errors_rate_limit_and_no_password_semantics() {
    use be6500_panel::auth::Auth;
    let fixture = Fixture::new();
    let service = Service::new(fixture.0.clone()).with_auth(Auth::new("test-password"));
    for _ in 0..5 {
        let response = shared_round_trip(
            &service,
            &post(
                "/api/session/login",
                br#"{"password":"private-secret"}"#,
                "",
            ),
        );
        assert_response(
            &response,
            401,
            &api_error("invalid_password", "Invalid password."),
            false,
        );
        assert!(!response.windows(14).any(|b| b == b"private-secret"));
    }
    let response = shared_round_trip(
        &service,
        &post("/api/session/login", br#"{"password":"test-password"}"#, ""),
    );
    assert_response(
        &response,
        429,
        &api_error("rate_limited", "Too many login attempts; try again later."),
        false,
    );
    assert!(parts(&response).0.contains("Retry-After: 60\r\n"));
    let no_password = Service::new(fixture.0.clone());
    for req in [
        b"GET /api/session HTTP/1.1\r\nHost: localhost\r\n\r\n".to_vec(),
        post("/api/session/login", br#"{"password":"anything"}"#, ""),
        post("/api/session/logout", b"", ""),
    ] {
        let response = shared_round_trip(&no_password, &req);
        assert_response(
            &response,
            200,
            b"{\"authenticated\":true,\"authRequired\":false}\n",
            false,
        );
    }
    let login = shared_round_trip(
        &no_password,
        &post("/api/session/login", br#"{"password":"anything"}"#, ""),
    );
    assert!(!parts(&login).0.contains("Set-Cookie:"));
}

#[test]
fn http_origin_frpc_regression_refuses_before_attempts_and_logout() {
    use be6500_panel::auth::Auth;
    let fixture = Fixture::new();
    let service = Service::new(fixture.0.clone()).with_auth(Auth::new("test-password"));
    for origin in [
        "http://evil.example",
        "https://panel.example:18890",
        "http://panel.example:18890/",
        "http://user@panel.example:18890",
        "null",
        "http://panel.example:18890?query",
        "http://panel.example:18890#fragment",
    ] {
        let req = post(
            "/api/session/login",
            br#"{"password":"test-password"}"#,
            &format!("Origin: {origin}\r\nX-Forwarded-Proto: https\r\n"),
        );
        let response = shared_round_trip(&service, &req);
        assert_response(
            &response,
            403,
            &api_error("origin_rejected", "Unsafe requests must be same-origin."),
            false,
        );
        assert!(!parts(&response).0.contains("Set-Cookie:"));
    }
    let response = shared_round_trip(
        &service,
        &post(
            "/api/session/login",
            br#"{"password":"test-password"}"#,
            "Origin: http://panel.example:18890\r\nX-Forwarded-Proto: https\r\n",
        ),
    );
    assert_response(
        &response,
        200,
        b"{\"authenticated\":true,\"authRequired\":true}\n",
        false,
    );
    let cookie = parts(&response)
        .0
        .lines()
        .find_map(|l| l.strip_prefix("Set-Cookie: "))
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    let response = shared_round_trip(
        &service,
        &post(
            "/api/session/logout",
            b"",
            &format!("Cookie: {cookie}\r\nSec-Fetch-Site: cross-site\r\n"),
        ),
    );
    assert_response(
        &response,
        403,
        &api_error("origin_rejected", "Unsafe requests must be same-origin."),
        false,
    );
    assert_response(
        &shared_round_trip(
            &service,
            format!("GET /api/session HTTP/1.1\r\nHost: localhost\r\nCookie: {cookie}\r\n\r\n")
                .as_bytes(),
        ),
        200,
        b"{\"authenticated\":true,\"authRequired\":true}\n",
        false,
    );
}

#[test]
fn http_json_content_type_and_body_limit_rejection_without_login() {
    use be6500_panel::auth::Auth;
    let fixture = Fixture::new();
    for body in [
        br#"{"password":"test-password","password":"test-password"}"#.as_slice(),
        br#"{"password":"test-password","unknown":0}"#,
        br#"{"Password":"test-password"}"#,
        br#"{"password":7}"#,
        br#"{"password":null}"#,
        br#"{}"#,
        br#"[]"#,
        br#"{"password":"test-password"} {}"#,
    ] {
        let service = Service::new(fixture.0.clone()).with_auth(Auth::new("test-password"));
        let response = shared_round_trip(&service, &post("/api/session/login", body, ""));
        assert!(parts(&response).0.starts_with("HTTP/1.1 400 "));
        assert!(!parts(&response).0.contains("Set-Cookie:"));
    }
    for content_type in [
        "text/plain",
        "application/jsonx",
        "application/json;",
        "application/json; charset=",
        "application/json; charset=utf-8; charset=ascii",
    ] {
        let service = Service::new(fixture.0.clone()).with_auth(Auth::new("test-password"));
        let request = format!(
            "POST /api/session/login HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\nContent-Type: {content_type}\r\n\r\n{{}}"
        );
        assert_response(
            &shared_round_trip(&service, request.as_bytes()),
            415,
            &api_error(
                "unsupported_media_type",
                "Content-Type must be application/json.",
            ),
            false,
        );
    }
    for (path, length) in [("/api/session/login", 4097), ("/api/session/logout", 65537)] {
        let service = Service::new(fixture.0.clone()).with_auth(Auth::new("test-password"));
        let request = format!(
            "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Length: {length}\r\nContent-Type: application/json\r\n\r\n"
        );
        assert_response(
            &shared_round_trip(&service, request.as_bytes()),
            413,
            &api_error("body_too_large", "Request body exceeds the endpoint limit."),
            false,
        );
    }
}

#[test]
fn http_body_uses_remaining_header_deadline_and_truncation_cannot_login() {
    use be6500_panel::auth::Auth;
    let fixture = Fixture::new();
    let service = Service::new(fixture.0.clone()).with_auth(Auth::new("test-password"));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let started = Instant::now();
        service
            .handle_with_deadlines(stream, Duration::from_millis(180), Duration::from_secs(1))
            .unwrap();
        started.elapsed()
    });
    let mut client = TcpStream::connect(addr).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    client
        .write_all(b"POST /api/session/login HTTP/1.1\r\nHost: localhost\r\n")
        .unwrap();
    thread::sleep(Duration::from_millis(110));
    client
        .write_all(b"Content-Length: 28\r\nContent-Type: application/json\r\n\r\n{\"password\":\"")
        .unwrap();
    let mut response = Vec::new();
    client.read_to_end(&mut response).unwrap();
    assert_response(&response, 408, b"{\"error\":\"request timeout\"}", false);
    assert!(server.join().unwrap() < Duration::from_millis(260));
    let service = Service::new(fixture.0.clone()).with_auth(Auth::new("test-password"));
    assert_response(&shared_round_trip(&service,b"POST /api/session/login HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: 28\r\n\r\n{\"password\":\""),400,b"{\"error\":\"bad request\"}",false);
}

#[test]
fn http_non_session_post_is_never_a_diagnostic_or_static_success() {
    use be6500_panel::{auth::Auth, static_files::StaticFiles};
    let fixture = Fixture::new();
    fs::write(fixture.0.join("index.html"), b"public login").unwrap();
    let service = Service::new(fixture.0.clone())
        .with_auth(Auth::new("test-password"))
        .with_static_files(StaticFiles::new(&fixture.0).unwrap());
    let login = shared_round_trip(
        &service,
        &post("/api/session/login", br#"{"password":"test-password"}"#, ""),
    );
    let cookie = parts(&login)
        .0
        .lines()
        .find_map(|l| l.strip_prefix("Set-Cookie: "))
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    for path in ["/api/health", "/api/system/memory", "/", "/index.html"] {
        assert_response(
            &shared_round_trip(
                &service,
                &post(
                    path,
                    b"",
                    &format!("Cookie: {cookie}\r\nOrigin: http://panel.example:18890\r\n"),
                ),
            ),
            405,
            b"{\"error\":\"method not allowed\"}",
            false,
        );
    }
}

#[test]
fn http_login_strict_json_exact_go_error_envelopes() {
    use be6500_panel::auth::Auth;
    let fixture = Fixture::new();
    for (body, code, message) in [
        (
            br#"{"password":"p","password":"q"}"#.as_slice(),
            "invalid_json",
            "Provide one JSON object with unique field names.",
        ),
        (
            br#"{"password":"p","extra":{"a":1,"a":2}}"#,
            "invalid_json",
            "Provide one JSON object with unique field names.",
        ),
        (
            br#"{"password":"p"} {}"#,
            "invalid_json",
            "Provide only one JSON object.",
        ),
        (br#"[]"#, "invalid_json", "JSON body must be an object."),
        (br#"null"#, "invalid_json", "JSON body must be an object."),
        (
            br#"{"password":null}"#,
            "invalid_input",
            "Required fields must be present and non-null.",
        ),
        (
            br#"{}"#,
            "invalid_input",
            "Required fields must be present and non-null.",
        ),
        (
            br#"{"Password":"p"}"#,
            "invalid_input",
            "Required fields must be present and non-null.",
        ),
        (
            br#"{"password":1}"#,
            "invalid_json",
            "JSON fields or types do not match the request contract.",
        ),
        (
            br#"{"password":"p","extra":1}"#,
            "invalid_json",
            "JSON fields or types do not match the request contract.",
        ),
    ] {
        let service = Service::new(fixture.0.clone()).with_auth(Auth::new("test-password"));
        assert_response(
            &shared_round_trip(&service, &post("/api/session/login", body, "")),
            400,
            &api_error(code, message),
            false,
        );
    }
}

#[test]
fn http_entropy_failure_is_fixed_and_no_password_logout_matches_browser() {
    use be6500_panel::auth::{Auth, AuthError};
    let fixture = Fixture::new();
    let service = Service::new(fixture.0.clone()).with_auth(Auth::with_sources(
        "test-password",
        || Duration::ZERO,
        |_| Err(AuthError::EntropyUnavailable),
    ));
    let response = shared_round_trip(
        &service,
        &post("/api/session/login", br#"{"password":"test-password"}"#, ""),
    );
    assert_response(
        &response,
        500,
        &api_error("session_unavailable", "Cannot create session."),
        false,
    );
    assert!(!parts(&response).0.contains("Set-Cookie:"));
    let service = Service::new(fixture.0.clone());
    let response = shared_round_trip(
        &service,
        b"POST /api/session/logout HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n",
    );
    assert_response(
        &response,
        200,
        b"{\"authenticated\":true,\"authRequired\":false}\n",
        false,
    );
    assert!(parts(&response).0.contains(
        "Set-Cookie: be6500panel_session=; Path=/; Max-Age=0; HttpOnly; SameSite=Strict\r\n"
    ));
}

#[test]
fn http_bad_post_framing_cannot_logout_or_create_session() {
    use be6500_panel::auth::Auth;
    let fixture = Fixture::new();
    let service = Service::new(fixture.0.clone()).with_auth(Auth::new("test-password"));
    let login = shared_round_trip(
        &service,
        &post("/api/session/login", br#"{"password":"test-password"}"#, ""),
    );
    let cookie = parts(&login)
        .0
        .lines()
        .find_map(|l| l.strip_prefix("Set-Cookie: "))
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    for framing in [
        "",
        "Content-Length: 0\r\nContent-Length: 0\r\n",
        "Content-Length: +0\r\n",
        "Content-Length: 0\r\nTransfer-Encoding: identity\r\n",
        "Content-Length: 0\r\nTransfer-Encoding: chunked\r\n",
        "Content-Length: 0\r\nOrigin: http://localhost\r\nOrigin: http://localhost\r\n",
    ] {
        let request = format!(
            "POST /api/session/logout HTTP/1.1\r\nHost: localhost\r\nCookie: {cookie}\r\n{framing}\r\n"
        );
        let response = shared_round_trip(&service, request.as_bytes());
        assert_response(&response, 400, b"{\"error\":\"bad request\"}", false);
        assert!(!parts(&response).0.contains("Set-Cookie:"));
        let response = shared_round_trip(
            &service,
            format!("GET /api/session HTTP/1.1\r\nHost: localhost\r\nCookie: {cookie}\r\n\r\n")
                .as_bytes(),
        );
        assert_response(
            &response,
            200,
            b"{\"authenticated\":true,\"authRequired\":true}\n",
            false,
        );
    }
    for invalid_cookie in [
        "be6500panel_session=short",
        "be6500panel_session=zzzz",
        "other=value",
    ] {
        assert_response(&shared_round_trip(&service,format!("GET /api/health HTTP/1.1\r\nHost: localhost\r\nCookie: {invalid_cookie}\r\n\r\n").as_bytes()),401,&api_error("unauthenticated","Authentication required."),false);
    }
}

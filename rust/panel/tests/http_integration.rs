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

#![cfg(unix)]
use be6500_panel::{
    auth::Auth,
    server::Service,
    server_loop::{LoopError, serve},
};
use std::{
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
fn exchange(address: std::net::SocketAddr, request: &[u8]) -> Vec<u8> {
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream.write_all(request).unwrap();
    stream.shutdown(Shutdown::Write).unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();
    response
}
#[test]
fn diagnostic_loop_keeps_exact_health_body_and_cancels_without_scheduler() {
    let mut listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let stop = cancel.clone();
    let caller = thread::spawn(move || {
        let health = exchange(
            address,
            b"GET /api/health HTTP/1.1\r\nHost: localhost\r\n\r\n",
        );
        assert!(health.starts_with(b"HTTP/1.1 200 "));
        assert!(health.ends_with(be6500_panel::server::HEALTH_BODY));
        let runtime = exchange(
            address,
            b"GET /api/runtime HTTP/1.1\r\nHost: localhost\r\n\r\n",
        );
        assert!(runtime.starts_with(b"HTTP/1.1 503 "));
        assert!(
            String::from_utf8(runtime)
                .unwrap()
                .contains("runtime_unavailable")
        );
        stop.store(true, Ordering::Release);
    });
    let service = Service::new("/proc".into());
    let started = Instant::now();
    serve(&mut listener, &service, None, &cancel).unwrap();
    caller.join().unwrap();
    assert!(started.elapsed() < Duration::from_secs(2));
}
#[test]
fn nonloopback_listener_needs_auth_before_any_request() {
    let mut listener = TcpListener::bind("0.0.0.0:0").unwrap();
    let cancel = AtomicBool::new(true);
    let service = Service::new("/proc".into());
    assert_eq!(
        serve(&mut listener, &service, None, &cancel),
        Err(LoopError::Authentication)
    );
    let authenticated = Service::new("/proc".into()).with_auth(Auth::new("test-only"));
    assert_eq!(serve(&mut listener, &authenticated, None, &cancel), Ok(()));
}

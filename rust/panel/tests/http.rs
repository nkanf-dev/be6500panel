use be6500_panel::http::{ErrorKind, MAX_HEADER_BYTES, Method, parse_request};

#[test]
fn http_get_head_and_versions() {
    for (method, expected) in [("GET", Method::Get), ("HEAD", Method::Head)] {
        let bytes = format!("{method} /api/health HTTP/1.1\r\nHost: localhost\r\n\r\n");
        let request = parse_request(bytes.as_bytes()).unwrap();
        assert_eq!(request.method, expected);
        assert_eq!(request.path(), "/api/health");
    }
    assert!(parse_request(b"GET /api/health HTTP/1.0\r\n\r\n").is_ok());
}

#[test]
fn http_ambiguous_framing_is_rejected() {
    for request in [
        &b"GET /api/health HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\nContent-Length: 0\r\n\r\n"[..],
        b"GET /api/health HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n",
        b"GET /api/health HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: identity\r\nContent-Length: 0\r\n\r\n",
        b"GET /api/health HTTP/1.1\r\nHost: localhost\r\nContent-Length: 1\r\n\r\nx",
        b"HEAD /api/health HTTP/1.1\r\nHost: localhost\r\nContent-Length: +0\r\n\r\n",
        b"GET /api/health HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0, 0\r\n\r\n",
        b"GET /api/health HTTP/1.1\r\nHost: localhost\r\n\r\nGET / HTTP/1.0\r\n\r\n",
    ] {
        assert!(parse_request(request).is_err(), "accepted {request:?}");
    }
    assert!(
        parse_request(b"GET / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n").is_ok()
    );
}

#[test]
fn http_requires_valid_host_headers_path_and_method() {
    for bad in [
        &b"GET / HTTP/1.1\r\n\r\n"[..],
        b"GET / HTTP/1.1\r\nHost: localhost\r\nHOST: localhost\r\n\r\n",
        b"GET / HTTP/1.1\r\nHost: \r\n\r\n",
        b"GET / HTTP/1.1\r\nHost: local host\r\n\r\n",
        b"GET http://localhost/ HTTP/1.1\r\nHost: localhost\r\n\r\n",
        b"GET /bad%zz HTTP/1.1\r\nHost: localhost\r\n\r\n",
        b"GET /bad%0a HTTP/1.1\r\nHost: localhost\r\n\r\n",
        b"GET /bad%c2%85 HTTP/1.1\r\nHost: localhost\r\n\r\n",
        b"GET /bad%c2%9f HTTP/1.1\r\nHost: localhost\r\n\r\n",
        b"GET /#fragment HTTP/1.1\r\nHost: localhost\r\n\r\n",
        b"GET / HTTP/2.0\r\nHost: localhost\r\n\r\n",
        b"GET / HTTP/1.1\nHost: localhost\n\n",
        b"GET / HTTP/1.1\r\nHost: localhost\r\n folded: value\r\n\r\n",
        b"GET / HTTP/1.1\r\nHost: localhost\r\nBad Name: value\r\n\r\n",
        b"GET / HTTP/1.1\r\nHost: localhost\r\nX-Test: a\x00b\r\n\r\n",
        b"GET / HTTP/1.1\r\nHost: localhost\r\n",
    ] {
        assert!(parse_request(bad).is_err(), "accepted {bad:?}");
    }
    assert_eq!(
        parse_request(b"POST / HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .unwrap_err()
            .kind,
        ErrorKind::MethodNotAllowed
    );
    assert!(
        parse_request(b"HEAD / HTTP/1.1\r\n\r\n")
            .unwrap_err()
            .head_only
    );
}

#[test]
fn http_header_size_and_count_limits() {
    let oversized = format!(
        "GET / HTTP/1.1\r\nHost: localhost\r\nX-Test: {}\r\n\r\n",
        "a".repeat(MAX_HEADER_BYTES)
    );
    assert_eq!(
        parse_request(oversized.as_bytes()).unwrap_err().kind,
        ErrorKind::HeadersTooLarge
    );
    let mut request = "GET / HTTP/1.1\r\nHost: localhost\r\n".to_string();
    for _ in 0..63 {
        request.push_str("X-A: b\r\n");
    }
    assert!(parse_request(format!("{request}\r\n").as_bytes()).is_ok());
    request.push_str("X-B: c\r\n\r\n");
    assert_eq!(
        parse_request(request.as_bytes()).unwrap_err().kind,
        ErrorKind::HeadersTooLarge
    );
    let long_path = format!(
        "GET /{} HTTP/1.1\r\nHost: localhost\r\n\r\n",
        "a".repeat(2048)
    );
    assert_eq!(
        parse_request(long_path.as_bytes()).unwrap_err().kind,
        ErrorKind::TargetTooLong
    );
}

#[test]
fn http_post_framing_bounds_and_needed_headers() {
    use be6500_panel::http::{MAX_BODY_BYTES, MAX_LOGIN_BODY_BYTES};
    let req = parse_request(b"POST /api/session/login HTTP/1.1\r\nHost: panel.example:18890\r\nOrigin: http://panel.example:18890\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: 24\r\nCookie: other=x\r\n\r\n").unwrap();
    assert_eq!(req.method, Method::Post);
    assert_eq!(req.content_length, 24);
    assert_eq!(req.host, Some("panel.example:18890"));
    assert_eq!(req.origin, Some("http://panel.example:18890"));
    assert_eq!(req.cookie, Some("other=x"));
    for (path, length, good) in [
        ("/api/session/login", MAX_LOGIN_BODY_BYTES, true),
        ("/api/session/login", MAX_LOGIN_BODY_BYTES + 1, false),
        ("/api/session/logout", MAX_BODY_BYTES, true),
        ("/api/session/logout", MAX_BODY_BYTES + 1, false),
    ] {
        let bytes =
            format!("POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Length: {length}\r\n\r\n");
        assert_eq!(parse_request(bytes.as_bytes()).is_ok(), good);
    }
    for headers in [
        "",
        "Content-Length: +1\r\n",
        "Content-Length: 1,1\r\n",
        "Content-Length: 0\r\nContent-Length: 0\r\n",
        "Content-Length: 0\r\nTransfer-Encoding: identity\r\n",
        "Content-Length: 0\r\nOrigin: http://localhost\r\nOrigin: http://localhost\r\n",
        "Content-Length: 0\r\nCookie: a=b\r\nCookie: c=d\r\n",
        "Content-Length: 0\r\nContent-Type: application/json\r\nContent-Type: application/json\r\n",
    ] {
        assert!(
            parse_request(
                format!("POST /api/session/login HTTP/1.1\r\nHost: localhost\r\n{headers}\r\n")
                    .as_bytes()
            )
            .is_err()
        );
    }
}

#[test]
fn http_request_debug_redacts_cookie_and_target() {
    let request=parse_request(b"GET /private-secret HTTP/1.1\r\nHost: localhost\r\nCookie: be6500panel_session=private-secret\r\n\r\n").unwrap();
    assert!(!format!("{request:?}").contains("private-secret"));
}

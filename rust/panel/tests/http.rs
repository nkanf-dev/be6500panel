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
    use be6500_panel::http::{MAX_BODY_BYTES, MAX_LOGIN_BODY_BYTES, MAX_RULES_BODY_BYTES};
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
        ("/api/proxy/local-rules", MAX_RULES_BODY_BYTES, true),
        ("/api/proxy/local-rules", MAX_RULES_BODY_BYTES + 1, false),
        ("/api/proxy/local-rules/preview", MAX_RULES_BODY_BYTES, true),
        (
            "/api/proxy/local-rules/preview",
            MAX_RULES_BODY_BYTES + 1,
            false,
        ),
        ("/api/proxy/local-rules/apply", MAX_BODY_BYTES, true),
        ("/api/proxy/select", MAX_BODY_BYTES, true),
        ("/api/proxy/nodes", 0, false),
        ("/api/proxy/subscription", 0, false),
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


#[test]
fn capture_exact_methods_and_query_metadata_are_preserved() {
    for (method, expected, headers, length) in [
        ("GET", Method::Get, "", 0),
        ("HEAD", Method::Head, "Content-Length: 0\r\n", 0),
        ("POST", Method::Post, "Content-Length: 23\r\n", 23),
        ("DELETE", Method::Delete, "", 0),
        ("DELETE", Method::Delete, "Content-Length: 0\r\n", 0),
    ] {
        for target in ["/api/proxy/capture", "/api/proxy/capture?mode=private%20value"] {
            let bytes = format!("{method} {target} HTTP/1.1\r\nHost: panel.example:18890\r\nOrigin: http://panel.example:18890\r\nCookie: private-cookie\r\nSec-Fetch-Site: same-origin\r\n{headers}\r\n");
            let request = parse_request(bytes.as_bytes()).unwrap();
            assert_eq!(request.method, expected);
            assert_eq!(request.target, target);
            assert_eq!(request.path(), "/api/proxy/capture");
            assert_eq!(request.content_length, length);
            assert_eq!(request.host, Some("panel.example:18890"));
            assert_eq!(request.origin, Some("http://panel.example:18890"));
            assert_eq!(request.cookie, Some("private-cookie"));
            assert_eq!(request.fetch_site, Some("same-origin"));
            assert!(request.same_origin());
        }
    }
    assert!(parse_request(b"DELETE /api/proxy/capture HTTP/1.0\r\n\r\n").is_ok());
}

#[test]
fn capture_post_requires_length_and_keeps_64kib_body_cap() {
    use be6500_panel::http::MAX_BODY_BYTES;
    for (length, accepted) in [(0, true), (MAX_BODY_BYTES, true), (MAX_BODY_BYTES + 1, false)] {
        let bytes = format!("POST /api/proxy/capture HTTP/1.1\r\nHost: localhost\r\nContent-Length: {length}\r\n\r\n");
        match parse_request(bytes.as_bytes()) {
            Ok(request) => { assert!(accepted); assert_eq!(request.content_length, length); }
            Err(error) => { assert!(!accepted); assert_eq!(error.kind, ErrorKind::BodyTooLarge); }
        }
    }
    assert_eq!(parse_request(b"POST /api/proxy/capture HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap_err().kind,
        ErrorKind::BadRequest);
}

#[test]
fn capture_delete_is_empty_only_and_ambiguous_framing_is_bad_request() {
    for headers in [
        "Content-Length: 1\r\n", "Content-Length: 65537\r\n",
        "Content-Length: +0\r\n", "Content-Length: -0\r\n", "Content-Length: \r\n",
        "Content-Length: 0,0\r\n", "Content-Length: 0x0\r\n",
        "Content-Length: 999999999999999999999999999999999999\r\n",
        "Content-Length: 0\r\ncontent-length: 0\r\n",
        "Content-Length: 0\r\nContent-Length: 1\r\n",
        "Transfer-Encoding: chunked\r\n", "Transfer-Encoding: identity\r\n",
        "Transfer-Encoding: \r\n", "Content-Length: 0\r\nTransfer-Encoding: chunked\r\n",
        "Content-Length: 0\r\nContent-Type: application/json\r\ncontent-type: application/json\r\n",
        "Content-Length : 0\r\n", " Content-Length: 0\r\n",
    ] {
        let bytes = format!("DELETE /api/proxy/capture HTTP/1.1\r\nHost: localhost\r\n{headers}\r\n");
        let error = parse_request(bytes.as_bytes()).unwrap_err();
        assert_eq!(error.kind, ErrorKind::BadRequest, "{headers:?}");
        assert!(!error.head_only);
    }
    for suffix in ["{}", "0\r\n\r\n", "GET / HTTP/1.0\r\n\r\n"] {
        let bytes = format!("DELETE /api/proxy/capture HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n{suffix}");
        assert_eq!(parse_request(bytes.as_bytes()).unwrap_err().kind, ErrorKind::BadRequest);
    }
}

#[test]
fn capture_delete_never_admits_other_session_static_or_api_paths() {
    for path in ["/", "/index.html", "/api/session/login", "/api/session/logout",
        "/api/runtime/stop", "/api/proxy/select", "/api/proxy/local-rules",
        "/api/proxy/capture/", "/api/proxy/capture/other", "/api/proxy/capturE",
        "/api/proxy/%63apture", "/api/proxy/capture%3Fmode=x", "/api/proxy/capture.json"] {
        let bytes = format!("DELETE {path} HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n");
        assert_eq!(parse_request(bytes.as_bytes()).unwrap_err().kind, ErrorKind::MethodNotAllowed, "{path}");
    }
    for method in ["PUT", "PATCH", "OPTIONS", "delete"] {
        let bytes = format!("{method} /api/proxy/capture HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n");
        assert_eq!(parse_request(bytes.as_bytes()).unwrap_err().kind, ErrorKind::MethodNotAllowed);
    }
}

#[test]
fn capture_delete_does_not_override_origin_or_trust_method_headers() {
    for metadata in ["Origin: https://localhost\r\n", "Origin: http://other.example\r\n",
        "Sec-Fetch-Site: cross-site\r\n"] {
        let bytes = format!("DELETE /api/proxy/capture HTTP/1.1\r\nHost: localhost\r\n{metadata}Content-Length: 0\r\n\r\n");
        assert!(!parse_request(bytes.as_bytes()).unwrap().same_origin());
    }
    let request = parse_request(b"DELETE /api/proxy/capture HTTP/1.1\r\nHost: localhost\r\nX-HTTP-Method-Override: POST\r\n\r\n").unwrap();
    assert_eq!(request.method, Method::Delete);
    assert_eq!(request.content_length, 0);
    let request = parse_request(b"GET /api/proxy/capture HTTP/1.1\r\nHost: localhost\r\nX-HTTP-Method-Override: DELETE\r\n\r\n").unwrap();
    assert_eq!(request.method, Method::Get);
}

#[test]
fn capture_delete_header_limits_and_duplicate_metadata_stay_strict() {
    let prefix = "DELETE /api/proxy/capture HTTP/1.1\r\nHost: localhost\r\n";
    let exact = format!("{prefix}X-Pad: {}\r\n\r\n", "a".repeat(MAX_HEADER_BYTES - prefix.len() - "X-Pad: ".len() - 4));
    assert_eq!(exact.len(), MAX_HEADER_BYTES);
    assert!(parse_request(exact.as_bytes()).is_ok());
    let over = exact.replacen("X-Pad: ", "X-Pad: a", 1);
    assert_eq!(parse_request(over.as_bytes()).unwrap_err().kind, ErrorKind::HeadersTooLarge);
    let exact_count = format!("{prefix}{}\r\n", "X-A: b\r\n".repeat(63));
    assert!(parse_request(exact_count.as_bytes()).is_ok());
    let over_count = format!("{prefix}{}\r\n", "X-A: b\r\n".repeat(64));
    assert_eq!(parse_request(over_count.as_bytes()).unwrap_err().kind, ErrorKind::HeadersTooLarge);
    for headers in ["Host: localhost\r\n", "Origin: http://localhost\r\norigin: http://localhost\r\n",
        "Cookie: a=b\r\nCOOKIE: c=d\r\n", "Sec-Fetch-Site: same-origin\r\nsec-fetch-site: same-origin\r\n"] {
        let bytes = format!("{prefix}{headers}\r\n");
        assert_eq!(parse_request(bytes.as_bytes()).unwrap_err().kind, ErrorKind::BadRequest);
    }
}

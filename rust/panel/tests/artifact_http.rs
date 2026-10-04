//! Pure stream fixtures; no sockets, TLS, DNS, transport or activation.
use be6500_panel::artifact_http::{
    HttpSourceError, MAX_BODY_BYTES, MAX_HEAD_BYTES, STREAM_BYTES, Url, parse_response,
};
use be6500_panel::readiness_tun::Budget;
use std::{
    io::{self, Cursor, Read},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
fn budget(cancel: &AtomicBool) -> Budget<'_> {
    Budget {
        deadline: Instant::now() + Duration::from_secs(60),
        cancel,
    }
}
fn body_error(bytes: &[u8]) -> HttpSourceError {
    let cancel = AtomicBool::new(false);
    let mut body = parse_response(Cursor::new(bytes), &budget(&cancel))
        .unwrap()
        .into_body();
    let mut scratch = [0; STREAM_BYTES];
    loop {
        match body.read(&mut scratch) {
            Ok(0) => panic!("expected private body failure"),
            Ok(_) => {}
            Err(error) => {
                let actual = *error
                    .get_ref()
                    .unwrap()
                    .downcast_ref::<HttpSourceError>()
                    .unwrap();
                let again = body.read(&mut scratch).unwrap_err();
                assert_eq!(
                    again.get_ref().unwrap().downcast_ref::<HttpSourceError>(),
                    Some(&actual)
                );
                return actual;
            }
        }
    }
}
fn decoded(bytes: &[u8]) -> Vec<u8> {
    let cancel = AtomicBool::new(false);
    let mut body = parse_response(Cursor::new(bytes), &budget(&cancel))
        .unwrap()
        .into_body();
    let mut out = Vec::new();
    body.read_to_end(&mut out).unwrap();
    out
}

#[test]
fn typed_https_and_explicit_numeric_loopback_urls_are_private_and_bounded() {
    let url = Url::parse("https://Example.COM:8443/private?q=value", false).unwrap();
    assert_eq!(url.scheme(), "https");
    assert_eq!(url.host(), "example.com");
    assert_eq!(url.port(), 8443);
    assert_eq!(url.authority(), "Example.COM:8443");
    assert_eq!(url.path_and_query(), "/private?q=value");
    assert_eq!(format!("{url:?}"), "ArtifactUrl([private])");
    assert_eq!(
        Url::parse("https://example.com", false)
            .unwrap()
            .path_and_query(),
        "/"
    );
    for text in ["http://127.0.0.1:8790/file", "http://[::1]/file"] {
        assert!(Url::parse(text, true).is_ok());
        assert!(Url::parse(text, false).is_err());
    }
    assert!(Url::parse("https://[2001:db8::1]:443/file", false).is_ok());
    let exact = format!("https://example.com/{}", "a".repeat(4096 - 20));
    assert_eq!(exact.len(), 4096);
    assert!(Url::parse(&exact, false).is_ok());
    assert!(Url::parse(&(exact + "a"), false).is_err());
}

#[test]
fn credentials_fragments_controls_mapped_zones_and_bad_schemes_ports_refuse() {
    for text in [
        "https://user:secret@example.com/file",
        "https://example.com/file#",
        "https://example.com/#secret",
        "https://example.com/\r\nHost:evil",
        "https://example.com/%0d%0a",
        "https://example.com/a b",
        "https://[::ffff:127.0.0.1]/file",
        "https://[fe80::1%25en0]/file",
        "https://example.com:0/file",
        "https://example.com:65536/file",
        "https://example.com:x/file",
        "https://example.com:/file",
        "https://-bad.example/file",
        "https://example..com/file",
        "file:///private/source",
        "ftp://example.com/file",
        "/relative/file",
        "https:opaque",
        "http://localhost/file",
        "http://192.0.2.1/file",
        "https://example.com/\\private",
    ] {
        assert!(Url::parse(text, true).is_err(), "{text}");
    }
}

#[test]
fn redirects_resolve_relative_absolute_query_and_network_paths_then_revalidate() {
    let base = Url::parse("https://example.com/a/b/file?old=yes", true).unwrap();
    for (location, expected) in [
        ("../next?x=1", "/a/next?x=1"),
        ("./other", "/a/b/other"),
        ("/new/path", "/new/path"),
        ("?new=yes", "/a/b/file?new=yes"),
        ("../../../../last", "/last"),
        ("./", "/a/b/"),
    ] {
        let next = base.resolve_location(location).unwrap();
        assert_eq!(next.host(), "example.com");
        assert_eq!(next.path_and_query(), expected);
    }
    let next = base.resolve_location("//other.example:8443/new").unwrap();
    assert_eq!(next.host(), "other.example");
    assert_eq!(next.port(), 8443);
    assert!(base.resolve_location("http://127.0.0.1/file").is_err());
    for location in [
        "file:///private",
        "//user@other.example/file",
        "/new#fragment",
        "/%0a",
        "",
    ] {
        assert!(base.resolve_location(location).is_err());
    }
}

#[test]
fn head_prefetch_keeps_body_and_response_getters_do_not_expose_debug_data() {
    let cancel = AtomicBool::new(false);
    let bytes = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Encoding: identity\r\n\r\nhello";
    let mut response = parse_response(Cursor::new(bytes), &budget(&cancel)).unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.location(), None);
    assert_eq!(format!("{response:?}"), "ArtifactHttpResponse([private])");
    assert_eq!(
        format!("{:?}", response.body_mut()),
        "ArtifactHttpBody([private])"
    );
    let mut out = Vec::new();
    response.body_mut().read_to_end(&mut out).unwrap();
    assert_eq!(out, b"hello");
    let response = parse_response(
        &b"HTTP/1.0 302 Found\r\nLocation: ../next\r\nContent-Length: 0\r\n\r\n"[..],
        &budget(&cancel),
    )
    .unwrap();
    assert_eq!(response.status(), 302);
    assert_eq!(response.location(), Some("../next"));
    assert_eq!(decoded(b"HTTP/1.0 200 OK\r\n\r\nclose-body"), b"close-body");
}

#[test]
fn duplicate_or_conflicting_framing_and_transport_decompression_are_refused() {
    let cancel = AtomicBool::new(false);
    for headers in [
        "Content-Length: 1\r\nContent-Length: 1\r\n",
        "Content-Length: 1, 1\r\n",
        "Content-Length: -1\r\n",
        "Content-Length: +1\r\n",
        "Content-Length: 18446744073709551616\r\n",
        "Transfer-Encoding: chunked\r\nContent-Length: 1\r\n",
        "Transfer-Encoding: chunked\r\nTransfer-Encoding: chunked\r\n",
        "Transfer-Encoding: gzip, chunked\r\n",
        "Content-Encoding: gzip\r\n",
        "Content-Encoding: identity\r\nContent-Encoding: identity\r\n",
        "Location: /one\r\nLocation: /two\r\n",
    ] {
        let bytes = format!("HTTP/1.1 200 OK\r\n{headers}\r\n");
        assert!(
            parse_response(Cursor::new(bytes), &budget(&cancel)).is_err(),
            "{headers}"
        );
    }
}

#[test]
fn strict_header_crlf_count_and_exact_size_bounds_are_enforced() {
    let cancel = AtomicBool::new(false);
    for bytes in [
        b"HTTP/1.1 200 OK\nBad: x\n\r\n\r\n".as_slice(),
        b"HTTP/1.1 200 OK\r\n folded: x\r\n\r\n",
        b"HTTP/1.1 200 OK\r\nX: a\x00b\r\n\r\n",
    ] {
        assert!(parse_response(Cursor::new(bytes), &budget(&cancel)).is_err());
    }
    let prefix = "HTTP/1.1 200 OK\r\nX: ";
    let suffix = "\r\n\r\n";
    for count in [MAX_HEAD_BYTES, MAX_HEAD_BYTES + 1] {
        let bytes = format!(
            "{prefix}{}{suffix}",
            "a".repeat(count - prefix.len() - suffix.len())
        );
        assert_eq!(
            parse_response(Cursor::new(bytes), &budget(&cancel)).is_ok(),
            count == MAX_HEAD_BYTES
        );
    }
    for count in [64, 65] {
        let bytes = format!("HTTP/1.1 200 OK\r\n{}\r\n", "X: a\r\n".repeat(count));
        assert_eq!(
            parse_response(Cursor::new(bytes), &budget(&cancel)).is_ok(),
            count == 64
        );
    }
}

#[test]
fn chunk_extensions_and_trailers_are_validated_without_changing_body() {
    assert_eq!(decoded(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3;name=value;quoted=\"a;b\"\r\nabc\r\n2\r\nde\r\n0\r\nX-Safe: value\r\n\r\n"), b"abcde");
    for framing in [
        "x\r\n",
        "1;\r\na\r\n0\r\n\r\n",
        "1;name=\"unterminated\r\na\r\n0\r\n\r\n",
        "1\r\naXX0\r\n\r\n",
        "0\r\nContent-Length: 0\r\n\r\n",
        "0\r\n folded: invalid\r\n\r\n",
        "0\r\nNoColon\r\n\r\n",
        "0\r\n\r\ntrailing",
        "1\r\n",
        "1\r\na\r\n",
        "0\r\nX: truncated",
    ] {
        let bytes = format!("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{framing}");
        assert!(matches!(
            body_error(bytes.as_bytes()),
            HttpSourceError::Framing | HttpSourceError::Truncated
        ));
    }
}

#[test]
fn exact_lengths_refuse_truncation_and_extra_payload_including_after_empty_status() {
    assert_eq!(
        body_error(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nabc"),
        HttpSourceError::Truncated
    );
    assert_eq!(
        body_error(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nabcd"),
        HttpSourceError::Framing
    );
    assert_eq!(
        body_error(b"HTTP/1.1 204 No Content\r\n\r\nextra"),
        HttpSourceError::Framing
    );
    assert!(decoded(b"HTTP/1.1 204 No Content\r\n\r\n").is_empty());
}
struct Repeat(u64);
impl Read for Repeat {
    fn read(&mut self, into: &mut [u8]) -> io::Result<usize> {
        let n = into.len().min(self.0 as usize);
        into[..n].fill(7);
        self.0 -= n as u64;
        Ok(n)
    }
}
fn count_body(mut body: impl Read) -> Result<u64, HttpSourceError> {
    let mut out = [0; STREAM_BYTES];
    let mut count = 0;
    loop {
        match body.read(&mut out) {
            Ok(0) => return Ok(count),
            Ok(n) => count += n as u64,
            Err(error) => {
                return Err(*error
                    .get_ref()
                    .unwrap()
                    .downcast_ref::<HttpSourceError>()
                    .unwrap());
            }
        }
    }
}

#[test]
fn close_length_and_chunk_limits_use_generators_not_whole_body_allocations() {
    let cancel = AtomicBool::new(false);
    for length in [MAX_BODY_BYTES, MAX_BODY_BYTES + 1] {
        let stream = Cursor::new(b"HTTP/1.1 200 OK\r\n\r\n").chain(Repeat(length));
        let body = parse_response(stream, &budget(&cancel))
            .unwrap()
            .into_body();
        assert_eq!(
            count_body(body),
            if length == MAX_BODY_BYTES {
                Ok(length)
            } else {
                Err(HttpSourceError::Limit)
            }
        );
    }
    let prefix = format!("HTTP/1.1 200 OK\r\nContent-Length: {MAX_BODY_BYTES}\r\n\r\n");
    let stream = Cursor::new(prefix).chain(Repeat(MAX_BODY_BYTES));
    assert_eq!(
        count_body(
            parse_response(stream, &budget(&cancel))
                .unwrap()
                .into_body()
        ),
        Ok(MAX_BODY_BYTES)
    );
    let chunk =
        format!("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{MAX_BODY_BYTES:x}\r\n");
    let stream = Cursor::new(chunk)
        .chain(Repeat(MAX_BODY_BYTES))
        .chain(Cursor::new(b"\r\n0\r\n\r\n"));
    assert_eq!(
        count_body(
            parse_response(stream, &budget(&cancel))
                .unwrap()
                .into_body()
        ),
        Ok(MAX_BODY_BYTES)
    );
    let too_big = format!(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n",
        MAX_BODY_BYTES + 1
    );
    assert_eq!(body_error(too_big.as_bytes()), HttpSourceError::Limit);
}

#[test]
fn chunk_line_count_and_trailer_caps_refuse_without_unbounded_retention() {
    let huge = format!(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n1;x={}\r\n",
        "a".repeat(1024)
    );
    assert_eq!(body_error(huge.as_bytes()), HttpSourceError::Limit);
    let many = format!(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{}0\r\n\r\n",
        "1\r\na\r\n".repeat(65537)
    );
    assert_eq!(body_error(many.as_bytes()), HttpSourceError::Limit);
    let trailers = format!(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n{}\r\n",
        "X: a\r\n".repeat(65)
    );
    assert_eq!(body_error(trailers.as_bytes()), HttpSourceError::Limit);
}

#[test]
fn callback_budget_and_source_failure_are_private_and_latched() {
    struct Cancel<'a>(&'a AtomicBool);
    impl Read for Cancel<'_> {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            self.0.store(true, Ordering::Relaxed);
            out[0] = 1;
            Ok(1)
        }
    }
    struct Fail;
    impl Read for Fail {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("private-url"))
        }
    }
    let cancel = AtomicBool::new(false);
    let expired = Budget {
        deadline: Instant::now(),
        cancel: &cancel,
    };
    assert_eq!(
        parse_response(Cursor::new(b""), &expired).err(),
        Some(HttpSourceError::Deadline)
    );
    assert_eq!(
        parse_response(Cancel(&cancel), &budget(&cancel)).err(),
        Some(HttpSourceError::Cancelled)
    );
    cancel.store(false, Ordering::Relaxed);
    assert_eq!(
        parse_response(Fail, &budget(&cancel)).err(),
        Some(HttpSourceError::Source)
    );
    let mut body = parse_response(
        Cursor::new(b"HTTP/1.1 200 OK\r\n\r\nbody"),
        &budget(&cancel),
    )
    .unwrap()
    .into_body();
    cancel.store(true, Ordering::Relaxed);
    let error = body.read(&mut [0; 8]).unwrap_err();
    assert_eq!(
        error.get_ref().unwrap().downcast_ref::<HttpSourceError>(),
        Some(&HttpSourceError::Cancelled)
    );
    for error in [
        HttpSourceError::Url,
        HttpSourceError::Source,
        HttpSourceError::Head,
        HttpSourceError::Framing,
        HttpSourceError::Limit,
        HttpSourceError::Deadline,
    ] {
        assert!(!format!("{error:?} {error}").contains("private-url"));
    }
}

#[test]
fn chunk_and_header_parsing_work_across_every_byte_boundary() {
    struct One(Cursor<Vec<u8>>);
    impl Read for One {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            if out.is_empty() {
                Ok(0)
            } else {
                self.0.read(&mut out[..1])
            }
        }
    }
    let cancel = AtomicBool::new(false);
    let bytes =
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nab\r\n1\r\nc\r\n0\r\n\r\n";
    let mut body = parse_response(One(Cursor::new(bytes.to_vec())), &budget(&cancel))
        .unwrap()
        .into_body();
    let mut out = Vec::new();
    body.read_to_end(&mut out).unwrap();
    assert_eq!(out, b"abc");
}

#[test]
fn oversized_declared_length_rejects_before_body_and_trailers_have_total_byte_cap() {
    let cancel = AtomicBool::new(false);
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
        MAX_BODY_BYTES + 1
    );
    assert_eq!(
        parse_response(Cursor::new(head), &budget(&cancel)).err(),
        Some(HttpSourceError::Limit)
    );
    let trailers = format!(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\nX: {}\r\n\r\n",
        "a".repeat(MAX_HEAD_BYTES)
    );
    assert_eq!(body_error(trailers.as_bytes()), HttpSourceError::Limit);
}

#[test]
fn known_length_and_chunk_completion_do_not_wait_for_lingering_peer_close() {
    struct Lingering {
        bytes: Cursor<Vec<u8>>,
    }
    impl Read for Lingering {
        fn read(&mut self, into: &mut [u8]) -> io::Result<usize> {
            assert!(
                self.bytes.position() < self.bytes.get_ref().len() as u64,
                "explicit frame completion must not read peer EOF"
            );
            self.bytes.read(into)
        }
    }
    let cancel = AtomicBool::new(false);
    for bytes in [
        b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nabc".as_slice(),
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n0\r\n\r\n",
        b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n",
    ] {
        let reader = Lingering {
            bytes: Cursor::new(bytes.to_vec()),
        };
        let mut body = parse_response(reader, &budget(&cancel))
            .unwrap()
            .into_body();
        let mut out = Vec::new();
        body.read_to_end(&mut out).unwrap();
        assert!(out == b"abc" || out.is_empty());
        assert_eq!(body.read(&mut [0; 8]).unwrap(), 0);
    }
}

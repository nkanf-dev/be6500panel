#![cfg(unix)]
//! Real host PTYs. Production shell is /bin/ash; this seam uses /bin/sh.
//! These are test definitions, not claims of Linux/device execution.
use be6500_panel::{
    http::{self, Method},
    product_terminal::{INPUT_BYTES, OUTPUT_BYTES, Terminal, decode_bytes, encode_bytes},
    readiness_tun::Budget,
};
use serde_json::{Value, json};
use std::{
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
struct Pty {
    terminal: Terminal,
    id: String,
    cursor: u64,
    seen: Vec<u8>,
}
fn request(
    t: &mut Terminal,
    path: &str,
    method: Method,
    query: &str,
    body: Value,
) -> Result<Value, be6500_panel::product_gateway::ApiError> {
    let cancel = AtomicBool::new(false);
    let budget = Budget {
        deadline: Instant::now() + Duration::from_secs(2),
        cancel: &cancel,
    };
    let bytes = serde_json::to_vec(&body).unwrap();
    t.handle(path, method, query, &bytes, &budget)
}
fn pause() {
    unsafe {
        libc::poll(std::ptr::null_mut(), 0, 10);
    }
}
impl Pty {
    fn open() -> Self {
        let mut terminal = Terminal::host_test();
        let value = request(
            &mut terminal,
            "/api/terminal/open",
            Method::Post,
            "",
            json!({}),
        )
        .unwrap();
        assert_eq!(value["term"], "xterm-256color");
        assert_eq!(value["rows"], 24);
        assert_eq!(value["cols"], 80);
        assert_eq!(value["id"].as_str().unwrap().len(), 64);
        let mut p = Self {
            terminal,
            id: value["id"].as_str().unwrap().into(),
            cursor: 0,
            seen: Vec::new(),
        };
        p.until("# ");
        p
    }
    fn input(&mut self, raw: &[u8]) {
        let v = request(
            &mut self.terminal,
            "/api/terminal/input",
            Method::Post,
            "",
            json!({"id":self.id,"data":encode_bytes(raw)}),
        )
        .unwrap();
        assert_eq!(v["accepted"], raw.len());
    }
    fn output(&mut self) -> Value {
        self.terminal.tick();
        let v = request(
            &mut self.terminal,
            "/api/terminal/output",
            Method::Get,
            &format!("id={}&offset={}", self.id, self.cursor),
            json!(null),
        )
        .unwrap();
        // Output base64 may contain up to 16KiB; split into input-sized chunks
        // for this independent strict decoder helper.
        let encoded = v["data"].as_str().unwrap();
        for chunk in encoded.as_bytes().chunks(4096) {
            self.seen
                .extend(decode_bytes(std::str::from_utf8(chunk).unwrap()).unwrap());
        }
        self.cursor = v["nextOffset"].as_u64().unwrap();
        v
    }
    fn until(&mut self, expected: &str) {
        let end = Instant::now() + Duration::from_secs(4);
        loop {
            self.output();
            if String::from_utf8_lossy(&self.seen).contains(expected) {
                return;
            }
            assert!(
                Instant::now() < end,
                "missing {expected:?}: {}",
                String::from_utf8_lossy(&self.seen)
            );
            pause();
        }
    }
    fn clear(&mut self) {
        self.output();
        self.seen.clear();
    }
    fn close(&mut self) {
        request(
            &mut self.terminal,
            "/api/terminal/close",
            Method::Delete,
            &format!("id={}", self.id),
            json!(null),
        )
        .unwrap();
        let until = Instant::now() + Duration::from_secs(3);
        while !self.terminal.close() {
            assert!(Instant::now() < until);
            pause();
        }
    }
}
impl Drop for Pty {
    fn drop(&mut self) {
        let until = Instant::now() + Duration::from_secs(2);
        while !self.terminal.close() && Instant::now() < until {
            pause();
        }
    }
}

#[test]
fn real_pty_term_canonical_mode_resize_and_binary_output() {
    let mut p = Pty::open();
    p.input(b"printf 'TERM=<%s>\\n' \"$TERM\"; tty; stty size; stty -a\r");
    p.until("TERM=<xterm-256color>");
    p.until("24 80");
    p.until("icanon");
    p.until("isig");
    assert!(!String::from_utf8_lossy(&p.seen).contains("not a tty"));
    let v = request(
        &mut p.terminal,
        "/api/terminal/resize",
        Method::Post,
        "",
        json!({"id":p.id,"rows":41,"cols":137}),
    )
    .unwrap();
    assert_eq!(v, json!({"rows":41,"cols":137}));
    p.clear();
    p.input(b"stty size; printf '\\033[38;5;196mRED\\033[0m\\n'\r");
    p.until("41 137");
    p.until("\u{1b}[38;5;196mRED\u{1b}[0m");
    p.close();
}
#[test]
fn raw_ctrl_c_interrupts_foreground_job_and_returns_interactive_prompt() {
    let mut p = Pty::open();
    p.clear();
    p.input(b"sleep 30\r");
    // Wait for echo before giving the shell time to transfer foreground pgrp.
    p.until("sleep 30");
    for _ in 0..10 {
        p.output();
        pause();
    }
    p.clear();
    p.input(b"\x03");
    p.until("# ");
    p.clear();
    p.input(b"printf 'ALIVE=%s\\n' yes\r");
    p.until("ALIVE=yes");
    p.close();
}
#[test]
fn raw_ctrl_z_stops_job_jobs_fg_resume_then_ctrl_c() {
    let mut p = Pty::open();
    p.clear();
    p.input(b"sleep 30\r");
    p.until("sleep 30");
    for _ in 0..10 {
        p.output();
        pause();
    }
    p.clear();
    p.input(b"\x1a");
    p.until("# ");
    p.clear();
    p.input(b"jobs\r");
    p.until("sleep 30");
    let seen = String::from_utf8_lossy(&p.seen).to_ascii_lowercase();
    assert!(
        seen.contains("stopped") || seen.contains("suspended"),
        "{seen}"
    );
    p.clear();
    p.input(b"fg\r");
    for _ in 0..10 {
        p.output();
        pause();
    }
    p.clear();
    p.input(b"\x03");
    p.until("# ");
    p.close();
}
#[test]
fn raw_ctrl_d_exits_shell_and_is_reaped() {
    let mut p = Pty::open();
    p.input(b"\x04");
    let end = Instant::now() + Duration::from_secs(3);
    loop {
        let v = p.output();
        if v["state"] == "exited" {
            assert_eq!(v["exitCode"], 0);
            break;
        }
        assert!(Instant::now() < end);
        pause();
    }
    let e = request(
        &mut p.terminal,
        "/api/terminal/input",
        Method::Post,
        "",
        json!({"id":p.id,"data":"Aw=="}),
    )
    .unwrap_err();
    assert_eq!(e.code, "terminal_exited");
    p.close();
}
#[cfg(target_os = "linux")]
#[test]
fn linux_close_reaps_owned_shell_and_kills_background_and_foreground_jobs() {
    let mut p = Pty::open();
    p.clear();
    p.input(b"sleep 30 & bg=$!; printf 'OWNED:%s:%s\\n' \"$$\" \"$bg\"; sleep 30\r");
    p.until("OWNED:");
    let end = Instant::now() + Duration::from_secs(3);
    let (shell, background) = loop {
        p.output();
        let text = String::from_utf8_lossy(&p.seen);
        let parsed = text.lines().find_map(|line| {
            let rest = line.strip_prefix("OWNED:")?;
            let (a, b) = rest.trim().split_once(':')?;
            Some((a.parse::<i32>().ok()?, b.parse::<i32>().ok()?))
        });
        if let Some(ids) = parsed {
            break ids;
        }
        assert!(Instant::now() < end);
        pause();
    };
    let own_pgrp = unsafe { libc::getpgrp() };
    assert_ne!(shell, own_pgrp);
    p.close();
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        if unsafe { libc::kill(shell, 0) } < 0 && unsafe { libc::kill(background, 0) } < 0 {
            break;
        }
        assert!(Instant::now() < until, "terminal process survived close");
        pause();
    }
    assert_eq!(unsafe { libc::getpgrp() }, own_pgrp);
    let mut status = 0;
    assert_eq!(
        unsafe { libc::waitpid(shell, &mut status, libc::WNOHANG) },
        -1
    );
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ECHILD)
    );
}
#[test]
fn explicit_open_only_single_session_validation_and_stale_ids() {
    let mut t = Terminal::host_test();
    assert_eq!(
        request(&mut t, "/api/terminal/open", Method::Get, "", json!({}))
            .unwrap_err()
            .status,
        405
    );
    assert_eq!(
        request(
            &mut t,
            "/api/terminal/output",
            Method::Get,
            &format!("id={}&offset=0", "0".repeat(64)),
            json!(null)
        )
        .unwrap_err()
        .status,
        404
    );
    assert_eq!(
        request(
            &mut t,
            "/api/terminal/open",
            Method::Post,
            "",
            json!({"rows":0})
        )
        .unwrap_err()
        .status,
        400
    );
    assert_eq!(
        request(
            &mut t,
            "/api/terminal/open",
            Method::Post,
            "",
            json!({"shell":"/bin/sh"})
        )
        .unwrap_err()
        .status,
        400
    );
    let mut p = Pty::open();
    assert_eq!(
        request(
            &mut p.terminal,
            "/api/terminal/open",
            Method::Post,
            "",
            json!({})
        )
        .unwrap_err()
        .code,
        "terminal_busy"
    );
    assert_eq!(
        request(
            &mut p.terminal,
            "/api/terminal/resize",
            Method::Post,
            "",
            json!({"id":p.id,"rows":513,"cols":80})
        )
        .unwrap_err()
        .status,
        400
    );
    assert_eq!(
        request(
            &mut p.terminal,
            "/api/terminal/input",
            Method::Post,
            "",
            json!({"id":p.id,"data":"%%%="})
        )
        .unwrap_err()
        .status,
        400
    );
    assert_eq!(
        request(
            &mut p.terminal,
            "/api/terminal/output",
            Method::Get,
            &format!("id={}&offset=18446744073709551615", p.id),
            json!(null)
        )
        .unwrap_err()
        .status,
        400
    );
    p.close();
    assert_eq!(
        request(
            &mut p.terminal,
            "/api/terminal/output",
            Method::Get,
            &format!("id={}&offset=0", p.id),
            json!(null)
        )
        .unwrap_err()
        .status,
        404
    );
}
#[test]
fn base64_preserves_all_bytes_and_ctrl_keys_with_strict_size_bounds() {
    let raw: Vec<u8> = (0..=255).cycle().take(INPUT_BYTES).collect();
    assert_eq!(decode_bytes(&encode_bytes(&raw)).unwrap(), raw);
    assert_eq!(decode_bytes("AwQa").unwrap(), b"\x03\x04\x1a");
    assert!(decode_bytes(&encode_bytes(&vec![0; INPUT_BYTES + 1])).is_err());
    for s in ["A===", "AB==", "ABC=", "AA", "AA==AAAA", "AA==\n"] {
        assert!(decode_bytes(s).is_err(), "{s}");
    }
}
#[test]
fn output_ring_is_bounded_and_reports_cursor_loss() {
    let mut p = Pty::open();
    p.input(b"i=0; while [ $i -lt 5000 ]; do printf '0123456789abcdef'; i=$((i+1)); done; printf '\\nDONE\\n'\r");
    p.until("DONE\r\n");
    let v = request(
        &mut p.terminal,
        "/api/terminal/output",
        Method::Get,
        &format!("id={}&offset=0", p.id),
        json!(null),
    )
    .unwrap();
    assert_eq!(v["truncated"], true);
    assert!(v["offset"].as_u64().unwrap() > 0);
    assert!(v["nextOffset"].as_u64().unwrap() - v["offset"].as_u64().unwrap() <= 16 * 1024);
    assert!(p.cursor - v["offset"].as_u64().unwrap() <= OUTPUT_BYTES as u64);
    p.close();
}
#[test]
fn fixed_http_methods_and_endpoint_body_bound_use_existing_origin_guard() {
    for (path, method, body) in [
        ("open", "POST", "{}"),
        ("input", "POST", "{}"),
        ("resize", "POST", "{}"),
        ("close", "POST", "{}"),
        ("close", "DELETE", ""),
    ] {
        let target = format!("/api/terminal/{path}");
        let raw = format!(
            "{method} {target} HTTP/1.1\r\nHost: panel.local\r\nOrigin: http://evil.local\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        let r = http::parse_request(raw.as_bytes()).unwrap();
        assert!(!r.same_origin());
    }
    let raw =
        b"POST /api/terminal/input HTTP/1.1\r\nHost: localhost\r\nContent-Length: 12289\r\n\r\n";
    assert_eq!(
        http::parse_request(raw).unwrap_err().kind,
        http::ErrorKind::BodyTooLarge
    );
}
#[test]
fn authenticated_http_boundary_never_opens_host_shell_or_accepts_cross_origin() {
    use be6500_panel::{auth::Auth, server::Service};
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        thread,
    };
    fn exchange(service: Service, origin: &'static str) -> Vec<u8> {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let caller = thread::spawn(move || {
            let mut stream = TcpStream::connect(address).unwrap();
            stream.write_all(format!("POST /api/terminal/open HTTP/1.1\r\nHost: localhost\r\nOrigin: {origin}\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{{}}",origin=origin).as_bytes()).unwrap();
            let mut out = Vec::new();
            stream.read_to_end(&mut out).unwrap();
            out
        });
        let (stream, _) = listener.accept().unwrap();
        service.handle(stream).unwrap();
        caller.join().unwrap()
    }
    // Host no-auth mode cannot become a root shell; authenticated native is required.
    let response = exchange(Service::new("/proc".into()), "http://localhost");
    assert!(response.starts_with(b"HTTP/1.1 503 "));
    let response = exchange(
        Service::new("/proc".into()).with_auth(Auth::new("secret")),
        "http://localhost",
    );
    assert!(response.starts_with(b"HTTP/1.1 401 "));
    let response = exchange(
        Service::new("/proc".into()).with_auth(Auth::new("secret")),
        "http://evil.local",
    );
    assert!(response.starts_with(b"HTTP/1.1 403 "));
}

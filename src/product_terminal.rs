//! One explicit root rescue PTY on the existing single owner lane.
//! No worker, listener, command whitelist, GET spawn, blocking IO or process adoption.
//! Linux cleanup enumerates only the owned shell session (not global processes).
use crate::{http::Method, product_gateway::ApiError, readiness_tun::Budget};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    ffi::CString,
    io,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    time::{Duration, Instant},
};

pub const OUTPUT_BYTES: usize = 64 << 10;
pub const INPUT_BYTES: usize = 8 << 10;
pub const OUTPUT_CHUNK: usize = 16 << 10;
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(300);
const IO_SLICE: Duration = Duration::from_millis(3);
const BODY_BYTES: usize = 12 << 10;
pub fn is_terminal_path(path: &str) -> bool {
    matches!(
        path,
        "/api/terminal/open"
            | "/api/terminal/output"
            | "/api/terminal/input"
            | "/api/terminal/resize"
            | "/api/terminal/close"
    )
}
fn error(status: u16, code: &'static str, message: &'static str) -> ApiError {
    ApiError {
        status,
        code,
        message,
    }
}
fn invalid() -> ApiError {
    error(
        400,
        "invalid_terminal_input",
        "Terminal fields or values are invalid.",
    )
}
fn unavailable() -> ApiError {
    error(503, "terminal_unavailable", "Terminal could not be opened.")
}
fn decode<T: serde::de::DeserializeOwned>(raw: &[u8]) -> Result<T, ApiError> {
    if raw.len() > BODY_BYTES {
        return Err(invalid());
    }
    serde_json::from_slice(raw).map_err(|_| invalid())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Open {
    #[serde(default = "rows")]
    rows: u16,
    #[serde(default = "cols")]
    cols: u16,
}
fn rows() -> u16 {
    24
}
fn cols() -> u16 {
    80
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    id: String,
    data: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Resize {
    id: String,
    rows: u16,
    cols: u16,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Close {
    id: String,
}
fn dimensions(rows: u16, cols: u16) -> Result<libc::winsize, ApiError> {
    if !(1..=512).contains(&rows) || !(1..=512).contains(&cols) {
        return Err(invalid());
    }
    Ok(libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    })
}
fn query(raw: &str, output: bool) -> Result<(String, u64), ApiError> {
    let mut id = None;
    let mut offset = None;
    for field in raw.split('&') {
        let (key, value) = field.split_once('=').ok_or_else(invalid)?;
        match key {
            "id" if id.is_none()
                && value.len() == 64
                && value.bytes().all(|b| b.is_ascii_hexdigit()) =>
            {
                id = Some(value.to_owned())
            }
            "offset"
                if output
                    && offset.is_none()
                    && !value.is_empty()
                    && value.bytes().all(|b| b.is_ascii_digit()) =>
            {
                offset = Some(value.parse::<u64>().map_err(|_| invalid())?)
            }
            _ => return Err(invalid()),
        }
    }
    Ok((
        id.ok_or_else(invalid)?,
        if output {
            offset.ok_or_else(invalid)?
        } else {
            0
        },
    ))
}
/// Standard padded base64; the raw byte stream is never UTF-8 decoded here.
pub fn encode_bytes(raw: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(raw.len().div_ceil(3) * 4);
    for chunk in raw.chunks(3) {
        let n = ((chunk[0] as u32) << 16)
            | ((chunk.get(1).copied().unwrap_or(0) as u32) << 8)
            | chunk.get(2).copied().unwrap_or(0) as u32;
        out.push(TABLE[(n >> 18) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}
pub fn decode_bytes(raw: &str) -> Result<Vec<u8>, ApiError> {
    if !raw.len().is_multiple_of(4) || raw.len() > INPUT_BYTES.div_ceil(3) * 4 {
        return Err(invalid());
    }
    let mut out = Vec::with_capacity(raw.len() / 4 * 3);
    fn digit(b: u8) -> Result<u32, ApiError> {
        Ok(match b {
            b'A'..=b'Z' => (b - b'A') as u32,
            b'a'..=b'z' => (b - b'a' + 26) as u32,
            b'0'..=b'9' => (b - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            _ => return Err(invalid()),
        })
    }
    for (i, c) in raw.as_bytes().chunks_exact(4).enumerate() {
        let a = digit(c[0])?;
        let b = digit(c[1])?;
        let end = (i + 1) * 4 == raw.len();
        if c[2] == b'=' {
            if !end || c[3] != b'=' || b & 15 != 0 {
                return Err(invalid());
            }
            out.push(((a << 2) | (b >> 4)) as u8);
        } else {
            let d = digit(c[2])?;
            out.push(((a << 2) | (b >> 4)) as u8);
            out.push(((b << 4) | (d >> 2)) as u8);
            if c[3] == b'=' {
                if !end || d & 3 != 0 {
                    return Err(invalid());
                }
            } else {
                out.push(((d << 6) | digit(c[3])?) as u8);
            }
        }
    }
    if out.len() > INPUT_BYTES {
        return Err(invalid());
    }
    Ok(out)
}
struct Session {
    id: String,
    master: OwnedFd,
    pid: libc::pid_t,
    reaped: bool,
    eof: bool,
    output: VecDeque<u8>,
    base: u64,
    input: VecDeque<u8>,
    touched: Instant,
    closing: Option<Instant>,
    close_exit_sent: bool,
    exit_code: Option<i32>,
    rows: u16,
    cols: u16,
}
impl Session {
    fn state(&self) -> &'static str {
        if self.closing.is_some() {
            "closing"
        } else if self.reaped {
            "exited"
        } else {
            "open"
        }
    }
    fn collect(&mut self, until: Instant) {
        let fd = self.master.as_raw_fd();
        let mut raw = [0u8; 4096];
        let mut total = 0;
        while !self.eof && total < OUTPUT_CHUNK && Instant::now() < until {
            let n = unsafe { libc::read(fd, raw.as_mut_ptr().cast(), raw.len()) };
            if n > 0 {
                for b in &raw[..n as usize] {
                    if self.output.len() == OUTPUT_BYTES {
                        self.output.pop_front();
                        self.base += 1;
                    }
                    self.output.push_back(*b);
                }
                total += n as usize;
            } else {
                if n == 0 || (n < 0 && io::Error::last_os_error().raw_os_error() == Some(libc::EIO))
                {
                    self.eof = true;
                }
                break;
            }
        }
        if !self.reaped && !self.input.is_empty() && Instant::now() < until {
            let (a, b) = self.input.as_slices();
            let chunk = if a.is_empty() { b } else { a };
            let n = unsafe { libc::write(fd, chunk.as_ptr().cast(), chunk.len()) };
            if n > 0 {
                self.input.drain(..n as usize);
            }
        }
        // Observe exit without reaping first: the PID remains reserved until all
        // owned-session cleanup signals have been issued (no PID reuse race).
        if !self.reaped {
            let mut info = unsafe { std::mem::zeroed::<libc::siginfo_t>() };
            let ok = unsafe {
                libc::waitid(
                    libc::P_PID,
                    self.pid as libc::id_t,
                    &mut info,
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            };
            if ok == 0 && unsafe { info.si_pid() } == self.pid {
                self.kill_jobs();
                let mut status = 0;
                if unsafe { libc::waitpid(self.pid, &mut status, libc::WNOHANG) } == self.pid {
                    self.reaped = true;
                    self.exit_code = Some(if libc::WIFEXITED(status) {
                        libc::WEXITSTATUS(status)
                    } else {
                        128 + libc::WTERMSIG(status)
                    });
                }
            }
        }
    }
    fn kill_jobs(&self) {
        if self.reaped {
            return;
        }
        // PTY foreground group handles job-control shells on both supported OSes.
        let group = unsafe { libc::tcgetpgrp(self.master.as_raw_fd()) };
        if group > 1 && group != self.pid && unsafe { libc::getsid(group) } == self.pid {
            unsafe {
                libc::kill(-group, libc::SIGKILL);
            }
        }
        #[cfg(target_os = "macos")]
        {
            let mut pids = [0i32; 8192];
            let n = unsafe {
                libc::proc_listallpids(
                    pids.as_mut_ptr().cast(),
                    std::mem::size_of_val(&pids) as i32,
                )
            };
            for &pid in pids.iter().take(n.max(0) as usize) {
                if pid > 1 && pid != self.pid && unsafe { libc::getsid(pid) } == self.pid {
                    unsafe {
                        libc::kill(pid, libc::SIGKILL);
                    }
                }
            }
        }
        #[cfg(target_os = "linux")]
        {
            // Interactive shells put background jobs in different process groups.
            // Only this owned session is signalled; cores have a different SID.
            if let Ok(entries) = std::fs::read_dir("/proc") {
                for entry in entries.take(8192).flatten() {
                    let Some(pid) = entry
                        .file_name()
                        .to_str()
                        .and_then(|n| n.parse::<i32>().ok())
                    else {
                        continue;
                    };
                    if pid <= 1 || pid == self.pid {
                        continue;
                    }
                    if unsafe { libc::getsid(pid) } == self.pid {
                        unsafe {
                            libc::kill(pid, libc::SIGKILL);
                        }
                    }
                }
            }
        }
    }
    fn begin_close(&mut self, now: Instant) {
        if self.closing.is_some() {
            return;
        }
        self.closing = Some(now);
        self.input.clear();
        if !self.reaped {
            self.kill_jobs();
            // Let the shell reap foreground/background jobs and exit normally.
            // Fallback is bounded and caller-driven; no waiting loop in an API.
            self.input.extend(b"\x03\n");
            unsafe {
                libc::kill(self.pid, libc::SIGCONT);
            }
        }
    }
    fn tick(&mut self, now: Instant) {
        if now.saturating_duration_since(self.touched) >= IDLE_TIMEOUT {
            self.begin_close(now);
        }
        if let Some(started) = self.closing {
            // Allow SIGCHLD processing before asking the shell to exit.
            if !self.reaped
                && !self.close_exit_sent
                && now.saturating_duration_since(started) >= Duration::from_millis(100)
            {
                self.input.extend(b"exit\n");
                self.close_exit_sent = true;
            }
            if !self.reaped && now.saturating_duration_since(started) >= Duration::from_millis(250)
            {
                self.kill_jobs();
                unsafe {
                    libc::kill(-self.pid, libc::SIGKILL);
                    libc::kill(self.pid, libc::SIGKILL);
                }
            }
        }
        self.collect(Instant::now() + IO_SLICE);
    }
}
/// Production calls `new`; fixed host shell seam is for real PTY integration tests.
pub struct Terminal {
    session: Option<Session>,
    shell: &'static str,
}
impl Default for Terminal {
    fn default() -> Self {
        Self::new()
    }
}
impl Terminal {
    pub fn new() -> Self {
        Self {
            session: None,
            shell: "/bin/ash",
        }
    }
    #[doc(hidden)]
    pub fn host_test() -> Self {
        Self {
            session: None,
            shell: "/bin/sh",
        }
    }
    pub fn tick(&mut self) {
        if let Some(s) = &mut self.session {
            s.tick(Instant::now());
            if s.closing.is_some() && s.reaped {
                self.session = None;
            }
        }
    }
    pub fn close(&mut self) -> bool {
        if let Some(s) = &mut self.session {
            s.begin_close(Instant::now());
        }
        self.tick();
        self.session.is_none()
    }
    fn session(&mut self, id: &str) -> Result<&mut Session, ApiError> {
        let s = self
            .session
            .as_mut()
            .filter(|s| s.id == id)
            .ok_or_else(|| {
                error(
                    404,
                    "terminal_not_found",
                    "Terminal session is no longer available.",
                )
            })?;
        s.touched = Instant::now();
        Ok(s)
    }
    pub fn handle(
        &mut self,
        path: &str,
        method: Method,
        raw_query: &str,
        body: &[u8],
        budget: &Budget<'_>,
    ) -> Result<Value, ApiError> {
        budget
            .check()
            .map_err(|_| error(504, "terminal_timeout", "Terminal request expired."))?;
        match (path, method) {
            ("/api/terminal/open", Method::Post) => {
                if !raw_query.is_empty() {
                    return Err(invalid());
                }
                let open: Open = decode(body)?;
                let size = dimensions(open.rows, open.cols)?;
                if self.session.is_some() {
                    return Err(error(
                        409,
                        "terminal_busy",
                        "Close the current terminal before opening another.",
                    ));
                }
                let s = spawn(self.shell, size)?;
                let result = json!({"id":s.id,"rows":s.rows,"cols":s.cols,"term":"xterm-256color","offset":0,"idleTimeoutSeconds":300});
                self.session = Some(s);
                Ok(result)
            }
            ("/api/terminal/output", Method::Get | Method::Head) => {
                let (id, requested) = query(raw_query, true)?;
                let s = self.session(&id)?;
                // GET only drains already-open PTY; it never creates a shell.
                s.collect((Instant::now() + IO_SLICE).min(budget.deadline));
                let end = s.base + s.output.len() as u64;
                if requested > end {
                    return Err(invalid());
                }
                let offset = requested.max(s.base);
                let raw: Vec<u8> = s
                    .output
                    .iter()
                    .skip((offset - s.base) as usize)
                    .take(OUTPUT_CHUNK)
                    .copied()
                    .collect();
                Ok(
                    json!({"id":s.id,"data":encode_bytes(&raw),"offset":offset,"nextOffset":offset+raw.len() as u64,"truncated":requested<s.base,"state":s.state(),"exitCode":s.exit_code}),
                )
            }
            ("/api/terminal/input", Method::Post) => {
                if !raw_query.is_empty() {
                    return Err(invalid());
                }
                let input: Input = decode(body)?;
                let raw = decode_bytes(&input.data)?;
                let s = self.session(&input.id)?;
                if s.reaped || s.closing.is_some() {
                    return Err(error(
                        409,
                        "terminal_exited",
                        "Terminal is not accepting input.",
                    ));
                }
                if s.input.len() + raw.len() > INPUT_BYTES {
                    return Err(error(
                        429,
                        "terminal_input_full",
                        "Terminal input queue is full; retry this input.",
                    ));
                }
                let accepted = raw.len();
                s.input.extend(raw);
                s.collect((Instant::now() + IO_SLICE).min(budget.deadline));
                Ok(json!({"accepted":accepted}))
            }
            ("/api/terminal/resize", Method::Post) => {
                if !raw_query.is_empty() {
                    return Err(invalid());
                }
                let input: Resize = decode(body)?;
                let size = dimensions(input.rows, input.cols)?;
                let s = self.session(&input.id)?;
                if s.reaped || s.closing.is_some() {
                    return Err(error(
                        409,
                        "terminal_exited",
                        "Terminal is not accepting resize.",
                    ));
                }
                if unsafe { libc::ioctl(s.master.as_raw_fd(), libc::TIOCSWINSZ, &size) } < 0 {
                    return Err(unavailable());
                }
                // TIOCSWINSZ sends SIGWINCH to the real foreground process group.
                s.rows = input.rows;
                s.cols = input.cols;
                Ok(json!({"rows":s.rows,"cols":s.cols}))
            }
            ("/api/terminal/close", Method::Delete | Method::Post) => {
                let id = if method == Method::Delete {
                    query(raw_query, false)?.0
                } else {
                    if !raw_query.is_empty() {
                        return Err(invalid());
                    }
                    decode::<Close>(body)?.id
                };
                self.session(&id)?.begin_close(Instant::now());
                self.tick();
                Ok(json!({"state":if self.session.is_none() { "closed" } else { "closing" }}))
            }
            _ => Err(error(
                405,
                "method_not_allowed",
                "Method is not allowed for this terminal endpoint.",
            )),
        }
    }
}
impl Drop for Terminal {
    fn drop(&mut self) {
        if let Some(s) = &mut self.session
            && !s.reaped
        {
            s.kill_jobs();
            unsafe {
                libc::kill(-s.pid, libc::SIGKILL);
                libc::kill(s.pid, libc::SIGKILL);
            }
            // Normal product_close retains the handle until WNOHANG succeeds.
            // Drop is emergency-only; it must never block the owner lane.
            unsafe {
                libc::waitpid(s.pid, std::ptr::null_mut(), libc::WNOHANG);
            }
        }
    }
}
fn spawn(shell: &str, mut size: libc::winsize) -> Result<Session, ApiError> {
    let mut entropy = [0u8; 32];
    getrandom::fill(&mut entropy).map_err(|_| unavailable())?;
    let id: String = entropy.iter().map(|b| format!("{b:02x}")).collect();
    // All allocations and environment preparation happen before fork.
    let path = CString::new(shell).map_err(|_| unavailable())?;
    let interactive = c"-i";
    let argv = [path.as_ptr(), interactive.as_ptr(), std::ptr::null()];
    let environment = [
        "TERM=xterm-256color".to_owned(),
        "PATH=/usr/sbin:/usr/bin:/sbin:/bin".to_owned(),
        "HOME=/root".to_owned(),
        "USER=root".to_owned(),
        format!("SHELL={shell}"),
        "PS1=# ".to_owned(),
    ];
    let env: Vec<CString> = environment
        .iter()
        .map(|s| CString::new(s.as_str()).unwrap())
        .collect();
    let mut envp: Vec<*const libc::c_char> = env.iter().map(|s| s.as_ptr()).collect();
    envp.push(std::ptr::null());
    let mut limit = unsafe { std::mem::zeroed::<libc::rlimit>() };
    if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) } < 0 {
        return Err(unavailable());
    }
    let close_limit = limit.rlim_cur.min(i32::MAX as _) as i32;
    let mut master = -1;
    let mut slave = -1;
    if unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut size,
        )
    } < 0
    {
        return Err(unavailable());
    }
    let master = unsafe { OwnedFd::from_raw_fd(master) };
    let slave = unsafe { OwnedFd::from_raw_fd(slave) };
    for fd in [master.as_raw_fd(), slave.as_raw_fd()] {
        if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
            return Err(unavailable());
        }
    }
    let flags = unsafe { libc::fcntl(master.as_raw_fd(), libc::F_GETFL) };
    if flags < 0
        || unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
    {
        return Err(unavailable());
    }
    let mut term = unsafe { std::mem::zeroed::<libc::termios>() };
    if unsafe { libc::tcgetattr(slave.as_raw_fd(), &mut term) } < 0 {
        return Err(unavailable());
    }
    term.c_iflag = libc::ICRNL | libc::IXON;
    term.c_oflag = libc::OPOST | libc::ONLCR;
    term.c_cflag = (term.c_cflag & !(libc::CSIZE | libc::PARENB)) | libc::CS8 | libc::CREAD;
    term.c_lflag =
        libc::ISIG | libc::ICANON | libc::ECHO | libc::ECHOE | libc::ECHOK | libc::IEXTEN;
    term.c_cc[libc::VINTR] = 3;
    term.c_cc[libc::VQUIT] = 28;
    term.c_cc[libc::VERASE] = 127;
    term.c_cc[libc::VKILL] = 21;
    term.c_cc[libc::VEOF] = 4;
    term.c_cc[libc::VSTART] = 17;
    term.c_cc[libc::VSTOP] = 19;
    term.c_cc[libc::VSUSP] = 26;
    term.c_cc[libc::VMIN] = 1;
    term.c_cc[libc::VTIME] = 0;
    if unsafe { libc::tcsetattr(slave.as_raw_fd(), libc::TCSANOW, &term) } < 0 {
        return Err(unavailable());
    }
    let pid = unsafe { libc::fork() };
    if pid < 0 {
        return Err(unavailable());
    }
    if pid == 0 {
        // Child: syscall/libc async-signal-safe operations only. Never allocate,
        // lock, format, invoke Rust destructors or return into the server lane.
        unsafe {
            if libc::setsid() < 0 || libc::ioctl(slave.as_raw_fd(), libc::TIOCSCTTY as _, 0) < 0 {
                libc::_exit(126);
            }
            for fd in 0..3 {
                if libc::dup2(slave.as_raw_fd(), fd) < 0 {
                    libc::_exit(126);
                }
            }
            let mut mask = std::mem::zeroed::<libc::sigset_t>();
            libc::sigemptyset(&mut mask);
            libc::sigprocmask(libc::SIG_SETMASK, &mask, std::ptr::null_mut());
            for signal in [
                libc::SIGINT,
                libc::SIGQUIT,
                libc::SIGTERM,
                libc::SIGHUP,
                libc::SIGPIPE,
                libc::SIGCHLD,
                libc::SIGTSTP,
                libc::SIGTTIN,
                libc::SIGTTOU,
                libc::SIGWINCH,
            ] {
                libc::signal(signal, libc::SIG_DFL);
            }
            #[cfg(target_os = "linux")]
            {
                if libc::syscall(libc::SYS_close_range, 3u32, u32::MAX, 0u32) < 0 {
                    for fd in 3..close_limit {
                        libc::close(fd);
                    }
                }
            }
            #[cfg(target_os = "macos")]
            {
                for fd in 3..close_limit {
                    libc::close(fd);
                }
            }
            #[cfg(not(any(target_os = "linux", target_os = "macos")))]
            {
                for fd in 3..close_limit {
                    libc::close(fd);
                }
            }
            libc::execve(path.as_ptr(), argv.as_ptr(), envp.as_ptr());
            libc::_exit(127);
        }
    }
    drop(slave);
    Ok(Session {
        id,
        master,
        pid,
        reaped: false,
        eof: false,
        output: VecDeque::with_capacity(OUTPUT_BYTES),
        base: 0,
        input: VecDeque::with_capacity(INPUT_BYTES),
        touched: Instant::now(),
        closing: None,
        close_exit_sent: false,
        exit_code: None,
        rows: size.ws_row,
        cols: size.ws_col,
    })
}

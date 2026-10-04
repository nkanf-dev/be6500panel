//! One synchronous listener/owner lane. No manager creation, PID adoption,
//! saved-intent restoration, per-client thread or scheduler. The caller retains
//! the borrowed owner after shutdown/cleanup errors and must retry close.
use crate::{runtime_http::RuntimeHttp, runtime_manager::ManagerError, server::Service};
use std::{
    fmt, io,
    net::TcpListener,
    os::fd::AsRawFd,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
const IDLE_SLICE: Duration = Duration::from_millis(100);
const RECOVERY_INTERVAL: Duration = Duration::from_secs(1);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopError {
    Authentication,
    Listener,
    Shutdown(ManagerError),
}
impl fmt::Display for LoopError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Authentication => "owned runtime listener requires authentication",
            Self::Listener => "runtime listener unavailable",
            Self::Shutdown(_) => "runtime shutdown needs cleanup retry",
        })
    }
}
impl std::error::Error for LoopError {}
fn stopped(cancel: &AtomicBool) -> bool {
    cancel.load(Ordering::Acquire)
}
fn close(
    runtime: &mut Option<&mut RuntimeHttp>,
    result: Result<(), LoopError>,
) -> Result<(), LoopError> {
    if let Some(owner) = runtime.as_deref_mut() {
        owner.close().map_err(LoopError::Shutdown)?;
    }
    result
}
/// Does not restore startup intent. The caller explicitly loads/restores the
/// existing owned runtime before serving; recovery ticks are not GET effects.
/// A non-loopback listener and every attached owner require nonempty auth.
pub fn serve(
    listener: &TcpListener,
    service: &Service,
    mut runtime: Option<&mut RuntimeHttp>,
    cancel: &AtomicBool,
) -> Result<(), LoopError> {
    let address = match listener.local_addr() {
        Ok(address) => address,
        Err(_) => return close(&mut runtime, Err(LoopError::Listener)),
    };
    if (runtime.is_some() || !address.ip().is_loopback()) && !service.authentication_required() {
        return Err(LoopError::Authentication);
    }
    if listener.set_nonblocking(true).is_err() {
        return close(&mut runtime, Err(LoopError::Listener));
    }
    let mut next_recovery = Instant::now() + RECOVERY_INTERVAL;
    loop {
        if stopped(cancel) {
            return close(&mut runtime, Ok(()));
        }
        let now = Instant::now();
        if now >= next_recovery {
            if let Some(owner) = runtime.as_deref_mut() {
                let _ = owner.poll_recovery(now);
            }
            next_recovery = Instant::now() + RECOVERY_INTERVAL;
        }
        if stopped(cancel) {
            return close(&mut runtime, Ok(()));
        }
        let mut poll = libc::pollfd {
            fd: listener.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut poll, 1, IDLE_SLICE.as_millis() as libc::c_int) };
        if stopped(cancel) {
            return close(&mut runtime, Ok(()));
        }
        if ready < 0 {
            if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return close(&mut runtime, Err(LoopError::Listener));
        }
        if ready == 0 {
            continue;
        }
        if poll.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            return close(&mut runtime, Err(LoopError::Listener));
        }
        if poll.revents & libc::POLLIN == 0 {
            return close(&mut runtime, Err(LoopError::Listener));
        }
        match listener.accept() {
            Ok((stream, _)) => {
                if stopped(cancel) {
                    return close(&mut runtime, Ok(()));
                }
                if stream.set_nonblocking(false).is_err() {
                    continue;
                }
                // The one existing connection handler retains absolute header/body/write
                // and native operation deadlines. Invalid clients cannot stop the owner.
                if let Some(owner) = runtime.as_deref_mut() {
                    let _ = service.handle_with_runtime(stream, owner);
                } else {
                    let _ = service.handle(stream);
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) => {}
            Err(_) => return close(&mut runtime, Err(LoopError::Listener)),
        }
    }
}

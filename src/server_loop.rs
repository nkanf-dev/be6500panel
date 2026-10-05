//! One synchronous listener/owner lane. No manager creation, PID adoption,
//! saved-intent restoration, per-client thread or scheduler. The caller retains
//! the borrowed owner after shutdown/cleanup errors and must retry close.
use crate::{runtime_http::RuntimeHttp, runtime_manager::ManagerError, server::Service};
use std::{
    fmt, io,
    net::{SocketAddr, TcpListener},
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
    cancel.load(Ordering::Acquire) || crate::shutdown::requested()
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
/// The caller owns one listener throughout serve and any cleanup retries.
/// The old FD is dropped only after the new exact-address bind is ready.
fn rebind(listener: &mut TcpListener, target: SocketAddr) -> io::Result<()> {
    let current = listener.local_addr()?;
    if target.ip().is_unspecified() || target.port() != current.port() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid management bind",
        ));
    }
    if target == current {
        return Ok(());
    }
    let next = TcpListener::bind(target)?;
    next.set_nonblocking(true)?;
    *listener = next;
    Ok(())
}
/// Does not restore startup intent. The caller explicitly loads/restores the
/// existing owned runtime before serving; recovery ticks are not GET effects.
/// A non-loopback listener and every attached owner require nonempty auth.
pub fn serve(
    listener: &mut TcpListener,
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
    service.set_management_listener(address);
    let mut next_recovery = Instant::now() + RECOVERY_INTERVAL;
    loop {
        if stopped(cancel) {
            return close(&mut runtime, Ok(()));
        }
        service.flush_streams();
        service.terminal_tick();
        let now = Instant::now();
        if now >= next_recovery {
            if let Some(owner) = runtime.as_deref_mut() {
                let _ = owner.poll_recovery(now);
            }
            service.product_tick(runtime.as_deref_mut());
            if service.authentication_required()
                && let Some(target) = service.management_rebind_address()
            {
                // Bind failure is a retryable management-path problem, not a
                // runtime shutdown. Keep serving the old listener and owner.
                if rebind(listener, target).is_ok() {
                    service.set_management_listener(target);
                }
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

/// Failed shutdown keeps the same authenticated entry and owner. There are no
/// recovery ticks or automatic cleanup retries. Only another signal or parsed
/// authenticated DELETE/stop action requests one caller-owned close retry.
/// Invalid clients and GET cannot trigger capture/core mutations.
pub fn serve_cleanup(
    listener: &TcpListener,
    service: &Service,
    runtime: &mut RuntimeHttp,
    signal_sequence: u32,
) -> Result<(), LoopError> {
    if !service.authentication_required() {
        return Err(LoopError::Authentication);
    }
    listener
        .set_nonblocking(true)
        .map_err(|_| LoopError::Listener)?;
    let cleanup_sequence = runtime.cleanup_sequence();
    loop {
        if crate::shutdown::sequence() != signal_sequence {
            return Ok(());
        }
        let mut poll = libc::pollfd {
            fd: listener.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut poll, 1, IDLE_SLICE.as_millis() as libc::c_int) };
        if ready < 0 {
            if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(LoopError::Listener);
        }
        if ready == 0 {
            continue;
        }
        if poll.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            return Err(LoopError::Listener);
        }
        if poll.revents & libc::POLLIN == 0 {
            continue;
        }
        match listener.accept() {
            Ok((stream, _)) => {
                if stream.set_nonblocking(false).is_err() {
                    continue;
                }
                let _ = service.handle_with_runtime(stream, runtime);
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                continue;
            }
            Err(_) => return Err(LoopError::Listener),
        }
        if runtime.cleanup_sequence() != cleanup_sequence {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpStream;

    #[test]
    fn rebind_same_process_keeps_port_and_closes_old_listener_only_after_success() {
        let mut listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let old = listener.local_addr().unwrap();
        let next = SocketAddr::new("::1".parse().unwrap(), old.port());
        rebind(&mut listener, next).unwrap();
        assert_eq!(listener.local_addr().unwrap(), next);
        assert!(TcpStream::connect(next).is_ok());
        assert!(TcpStream::connect(old).is_err());
        // A free old address proves the old FD was dropped, not held by a clone.
        let old_again = TcpListener::bind(old).unwrap();
        assert_eq!(old_again.local_addr().unwrap(), old);
    }

    #[test]
    fn busy_rebind_keeps_old_entry_and_can_retry_without_shutdown() {
        let mut listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let old = listener.local_addr().unwrap();
        let next = SocketAddr::new("::1".parse().unwrap(), old.port());
        let busy = TcpListener::bind(next).unwrap();
        assert!(rebind(&mut listener, next).is_err());
        assert_eq!(listener.local_addr().unwrap(), old);
        assert!(TcpStream::connect(old).is_ok());
        drop(busy);
        rebind(&mut listener, next).unwrap();
        assert_eq!(listener.local_addr().unwrap(), next);
        assert!(TcpStream::connect(next).is_ok());
    }

    #[test]
    fn rebind_never_widens_to_wildcard_or_changes_port() {
        let mut listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let old = listener.local_addr().unwrap();
        assert!(
            rebind(
                &mut listener,
                SocketAddr::new("0.0.0.0".parse().unwrap(), old.port())
            )
            .is_err()
        );
        assert!(rebind(&mut listener, SocketAddr::new(old.ip(), 0)).is_err());
        assert_eq!(listener.local_addr().unwrap(), old);
    }
}

//! Fixed cooperative SIGINT/SIGTERM cancellation. No thread, allocation or
//! dangling pointer in the handler. Native cleanup/reap has independent budgets.
use std::{
    fmt, io,
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
};
static REQUESTED: AtomicBool = AtomicBool::new(false);
static INSTALLED: AtomicBool = AtomicBool::new(false);
static SIGNAL_SEQUENCE: AtomicU32 = AtomicU32::new(0);
pub fn sequence() -> u32 {
    SIGNAL_SEQUENCE.load(Ordering::Acquire)
}
pub fn requested() -> bool {
    REQUESTED.load(Ordering::Acquire)
}
pub fn flag() -> &'static AtomicBool {
    &REQUESTED
}
extern "C" fn handler(_: libc::c_int) {
    REQUESTED.store(true, Ordering::Release);
    SIGNAL_SEQUENCE.fetch_add(1, Ordering::AcqRel);
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SignalError;
impl fmt::Display for SignalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("native shutdown signal setup unavailable")
    }
}
impl std::error::Error for SignalError {}
pub struct SignalGuard {
    previous: [libc::sigaction; 2],
}
impl fmt::Debug for SignalGuard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("NativeSignalGuard")
    }
}
impl SignalGuard {
    pub fn install() -> Result<Self, SignalError> {
        if INSTALLED
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(SignalError);
        }
        REQUESTED.store(false, Ordering::Release);
        SIGNAL_SEQUENCE.store(0, Ordering::Release);
        let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
        action.sa_sigaction = handler as *const () as usize;
        if unsafe { libc::sigemptyset(&mut action.sa_mask) } != 0 {
            INSTALLED.store(false, Ordering::Release);
            return Err(SignalError);
        }
        action.sa_flags = 0;
        let mut previous: [libc::sigaction; 2] = unsafe { std::mem::zeroed() };
        for (index, signal) in [libc::SIGINT, libc::SIGTERM].into_iter().enumerate() {
            if unsafe { libc::sigaction(signal, &action, &mut previous[index]) } != 0 {
                if index == 1 {
                    unsafe {
                        libc::sigaction(libc::SIGINT, &previous[0], std::ptr::null_mut());
                    }
                }
                INSTALLED.store(false, Ordering::Release);
                return Err(SignalError);
            }
        }
        Ok(Self { previous })
    }
}
impl Drop for SignalGuard {
    fn drop(&mut self) {
        for (index, signal) in [libc::SIGINT, libc::SIGTERM].into_iter().enumerate() {
            unsafe {
                libc::sigaction(signal, &self.previous[index], std::ptr::null_mut());
            }
        }
        INSTALLED.store(false, Ordering::Release);
    }
}
/// Bounded synchronous close retry backoff; no sleep framework or helperthread.
pub fn wait_cleanup_retry(milliseconds: i32) {
    let result = unsafe { libc::poll(std::ptr::null_mut(), 0, milliseconds.clamp(0, 1000)) };
    if result < 0 {
        let _ = io::Error::last_os_error();
    }
}

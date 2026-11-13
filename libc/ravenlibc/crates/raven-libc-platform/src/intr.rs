// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `EINTR`, restart classes and deferred cancellation (doc 11 section 3.7).
//!
//! Every blocking libc call is written as `cancellable(restartable(...))`. A class R call is
//! retried after `EINTR` only when every handler that ran had `SA_RESTART`; class N calls return
//! `EINTR` regardless. `close` maps `EINTR` to success (the fd is gone either way, A-sys-11).
//! Pool-wide state lives in `raven-libc-thread` (C57) and installs the two hooks of
//! `raven-libc-sys::cancel`; with no hooks installed the wrappers are transparent.
//!
//! Cancellation is deferred only: `PTHREAD_CANCEL_ASYNCHRONOUS` is accepted and behaves as
//! deferred (O-7). Handlers run only at libc boundaries, never inside the fd-table lock.

use raven_abi::errno::Errno;

/// Restart class of a blocking call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallClass {
    /// Restart after `EINTR` when the handlers allow it.
    Restartable,
    /// Never restart; return `EINTR` even with `SA_RESTART`.
    NeverRestarted,
    /// `close`: `EINTR` is success.
    Close,
}

/// Class R names (doc 11 section 3.7 item 3).
pub const CLASS_R: &[&str] = &[
    "open",
    "openat",
    "read",
    "readv",
    "pread",
    "preadv",
    "write",
    "writev",
    "pwrite",
    "pwritev",
    "ioctl",
    "fcntl",
    "flock",
    "lockf",
    "accept",
    "accept4",
    "recv",
    "recvfrom",
    "recvmsg",
    "send",
    "sendto",
    "sendmsg",
    "wait",
    "waitpid",
    "wait3",
    "wait4",
    "waitid",
    "sem_wait",
    "pthread_mutex_lock",
    "connect",
];

/// Class N names (doc 11 section 3.7 item 3).
pub const CLASS_N: &[&str] = &[
    "poll",
    "ppoll",
    "select",
    "pselect",
    "epoll_wait",
    "epoll_pwait",
    "nanosleep",
    "clock_nanosleep",
    "sleep",
    "usleep",
    "pause",
    "sigsuspend",
    "sigtimedwait",
    "sigwait",
    "sigwaitinfo",
    "sem_timedwait",
    "pthread_cond_timedwait",
    "mq_timedreceive",
    "mq_timedsend",
    "io_getevents",
];

/// Classify a call by name.
pub fn class_of(name: &str) -> Option<CallClass> {
    if name == "close" || name == "posix_close" {
        return Some(CallClass::Close);
    }
    if CLASS_R.contains(&name) {
        return Some(CallClass::Restartable);
    }
    if CLASS_N.contains(&name) {
        return Some(CallClass::NeverRestarted);
    }
    None
}

/// Run one blocking operation under a restart decision.
///
/// `restart_allowed` is computed by the signal area after the pending handlers ran; it is
/// `false` when no hook is installed, so the wrapper is transparent.
pub fn run<T>(
    class: CallClass,
    restart_allowed: bool,
    mut operation: impl FnMut() -> Result<T, Errno>,
) -> Result<T, Errno> {
    loop {
        match operation() {
            Err(Errno::EINTR) => match class {
                CallClass::Close => return Err(Errno::EINTR),
                CallClass::Restartable if restart_allowed => continue,
                CallClass::Restartable | CallClass::NeverRestarted => return Err(Errno::EINTR),
            },
            other => return other,
        }
    }
}

/// The `close` wrapper: `EINTR` means the descriptor is gone, which is success.
pub fn close(mut operation: impl FnMut() -> Result<(), Errno>) -> Result<(), Errno> {
    match operation() {
        Err(Errno::EINTR) => Ok(()),
        other => other,
    }
}

/// A cancellation-aware wrapper: check pending cancellation before entering the call.
pub fn cancellable<T>(mut operation: impl FnMut() -> Result<T, Errno>) -> Result<T, Errno> {
    #[cfg(feature = "cancel")]
    if raven_libc_sys::cancel::pending_cancel() {
        return Err(Errno::EINTR);
    }
    operation()
}

/// Whether the installed hooks ask for a restart after `EINTR`.
pub fn restart_wanted() -> bool {
    #[cfg(feature = "cancel")]
    {
        matches!(
            raven_libc_sys::cancel::on_eintr(),
            raven_libc_sys::cancel::RestartAction::Restart
        )
    }
    #[cfg(not(feature = "cancel"))]
    {
        false
    }
}

/// Remaining nanoseconds before an absolute monotonic deadline; `0` when it has passed.
pub fn deadline_remaining_ns(deadline_ns: u64, now_ns: u64) -> u64 {
    deadline_ns.saturating_sub(now_ns)
}

/// An absolute monotonic deadline from a relative timeout and a start time.
pub fn deadline_from_now(timeout_ns: u64, now_ns: u64) -> u64 {
    now_ns.saturating_add(timeout_ns)
}

/// The POSIX.1-2024 cancellation-point list this crate implements wrappers for.
pub const CANCELLATION_POINTS: &[&str] = &[
    "accept",
    "accept4",
    "aio_suspend",
    "clock_nanosleep",
    "close",
    "connect",
    "creat",
    "fcntl",
    "fdatasync",
    "fsync",
    "lockf",
    "mq_receive",
    "mq_send",
    "mq_timedreceive",
    "mq_timedsend",
    "msgrcv",
    "msgsnd",
    "msync",
    "nanosleep",
    "open",
    "openat",
    "pause",
    "poll",
    "ppoll",
    "pread",
    "preadv",
    "pselect",
    "pthread_cond_timedwait",
    "pthread_cond_wait",
    "pthread_join",
    "pthread_testcancel",
    "pwrite",
    "pwritev",
    "read",
    "readv",
    "recv",
    "recvfrom",
    "recvmsg",
    "select",
    "sem_timedwait",
    "sem_wait",
    "send",
    "sendmsg",
    "sendto",
    "sigsuspend",
    "sigtimedwait",
    "sigwait",
    "sigwaitinfo",
    "sleep",
    "system",
    "tcdrain",
    "usleep",
    "wait",
    "wait3",
    "wait4",
    "waitid",
    "waitpid",
    "write",
    "writev",
];

/// Whether `name` is a POSIX cancellation point.
pub fn is_cancellation_point(name: &str) -> bool {
    CANCELLATION_POINTS.contains(&name)
}

#[cfg(test)]
#[path = "intr_ut.rs"]
mod tests;

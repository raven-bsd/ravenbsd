// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The raw process-signal queue syscalls (`PROC_SIGNAL` 0x4A / `PROC_SIGRECV` 0x4B).
//!
//! `PROC_SIGNAL` queues a signal for a pid/process group; `PROC_SIGRECV` blocks for the next
//! queued signal and writes `[signo, sender_pid, 0]`. They are raw numbers, not
//! `raven_abi::syscall::Syscall` variants, so the typed wrapper count is unchanged.

use raven_abi::errno::Errno;

use crate::arch;

/// Raw `proc_signal(target, signo) -> 0`.
pub const PROC_SIGNAL: usize = 0x4A;
/// Raw `proc_sigrecv(out, flags) -> signo`; `flags` bit 0 is `WNOHANG`.
pub const PROC_SIGRECV: usize = 0x4B;
/// `WNOHANG` for `PROC_SIGRECV`.
pub const WNOHANG: u32 = 1;
/// Raw `thread_sigqueue(thread, signo, value) -> 0` (C19 provider operation).
pub const THREAD_SIGQUEUE: usize = raven_abi::syscall::THREAD_SIGQUEUE;
/// Raw `sigpending(out) -> count` (C19 provider operation).
pub const SIGPENDING: usize = raven_abi::syscall::SIGPENDING;
/// Raw `sigwait(set, out, timeout_ns, flags) -> signo` (C19 provider operation).
pub const SIGWAIT: usize = raven_abi::syscall::SIGWAIT;
/// Raw `sigprocmask(how, set, old) -> 0` (C19 provider operation).
pub const SIGPROCMASK: usize = raven_abi::syscall::SIGPROCMASK;
/// `SIGWAIT` flag: poll once and return `EAGAIN` when nothing matches.
pub const SIGWAIT_POLL: u32 = raven_abi::syscall::SIGWAIT_POLL;
/// `Handle::NONE`: target the calling thread (`raven_abi::types::Handle`).
pub const HANDLE_NONE: u32 = u32::MAX;

/// Queue `signo` for `pid` (positive pid, 0 = caller's group, negative = -pgid).
///
/// # Errors
///
/// The kernel's `EINVAL`/`EPERM`/`ESRCH`.
pub fn proc_signal(pid: i32, signo: i32) -> Result<(), Errno> {
    // SAFETY: PROC_SIGNAL takes two scalar arguments.
    crate::errno::ret(unsafe { arch::syscall2(PROC_SIGNAL, pid as usize, signo as usize) })
        .map(|_| ())
}

/// Receive the next queued signal into `out` (`[signo, sender, 0]`).
///
/// With `WNOHANG` the call returns `EAGAIN` instead of blocking.
///
/// # Errors
///
/// The kernel's `EAGAIN`/`EINTR`/`EFAULT`/`EINVAL`.
pub fn proc_sigrecv(out: &mut [u64; 3], flags: u32) -> Result<i32, Errno> {
    // SAFETY: PROC_SIGRECV writes exactly 24 bytes into the caller's `out`.
    crate::errno::ret_i32(unsafe {
        arch::syscall2(PROC_SIGRECV, out.as_mut_ptr() as usize, flags as usize)
    })
}

/// Queue `signo` on one thread (`Handle::NONE` = the calling thread).
///
/// # Errors
///
/// The kernel's `EINVAL`/`EBADF`/`EPERM`/`ESRCH`.
pub fn thread_sigqueue(thread: u32, signo: i32, value: u64) -> Result<(), Errno> {
    // SAFETY: THREAD_SIGQUEUE takes three scalar words and validates the handle itself.
    crate::errno::ret(unsafe {
        arch::syscall3(
            THREAD_SIGQUEUE,
            thread as usize,
            signo as usize,
            value as usize,
        )
    })
    .map(|_| ())
}

/// Read the kernel's authoritative pending word for the calling thread.
///
/// # Errors
///
/// The kernel's `EFAULT`/`ESRCH`.
pub fn sigpending(out: &mut u64) -> Result<usize, Errno> {
    // SAFETY: SIGPENDING writes exactly one u64 into the live `out`.
    crate::errno::ret(unsafe { arch::syscall1(SIGPENDING, out as *mut u64 as usize) })
}

/// Consume one pending signal in `set` with the kernel's atomic temporary mask.
///
/// `out` receives the six-word record `[signo, sender_pid, si_code, si_uid, si_value, 0]`.
///
/// # Errors
///
/// The kernel's `EAGAIN` (poll/timeout), `EINVAL`, `EFAULT`, `EINTR`.
pub fn sigwait(set: u64, out: &mut [u64; 6], timeout_ns: u64, flags: u32) -> Result<i32, Errno> {
    // SAFETY: SIGWAIT reads one u64 and writes exactly six u64 words into live objects.
    crate::errno::ret_i32(unsafe {
        arch::syscall4(
            SIGWAIT,
            &set as *const u64 as usize,
            out.as_mut_ptr() as usize,
            timeout_ns as usize,
            flags as usize,
        )
    })
}

/// Change or read the calling thread's kernel signal mask.
///
/// # Errors
///
/// The kernel's `EINVAL`/`EFAULT`/`ESRCH`.
pub fn sigprocmask(how: i32, set: Option<u64>, old: Option<&mut u64>) -> Result<(), Errno> {
    let set_word = set.unwrap_or(0);
    let set_ptr = if set.is_some() {
        &set_word as *const u64 as usize
    } else {
        0
    };
    let old_ptr = old.map_or(0, |word| word as *mut u64 as usize);
    // SAFETY: both pointers name live u64 words for the duration of the call; the kernel copies
    // one word in and/or out.
    crate::errno::ret(unsafe { arch::syscall3(SIGPROCMASK, how as usize, set_ptr, old_ptr) })
        .map(|_| ())
}

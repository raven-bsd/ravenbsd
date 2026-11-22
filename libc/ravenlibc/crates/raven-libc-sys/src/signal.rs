// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The raw process-signal queue syscalls (`PROC_SIGNAL` 0x4A / `PROC_SIGRECV` 0x4B).
//!
//! `PROC_SIGNAL` queues a signal for a pid/process group; `PROC_SIGRECV` blocks for the next
//! queued signal and writes `[signo, sender_pid, 0]`. They are raw numbers, not
//! `raven_abi::syscall::Syscall` variants, so the typed wrapper count is unchanged.

use raven_abi::errno::Errno;
use raven_abi::signal::{SIG_SETUP, SIGRESEND};
use raven_abi::syscall::THREAD_KICK;

use crate::arch;

/// Raw `proc_signal(target, signo) -> 0`.
pub const PROC_SIGNAL: usize = 0x4A;
/// Raw `proc_sigrecv(out, flags) -> signo`; `flags` bit 0 is `WNOHANG`.
pub const PROC_SIGRECV: usize = 0x4B;
/// `WNOHANG` for `PROC_SIGRECV`.
pub const WNOHANG: u32 = 1;

/// `SIG_SETUP`: register the user signal entry and its TLS-relative context area.
///
/// # Errors
///
/// The kernel's `EINVAL`/`EFAULT`/`ESRCH`.
pub fn sig_setup(entry: usize, ctx_offset: usize, flags: u32) -> Result<(), Errno> {
    // SAFETY: three scalar arguments; the kernel validates the user entry and the offset.
    crate::errno::ret(unsafe { arch::syscall3(SIG_SETUP, entry, ctx_offset, flags as usize) })
        .map(|_| ())
}

/// `THREAD_KICK`: set sticky signal bits on a thread and wake an interruptible target.
///
/// # Errors
///
/// The kernel's `EBADF`/`EPERM`/`ESRCH`.
pub fn thread_kick(thread: u32, bits: u32) -> Result<(), Errno> {
    // SAFETY: two scalar arguments; the kernel validates the thread handle.
    crate::errno::ret(unsafe { arch::syscall2(THREAD_KICK, thread as usize, bits as usize) })
        .map(|_| ())
}

/// `SIGRESEND`: re-enter the signal handler after `SIGRETURN`.
///
/// # Errors
///
/// The kernel's `EINVAL`/`ENOSYS`.
pub fn sigresend(sig: u32, info: usize, mask: u64) -> Result<(), Errno> {
    // SAFETY: three scalar arguments; `info` is opaque to the kernel.
    crate::errno::ret(unsafe { arch::syscall3(SIGRESEND, sig as usize, info, mask as usize) })
        .map(|_| ())
}

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

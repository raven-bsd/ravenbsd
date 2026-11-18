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

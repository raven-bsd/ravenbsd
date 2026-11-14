// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `signalfd` and `struct signalfd_siginfo` (`L04`, `09` 3.7).
//!
//! The record is the Linux 128-byte layout of the installed `sys/signalfd.h`; the unit test
//! parses that header and asserts the offsets. The signal source itself needs `PG-7`: until it
//! exists [`signalfd`] validates its arguments and returns `ENOSYS`; there is no fake success.

use crate::seams::{Errno, FdKind, FdTable, SigSet, SignalOps, sigset_contains};

/// `SFD_CLOEXEC`.
pub const SFD_CLOEXEC: i32 = raven_libc_abi::fcntl::O_CLOEXEC;
/// `SFD_NONBLOCK`.
pub const SFD_NONBLOCK: i32 = raven_libc_abi::fcntl::O_NONBLOCK;

/// `SIGKILL`: cannot be caught, blocked or queued.
pub const SIGKILL: u32 = 9;
/// `SIGSTOP`: cannot be caught or blocked.
pub const SIGSTOP: u32 = 19;

/// The Linux `struct signalfd_siginfo` (128 bytes).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SignalfdSiginfo {
    /// Signal number.
    pub ssi_signo: u32,
    /// `errno` of the signal.
    pub ssi_errno: i32,
    /// `si_code`.
    pub ssi_code: i32,
    /// Sending process id.
    pub ssi_pid: u32,
    /// Sending user id.
    pub ssi_uid: u32,
    /// File descriptor of the signal source.
    pub ssi_fd: i32,
    /// Thread id.
    pub ssi_tid: u32,
    /// Band event.
    pub ssi_band: u32,
    /// Timer overrun count.
    pub ssi_overrun: u32,
    /// Trap number.
    pub ssi_trapno: u32,
    /// Exit status or signal.
    pub ssi_status: i32,
    /// Integer value.
    pub ssi_int: i32,
    /// Pointer value.
    pub ssi_ptr: u64,
    /// User CPU time.
    pub ssi_utime: u64,
    /// System CPU time.
    pub ssi_stime: u64,
    /// Fault address.
    pub ssi_addr: u64,
    /// Low address bit.
    pub ssi_addr_lsb: u16,
    /// Reserved padding.
    pub ssi_pad2: u16,
    /// Syscall number.
    pub ssi_syscall: i32,
    /// Syscall call address.
    pub ssi_call_addr: u64,
    /// Syscall architecture.
    pub ssi_arch: u32,
    /// Trailing padding to 128 bytes.
    pub ssi_pad: [u8; 28],
}

const _: () = assert!(core::mem::size_of::<SignalfdSiginfo>() == 128);
const _: () = assert!(core::mem::offset_of!(SignalfdSiginfo, ssi_ptr) == 48);
const _: () = assert!(core::mem::offset_of!(SignalfdSiginfo, ssi_addr) == 72);
const _: () = assert!(core::mem::offset_of!(SignalfdSiginfo, ssi_call_addr) == 88);
const _: () = assert!(core::mem::offset_of!(SignalfdSiginfo, ssi_arch) == 96);

/// Whether the mask names an internal signal that `signalfd` must reject.
#[must_use]
pub fn mask_has_internal(mask: &SigSet) -> bool {
    sigset_contains(mask, SIGKILL) || sigset_contains(mask, SIGSTOP)
}

/// `signalfd(fd, mask, flags)`.
///
/// `fd == -1` creates a new descriptor; a valid descriptor updates its mask and returns the
/// same number. The flags and the mask are validated before the `PG-7` check so a caller with a
/// bad argument always sees `EINVAL`.
pub fn signalfd<T: FdTable, S: SignalOps>(
    t: &mut T,
    signal: &mut S,
    fd: i32,
    mask: &SigSet,
    flags: i32,
) -> Result<i32, Errno> {
    if flags & !(SFD_CLOEXEC | SFD_NONBLOCK) != 0 {
        return Err(Errno::EINVAL);
    }
    if mask_has_internal(mask) {
        return Err(Errno::EINVAL);
    }
    if fd == -1 {
        let handle = signal.create(mask, flags)?;
        return t.install(handle, FdKind::SignalFd, flags & SFD_CLOEXEC != 0);
    }
    if t.kind_of(fd)? != FdKind::SignalFd {
        return Err(Errno::EINVAL);
    }
    let handle = t.handle_of(fd)?;
    signal.update(handle, mask)?;
    Ok(fd)
}

#[cfg(test)]
#[path = "signalfd_ut.rs"]
mod tests;

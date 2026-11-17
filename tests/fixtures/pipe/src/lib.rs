// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Raw process helpers shared by the P2j pipe fixtures.

#![cfg_attr(not(test), no_std)]
#![allow(unsafe_code)]

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::Handle;

/// Raw `proc_fork(src, flags)` number (the ABI crate is reviewer-owned).
pub const PROC_FORK: usize = 0x39;
/// Raw `proc_sigrecv(out, flags)` number.
pub const PROC_SIGRECV: usize = 0x4B;
/// `PROC_SIGRECV` non-blocking flag.
pub const WNOHANG: usize = 1;
/// POSIX `SIGPIPE`.
pub const SIGPIPE: u32 = 13;

/// Fork `source` with the raw `PROC_FORK` call.
pub fn fork(source: Handle) -> Result<Handle, Errno> {
    // SAFETY: all arguments are scalar handles/flags; the kernel validates them.
    handle(unsafe { raven_rt::sys::syscall6(PROC_FORK, source.0 as usize, 0, 0, 0, 0, 0) })
}

/// Reap `process` and store its exit code.
pub fn wait(process: Handle, code: &mut i32) -> Result<(), Errno> {
    // SAFETY: `code` is a live writable i32 and the kernel writes at most four bytes.
    Errno::from_ret(unsafe {
        raven_rt::sys::syscall6(
            Syscall::PROC_WAIT as usize,
            process.0 as usize,
            code as *mut i32 as usize,
            0,
            0,
            0,
            0,
        )
    })
    .map(|_| ())
}

/// Receive one queued signal into `(signo, sender, 0)`.
pub fn sigrecv(out: &mut [u64; 3], flags: usize) -> Result<u32, Errno> {
    // SAFETY: `out` is a live three-word array and the kernel writes exactly 24 bytes.
    Errno::from_ret(unsafe {
        raven_rt::sys::syscall6(PROC_SIGRECV, out.as_mut_ptr() as usize, flags, 0, 0, 0, 0)
    })?;
    u32::try_from(out[0]).map_err(|_| Errno::EINVAL)
}

fn handle(raw: isize) -> Result<Handle, Errno> {
    let value = Errno::from_ret(raw)?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EBADF)
}

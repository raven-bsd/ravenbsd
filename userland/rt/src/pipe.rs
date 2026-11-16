// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `pipe(2)` and `pipe2(2)`.
//!
//! The kernel numbers are raw kernel-owned calls (`0x6e`/`0x6f`); the contract crate keeps the
//! public native table unchanged until a later ABI review. The two returned values are ordinary
//! handles, so `read`, `write`, `handle_dup` and `handle_close` work on them unchanged.

use raven_abi::errno::Errno;
use raven_abi::types::Handle;

use crate::sys::{syscall1, syscall2};

/// `O_NONBLOCK` (the Linux/libc descriptor value).
pub const O_NONBLOCK: u32 = 0o4000;
/// `O_CLOEXEC` (the Linux/libc descriptor value).
pub const O_CLOEXEC: u32 = 0o2000000;

/// Raw `pipe(fds)`.
const PIPE: usize = 0x6e;
/// Raw `pipe2(fds, flags)`.
const PIPE2: usize = 0x6f;

/// Create a blocking pipe and return `(read, write)` handles.
///
/// # Errors
/// [`Errno`] from the kernel (`ENOMEM`, `EMFILE`, `EFAULT`).
pub fn pipe() -> Result<(Handle, Handle), Errno> {
    let mut fds = [0i32; 2];
    // SAFETY: `fds` is a live two-element array; the kernel writes exactly two i32 values.
    let raw = unsafe { syscall1(PIPE, fds.as_mut_ptr() as usize) };
    Errno::from_ret(raw)?;
    decode_fds(fds)
}

/// Create a pipe with `flags` (`O_NONBLOCK`, `O_CLOEXEC`).
///
/// # Errors
/// [`Errno`] from the kernel (`EINVAL` for an unknown flag, `ENOMEM`, `EMFILE`, `EFAULT`).
pub fn pipe2(flags: u32) -> Result<(Handle, Handle), Errno> {
    let mut fds = [0i32; 2];
    // SAFETY: `fds` is a live two-element array; the kernel writes exactly two i32 values.
    let raw = unsafe { syscall2(PIPE2, fds.as_mut_ptr() as usize, flags as usize) };
    Errno::from_ret(raw)?;
    decode_fds(fds)
}

fn decode_fds(fds: [i32; 2]) -> Result<(Handle, Handle), Errno> {
    let read = u32::try_from(fds[0]).map_err(|_| Errno::EBADF)?;
    let write = u32::try_from(fds[1]).map_err(|_| Errno::EBADF)?;
    Ok((Handle(read), Handle(write)))
}

#[cfg(test)]
#[path = "pipe_ut.rs"]
mod tests;

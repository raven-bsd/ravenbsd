// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Raw controlling-terminal helpers shared by the P2m tty fixtures.

#![cfg_attr(not(test), no_std)]
#![allow(unsafe_code)] // each raw syscall block has a SAFETY comment

use raven_abi::errno::Errno;
use raven_abi::types::Handle;

const PROC_CTTY_SET: usize = 0x1e0;
const PROC_CTTY_GET: usize = 0x1e1;
const PROC_CTTY_SET_FG: usize = 0x1e2;
const PROC_SELF_INFO: usize = 0x35;
const PROC_SIGNAL: usize = 0x4A;

/// Acquire `handle` as the calling session leader's controlling terminal.
pub fn tiocsctty(handle: Handle) -> Result<(), Errno> {
    // SAFETY: the handle is a scalar; the kernel validates it.
    let result = unsafe { raven_rt::sys::syscall1(PROC_CTTY_SET, handle.0 as usize) };
    Errno::from_ret(result).map(|_| ())
}

/// Read the controlling terminal's foreground process group.
pub fn tcgetpgrp(handle: Handle) -> Result<i32, Errno> {
    let mut pgrp = 0i32;
    // SAFETY: `pgrp` is a live i32; the kernel writes exactly four bytes through the pointer.
    let result = unsafe {
        raven_rt::sys::syscall2(
            PROC_CTTY_GET,
            handle.0 as usize,
            &mut pgrp as *mut i32 as usize,
        )
    };
    Errno::from_ret(result)?;
    Ok(pgrp)
}

/// Set the controlling terminal's foreground process group.
pub fn tcsetpgrp(handle: Handle, pgrp: i32) -> Result<(), Errno> {
    // SAFETY: both arguments are scalar; the kernel validates the handle and process group.
    let result = unsafe {
        raven_rt::sys::syscall2(PROC_CTTY_SET_FG, handle.0 as usize, pgrp as isize as usize)
    };
    Errno::from_ret(result).map(|_| ())
}

/// Send a signal to a pid or process group.
pub fn signal(target: i32, signo: u32) -> Result<(), Errno> {
    // SAFETY: both arguments are scalar; the kernel validates the target and signal.
    let result =
        unsafe { raven_rt::sys::syscall2(PROC_SIGNAL, target as isize as usize, signo as usize) };
    Errno::from_ret(result).map(|_| ())
}

/// The caller's process-group id.
pub fn self_pgid() -> Result<i32, Errno> {
    let mut info = [0u64; 8];
    // SAFETY: PROC_SELF_INFO writes eight u64 words to the live array owned by the caller.
    let raw =
        unsafe { raven_rt::sys::syscall2(PROC_SELF_INFO, info.as_mut_ptr() as usize, info.len()) };
    Errno::from_ret(raw)?;
    i32::try_from(info[2]).map_err(|_| Errno::EINVAL)
}

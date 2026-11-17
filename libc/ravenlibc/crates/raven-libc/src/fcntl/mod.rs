// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The C `fcntl` record-lock pointer hand-off.
//!
//! `raven-libc-platform` forbids `unsafe` and works on safe values, so the shell owns the
//! caller's `struct flock` copy-in/copy-out and the fd-to-handle translation stays in the
//! platform crate.

use core::ffi::c_int;

use raven_libc_abi::structs::Flock;
use raven_libc_platform::Errno;

/// Apply one `F_GETLK`/`F_SETLK`/`F_SETLKW` to the file behind `fd`.
///
/// # Errors
/// [`Errno::EFAULT`] for a null lock pointer and the platform/kernel errno otherwise.
// The target-only fcntl entry is the only caller; the host build compiles the module for tests.
#[cfg_attr(not(target_os = "none"), allow(dead_code))]
pub(crate) fn record_lock(fd: c_int, command: c_int, argument: usize) -> Result<c_int, Errno> {
    if argument == 0 {
        return Err(Errno::EFAULT);
    }
    // SAFETY: the caller's `fcntl` contract for a lock command is a live, writable `struct flock`.
    let mut lock = unsafe { core::ptr::read(argument as *const Flock) };
    raven_libc_platform::fcntl::record_lock(
        crate::ffi::platform::global(),
        fd,
        command,
        &mut lock,
    )?;
    // SAFETY: the same caller-owned pointer is writable and the record holds no pointers.
    unsafe { core::ptr::write(argument as *mut Flock, lock) };
    Ok(0)
}

#[cfg(test)]
#[path = "fcntl_ut.rs"]
mod tests;

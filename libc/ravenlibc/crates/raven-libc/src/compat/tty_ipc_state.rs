// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Shared error mapping and the speed-setter shell for the `<termios.h>` exports.

use core::ffi::c_int;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::structs::Termios;

use super::fail;

/// Map a platform error into `-1` with `errno` set.
pub(crate) fn map(error: raven_libc_platform::Errno) -> c_int {
    crate::ffi::errno::set_platform(error);
    -1
}

/// Apply one of the speed setters to a caller record.
///
/// # Safety
/// `termios_p` is NULL or points to a writable `struct termios`.
pub(crate) unsafe fn set_speed(
    termios_p: *mut Termios,
    speed: u32,
    apply: fn(&mut Termios, u32) -> Result<(), Errno>,
) -> c_int {
    // SAFETY: the caller contract above says the pointer is NULL or points to a writable
    // `struct termios`.
    let Some(termios) = (unsafe { termios_p.as_mut() }) else {
        return fail(raven_libc_ipc::EINVAL);
    };
    match apply(termios, speed) {
        Ok(()) => 0,
        Err(error) => fail(error),
    }
}

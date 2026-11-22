// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `tcsetattr(3)`.

use core::ffi::c_int;

use raven_libc_abi::structs::Termios;

/// Apply `termios_p` to the terminal `fd`.
///
/// The platform seam exposes `TCSETS` (the `TCSANOW` action); the drain/flush variants share
/// the same attribute copy here because the tty scheme applies the settings synchronously.
/// `TCSADRAIN` is safe by construction: the pty applies the record without touching the master
/// output queue, so bytes queued before the call are still readable afterwards (K444).
///
/// # Safety
/// `termios_p` is NULL or points to a readable `struct termios`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcsetattr(
    fd: c_int,
    _optional_actions: c_int,
    termios_p: *const Termios,
) -> c_int {
    if termios_p.is_null() {
        crate::ffi::errno::set(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        ));
        return -1;
    }
    // SAFETY: `termios_p` is the caller's readable struct termios.
    let value = unsafe { core::ptr::read(termios_p) };
    match raven_libc_platform::ioctl::ioctl_tcsets(crate::ffi::platform::global(), fd, &value) {
        Ok(()) => 0,
        Err(error) => {
            crate::ffi::errno::set(crate::platform::map_error(error));
            -1
        }
    }
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `tcgetattr(3)`.

use core::ffi::c_int;

use raven_libc_abi::structs::Termios;

/// Fill `termios_p` with the terminal attributes of `fd`.
///
/// # Safety
/// `termios_p` is NULL or points to a writable `struct termios`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcgetattr(fd: c_int, termios_p: *mut Termios) -> c_int {
    if termios_p.is_null() {
        crate::ffi::errno::set(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        ));
        return -1;
    }
    match raven_libc_platform::ioctl::ioctl_tcgets(crate::ffi::platform::global(), fd) {
        Ok(value) => {
            // SAFETY: `termios_p` is the caller's writable struct termios.
            unsafe { core::ptr::write(termios_p, value) };
            0
        }
        Err(error) => {
            crate::ffi::errno::set(crate::platform::map_error(error));
            -1
        }
    }
}

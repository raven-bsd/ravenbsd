// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pipe2(2)`.
use core::ffi::c_int;

/// Create a pipe and write both descriptors to `fds`.
///
/// # Safety
/// `fds` is NULL or points to two writable `int` values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pipe2(fds: *mut c_int, flags: c_int) -> c_int {
    if fds.is_null() {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    match crate::ffi::platform::global().pipe2(flags) {
        Ok((read, write)) => {
            // SAFETY: `fds` points to two writable ints per the caller contract.
            unsafe {
                core::ptr::write(fds, read);
                core::ptr::write(fds.add(1), write);
            }
            0
        }
        Err(error) => crate::ffi::ret::int(Err(crate::platform::map_error(error))),
    }
}

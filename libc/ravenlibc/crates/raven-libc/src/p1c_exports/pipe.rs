// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `pipe`.

use super::*;

/// Create a pipe.
///
/// # Safety
/// `fds` points to two writable `int` values.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn pipe(fds: *mut c_int) -> c_int {
    if fds.is_null() {
        return ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    match global().pipe() {
        Ok((read, write)) => {
            // SAFETY: the caller supplied two writable ints.
            unsafe {
                core::ptr::write(fds, read);
                core::ptr::write(fds.add(1), write);
            }
            0
        }
        Err(error) => ret::int(Err(map_error(error))),
    }
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `posix_getdents(3)`.
use core::ffi::{c_int, c_void};

/// Read raw directory records from `fd`.
///
/// # Safety
/// `buf` points to `len` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn posix_getdents(
    fd: c_int,
    buf: *mut c_void,
    len: usize,
    flags: c_int,
) -> isize {
    if flags != 0 {
        return crate::ffi::ret::ssize(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EOPNOTSUPP as u16,
        )));
    }
    if len != 0 && buf.is_null() {
        return crate::ffi::ret::ssize(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    // SAFETY: the caller's buffer contract; a zero length is the empty slice.
    let out: &mut [u8] = if len == 0 {
        &mut []
    } else {
        unsafe { core::slice::from_raw_parts_mut(buf.cast::<u8>(), len) }
    };
    let result = raven_libc_platform::compat::getdents(crate::ffi::platform::global(), fd, out);
    match result {
        Ok(count) => crate::ffi::ret::ssize(Ok(count as isize)),
        Err(error) => crate::ffi::ret::ssize(Err(raven_libc_abi::errno::Errno(error.0))),
    }
}

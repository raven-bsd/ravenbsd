// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `readlinkat`.

use super::*;

/// Read a symbolic link relative to `dirfd`.
///
/// # Safety
/// `path` is NUL terminated; `buf` has `len` writable bytes.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn readlinkat(
    dirfd: c_int,
    path: *const c_char,
    buf: *mut c_char,
    len: usize,
) -> isize {
    if path.is_null() {
        return ret::ssize(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    // SAFETY: the caller's C string contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    // SAFETY: the caller's writable buffer contract.
    let buf = unsafe { crate::ffi::cstr::slice_mut(buf, len) };
    ssize(raven_libc_platform::compat::readlinkat(
        global(),
        dirfd,
        path,
        buf,
    ))
}

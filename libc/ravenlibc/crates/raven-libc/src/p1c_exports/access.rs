// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P1c exports, grouped so each codegen unit stays within the K4 C-symbol budget.

use super::*;

/// Access check relative to `dirfd`.
///
/// # Safety
/// `path` is NUL terminated.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn faccessat(
    dirfd: c_int,
    path: *const c_char,
    mode: c_int,
    flags: c_int,
) -> c_int {
    if path.is_null() {
        return ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    // SAFETY: the caller's C string contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    unit(raven_libc_platform::compat::faccessat(
        global(),
        dirfd,
        path,
        mode,
        flags,
    ))
}
/// Read a path configuration value.
///
/// # Safety
/// C ABI scalar arguments.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn fpathconf(fd: c_int, name: c_int) -> c_long {
    match global().fpathconf(fd, name) {
        Ok(value) => c_long::try_from(value).unwrap_or(c_long::MAX),
        Err(error) => ret::long(Err(map_error(error))),
    }
}
/// Fill a buffer with entropy.
///
/// # Safety
/// `buffer` has `len` writable bytes and `len <= 256`.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn getentropy(buffer: *mut c_void, len: usize) -> c_int {
    // SAFETY: the caller's buffer contract.
    let buffer = unsafe { crate::ffi::cstr::slice_mut(buffer.cast(), len) };
    unit(global().getentropy(buffer))
}
/// Return the page size.
///
/// # Safety
/// C ABI no arguments.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn getpagesize() -> c_int {
    raven_libc_platform::mem::getpagesize()
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `symlinkat(2)`.
use core::ffi::{c_char, c_int};

const AT_FDCWD: c_int = -100;

/// Create a symlink relative to `dirfd`; cwd-relative calls use `symlink`.
///
/// # Safety
/// Both path pointers are NULL or NUL-terminated strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn symlinkat(
    target: *const c_char,
    dirfd: c_int,
    link: *const c_char,
) -> c_int {
    if dirfd != AT_FDCWD {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::ENOSYS as u16,
        )));
    }
    // SAFETY: the caller's C string contract.
    let target = unsafe { crate::ffi::cstr::slice(target) };
    // SAFETY: the caller's C string contract.
    let link = unsafe { crate::ffi::cstr::slice(link) };
    crate::ffi::ret::int(
        crate::ffi::platform::global()
            .symlink(target, link)
            .map(|()| 0)
            .map_err(crate::platform::map_error),
    )
}

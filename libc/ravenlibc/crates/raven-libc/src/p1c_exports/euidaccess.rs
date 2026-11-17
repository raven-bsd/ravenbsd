// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `euidaccess`.

use super::*;

/// Effective-uid access check.
///
/// # Safety
/// `path` is NUL terminated.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn euidaccess(path: *const c_char, mode: c_int) -> c_int {
    if path.is_null() {
        return ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    // SAFETY: the caller's C string contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    unit(raven_libc_platform::compat::euidaccess(
        global(),
        path,
        mode,
    ))
}

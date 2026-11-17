// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `lchmod`.

use super::*;

/// Change a symlink's mode (unsupported on Raven).
///
/// # Safety
/// `path` is NUL terminated.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn lchmod(path: *const c_char, mode: u32) -> c_int {
    if path.is_null() {
        return ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    // SAFETY: the caller's C string contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    unit(raven_libc_platform::compat::lchmod(global(), path, mode))
}

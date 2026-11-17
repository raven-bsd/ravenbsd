// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `pathconf`.

use super::*;

/// Read a path configuration value.
///
/// # Safety
/// `path` is NUL terminated.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn pathconf(path: *const c_char, name: c_int) -> c_long {
    if path.is_null() {
        return ret::long(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    // SAFETY: the caller's C string contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    match global().pathconf(path, name) {
        Ok(value) => c_long::try_from(value).unwrap_or(c_long::MAX),
        Err(error) => ret::long(Err(map_error(error))),
    }
}

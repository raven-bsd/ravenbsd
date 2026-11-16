// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `link(2)`.
use core::ffi::{c_char, c_int};

/// Create a hard link; the filesystem call is not exposed yet (registered PG-31 stub).
///
/// # Safety
/// Both pointers are NULL or NUL-terminated strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn link(_oldpath: *const c_char, _newpath: *const c_char) -> c_int {
    crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
        raven_libc_abi::errno::ENOSYS as u16,
    )))
}

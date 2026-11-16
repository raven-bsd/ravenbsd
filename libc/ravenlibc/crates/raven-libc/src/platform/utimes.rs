// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `utimes`.
use core::ffi::{c_char, c_int, c_void};

/// Set the times of a path; the filesystem `UTIMES` call is not wired yet (PG-31 stub).
///
/// # Safety
/// The pointers are not dereferenced.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn utimes(_path: *const c_char, _times: *const c_void) -> c_int {
    crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
        raven_libc_abi::errno::ENOSYS as u16,
    )))
}

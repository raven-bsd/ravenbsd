// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `futimens(2)`.
use core::ffi::{c_int, c_void};

/// Set the times of an open descriptor; the `UTIMES` call is not wired yet (PG-31 stub).
///
/// # Safety
/// The pointer is not dereferenced.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn futimens(_fd: c_int, _times: *const c_void) -> c_int {
    crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
        raven_libc_abi::errno::ENOSYS as u16,
    )))
}

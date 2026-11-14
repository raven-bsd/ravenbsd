// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `mprotect(2)`.
use core::ffi::{c_int, c_void};

/// Change the protection of `len` bytes at `addr`.
///
/// # Safety
/// The caller's `mprotect` contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mprotect(addr: *mut c_void, len: usize, protection: c_int) -> c_int {
    crate::ffi::ret::platform_int(
        raven_libc_platform::mem::mprotect(
            crate::ffi::platform::global(),
            addr as usize,
            len,
            protection,
        )
        .map(|()| 0),
    )
}

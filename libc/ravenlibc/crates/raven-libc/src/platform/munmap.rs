// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `munmap(2)`.
use core::ffi::{c_int, c_void};

/// Unmap `len` bytes at `addr`.
///
/// # Safety
/// The caller's `munmap` contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn munmap(addr: *mut c_void, len: usize) -> c_int {
    crate::ffi::ret::platform_int(
        raven_libc_platform::mem::munmap(crate::ffi::platform::global(), addr as usize, len)
            .map(|()| 0),
    )
}

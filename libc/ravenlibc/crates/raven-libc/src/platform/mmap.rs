// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `mmap(2)`.
use core::ffi::{c_int, c_void};

/// Map `len` bytes and return the address, or `MAP_FAILED`.
///
/// # Safety
/// The caller's `mmap` contract; the platform validates the range and protection bits.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmap(
    addr: *mut c_void,
    len: usize,
    protection: c_int,
    flags: c_int,
    fd: c_int,
    offset: i64,
) -> *mut c_void {
    let result = raven_libc_platform::mem::mmap(
        crate::ffi::platform::global(),
        addr as usize,
        len,
        protection,
        flags,
        fd,
        offset,
    );
    match result {
        Ok(address) => address as *mut c_void,
        Err(error) => {
            crate::ffi::errno::set_platform(error);
            usize::MAX as *mut c_void
        }
    }
}

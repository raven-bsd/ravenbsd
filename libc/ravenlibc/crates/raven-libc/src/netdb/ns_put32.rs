// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `ns_put32(3)`.
use core::ffi::c_int;

/// Write one big-endian 32-bit word.
///
/// # Safety
/// `cp` points to at least four writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_put32(value: u32, cp: *mut u8) -> c_int {
    if cp.is_null() {
        return 0;
    }
    // SAFETY: the caller's four writable bytes.
    let bytes = unsafe { core::slice::from_raw_parts_mut(cp, 4) };
    c_int::from(raven_libc_resolv::ns_put32(value, bytes))
}

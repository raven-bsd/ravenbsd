// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `ns_get16(3)`.

/// Read one big-endian 16-bit word.
///
/// # Safety
/// `cp` points to at least two readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_get16(cp: *const u8) -> u16 {
    if cp.is_null() {
        return 0;
    }
    // SAFETY: the caller's two readable bytes.
    let bytes = unsafe { core::slice::from_raw_parts(cp, 2) };
    raven_libc_resolv::ns_get16(bytes).unwrap_or(0)
}

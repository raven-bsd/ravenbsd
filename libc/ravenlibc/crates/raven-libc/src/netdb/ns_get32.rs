// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `ns_get32(3)`.

/// Read one big-endian 32-bit word.
///
/// # Safety
/// `cp` points to at least four readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_get32(cp: *const u8) -> u32 {
    if cp.is_null() {
        return 0;
    }
    // SAFETY: the caller's four readable bytes.
    let bytes = unsafe { core::slice::from_raw_parts(cp, 4) };
    raven_libc_resolv::ns_get32(bytes).unwrap_or(0)
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the snvis export (ticket C65, `17` section 13). The logic is
// `raven-libc-compat::vis`; this file owns only the C signature and the errno write.
//! `snvis` C-ABI export (ticket C65).

use core::ffi::c_char;

use raven_libc_compat::vis;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn snvis(
    dst: *mut c_char,
    dlen: usize,
    c: core::ffi::c_int,
    flags: core::ffi::c_int,
    nextc: core::ffi::c_int,
    extra: *const c_char,
) -> *mut c_char {
    if dst.is_null() || dlen == 0 {
        return core::ptr::null_mut();
    }
    // SAFETY: `extra` is a NUL-terminated C string or null.
    let extra = unsafe { super::c_bytes(extra, super::MAX_CSTR) };
    // SAFETY: the caller gives `dlen` writable bytes.
    let buffer = unsafe { super::out_bytes(dst, dlen) };
    match vis::snvis(buffer, c as u8, flags as u32, nextc as u8, extra) {
        Some(length) if length < dlen => {
            // SAFETY: inside the caller's buffer.
            unsafe { *dst.add(length) = 0 };
            // SAFETY: inside the caller's buffer.
            unsafe { dst.add(length) }
        }
        _ => core::ptr::null_mut(),
    }
}

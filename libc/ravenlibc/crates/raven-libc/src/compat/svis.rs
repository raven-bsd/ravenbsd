// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the svis export (ticket C65, `17` section 13). The logic is
// `raven-libc-compat::vis`; this file owns only the C signature and the errno write.
//! `svis` C-ABI export (ticket C65).

use core::ffi::c_char;

use raven_libc_compat::vis;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn svis(
    dst: *mut c_char,
    c: core::ffi::c_int,
    flags: core::ffi::c_int,
    nextc: core::ffi::c_int,
    extra: *const c_char,
) -> *mut c_char {
    if dst.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: `extra` is a NUL-terminated C string or null (bounded by 1 MiB).
    let extra = unsafe { super::c_bytes(extra, super::MAX_CSTR) };
    // SAFETY: `vis` needs at least 8 bytes.
    let buffer = unsafe { super::out_bytes(dst, 8) };
    match vis::svis(buffer, c as u8, flags as u32, nextc as u8, extra) {
        Some(length) => {
            // SAFETY: inside the caller's buffer.
            unsafe { *dst.add(length) = 0 };
            // SAFETY: inside the caller's buffer.
            unsafe { dst.add(length) }
        }
        None => core::ptr::null_mut(),
    }
}

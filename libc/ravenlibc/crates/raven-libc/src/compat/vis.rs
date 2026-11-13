// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the vis export (ticket C65, `17` section 13). The logic is
// `raven-libc-compat::vis`; this file owns only the C signature and the errno write.
//! `vis` C-ABI export (ticket C65).

use core::ffi::c_char;

use raven_libc_compat::vis;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn vis(
    dst: *mut c_char,
    c: core::ffi::c_int,
    flags: core::ffi::c_int,
    nextc: core::ffi::c_int,
) -> *mut c_char {
    if dst.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: the C contract gives `vis` at least 8 writable bytes.
    let buffer = unsafe { super::out_bytes(dst, 8) };
    match vis::vis(buffer, c as u8, flags as u32, nextc as u8) {
        Some(length) => {
            // SAFETY: the caller's buffer has at least `length + 1` bytes.
            unsafe { *dst.add(length) = 0 };
            // SAFETY: the pointer arithmetic stays inside the caller's buffer.
            unsafe { dst.add(length) }
        }
        None => {
            // SAFETY: the errno cell is this thread's.
            unsafe { super::set_errno(super::ENOSPC) };
            core::ptr::null_mut()
        }
    }
}

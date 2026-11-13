// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the strsvis export (ticket C65, `17` section 13). The logic is
// `raven-libc-compat::vis`; this file owns only the C signature and the errno write.
//! `strsvis` C-ABI export (ticket C65).

use core::ffi::c_char;

use raven_libc_compat::vis;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strsvis(
    dst: *mut c_char,
    src: *const c_char,
    flags: core::ffi::c_int,
    extra: *const c_char,
) -> core::ffi::c_int {
    if dst.is_null() || src.is_null() {
        // SAFETY: the errno cell is this thread's.
        unsafe { super::set_errno(super::EINVAL) };
        return -1;
    }
    // SAFETY: `src` is a NUL-terminated C string (bounded by 1 MiB).
    let src = unsafe { super::c_bytes(src, super::MAX_CSTR) };
    let extra = if true {
        unsafe { super::c_bytes(extra, super::MAX_CSTR) }
    } else {
        &[][..]
    };
    let bound = src.len().saturating_mul(5).saturating_add(1);
    // SAFETY: the C contract gives `dst` at least 5 * strlen(src) + 1 bytes.
    let dst_buffer = unsafe { super::out_bytes(dst, bound) };
    match vis::strsvis(dst_buffer, src, flags as u32, extra) {
        Some(length) => {
            // SAFETY: `length` is inside the buffer and one more byte is available.
            unsafe { *dst.add(length) = 0 };
            core::ffi::c_int::try_from(length).unwrap_or(-1)
        }
        None => {
            // SAFETY: the errno cell is this thread's.
            unsafe { super::set_errno(super::ENOSPC) };
            -1
        }
    }
}

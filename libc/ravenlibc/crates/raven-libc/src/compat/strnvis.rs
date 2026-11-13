// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the strnvis export (ticket C65, `17` section 13). The logic is
// `raven-libc-compat::vis`; this file owns only the C signature and the errno write.
//! `strnvis` C-ABI export (ticket C65).

use core::ffi::c_char;

use raven_libc_compat::vis;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strnvis(
    dst: *mut c_char,
    dlen: usize,
    src: *const c_char,
    flags: core::ffi::c_int,
) -> core::ffi::c_int {
    if dst.is_null() || src.is_null() || dlen == 0 {
        // SAFETY: the errno cell is this thread's.
        unsafe { super::set_errno(super::EINVAL) };
        return -1;
    }
    // SAFETY: `src` is a NUL-terminated C string (bounded by 1 MiB).
    let src = unsafe { super::c_bytes(src, super::MAX_CSTR) };
    // SAFETY: the caller gives `dlen` writable bytes.
    let dst_buffer = unsafe { super::out_bytes(dst, dlen) };
    match vis::strnvis(dst_buffer, src, flags as u32) {
        Some(length) if length < dlen => {
            // SAFETY: one more byte is inside the caller's buffer.
            unsafe { *dst.add(length) = 0 };
            core::ffi::c_int::try_from(length).unwrap_or(-1)
        }
        _ => {
            // SAFETY: the errno cell is this thread's.
            unsafe { super::set_errno(super::ENOSPC) };
            -1
        }
    }
}

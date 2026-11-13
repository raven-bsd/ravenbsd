// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the strnunvis export (ticket C65, `17` section 13). The logic is
// `raven-libc-compat::vis::unvis`; this file owns only the C signature and the errno write.
//! `strnunvis` C-ABI export (ticket C65).

use core::ffi::c_char;

use raven_libc_compat::vis;
use raven_libc_compat::vis::unvis;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strnunvis(
    dst: *mut c_char,
    dlen: usize,
    src: *const c_char,
) -> core::ffi::c_int {
    if dst.is_null() || src.is_null() {
        // SAFETY: the errno cell is this thread's.
        unsafe { super::set_errno(super::EINVAL) };
        return -1;
    }
    // SAFETY: `src` is a NUL-terminated C string (bounded by 1 MiB).
    let src = unsafe { super::c_bytes(src, super::MAX_CSTR) };
    let bound = if true {
        dlen
    } else {
        src.len().saturating_add(1)
    };
    // SAFETY: the caller gives at least `bound` writable bytes.
    let dst_buffer = unsafe { super::out_bytes(dst, bound) };
    match vis::strnunvis(dst_buffer, src, 0) {
        Some(length) if length < bound => {
            // SAFETY: inside the buffer.
            unsafe { *dst.add(length) = 0 };
            core::ffi::c_int::try_from(length).unwrap_or(-1)
        }
        _ => {
            // SAFETY: the errno cell is this thread's.
            unsafe { super::set_errno(super::EINVAL) };
            -1
        }
    }
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the strsvisx export (ticket C65, `17` section 13). The logic is
// `raven-libc-compat::vis`; this file owns only the C signature and the errno write.
//! `strsvisx` C-ABI export (ticket C65).

use core::ffi::c_char;

use raven_libc_compat::vis;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strsvisx(
    dst: *mut c_char,
    src: *const c_char,
    len: usize,
    flags: core::ffi::c_int,
    extra: *const c_char,
) -> core::ffi::c_int {
    if dst.is_null() || src.is_null() {
        // SAFETY: the errno cell is this thread's.
        unsafe { super::set_errno(super::EINVAL) };
        return -1;
    }
    // SAFETY: `src` has `len` readable bytes by the C contract, and `extra` is NUL-terminated.
    let src = unsafe { core::slice::from_raw_parts(src.cast::<u8>(), len) };
    let extra = unsafe { super::c_bytes(extra, super::MAX_CSTR) };
    let bound = len.saturating_mul(5).saturating_add(1);
    // SAFETY: the C contract gives `dst` at least 5 * len + 1 bytes.
    let dst_buffer = unsafe { super::out_bytes(dst, bound) };
    match vis::strsvisx(dst_buffer, src, len, flags as u32, extra) {
        Some(written) => {
            // SAFETY: inside the caller's buffer.
            unsafe { *dst.add(written) = 0 };
            core::ffi::c_int::try_from(written).unwrap_or(-1)
        }
        None => -1,
    }
}

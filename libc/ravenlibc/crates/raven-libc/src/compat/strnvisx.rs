// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the strnvisx export (ticket C65, `17` section 13). The logic is
// `raven-libc-compat::vis`; this file owns only the C signature and the errno write.
//! `strnvisx` C-ABI export (ticket C65).

use core::ffi::c_char;

use raven_libc_compat::vis;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strnvisx(
    dst: *mut c_char,
    dlen: usize,
    src: *const c_char,
    len: usize,
    flags: core::ffi::c_int,
) -> core::ffi::c_int {
    if dst.is_null() || src.is_null() || dlen == 0 {
        // SAFETY: the errno cell is this thread's.
        unsafe { super::set_errno(super::EINVAL) };
        return -1;
    }
    // SAFETY: `src` has `len` readable bytes; `dst` has `dlen` writable bytes.
    let src = unsafe { core::slice::from_raw_parts(src.cast::<u8>(), len) };
    let dst_buffer = unsafe { super::out_bytes(dst, dlen) };
    match vis::strnvisx(dst_buffer, src, len, flags as u32) {
        Some(written) if written < dlen => {
            // SAFETY: inside the caller's buffer.
            unsafe { *dst.add(written) = 0 };
            core::ffi::c_int::try_from(written).unwrap_or(-1)
        }
        _ => -1,
    }
}

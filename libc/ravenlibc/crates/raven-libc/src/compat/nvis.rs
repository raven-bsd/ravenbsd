// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the nvis export (ticket C65, `17` section 13). The logic is
// `raven-libc-compat::vis`; this file owns only the C signature and the errno write.
//! `nvis` C-ABI export (ticket C65).

use core::ffi::c_char;

use raven_libc_compat::vis;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nvis(
    dst: *mut c_char,
    dlen: usize,
    c: core::ffi::c_int,
    flags: core::ffi::c_int,
    nextc: core::ffi::c_int,
) -> *mut c_char {
    if dst.is_null() || dlen == 0 {
        // SAFETY: the errno cell is this thread's.
        unsafe { super::set_errno(super::ENOSPC) };
        return core::ptr::null_mut();
    }
    // SAFETY: the caller gives `dlen` writable bytes.
    let buffer = unsafe { super::out_bytes(dst, dlen) };
    match vis::nvis(buffer, c as u8, flags as u32, nextc as u8) {
        Some(length) => {
            if length < dlen {
                // SAFETY: index `length` is inside the caller's buffer.
                unsafe { *dst.add(length) = 0 };
                // SAFETY: inside the buffer.
                unsafe { dst.add(length) }
            } else {
                core::ptr::null_mut()
            }
        }
        None => core::ptr::null_mut(),
    }
}

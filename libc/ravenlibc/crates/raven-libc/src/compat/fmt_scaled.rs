// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the fmt_scaled export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `fmt_scaled` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

use raven_libc_compat::humanize;

/// `fmt_scaled`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fmt_scaled(number: i64, buffer: *mut c_char) -> c_int {
    if buffer.is_null() {
        // SAFETY: the errno cell is this thread's.
        unsafe { super::set_errno(super::EINVAL) };
        return -1;
    }
    let mut scratch = [0u8; humanize::FMT_SCALED_STRSIZE];
    match humanize::fmt_scaled(number, &mut scratch) {
        Ok(()) => {
            let length = scratch
                .iter()
                .position(|&byte| byte == 0)
                .unwrap_or(scratch.len());
            // SAFETY: the C contract gives `buffer` FMT_SCALED_STRSIZE bytes.
            unsafe { super::write_cstr(buffer, &scratch[..length]) };
            0
        }
        Err(error) => {
            // SAFETY: the errno cell is this thread's.
            unsafe { super::set_errno(c_int::from(error.0)) };
            -1
        }
    }
}

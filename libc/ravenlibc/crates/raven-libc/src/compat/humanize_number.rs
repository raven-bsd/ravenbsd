// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the humanize_number export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `humanize_number` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

use raven_libc_compat::humanize;

/// `humanize_number`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn humanize_number(
    buffer: *mut c_char,
    length: usize,
    quotient: i64,
    suffix: *const c_char,
    scale: c_int,
    flags: c_int,
) -> c_int {
    if buffer.is_null() || suffix.is_null() {
        return -1;
    }
    // SAFETY: `suffix` is a NUL-terminated C string.
    let suffix = unsafe { super::c_bytes(suffix, super::MAX_CSTR) };
    if length == 0 {
        return -1;
    }
    // SAFETY: the caller gives `length` writable bytes.
    let destination = unsafe { super::out_bytes(buffer, length) };
    let result = humanize::humanize_number(destination, quotient, suffix, scale, flags);
    if result >= 0 {
        let written = usize::try_from(result).unwrap_or(0);
        if written < length {
            // SAFETY: index `written` is inside the buffer.
            unsafe { *buffer.add(written) = 0 };
        } else if length > 0 {
            // SAFETY: the last byte is inside the buffer.
            unsafe { *buffer.add(length - 1) = 0 };
        }
    }
    result
}

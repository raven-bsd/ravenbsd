// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getw` and `putw`: one `int` in binary form.

use core::ffi::{c_int, c_void};

use raven_libc_stdio::EOF;

/// `getw`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getw(stream: *mut c_void) -> c_int {
    let mut value: c_int = 0;
    // SAFETY: `value` is one writable `int`; the stream is the caller's FILE.
    let read = unsafe {
        super::fread_impl(
            (&raw mut value).cast(),
            core::mem::size_of::<c_int>(),
            1,
            stream,
        )
    };
    if read == 1 { value } else { EOF }
}

/// `putw`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn putw(value: c_int, stream: *mut c_void) -> c_int {
    // SAFETY: `value` is one readable `int`; the stream is the caller's FILE.
    let written = unsafe {
        super::fwrite_impl(
            (&raw const value).cast(),
            core::mem::size_of::<c_int>(),
            1,
            stream,
        )
    };
    if written == 1 { 0 } else { EOF }
}

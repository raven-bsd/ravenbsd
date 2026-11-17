// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `setbuf`, `setbuffer` and `setlinebuf`.

use core::ffi::{c_char, c_int, c_void};

const IOFBF: c_int = 0;
const IOLBF: c_int = 1;
const IONBF: c_int = 2;
const BUFSIZ: usize = 4096;

/// `setbuf`.
///
/// # Safety
/// `stream` is a FILE; `buf` is NULL or writable for `BUFSIZ` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn setbuf(stream: *mut c_void, buf: *mut c_char) {
    // SAFETY: forwarded C contract.
    let _ = unsafe {
        if buf.is_null() {
            super::setvbuf_impl(stream, buf, IONBF, 0)
        } else {
            super::setvbuf_impl(stream, buf, IOFBF, BUFSIZ)
        }
    };
}

/// `setbuffer`.
///
/// # Safety
/// `stream` is a FILE; `buf` is NULL or writable for `size` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn setbuffer(stream: *mut c_void, buf: *mut c_char, size: usize) {
    // SAFETY: forwarded C contract.
    let _ = unsafe { super::setvbuf_impl(stream, buf, IOFBF, size) };
}

/// `setlinebuf`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn setlinebuf(stream: *mut c_void) {
    // SAFETY: forwarded C contract.
    let _ = unsafe { super::setvbuf_impl(stream, core::ptr::null_mut(), IOLBF, 0) };
}

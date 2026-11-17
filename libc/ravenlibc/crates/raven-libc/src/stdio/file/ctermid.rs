// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `ctermid`: the controlling terminal's name.

use core::ffi::c_char;

static TTY: [u8; 9] = *b"/dev/tty\0";

/// `ctermid`.
///
/// # Safety
/// `s` is NULL or writable for `L_ctermid` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ctermid(s: *mut c_char) -> *mut c_char {
    if s.is_null() {
        return TTY.as_ptr().cast_mut().cast();
    }
    // SAFETY: the caller grants `L_ctermid` writable bytes; the source is 9 bytes.
    unsafe { core::ptr::copy_nonoverlapping(TTY.as_ptr(), s.cast(), TTY.len()) };
    s
}

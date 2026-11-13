// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `strsignal(3)`.
use core::ffi::{c_char, c_int};

static mut MESSAGE: [u8; 64] = [0; 64];

/// The text for signal `s`.
///
/// # Safety
/// The returned pointer is valid until the next `strsignal` call in this thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strsignal(s: c_int) -> *mut c_char {
    // SAFETY: the buffer is process-static; the TCB slot arrives with C57 (RD-14).
    let buffer = unsafe { core::slice::from_raw_parts_mut((&raw mut MESSAGE).cast::<u8>(), 64) };
    // SAFETY: the static buffer has 64 writable bytes.
    let _ = unsafe {
        crate::ffi::out::write_cstr(
            buffer.as_mut_ptr().cast(),
            raven_libc_str::errmsg::strsignal::signal_message(s).unwrap_or(b"Unknown signal"),
            buffer.len(),
        )
    };
    buffer.as_mut_ptr().cast()
}

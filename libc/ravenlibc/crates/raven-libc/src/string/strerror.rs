// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `strerror(3)`, `strerror_r(3)` (XSI) and `__xpg_strerror_r`'s strict sibling.
use core::ffi::{c_char, c_int};

/// The process-wide 64-byte message buffer (per-thread only after C57).
static mut MESSAGE: [u8; 64] = [0; 64];

/// The text for `e`.
///
/// # Safety
/// The returned pointer is valid until the next `strerror` call in this thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strerror(e: c_int) -> *mut c_char {
    // SAFETY: the buffer is process-static and `strerror` is not re-entrant by contract; C57
    // replaces this slot with the TCB one (RD-14). The slice is the 64-byte buffer itself.
    let buffer = unsafe { core::slice::from_raw_parts_mut((&raw mut MESSAGE).cast::<u8>(), 64) };
    match raven_libc_str::errmsg::strerror::message(e) {
        Some(text) => {
            // SAFETY: the static buffer has 64 writable bytes.
            let copied = unsafe {
                crate::ffi::out::write_cstr(buffer.as_mut_ptr().cast(), text, buffer.len())
            };
            let _ = copied;
            buffer.as_mut_ptr().cast()
        }
        None => buffer.as_mut_ptr().cast(),
    }
}

/// The XSI `strerror_r`: `int` return, never writes `errno`.
///
/// # Safety
/// `buf` has `n` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strerror_r(e: c_int, buf: *mut c_char, n: usize) -> c_int {
    // SAFETY: the caller's contract is the strerror_r(3) contract.
    unsafe { raven_libc_str::raw::strerror_r(e, buf.cast(), n) }
}

/// The GNU `strerror_r` form: returns `buf`.
///
/// # Safety
/// `buf` has `n` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __xpg_strerror_r(e: c_int, buf: *mut c_char, n: usize) -> *mut c_char {
    // SAFETY: the caller's contract is the GNU strerror_r contract.
    unsafe { raven_libc_str::raw::gnu_strerror_r(e, buf.cast(), n).cast() }
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `strtok(3)` and `strtok_r(3)`.
use core::ffi::c_char;

/// The next token; the cursor is the caller's `last` slot.
///
/// # Safety
/// `last` is a writable pointer slot; `s` is NULL or a NUL-terminated string; `delim` is a
/// readable NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strtok_r(
    s: *mut c_char,
    delim: *const c_char,
    last: *mut *mut c_char,
) -> *mut c_char {
    // SAFETY: the caller's contract is the strtok_r(3) contract.
    unsafe { raven_libc_str::raw::strtok_r(s.cast(), delim.cast(), last.cast::<*mut u8>()).cast() }
}

/// The per-thread-cursor form; the shell owns the cursor slot until C57 provides the TCB one.
///
/// # Safety
/// `s` is NULL or a NUL-terminated string; `delim` is a readable NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strtok(s: *mut c_char, delim: *const c_char) -> *mut c_char {
    static mut CURSOR: *mut u8 = core::ptr::null_mut();
    // SAFETY: the cursor is per-process until the TCB slot exists; this branch is single-threaded
    // before C57 (`rd-14` tracks the per-thread slot).
    unsafe {
        raven_libc_str::raw::strtok(&raw mut CURSOR, s.cast::<u8>(), delim.cast::<u8>()).cast()
    }
}

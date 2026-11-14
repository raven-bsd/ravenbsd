// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getsubopt(3)`.
use alloc::vec::Vec;
use core::ffi::{c_char, c_int};

/// Parse one comma-separated sub-option.
///
/// # Safety
/// `optionp` is writable and points to a mutable C string; `tokens` is a NULL-terminated array.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getsubopt(
    optionp: *mut *mut c_char,
    tokens: *const *mut c_char,
    valuep: *mut *mut c_char,
) -> c_int {
    // SAFETY: the caller's optionp slot is readable and `optionp` is non-NULL.
    if optionp.is_null() || valuep.is_null() || unsafe { (*optionp).is_null() } {
        return -1;
    }
    // SAFETY: the caller's option string.
    let base = unsafe { *optionp };
    let length = unsafe { crate::ffi::cstr::slice(base) }.len();
    // SAFETY: the C string has `length` mutable bytes before its NUL.
    let buffer = unsafe { core::slice::from_raw_parts_mut(base.cast::<u8>(), length) };
    let names = unsafe { collect_tokens(tokens) };
    let parsed = raven_libc_stdlib::misc::getsubopt(0, buffer, &names);
    let value = match parsed.value {
        Some((start, _)) => base.wrapping_add(start),
        None => core::ptr::null_mut(),
    };
    let next = base.wrapping_add(parsed.next);
    // SAFETY: the caller's writable slots and the global `suboptarg`.
    unsafe {
        *valuep = value;
        *optionp = next;
        crate::stdlib::suboptarg::suboptarg = match parsed.token {
            Some((start, _)) => base.wrapping_add(start),
            None => core::ptr::null_mut(),
        };
    }
    match parsed.index {
        Some(index) => c_int::try_from(index).unwrap_or(-1),
        None => -1,
    }
}

/// Read the NULL-terminated token name array.
///
/// # Safety
/// `tokens` is NULL or a NULL-terminated array of C strings.
unsafe fn collect_tokens(tokens: *const *mut c_char) -> Vec<&'static [u8]> {
    let mut names = Vec::new();
    if tokens.is_null() {
        return names;
    }
    let mut index = 0usize;
    while index < 4096 {
        // SAFETY: the caller's NULL-terminated token array.
        let token = unsafe { *tokens.add(index) };
        if token.is_null() {
            break;
        }
        // SAFETY: each token is a NUL-terminated string.
        names.push(unsafe { crate::ffi::cstr::slice(token) });
        index = index.wrapping_add(1);
    }
    names
}

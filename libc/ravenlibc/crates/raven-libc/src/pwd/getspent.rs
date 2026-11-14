// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getspent`, `getspnam` and `getspnam_r`.

use core::ffi::{c_char, c_int};

use raven_libc_abi::errno::{EFAULT, EINVAL};

use super::shadow;

/// `getspent(3)`.
#[unsafe(no_mangle)]
pub extern "C" fn getspent() -> *mut shadow::Spwd {
    match shadow::next() {
        Some(entry) => shadow::fill_static(&entry),
        None => core::ptr::null_mut(),
    }
}

/// `getspnam(3)`.
///
/// # Safety
/// `name` is a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getspnam(name: *const c_char) -> *mut shadow::Spwd {
    // SAFETY: the caller's C string.
    let name = unsafe { crate::ffi::cstr::slice(name) };
    match shadow::by_name(name) {
        Some(entry) => shadow::fill_static(&entry),
        None => core::ptr::null_mut(),
    }
}

/// `getspnam_r(3)`.
///
/// # Safety
/// The pointers follow the C record/buffer/output contracts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getspnam_r(
    name: *const c_char,
    record: *mut shadow::Spwd,
    buf: *mut c_char,
    buflen: usize,
    result: *mut *mut shadow::Spwd,
) -> c_int {
    if result.is_null() {
        return EINVAL;
    }
    // SAFETY: the caller's output slot.
    unsafe { result.write(core::ptr::null_mut()) };
    if name.is_null() || record.is_null() || (buf.is_null() && buflen != 0) {
        return EFAULT;
    }
    // SAFETY: the caller's name and buffer.
    let name = unsafe { crate::ffi::cstr::slice(name) };
    let buffer = unsafe { crate::ffi::cast::bytes_mut(buf.cast(), buflen) };
    match shadow::by_name(name) {
        None => 0,
        Some(entry) => match shadow::fill(&entry, record, buffer) {
            Ok(()) => {
                // SAFETY: the caller's output slot.
                unsafe { result.write(record) };
                0
            }
            Err(error) => i32::from(error),
        },
    }
}

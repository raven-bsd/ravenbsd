// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getpwuid_r(3)`.
use core::ffi::{c_char, c_int};

use raven_libc_abi::errno::{EFAULT, EINVAL};
use raven_libc_abi::structs::Passwd;

/// Look up one user id into the caller's record and buffer.
///
/// # Safety
/// Every pointer follows its C contract; `result` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getpwuid_r(
    uid: u32,
    record: *mut Passwd,
    buf: *mut c_char,
    buflen: usize,
    result: *mut *mut Passwd,
) -> c_int {
    if result.is_null() {
        return EINVAL;
    }
    // SAFETY: the caller's writable result slot.
    unsafe { result.write(core::ptr::null_mut()) };
    if record.is_null() || (buf.is_null() && buflen != 0) {
        return EFAULT;
    }
    // SAFETY: the caller's buffer contract.
    let buffer = unsafe { crate::ffi::cast::bytes_mut(buf.cast(), buflen) };
    match crate::pwd::lookup::passwd_by_uid(uid) {
        Ok(None) => 0,
        Err(error) => error,
        Ok(Some(entry)) => match crate::ffi::tcbbuf::fill_passwd(&entry, record, buffer) {
            Ok(()) => {
                // SAFETY: the caller's writable result slot.
                unsafe { result.write(record) };
                0
            }
            Err(error) => i32::from(error.0),
        },
    }
}

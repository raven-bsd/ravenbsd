// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getprotobyname*` and `getprotobynumber*`.

use core::ffi::{c_char, c_int};

use raven_libc_abi::errno::{EFAULT, EINVAL};

use super::records::{self, Protoent, pack_proto};

/// `getprotobyname(3)`.
///
/// # Safety
/// `name` is a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getprotobyname(name: *const c_char) -> *mut Protoent {
    // SAFETY: the caller's C string.
    let name = unsafe { crate::ffi::cstr::slice(name) };
    records::proto_by_name(name)
        .map(|entry| records::fill_proto(&entry))
        .unwrap_or(core::ptr::null_mut())
}

/// `getprotobyname_r(3)`.
///
/// # Safety
/// The pointers follow the C record/buffer/output contracts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getprotobyname_r(
    name: *const c_char,
    record: *mut Protoent,
    buf: *mut c_char,
    buflen: usize,
    result: *mut *mut Protoent,
) -> c_int {
    if result.is_null() {
        return EINVAL;
    }
    // SAFETY: the caller's output slot.
    unsafe { result.write(core::ptr::null_mut()) };
    if name.is_null() || record.is_null() || (buf.is_null() && buflen != 0) {
        return EFAULT;
    }
    // SAFETY: the caller's string and buffer.
    let name = unsafe { crate::ffi::cstr::slice(name) };
    let buffer = unsafe { crate::ffi::cast::bytes_mut(buf.cast(), buflen) };
    match records::proto_by_name(name) {
        None => 0,
        Some(entry) => match pack_proto(&entry, unsafe { &mut *record }, buffer) {
            Ok(()) => {
                // SAFETY: the caller's output slot.
                unsafe { result.write(record) };
                0
            }
            Err(error) => i32::from(error.0),
        },
    }
}

/// `getprotobynumber(3)`.
#[unsafe(no_mangle)]
pub extern "C" fn getprotobynumber(number: c_int) -> *mut Protoent {
    let Ok(number) = u8::try_from(number) else {
        return core::ptr::null_mut();
    };
    records::proto_by_number(number)
        .map(|entry| records::fill_proto(&entry))
        .unwrap_or(core::ptr::null_mut())
}

/// `getprotobynumber_r(3)`.
///
/// # Safety
/// The pointers follow the C record/buffer/output contracts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getprotobynumber_r(
    number: c_int,
    record: *mut Protoent,
    buf: *mut c_char,
    buflen: usize,
    result: *mut *mut Protoent,
) -> c_int {
    if result.is_null() {
        return EINVAL;
    }
    // SAFETY: the caller's output slot.
    unsafe { result.write(core::ptr::null_mut()) };
    if record.is_null() || (buf.is_null() && buflen != 0) {
        return EFAULT;
    }
    let Ok(number) = u8::try_from(number) else {
        return 0;
    };
    // SAFETY: the caller's buffer.
    let buffer = unsafe { crate::ffi::cast::bytes_mut(buf.cast(), buflen) };
    match records::proto_by_number(number) {
        None => 0,
        Some(entry) => match pack_proto(&entry, unsafe { &mut *record }, buffer) {
            Ok(()) => {
                // SAFETY: the caller's output slot.
                unsafe { result.write(record) };
                0
            }
            Err(error) => i32::from(error.0),
        },
    }
}

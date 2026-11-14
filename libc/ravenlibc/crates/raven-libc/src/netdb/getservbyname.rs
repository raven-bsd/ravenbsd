// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getservbyname*` and `getservbyport*`.

use core::ffi::{c_char, c_int};

use raven_libc_abi::errno::{EFAULT, EINVAL};

use super::records::{self, Servent, pack_serv};

fn optional(bytes: *const c_char) -> Option<&'static [u8]> {
    if bytes.is_null() {
        None
    } else {
        // SAFETY: the caller's optional C string.
        Some(unsafe { crate::ffi::cstr::slice(bytes) })
    }
}

/// `getservbyname(3)`.
///
/// # Safety
/// `name` is a NUL-terminated string; `proto` is NULL or one.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getservbyname(name: *const c_char, proto: *const c_char) -> *mut Servent {
    // SAFETY: the caller's C strings.
    let name = unsafe { crate::ffi::cstr::slice(name) };
    records::serv_by_name(name, optional(proto))
        .map(|entry| records::fill_serv(&entry))
        .unwrap_or(core::ptr::null_mut())
}

/// `getservbyname_r(3)`.
///
/// # Safety
/// The pointers follow the C record/buffer/output contracts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getservbyname_r(
    name: *const c_char,
    proto: *const c_char,
    record: *mut Servent,
    buf: *mut c_char,
    buflen: usize,
    result: *mut *mut Servent,
) -> c_int {
    if result.is_null() {
        return EINVAL;
    }
    // SAFETY: the caller's output slot.
    unsafe { result.write(core::ptr::null_mut()) };
    if name.is_null() || record.is_null() || (buf.is_null() && buflen != 0) {
        return EFAULT;
    }
    // SAFETY: the caller's strings and buffer.
    let name = unsafe { crate::ffi::cstr::slice(name) };
    let buffer = unsafe { crate::ffi::cast::bytes_mut(buf.cast(), buflen) };
    match records::serv_by_name(name, optional(proto)) {
        None => 0,
        Some(entry) => match pack_serv(&entry, unsafe { &mut *record }, buffer) {
            Ok(()) => {
                // SAFETY: the caller's output slot.
                unsafe { result.write(record) };
                0
            }
            Err(error) => i32::from(error.0),
        },
    }
}

/// `getservbyport(3)`.
///
/// # Safety
/// `proto` is NULL or a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getservbyport(port: c_int, proto: *const c_char) -> *mut Servent {
    let port = u16::from_be(port as u16);
    records::serv_by_port(port, optional(proto))
        .map(|entry| records::fill_serv(&entry))
        .unwrap_or(core::ptr::null_mut())
}

/// `getservbyport_r(3)`.
///
/// # Safety
/// The pointers follow the C record/buffer/output contracts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getservbyport_r(
    port: c_int,
    proto: *const c_char,
    record: *mut Servent,
    buf: *mut c_char,
    buflen: usize,
    result: *mut *mut Servent,
) -> c_int {
    if result.is_null() {
        return EINVAL;
    }
    // SAFETY: the caller's output slot.
    unsafe { result.write(core::ptr::null_mut()) };
    if record.is_null() || (buf.is_null() && buflen != 0) {
        return EFAULT;
    }
    let port = u16::from_be(port as u16);
    // SAFETY: the caller's buffer.
    let buffer = unsafe { crate::ffi::cast::bytes_mut(buf.cast(), buflen) };
    match records::serv_by_port(port, optional(proto)) {
        None => 0,
        Some(entry) => match pack_serv(&entry, unsafe { &mut *record }, buffer) {
            Ok(()) => {
                // SAFETY: the caller's output slot.
                unsafe { result.write(record) };
                0
            }
            Err(error) => i32::from(error.0),
        },
    }
}

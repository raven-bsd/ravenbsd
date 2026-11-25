// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Legacy host lookups: `gethostbyname*` and their `_r` forms.

use core::ffi::{c_char, c_int};

use raven_libc_abi::errno::{EFAULT, EINVAL};
use raven_libc_abi::socket::AF_INET;

use super::records::{self, Hostent, pack_host_r};

/// `gethostbyname(3)`.
///
/// # Safety
/// `name` is a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gethostbyname(name: *const c_char) -> *mut Hostent {
    // SAFETY: the caller's C string.
    let name = unsafe { crate::ffi::cstr::slice(name) };
    records::host_by_name(name, AF_INET)
        .map(|entry| records::fill_host(&entry))
        .unwrap_or(core::ptr::null_mut())
}

/// `gethostbyname2(3)`.
///
/// # Safety
/// `name` is a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gethostbyname2(name: *const c_char, af: c_int) -> *mut Hostent {
    // SAFETY: the caller's C string.
    let name = unsafe { crate::ffi::cstr::slice(name) };
    records::host_by_name(name, af)
        .map(|entry| records::fill_host(&entry))
        .unwrap_or(core::ptr::null_mut())
}

unsafe fn lookup_r(
    name: *const c_char,
    af: c_int,
    record: *mut Hostent,
    buf: *mut c_char,
    buflen: usize,
    result: *mut *mut Hostent,
    herror: *mut c_int,
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
    match records::host_by_name(name, af) {
        None => {
            if !herror.is_null() {
                // SAFETY: the caller's h_errno output.
                unsafe { herror.write(1) };
            }
            0
        }
        Some(entry) => match pack_host_r(&entry, unsafe { &mut *record }, buffer) {
            Ok(()) => {
                // SAFETY: the caller's output slot.
                unsafe { result.write(record) };
                0
            }
            Err(error) => i32::from(error.0),
        },
    }
}

/// `gethostbyname_r(3)`.
///
/// # Safety
/// The pointers follow the C record/buffer/output contracts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gethostbyname_r(
    name: *const c_char,
    record: *mut Hostent,
    buf: *mut c_char,
    buflen: usize,
    result: *mut *mut Hostent,
    herror: *mut c_int,
) -> c_int {
    unsafe { lookup_r(name, 2, record, buf, buflen, result, herror) } // SAFETY: the caller follows the gethostbyname_r record/buffer/output contract; lookup_r only writes within buflen.
}

/// `gethostbyname2_r(3)`.
///
/// # Safety
/// As [`gethostbyname_r`], plus `af`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gethostbyname2_r(
    name: *const c_char,
    af: c_int,
    record: *mut Hostent,
    buf: *mut c_char,
    buflen: usize,
    result: *mut *mut Hostent,
    herror: *mut c_int,
) -> c_int {
    unsafe { lookup_r(name, af, record, buf, buflen, result, herror) } // SAFETY: the caller follows the gethostbyname2_r record/buffer/output contract; lookup_r only writes within buflen.
}

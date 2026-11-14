// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `gethostbyaddr*`, `gethostent` and `sethostent`.

use core::ffi::{c_char, c_int, c_void};

use raven_libc_abi::errno::{EFAULT, EINVAL};

use super::records::{self, Hostent, pack_host_r};

/// `gethostbyaddr(3)`.
///
/// # Safety
/// `addr` points to `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gethostbyaddr(addr: *const c_void, len: u32, af: c_int) -> *mut Hostent {
    // SAFETY: the caller's address buffer.
    let bytes = match unsafe { crate::ffi::sockaddr::view(addr, len) } {
        Ok(bytes) => bytes,
        Err(_) => return core::ptr::null_mut(),
    };
    records::host_by_addr(bytes, af)
        .map(|entry| records::fill_host(&entry))
        .unwrap_or(core::ptr::null_mut())
}

/// `gethostbyaddr_r(3)`.
///
/// # Safety
/// The pointers follow the C record/buffer/output contracts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gethostbyaddr_r(
    addr: *const c_void,
    len: u32,
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
    if addr.is_null() || record.is_null() || (buf.is_null() && buflen != 0) {
        return EFAULT;
    }
    // SAFETY: the caller's address and buffer.
    let bytes = unsafe { crate::ffi::sockaddr::view(addr, len) }.unwrap_or(&[]);
    let buffer = unsafe { crate::ffi::cast::bytes_mut(buf.cast(), buflen) };
    match records::host_by_addr(bytes, af) {
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

/// `gethostent(3)`.
#[unsafe(no_mangle)]
pub extern "C" fn gethostent() -> *mut Hostent {
    records::next_host()
        .map(|entry| records::fill_host(&entry))
        .unwrap_or(core::ptr::null_mut())
}

/// `sethostent(3)`.
#[unsafe(no_mangle)]
pub extern "C" fn sethostent(_stayopen: c_int) {
    records::set_hosts();
}

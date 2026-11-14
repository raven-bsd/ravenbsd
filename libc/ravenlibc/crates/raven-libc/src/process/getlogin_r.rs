// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getlogin_r(3)`.
use core::ffi::{c_char, c_int};

use raven_libc_abi::errno::{ENXIO, ERANGE};

/// Copy the login name into the caller's buffer.
///
/// # Safety
/// `buffer` points to `size` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getlogin_r(buffer: *mut c_char, size: usize) -> c_int {
    let name = match crate::ffi::map_proc::login(&crate::ffi::procbackend::backend()) {
        Ok(name) => name,
        Err(_) => return ENXIO,
    };
    if buffer.is_null() || name.len().saturating_add(1) > size {
        return ERANGE;
    }
    // SAFETY: the caller's writable buffer of `size` bytes.
    let out = unsafe { crate::ffi::cast::bytes_mut(buffer.cast(), size) };
    let copy = match out.get_mut(..name.len()) {
        Some(copy) => copy,
        None => return ERANGE,
    };
    copy.copy_from_slice(&name);
    if let Some(last) = out.get_mut(name.len()) {
        *last = 0;
    }
    0
}

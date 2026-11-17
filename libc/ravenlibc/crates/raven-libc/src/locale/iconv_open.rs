// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `iconv_open` from `<iconv.h>`.

use alloc::boxed::Box;
use core::ffi::{c_char, c_void};

use raven_libc_abi::errno::Errno;

/// `iconv_open`.
///
/// # Safety
/// Both strings are NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iconv_open(to: *const c_char, from: *const c_char) -> *mut c_void {
    if to.is_null() || from.is_null() {
        return crate::ffi::ret::null(Errno(22));
    }
    // SAFETY: the caller's C string contract.
    let (to, from) = unsafe { (crate::ffi::cstr::slice(to), crate::ffi::cstr::slice(from)) };
    match raven_libc_iconv::iconv_open(to, from) {
        Ok(conversion) => Box::into_raw(Box::new(conversion)).cast::<c_void>(),
        Err(_) => crate::ffi::ret::null(Errno(22)),
    }
}

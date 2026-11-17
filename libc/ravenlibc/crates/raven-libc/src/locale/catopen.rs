// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `catopen` from `<nl_types.h>`.

use alloc::boxed::Box;
use core::ffi::{c_char, c_int, c_void};

use raven_libc_abi::errno::Errno;
use raven_libc_locale::nls::Catalog;

use super::catalog::CATD_FAILED;

/// `catopen`.
///
/// # Safety
/// `name` is NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn catopen(name: *const c_char, oflag: c_int) -> *mut c_void {
    if name.is_null() {
        crate::ffi::errno::set(Errno(22));
        return CATD_FAILED as *mut c_void;
    }
    // SAFETY: the caller's C string contract.
    let name = unsafe { crate::ffi::cstr::slice(name) };
    let locale = super::_locale_name();
    match Catalog::open(
        &super::ShellFiles,
        &super::EmptyEnv,
        name,
        oflag,
        false,
        &locale,
    ) {
        Ok(catalog) => Box::into_raw(Box::new(catalog)).cast::<c_void>(),
        Err(_) => {
            crate::ffi::errno::set(Errno(2));
            CATD_FAILED as *mut c_void
        }
    }
}

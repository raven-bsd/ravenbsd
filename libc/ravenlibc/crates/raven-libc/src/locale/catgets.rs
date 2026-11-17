// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `catgets` from `<nl_types.h>`.

use core::ffi::{c_char, c_int, c_void};

use raven_libc_locale::nls::Catalog;

use super::catalog::{CATD_FAILED, c_string};

/// `catgets`.
///
/// # Safety
/// `catd` is an open catalog; `default` is NULL or NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn catgets(
    catd: *mut c_void,
    set: c_int,
    msg: c_int,
    default: *const c_char,
) -> *mut c_char {
    let fallback = if default.is_null() {
        c_string(b"")
    } else {
        default.cast_mut()
    };
    if catd.is_null() || catd as usize == CATD_FAILED {
        return fallback;
    }
    // SAFETY: `catd` came from `catopen`.
    let catalog = unsafe { &*(catd.cast::<Catalog>()) };
    // SAFETY: the caller's default string contract.
    let bytes = unsafe { crate::ffi::cstr::slice(default) };
    let value = catalog.gets(set, msg, bytes);
    if value.as_ptr() == bytes.as_ptr() {
        return fallback;
    }
    c_string(value)
}

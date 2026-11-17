// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `<nl_types.h>` exports over C48's XSI catalog reader.

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::ffi::{c_char, c_int, c_void};

use raven_libc_abi::errno::Errno;
use raven_libc_locale::nls::Catalog;

/// `(nl_catd)-1`, the failure sentinel of `catopen`.
const CATD_FAILED: usize = usize::MAX;

struct ReturnBuffer(UnsafeCell<Vec<u8>>);

// SAFETY: the shell is single-threaded until C57; the buffer is written for one call at a time.
unsafe impl Sync for ReturnBuffer {}

static RETURN: ReturnBuffer = ReturnBuffer(UnsafeCell::new(Vec::new()));

/// Copy a catalog value into the one C return buffer and terminate it.
fn c_string(bytes: &[u8]) -> *mut c_char {
    // SAFETY: single-threaded access; the buffer is only written here.
    let buffer = unsafe { &mut *RETURN.0.get() };
    buffer.clear();
    buffer.extend_from_slice(bytes);
    buffer.push(0);
    buffer.as_mut_ptr().cast::<c_char>()
}

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

/// `catclose`.
///
/// # Safety
/// `catd` is an open catalog or the failure sentinel.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn catclose(catd: *mut c_void) -> c_int {
    if catd.is_null() || catd as usize == CATD_FAILED {
        crate::ffi::errno::set(Errno(9));
        return -1;
    }
    // SAFETY: `catd` is a Box from `catopen`.
    drop(unsafe { Box::from_raw(catd.cast::<Catalog>()) });
    0
}

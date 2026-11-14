// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `dlvsym(3)`.
use core::ffi::{c_char, c_void};

use super::table;

/// Look up a versioned symbol through the loader table.
///
/// # Safety
/// `name` and `version` are NUL-terminated strings; `handle` is live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dlvsym(
    handle: *mut c_void,
    name: *const c_char,
    version: *const c_char,
) -> *mut c_void {
    let table = table::get();
    if table.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: the table was installed by the loader; the caller supplies the strings.
    unsafe { ((*table).dlvsym)(handle.cast(), name.cast(), version.cast(), 0).cast() }
}

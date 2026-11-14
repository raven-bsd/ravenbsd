// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `dlsym(3)`.
use core::ffi::{c_char, c_void};

use super::table;

/// Look up a symbol through the loader table.
///
/// # Safety
/// `name` is a NUL-terminated string; `handle` is a live handle or a special handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void {
    let table = table::get();
    if table.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: the table was installed by the loader; the caller supplies the string.
    unsafe { ((*table).dlsym)(handle.cast(), name.cast(), 0).cast() }
}

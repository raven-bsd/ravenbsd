// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `dlopen(3)`.
use core::ffi::{c_char, c_int, c_void};

use super::table;

/// Open a shared object through the loader table.
///
/// # Safety
/// `path` is NULL or a NUL-terminated string; the loader owns the returned handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dlopen(path: *const c_char, flags: c_int) -> *mut c_void {
    let table = table::get();
    if table.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: the table was installed by the loader with a valid version/size.
    unsafe { ((*table).dlopen)(path.cast(), flags).cast() }
}

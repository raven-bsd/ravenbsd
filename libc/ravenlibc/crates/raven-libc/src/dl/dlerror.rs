// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `dlerror(3)`.
use core::ffi::c_char;

use super::table;

const STATIC_ERROR: &[u8] = b"Dynamic loading not supported in static executables\0";

/// Return the last loader error, or the static-only message.
#[unsafe(no_mangle)]
pub extern "C" fn dlerror() -> *const c_char {
    let table = table::get();
    if table.is_null() {
        return STATIC_ERROR.as_ptr().cast();
    }
    // SAFETY: the table was installed by the loader.
    unsafe { ((*table).dlerror)() }.cast()
}

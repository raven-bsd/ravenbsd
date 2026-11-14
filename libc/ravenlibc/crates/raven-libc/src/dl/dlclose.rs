// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `dlclose(3)`.
use core::ffi::{c_int, c_void};

use super::table;

/// Close a handle through the loader table.
///
/// # Safety
/// `handle` is a live handle returned by `dlopen`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dlclose(handle: *mut c_void) -> c_int {
    let table = table::get();
    if table.is_null() {
        return -1;
    }
    // SAFETY: the table was installed by the loader.
    unsafe { ((*table).dlclose)(handle.cast()) }
}

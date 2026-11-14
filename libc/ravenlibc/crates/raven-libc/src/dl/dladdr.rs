// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `dladdr(3)`.
use core::ffi::{c_int, c_void};

use super::table;

/// Describe the object containing `addr` through the loader table.
///
/// # Safety
/// `info` points to a writable `Dl_info`; the loader writes it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dladdr(addr: *const c_void, info: *mut c_void) -> c_int {
    let table = table::get();
    if table.is_null() {
        return 0;
    }
    // SAFETY: the table was installed by the loader; the caller owns `info`.
    unsafe { ((*table).dladdr)(addr as usize, info.cast()) }
}

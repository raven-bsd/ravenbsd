// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `dl_iterate_phdr(3)`.
use core::ffi::c_int;

use super::table;

/// Walk the loaded objects through the loader table.
///
/// # Safety
/// `callback` follows `dl_iterate_phdr`'s contract; `data` is passed through.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dl_iterate_phdr(
    callback: Option<extern "C" fn(*mut u8, usize, *mut u8) -> c_int>,
    data: *mut u8,
) -> c_int {
    let table = table::get();
    if table.is_null() {
        return 0;
    }
    // SAFETY: the table was installed by the loader.
    unsafe { ((*table).dl_iterate_phdr)(callback, data) }
}

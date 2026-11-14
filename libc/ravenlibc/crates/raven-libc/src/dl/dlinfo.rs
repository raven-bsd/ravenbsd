// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `dlinfo(3)`.
use core::ffi::{c_int, c_void};

use super::table;

/// Query loader state through the loader table.
///
/// # Safety
/// `handle` is live and `arg` follows the request's contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dlinfo(handle: *mut c_void, request: c_int, arg: *mut c_void) -> c_int {
    let table = table::get();
    if table.is_null() {
        return -1;
    }
    // SAFETY: the table was installed by the loader; the caller supplies the request buffer.
    unsafe { ((*table).dlinfo)(handle.cast(), request, arg.cast()) }
}

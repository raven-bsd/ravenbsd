// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `hcreate_r`.

use core::ffi::c_int;

use raven_libc_abi::errno::Errno;

use super::search::HsearchData;

/// `hcreate_r`.
///
/// # Safety
/// `data` is NULL or a writable `struct hsearch_data`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hcreate_r(_nel: usize, data: *mut HsearchData) -> c_int {
    if data.is_null() {
        crate::ffi::errno::set(Errno(12));
        return 0;
    }
    match super::search::new_table() {
        Ok(table) => {
            super::search::install(data, table);
            1
        }
        Err(_) => {
            crate::ffi::errno::set(Errno(12));
            0
        }
    }
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `hcreate`.

use core::ffi::c_int;

use raven_libc_abi::errno::Errno;

/// `hcreate`.
#[unsafe(no_mangle)]
pub extern "C" fn hcreate(_nel: usize) -> c_int {
    match super::search::new_table() {
        Ok(table) => {
            // SAFETY: the global slot is replaced for one caller at a time.
            unsafe { *super::search::GLOBAL.0.get() = Some(table) };
            1
        }
        Err(_) => {
            crate::ffi::errno::set(Errno(12));
            0
        }
    }
}

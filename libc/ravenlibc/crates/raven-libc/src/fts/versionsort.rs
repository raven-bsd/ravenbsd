// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `versionsort(3)`.
use core::ffi::c_int;

use raven_libc_abi::structs::Dirent;
use raven_libc_fts::versionsort as compare;

use super::dirent_entry;

/// Compare two C `dirent` pointers as version strings.
///
/// # Safety
/// Both pointers point to readable `struct dirent` pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn versionsort(a: *const *const Dirent, b: *const *const Dirent) -> c_int {
    if a.is_null() || b.is_null() {
        return 0;
    }
    // SAFETY: the caller's pointer-to-pointer contract.
    let (Some(left), Some(right)) = (unsafe { dirent_entry(*a) }, unsafe { dirent_entry(*b) })
    else {
        return 0;
    };
    compare(&left, &right)
}

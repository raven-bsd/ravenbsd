// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getgroups(2)`.
use core::ffi::c_int;

/// Fill the caller's group list; a zero size returns the count.
///
/// # Safety
/// `list` is NULL or points to `size` writable `gid_t` slots.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getgroups(size: c_int, list: *mut u32) -> c_int {
    let result = crate::ffi::map_proc::getgroups(&crate::ffi::procbackend::backend(), size, list);
    crate::ffi::ret::int(result.map_err(crate::ffi::map_proc::error))
}

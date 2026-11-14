// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `setgroups(2)`.
use core::ffi::c_int;

/// Replace the supplementary group list through the backend.
///
/// # Safety
/// `list` is NULL or points to `size` readable `gid_t` slots.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn setgroups(size: c_int, list: *const u32) -> c_int {
    let result = crate::ffi::map_proc::setgroups(&crate::ffi::procbackend::backend(), size, list);
    crate::ffi::ret::int(result.map(|()| 0).map_err(crate::ffi::map_proc::error))
}

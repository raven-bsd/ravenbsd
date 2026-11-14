// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `posix_spawnattr_setflags(3)`.
use core::ffi::c_int;

use raven_libc_abi::errno::EINVAL;
use raven_libc_abi::structs::PosixSpawnattrT;

/// Set the `POSIX_SPAWN_*` flags, rejecting unknown bits.
///
/// # Safety
/// `attr` is NULL or points to a writable attribute blob.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn posix_spawnattr_setflags(attr: *mut PosixSpawnattrT, flags: i16) -> c_int {
    if attr.is_null() {
        return EINVAL;
    }
    // SAFETY: the caller's writable blob.
    let view = unsafe { crate::ffi::map_proc::attr_view(attr) };
    let mut model = crate::ffi::map_proc::attr_model(view);
    if let Err(error) = model.set_flags(flags) {
        return error;
    }
    crate::ffi::map_proc::attr_store(view, &model);
    0
}

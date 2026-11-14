// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `posix_spawn_file_actions_addclosefrom_np(3)`.
use core::ffi::c_int;

use raven_libc_abi::errno::EINVAL;
use raven_libc_abi::structs::PosixSpawnFileActionsT;

/// Append a `CLOSEFROM` action.
///
/// # Safety
/// `actions` points to a writable file-action blob.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn posix_spawn_file_actions_addclosefrom_np(
    actions: *mut PosixSpawnFileActionsT,
    from: c_int,
) -> c_int {
    if actions.is_null() {
        return EINVAL;
    }
    // SAFETY: the caller's writable blob.
    let view = unsafe { crate::ffi::map_proc::actions_view(actions) };
    match view.load().and_then(|list| list.add_closefrom(from)) {
        Ok(()) => 0,
        Err(error) => error,
    }
}

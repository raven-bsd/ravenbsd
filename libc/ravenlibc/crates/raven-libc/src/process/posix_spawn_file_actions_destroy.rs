// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `posix_spawn_file_actions_destroy(3)`.
use core::ffi::c_int;

use raven_libc_abi::errno::EINVAL;
use raven_libc_abi::structs::PosixSpawnFileActionsT;

/// Release the action list the blob owns.
///
/// # Safety
/// `actions` points to an initialised `posix_spawn_file_actions_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn posix_spawn_file_actions_destroy(
    actions: *mut PosixSpawnFileActionsT,
) -> c_int {
    if actions.is_null() {
        return EINVAL;
    }
    // SAFETY: the caller's writable blob and its owned list.
    unsafe { crate::ffi::map_proc::actions_view(actions).destroy() };
    0
}

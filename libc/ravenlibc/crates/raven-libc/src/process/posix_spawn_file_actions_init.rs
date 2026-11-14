// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `posix_spawn_file_actions_init(3)`.
use core::ffi::c_int;

use raven_libc_abi::errno::EINVAL;
use raven_libc_abi::structs::PosixSpawnFileActionsT;

/// Initialise the file-action blob (no memory is owned until the first action).
///
/// # Safety
/// `actions` points to a writable `posix_spawn_file_actions_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn posix_spawn_file_actions_init(
    actions: *mut PosixSpawnFileActionsT,
) -> c_int {
    if actions.is_null() {
        return EINVAL;
    }
    // SAFETY: the caller's writable 80-byte blob.
    unsafe { crate::ffi::map_proc::actions_view(actions).init() };
    0
}

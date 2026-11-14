// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `posix_spawn_file_actions_addopen(3)`.
use core::ffi::{c_char, c_int};

use raven_libc_abi::errno::EINVAL;
use raven_libc_abi::structs::PosixSpawnFileActionsT;

/// Append an `OPEN` action.
///
/// # Safety
/// The blob and the path follow their C contracts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn posix_spawn_file_actions_addopen(
    actions: *mut PosixSpawnFileActionsT,
    fd: c_int,
    path: *const c_char,
    oflag: c_int,
    mode: u32,
) -> c_int {
    if actions.is_null() || path.is_null() {
        return EINVAL;
    }
    // SAFETY: the caller's writable blob and NUL-terminated path.
    let (view, path) = unsafe {
        (
            crate::ffi::map_proc::actions_view(actions),
            crate::ffi::cstr::slice(path),
        )
    };
    match view
        .load()
        .and_then(|list| list.add_open(fd, path, oflag, mode))
    {
        Ok(()) => 0,
        Err(error) => error,
    }
}

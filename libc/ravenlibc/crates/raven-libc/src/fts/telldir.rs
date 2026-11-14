// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `telldir(3)`.
use core::ffi::c_long;

use super::ShellDir;

/// The current position cookie.
///
/// # Safety
/// `dir` is a live `DIR`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn telldir(dir: *mut ShellDir) -> c_long {
    if dir.is_null() {
        return -1;
    }
    // SAFETY: the caller's DIR.
    let state = unsafe { &*dir };
    state.stream.telldir() as c_long
}

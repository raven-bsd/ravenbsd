// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `seekdir(3)`.
use core::ffi::c_long;

use super::ShellDir;
use crate::platform::ShellDirSys;

/// Position a directory stream at the cookie `off`.
///
/// # Safety
/// `dir` is a live `DIR`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seekdir(dir: *mut ShellDir, off: c_long) {
    if dir.is_null() {
        return;
    }
    // SAFETY: the caller's DIR.
    let state = unsafe { &mut *dir };
    let mut sys = ShellDirSys;
    state.stream.seekdir(&mut sys, off as i64);
}

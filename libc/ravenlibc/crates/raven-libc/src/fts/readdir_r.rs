// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `readdir_r(3)`.
use core::ffi::c_int;

use raven_libc_abi::structs::Dirent;

use super::{ShellDir, fill_dirent};
use crate::platform::ShellDirSys;

/// The deprecated reentrant form: copy the next entry into `entry`.
///
/// # Safety
/// `dir` is live; `entry` and `result` are writable, or `result` is NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn readdir_r(
    dir: *mut ShellDir,
    entry: *mut Dirent,
    result: *mut *mut Dirent,
) -> c_int {
    if dir.is_null() || entry.is_null() {
        return i32::from(raven_libc_abi::errno::EBADF as u16);
    }
    // SAFETY: the caller's DIR and entry.
    let state = unsafe { &mut *dir };
    let mut sys = ShellDirSys;
    match state.stream.readdir(&mut sys) {
        Ok(Some(value)) => {
            // SAFETY: the caller's writable entry.
            unsafe {
                fill_dirent(&value, &mut *entry);
            }
            if !result.is_null() {
                // SAFETY: the caller's writable result slot.
                unsafe { *result = entry };
            }
            0
        }
        Ok(None) => {
            if !result.is_null() {
                // SAFETY: the caller's writable result slot.
                unsafe { *result = core::ptr::null_mut() };
            }
            0
        }
        Err(error) => i32::from(error.0),
    }
}

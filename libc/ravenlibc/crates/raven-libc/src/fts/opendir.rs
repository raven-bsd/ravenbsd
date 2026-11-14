// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `opendir(3)`.
use alloc::boxed::Box;
use core::ffi::c_char;

use raven_libc_fts::DirStream;

use super::ShellDir;
use crate::platform::ShellDirSys;

/// Open a directory stream.
///
/// # Safety
/// `path` is NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn opendir(path: *const c_char) -> *mut ShellDir {
    // SAFETY: the caller's path contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    let mut sys = ShellDirSys;
    match DirStream::opendir(&mut sys, path) {
        Ok(stream) => Box::into_raw(ShellDir::new(stream)),
        Err(error) => crate::ffi::ret::null(error),
    }
}

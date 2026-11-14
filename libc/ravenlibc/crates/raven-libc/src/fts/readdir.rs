// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `readdir(3)`.
use raven_libc_abi::structs::Dirent;

use super::{ShellDir, fill_dirent};
use crate::platform::ShellDirSys;

/// Return the next directory entry, or NULL at the end.
///
/// # Safety
/// `dir` is a live `DIR` returned by `opendir`/`fdopendir`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn readdir(dir: *mut ShellDir) -> *mut Dirent {
    if dir.is_null() {
        return crate::ffi::ret::null(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EBADF as u16,
        ));
    }
    // SAFETY: the caller's DIR.
    let state = unsafe { &mut *dir };
    let mut sys = ShellDirSys;
    match state.stream.readdir(&mut sys) {
        Ok(Some(entry)) => {
            fill_dirent(&entry, &mut state.entry);
            (&mut state.entry) as *mut Dirent
        }
        Ok(None) => core::ptr::null_mut(),
        Err(error) => {
            crate::ffi::errno::set(error);
            core::ptr::null_mut()
        }
    }
}

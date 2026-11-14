// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `closedir(3)`.
use alloc::boxed::Box;

use super::ShellDir;
use crate::platform::ShellDirSys;

/// Close a directory stream.
///
/// # Safety
/// `dir` is a live `DIR`; it is not used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn closedir(dir: *mut ShellDir) -> i32 {
    if dir.is_null() {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EBADF as u16,
        )));
    }
    // SAFETY: the caller's DIR came from `Box::into_raw`.
    let state = unsafe { Box::from_raw(dir) };
    let mut sys = ShellDirSys;
    match state.stream.closedir(&mut sys) {
        Ok(()) => 0,
        Err(error) => crate::ffi::ret::int(Err(error)),
    }
}

// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `dirfd(3)`.
use core::ffi::c_int;

use super::ShellDir;

/// The descriptor owned by a directory stream.
///
/// # Safety
/// `dir` is a live `DIR`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dirfd(dir: *mut ShellDir) -> c_int {
    if dir.is_null() {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EBADF as u16,
        )));
    }
    // SAFETY: the caller's DIR.
    let state = unsafe { &*dir };
    state.stream.dirfd().0
}

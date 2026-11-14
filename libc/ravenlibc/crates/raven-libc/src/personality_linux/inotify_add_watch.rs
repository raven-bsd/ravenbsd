// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `inotify_add_watch(2)`.
use core::ffi::{c_char, c_int};

use raven_libc_personality_linux::seams::Errno;

use crate::personality_common::{RealFdTable, RealWatch, ret_int};

/// Add a watch; the mask is validated before the PG-38 `ENOSYS`.
///
/// # Safety
/// `path` is a NUL-terminated path.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inotify_add_watch(fd: c_int, path: *const c_char, mask: u32) -> c_int {
    if path.is_null() {
        return ret_int(Err(Errno::EFAULT));
    }
    // SAFETY: the caller's path contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    ret_int(raven_libc_personality_linux::inotify::add_watch(
        &mut RealFdTable,
        &mut RealWatch,
        fd,
        path,
        mask,
    ))
}

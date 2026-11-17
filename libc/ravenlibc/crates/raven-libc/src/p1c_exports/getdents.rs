// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `getdents`.

use super::*;

/// Read packed directory entries.
///
/// # Safety
/// `dirp` has `count` writable bytes.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn getdents(fd: c_int, dirp: *mut c_void, count: usize) -> c_int {
    // SAFETY: the caller's buffer contract; NULL with zero is the empty slice.
    let buffer = unsafe { crate::ffi::cstr::slice_mut(dirp.cast(), count) };
    int(raven_libc_platform::compat::getdents(global(), fd, buffer)
        .map(|written| c_int::try_from(written).unwrap_or(c_int::MAX)))
}

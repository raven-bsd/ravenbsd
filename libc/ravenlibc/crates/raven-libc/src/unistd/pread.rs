// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pread(2)`.
use core::ffi::{c_int, c_void};

/// Read at `offset`; `ENOSYS` until PG-22.
///
/// # Safety
/// `buffer` has `count` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pread(fd: c_int, buffer: *mut c_void, count: usize, offset: i64) -> isize {
    // SAFETY: the caller's buffer contract; a NULL with zero is the empty slice.
    let slice = unsafe { crate::ffi::cstr::slice_mut(buffer.cast(), count) };
    crate::ffi::ret::platform_ssize(crate::ffi::platform::global().pread(fd, slice, offset))
}

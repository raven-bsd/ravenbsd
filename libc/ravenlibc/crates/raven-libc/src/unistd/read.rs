// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `read(2)`.
use core::ffi::{c_int, c_void};

/// Read up to `count` bytes into `buffer`.
///
/// # Safety
/// `buffer` has `count` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn read(fd: c_int, buffer: *mut c_void, count: usize) -> isize {
    // SAFETY: the caller's buffer contract; a NULL with zero is the empty slice.
    let slice = unsafe { crate::ffi::cstr::slice_mut(buffer.cast(), count) };
    crate::ffi::ret::platform_ssize(crate::ffi::platform::global().read(fd, slice))
}

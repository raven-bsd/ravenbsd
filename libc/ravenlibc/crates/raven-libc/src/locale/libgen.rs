// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Shared fallbacks for the `<libgen.h>` exports.

use core::ffi::c_char;

/// The `.` fallback.
pub(crate) static DOT: [u8; 2] = *b".\0";
/// The `/` fallback.
pub(crate) static ROOT: [u8; 2] = *b"/\0";

/// The writable bytes of a NUL-terminated string.
///
/// # Safety
/// `s` is a NUL-terminated writable buffer.
pub(crate) unsafe fn writable<'a>(s: *mut c_char) -> &'a mut [u8] {
    // SAFETY: the caller's C string contract.
    let length = unsafe { crate::ffi::cstr::slice(s) }.len();
    // SAFETY: the same buffer is writable for `length` bytes.
    unsafe { core::slice::from_raw_parts_mut(s.cast::<u8>(), length) }
}

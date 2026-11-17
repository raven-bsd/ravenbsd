// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `getentropy`.

use super::*;

/// Fill a buffer with entropy.
///
/// # Safety
/// `buffer` has `len` writable bytes and `len <= 256`.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn getentropy(buffer: *mut c_void, len: usize) -> c_int {
    // SAFETY: the caller's buffer contract.
    let buffer = unsafe { crate::ffi::cstr::slice_mut(buffer.cast(), len) };
    unit(global().getentropy(buffer))
}

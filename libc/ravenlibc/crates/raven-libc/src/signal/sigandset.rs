// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sigandset(3)`.
use core::ffi::c_int;

use raven_libc_signal::SigSet;

/// Store the and of `left` and `right` in `dst`.
///
/// # Safety
/// All three pointers point to valid `sigset_t` values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigandset(
    dst: *mut SigSet,
    left: *const SigSet,
    right: *const SigSet,
) -> c_int {
    if dst.is_null() || left.is_null() || right.is_null() {
        return -1;
    }
    // SAFETY: the caller supplies valid sigset_t pointers.
    unsafe { *dst = (*left).and(&*right) };
    0
}

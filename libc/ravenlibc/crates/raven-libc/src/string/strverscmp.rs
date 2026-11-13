// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `strverscmp(3)`.
use core::ffi::{c_char, c_int};

/// Version-aware comparison.
///
/// # Safety
/// Both pointers are readable NUL-terminated strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strverscmp(a: *const c_char, b: *const c_char) -> c_int {
    // SAFETY: the caller's contract is the strverscmp(3) contract.
    unsafe { raven_libc_str::raw::strverscmp(a.cast(), b.cast()) }
}

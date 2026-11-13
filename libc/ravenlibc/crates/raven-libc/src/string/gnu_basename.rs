// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `__gnu_basename(3)` (the `<string.h>` GNU form).
use core::ffi::c_char;

/// The GNU basename: everything after the last `/`, without modifying the argument.
///
/// # Safety
/// `path` is a readable NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __gnu_basename(path: *const c_char) -> *mut c_char {
    // SAFETY: the caller's contract is the GNU basename contract.
    unsafe { raven_libc_str::raw::gnu_basename(path.cast()).cast() }
}

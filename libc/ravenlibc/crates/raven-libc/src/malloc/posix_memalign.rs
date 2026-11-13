// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `posix_memalign(3)`.
use core::ffi::c_void;

/// Store a `size`-byte block aligned to `alignment` at `*memptr`.
///
/// # Safety
/// `memptr` is a writable pointer slot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn posix_memalign(
    memptr: *mut *mut c_void,
    alignment: usize,
    size: usize,
) -> i32 {
    let mut result = 0usize;
    let status = crate::malloc::heap::api().posix_memalign(&mut result, alignment, size);
    if status == 0 && !memptr.is_null() {
        // SAFETY: the caller grants a writable pointer slot.
        unsafe { *memptr = result as *mut c_void };
    }
    status
}

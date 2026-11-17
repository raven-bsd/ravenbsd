// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! C23 `free_sized` and `free_aligned_sized`.
//!
//! The current allocator frees by pointer and does not use the size/alignment hints; the
//! declarations exist so C23 callers link, and the size mismatch remains the caller's undefined
//! behaviour as the standard requires.

use core::ffi::c_void;

/// C23 `free_sized`: free `pointer`; the size is a hint for sized allocators.
///
/// # Safety
/// `pointer` is null or a live allocation, and `size` is its requested size.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn free_sized(pointer: *mut c_void, _size: usize) {
    // SAFETY: the caller's `free` contract.
    unsafe { crate::malloc::free::free(pointer) }
}

/// C23 `free_aligned_sized`: free an aligned allocation; the hints are for sized allocators.
///
/// # Safety
/// `pointer` is null or a live aligned allocation, `alignment` is its alignment, and `size` is
/// its requested size.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn free_aligned_sized(pointer: *mut c_void, _alignment: usize, _size: usize) {
    // SAFETY: the caller's `free` contract.
    unsafe { crate::malloc::free::free(pointer) }
}

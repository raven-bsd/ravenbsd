// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `reallocarray(3)`.
use core::ffi::c_void;

/// Resize a block to `count * size` bytes with an overflow check.
///
/// # Safety
/// `pointer` is NULL or a live block from this allocator.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn reallocarray(
    pointer: *mut c_void,
    count: usize,
    size: usize,
) -> *mut c_void {
    crate::malloc::heap::api().reallocarray(pointer as usize, count, size) as *mut c_void
}
